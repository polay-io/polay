//! Write-overlay on top of a base [`StateStore`].
//!
//! Used by the parallel executor to give each transaction its own isolated
//! write buffer while sharing the same base state for reads.  After execution
//! the overlay can be drained and flushed into the base store.

use std::collections::BTreeMap;

use parking_lot::RwLock;

use crate::error::StateResult;
use crate::store::StateStore;

/// A write-overlay on top of a base [`StateStore`].
///
/// Reads check the overlay first, then fall through to the base store.
/// Writes go to the overlay only.  After execution, the overlay can be
/// drained and its writes applied to the base store.
pub struct OverlayStore<'a> {
    base: &'a dyn StateStore,
    overlay: RwLock<BTreeMap<Vec<u8>, Option<Vec<u8>>>>, // None = tombstone (deleted)
}

impl<'a> OverlayStore<'a> {
    /// Create a new empty overlay backed by `base`.
    pub fn new(base: &'a dyn StateStore) -> Self {
        Self {
            base,
            overlay: RwLock::new(BTreeMap::new()),
        }
    }

    /// Consume the overlay and return all pending writes.
    ///
    /// Each entry is `(key, Some(value))` for puts or `(key, None)` for deletes.
    pub fn drain_writes(self) -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
        self.overlay.into_inner()
    }

    /// Consume the overlay and flush all pending writes directly into the
    /// base store.  Returns the number of write operations applied.
    pub fn flush(self) -> StateResult<usize> {
        let writes = self.overlay.into_inner();
        let count = writes.len();
        for (key, value) in writes {
            match value {
                Some(data) => self.base.put_raw(&key, &data)?,
                None => self.base.delete(&key)?,
            }
        }
        Ok(count)
    }

    /// Return the number of pending writes in the overlay.
    pub fn pending_writes(&self) -> usize {
        self.overlay.read().len()
    }
}

// SAFETY: OverlayStore is Send + Sync because:
// - `base` is `&dyn StateStore` which is `Send + Sync` (trait bound)
// - `overlay` is `RwLock<BTreeMap<...>>` which is `Send + Sync`
unsafe impl<'a> Send for OverlayStore<'a> {}
unsafe impl<'a> Sync for OverlayStore<'a> {}

impl<'a> StateStore for OverlayStore<'a> {
    fn get_raw(&self, key: &[u8]) -> StateResult<Option<Vec<u8>>> {
        // Check overlay first.
        let overlay = self.overlay.read();
        if let Some(entry) = overlay.get(key) {
            return Ok(entry.clone()); // Some(data) or None (tombstone)
        }
        // Fall through to base store.
        self.base.get_raw(key)
    }

