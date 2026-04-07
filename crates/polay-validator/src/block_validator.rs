//! Pre-consensus block validation.
//!
//! This module implements the **critical** security check that must happen
//! before a validator votes on a block proposal. Without these checks, a
//! malicious proposer could get invalid blocks committed by the network.
//!
//! Two validation levels are offered:
//!
//! - [`BlockValidator::validate_proposed_block`] -- Full validation including
//!   transaction execution and state root verification. Call this before
//!   casting a prevote in consensus.
//!
//! - [`BlockValidator::validate_block_light`] -- Cheaper structural checks
//!   only (no execution). Useful for quick rejection of obviously malformed
//!   blocks.

use polay_config::ChainConfig;
use polay_crypto::{hash_block_header, merkle_root, sha256, PolayPublicKey};
use polay_execution::Executor;
use polay_state::{compute_state_root, StateStore};
use polay_types::block::Block;
use polay_types::hash::Hash;
use polay_types::transaction::SignedTransaction;
use tracing::debug;

// ---------------------------------------------------------------------------
// BlockValidationError
// ---------------------------------------------------------------------------

/// Errors produced by pre-consensus block validation.
#[derive(Debug, thiserror::Error)]
pub enum BlockValidationError {
    #[error("invalid block hash: expected {expected}, got {got}")]
    InvalidBlockHash { expected: String, got: String },

    #[error("invalid parent hash: expected {expected}, got {got}")]
    InvalidParentHash { expected: String, got: String },

    #[error("invalid transactions root: expected {expected}, got {got}")]
    InvalidTransactionsRoot { expected: String, got: String },

    #[error("invalid state root: expected {expected}, got {got}")]
    InvalidStateRoot { expected: String, got: String },

    #[error("invalid transaction at index {index}: {reason}")]
    InvalidTransaction { index: usize, reason: String },

    #[error("block height mismatch: expected {expected}, got {got}")]
    HeightMismatch { expected: u64, got: u64 },

    #[error("chain id mismatch: expected {expected}, got {got}")]
    ChainIdMismatch { expected: String, got: String },

    #[error("block exceeds max transactions: {count} > {max}")]
    TooManyTransactions { count: usize, max: usize },

    #[error("block exceeds gas limit")]
    BlockGasExceeded,

    #[error("execution error: {0}")]
    ExecutionError(String),

    #[error("state error: {0}")]
    StateError(String),
}

// ---------------------------------------------------------------------------
// BlockValidator
// ---------------------------------------------------------------------------

/// Validates proposed blocks before the local validator votes on them.
///
/// This is the primary defense against malicious proposers. Every non-trivial
/// field in the block header and every transaction is re-checked independently
/// to ensure the proposer did not tamper with any data.
pub struct BlockValidator {
    chain_config: ChainConfig,
}

impl BlockValidator {
    /// Create a new block validator from chain configuration.
    pub fn new(chain_config: ChainConfig) -> Self {
        Self { chain_config }
    }

    /// Full block validation -- call BEFORE voting in consensus.
    ///
    /// This is the critical security check. It verifies:
    /// 1. Chain ID matches
    /// 2. Height is correct
    /// 3. Parent hash matches our chain tip
    /// 4. Transaction count is within limits
    /// 5. Block header hash is correctly computed
    /// 6. Transactions merkle root matches the transactions
    /// 7. Every transaction has a valid signature and correct tx_hash
    /// 8. Execution produces the claimed state root
    pub fn validate_proposed_block(
        &self,
        block: &Block,
        expected_height: u64,
        expected_parent_hash: &Hash,
        store: &dyn StateStore,
    ) -> Result<(), BlockValidationError> {
        // 1. Verify chain_id matches.
        self.validate_chain_id(&block.header.chain_id)?;

        // 2. Verify height is correct.
        self.validate_height(block.header.height, expected_height)?;

        // 3. Verify parent hash matches our chain tip.
        self.validate_parent_hash(&block.header.parent_hash, expected_parent_hash)?;

        // 4. Verify block doesn't exceed max transactions.
        self.validate_tx_count(block.transactions.len())?;

        // 5. Verify block header hash is correctly computed.
        self.validate_block_hash(block)?;

        // 6. Verify transactions merkle root.
        self.validate_transactions_root(block)?;

        // 7. Validate each transaction individually (stateless checks).
        self.validate_all_transactions(&block.transactions)?;

        // 8. Execute all transactions and verify state root.
        //    This is the most expensive check but ensures correctness.
        self.validate_execution_and_state_root(block, store)?;

        debug!(
            height = block.header.height,
            hash = %block.header.hash,
            txs = block.transactions.len(),
            "block passed full pre-consensus validation"
        );

        Ok(())
    }

