//! Parallel execution engine.
//!
//! Builds on the sequential [`Executor`] and the [`scheduler`] to execute
//! non-conflicting transactions truly concurrently using rayon and per-
//! transaction [`OverlayStore`] instances.
//!
//! ## Design
//!
//! 1. The scheduler partitions transactions into non-conflicting batches.
//! 2. For each batch, every transaction gets its own [`OverlayStore`] — a
//!    lightweight write-through cache that reads from the shared base state and
//!    captures writes locally.
//! 3. Transactions within a batch are executed in parallel via rayon's thread
//!    pool.
//! 4. After the batch completes, all overlay writes are flushed to the base
//!    store.  This is safe because the scheduler guarantees that transactions
//!    within a batch have disjoint write sets.
//! 5. The next batch then runs against the updated base state.

use std::collections::BTreeMap;

use rayon::prelude::*;
use tracing::info;

use polay_state::{OverlayStore, StateStore};
use polay_types::{Address, SignedTransaction, TransactionReceipt};

use crate::executor::Executor;
use crate::scheduler::{schedule_parallel, schedule_stats, ExecutionBatch, ScheduleStats};

/// Parallel execution engine that wraps the sequential [`Executor`].
///
/// Strategy:
/// 1. Schedule transactions into non-conflicting batches.
/// 2. For each batch, execute transactions in parallel using rayon and
///    per-transaction [`OverlayStore`] instances.
/// 3. Flush overlays to the base store after each batch.
/// 4. Reassemble receipts in the original transaction order.
pub struct ParallelExecutor {
    executor: Executor,
}

impl ParallelExecutor {
    /// Create a new parallel executor wrapping the given sequential executor.
    pub fn new(executor: Executor) -> Self {
        Self { executor }
    }

    /// Execute a block's transactions with true parallel execution.
    ///
    /// Returns receipts in the **original** transaction order (matching block
    /// order), together with scheduling statistics.
    pub fn execute_block_parallel(
        &self,
        transactions: &[SignedTransaction],
        store: &dyn StateStore,
        height: u64,
        block_proposer: &Address,
    ) -> (Vec<TransactionReceipt>, ScheduleStats) {
        let batches = schedule_parallel(transactions);
        let stats = schedule_stats(&batches);

        info!(
            height,
            total_txs = stats.total_transactions,
            batches = stats.batch_count,
            max_batch = stats.max_batch_size,
            parallelism = format!("{:.2}", stats.parallelism_ratio),
            "parallel scheduler results"
        );

        let mut all_results: Vec<(usize, TransactionReceipt)> = Vec::new();

        for batch in &batches {
            let batch_results = self.execute_batch_parallel(batch, store, height, block_proposer);
            all_results.extend(batch_results);
        }

        // Sort back to original transaction order.
        all_results.sort_by_key(|(idx, _)| *idx);
        let receipts = all_results.into_iter().map(|(_, r)| r).collect();

        (receipts, stats)
    }

    /// Fallback: sequential execution (identical to [`Executor::execute_block`]).
    pub fn execute_block_sequential(
        &self,
        transactions: &[SignedTransaction],
        store: &dyn StateStore,
        height: u64,
        block_proposer: &Address,
    ) -> Vec<TransactionReceipt> {
        self.executor.execute_block(transactions, store, height, block_proposer)
    }