    fn put_raw(&self, key: &[u8], value: &[u8]) -> StateResult<()> {
        self.overlay
            .write()
            .insert(key.to_vec(), Some(value.to_vec()));
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> StateResult<()> {
        self.overlay.write().insert(key.to_vec(), None);
        Ok(())
    }

    fn prefix_scan(&self, prefix: &[u8]) -> StateResult<Vec<(Vec<u8>, Vec<u8>)>> {
        // Start with base results.
        let mut results: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
        for (k, v) in self.base.prefix_scan(prefix)? {
            results.insert(k, v);
        }

        // Apply overlay on top (puts override, tombstones remove).
        let overlay = self.overlay.read();
        for (k, v) in overlay.iter() {
            if k.starts_with(prefix) {
                match v {
                    Some(data) => {
                        results.insert(k.clone(), data.clone());
                    }
                    None => {
                        results.remove(k);
                    }
                }
            }
        }

        Ok(results.into_iter().collect())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;

    #[test]
    fn overlay_reads_from_base() {
        let base = MemoryStore::new();
        base.put_raw(b"key1", b"value1").unwrap();

        let overlay = OverlayStore::new(&base);
        assert_eq!(overlay.get_raw(b"key1").unwrap(), Some(b"value1".to_vec()));
    }

    #[test]
    fn overlay_writes_dont_affect_base() {
        let base = MemoryStore::new();
        let overlay = OverlayStore::new(&base);

        overlay.put_raw(b"key1", b"overlay_value").unwrap();

        // Overlay sees it.
        assert_eq!(
            overlay.get_raw(b"key1").unwrap(),
            Some(b"overlay_value".to_vec())
        );
        // Base does not.
        assert!(base.get_raw(b"key1").unwrap().is_none());
    }

    #[test]
    fn overlay_write_shadows_base() {
        let base = MemoryStore::new();
        base.put_raw(b"key1", b"base_value").unwrap();

        let overlay = OverlayStore::new(&base);
        overlay.put_raw(b"key1", b"overlay_value").unwrap();

        assert_eq!(
            overlay.get_raw(b"key1").unwrap(),
            Some(b"overlay_value".to_vec())
        );
        // Base is unchanged.
        assert_eq!(base.get_raw(b"key1").unwrap(), Some(b"base_value".to_vec()));
    }

    #[test]
    fn overlay_flush_applies_to_base() {
        let base = MemoryStore::new();
        base.put_raw(b"existing", b"old").unwrap();

        let overlay = OverlayStore::new(&base);
        overlay.put_raw(b"key1", b"val1").unwrap();
        overlay.put_raw(b"existing", b"new").unwrap();

        let count = overlay.flush().unwrap();
        assert_eq!(count, 2);
        assert_eq!(base.get_raw(b"key1").unwrap(), Some(b"val1".to_vec()));
        assert_eq!(base.get_raw(b"existing").unwrap(), Some(b"new".to_vec()));
    }

    #[test]
    fn overlay_delete_hides_base_value() {
        let base = MemoryStore::new();
        base.put_raw(b"key1", b"value1").unwrap();

        let overlay = OverlayStore::new(&base);
        overlay.delete(b"key1").unwrap();

        // Overlay sees a tombstone (None).
        assert!(overlay.get_raw(b"key1").unwrap().is_none());
        // Base is unchanged.
        assert_eq!(base.get_raw(b"key1").unwrap(), Some(b"value1".to_vec()));
    }

    #[test]
    fn overlay_delete_flushes_as_delete() {
        let base = MemoryStore::new();
        base.put_raw(b"key1", b"value1").unwrap();

        let overlay = OverlayStore::new(&base);
        overlay.delete(b"key1").unwrap();
        overlay.flush().unwrap();

        assert!(base.get_raw(b"key1").unwrap().is_none());
    }

    #[test]
    fn overlay_prefix_scan_merges() {
        let base = MemoryStore::new();
        base.put_raw(b"pfx:a", b"base_a").unwrap();
        base.put_raw(b"pfx:b", b"base_b").unwrap();
        base.put_raw(b"pfx:c", b"base_c").unwrap();
        base.put_raw(b"other:x", b"other_x").unwrap();

        let overlay = OverlayStore::new(&base);
        // Override one, delete one, add one.
        overlay.put_raw(b"pfx:a", b"overlay_a").unwrap();
        overlay.delete(b"pfx:b").unwrap();
        overlay.put_raw(b"pfx:d", b"overlay_d").unwrap();

        let results = overlay.prefix_scan(b"pfx:").unwrap();
        let keys: Vec<Vec<u8>> = results.iter().map(|(k, _)| k.clone()).collect();
        let vals: Vec<Vec<u8>> = results.iter().map(|(_, v)| v.clone()).collect();

        assert_eq!(
            keys,
            vec![b"pfx:a".to_vec(), b"pfx:c".to_vec(), b"pfx:d".to_vec()]
        );
        assert_eq!(
            vals,
            vec![
                b"overlay_a".to_vec(),
                b"base_c".to_vec(),
                b"overlay_d".to_vec()
            ]
        );
    }

    #[test]
    fn overlay_pending_writes_count() {
        let base = MemoryStore::new();
        let overlay = OverlayStore::new(&base);

        assert_eq!(overlay.pending_writes(), 0);
        overlay.put_raw(b"k1", b"v1").unwrap();
        assert_eq!(overlay.pending_writes(), 1);
        overlay.put_raw(b"k2", b"v2").unwrap();
        assert_eq!(overlay.pending_writes(), 2);
        overlay.delete(b"k3").unwrap();
        assert_eq!(overlay.pending_writes(), 3);
    }

    #[test]
    fn overlay_drain_writes_returns_all() {
        let base = MemoryStore::new();
        let overlay = OverlayStore::new(&base);

        overlay.put_raw(b"k1", b"v1").unwrap();
        overlay.delete(b"k2").unwrap();

        let writes = overlay.drain_writes();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[&b"k1".to_vec()], Some(b"v1".to_vec()));
        assert_eq!(writes[&b"k2".to_vec()], None);
    }

    #[test]
    fn empty_overlay_flush_returns_zero() {
        let base = MemoryStore::new();
        let overlay = OverlayStore::new(&base);
        assert_eq!(overlay.flush().unwrap(), 0);
    }

    #[test]
    fn overlay_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<OverlayStore<'_>>();
    }
}