    /// Light validation -- cheaper checks only (no execution).
    ///
    /// Used for quick rejection of obviously bad blocks. Does NOT verify
    /// transaction signatures or execute transactions, so it cannot catch
    /// all attacks. Use [`validate_proposed_block`] for full security.
    pub fn validate_block_light(
        &self,
        block: &Block,
        expected_height: u64,
        expected_parent_hash: &Hash,
    ) -> Result<(), BlockValidationError> {
        self.validate_chain_id(&block.header.chain_id)?;
        self.validate_height(block.header.height, expected_height)?;
        self.validate_parent_hash(&block.header.parent_hash, expected_parent_hash)?;
        self.validate_tx_count(block.transactions.len())?;
        self.validate_block_hash(block)?;
        self.validate_transactions_root(block)?;
        Ok(())
    }

    // -- Individual validation steps ----------------------------------------

    fn validate_chain_id(&self, chain_id: &str) -> Result<(), BlockValidationError> {
        if chain_id != self.chain_config.chain_id {
            return Err(BlockValidationError::ChainIdMismatch {
                expected: self.chain_config.chain_id.clone(),
                got: chain_id.to_string(),
            });
        }
        Ok(())
    }

    fn validate_height(&self, got: u64, expected: u64) -> Result<(), BlockValidationError> {
        if got != expected {
            return Err(BlockValidationError::HeightMismatch { expected, got });
        }
        Ok(())
    }

    fn validate_parent_hash(
        &self,
        got: &Hash,
        expected: &Hash,
    ) -> Result<(), BlockValidationError> {
        if got != expected {
            return Err(BlockValidationError::InvalidParentHash {
                expected: expected.to_hex(),
                got: got.to_hex(),
            });
        }
        Ok(())
    }

    fn validate_tx_count(&self, count: usize) -> Result<(), BlockValidationError> {
        let max = self.chain_config.max_block_transactions;
        if count > max {
            return Err(BlockValidationError::TooManyTransactions { count, max });
        }
        Ok(())
    }

    fn validate_block_hash(&self, block: &Block) -> Result<(), BlockValidationError> {
        let computed = hash_block_header(&block.header).map_err(|e| {
            BlockValidationError::ExecutionError(format!("failed to hash block header: {e}"))
        })?;
        if computed != block.header.hash {
            return Err(BlockValidationError::InvalidBlockHash {
                expected: computed.to_hex(),
                got: block.header.hash.to_hex(),
            });
        }
        Ok(())
    }

    fn validate_transactions_root(&self, block: &Block) -> Result<(), BlockValidationError> {
        let tx_hashes: Vec<Hash> = block.transactions.iter().map(|tx| tx.tx_hash).collect();
        let computed_root = if tx_hashes.is_empty() {
            Hash::ZERO
        } else {
            merkle_root(&tx_hashes)
        };
        if computed_root != block.header.transactions_root {
            return Err(BlockValidationError::InvalidTransactionsRoot {
                expected: computed_root.to_hex(),
                got: block.header.transactions_root.to_hex(),
            });
        }
        Ok(())
    }

