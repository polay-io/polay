use polay_crypto::{merkle_root, sha256};
use polay_types::address::Address;
use polay_types::block::{Block, BlockHeader};
use polay_types::hash::Hash;
use polay_types::transaction::SignedTransaction;

/// Block construction logic for the designated proposer.
///
/// `BlockProposer` is a stateless helper that assembles a complete [`Block`]
/// from its constituent parts, computing the transactions Merkle root and the
/// block header hash along the way.
pub struct BlockProposer;

impl BlockProposer {
    /// Assemble a new block for the given `height` and `round`.
    ///
    /// Steps:
    /// 1. Compute the transactions root as the Merkle root of transaction
    ///    hashes.
    /// 2. Build a [`BlockHeader`] with all provided fields.
    /// 3. Compute and set the block header hash via SHA-256.
    /// 4. Return the assembled [`Block`].
    ///
    /// # Arguments
    ///
    /// * `height`        - The sequential block number.
    /// * `round`         - The consensus round (informational; not stored in
    ///                     the header).
    /// * `parent_hash`   - Hash of the parent block (`Hash::ZERO` for genesis).
    /// * `state_root`    - Post-execution state trie root.
    /// * `transactions`  - Ordered list of signed transactions to include.
    /// * `chain_id`      - Chain identifier string.
    /// * `proposer`      - Address of the validator proposing this block.
    /// * `timestamp`     - Unix timestamp (seconds) for the block.
    #[allow(clippy::too_many_arguments)]
    pub fn propose_block(
        height: u64,
        _round: u32,
        parent_hash: Hash,
        state_root: Hash,
        transactions: Vec<SignedTransaction>,
        chain_id: String,
        proposer: Address,
        timestamp: u64,
    ) -> Block {
        // Compute the transactions root from the tx hashes.
        let tx_hashes: Vec<Hash> = transactions.iter().map(|tx| tx.tx_hash).collect();
        let transactions_root = merkle_root(&tx_hashes);

        // Build the header.
        let mut header = BlockHeader {
            height,
            timestamp,
            parent_hash,
            state_root,
            transactions_root,
            proposer,
            chain_id,
            hash: Hash::ZERO,
        };

        // Compute the block hash.
        header.compute_hash(|bytes| {
            let h = sha256(bytes);
            h.to_bytes()
        });

        Block::new(header, transactions)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_types::signature::Signature;
    use polay_types::transaction::{Transaction, TransactionAction};

    fn make_signed_tx(nonce: u64) -> SignedTransaction {
        let tx = Transaction {
            chain_id: "polay-test".into(),
            nonce,
            signer: Address::ZERO,
            action: TransactionAction::Transfer {
                to: Address::new([0x01; 32]),
                amount: 100,
            },
            max_fee: 10,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };
        let tx_hash = sha256(&borsh::to_vec(&tx).unwrap());
        SignedTransaction::new(tx, Signature::ZERO, tx_hash, vec![0u8; 32])
    }

    #[test]
    fn propose_empty_block() {
        let block = BlockProposer::propose_block(
            1,
            0,
            Hash::ZERO,
            Hash::ZERO,
            vec![],
            "polay-test".into(),
            Address::ZERO,
            1_700_000_000,
        );

        assert_eq!(block.height(), 1);
        assert_eq!(block.tx_count(), 0);
        assert!(!block.hash().is_zero());
        assert_eq!(block.header.transactions_root, Hash::ZERO);
    }

    #[test]
    fn propose_block_with_transactions() {
        let txs = vec![make_signed_tx(0), make_signed_tx(1), make_signed_tx(2)];

        let block = BlockProposer::propose_block(
            5,
            0,
            Hash::new([0xAA; 32]),
            Hash::new([0xBB; 32]),
            txs,
            "polay-test".into(),
            Address::new([0x01; 32]),
            1_700_000_000,
        );

        assert_eq!(block.height(), 5);
        assert_eq!(block.tx_count(), 3);
        assert!(!block.hash().is_zero());
        // Transactions root should not be zero when there are transactions.
        assert_ne!(block.header.transactions_root, Hash::ZERO);
    }

    #[test]
    fn block_hash_is_deterministic() {
        let txs = vec![make_signed_tx(0)];

        let b1 = BlockProposer::propose_block(
            1,
            0,
            Hash::ZERO,
            Hash::ZERO,
            txs.clone(),
            "polay-test".into(),
            Address::ZERO,
            1_700_000_000,
        );
        let b2 = BlockProposer::propose_block(
            1,
            0,
            Hash::ZERO,
            Hash::ZERO,
            txs,
            "polay-test".into(),
            Address::ZERO,
            1_700_000_000,
        );

        assert_eq!(b1.hash(), b2.hash());
    }

    #[test]
    fn different_heights_produce_different_hashes() {
        let b1 = BlockProposer::propose_block(
            1,
            0,
            Hash::ZERO,
            Hash::ZERO,
            vec![],
            "polay-test".into(),
            Address::ZERO,
            1_700_000_000,
        );
        let b2 = BlockProposer::propose_block(
            2,
            0,
            Hash::ZERO,
            Hash::ZERO,
            vec![],
            "polay-test".into(),
            Address::ZERO,
            1_700_000_000,
        );

        assert_ne!(b1.hash(), b2.hash());
    }

    #[test]
    fn transactions_root_matches_merkle() {
        let txs = vec![make_signed_tx(0), make_signed_tx(1)];
        let expected_root = merkle_root(&[txs[0].tx_hash, txs[1].tx_hash]);

        let block = BlockProposer::propose_block(
            1,
            0,
            Hash::ZERO,
            Hash::ZERO,
            txs,
            "polay-test".into(),
            Address::ZERO,
            1_700_000_000,
        );

        assert_eq!(block.header.transactions_root, expected_root);
    }
}
