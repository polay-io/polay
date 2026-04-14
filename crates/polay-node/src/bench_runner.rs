//! Built-in execution benchmark runner for the POLAY node.
//!
//! Runs an end-to-end benchmark: generates accounts, funds them, builds signed
//! transactions, executes them both sequentially and in parallel, and prints
//! comparative performance statistics.

use std::time::Instant;

use polay_config::ChainConfig;
use polay_crypto::{sha256, sign_transaction, PolayKeypair};
use polay_execution::scheduler::{schedule_parallel, schedule_stats};
use polay_execution::{Executor, ParallelExecutor};
use polay_state::{MemoryStore, StateStore, StateWriter};
use polay_types::{AccountState, Address, SignedTransaction, Transaction, TransactionAction};

const CHAIN_ID: &str = "polay-bench";

/// Run the execution benchmark with the given parameters.
pub fn run_bench(num_txs: usize, num_accounts: usize) {
    println!();
    println!("=== POLAY Execution Benchmark ===");
    println!("Accounts:         {}", num_accounts);
    println!("Transactions:     {}", num_txs);
    println!("---");

    // 1. Create an in-memory state store.
    let store = MemoryStore::new();

    // 2. Generate accounts and fund them.
    let keypairs = generate_funded_accounts(&store, num_accounts);

    // 3. Build signed transactions (mix of transfers).
    let transactions = build_transactions(&keypairs, num_txs);

    // 4. Get scheduling stats.
    let batches = schedule_parallel(&transactions);
    let sched_stats = schedule_stats(&batches);

    // 5. Sequential execution.
    // Reset accounts before sequential run.
    reset_accounts(&store, &keypairs);
    let config = bench_config();
    let executor = Executor::new(config.clone());

    let seq_start = Instant::now();
    let bench_proposer = Address::new([0xBB; 32]);
    let seq_receipts = executor.execute_block(&transactions, &store, 1, &bench_proposer);
    let seq_duration = seq_start.elapsed();

    let seq_success = seq_receipts.iter().filter(|r| r.success).count();
    let seq_failed = seq_receipts.iter().filter(|r| !r.success).count();
    let seq_ms = seq_duration.as_secs_f64() * 1000.0;
    let seq_tps = if seq_ms > 0.0 {
        (seq_success as f64 / seq_ms) * 1000.0
    } else {
        0.0
    };
    let seq_avg = if seq_success > 0 {
        seq_ms / seq_success as f64
    } else {
        0.0
    };

    println!("Sequential:");
    println!("  Total time:     {:.1}ms", seq_ms);
    println!("  TPS:            {:.0}", seq_tps);
    println!("  Avg per tx:     {:.3}ms", seq_avg);
    if seq_failed > 0 {
        println!("  Success/Failed: {}/{}", seq_success, seq_failed);
    }
    println!("---");

    // 6. Parallel execution.
    // Reset accounts before parallel run.
    reset_accounts(&store, &keypairs);
    let par = ParallelExecutor::new(Executor::new(config));

    let par_start = Instant::now();
    let (par_receipts, par_stats) =
        par.execute_block_parallel(&transactions, &store, 1, &bench_proposer);
    let par_duration = par_start.elapsed();

    let par_success = par_receipts.iter().filter(|r| r.success).count();
    let par_failed = par_receipts.iter().filter(|r| !r.success).count();
    let par_ms = par_duration.as_secs_f64() * 1000.0;
    let par_tps = if par_ms > 0.0 {
        (par_success as f64 / par_ms) * 1000.0
    } else {
        0.0
    };
    let par_avg = if par_success > 0 {
        par_ms / par_success as f64
    } else {
        0.0
    };

    println!("Parallel:");
    println!("  Total time:     {:.1}ms", par_ms);
    println!("  TPS:            {:.0}", par_tps);
    println!("  Avg per tx:     {:.3}ms", par_avg);
    println!("  Batches:        {}", par_stats.batch_count);
    println!("  Parallelism:    {:.1}", par_stats.parallelism_ratio);
    if par_failed > 0 {
        println!("  Success/Failed: {}/{}", par_success, par_failed);
    }
    println!("---");

    // 7. Speedup.
    let speedup = if par_ms > 0.0 { seq_ms / par_ms } else { 0.0 };
    println!("Speedup:          {:.2}x", speedup);
    println!();

    // 8. Scheduling stats.
    println!("Schedule stats:");
    println!("  Total txs:      {}", sched_stats.total_transactions);
    println!("  Batches:        {}", sched_stats.batch_count);
    println!("  Max batch size: {}", sched_stats.max_batch_size);
    println!("  Parallelism:    {:.1}", sched_stats.parallelism_ratio);
    println!();

    // 9. Execution mode.
    println!("Execution mode:   rayon parallel (overlay-per-tx)");
    println!("Rayon threads:    {}", rayon::current_num_threads());
    println!();
}

fn bench_config() -> ChainConfig {
    let mut config = ChainConfig::default();
    config.chain_id = CHAIN_ID.to_string();
    // Raise limits to avoid hitting them during benchmarks.
    config.max_block_gas = u64::MAX;
    config.max_block_transactions = usize::MAX;
    config
}

fn generate_funded_accounts(store: &dyn StateStore, n: usize) -> Vec<PolayKeypair> {
    let mut keypairs = Vec::with_capacity(n);
    for i in 0..n {
        let secret = sha256(&(i as u32).to_le_bytes()).to_bytes();
        let kp = PolayKeypair::from_bytes(&secret).unwrap();
        StateWriter::new(store)
            .set_account(&AccountState::with_balance(kp.address(), u64::MAX / 2, 0))
            .unwrap();
        keypairs.push(kp);
    }
    keypairs
}

fn reset_accounts(store: &dyn StateStore, keypairs: &[PolayKeypair]) {
    for kp in keypairs {
        StateWriter::new(store)
            .set_account(&AccountState::with_balance(kp.address(), u64::MAX / 2, 0))
            .unwrap();
    }
}

fn build_transactions(keypairs: &[PolayKeypair], num_txs: usize) -> Vec<SignedTransaction> {
    let mut txs = Vec::with_capacity(num_txs);
    let num_accounts = keypairs.len();

    for i in 0..num_txs {
        let sender_idx = i % num_accounts;
        let receiver_idx = (i + 1) % num_accounts;
        let nonce = (i / num_accounts) as u64;

        let kp = &keypairs[sender_idx];
        let receiver = keypairs[receiver_idx].address();

        // Mix of transaction types: 80% transfers, 10% create profile, 10% transfers with varying amounts.
        let action = match i % 10 {
            0..=7 => TransactionAction::Transfer {
                to: receiver,
                amount: 100 + (i as u64 % 1000),
            },
            8 => TransactionAction::Transfer {
                to: receiver,
                amount: 50_000 + (i as u64 % 50_000),
            },
            _ => TransactionAction::Transfer {
                to: receiver,
                amount: 1 + (i as u64 % 100),
            },
        };

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce,
            signer: kp.address(),
            action,
            max_fee: 1_000_000,
            timestamp: 1_700_000_000 + i as u64,
            session: None,
            sponsor: None,
        };

        let signed = sign_transaction(kp, tx).unwrap();
        txs.push(signed);
    }

    txs
}
