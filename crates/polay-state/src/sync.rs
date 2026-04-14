//! State sync manager.
//!
//! Implements a state machine that coordinates the download and verification of
//! a state snapshot from peers.  The sync manager does not perform I/O itself;
//! instead it returns [`SyncAction`]s that the caller must execute (send
//! network messages, apply chunks, etc.).

use std::sync::Arc;

use crate::error::StateResult;
use crate::snapshot::{SnapshotChunk, SnapshotRestorer, StateSnapshot};
use crate::store::StateStore;

// ---------------------------------------------------------------------------
// SyncAction
// ---------------------------------------------------------------------------

/// An action the caller should perform on behalf of the sync manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    /// Send a request to a peer for snapshot metadata at the given height.
    RequestSnapshot(u64),
    /// Send a request to a peer for a specific chunk.
    RequestChunk(u64, u32),
    /// All chunks received and verified; apply them to the store.
    ApplyChunks,
    /// Sync has completed successfully.
    SyncComplete,
    /// Sync has failed.
    SyncFailed(String),
}

// ---------------------------------------------------------------------------
// SyncPhase
// ---------------------------------------------------------------------------

/// Phases of the sync lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncPhase {
    /// No sync in progress.
    Idle,
    /// Waiting for snapshot metadata from a peer.
    RequestingSnapshot,
    /// Downloading chunks.
    DownloadingChunks,
    /// All chunks downloaded; ready to apply and verify.
    Verifying,
    /// Sync completed successfully.
    Complete,
    /// Sync failed.
    Failed(String),
}

// ---------------------------------------------------------------------------
// StateSyncManager
// ---------------------------------------------------------------------------

/// Coordinates the state-sync lifecycle.
///
/// Usage:
/// 1. Call [`start_sync`] to begin.  Execute the returned action.
/// 2. When a snapshot metadata response arrives, call [`on_snapshot_metadata`].
///    Execute all returned actions (chunk requests).
/// 3. As chunks arrive, call [`on_chunk_received`].  Execute any returned
///    actions.
/// 4. When all chunks are in, the manager returns [`SyncAction::ApplyChunks`].
///    The caller should then call [`apply_all_chunks`] and finally get
///    [`SyncAction::SyncComplete`].
pub struct StateSyncManager {
    state: SyncPhase,
    store: Arc<dyn StateStore>,
    /// Snapshot metadata (populated after receiving metadata).
    snapshot: Option<StateSnapshot>,
    /// Chunk storage (populated as chunks arrive).
    chunks: Vec<Option<SnapshotChunk>>,
    /// Received flags.
    received: Vec<bool>,
}

impl StateSyncManager {
    /// Create a new sync manager.
    pub fn new(store: Arc<dyn StateStore>) -> Self {
        Self {
            state: SyncPhase::Idle,
            store,
            snapshot: None,
            chunks: Vec::new(),
            received: Vec::new(),
        }
    }

    /// Start syncing to the given target height.
    pub fn start_sync(&mut self, target_height: u64) -> SyncAction {
        self.state = SyncPhase::RequestingSnapshot;
        SyncAction::RequestSnapshot(target_height)
    }

    /// Handle received snapshot metadata.
    pub fn on_snapshot_metadata(&mut self, snapshot: StateSnapshot) -> Vec<SyncAction> {
        let total = snapshot.total_chunks as usize;
        let height = snapshot.height;

        self.received = vec![false; total];
        self.chunks = vec![None; total];

        let actions: Vec<SyncAction> = (0..total)
            .map(|i| SyncAction::RequestChunk(height, i as u32))
            .collect();

        self.snapshot = Some(snapshot);
        self.state = SyncPhase::DownloadingChunks;

        actions
    }

    /// Handle a received chunk.  Verifies it and stores it internally.
    pub fn on_chunk_received(&mut self, chunk: SnapshotChunk) -> Vec<SyncAction> {
        if self.state != SyncPhase::DownloadingChunks {
            return vec![];
        }

        let snapshot = match &self.snapshot {
            Some(s) => s,
            None => return vec![],
        };

        let idx = chunk.chunk_index as usize;
        if idx >= self.received.len() {
            let msg = format!("chunk index {} out of range", idx);
            self.state = SyncPhase::Failed(msg.clone());
            return vec![SyncAction::SyncFailed(msg)];
        }

        if !SnapshotRestorer::verify_chunk(snapshot, &chunk) {
            let msg = format!("chunk {} failed verification", idx);
            self.state = SyncPhase::Failed(msg.clone());
            return vec![SyncAction::SyncFailed(msg)];
        }

        self.received[idx] = true;
        self.chunks[idx] = Some(chunk);

        if self.received.iter().all(|&r| r) {
            self.state = SyncPhase::Verifying;
            vec![SyncAction::ApplyChunks]
        } else {
            vec![]
        }
    }

