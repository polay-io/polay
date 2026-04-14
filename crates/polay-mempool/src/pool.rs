use std::collections::BTreeMap;

use dashmap::DashMap;
use tracing::{debug, warn};

use polay_crypto::PolayPublicKey;
use polay_types::{Address, Hash, SignedTransaction};

use crate::error::MempoolError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default maximum nonce gap allowed per sender.
const DEFAULT_MAX_NONCE_GAP: u64 = 16;

// ---------------------------------------------------------------------------
// MempoolConfig
// ---------------------------------------------------------------------------

/// Configuration parameters for the transaction mempool.
#[derive(Debug, Clone)]
pub struct MempoolConfig {
    /// Maximum number of transactions the pool will hold.
    pub max_size: usize,
    /// Maximum number of pending transactions allowed per account.
    pub max_per_account: usize,
    /// Minimum fee (in native token units) a transaction must carry.
    pub min_fee: u64,
    /// When true, verify the Ed25519 signature on every insert as a
    /// second line of defense.
    pub verify_signature: bool,
    /// Expected chain_id. If set, transactions with a different chain_id are
    /// rejected. If empty, the check is skipped.
    pub chain_id: String,
    /// Maximum allowed nonce gap per sender. Transactions with a nonce too
    /// far ahead of the sender's lowest pending nonce are rejected.
    pub max_nonce_gap: u64,
    /// Maximum age of a transaction in seconds. Transactions older than this
    /// are evicted during cleanup sweeps. 0 means no TTL.
    pub tx_ttl_secs: u64,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10_000,
            max_per_account: 100,
            min_fee: 1_000,
            verify_signature: true,
            chain_id: String::new(),
            max_nonce_gap: DEFAULT_MAX_NONCE_GAP,
            tx_ttl_secs: 300, // 5 minutes
        }
    }
}

// ---------------------------------------------------------------------------
// Mempool
// ---------------------------------------------------------------------------

/// A concurrent, fee-prioritized transaction mempool for the POLAY blockchain.
///
/// The pool stores pending transactions indexed by their hash and maintains
/// per-sender queues ordered by nonce. All operations are lock-free at the
/// map level thanks to [`DashMap`], making the mempool safe for concurrent
/// reads and writes from multiple async tasks.
pub struct Mempool {
    /// Primary store: tx_hash -> SignedTransaction.
    pending: DashMap<Hash, SignedTransaction>,

    /// Per-sender index: sender address -> (nonce -> tx_hash).
    ///
    /// The inner `BTreeMap` keeps transactions for a given sender sorted by
    /// nonce, which is critical for correct ordering during block production.
    by_sender: DashMap<Address, BTreeMap<u64, Hash>>,

    /// Recently seen transaction hashes (replay protection).
    /// Tracks hashes of transactions that have been included in blocks or
    /// previously submitted, to quickly reject duplicates.
    recently_seen: DashMap<Hash, ()>,

    /// Pool configuration.
    config: MempoolConfig,
}

impl Mempool {
    /// Create a new mempool with the given configuration.
    pub fn new(config: MempoolConfig) -> Self {
        Self {
            pending: DashMap::new(),
            by_sender: DashMap::new(),
            recently_seen: DashMap::new(),
            config,
        }
    }

    // -- Insertion ---------------------------------------------------------

