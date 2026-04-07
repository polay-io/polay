use criterion::{black_box, criterion_group, criterion_main, Criterion};

use polay_crypto::sha256;
use polay_state::store::{MemoryStore, RocksDbStore, StateStore};
use polay_state::merkle::compute_state_root;

fn bench_memory_store_put(c: &mut Criterion) {
    let store = MemoryStore::new();
    let key = sha256(b"bench-key").to_bytes();
    let value = vec![0xABu8; 128];

    c.bench_function("memory_store_put", |b| {
        b.iter(|| {
            store.put_raw(black_box(&key), black_box(&value)).unwrap();
        });
    });
}

fn bench_memory_store_get(c: &mut Criterion) {
    let store = MemoryStore::new();
    let key = sha256(b"bench-key").to_bytes();
    let value = vec![0xABu8; 128];
    store.put_raw(&key, &value).unwrap();

    c.bench_function("memory_store_get", |b| {
        b.iter(|| {
            black_box(store.get_raw(black_box(&key)).unwrap());
        });
    });
}

fn bench_rocksdb_store_put(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let store = RocksDbStore::new(dir.path().to_str().unwrap()).unwrap();
    let value = vec![0xABu8; 128];

    let mut counter = 0u64;
    c.bench_function("rocksdb_store_put", |b| {
        b.iter(|| {
            let key = sha256(&counter.to_le_bytes()).to_bytes();
            counter += 1;
            store.put_raw(black_box(&key), black_box(&value)).unwrap();
        });
    });
}

fn bench_rocksdb_store_get(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let store = RocksDbStore::new(dir.path().to_str().unwrap()).unwrap();
    let key = sha256(b"bench-key").to_bytes();
    let value = vec![0xABu8; 128];
    store.put_raw(&key, &value).unwrap();

    c.bench_function("rocksdb_store_get", |b| {
        b.iter(|| {
            black_box(store.get_raw(black_box(&key)).unwrap());
        });
    });
}

fn bench_compute_state_root_100(c: &mut Criterion) {
    let store = MemoryStore::new();
    // Populate with 100 entries under the balance prefix (0x02).
    for i in 0u32..100 {
        let mut key = vec![0x02u8]; // PREFIX_BALANCE
        key.extend_from_slice(&sha256(&i.to_le_bytes()).to_bytes());
        let value = i.to_le_bytes().to_vec();
        store.put_raw(&key, &value).unwrap();
    }

    c.bench_function("compute_state_root_100", |b| {
        b.iter(|| {
            black_box(compute_state_root(&store).unwrap());
        });
    });
}

fn bench_compute_state_root_1000(c: &mut Criterion) {
    let store = MemoryStore::new();
    for i in 0u32..1000 {
        let mut key = vec![0x02u8]; // PREFIX_BALANCE
        key.extend_from_slice(&sha256(&i.to_le_bytes()).to_bytes());
        let value = i.to_le_bytes().to_vec();
        store.put_raw(&key, &value).unwrap();
    }

    c.bench_function("compute_state_root_1000", |b| {
        b.iter(|| {
            black_box(compute_state_root(&store).unwrap());
        });
    });
}

fn bench_prefix_scan_100(c: &mut Criterion) {
    let store = MemoryStore::new();
    // Populate with 100 entries under a prefix.
    let prefix = 0x02u8; // PREFIX_BALANCE
    for i in 0u32..100 {
        let mut key = vec![prefix];
        key.extend_from_slice(&sha256(&i.to_le_bytes()).to_bytes());
        let value = vec![0xCDu8; 64];
        store.put_raw(&key, &value).unwrap();
    }

    c.bench_function("prefix_scan_100", |b| {
        b.iter(|| {
            black_box(store.prefix_scan(black_box(&[prefix])).unwrap());
        });
    });
}

criterion_group!(
    benches,
    bench_memory_store_put,
    bench_memory_store_get,
    bench_rocksdb_store_put,
    bench_rocksdb_store_get,
    bench_compute_state_root_100,
    bench_compute_state_root_1000,
    bench_prefix_scan_100,
);
criterion_main!(benches);
