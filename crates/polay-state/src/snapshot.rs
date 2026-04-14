//! State snapshot creation, chunking, and restoration.
//!
//! Allows a node to capture the entire state at a given block height, split it
//! into fixed-size chunks suitable for streaming over the network, and
//! reassemble those chunks on a syncing node.  Each chunk is individually
//! verifiable via a Merkle commitment embedded in the snapshot metadata.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use polay_types::Hash;

use crate::error::StateResult;
use crate::merkle::compute_state_root;
use crate::store::StateStore;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Target maximum serialized size of a single snapshot chunk (1 MiB).
pub const CHUNK_SIZE: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// Snapshot types
// ---------------------------------------------------------------------------

/// Metadata describing a complete state snapshot at a particular height.
///
/// Sent to syncing nodes first so they know how many chunks to expect and can
/// verify each chunk independently.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateSnapshot {
    /// The block height at which the snapshot was taken.
    pub height: u64,
    /// The Merkle state root at this height (computed over the state entries).
    pub state_root: Hash,
    /// Total number of chunks the snapshot is split into.
    pub total_chunks: u32,
    /// One hash per chunk, in order.  Used as Merkle leaves for verification.
    pub chunk_hashes: Vec<Hash>,
}

/// A single chunk of a state snapshot, carrying a batch of key-value pairs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotChunk {
    /// Height at which the snapshot was taken.
    pub height: u64,
    /// Zero-based index of this chunk within the snapshot.
    pub chunk_index: u32,
    /// Total number of chunks in the snapshot.
    pub total_chunks: u32,
    /// Key-value entries contained in this chunk.
    pub entries: Vec<(Vec<u8>, Vec<u8>)>,
    /// Hash of this chunk's entries (must match the corresponding entry in
    /// [`StateSnapshot::chunk_hashes`]).
    pub chunk_hash: Hash,
}

// ---------------------------------------------------------------------------
// SnapshotCreator
// ---------------------------------------------------------------------------

/// Creates snapshot metadata and chunks from a [`StateStore`].
pub struct SnapshotCreator;

impl SnapshotCreator {
    /// Create a full snapshot from a [`StateStore`] at the given `height`.
    ///
    /// `state_root` should be the Merkle state root already computed for this
    /// height (the caller supplies it so the snapshot can record the expected
    /// root without recomputing it here).
    ///
    /// Returns the snapshot metadata together with the ordered list of chunks.
    pub fn create_snapshot(
        store: &dyn StateStore,
        height: u64,
        state_root: Hash,
    ) -> StateResult<(StateSnapshot, Vec<SnapshotChunk>)> {
        // Retrieve ALL key-value pairs from the store using an empty prefix.
        let all_entries = store.prefix_scan(&[])?;

        // Split entries into chunks of roughly CHUNK_SIZE bytes each.
        let raw_chunks = Self::split_into_chunks(&all_entries);

        let mut chunks = Vec::with_capacity(raw_chunks.len());
        let mut chunk_hashes = Vec::with_capacity(raw_chunks.len());

        for (idx, entries) in raw_chunks.into_iter().enumerate() {
            let chunk_hash = Self::hash_chunk(&entries);
            chunk_hashes.push(chunk_hash);
            chunks.push(SnapshotChunk {
                height,
                chunk_index: idx as u32,
                total_chunks: 0, // patched below
                entries,
                chunk_hash,
            });
        }

        let total_chunks = chunks.len() as u32;
        for chunk in &mut chunks {
            chunk.total_chunks = total_chunks;
        }

        let snapshot = StateSnapshot {
            height,
            state_root,
            total_chunks,
            chunk_hashes,
        };

        Ok((snapshot, chunks))
    }

