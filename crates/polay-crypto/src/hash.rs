use sha2::{Digest, Sha256};

use polay_types::{BlockHeader, Hash, Transaction};

use crate::error::{CryptoError, CryptoResult};

/// Compute the SHA-256 hash of arbitrary data.
pub fn sha256(data: &[u8]) -> Hash {
    let digest = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Hash::new(out)
}

// Domain-separation prefixes for different hash contexts.
const DOMAIN_TX: &[u8] = b"POLAY-TX:";
const DOMAIN_BLOCK: &[u8] = b"POLAY-BLK:";
const DOMAIN_MERKLE: &[u8] = b"POLAY-MKL:";

/// Hash a `Transaction` with domain separation.
pub fn hash_transaction(tx: &Transaction) -> CryptoResult<Hash> {
    let bytes = borsh::to_vec(tx).map_err(|e| CryptoError::HashError(e.to_string()))?;
    let mut prefixed = Vec::with_capacity(DOMAIN_TX.len() + bytes.len());
    prefixed.extend_from_slice(DOMAIN_TX);
    prefixed.extend_from_slice(&bytes);
    Ok(sha256(&prefixed))
}

/// Hash a `BlockHeader` with domain separation, excluding the `hash` field.
///
/// Uses `BlockHeader::hash_input_bytes()` which zeroes the `hash` field
/// before Borsh-encoding the full struct, producing a deterministic digest
/// regardless of the current value of `header.hash`.
pub fn hash_block_header(header: &BlockHeader) -> CryptoResult<Hash> {
    let bytes = header.hash_input_bytes();
    let mut prefixed = Vec::with_capacity(DOMAIN_BLOCK.len() + bytes.len());
    prefixed.extend_from_slice(DOMAIN_BLOCK);
    prefixed.extend_from_slice(&bytes);
    Ok(sha256(&prefixed))
}

/// Compute the Merkle root of a list of hashes using a simple binary Merkle tree.
///
/// - Empty list  -> `Hash::ZERO`
/// - Single hash -> that hash itself
/// - Otherwise   -> pair-wise combine with domain-separated SHA-256. Unpaired
///                  nodes at the end of an odd-length level are promoted directly
///                  (not duplicated), which prevents second-preimage attacks
///                  where `[A,B,C]` and `[A,B,C,C]` could produce the same root.
pub fn merkle_root(hashes: &[Hash]) -> Hash {
    if hashes.is_empty() {
        return Hash::ZERO;
    }
    if hashes.len() == 1 {
        return hashes[0];
    }

    let mut level: Vec<Hash> = hashes.to_vec();

    while level.len() > 1 {
        let mut next_level = Vec::with_capacity((level.len() + 1) / 2);

        // Pair-wise combine.
        let mut i = 0;
        while i + 1 < level.len() {
            let mut combined = Vec::with_capacity(DOMAIN_MERKLE.len() + 64);
            combined.extend_from_slice(DOMAIN_MERKLE);
            combined.extend_from_slice(level[i].as_bytes());
            combined.extend_from_slice(level[i + 1].as_bytes());
            next_level.push(sha256(&combined));
            i += 2;
        }

        // Promote unpaired last node directly (no duplication).
        if i < level.len() {
            next_level.push(level[i]);
        }

        level = next_level;
    }

    level[0]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_types::{Address, Transaction, TransactionAction};

    #[test]
    fn sha256_known_vector() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let h = sha256(b"");
        assert_eq!(
            h.to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_hello() {
        // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let h = sha256(b"hello");
        assert_eq!(
            h.to_hex(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn hash_transaction_deterministic() {
        let tx = Transaction {
            chain_id: "polay-testnet".to_string(),
            nonce: 1,
            signer: Address::ZERO,
            action: TransactionAction::Transfer {
                to: Address::ZERO,
                amount: 50,
            },
            max_fee: 100,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };
        let h1 = hash_transaction(&tx).unwrap();
        let h2 = hash_transaction(&tx).unwrap();
        assert_eq!(h1, h2);
        assert!(!h1.is_zero());
    }

    #[test]
    fn hash_block_header_excludes_hash_field() {
        let mut header = BlockHeader {
            height: 1,
            timestamp: 1_700_000_000,
            parent_hash: Hash::ZERO,
            state_root: Hash::ZERO,
            transactions_root: Hash::ZERO,
            proposer: Address::ZERO,
            chain_id: "polay-testnet".to_string(),
            hash: Hash::ZERO,
        };

        let h1 = hash_block_header(&header).unwrap();

        // Changing the `hash` field should NOT change the header hash.
        header.hash = Hash::new([0xff; 32]);
        let h2 = hash_block_header(&header).unwrap();
        assert_eq!(h1, h2);

        // Changing a real field SHOULD change the hash.
        header.height = 2;
        let h3 = hash_block_header(&header).unwrap();
        assert_ne!(h1, h3);
    }

    #[test]
    fn merkle_root_empty() {
        assert_eq!(merkle_root(&[]), Hash::ZERO);
    }

    #[test]
    fn merkle_root_single() {
        let h = sha256(b"only child");
        assert_eq!(merkle_root(&[h]), h);
    }

    #[test]
    fn merkle_root_two() {
        let a = sha256(b"a");
        let b = sha256(b"b");
        let root = merkle_root(&[a, b]);

        // Manual: SHA-256(DOMAIN || a || b)
        let mut combined = Vec::new();
        combined.extend_from_slice(b"POLAY-MKL:");
        combined.extend_from_slice(a.as_bytes());
        combined.extend_from_slice(b.as_bytes());
        let expected = sha256(&combined);
        assert_eq!(root, expected);
    }

    #[test]
    fn merkle_root_three_promotes_unpaired() {
        let a = sha256(b"a");
        let b = sha256(b"b");
        let c = sha256(b"c");
        let root = merkle_root(&[a, b, c]);

        // Level 1: [H(DOMAIN||a||b), c]  (c is promoted, not duplicated)
        let mut ab = Vec::new();
        ab.extend_from_slice(b"POLAY-MKL:");
        ab.extend_from_slice(a.as_bytes());
        ab.extend_from_slice(b.as_bytes());
        let hab = sha256(&ab);

        // Level 0: H(DOMAIN||hab||c)
        let mut final_buf = Vec::new();
        final_buf.extend_from_slice(b"POLAY-MKL:");
        final_buf.extend_from_slice(hab.as_bytes());
        final_buf.extend_from_slice(c.as_bytes());
        let expected = sha256(&final_buf);
        assert_eq!(root, expected);
    }

    #[test]
    fn merkle_root_second_preimage_resistance() {
        // [A,B,C] and [A,B,C,ZERO] must produce DIFFERENT roots.
        let a = sha256(b"a");
        let b = sha256(b"b");
        let c = sha256(b"c");
        let root3 = merkle_root(&[a, b, c]);
        let root4 = merkle_root(&[a, b, c, Hash::ZERO]);
        assert_ne!(root3, root4, "merkle tree must resist second-preimage via padding");
    }

    #[test]
    fn merkle_root_deterministic() {
        let hashes: Vec<Hash> = (0..8u8).map(|i| sha256(&[i])).collect();
        let r1 = merkle_root(&hashes);
        let r2 = merkle_root(&hashes);
        assert_eq!(r1, r2);
        assert!(!r1.is_zero());
    }

    #[test]
    fn merkle_root_order_matters() {
        let a = sha256(b"a");
        let b = sha256(b"b");
        assert_ne!(merkle_root(&[a, b]), merkle_root(&[b, a]));
    }
}
