use criterion::{black_box, criterion_group, criterion_main, Criterion};

use polay_config::ChainConfig;
use polay_crypto::{sha256, sign_transaction, PolayKeypair};
use polay_execution::scheduler::schedule_parallel;
use polay_execution::{Executor, ParallelExecutor};
use polay_state::{MemoryStore, StateStore, StateWriter};
use polay_types::{AccountState, Address, Hash, SignedTransaction, Transaction, TransactionAction};

const CHAIN_ID: &str = "polay-bench";

fn make_config() -> ChainConfig {
    let mut config = ChainConfig::default();
    // Raise block gas limit to avoid hitting it during benchmarks.
    config.max_block_gas = u64::MAX;
    config
}

/// Create a funded keypair account in the store, returning the keypair.
fn create_funded_account(store: &dyn StateStore, balance: u64, idx: u32) -> PolayKeypair {
    let keypair = PolayKeypair::from_bytes(&sha256(&idx.to_le_bytes()).to_bytes()).unwrap();
    let addr = keypair.address();
    StateWriter::new(store)
        .set_account(&AccountState::with_balance(addr, balance, 0))
        .unwrap();
    keypair
}

/// Build a properly signed transfer transaction.
fn make_signed_transfer(
    keypair: &PolayKeypair,
    to: Address,
    amount: u64,
    nonce: u64,
) -> SignedTransaction {
    let tx = Transaction {
        chain_id: CHAIN_ID.into(),
        nonce,
        signer: keypair.address(),
        action: TransactionAction::Transfer { to, amount },
        max_fee: 500_000,
        timestamp: 1_700_000_000,
        session: None,
        sponsor: None,
    };
    sign_transaction(keypair, tx).unwrap()
}

/// Build a properly signed mint-asset transaction.
fn make_signed_mint(
    keypair: &PolayKeypair,
    asset_class_id: Hash,
    to: Address,
    amount: u64,
    nonce: u64,
) -> SignedTransaction {
    let tx = Transaction {
        chain_id: CHAIN_ID.into(),
        nonce,
        signer: keypair.address(),
        action: TransactionAction::MintAsset {
            asset_class_id,
            to,
            amount,
            metadata: None,
        },
        max_fee: 500_000,
        timestamp: 1_700_000_000,
        session: None,
        sponsor: None,
    };
    sign_transaction(keypair, tx).unwrap()
}

/// Build a properly signed buy-listing transaction.
fn make_signed_buy_listing(
    keypair: &PolayKeypair,
    listing_id: Hash,
    nonce: u64,
) -> SignedTransaction {
    let tx = Transaction {
        chain_id: CHAIN_ID.into(),
        nonce,
        signer: keypair.address(),
        action: TransactionAction::BuyListing { listing_id },
        max_fee: 1_000_000,
        timestamp: 1_700_000_000,
        session: None,
        sponsor: None,
    };
    sign_transaction(keypair, tx).unwrap()
}

fn bench_execute_transfer(c: &mut Criterion) {
    let config = make_config();
    let executor = Executor::new(config);
    let store = MemoryStore::new();
    let kp = create_funded_account(&store, u64::MAX / 2, 0);
    let receiver = Address::new([0xBB; 32]);
    let stx = make_signed_transfer(&kp, receiver, 100, 0);

    c.bench_function("execute_transfer", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for i in 0..iters {
                // Reset the account state each iteration to avoid running out of balance.
                StateWriter::new(&store)
                    .set_account(&AccountState::with_balance(kp.address(), u64::MAX / 2, i))
                    .unwrap();
                let start = std::time::Instant::now();
                black_box(executor.execute_transaction(&stx, &store, 1).unwrap());
                total += start.elapsed();
            }
            total
        });
    });
}

fn bench_execute_mint_asset(c: &mut Criterion) {
    let config = make_config();
    let executor = Executor::new(config);
    let store = MemoryStore::new();
    let kp = create_funded_account(&store, u64::MAX / 2, 0);
    let receiver = Address::new([0xCC; 32]);

    // Create an asset class first.
    let create_tx = {
        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: kp.address(),
            action: TransactionAction::CreateAssetClass {
                name: "BenchAsset".into(),
                symbol: "BNCH".into(),
                asset_type: polay_types::AssetType::Fungible,
                max_supply: None,
                metadata_uri: "https://example.com".into(),
            },
            max_fee: 1_000_000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };
        sign_transaction(&kp, tx).unwrap()
    };
    let result = executor.execute_transaction(&create_tx, &store, 1).unwrap();
    assert!(result.receipt.success, "asset class creation failed");

    // Extract the asset class ID from the event attributes.
    let asset_class_id = result
        .receipt
        .events
        .iter()
        .find_map(|e| {
            if e.module == "asset" && e.action == "create_class" {
                e.attributes.iter().find_map(|(k, v)| {
                    if k == "asset_class_id" {
                        let bytes = hex::decode(v).ok()?;
                        if bytes.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            Some(Hash::new(arr))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .expect("could not find asset class ID in events");

    let stx = make_signed_mint(&kp, asset_class_id, receiver, 100, 1);

    c.bench_function("execute_mint_asset", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for i in 0..iters {
                StateWriter::new(&store)
                    .set_account(&AccountState::with_balance(kp.address(), u64::MAX / 2, i + 1))
                    .unwrap();
                let start = std::time::Instant::now();
                black_box(executor.execute_transaction(&stx, &store, 1).unwrap());
                total += start.elapsed();
            }
            total
        });
    });
}

fn bench_execute_buy_listing(c: &mut Criterion) {
    // BuyListing requires a listing to exist. Since the listing gets consumed
    // on each buy, we measure the execution attempt (which will fail if the
    // listing doesn't exist, but still exercises the code path through fee
    // deduction and dispatch).
    let config = make_config();
    let executor = Executor::new(config);
    let store = MemoryStore::new();
    let kp = create_funded_account(&store, u64::MAX / 2, 0);
    let listing_id = sha256(b"bench-listing");
    let stx = make_signed_buy_listing(&kp, listing_id, 0);

    c.bench_function("execute_buy_listing", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for i in 0..iters {
                StateWriter::new(&store)
                    .set_account(&AccountState::with_balance(kp.address(), u64::MAX / 2, i))
                    .unwrap();
                let start = std::time::Instant::now();
                // The result may be a failed receipt (listing not found), but
                // that still exercises the full dispatch path.
                black_box(executor.execute_transaction(&stx, &store, 1).unwrap());
                total += start.elapsed();
            }
            total
        });
    });
}