    /// Compute the SHA-256 hash over a chunk's key-value entries.
    ///
    /// The hash is computed as `sha256( len(entries) || for each (k,v):
    /// len(k) || k || len(v) || v )` where lengths are encoded as little-
    /// endian u64 bytes.  This is deterministic for the same ordered set of
    /// entries.
    pub fn hash_chunk(entries: &[(Vec<u8>, Vec<u8>)]) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update((entries.len() as u64).to_le_bytes());
        for (key, value) in entries {
            hasher.update((key.len() as u64).to_le_bytes());
            hasher.update(key);
            hasher.update((value.len() as u64).to_le_bytes());
            hasher.update(value);
        }
        let digest: [u8; 32] = hasher.finalize().into();
        Hash::new(digest)
    }

    /// Split an ordered list of entries into chunks whose total serialized
    /// byte size is approximately [`CHUNK_SIZE`].
    ///
    /// If there are no entries a single empty chunk is still produced so that
    /// a snapshot always has at least one chunk.
    fn split_into_chunks(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<Vec<(Vec<u8>, Vec<u8>)>> {
        if entries.is_empty() {
            return vec![vec![]];
        }

        let mut chunks: Vec<Vec<(Vec<u8>, Vec<u8>)>> = Vec::new();
        let mut current_chunk: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        let mut current_size: usize = 0;

        for (key, value) in entries {
            let entry_size = key.len() + value.len();
            // Start a new chunk if adding this entry would exceed the limit,
            // unless the current chunk is still empty (a single entry that
            // exceeds CHUNK_SIZE still goes into its own chunk).
            if current_size + entry_size > CHUNK_SIZE && !current_chunk.is_empty() {
                chunks.push(std::mem::take(&mut current_chunk));
                current_size = 0;
            }
            current_chunk.push((key.clone(), value.clone()));
            current_size += entry_size;
        }

        if !current_chunk.is_empty() {
            chunks.push(current_chunk);
        }

        chunks
    }
}

// ---------------------------------------------------------------------------
// SnapshotRestorer
// ---------------------------------------------------------------------------

/// Verifies and applies snapshot chunks to a target [`StateStore`].
pub struct SnapshotRestorer;

impl SnapshotRestorer {
    /// Verify a chunk against the snapshot metadata.
    ///
    /// Returns `true` if:
    /// 1. The chunk's index is within range.
    /// 2. The chunk's self-reported hash matches the re-computed hash.
    /// 3. The re-computed hash matches the corresponding entry in the
    ///    snapshot's `chunk_hashes`.
    pub fn verify_chunk(snapshot: &StateSnapshot, chunk: &SnapshotChunk) -> bool {
        let idx = chunk.chunk_index as usize;
        if idx >= snapshot.chunk_hashes.len() {
            return false;
        }

        let computed = SnapshotCreator::hash_chunk(&chunk.entries);
        if computed != chunk.chunk_hash {
            return false;
        }

        snapshot.chunk_hashes[idx] == computed
    }

    /// Apply a verified chunk to the target state store.
    ///
    /// The caller is responsible for verifying the chunk before calling this
    /// method.  Each key-value pair in the chunk is written via `put_raw`.
    pub fn apply_chunk(store: &dyn StateStore, chunk: &SnapshotChunk) -> StateResult<()> {
        for (key, value) in &chunk.entries {
            store.put_raw(key, value)?;
        }
        Ok(())
    }

    /// Verify that the restored state matches the expected Merkle state root.
    ///
    /// This recomputes the state root from the store and compares it against
    /// `expected_root`.
    pub fn verify_restored_state(
        store: &dyn StateStore,
        expected_root: &Hash,
    ) -> StateResult<bool> {
        let commitment = compute_state_root(store)?;
        Ok(commitment.root == *expected_root)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys;
    use crate::merkle::compute_state_root;
    use crate::store::MemoryStore;
    use polay_types::Address;

    /// Helper: populate a MemoryStore with `n` balance entries.
    fn populate_store(store: &MemoryStore, n: usize) {
        for i in 0..n {
            let mut addr_bytes = [0u8; 32];
            addr_bytes[0] = (i >> 24) as u8;
            addr_bytes[1] = (i >> 16) as u8;
            addr_bytes[2] = (i >> 8) as u8;
            addr_bytes[3] = i as u8;
            let addr = Address::new(addr_bytes);
            let key = keys::balance_key(&addr);
            let value = format!("{}", i * 100);
            store.put_raw(&key, value.as_bytes()).unwrap();
        }
    }

    // -- empty store ----------------------------------------------------------

    #[test]
    fn snapshot_empty_store() {
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        let (snapshot, chunks) = SnapshotCreator::create_snapshot(&store, 0, state_root).unwrap();

        assert_eq!(snapshot.height, 0);
        assert_eq!(snapshot.state_root, state_root);
        assert_eq!(snapshot.total_chunks, 1);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].entries.is_empty());

