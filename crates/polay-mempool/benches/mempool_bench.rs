use criterion::{black_box, criterion_group, criterion_main, Criterion};

use polay_mempool::{Mempool, MempoolConfig};
use polay_types::{Address, Hash, Signature, SignedTransaction, Transaction, TransactionAction};

/// Build a `SignedTransaction` with unique hash derived from the given seed.
fn make_tx(sender_byte: u8, nonce: u64, fee: u64, hash_seed: u16) -> SignedTransaction {
    let sender = Address::new([sender_byte; 32]);
    let tx = Transaction {
        chain_id: "polay-bench".into(),
        nonce,
        signer: sender,
        action: TransactionAction::Transfer {
            to: Address::new([0xBB; 32]),
            amount: 1_000,
        },
        max_fee: fee,
        timestamp: 1_700_000_000,
        session: None,
        sponsor: None,
    };

    let mut hash_bytes = [0u8; 32];
    hash_bytes[0] = (hash_seed & 0xFF) as u8;
    hash_bytes[1] = ((hash_seed >> 8) & 0xFF) as u8;
    hash_bytes[2] = sender_byte;
    hash_bytes[3] = (nonce & 0xFF) as u8;

    SignedTransaction::new(tx, Signature::ZERO, Hash::new(hash_bytes), vec![0u8; 32])
}

fn bench_pool() -> Mempool {
    Mempool::new(MempoolConfig {
        max_size: 100_000,
        max_per_account: 10_000,
        min_fee: 100,
        verify_signature: false,
        ..MempoolConfig::default()
    })
}

fn bench_mempool_insert(c: &mut Criterion) {
    c.bench_function("mempool_insert", |b| {
        let mut counter = 0u16;
        b.iter(|| {
            let pool = bench_pool();
            let tx = make_tx(0xAA, 0, 5_000, counter);
            counter = counter.wrapping_add(1);
            let _ = black_box(pool.insert(tx));
        });
    });
}

fn bench_mempool_insert_1000(c: &mut Criterion) {
    c.bench_function("mempool_insert_1000", |b| {
        b.iter(|| {
            let pool = bench_pool();
            for i in 0..1000u16 {
                // Use different senders to avoid per-account limits.
                let sender_byte = ((i / 100) as u8).wrapping_add(1);
                let nonce = (i % 100) as u64;
                let tx = make_tx(sender_byte, nonce, 5_000 + (i as u64), i);
                pool.insert(tx).unwrap();
            }
            black_box(&pool);
        });
    });
}

fn bench_mempool_get_pending_100(c: &mut Criterion) {
    let pool = bench_pool();
    // Pre-fill with 1000 transactions from different senders.
    for i in 0..1000u16 {
        let sender_byte = ((i / 100) as u8).wrapping_add(1);
        let nonce = (i % 100) as u64;
        let tx = make_tx(sender_byte, nonce, 5_000 + (i as u64), i);
        pool.insert(tx).unwrap();
    }
    assert_eq!(pool.size(), 1000);

    c.bench_function("mempool_get_pending_100", |b| {
        b.iter(|| {
            black_box(pool.get_pending_for_block(100));
        });
    });
}

fn bench_mempool_remove_batch_100(c: &mut Criterion) {
    c.bench_function("mempool_remove_batch_100", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for iter in 0..iters {
                let pool = bench_pool();
                let mut hashes = Vec::with_capacity(100);
                let offset = (iter * 1000) as u16;
                for i in 0..1000u16 {
                    let sender_byte = ((i / 100) as u8).wrapping_add(1);
                    let nonce = (i % 100) as u64;
                    let tx = make_tx(
                        sender_byte,
                        nonce,
                        5_000 + (i as u64),
                        i.wrapping_add(offset),
                    );
                    if i < 100 {
                        hashes.push(tx.tx_hash);
                    }
                    pool.insert(tx).unwrap();
                }

                let start = std::time::Instant::now();
                pool.remove_batch(black_box(&hashes));
                total += start.elapsed();
            }
            total
        });
    });
}

criterion_group!(
    benches,
    bench_mempool_insert,
    bench_mempool_insert_1000,
    bench_mempool_get_pending_100,
    bench_mempool_remove_batch_100,
);
criterion_main!(benches);