    /// Return a reference to the inner sequential executor.
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Execute all transactions in a batch in parallel using rayon.
    ///
    /// Each transaction gets its own [`OverlayStore`] so writes are isolated.
    /// After all transactions complete, overlay writes are flushed to the base
    /// store.  The scheduler guarantees no write-set conflicts within a batch,
    /// so the flush order is irrelevant.
    fn execute_batch_parallel(
        &self,
        batch: &ExecutionBatch,
        store: &dyn StateStore,
        height: u64,
        block_proposer: &Address,
    ) -> Vec<(usize, TransactionReceipt)> {
        // Execute all transactions in parallel, each against its own overlay.
        let batch_results: Vec<(usize, TransactionReceipt, BTreeMap<Vec<u8>, Option<Vec<u8>>>)> =
            batch
                .transactions
                .par_iter()
                .map(|(idx, tx)| {
                    let overlay = OverlayStore::new(store);

                    let receipt = match self.executor.execute_transaction(tx, &overlay, height, block_proposer) {
                        Ok(exec_result) => exec_result.receipt,
                        Err(e) => {
                            let par_fee_payer = tx.transaction.sponsor.unwrap_or(*tx.signer());
                            TransactionReceipt::failure(
                                tx.tx_hash,
                                height,
                                0,
                                0,
                                par_fee_payer,
                                e.to_string(),
                            )
                        }
                    };

                    let writes = overlay.drain_writes();
                    (*idx, receipt, writes)
                })
                .collect();

        // Flush all overlays to the base store sequentially.
        // Order within a batch is irrelevant since write sets are disjoint.
        let mut results = Vec::with_capacity(batch_results.len());
        for (idx, receipt, writes) in batch_results {
            for (key, value) in &writes {
                match value {
                    Some(data) => {
                        let _ = store.put_raw(key, data);
                    }
                    None => {
                        let _ = store.delete(key);
                    }
                }
            }
            results.push((idx, receipt));
        }

        results
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_config::ChainConfig;
    use polay_crypto::sha256;
    use polay_state::{MemoryStore, StateView, StateWriter};
    use polay_types::{AccountState, Address, Signature, Transaction, TransactionAction};

    const CHAIN_ID: &str = "polay-devnet-1";

    fn addr(b: u8) -> Address {
        Address::new([b; 32])
    }

    fn make_config() -> ChainConfig {
        ChainConfig::default()
    }

    fn seed_account(store: &dyn StateStore, address: Address, balance: u64) {
        StateWriter::new(store)
            .set_account(&AccountState::with_balance(address, balance, 0))
            .unwrap();
    }

    fn make_signed_tx(tx: Transaction) -> SignedTransaction {
        let sig = Signature::new([0xAB; 64]);
        let signing_bytes = tx.signing_bytes();
        let mut payload = Vec::with_capacity(signing_bytes.len() + 64);
        payload.extend_from_slice(&signing_bytes);
        payload.extend_from_slice(sig.as_bytes());
        let tx_hash = sha256(&payload);
        SignedTransaction::new(tx, sig, tx_hash, vec![0u8; 32])
    }

    /// Build a transfer transaction from `sender` to `receiver`.
    fn transfer_tx(sender: Address, receiver: Address, amount: u64, nonce: u64) -> SignedTransaction {
        make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce,
            signer: sender,
            action: TransactionAction::Transfer {
                to: receiver,
                amount,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        })
    }

    // -- Core invariant: parallel produces same receipts as sequential --------

    #[test]
    fn parallel_and_sequential_produce_identical_receipts() {
        // Set up two independent transfers: addr(1)->addr(2) and addr(3)->addr(4).
        let config = make_config();
        let store_par = MemoryStore::new();
        let store_seq = MemoryStore::new();

        for store in [&store_par as &dyn StateStore, &store_seq as &dyn StateStore] {
            seed_account(store, addr(1), 10_000_000);
            seed_account(store, addr(3), 10_000_000);
        }

        let txs = vec![
            transfer_tx(addr(1), addr(2), 100, 0),
            transfer_tx(addr(3), addr(4), 200, 0),
        ];

        let executor_seq = Executor::new(config.clone());
        let seq_receipts = executor_seq.execute_block(&txs, &store_seq, 1, &Address::ZERO);

        let par_executor = ParallelExecutor::new(Executor::new(config));
        let (par_receipts, stats) = par_executor.execute_block_parallel(&txs, &store_par, 1, &Address::ZERO);

        // Receipts must be identical and in the same order.
        assert_eq!(seq_receipts.len(), par_receipts.len());
        for (s, p) in seq_receipts.iter().zip(par_receipts.iter()) {
            assert_eq!(s.tx_hash, p.tx_hash, "tx_hash mismatch");
            assert_eq!(s.success, p.success, "success mismatch");
            assert_eq!(s.fee_used, p.fee_used, "fee_used mismatch");
            assert_eq!(s.gas_used, p.gas_used, "gas_used mismatch");
            assert_eq!(s.block_height, p.block_height, "block_height mismatch");
            assert_eq!(s.error, p.error, "error mismatch");
        }

        // These two are independent so should fit in a single batch.
        assert_eq!(stats.batch_count, 1);
    }