    /// Insert a signed transaction into the mempool.
    ///
    /// # Errors
    ///
    /// Returns [`MempoolError`] if:
    /// - The transaction already exists (`TransactionAlreadyExists`)
    /// - The transaction was recently seen (`DuplicateTransaction`)
    /// - The chain_id does not match (`ChainIdMismatch`)
    /// - The pool is at capacity (`MempoolFull`)
    /// - The sender already has `max_per_account` pending txs (`MempoolFull`)
    /// - The fee is below the configured minimum (`FeeTooLow`)
    /// - The nonce gap is too large (`NonceGapTooLarge`)
    pub fn insert(&self, tx: SignedTransaction) -> Result<(), MempoolError> {
        let tx_hash = tx.tx_hash;
        let sender = *tx.signer();
        let nonce = tx.nonce();
        let fee = tx.transaction.max_fee;

        // 1. Duplicate check (pending pool).
        if self.pending.contains_key(&tx_hash) {
            return Err(MempoolError::TransactionAlreadyExists);
        }

        // 1b. Replay protection: check recently-seen set.
        if self.recently_seen.contains_key(&tx_hash) {
            return Err(MempoolError::DuplicateTransaction);
        }

        // 1c. Chain ID check.
        if !self.config.chain_id.is_empty() && tx.transaction.chain_id != self.config.chain_id {
            return Err(MempoolError::ChainIdMismatch {
                expected: self.config.chain_id.clone(),
                got: tx.transaction.chain_id.clone(),
            });
        }

        // 2. Global capacity check -- try eviction if full.
        if self.pending.len() >= self.config.max_size && !self.try_evict_lowest_fee(&tx) {
            warn!(
                pool_size = self.pending.len(),
                max = self.config.max_size,
                "mempool is full, rejecting transaction"
            );
            return Err(MempoolError::MempoolFull);
        }

        // 3. Per-account limit check.
        if let Some(sender_txs) = self.by_sender.get(&sender) {
            if sender_txs.len() >= self.config.max_per_account {
                warn!(
                    sender = %sender,
                    count = sender_txs.len(),
                    max = self.config.max_per_account,
                    "per-account limit reached"
                );
                return Err(MempoolError::MempoolFull);
            }
        }

        // 4. Minimum fee check.
        if fee < self.config.min_fee {
            return Err(MempoolError::FeeTooLow {
                minimum: self.config.min_fee,
                got: fee,
            });
        }

        // 4b. Nonce gap check.
        self.check_nonce_gap(&sender, nonce)?;

        // 5. Signature verification (second line of defense).
        if self.config.verify_signature {
            if tx.signer_pubkey.len() != 32 {
                return Err(MempoolError::InvalidSignature(format!(
                    "signer_pubkey must be 32 bytes, got {}",
                    tx.signer_pubkey.len(),
                )));
            }
            let pubkey_bytes: [u8; 32] = tx.signer_pubkey[..32]
                .try_into()
                .expect("length already checked");
            let pubkey = PolayPublicKey::from_bytes(&pubkey_bytes)
                .map_err(|e| MempoolError::InvalidSignature(format!("invalid public key: {e}")))?;
            polay_crypto::verify_transaction_with_key(&tx, &pubkey).map_err(|e| {
                MempoolError::InvalidSignature(format!("signature verification failed: {e}"))
            })?;
        }

        // 6. Insert into primary store.
        self.pending.insert(tx_hash, tx);

        // 7. Insert into per-sender index.
        self.by_sender
            .entry(sender)
            .or_default()
            .insert(nonce, tx_hash);

        debug!(
            tx_hash = %tx_hash,
            sender = %sender,
            nonce = nonce,
            fee = fee,
            pool_size = self.pending.len(),
            "transaction inserted into mempool"
        );

        Ok(())
    }

    // -- Removal -----------------------------------------------------------

    /// Remove a single transaction by its hash, returning it if it existed.
    pub fn remove(&self, tx_hash: &Hash) -> Option<SignedTransaction> {
        let (_, tx) = self.pending.remove(tx_hash)?;

        // Clean up the per-sender index.
        let sender = *tx.signer();
        let nonce = tx.nonce();

        if let Some(mut sender_txs) = self.by_sender.get_mut(&sender) {
            sender_txs.remove(&nonce);
            if sender_txs.is_empty() {
                // Drop the mutable reference before removing the key so we
                // don't deadlock on the same DashMap shard.
                drop(sender_txs);
                self.by_sender.remove(&sender);
            }
        }

        debug!(tx_hash = %tx_hash, "transaction removed from mempool");

        Some(tx)
    }

    /// Remove a batch of transactions by their hashes.
    ///
    /// This is typically called after a block is finalized to evict all
    /// transactions that were included in the block.
    pub fn remove_batch(&self, tx_hashes: &[Hash]) {
        for tx_hash in tx_hashes {
            self.remove(tx_hash);
        }
    }

    // -- Queries -----------------------------------------------------------

    /// Look up a transaction by hash. Returns a clone if present.
    pub fn get(&self, tx_hash: &Hash) -> Option<SignedTransaction> {
        self.pending.get(tx_hash).map(|entry| entry.clone())
    }

