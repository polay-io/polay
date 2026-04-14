//! Merkle tree state commitments.
//!
//! Implements a sorted Merkle tree over all state key-value pairs to produce
//! a deterministic cryptographic commitment (state root) for each block.
//! Given the same state, any node will compute the identical root hash.

use sha2::{Digest, Sha256};

use polay_types::Hash;

use crate::error::StateResult;
use crate::keys;
use crate::store::StateStore;

// ---------------------------------------------------------------------------
// StateCommitment
// ---------------------------------------------------------------------------

/// The result of computing the Merkle root over the entire chain state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateCommitment {
    /// The Merkle root hash of the entire state.
    pub root: Hash,
    /// Number of leaf entries in the state tree.
    pub entry_count: u64,
}

// ---------------------------------------------------------------------------
// Side (proof direction)
// ---------------------------------------------------------------------------

/// Indicates which side of the parent node a sibling hash belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The sibling is on the left; the current node is on the right.
    Left,
    /// The sibling is on the right; the current node is on the left.
    Right,
}

// ---------------------------------------------------------------------------
// MerkleProof
// ---------------------------------------------------------------------------

/// An inclusion proof for a single leaf within the Merkle tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// The leaf hash that this proof covers.
    pub leaf: Hash,
    /// Sibling hashes from the leaf up to the root, together with their side.
    pub siblings: Vec<(Hash, Side)>,
    /// The expected Merkle root.
    pub root: Hash,
}

// ---------------------------------------------------------------------------
// MerkleTree
// ---------------------------------------------------------------------------

/// A sorted Merkle tree builder.
///
/// Leaves are inserted as `(key, value)` pairs.  Before computing the root
/// the leaves are sorted lexicographically (by their hash) to guarantee
/// deterministic ordering regardless of insertion order.
pub struct MerkleTree {
    leaves: Vec<Hash>,
}

impl MerkleTree {
    /// Create a new, empty Merkle tree.
    pub fn new() -> Self {
        Self { leaves: Vec::new() }
    }

    /// Add a leaf computed as `sha256(key || value)`.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(value);
        let digest: [u8; 32] = hasher.finalize().into();
        self.leaves.push(Hash::new(digest));
    }

    /// Compute the Merkle root.
    ///
    /// The leaves are sorted lexicographically before building the tree so
    /// that the root is deterministic regardless of insertion order.
    ///
    /// - An empty tree returns `Hash::ZERO`.
    /// - Odd layers duplicate the last node before hashing upward.
    pub fn root(&self) -> Hash {
        if self.leaves.is_empty() {
            return Hash::ZERO;
        }

        let mut layer = self.sorted_leaves();

        while layer.len() > 1 {
            layer = Self::hash_layer(&layer);
        }

        layer[0]
    }

    /// Return the number of leaves inserted so far.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Return `true` if the tree contains no leaves.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Generate a Merkle inclusion proof for the leaf at `index` in the
    /// **sorted** leaf list.
    ///
    /// Returns `None` if the index is out of bounds.
    pub fn proof(&self, index: usize) -> Option<MerkleProof> {
        if index >= self.leaves.len() {
            return None;
        }

        let sorted = self.sorted_leaves();
        let leaf = sorted[index];
        let mut siblings = Vec::new();
        let mut layer = sorted;
        let mut idx = index;

        while layer.len() > 1 {
            // Duplicate the last element if the layer is odd-length.
            if !layer.len().is_multiple_of(2) {
                let last = *layer.last().unwrap();
                layer.push(last);
            }

            // Determine sibling.
            let sibling_idx = if idx.is_multiple_of(2) {
                idx + 1
            } else {
                idx - 1
            };
            let side = if idx.is_multiple_of(2) {
                Side::Right
            } else {
                Side::Left
            };
            siblings.push((layer[sibling_idx], side));

            // Move to the next layer.
            let mut next = Vec::with_capacity(layer.len() / 2);
            for pair in layer.chunks(2) {
                next.push(hash_pair(&pair[0], &pair[1]));
            }
            layer = next;
            idx /= 2;
        }

        let root = layer[0];
        Some(MerkleProof {
            leaf,
            siblings,
            root,
        })
    }

    /// Verify a Merkle inclusion proof.
    ///
    /// Recomputes the root from `leaf` and the sibling chain and checks
    /// whether it matches `root`.
    pub fn verify_proof(root: &Hash, leaf: &Hash, proof: &MerkleProof) -> bool {
        let mut current = *leaf;
        for (sibling, side) in &proof.siblings {
            current = match side {
                Side::Left => hash_pair(sibling, &current),
                Side::Right => hash_pair(&current, sibling),
            };
        }
        current == *root
    }

    // -- private helpers -----------------------------------------------------

    /// Return the leaves sorted lexicographically by their hash bytes.
    fn sorted_leaves(&self) -> Vec<Hash> {
        let mut sorted = self.leaves.clone();
        sorted.sort();
        sorted
    }

    /// Hash adjacent pairs in a layer to produce the parent layer.
    /// If the layer has an odd number of elements the last element is
    /// duplicated before hashing.
    fn hash_layer(layer: &[Hash]) -> Vec<Hash> {
        let mut padded;
        let input = if !layer.len().is_multiple_of(2) {
            padded = layer.to_vec();
            padded.push(*layer.last().unwrap());
            &padded
        } else {
            // Work around borrow-checker: create a local binding.
            layer
        };

        let mut next = Vec::with_capacity(input.len().div_ceil(2));
        for pair in input.chunks(2) {
            next.push(hash_pair(&pair[0], &pair[1]));
        }
        next
    }
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// SHA-256 hash of `left || right`.
fn hash_pair(left: &Hash, right: &Hash) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(left.as_bytes());
    hasher.update(right.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    Hash::new(digest)
}