    fn validate_all_transactions(
        &self,
        txs: &[SignedTransaction],
    ) -> Result<(), BlockValidationError> {
        for (i, tx) in txs.iter().enumerate() {
            // Verify tx_hash is correct.
            // The canonical tx_hash is sha256(signing_bytes || sig_bytes).
            let signing_bytes = tx.transaction.signing_bytes();
            let sig_bytes = tx.signature.as_bytes();
            let mut payload = Vec::with_capacity(signing_bytes.len() + sig_bytes.len());
            payload.extend_from_slice(&signing_bytes);
            payload.extend_from_slice(sig_bytes);
            let computed_hash = sha256(&payload);
            if computed_hash != tx.tx_hash {
                return Err(BlockValidationError::InvalidTransaction {
                    index: i,
                    reason: format!(
                        "tx_hash mismatch: expected {}, got {}",
                        computed_hash.to_hex(),
                        tx.tx_hash.to_hex()
                    ),
                });
            }

            // Verify signer_pubkey is 32 bytes.
            if tx.signer_pubkey.len() != 32 {
                return Err(BlockValidationError::InvalidTransaction {
                    index: i,
                    reason: format!(
                        "invalid signer_pubkey length: expected 32, got {}",
                        tx.signer_pubkey.len()
                    ),
                });
            }

            // Parse public key.
            let pubkey_bytes: [u8; 32] = tx.signer_pubkey[..32]
                .try_into()
                .expect("length already checked");
            let pubkey = match PolayPublicKey::from_bytes(&pubkey_bytes) {
                Ok(pk) => pk,
                Err(_) => {
                    return Err(BlockValidationError::InvalidTransaction {
                        index: i,
                        reason: "invalid public key".into(),
                    })
                }
            };

            // Verify address matches pubkey.
            if pubkey.address() != tx.transaction.signer {
                return Err(BlockValidationError::InvalidTransaction {
                    index: i,
                    reason: "signer address doesn't match pubkey".into(),
                });
            }

            // Verify Ed25519 signature.
            let tx_signing_payload =
                polay_crypto::build_tx_signing_payload(&tx.transaction).map_err(|e| {
                    BlockValidationError::InvalidTransaction {
                        index: i,
                        reason: format!("failed to build signing payload: {e}"),
                    }
                })?;
            if let Err(e) = pubkey.verify(&tx_signing_payload, &tx.signature) {
                return Err(BlockValidationError::InvalidTransaction {
                    index: i,
                    reason: format!("invalid signature: {e}"),
                });
            }
        }
        Ok(())
    }

    fn validate_execution_and_state_root(
        &self,
        block: &Block,
        store: &dyn StateStore,
    ) -> Result<(), BlockValidationError> {
        // Execute all transactions to verify the proposed state root.
        let executor = Executor::new(self.chain_config.clone());
        let _receipts =
            executor.execute_block(&block.transactions, store, block.header.height, &block.header.proposer);

        // Compute the resulting state root.
        let commitment = compute_state_root(store)
            .map_err(|e| BlockValidationError::StateError(e.to_string()))?;

        if commitment.root != block.header.state_root {
            return Err(BlockValidationError::InvalidStateRoot {
                expected: block.header.state_root.to_hex(),
                got: commitment.root.to_hex(),
            });
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_config::ChainConfig;
    use polay_consensus::BlockProposer;
    use polay_crypto::{sha256, PolayKeypair};
    use polay_state::{MemoryStore, StateWriter};
    use polay_types::address::Address;
    use polay_types::signature::Signature;
    use polay_types::transaction::{Transaction, TransactionAction};

    const CHAIN_ID: &str = "polay-devnet-1";

    fn test_config() -> ChainConfig {
        ChainConfig::default()
    }

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn seed_account(store: &dyn StateStore, addr: Address, balance: u64) {
        StateWriter::new(store)
            .set_account(&polay_types::AccountState::with_balance(addr, balance, 0))
            .unwrap();
    }

    /// Build a signed transaction with a real Ed25519 signature and valid tx_hash.
    fn make_real_signed_tx(kp: &PolayKeypair, nonce: u64) -> SignedTransaction {
        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce,
            signer: kp.address(),
            action: TransactionAction::Transfer {
                to: test_addr(0xBB),
                amount: 100,
            },
            max_fee: 100_000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };

        let signing_bytes = tx.signing_bytes();
        let sig = kp.sign(&signing_bytes);

        let mut payload = Vec::with_capacity(signing_bytes.len() + 64);
        payload.extend_from_slice(&signing_bytes);
        payload.extend_from_slice(sig.as_bytes());
        let tx_hash = sha256(&payload);

        SignedTransaction::new(tx, sig, tx_hash, kp.public_key().to_bytes().to_vec())
    }

    /// Build a signed transaction with a fake signature (for negative tests).
    fn make_fake_signed_tx(nonce: u64, hash_seed: u8) -> SignedTransaction {
        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce,
            signer: test_addr(0x01),
            action: TransactionAction::Transfer {
                to: test_addr(0xBB),
                amount: 100,
            },
            max_fee: 100_000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };
        let sig = Signature::new([0xAB; 64]);
        let signing_bytes = tx.signing_bytes();
        let mut payload = Vec::with_capacity(signing_bytes.len() + 64);
        payload.extend_from_slice(&signing_bytes);
        payload.extend_from_slice(sig.as_bytes());
        let tx_hash = sha256(&payload);
        SignedTransaction::new(tx, sig, tx_hash, vec![hash_seed; 32])
    }