    /// Check whether a transaction exists in the pool.
    pub fn contains(&self, tx_hash: &Hash) -> bool {
        self.pending.contains_key(tx_hash)
    }

    /// Return up to `max_txs` transactions for inclusion in the next block,
    /// prioritized by fee (highest `max_fee` first).
    pub fn get_pending_for_block(&self, max_txs: usize) -> Vec<SignedTransaction> {
        // Collect all pending transactions.
        let mut txs: Vec<SignedTransaction> = self
            .pending
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by fee descending (highest-paying transactions first).
        txs.sort_by(|a, b| b.transaction.max_fee.cmp(&a.transaction.max_fee));

        txs.truncate(max_txs);
        txs
    }

    /// Return all pending transactions for a given sender, ordered by nonce.
    pub fn get_account_txs(&self, sender: &Address) -> Vec<SignedTransaction> {
        let Some(sender_txs) = self.by_sender.get(sender) else {
            return Vec::new();
        };

        sender_txs
            .values()
            .filter_map(|tx_hash| self.pending.get(tx_hash).map(|e| e.clone()))
            .collect()
    }

    /// Return the current number of transactions in the pool.
    pub fn size(&self) -> usize {
        self.pending.len()
    }

    /// Evict transactions older than `tx_ttl_secs`.
    ///
    /// Returns the number of transactions evicted. Call this periodically
    /// (e.g. once per block production cycle) to keep the mempool fresh.
    pub fn evict_expired(&self) -> usize {
        if self.config.tx_ttl_secs == 0 {
            return 0;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let cutoff = now.saturating_sub(self.config.tx_ttl_secs);
        let mut expired: Vec<Hash> = Vec::new();

        for entry in self.pending.iter() {
            if entry.value().transaction.timestamp < cutoff {
                expired.push(*entry.key());
            }
        }

        let count = expired.len();
        for tx_hash in &expired {
            self.remove(tx_hash);
        }

        if count > 0 {
            debug!(
                evicted = count,
                ttl = self.config.tx_ttl_secs,
                "evicted expired transactions"
            );
        }

        count
    }

    /// Remove all transactions from the pool.
    pub fn clear(&self) {
        self.pending.clear();
        self.by_sender.clear();
        self.recently_seen.clear();

        debug!("mempool cleared");
    }

    // -- Security ----------------------------------------------------------

    /// Check whether a nonce gap is within the allowed limit.
    ///
    /// If the sender already has pending transactions, the new transaction's
    /// nonce must not be more than `max_nonce_gap` ahead of the lowest
    /// pending nonce.
    fn check_nonce_gap(&self, sender: &Address, nonce: u64) -> Result<(), MempoolError> {
        if let Some(sender_txs) = self.by_sender.get(sender) {
            if let Some((&lowest_nonce, _)) = sender_txs.iter().next() {
                let gap = nonce.saturating_sub(lowest_nonce);
                if gap > self.config.max_nonce_gap {
                    return Err(MempoolError::NonceGapTooLarge {
                        max_gap: self.config.max_nonce_gap,
                        gap,
                    });
                }
            }
        }
        Ok(())
    }

    /// Try to evict the lowest-fee transaction in the pool to make room for a
    /// higher-fee transaction.
    ///
    /// Returns `true` if eviction succeeded and space was freed.
    fn try_evict_lowest_fee(&self, new_tx: &SignedTransaction) -> bool {
        let new_fee = new_tx.transaction.max_fee;

        // Find the lowest-fee transaction in the pool.
        let mut lowest_fee = u64::MAX;
        let mut lowest_hash = None;
        for entry in self.pending.iter() {
            let fee = entry.value().transaction.max_fee;
            if fee < lowest_fee {
                lowest_fee = fee;
                lowest_hash = Some(*entry.key());
            }
        }

        // Only evict if the new tx has a strictly higher fee.
        if let Some(hash) = lowest_hash {
            if new_fee > lowest_fee {
                self.remove(&hash);
                debug!(
                    evicted_hash = %hash,
                    evicted_fee = lowest_fee,
                    new_fee = new_fee,
                    "evicted lowest-fee transaction to make room"
                );
                return true;
            }
        }

        false
    }

    /// Mark a transaction hash as recently seen (for replay protection).
    ///
    /// Call this after a transaction is included in a block.
    pub fn mark_seen(&self, tx_hash: Hash) {
        self.recently_seen.insert(tx_hash, ());
    }

    /// Mark a batch of transaction hashes as recently seen.
    pub fn mark_seen_batch(&self, tx_hashes: &[Hash]) {
        for hash in tx_hashes {
            self.recently_seen.insert(*hash, ());
        }
    }

    /// Clear the recently-seen set. Call periodically or after a certain
    /// number of blocks to reclaim memory.
    pub fn clear_recently_seen(&self) {
        self.recently_seen.clear();
    }

    /// Return the number of recently-seen entries.
    pub fn recently_seen_count(&self) -> usize {
        self.recently_seen.len()
    }

    // -- Maintenance -------------------------------------------------------

    /// Remove all transactions from `sender` whose nonce is strictly below
    /// `min_nonce`.
    ///
    /// This is called after state updates confirm that certain nonces have
    /// been executed, making those transactions obsolete.
    pub fn prune_below_nonce(&self, sender: &Address, min_nonce: u64) {
        let hashes_to_remove: Vec<Hash> = {
            let Some(mut sender_txs) = self.by_sender.get_mut(sender) else {
                return;
            };

            // Collect all nonces below the threshold.
            let stale_nonces: Vec<u64> = sender_txs
                .range(..min_nonce)
                .map(|(&nonce, _)| nonce)
                .collect();

            let mut hashes = Vec::with_capacity(stale_nonces.len());
            for nonce in stale_nonces {
                if let Some(hash) = sender_txs.remove(&nonce) {
                    hashes.push(hash);
                }
            }

            hashes
        };

        // Remove from the primary store (outside the by_sender borrow).
        for hash in &hashes_to_remove {
            self.pending.remove(hash);
        }

        // Clean up empty sender entry.
        if let Some(sender_txs) = self.by_sender.get(sender) {
            if sender_txs.is_empty() {
                drop(sender_txs);
                self.by_sender.remove(sender);
            }
        }

        if !hashes_to_remove.is_empty() {
            debug!(
                sender = %sender,
                min_nonce = min_nonce,
                pruned = hashes_to_remove.len(),
                "pruned stale transactions"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_types::{Signature, Transaction, TransactionAction};

    /// Helper: build a `SignedTransaction` with the given sender bytes, nonce,
    /// fee, and a unique tx_hash derived from `hash_seed`.
    fn make_tx(sender_byte: u8, nonce: u64, fee: u64, hash_seed: u8) -> SignedTransaction {
        let sender = Address::new([sender_byte; 32]);
        let tx = Transaction {
            chain_id: "polay-testnet-1".into(),
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
        hash_bytes[0] = hash_seed;
        hash_bytes[1] = sender_byte;
        hash_bytes[2] = (nonce & 0xFF) as u8;
        hash_bytes[3] = ((nonce >> 8) & 0xFF) as u8;

        SignedTransaction::new(tx, Signature::ZERO, Hash::new(hash_bytes), vec![0u8; 32])
    }

    fn default_pool() -> Mempool {
        Mempool::new(MempoolConfig {
            verify_signature: false,
            ..MempoolConfig::default()
        })
    }

    fn small_pool() -> Mempool {
        Mempool::new(MempoolConfig {
            max_size: 5,
            max_per_account: 3,
            min_fee: 500,
            verify_signature: false,
            ..MempoolConfig::default()
        })
    }

    // -- insert -----------------------------------------------------------

    #[test]
    fn insert_single_transaction() {
        let pool = default_pool();
        let tx = make_tx(0xAA, 0, 5_000, 1);
        assert!(pool.insert(tx).is_ok());
        assert_eq!(pool.size(), 1);
    }

    #[test]
    fn insert_duplicate_is_rejected() {
        let pool = default_pool();
        let tx = make_tx(0xAA, 0, 5_000, 1);
        pool.insert(tx.clone()).unwrap();
        let result = pool.insert(tx);
        assert_eq!(result, Err(MempoolError::TransactionAlreadyExists));
    }

    #[test]
    fn insert_when_full_is_rejected() {
        let pool = small_pool();
        for i in 0..5u8 {
            // Different senders so per-account limit is not hit.
            let tx = make_tx(i, 0, 1_000, i);
            pool.insert(tx).unwrap();
        }
        assert_eq!(pool.size(), 5);

        let tx = make_tx(0xFF, 0, 1_000, 0xFF);
        assert_eq!(pool.insert(tx), Err(MempoolError::MempoolFull));
    }

    #[test]
    fn insert_per_account_limit_is_enforced() {
        let pool = small_pool(); // max_per_account = 3
        for nonce in 0..3u64 {
            let tx = make_tx(0xAA, nonce, 1_000, nonce as u8);
            pool.insert(tx).unwrap();
        }
        let tx = make_tx(0xAA, 3, 1_000, 3);
        assert_eq!(pool.insert(tx), Err(MempoolError::MempoolFull));
    }

    #[test]
    fn insert_fee_too_low_is_rejected() {
        let pool = small_pool(); // min_fee = 500
        let tx = make_tx(0xAA, 0, 100, 1);
        assert_eq!(
            pool.insert(tx),
            Err(MempoolError::FeeTooLow {
                minimum: 500,
                got: 100,
            })
        );
    }

    // -- remove -----------------------------------------------------------

    #[test]
    fn remove_existing_transaction() {
        let pool = default_pool();
        let tx = make_tx(0xAA, 0, 5_000, 1);
        let hash = tx.tx_hash;
        pool.insert(tx.clone()).unwrap();

        let removed = pool.remove(&hash);
        assert_eq!(removed.as_ref().map(|t| t.tx_hash), Some(tx.tx_hash));
        assert_eq!(pool.size(), 0);
        assert!(!pool.contains(&hash));
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let pool = default_pool();
        assert!(pool.remove(&Hash::ZERO).is_none());
    }

    #[test]
    fn remove_cleans_sender_index() {
        let pool = default_pool();
        let sender = Address::new([0xAA; 32]);
        let tx = make_tx(0xAA, 0, 5_000, 1);
        let hash = tx.tx_hash;
        pool.insert(tx).unwrap();
        pool.remove(&hash);

        // The sender should have no entries left.
        assert!(pool.get_account_txs(&sender).is_empty());
    }

    // -- remove_batch -----------------------------------------------------

    #[test]
    fn remove_batch_removes_all() {
        let pool = default_pool();
        let tx1 = make_tx(0xAA, 0, 5_000, 1);
        let tx2 = make_tx(0xBB, 0, 5_000, 2);
        let h1 = tx1.tx_hash;
        let h2 = tx2.tx_hash;
        pool.insert(tx1).unwrap();
        pool.insert(tx2).unwrap();

        pool.remove_batch(&[h1, h2]);
        assert_eq!(pool.size(), 0);
    }

    // -- get / contains ---------------------------------------------------

    #[test]
    fn get_returns_transaction() {
        let pool = default_pool();
        let tx = make_tx(0xAA, 0, 5_000, 1);
        let hash = tx.tx_hash;
        pool.insert(tx.clone()).unwrap();

        let fetched = pool.get(&hash).unwrap();
        assert_eq!(fetched.tx_hash, tx.tx_hash);
    }

    #[test]
    fn get_missing_returns_none() {
        let pool = default_pool();
        assert!(pool.get(&Hash::ZERO).is_none());
    }

    #[test]
    fn contains_works() {
        let pool = default_pool();
        let tx = make_tx(0xAA, 0, 5_000, 1);
        let hash = tx.tx_hash;
        pool.insert(tx).unwrap();

        assert!(pool.contains(&hash));
        assert!(!pool.contains(&Hash::ZERO));
    }

    // -- get_pending_for_block --------------------------------------------

    #[test]
    fn pending_for_block_ordered_by_fee_desc() {
        let pool = default_pool();
        pool.insert(make_tx(0x01, 0, 1_000, 1)).unwrap();
        pool.insert(make_tx(0x02, 0, 9_000, 2)).unwrap();
        pool.insert(make_tx(0x03, 0, 5_000, 3)).unwrap();

        let block_txs = pool.get_pending_for_block(10);
        assert_eq!(block_txs.len(), 3);
        assert_eq!(block_txs[0].transaction.max_fee, 9_000);
        assert_eq!(block_txs[1].transaction.max_fee, 5_000);
        assert_eq!(block_txs[2].transaction.max_fee, 1_000);
    }

    #[test]
    fn pending_for_block_respects_limit() {
        let pool = default_pool();
        for i in 0..10u8 {
            pool.insert(make_tx(i, 0, 1_000 + (i as u64) * 100, i))
                .unwrap();
        }
        let block_txs = pool.get_pending_for_block(3);
        assert_eq!(block_txs.len(), 3);
        // The three highest fees should be picked.
        assert_eq!(block_txs[0].transaction.max_fee, 1_900);
        assert_eq!(block_txs[1].transaction.max_fee, 1_800);
        assert_eq!(block_txs[2].transaction.max_fee, 1_700);
    }

    #[test]
    fn pending_for_block_empty_pool() {
        let pool = default_pool();
        let block_txs = pool.get_pending_for_block(100);
        assert!(block_txs.is_empty());
    }

    // -- get_account_txs --------------------------------------------------

    #[test]
    fn account_txs_ordered_by_nonce() {
        let pool = default_pool();
        // Insert out of order.
        pool.insert(make_tx(0xAA, 5, 5_000, 1)).unwrap();
        pool.insert(make_tx(0xAA, 2, 5_000, 2)).unwrap();
        pool.insert(make_tx(0xAA, 8, 5_000, 3)).unwrap();

        let sender = Address::new([0xAA; 32]);
        let txs = pool.get_account_txs(&sender);
        assert_eq!(txs.len(), 3);
        assert_eq!(txs[0].nonce(), 2);
        assert_eq!(txs[1].nonce(), 5);
        assert_eq!(txs[2].nonce(), 8);
    }

    #[test]
    fn account_txs_unknown_sender_returns_empty() {
        let pool = default_pool();
        let unknown = Address::new([0xFF; 32]);
        assert!(pool.get_account_txs(&unknown).is_empty());
    }

    // -- size / clear -----------------------------------------------------

    #[test]
    fn size_reflects_pool_state() {
        let pool = default_pool();
        assert_eq!(pool.size(), 0);

        pool.insert(make_tx(0xAA, 0, 5_000, 1)).unwrap();
        assert_eq!(pool.size(), 1);

        pool.insert(make_tx(0xBB, 0, 5_000, 2)).unwrap();
        assert_eq!(pool.size(), 2);
    }

    #[test]
    fn clear_empties_pool() {
        let pool = default_pool();
        for i in 0..5u8 {
            pool.insert(make_tx(i, 0, 5_000, i)).unwrap();
        }
        assert_eq!(pool.size(), 5);

        pool.clear();
        assert_eq!(pool.size(), 0);
    }

    // -- prune_below_nonce ------------------------------------------------

    #[test]
    fn prune_removes_stale_nonces() {
        let pool = default_pool();
        let sender = Address::new([0xAA; 32]);

        pool.insert(make_tx(0xAA, 0, 5_000, 10)).unwrap();
        pool.insert(make_tx(0xAA, 1, 5_000, 11)).unwrap();
        pool.insert(make_tx(0xAA, 2, 5_000, 12)).unwrap();
        pool.insert(make_tx(0xAA, 5, 5_000, 15)).unwrap();
        assert_eq!(pool.size(), 4);

        // Prune nonces < 2 -> removes nonces 0 and 1.
        pool.prune_below_nonce(&sender, 2);
        assert_eq!(pool.size(), 2);

        let remaining = pool.get_account_txs(&sender);
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].nonce(), 2);
        assert_eq!(remaining[1].nonce(), 5);
    }

    #[test]
    fn prune_all_removes_sender_entry() {
        let pool = default_pool();
        let sender = Address::new([0xAA; 32]);

        pool.insert(make_tx(0xAA, 0, 5_000, 1)).unwrap();
        pool.insert(make_tx(0xAA, 1, 5_000, 2)).unwrap();

        // Prune everything.
        pool.prune_below_nonce(&sender, 100);
        assert_eq!(pool.size(), 0);
        assert!(pool.get_account_txs(&sender).is_empty());
    }

    #[test]
    fn prune_unknown_sender_is_noop() {
        let pool = default_pool();
        let unknown = Address::new([0xFF; 32]);
        pool.prune_below_nonce(&unknown, 10); // should not panic
        assert_eq!(pool.size(), 0);
    }

    #[test]
    fn prune_does_not_affect_other_senders() {
        let pool = default_pool();
        let sender_a = Address::new([0xAA; 32]);
        let sender_b = Address::new([0xBB; 32]);

        pool.insert(make_tx(0xAA, 0, 5_000, 1)).unwrap();
        pool.insert(make_tx(0xAA, 1, 5_000, 2)).unwrap();
        pool.insert(make_tx(0xBB, 0, 5_000, 3)).unwrap();

        pool.prune_below_nonce(&sender_a, 10);

        assert_eq!(pool.size(), 1);
        assert!(pool.get_account_txs(&sender_a).is_empty());
        assert_eq!(pool.get_account_txs(&sender_b).len(), 1);
    }

    // -- concurrent access (basic smoke) ----------------------------------

    #[test]
    fn concurrent_inserts() {
        use std::sync::Arc;

        let pool = Arc::new(Mempool::new(MempoolConfig {
            max_size: 1_000,
            max_per_account: 100,
            min_fee: 100,
            verify_signature: false,
            ..MempoolConfig::default()
        }));

        let handles: Vec<_> = (0..8u8)
            .map(|thread_id| {
                let pool = Arc::clone(&pool);
                std::thread::spawn(move || {
                    for nonce in 0..10u64 {
                        let hash_seed = thread_id.wrapping_mul(50).wrapping_add(nonce as u8);
                        let tx = make_tx(thread_id, nonce, 1_000, hash_seed);
                        let _ = pool.insert(tx);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // 8 threads x 10 transactions each.
        assert_eq!(pool.size(), 80);
    }

    // -- edge cases -------------------------------------------------------

    #[test]
    fn insert_at_exact_fee_boundary() {
        let pool = small_pool(); // min_fee = 500
        let tx = make_tx(0xAA, 0, 500, 1);
        assert!(pool.insert(tx).is_ok());
    }

    #[test]
    fn insert_one_below_fee_boundary() {
        let pool = small_pool(); // min_fee = 500
        let tx = make_tx(0xAA, 0, 499, 1);
        assert_eq!(
            pool.insert(tx),
            Err(MempoolError::FeeTooLow {
                minimum: 500,
                got: 499,
            })
        );
    }

    #[test]
    fn remove_then_reinsert() {
        let pool = default_pool();
        let tx = make_tx(0xAA, 0, 5_000, 1);
        let hash = tx.tx_hash;

        pool.insert(tx.clone()).unwrap();
        pool.remove(&hash);
        assert_eq!(pool.size(), 0);

        // Should be able to insert again.
        assert!(pool.insert(tx).is_ok());
        assert_eq!(pool.size(), 1);
    }

    // -- Nonce gap detection ------------------------------------------------

    #[test]
    fn nonce_gap_within_limit_accepted() {
        let pool = Mempool::new(MempoolConfig {
            max_nonce_gap: 5,
            verify_signature: false,
            ..MempoolConfig::default()
        });
        // Insert nonce 0 first.
        pool.insert(make_tx(0xAA, 0, 5_000, 1)).unwrap();
        // Nonce 5 is within the gap of 5.
        pool.insert(make_tx(0xAA, 5, 5_000, 2)).unwrap();
        assert_eq!(pool.size(), 2);
    }

    #[test]
    fn nonce_gap_exceeds_limit_rejected() {
        let pool = Mempool::new(MempoolConfig {
            max_nonce_gap: 3,
            verify_signature: false,
            ..MempoolConfig::default()
        });
        pool.insert(make_tx(0xAA, 0, 5_000, 1)).unwrap();
        // Nonce 10 is 10 ahead of lowest (0), gap = 10 > 3.
        let result = pool.insert(make_tx(0xAA, 10, 5_000, 2));
        assert!(matches!(result, Err(MempoolError::NonceGapTooLarge { .. })));
    }

    #[test]
    fn nonce_gap_no_existing_txs_accepted() {
        // When there are no existing txs for a sender, any nonce is fine.
        let pool = Mempool::new(MempoolConfig {
            max_nonce_gap: 3,
            verify_signature: false,
            ..MempoolConfig::default()
        });
        pool.insert(make_tx(0xAA, 100, 5_000, 1)).unwrap();
        assert_eq!(pool.size(), 1);
    }

    // -- Chain ID check -----------------------------------------------------

    #[test]
    fn chain_id_mismatch_rejected() {
        let pool = Mempool::new(MempoolConfig {
            chain_id: "polay-mainnet".into(),
            verify_signature: false,
            ..MempoolConfig::default()
        });
        // make_tx creates txs with chain_id "polay-testnet-1"
        let result = pool.insert(make_tx(0xAA, 0, 5_000, 1));
        assert!(matches!(result, Err(MempoolError::ChainIdMismatch { .. })));
    }

    #[test]
    fn chain_id_empty_skips_check() {
        let pool = Mempool::new(MempoolConfig {
            chain_id: "".into(),
            verify_signature: false,
            ..MempoolConfig::default()
        });
        assert!(pool.insert(make_tx(0xAA, 0, 5_000, 1)).is_ok());
    }

    // -- Duplicate / recently seen ------------------------------------------

    #[test]
    fn recently_seen_tx_rejected() {
        let pool = default_pool();
        let tx = make_tx(0xAA, 0, 5_000, 1);
        let hash = tx.tx_hash;

        // Mark it as seen before insertion.
        pool.mark_seen(hash);
        let result = pool.insert(tx);
        assert_eq!(result, Err(MempoolError::DuplicateTransaction));
    }

    #[test]
    fn clear_recently_seen_allows_resubmission() {
        let pool = default_pool();
        let tx = make_tx(0xAA, 0, 5_000, 1);
        let hash = tx.tx_hash;

        pool.mark_seen(hash);
        assert_eq!(pool.recently_seen_count(), 1);

        pool.clear_recently_seen();
        assert_eq!(pool.recently_seen_count(), 0);

        // Now it should be insertable.
        assert!(pool.insert(tx).is_ok());
    }

    // -- Low-fee eviction ---------------------------------------------------

    #[test]
    fn eviction_when_full_higher_fee_replaces_lower() {
        let pool = Mempool::new(MempoolConfig {
            max_size: 3,
            max_per_account: 100,
            min_fee: 100,
            verify_signature: false,
            ..MempoolConfig::default()
        });

        // Fill the pool with fee 1000 each.
        pool.insert(make_tx(0x01, 0, 1_000, 1)).unwrap();
        pool.insert(make_tx(0x02, 0, 2_000, 2)).unwrap();
        pool.insert(make_tx(0x03, 0, 3_000, 3)).unwrap();
        assert_eq!(pool.size(), 3);

        // Insert a tx with fee 5000 -- should evict the fee-1000 tx.
        pool.insert(make_tx(0x04, 0, 5_000, 4)).unwrap();
        assert_eq!(pool.size(), 3);

        // The lowest-fee tx (1000) should be gone.
        let block_txs = pool.get_pending_for_block(10);
        let fees: Vec<u64> = block_txs.iter().map(|t| t.transaction.max_fee).collect();
        assert!(!fees.contains(&1_000));
        assert!(fees.contains(&5_000));
    }

    #[test]
    fn eviction_when_full_lower_fee_rejected() {
        let pool = Mempool::new(MempoolConfig {
            max_size: 2,
            max_per_account: 100,
            min_fee: 100,
            verify_signature: false,
            ..MempoolConfig::default()
        });

        pool.insert(make_tx(0x01, 0, 5_000, 1)).unwrap();
        pool.insert(make_tx(0x02, 0, 5_000, 2)).unwrap();
        assert_eq!(pool.size(), 2);

        // Try to insert a tx with lower fee -- should be rejected.
        let result = pool.insert(make_tx(0x03, 0, 100, 3));
        assert_eq!(result, Err(MempoolError::MempoolFull));
    }
}