    #[test]
    fn parallel_preserves_order_for_conflicting_txs() {
        // Same signer sends two transfers — must be serialised into two
        // batches, but receipts come back in original order.
        let config = make_config();
        let store = MemoryStore::new();
        seed_account(&store, addr(1), 10_000_000);

        let txs = vec![
            transfer_tx(addr(1), addr(2), 100, 0),
            transfer_tx(addr(1), addr(3), 200, 1),
        ];

        let par = ParallelExecutor::new(Executor::new(config));
        let (receipts, stats) = par.execute_block_parallel(&txs, &store, 5, &Address::ZERO);

        assert_eq!(receipts.len(), 2);
        assert!(receipts[0].success);
        assert!(receipts[1].success);
        assert_eq!(receipts[0].block_height, 5);
        assert_eq!(receipts[1].block_height, 5);
        assert_eq!(stats.batch_count, 2, "same-signer txs need separate batches");
    }

    #[test]
    fn empty_block_produces_empty_receipts() {
        let config = make_config();
        let store = MemoryStore::new();
        let par = ParallelExecutor::new(Executor::new(config));

        let (receipts, stats) = par.execute_block_parallel(&[], &store, 1, &Address::ZERO);
        assert!(receipts.is_empty());
        assert_eq!(stats.total_transactions, 0);
        assert_eq!(stats.batch_count, 0);
    }

    #[test]
    fn sequential_fallback_works() {
        let config = make_config();
        let store = MemoryStore::new();
        seed_account(&store, addr(1), 10_000_000);

        let txs = vec![transfer_tx(addr(1), addr(2), 100, 0)];
        let par = ParallelExecutor::new(Executor::new(config));
        let receipts = par.execute_block_sequential(&txs, &store, 1, &Address::ZERO);

        assert_eq!(receipts.len(), 1);
        assert!(receipts[0].success);
    }

    #[test]
    fn failed_transaction_still_has_receipt() {
        let config = make_config();
        let store = MemoryStore::new();
        // No account seeded — execution will fail.

        let txs = vec![transfer_tx(addr(99), addr(2), 100, 0)];
        let par = ParallelExecutor::new(Executor::new(config));
        let (receipts, _) = par.execute_block_parallel(&txs, &store, 1, &Address::ZERO);

        assert_eq!(receipts.len(), 1);
        assert!(!receipts[0].success);
    }

    #[test]
    fn parallel_identical_to_sequential_mixed_success_failure() {
        let config = make_config();
        let store_par = MemoryStore::new();
        let store_seq = MemoryStore::new();

        for store in [&store_par as &dyn StateStore, &store_seq as &dyn StateStore] {
            seed_account(store, addr(1), 10_000_000);
            // addr(99) is NOT seeded — will fail.
        }

        let txs = vec![
            transfer_tx(addr(1), addr(2), 100, 0),
            transfer_tx(addr(99), addr(3), 200, 0), // will fail
        ];

        let seq_receipts = Executor::new(config.clone()).execute_block(&txs, &store_seq, 1, &Address::ZERO);
        let (par_receipts, _) =
            ParallelExecutor::new(Executor::new(config)).execute_block_parallel(&txs, &store_par, 1, &Address::ZERO);

        assert_eq!(seq_receipts.len(), par_receipts.len());
        for (s, p) in seq_receipts.iter().zip(par_receipts.iter()) {
            assert_eq!(s.tx_hash, p.tx_hash);
            assert_eq!(s.success, p.success);
            assert_eq!(s.fee_used, p.fee_used);
            assert_eq!(s.gas_used, p.gas_used);
            assert_eq!(s.error, p.error);
        }
    }

    // -- New tests: parallel correctness and state consistency ----------------