    /// Apply all stored chunks to the state store and verify the state root.
    pub fn apply_all_chunks(&mut self) -> StateResult<SyncAction> {
        if self.state != SyncPhase::Verifying {
            let msg = "apply_all_chunks called in wrong state".to_string();
            self.state = SyncPhase::Failed(msg.clone());
            return Ok(SyncAction::SyncFailed(msg));
        }

        // Apply each chunk.
        for chunk in self.chunks.iter().flatten() {
            SnapshotRestorer::apply_chunk(self.store.as_ref(), chunk)?;
        }

        // Verify the state root.
        let snapshot = self.snapshot.as_ref().unwrap();
        let valid =
            SnapshotRestorer::verify_restored_state(self.store.as_ref(), &snapshot.state_root)?;

        if valid {
            self.state = SyncPhase::Complete;
            Ok(SyncAction::SyncComplete)
        } else {
            let msg = "restored state root does not match snapshot".to_string();
            self.state = SyncPhase::Failed(msg.clone());
            Ok(SyncAction::SyncFailed(msg))
        }
    }

    /// Sync progress as a fraction in `[0.0, 1.0]`.
    pub fn progress(&self) -> f64 {
        match &self.state {
            SyncPhase::Idle | SyncPhase::RequestingSnapshot => 0.0,
            SyncPhase::DownloadingChunks => {
                if self.received.is_empty() {
                    return 1.0;
                }
                let done = self.received.iter().filter(|&&r| r).count();
                done as f64 / self.received.len() as f64
            }
            SyncPhase::Verifying | SyncPhase::Complete => 1.0,
            SyncPhase::Failed(_) => 0.0,
        }
    }

    /// Returns `true` if the sync completed successfully.
    pub fn is_complete(&self) -> bool {
        self.state == SyncPhase::Complete
    }

    /// Returns the current phase.
    pub fn phase(&self) -> &SyncPhase {
        &self.state
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
    use crate::snapshot::{SnapshotCreator, CHUNK_SIZE};
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

    // -- full sync flow (happy path) ------------------------------------------

    #[test]
    fn full_sync_happy_path() {
        // Source node: create snapshot.
        let source = MemoryStore::new();
        populate_store(&source, 10);
        let state_root = compute_state_root(&source).unwrap().root;
        let (snapshot, chunks) = SnapshotCreator::create_snapshot(&source, 50, state_root).unwrap();

        // Syncing node: drive the state machine.
        let target = Arc::new(MemoryStore::new());
        let mut mgr = StateSyncManager::new(target.clone());

        // Step 1: start sync.
        let action = mgr.start_sync(50);
        assert_eq!(action, SyncAction::RequestSnapshot(50));
        assert_eq!(mgr.progress(), 0.0);

        // Step 2: receive metadata.
        let actions = mgr.on_snapshot_metadata(snapshot.clone());
        assert_eq!(actions.len(), snapshot.total_chunks as usize);
        for (i, a) in actions.iter().enumerate() {
            assert_eq!(*a, SyncAction::RequestChunk(50, i as u32));
        }

        // Step 3: feed chunks one by one.
        for (i, chunk) in chunks.iter().enumerate() {
            let actions = mgr.on_chunk_received(chunk.clone());
            if i < chunks.len() - 1 {
                assert!(actions.is_empty());
            } else {
                // Last chunk triggers ApplyChunks.
                assert_eq!(actions, vec![SyncAction::ApplyChunks]);
            }
        }
        assert_eq!(mgr.progress(), 1.0);

        // Step 4: apply and verify.
        let result = mgr.apply_all_chunks().unwrap();
        assert_eq!(result, SyncAction::SyncComplete);
        assert!(mgr.is_complete());

        // Verify target state.
        let target_root = compute_state_root(target.as_ref()).unwrap().root;
        assert_eq!(target_root, state_root);
    }

    // -- chunk order doesn't matter -------------------------------------------

    #[test]
    fn sync_chunks_out_of_order() {
        let source = MemoryStore::new();
        populate_store(&source, 15);
        let state_root = compute_state_root(&source).unwrap().root;
        let (snapshot, chunks) = SnapshotCreator::create_snapshot(&source, 5, state_root).unwrap();

        let target = Arc::new(MemoryStore::new());
        let mut mgr = StateSyncManager::new(target.clone());
        mgr.start_sync(5);
        mgr.on_snapshot_metadata(snapshot);

        // Feed chunks in reverse order.
        let mut reversed = chunks.clone();
        reversed.reverse();
        for (i, chunk) in reversed.iter().enumerate() {
            let actions = mgr.on_chunk_received(chunk.clone());
            if i < reversed.len() - 1 {
                assert!(actions.is_empty());
            } else {
                assert_eq!(actions, vec![SyncAction::ApplyChunks]);
            }
        }

        let result = mgr.apply_all_chunks().unwrap();
        assert_eq!(result, SyncAction::SyncComplete);
    }

    // -- chunk verification failure -------------------------------------------

    #[test]
    fn sync_rejects_tampered_chunk() {
        let source = MemoryStore::new();
        populate_store(&source, 5);
        let state_root = compute_state_root(&source).unwrap().root;
        let (snapshot, mut chunks) =
            SnapshotCreator::create_snapshot(&source, 1, state_root).unwrap();

        let target = Arc::new(MemoryStore::new());
        let mut mgr = StateSyncManager::new(target);
        mgr.start_sync(1);
        mgr.on_snapshot_metadata(snapshot);

        // Tamper with the chunk's entries.
        chunks[0].entries.push((b"evil".to_vec(), b"data".to_vec()));

        let actions = mgr.on_chunk_received(chunks[0].clone());
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            SyncAction::SyncFailed(msg) => assert!(msg.contains("failed verification")),
            other => panic!("expected SyncFailed, got {:?}", other),
        }
        assert_eq!(
            *mgr.phase(),
            SyncPhase::Failed("chunk 0 failed verification".into())
        );
    }

