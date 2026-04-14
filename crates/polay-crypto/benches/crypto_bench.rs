use criterion::{black_box, criterion_group, criterion_main, Criterion};

use polay_crypto::{
    merkle_root, sha256, sign_transaction, verify_transaction_with_key, PolayKeypair,
};
use polay_types::{Address, Hash, Transaction, TransactionAction};

fn make_transfer_tx(signer: Address) -> Transaction {
    Transaction {
        chain_id: "polay-bench".to_string(),
        nonce: 0,
        signer,
        action: TransactionAction::Transfer {
            to: Address::ZERO,
            amount: 1_000,
        },
        max_fee: 500_000,
        timestamp: 1_700_000_000,
        session: None,
        sponsor: None,
    }
}

fn bench_keygen(c: &mut Criterion) {
    c.bench_function("keygen", |b| {
        b.iter(|| {
            black_box(PolayKeypair::generate());
        });
    });
}

fn bench_sign_transaction(c: &mut Criterion) {
    let keypair = PolayKeypair::generate();
    let tx = make_transfer_tx(keypair.address());

    c.bench_function("sign_transaction", |b| {
        b.iter(|| {
            let tx_clone = tx.clone();
            black_box(sign_transaction(&keypair, tx_clone).unwrap());
        });
    });
}

fn bench_verify_transaction(c: &mut Criterion) {
    let keypair = PolayKeypair::generate();
    let tx = make_transfer_tx(keypair.address());
    let signed = sign_transaction(&keypair, tx).unwrap();
    let pubkey = keypair.public_key();

    c.bench_function("verify_transaction", |b| {
        b.iter(|| {
            black_box(verify_transaction_with_key(&signed, &pubkey).unwrap());
        });
    });
}

fn bench_sha256_small(c: &mut Criterion) {
    let data = [0xABu8; 32];
    c.bench_function("sha256_small", |b| {
        b.iter(|| {
            black_box(sha256(black_box(&data)));
        });
    });
}

fn bench_sha256_1kb(c: &mut Criterion) {
    let data = vec![0xCDu8; 1024];
    c.bench_function("sha256_1kb", |b| {
        b.iter(|| {
            black_box(sha256(black_box(&data)));
        });
    });
}

fn bench_sha256_64kb(c: &mut Criterion) {
    let data = vec![0xEFu8; 65536];
    c.bench_function("sha256_64kb", |b| {
        b.iter(|| {
            black_box(sha256(black_box(&data)));
        });
    });
}

fn bench_merkle_root_100(c: &mut Criterion) {
    let hashes: Vec<Hash> = (0..100u32).map(|i| sha256(&i.to_le_bytes())).collect();
    c.bench_function("merkle_root_100", |b| {
        b.iter(|| {
            black_box(merkle_root(black_box(&hashes)));
        });
    });
}

fn bench_merkle_root_1000(c: &mut Criterion) {
    let hashes: Vec<Hash> = (0..1000u32).map(|i| sha256(&i.to_le_bytes())).collect();
    c.bench_function("merkle_root_1000", |b| {
        b.iter(|| {
            black_box(merkle_root(black_box(&hashes)));
        });
    });
}

fn bench_merkle_root_10000(c: &mut Criterion) {
    let hashes: Vec<Hash> = (0..10000u32).map(|i| sha256(&i.to_le_bytes())).collect();
    c.bench_function("merkle_root_10000", |b| {
        b.iter(|| {
            black_box(merkle_root(black_box(&hashes)));
        });
    });
}

criterion_group!(
    benches,
    bench_keygen,
    bench_sign_transaction,
    bench_verify_transaction,
    bench_sha256_small,
    bench_sha256_1kb,
    bench_sha256_64kb,
    bench_merkle_root_100,
    bench_merkle_root_1000,
    bench_merkle_root_10000,
);
criterion_main!(benches);