    /// Build a valid block using the same mechanism as block_producer.
    fn build_valid_block(
        height: u64,
        parent_hash: Hash,
        state_root: Hash,
        transactions: Vec<SignedTransaction>,
    ) -> Block {
        BlockProposer::propose_block(
            height,
            0,
            parent_hash,
            state_root,
            transactions,
            CHAIN_ID.to_string(),
            test_addr(0x01),
            1_700_000_000,
        )
    }

    // -- Positive test -------------------------------------------------------

    #[test]
    fn valid_empty_block_passes_validation() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();

        let state_root = compute_state_root(&store).unwrap().root;
        let block = build_valid_block(1, Hash::ZERO, state_root, vec![]);

        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_ok(), "valid empty block should pass: {:?}", result);
    }

    #[test]
    fn valid_block_with_transactions_passes_full_validation() {
        let config = test_config();
        let store = MemoryStore::new();

        let kp = PolayKeypair::generate();
        seed_account(&store, kp.address(), 10_000_000);
        // Also seed the receiver so transfer doesn't fail on missing account
        // (actually, transfer creates the account -- let's just make sure the
        // sender has enough)

        let txs = vec![make_real_signed_tx(&kp, 0)];

        // Execute to get state root AFTER execution (mimicking what the
        // proposer does).
        let executor = Executor::new(config.clone());
        let _receipts = executor.execute_block(&txs, &store, 1, &test_addr(0x01));
        let state_root = compute_state_root(&store).unwrap().root;

        // Now build the block with the post-execution state root.
        let block = build_valid_block(1, Hash::ZERO, state_root, txs);

        // For full validation, we need a fresh store with the same initial
        // state (pre-execution) because validation re-executes.
        let fresh_store = MemoryStore::new();
        seed_account(&fresh_store, kp.address(), 10_000_000);

        let validator = BlockValidator::new(config);
        let result = validator.validate_proposed_block(
            &block,
            1,
            &Hash::ZERO,
            &fresh_store,
        );
        assert!(result.is_ok(), "valid block with txs should pass: {:?}", result);
    }

    // -- Block hash tests ----------------------------------------------------

    #[test]
    fn wrong_block_hash_is_rejected() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        let mut block = build_valid_block(1, Hash::ZERO, state_root, vec![]);
        // Tamper with the block hash.
        block.header.hash = Hash::new([0xFF; 32]);

        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BlockValidationError::InvalidBlockHash { .. }),
            "expected InvalidBlockHash, got: {err}"
        );
    }

    // -- Parent hash tests ---------------------------------------------------

    #[test]
    fn wrong_parent_hash_is_rejected() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        let block = build_valid_block(1, Hash::new([0xAA; 32]), state_root, vec![]);

        // We expect parent_hash = Hash::ZERO, but block has [0xAA; 32].
        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BlockValidationError::InvalidParentHash { .. }),
            "expected InvalidParentHash, got: {err}"
        );
    }

    // -- Transactions root tests ---------------------------------------------

    #[test]
    fn wrong_transactions_root_is_rejected() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        let mut block = build_valid_block(1, Hash::ZERO, state_root, vec![]);
        // Tamper with the transactions root (and recompute the block hash so
        // the block hash check passes first).
        block.header.transactions_root = Hash::new([0xDD; 32]);
        block.header.compute_hash(|bytes| sha256(bytes).to_bytes());

        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BlockValidationError::InvalidTransactionsRoot { .. }),
            "expected InvalidTransactionsRoot, got: {err}"
        );
    }

    // -- Transaction signature tests -----------------------------------------

    #[test]
    fn invalid_transaction_signature_is_rejected() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();

        // Use a fake signed transaction (invalid Ed25519 sig).
        let txs = vec![make_fake_signed_tx(0, 0x01)];
        let state_root = compute_state_root(&store).unwrap().root;
        let block = build_valid_block(1, Hash::ZERO, state_root, txs);

        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BlockValidationError::InvalidTransaction { .. }),
            "expected InvalidTransaction, got: {err}"
        );
    }

    // -- Tampered tx_hash tests ----------------------------------------------

    #[test]
    fn tampered_tx_hash_is_rejected() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();

        let kp = PolayKeypair::generate();
        let mut tx = make_real_signed_tx(&kp, 0);
        // Tamper with the tx_hash.
        tx.tx_hash = Hash::new([0xEE; 32]);

        let txs = vec![tx];
        let state_root = compute_state_root(&store).unwrap().root;
        // Build block with the tampered tx (merkle root will be computed
        // from the tampered hash, so the merkle root check will pass).
        let block = build_valid_block(1, Hash::ZERO, state_root, txs);

        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BlockValidationError::InvalidTransaction { index: 0, .. }),
            "expected InvalidTransaction at index 0, got: {err}"
        );
    }

    // -- Height mismatch tests -----------------------------------------------

    #[test]
    fn wrong_height_is_rejected() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        let block = build_valid_block(5, Hash::ZERO, state_root, vec![]);

        // We expect height 1, but block says 5.
        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BlockValidationError::HeightMismatch { expected: 1, got: 5 }),
            "expected HeightMismatch, got: {err}"
        );
    }

    // -- Chain ID mismatch tests ---------------------------------------------

    #[test]
    fn wrong_chain_id_is_rejected() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        // Build a block with a different chain_id.
        let mut block = build_valid_block(1, Hash::ZERO, state_root, vec![]);
        block.header.chain_id = "evil-chain-1".to_string();
        block.header.compute_hash(|bytes| sha256(bytes).to_bytes());

        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BlockValidationError::ChainIdMismatch { .. }),
            "expected ChainIdMismatch, got: {err}"
        );
    }

    // -- Max transactions tests ----------------------------------------------

    #[test]
    fn too_many_transactions_is_rejected() {
        let mut config = test_config();
        config.max_block_transactions = 2; // Very low limit for testing.
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        let txs = vec![
            make_fake_signed_tx(0, 1),
            make_fake_signed_tx(1, 2),
            make_fake_signed_tx(2, 3),
        ];
        let block = build_valid_block(1, Hash::ZERO, state_root, txs);

        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BlockValidationError::TooManyTransactions { count: 3, max: 2 }),
            "expected TooManyTransactions, got: {err}"
        );
    }

    // -- Light validation tests ----------------------------------------------

    #[test]
    fn light_validation_catches_bad_hash() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        let mut block = build_valid_block(1, Hash::ZERO, state_root, vec![]);
        block.header.hash = Hash::new([0xFF; 32]);

        let result =
            validator.validate_block_light(&block, 1, &Hash::ZERO);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BlockValidationError::InvalidBlockHash { .. }
        ));
    }

    #[test]
    fn light_validation_catches_bad_parent() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        let block = build_valid_block(1, Hash::new([0xAA; 32]), state_root, vec![]);

        let result =
            validator.validate_block_light(&block, 1, &Hash::ZERO);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BlockValidationError::InvalidParentHash { .. }
        ));
    }

    #[test]
    fn light_validation_passes_valid_block() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        let block = build_valid_block(1, Hash::ZERO, state_root, vec![]);

        let result =
            validator.validate_block_light(&block, 1, &Hash::ZERO);
        assert!(result.is_ok(), "light validation should pass: {:?}", result);
    }

    // -- State root mismatch test (requires execution) -----------------------

    #[test]
    fn wrong_state_root_is_rejected() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();

        // Build a block with a bogus state root but otherwise correct.
        let block = build_valid_block(1, Hash::ZERO, Hash::new([0xCC; 32]), vec![]);

        let result =
            validator.validate_proposed_block(&block, 1, &Hash::ZERO, &store);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BlockValidationError::InvalidStateRoot { .. }),
            "expected InvalidStateRoot, got: {err}"
        );
    }

    // -- Light validation does NOT catch signature issues ---------------------

    #[test]
    fn light_validation_does_not_check_signatures() {
        let config = test_config();
        let validator = BlockValidator::new(config);
        let store = MemoryStore::new();
        let state_root = compute_state_root(&store).unwrap().root;

        // Block with fake-signed transactions -- light validation should pass
        // because it skips signature checks.
        let txs = vec![make_fake_signed_tx(0, 1)];
        let block = build_valid_block(1, Hash::ZERO, state_root, txs);

        let result = validator.validate_block_light(&block, 1, &Hash::ZERO);
        assert!(
            result.is_ok(),
            "light validation should not check signatures: {:?}",
            result
        );
    }
}