        // Verify the single (empty) chunk.
        assert!(SnapshotRestorer::verify_chunk(&snapshot, &chunks[0]));
    }

    // -- small store (single chunk) -------------------------------------------

    #[test]
    fn snapshot_small_store_single_chunk() {
        let store = MemoryStore::new();
        populate_store(&store, 5);
        let state_root = compute_state_root(&store).unwrap().root;

        let (snapshot, chunks) = SnapshotCreator::create_snapshot(&store, 10, state_root).unwrap();

        assert_eq!(snapshot.height, 10);
        assert_eq!(snapshot.total_chunks, 1);
        assert_eq!(chunks.len(), 1);

        // All 5 entries should be in the single chunk.
        assert_eq!(chunks[0].entries.len(), 5);

        // Verify chunk.
        assert!(SnapshotRestorer::verify_chunk(&snapshot, &chunks[0]));
    }

    // -- restore to a new store -----------------------------------------------

    #[test]
    fn snapshot_restore_matches_original() {
        let source = MemoryStore::new();
        populate_store(&source, 20);
        let state_root = compute_state_root(&source).unwrap().root;

        let (snapshot, chunks) = SnapshotCreator::create_snapshot(&source, 42, state_root).unwrap();

        // Restore into a fresh store.
        let target = MemoryStore::new();
        for chunk in &chunks {
            assert!(SnapshotRestorer::verify_chunk(&snapshot, chunk));
            SnapshotRestorer::apply_chunk(&target, chunk).unwrap();
        }

        // Verify the restored state root matches.
        assert!(SnapshotRestorer::verify_restored_state(&target, &state_root).unwrap());

        // Double-check: the state roots are identical.
        let target_root = compute_state_root(&target).unwrap().root;
        assert_eq!(state_root, target_root);
    }

    // -- multi-chunk store ----------------------------------------------------

    #[test]
    fn snapshot_multi_chunk() {
        let store = MemoryStore::new();
        // Insert enough data to produce multiple chunks.  Each entry is
        // ~33 bytes key + a large value.  With CHUNK_SIZE = 1 MiB we need
        // a few MiB of data total.
        let large_value = vec![0xABu8; 64 * 1024]; // 64 KiB per entry
        for i in 0u32..40 {
            let mut key = vec![keys::PREFIX_BALANCE];
            key.extend_from_slice(&i.to_be_bytes());
            key.resize(33, 0);
            store.put_raw(&key, &large_value).unwrap();
        }

        let state_root = compute_state_root(&store).unwrap().root;
        let (snapshot, chunks) = SnapshotCreator::create_snapshot(&store, 99, state_root).unwrap();

        // We should have more than one chunk.
        assert!(
            snapshot.total_chunks > 1,
            "expected multiple chunks, got {}",
            snapshot.total_chunks
        );
        assert_eq!(chunks.len(), snapshot.total_chunks as usize);

        // Verify all chunks.
        for chunk in &chunks {
            assert!(SnapshotRestorer::verify_chunk(&snapshot, chunk));
        }

        // Restore and verify state root.
        let target = MemoryStore::new();
        for chunk in &chunks {
            SnapshotRestorer::apply_chunk(&target, chunk).unwrap();
        }
        assert!(SnapshotRestorer::verify_restored_state(&target, &state_root).unwrap());
    }

    // -- chunk verification failures ------------------------------------------

    #[test]
    fn verify_chunk_rejects_tampered_entries() {
        let store = MemoryStore::new();
        populate_store(&store, 5);
        let state_root = compute_state_root(&store).unwrap().root;

        let (snapshot, mut chunks) =
            SnapshotCreator::create_snapshot(&store, 1, state_root).unwrap();

        // Tamper with the first entry's value.
        chunks[0].entries[0].1 = b"tampered".to_vec();

        assert!(!SnapshotRestorer::verify_chunk(&snapshot, &chunks[0]));
    }

    #[test]
    fn verify_chunk_rejects_wrong_index() {
        let store = MemoryStore::new();
        populate_store(&store, 5);
        let state_root = compute_state_root(&store).unwrap().root;

        let (snapshot, mut chunks) =
            SnapshotCreator::create_snapshot(&store, 1, state_root).unwrap();

        // Point to a chunk index that doesn't exist.
        chunks[0].chunk_index = 999;
        assert!(!SnapshotRestorer::verify_chunk(&snapshot, &chunks[0]));
    }

    #[test]
    fn verify_restored_state_detects_mismatch() {
        let store = MemoryStore::new();
        populate_store(&store, 3);

        let wrong_root = Hash::new([0xFF; 32]);
        assert!(!SnapshotRestorer::verify_restored_state(&store, &wrong_root).unwrap());
    }

    // -- hash_chunk determinism -----------------------------------------------

    #[test]
    fn hash_chunk_deterministic() {
        let entries: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (b"key1".to_vec(), b"val1".to_vec()),
            (b"key2".to_vec(), b"val2".to_vec()),
        ];
        let h1 = SnapshotCreator::hash_chunk(&entries);
        let h2 = SnapshotCreator::hash_chunk(&entries);
        assert_eq!(h1, h2);
        assert_ne!(h1, Hash::ZERO);
    }

    #[test]
    fn hash_chunk_differs_for_different_entries() {
        let entries_a: Vec<(Vec<u8>, Vec<u8>)> = vec![(b"key1".to_vec(), b"val1".to_vec())];
        let entries_b: Vec<(Vec<u8>, Vec<u8>)> = vec![(b"key1".to_vec(), b"val2".to_vec())];
        assert_ne!(
            SnapshotCreator::hash_chunk(&entries_a),
            SnapshotCreator::hash_chunk(&entries_b),
        );
    }

    // -- snapshot with mixed prefixes -----------------------------------------

    #[test]
    fn snapshot_includes_all_prefixes() {
        let store = MemoryStore::new();

        // Insert entries under different prefixes.
        let addr = Address::new([0x01; 32]);
        store.put_raw(&keys::balance_key(&addr), b"100").unwrap();
        store
            .put_raw(&keys::account_key(&addr), b"acct-data")
            .unwrap();
        store.put_raw(&keys::chain_height_key(), b"42").unwrap();
        store.put_raw(&keys::block_key(1), b"block-data").unwrap();

        let state_root = compute_state_root(&store).unwrap().root;

        let (snapshot, chunks) = SnapshotCreator::create_snapshot(&store, 42, state_root).unwrap();

        // All 4 entries should be captured (prefix_scan with empty prefix
        // returns everything, including chain meta and blocks).
        let total_entries: usize = chunks.iter().map(|c| c.entries.len()).sum();
        assert_eq!(total_entries, 4);

        // Restore to a fresh store.
        let target = MemoryStore::new();
        for chunk in &chunks {
            assert!(SnapshotRestorer::verify_chunk(&snapshot, chunk));
            SnapshotRestorer::apply_chunk(&target, chunk).unwrap();
        }

        // The balance and account entries should be present.
        assert_eq!(
            target.get_raw(&keys::balance_key(&addr)).unwrap().unwrap(),
            b"100".to_vec(),
        );
        assert_eq!(
            target.get_raw(&keys::account_key(&addr)).unwrap().unwrap(),
            b"acct-data".to_vec(),
        );
    }
}