    #[test]
    fn parallel_matches_sequential_state() {
        // Execute the same transactions on two identical stores — one sequential,
        // one parallel — and verify that final account states match exactly.
        let config = make_config();
        let store_seq = MemoryStore::new();
        let store_par = MemoryStore::new();

        // 4 independent senders, each with their own receiver.
        let senders: Vec<u8> = vec![1, 3, 5, 7];
        let receivers: Vec<u8> = vec![2, 4, 6, 8];

        for store in [&store_seq as &dyn StateStore, &store_par as &dyn StateStore] {
            for &s in &senders {
                seed_account(store, addr(s), 10_000_000);
            }
        }

        let txs: Vec<SignedTransaction> = senders
            .iter()
            .zip(receivers.iter())
            .enumerate()
            .map(|(i, (&s, &r))| transfer_tx(addr(s), addr(r), 100 + i as u64 * 50, 0))
            .collect();

        let seq_receipts = Executor::new(config.clone()).execute_block(&txs, &store_seq, 1, &Address::ZERO);
        let (par_receipts, stats) =
            ParallelExecutor::new(Executor::new(config)).execute_block_parallel(&txs, &store_par, 1, &Address::ZERO);

        // All should be in a single batch (independent senders/receivers).
        assert_eq!(stats.batch_count, 1);

        // Receipts match.
        assert_eq!(seq_receipts.len(), par_receipts.len());
        for (s, p) in seq_receipts.iter().zip(par_receipts.iter()) {
            assert_eq!(s.success, p.success);
            assert_eq!(s.fee_used, p.fee_used);
            assert_eq!(s.gas_used, p.gas_used);
        }

        // Final state matches for all involved accounts.
        let seq_view = StateView::new(&store_seq);
        let par_view = StateView::new(&store_par);
        for &a in senders.iter().chain(receivers.iter()) {
            let seq_acct = seq_view.get_account(&addr(a)).unwrap();
            let par_acct = par_view.get_account(&addr(a)).unwrap();
            assert_eq!(
                seq_acct, par_acct,
                "account state mismatch for addr({})",
                a
            );
        }
    }

    #[test]
    fn parallel_with_many_independent_transactions() {
        // A larger batch to exercise rayon's thread pool.
        let config = make_config();
        let store = MemoryStore::new();
        let n = 50;

        // Create 50 independent senders, each transferring to a unique receiver.
        for i in 0..n {
            let sender = addr((i * 2 + 1) as u8);
            seed_account(&store, sender, 10_000_000);
        }

        let txs: Vec<SignedTransaction> = (0..n)
            .map(|i| {
                let sender = addr((i * 2 + 1) as u8);
                let receiver = addr((i * 2 + 2) as u8);
                transfer_tx(sender, receiver, 100, 0)
            })
            .collect();

        let par = ParallelExecutor::new(Executor::new(config));
        let (receipts, stats) = par.execute_block_parallel(&txs, &store, 1, &Address::ZERO);

        assert_eq!(receipts.len(), n);
        assert!(
            receipts.iter().all(|r| r.success),
            "all independent transactions should succeed"
        );
        assert_eq!(
            stats.batch_count, 1,
            "all independent transactions should fit in one batch"
        );
    }

    #[test]
    fn parallel_chain_of_dependent_transactions() {
        // Same signer sends N transactions — they must be serialised into N
        // batches, and the final state must be correct.
        let config = make_config();
        let store_seq = MemoryStore::new();
        let store_par = MemoryStore::new();

        for store in [&store_seq as &dyn StateStore, &store_par as &dyn StateStore] {
            seed_account(store, addr(1), 100_000_000);
        }

        let n = 5;
        let txs: Vec<SignedTransaction> = (0..n)
            .map(|i| transfer_tx(addr(1), addr(2), 100, i as u64))
            .collect();

        let seq_receipts = Executor::new(config.clone()).execute_block(&txs, &store_seq, 1, &Address::ZERO);
        let (par_receipts, stats) =
            ParallelExecutor::new(Executor::new(config)).execute_block_parallel(&txs, &store_par, 1, &Address::ZERO);

        assert_eq!(stats.batch_count, n, "each tx from same signer needs its own batch");

        // All receipts match.
        for (s, p) in seq_receipts.iter().zip(par_receipts.iter()) {
            assert_eq!(s.tx_hash, p.tx_hash);
            assert_eq!(s.success, p.success);
            assert_eq!(s.fee_used, p.fee_used);
        }

        // Final state matches.
        let seq_view = StateView::new(&store_seq);
        let par_view = StateView::new(&store_par);
        for a in [1u8, 2] {
            assert_eq!(
                seq_view.get_account(&addr(a)).unwrap(),
                par_view.get_account(&addr(a)).unwrap(),
                "account state mismatch for addr({})",
                a
            );
        }
    }
}