// ---------------------------------------------------------------------------
// compute_state_root
// ---------------------------------------------------------------------------

/// All state prefixes that contribute to the state commitment.
const STATE_PREFIXES: [u8; 12] = [
    keys::PREFIX_ACCOUNT,
    keys::PREFIX_BALANCE,
    keys::PREFIX_ASSET_CLASS,
    keys::PREFIX_ASSET_BALANCE,
    keys::PREFIX_VALIDATOR,
    keys::PREFIX_DELEGATION,
    keys::PREFIX_LISTING,
    keys::PREFIX_PROFILE,
    keys::PREFIX_ACHIEVEMENT,
    keys::PREFIX_ATTESTOR,
    keys::PREFIX_MATCH_RESULT,
    keys::PREFIX_MATCH_SETTLEMENT,
];

/// Compute the Merkle root over all state entries in the store.
///
/// The resulting [`StateCommitment`] contains a deterministic hash that
/// uniquely identifies the entire chain state.  Two stores with identical
/// contents will always produce the same root.
pub fn compute_state_root(store: &dyn StateStore) -> StateResult<StateCommitment> {
    let mut tree = MerkleTree::new();

    for prefix in STATE_PREFIXES {
        let entries = store.prefix_scan(&[prefix])?;
        for (key, value) in entries {
            tree.insert(&key, &value);
        }
    }

    Ok(StateCommitment {
        root: tree.root(),
        entry_count: tree.len() as u64,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;

    // -- MerkleTree unit tests -----------------------------------------------

    #[test]
    fn empty_tree_root_is_zero() {
        let tree = MerkleTree::new();
        assert_eq!(tree.root(), Hash::ZERO);
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
    }

    #[test]
    fn single_entry_tree() {
        let mut tree = MerkleTree::new();
        tree.insert(b"key1", b"value1");
        let root = tree.root();
        assert_ne!(root, Hash::ZERO);
        assert_eq!(tree.len(), 1);

        // Same input produces same root.
        let mut tree2 = MerkleTree::new();
        tree2.insert(b"key1", b"value1");
        assert_eq!(tree2.root(), root);
    }

    #[test]
    fn multiple_entries_deterministic() {
        let mut tree_a = MerkleTree::new();
        tree_a.insert(b"key1", b"value1");
        tree_a.insert(b"key2", b"value2");
        tree_a.insert(b"key3", b"value3");

        // Insert in different order.
        let mut tree_b = MerkleTree::new();
        tree_b.insert(b"key3", b"value3");
        tree_b.insert(b"key1", b"value1");
        tree_b.insert(b"key2", b"value2");

        assert_eq!(tree_a.root(), tree_b.root());
        assert_ne!(tree_a.root(), Hash::ZERO);
    }

    #[test]
    fn different_values_produce_different_roots() {
        let mut tree_a = MerkleTree::new();
        tree_a.insert(b"key1", b"value1");

        let mut tree_b = MerkleTree::new();
        tree_b.insert(b"key1", b"value2");

        assert_ne!(tree_a.root(), tree_b.root());
    }

    #[test]
    fn odd_number_of_leaves() {
        let mut tree = MerkleTree::new();
        tree.insert(b"a", b"1");
        tree.insert(b"b", b"2");
        tree.insert(b"c", b"3");

        let root = tree.root();
        assert_ne!(root, Hash::ZERO);

        // Deterministic check.
        let mut tree2 = MerkleTree::new();
        tree2.insert(b"c", b"3");
        tree2.insert(b"a", b"1");
        tree2.insert(b"b", b"2");
        assert_eq!(tree2.root(), root);
    }

    #[test]
    fn proof_generation_and_verification() {
        let mut tree = MerkleTree::new();
        tree.insert(b"alpha", b"100");
        tree.insert(b"beta", b"200");
        tree.insert(b"gamma", b"300");
        tree.insert(b"delta", b"400");

        let root = tree.root();

        for i in 0..tree.len() {
            let proof = tree.proof(i).expect("proof should exist");
            assert_eq!(proof.root, root);
            assert!(
                MerkleTree::verify_proof(&root, &proof.leaf, &proof),
                "proof for leaf {i} should verify"
            );
        }
    }

    #[test]
    fn proof_fails_for_wrong_leaf() {
        let mut tree = MerkleTree::new();
        tree.insert(b"key1", b"val1");
        tree.insert(b"key2", b"val2");

        let root = tree.root();
        let proof = tree.proof(0).unwrap();

        // Tamper with the leaf.
        let fake_leaf = Hash::new([0xAB; 32]);
        assert!(!MerkleTree::verify_proof(&root, &fake_leaf, &proof));
    }

    #[test]
    fn proof_fails_for_wrong_root() {
        let mut tree = MerkleTree::new();
        tree.insert(b"key1", b"val1");
        tree.insert(b"key2", b"val2");

        let proof = tree.proof(0).unwrap();
        let fake_root = Hash::new([0xCD; 32]);
        assert!(!MerkleTree::verify_proof(&fake_root, &proof.leaf, &proof));
    }

    #[test]
    fn proof_out_of_bounds_returns_none() {
        let mut tree = MerkleTree::new();
        tree.insert(b"key1", b"val1");
        assert!(tree.proof(1).is_none());
        assert!(tree.proof(100).is_none());
    }

    #[test]
    fn proof_single_leaf() {
        let mut tree = MerkleTree::new();
        tree.insert(b"only", b"leaf");

        let root = tree.root();
        let proof = tree.proof(0).unwrap();
        assert_eq!(proof.siblings.len(), 0);
        assert!(MerkleTree::verify_proof(&root, &proof.leaf, &proof));
    }

    // -- compute_state_root tests --------------------------------------------

    #[test]
    fn empty_store_state_root() {
        let store = MemoryStore::new();
        let commitment = compute_state_root(&store).unwrap();
        assert_eq!(commitment.root, Hash::ZERO);
        assert_eq!(commitment.entry_count, 0);
    }

    #[test]
    fn state_root_changes_when_state_changes() {
        let store = MemoryStore::new();
        let root_empty = compute_state_root(&store).unwrap();

        // Add an account balance.
        store
            .put_raw(
                &keys::balance_key(&polay_types::Address::new([1u8; 32])),
                b"some-value",
            )
            .unwrap();
        let root_one = compute_state_root(&store).unwrap();

        assert_ne!(root_empty.root, root_one.root);
        assert_eq!(root_one.entry_count, 1);

        // Add another entry.
        store
            .put_raw(
                &keys::balance_key(&polay_types::Address::new([2u8; 32])),
                b"other",
            )
            .unwrap();
        let root_two = compute_state_root(&store).unwrap();

        assert_ne!(root_one.root, root_two.root);
        assert_eq!(root_two.entry_count, 2);
    }

    #[test]
    fn state_root_is_deterministic_across_stores() {
        let store_a = MemoryStore::new();
        let store_b = MemoryStore::new();

        let addr = polay_types::Address::new([0x42; 32]);
        let key = keys::balance_key(&addr);
        let value = b"12345";

        store_a.put_raw(&key, value).unwrap();
        store_b.put_raw(&key, value).unwrap();

        let root_a = compute_state_root(&store_a).unwrap();
        let root_b = compute_state_root(&store_b).unwrap();
        assert_eq!(root_a, root_b);
    }

    #[test]
    fn state_root_ignores_chain_meta_and_blocks() {
        let store = MemoryStore::new();

        // Add chain metadata (should NOT affect state root).
        store.put_raw(&keys::chain_height_key(), b"1").unwrap();
        store.put_raw(&keys::latest_hash_key(), &[0u8; 32]).unwrap();
        store.put_raw(&keys::block_key(1), b"block-data").unwrap();

        let commitment = compute_state_root(&store).unwrap();
        assert_eq!(commitment.root, Hash::ZERO);
        assert_eq!(commitment.entry_count, 0);
    }

    // -- prefix_scan tests ---------------------------------------------------

    #[test]
    fn memory_store_prefix_scan_returns_matching_keys() {
        let store = MemoryStore::new();
        let addr1 = polay_types::Address::new([1u8; 32]);
        let addr2 = polay_types::Address::new([2u8; 32]);

        store.put_raw(&keys::balance_key(&addr1), b"100").unwrap();
        store.put_raw(&keys::balance_key(&addr2), b"200").unwrap();
        store.put_raw(&keys::account_key(&addr1), b"acct").unwrap();

        let results = store.prefix_scan(&[keys::PREFIX_BALANCE]).unwrap();
        assert_eq!(results.len(), 2);

        // All results should have the balance prefix.
        for (key, _) in &results {
            assert_eq!(key[0], keys::PREFIX_BALANCE);
        }
    }

    #[test]
    fn memory_store_prefix_scan_empty_prefix() {
        let store = MemoryStore::new();
        store.put_raw(b"\x99key", b"val").unwrap();

        let results = store.prefix_scan(&[0x99]).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn rocksdb_store_prefix_scan() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::store::RocksDbStore::new(dir.path().to_str().unwrap()).unwrap();

        let addr1 = polay_types::Address::new([1u8; 32]);
        let addr2 = polay_types::Address::new([2u8; 32]);

        store.put_raw(&keys::balance_key(&addr1), b"100").unwrap();
        store.put_raw(&keys::balance_key(&addr2), b"200").unwrap();
        store.put_raw(&keys::account_key(&addr1), b"acct").unwrap();

        let results = store.prefix_scan(&[keys::PREFIX_BALANCE]).unwrap();
        assert_eq!(results.len(), 2);

        for (key, _) in &results {
            assert_eq!(key[0], keys::PREFIX_BALANCE);
        }
    }

    #[test]
    fn rocksdb_state_root_matches_memory_store() {
        let dir = tempfile::tempdir().unwrap();
        let rocks = crate::store::RocksDbStore::new(dir.path().to_str().unwrap()).unwrap();
        let mem = MemoryStore::new();

        let addr = polay_types::Address::new([0x42; 32]);
        let key = keys::balance_key(&addr);
        let value = b"12345";

        rocks.put_raw(&key, value).unwrap();
        mem.put_raw(&key, value).unwrap();

        let root_rocks = compute_state_root(&rocks).unwrap();
        let root_mem = compute_state_root(&mem).unwrap();
        assert_eq!(root_rocks, root_mem);
    }
}