    // -- progress tracking ----------------------------------------------------

    #[test]
    fn progress_tracking() {
        let source = MemoryStore::new();
        // Insert enough large entries to get multiple chunks.
        let large_val = vec![0u8; CHUNK_SIZE / 2 + 100];
        for i in 0u8..3 {
            let key = vec![keys::PREFIX_BALANCE, i, 0, 0, 0];
            source.put_raw(&key, &large_val).unwrap();
        }
        let state_root = compute_state_root(&source).unwrap().root;
        let (snapshot, chunks) = SnapshotCreator::create_snapshot(&source, 1, state_root).unwrap();
        assert!(chunks.len() >= 2, "need at least 2 chunks for this test");

        let target = Arc::new(MemoryStore::new());
        let mut mgr = StateSyncManager::new(target);
        mgr.start_sync(1);
        assert_eq!(mgr.progress(), 0.0);

        mgr.on_snapshot_metadata(snapshot);
        assert_eq!(mgr.progress(), 0.0);

        // Feed first chunk.
        mgr.on_chunk_received(chunks[0].clone());
        let expected = 1.0 / chunks.len() as f64;
        assert!((mgr.progress() - expected).abs() < 1e-9);
    }

    // -- apply in wrong state -------------------------------------------------

    #[test]
    fn apply_all_chunks_in_wrong_state() {
        let store = Arc::new(MemoryStore::new());
        let mut mgr = StateSyncManager::new(store);
        let result = mgr.apply_all_chunks().unwrap();
        match result {
            SyncAction::SyncFailed(msg) => assert!(msg.contains("wrong state")),
            other => panic!("expected SyncFailed, got {:?}", other),
        }
    }

    // -- empty snapshot sync --------------------------------------------------

    #[test]
    fn sync_empty_snapshot() {
        let source = MemoryStore::new();
        let state_root = compute_state_root(&source).unwrap().root;
        let (snapshot, chunks) = SnapshotCreator::create_snapshot(&source, 0, state_root).unwrap();

        let target = Arc::new(MemoryStore::new());
        let mut mgr = StateSyncManager::new(target.clone());
        mgr.start_sync(0);
        mgr.on_snapshot_metadata(snapshot);

        // Single empty chunk.
        let actions = mgr.on_chunk_received(chunks[0].clone());
        assert_eq!(actions, vec![SyncAction::ApplyChunks]);

        let result = mgr.apply_all_chunks().unwrap();
        assert_eq!(result, SyncAction::SyncComplete);
        assert!(mgr.is_complete());
    }
}