/// Helper: set up N independent funded accounts and generate one transfer per account.
fn setup_independent_transfers(store: &MemoryStore, n: usize) -> Vec<SignedTransaction> {
    let mut txs = Vec::with_capacity(n);
    for i in 0..n {
        let kp = create_funded_account(store, u64::MAX / 2, i as u32);
        let receiver = Address::new([((i + 128) & 0xFF) as u8; 32]);
        txs.push(make_signed_transfer(&kp, receiver, 100, 0));
    }
    txs
}

fn bench_execute_block_10(c: &mut Criterion) {
    let config = make_config();
    let executor = Executor::new(config);
    let store = MemoryStore::new();
    let txs = setup_independent_transfers(&store, 10);

    c.bench_function("execute_block_10", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                // Reset account balances.
                for tx in &txs {
                    let addr = *tx.signer();
                    StateWriter::new(&store)
                        .set_account(&AccountState::with_balance(addr, u64::MAX / 2, 0))
                        .unwrap();
                }
                let start = std::time::Instant::now();
                black_box(executor.execute_block(&txs, &store, 1, &Address::ZERO));
                total += start.elapsed();
            }
            total
        });
    });
}

fn bench_execute_block_100(c: &mut Criterion) {
    let config = make_config();
    let executor = Executor::new(config);
    let store = MemoryStore::new();
    let txs = setup_independent_transfers(&store, 100);

    c.bench_function("execute_block_100", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                for tx in &txs {
                    let addr = *tx.signer();
                    StateWriter::new(&store)
                        .set_account(&AccountState::with_balance(addr, u64::MAX / 2, 0))
                        .unwrap();
                }
                let start = std::time::Instant::now();
                black_box(executor.execute_block(&txs, &store, 1, &Address::ZERO));
                total += start.elapsed();
            }
            total
        });
    });
}

fn bench_execute_block_1000(c: &mut Criterion) {
    let config = make_config();
    let executor = Executor::new(config);
    let store = MemoryStore::new();
    let txs = setup_independent_transfers(&store, 1000);

    c.bench_function("execute_block_1000", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                for tx in &txs {
                    let addr = *tx.signer();
                    StateWriter::new(&store)
                        .set_account(&AccountState::with_balance(addr, u64::MAX / 2, 0))
                        .unwrap();
                }
                let start = std::time::Instant::now();
                black_box(executor.execute_block(&txs, &store, 1, &Address::ZERO));
                total += start.elapsed();
            }
            total
        });
    });
}

fn bench_execute_block_parallel_100(c: &mut Criterion) {
    let config = make_config();
    let par = ParallelExecutor::new(Executor::new(config));
    let store = MemoryStore::new();
    let txs = setup_independent_transfers(&store, 100);

    c.bench_function("execute_block_parallel_100", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                for tx in &txs {
                    let addr = *tx.signer();
                    StateWriter::new(&store)
                        .set_account(&AccountState::with_balance(addr, u64::MAX / 2, 0))
                        .unwrap();
                }
                let start = std::time::Instant::now();
                black_box(par.execute_block_parallel(&txs, &store, 1));
                total += start.elapsed();
            }
            total
        });
    });
}

fn bench_execute_block_parallel_1000(c: &mut Criterion) {
    let config = make_config();
    let par = ParallelExecutor::new(Executor::new(config));
    let store = MemoryStore::new();
    let txs = setup_independent_transfers(&store, 1000);

    c.bench_function("execute_block_parallel_1000", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                for tx in &txs {
                    let addr = *tx.signer();
                    StateWriter::new(&store)
                        .set_account(&AccountState::with_balance(addr, u64::MAX / 2, 0))
                        .unwrap();
                }
                let start = std::time::Instant::now();
                black_box(par.execute_block_parallel(&txs, &store, 1));
                total += start.elapsed();
            }
            total
        });
    });
}

fn bench_schedule_100(c: &mut Criterion) {
    let store = MemoryStore::new();
    let txs = setup_independent_transfers(&store, 100);

    c.bench_function("schedule_100", |b| {
        b.iter(|| {
            black_box(schedule_parallel(black_box(&txs)));
        });
    });
}

fn bench_schedule_1000(c: &mut Criterion) {
    let store = MemoryStore::new();
    let txs = setup_independent_transfers(&store, 1000);

    c.bench_function("schedule_1000", |b| {
        b.iter(|| {
            black_box(schedule_parallel(black_box(&txs)));
        });
    });
}

criterion_group!(
    benches,
    bench_execute_transfer,
    bench_execute_mint_asset,
    bench_execute_buy_listing,
    bench_execute_block_10,
    bench_execute_block_100,
    bench_execute_block_1000,
    bench_execute_block_parallel_100,
    bench_execute_block_parallel_1000,
    bench_schedule_100,
    bench_schedule_1000,
);
criterion_main!(benches);
