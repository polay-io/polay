//! Storage backend trait and implementations.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use parking_lot::RwLock;

use crate::error::{StateError, StateResult};

// ---------------------------------------------------------------------------
// StateStore trait
// ---------------------------------------------------------------------------

/// A low-level key-value store used for all on-chain state.
///
/// Only raw-byte operations are part of the trait so it remains
/// dyn-compatible. Typed helpers are provided as free functions below.
pub trait StateStore: Send + Sync {
    /// Retrieve raw bytes for the given key, or `None` if not present.
    fn get_raw(&self, key: &[u8]) -> StateResult<Option<Vec<u8>>>;

    /// Store raw bytes under the given key.
    fn put_raw(&self, key: &[u8], value: &[u8]) -> StateResult<()>;

    /// Delete a key from the store. No-op if the key does not exist.
    fn delete(&self, key: &[u8]) -> StateResult<()>;

    /// Return all key-value pairs whose key starts with `prefix`, sorted
    /// lexicographically by key.
    fn prefix_scan(&self, prefix: &[u8]) -> StateResult<Vec<(Vec<u8>, Vec<u8>)>>;
}

// ---------------------------------------------------------------------------
// Typed convenience helpers (free functions)
// ---------------------------------------------------------------------------

/// Deserialize a Borsh-encoded value from the store.
pub fn store_get<T: BorshDeserialize>(
    store: &dyn StateStore,
    key: &[u8],
) -> StateResult<Option<T>> {
    match store.get_raw(key)? {
        Some(bytes) => {
            let val = T::try_from_slice(&bytes).map_err(|e| {
                StateError::SerializationError(format!(
                    "borsh decode for key {}: {}",
                    hex::encode(key),
                    e,
                ))
            })?;
            Ok(Some(val))
        }
        None => Ok(None),
    }
}

/// Borsh-encode a value and store it.
pub fn store_put<T: BorshSerialize>(
    store: &dyn StateStore,
    key: &[u8],
    value: &T,
) -> StateResult<()> {
    let bytes = borsh::to_vec(value).map_err(|e| {
        StateError::SerializationError(format!("borsh encode for key {}: {}", hex::encode(key), e,))
    })?;
    store.put_raw(key, &bytes)
}

// ---------------------------------------------------------------------------
// RocksDbStore
// ---------------------------------------------------------------------------

/// A persistent [`StateStore`] backed by RocksDB.
pub struct RocksDbStore {
    db: rocksdb::DB,
}

impl RocksDbStore {
    /// Open (or create) a RocksDB database at `path`.
    pub fn new(path: &str) -> StateResult<Self> {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        let db =
            rocksdb::DB::open(&opts, path).map_err(|e| StateError::StorageError(e.to_string()))?;
        Ok(Self { db })
    }
}

impl StateStore for RocksDbStore {
    fn get_raw(&self, key: &[u8]) -> StateResult<Option<Vec<u8>>> {
        self.db
            .get(key)
            .map_err(|e| StateError::StorageError(e.to_string()))
    }

    fn put_raw(&self, key: &[u8], value: &[u8]) -> StateResult<()> {
        self.db
            .put(key, value)
            .map_err(|e| StateError::StorageError(e.to_string()))
    }

    fn delete(&self, key: &[u8]) -> StateResult<()> {
        self.db
            .delete(key)
            .map_err(|e| StateError::StorageError(e.to_string()))
    }

    fn prefix_scan(&self, prefix: &[u8]) -> StateResult<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut results = Vec::new();
        let iter = self.db.prefix_iterator(prefix);
        for item in iter {
            let (key, value) = item.map_err(|e| StateError::StorageError(e.to_string()))?;
            if !key.starts_with(prefix) {
                break;
            }
            results.push((key.to_vec(), value.to_vec()));
        }
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// MemoryStore
// ---------------------------------------------------------------------------

/// An in-memory [`StateStore`] backed by a `BTreeMap`, intended for testing.
pub struct MemoryStore {
    data: RwLock<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl MemoryStore {
    /// Create a new, empty in-memory store.
    pub fn new() -> Self {
        Self {
            data: RwLock::new(BTreeMap::new()),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStore for MemoryStore {
    fn get_raw(&self, key: &[u8]) -> StateResult<Option<Vec<u8>>> {
        Ok(self.data.read().get(key).cloned())
    }

    fn put_raw(&self, key: &[u8], value: &[u8]) -> StateResult<()> {
        self.data.write().insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> StateResult<()> {
        self.data.write().remove(key);
        Ok(())
    }

    fn prefix_scan(&self, prefix: &[u8]) -> StateResult<Vec<(Vec<u8>, Vec<u8>)>> {
        let data = self.data.read();
        let prefix_vec = prefix.to_vec();
        // BTreeMap range from prefix.. gives us all keys >= prefix.
        // We stop once the key no longer starts with the prefix.
        let results = data
            .range(prefix_vec..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn run_basic_crud(store: &dyn StateStore) {
        let key = b"test-key";
        let value = b"test-value";

        // Initially empty.
        assert!(store.get_raw(key).unwrap().is_none());

        // Put and get.
        store.put_raw(key, value).unwrap();
        assert_eq!(store.get_raw(key).unwrap().unwrap(), value.to_vec());

        // Overwrite.
        let value2 = b"updated";
        store.put_raw(key, value2).unwrap();
        assert_eq!(store.get_raw(key).unwrap().unwrap(), value2.to_vec());

        // Delete.
        store.delete(key).unwrap();
        assert!(store.get_raw(key).unwrap().is_none());

        // Delete of nonexistent key is a no-op.
        store.delete(key).unwrap();
    }

    fn run_borsh_typed(store: &dyn StateStore) {
        let key = b"typed";
        let val: u64 = 42;
        store_put(store, key, &val).unwrap();
        let got: u64 = store_get(store, key).unwrap().unwrap();
        assert_eq!(got, 42);
    }

    #[test]
    fn memory_store_crud() {
        run_basic_crud(&MemoryStore::new());
    }

    #[test]
    fn memory_store_borsh_typed() {
        run_borsh_typed(&MemoryStore::new());
    }

    #[test]
    fn rocksdb_store_crud() {
        let dir = tempfile::tempdir().unwrap();
        let store = RocksDbStore::new(dir.path().to_str().unwrap()).unwrap();
        run_basic_crud(&store);
    }

    #[test]
    fn rocksdb_store_borsh_typed() {
        let dir = tempfile::tempdir().unwrap();
        let store = RocksDbStore::new(dir.path().to_str().unwrap()).unwrap();
        run_borsh_typed(&store);
    }
}
