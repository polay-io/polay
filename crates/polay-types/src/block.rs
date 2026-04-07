use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::hash::Hash;
use crate::transaction::SignedTransaction;

/// The header of a block in the POLAY blockchain.
///
/// The `hash` field is set externally after computing the digest over the
/// borsh-encoded header bytes (with `hash` set to `Hash::ZERO` during
/// computation).
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct BlockHeader {
    /// Sequential block number.
    pub height: u64,
    /// Unix timestamp (seconds) when the block was proposed.
    pub timestamp: u64,
    /// Hash of the parent block. `Hash::ZERO` for the genesis block.
    pub parent_hash: Hash,
    /// Merkle root of the post-execution state trie.
    pub state_root: Hash,
    /// Merkle root of the transactions included in this block.
    pub transactions_root: Hash,
    /// Address of the validator that proposed this block.
    pub proposer: Address,
    /// Chain identifier (must match the chain's configuration).
    pub chain_id: String,
    /// The block's own hash. Set to `Hash::ZERO` while computing the hash,
    /// then filled in afterward.
    pub hash: Hash,
}

impl BlockHeader {
    /// Produce the canonical bytes used as input to the hash function.
    ///
    /// The `hash` field is temporarily zeroed so that the digest is computed
    /// over a deterministic payload regardless of its prior value.
    pub fn hash_input_bytes(&self) -> Vec<u8> {
        let mut copy = self.clone();
        copy.hash = Hash::ZERO;
        borsh::to_vec(&copy).expect("borsh serialization of BlockHeader should not fail")
    }

    /// Compute and set the block hash using the provided hash function.
    ///
    /// The closure receives the canonical borsh bytes and must return a
    /// 32-byte digest. This keeps `polay-types` free of a direct `sha2`
    /// dependency.
    ///
    /// ```ignore
    /// use sha2::{Sha256, Digest};
    /// header.compute_hash(|bytes| {
    ///     let digest = Sha256::digest(bytes);
    ///     let mut out = [0u8; 32];
    ///     out.copy_from_slice(&digest);
    ///     out
    /// });
    /// ```
    pub fn compute_hash(&mut self, hasher: impl FnOnce(&[u8]) -> [u8; 32]) {
        let input = self.hash_input_bytes();
        self.hash = Hash::new(hasher(&input));
    }

    /// Returns `true` if this is the genesis block (height 0, parent is zero).
    pub fn is_genesis(&self) -> bool {
        self.height == 0 && self.parent_hash.is_zero()
    }
}

/// A complete block: header plus the ordered list of signed transactions.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct Block {
    /// The block header.
    pub header: BlockHeader,
    /// Transactions included in this block, in execution order.
    pub transactions: Vec<SignedTransaction>,
}

impl Block {
    /// Create a new block.
    pub fn new(header: BlockHeader, transactions: Vec<SignedTransaction>) -> Self {
        Self {
            header,
            transactions,
        }
    }

    /// Convenience: the block height.
    pub fn height(&self) -> u64 {
        self.header.height
    }

    /// Convenience: the block hash.
    pub fn hash(&self) -> &Hash {
        &self.header.hash
    }

    /// Number of transactions in the block.
    pub fn tx_count(&self) -> usize {
        self.transactions.len()
    }

    /// Returns `true` if this is the genesis block.
    pub fn is_genesis(&self) -> bool {
        self.header.is_genesis()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> BlockHeader {
        BlockHeader {
            height: 1,
            timestamp: 1700000000,
            parent_hash: Hash::new([0xAA; 32]),
            state_root: Hash::new([0xBB; 32]),
            transactions_root: Hash::new([0xCC; 32]),
            proposer: Address::new([0x01; 32]),
            chain_id: "polay-testnet-1".into(),
            hash: Hash::ZERO,
        }
    }

    #[test]
    fn hash_input_bytes_deterministic() {
        let header = sample_header();
        let a = header.hash_input_bytes();
        let b = header.hash_input_bytes();
        assert_eq!(a, b);
    }

    #[test]
    fn hash_input_bytes_ignores_hash_field() {
        let mut h1 = sample_header();
        let mut h2 = sample_header();
        h2.hash = Hash::new([0xFF; 32]);
        // Both should produce the same input bytes because `hash` is zeroed.
        assert_eq!(h1.hash_input_bytes(), h2.hash_input_bytes());

        // After compute_hash they should also match.
        let simple_hasher = |bytes: &[u8]| {
            // Trivial "hash" for testing: XOR-fold all bytes.
            let mut out = [0u8; 32];
            for (i, &b) in bytes.iter().enumerate() {
                out[i % 32] ^= b;
            }
            out
        };
        h1.compute_hash(simple_hasher);
        h2.compute_hash(simple_hasher);
        assert_eq!(h1.hash, h2.hash);
        assert_ne!(h1.hash, Hash::ZERO);
    }

    #[test]
    fn genesis_detection() {
        let mut header = sample_header();
        assert!(!header.is_genesis());

        header.height = 0;
        header.parent_hash = Hash::ZERO;
        assert!(header.is_genesis());
    }

    #[test]
    fn block_convenience_methods() {
        let block = Block::new(sample_header(), vec![]);
        assert_eq!(block.height(), 1);
        assert_eq!(block.tx_count(), 0);
        assert!(!block.is_genesis());
    }

    #[test]
    fn serde_round_trip_header() {
        let header = sample_header();
        let json = serde_json::to_string(&header).unwrap();
        let parsed: BlockHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(header, parsed);
    }

    #[test]
    fn borsh_round_trip_header() {
        let header = sample_header();
        let encoded = borsh::to_vec(&header).unwrap();
        let decoded = BlockHeader::try_from_slice(&encoded).unwrap();
        assert_eq!(header, decoded);
    }

    #[test]
    fn serde_round_trip_block() {
        let block = Block::new(sample_header(), vec![]);
        let json = serde_json::to_string(&block).unwrap();
        let parsed: Block = serde_json::from_str(&json).unwrap();
        assert_eq!(block, parsed);
    }

    #[test]
    fn borsh_round_trip_block() {
        let block = Block::new(sample_header(), vec![]);
        let encoded = borsh::to_vec(&block).unwrap();
        let decoded = Block::try_from_slice(&encoded).unwrap();
        assert_eq!(block, decoded);
    }
}
