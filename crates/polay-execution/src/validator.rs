//! Stateless and stateful transaction validation.

use polay_config::ChainConfig;
use polay_crypto::{sha256, PolayPublicKey};
use polay_state::{StateStore, StateView};
use polay_types::{SignedTransaction, TransactionAction};
use tracing::debug;

use crate::error::ExecutionError;
use crate::input_validation;

// ---------------------------------------------------------------------------
// Stateless validation
// ---------------------------------------------------------------------------

/// Validate a signed transaction without accessing any state.
///
/// Checks:
/// - `chain_id` matches the expected chain
/// - `max_fee > 0`
/// - signer is not the zero address
/// - `tx_hash` matches the recomputed hash
/// - Transaction size does not exceed the configured maximum
/// - Action-specific input validation (field lengths, ranges, etc.)
pub fn validate_stateless(tx: &SignedTransaction, chain_id: &str) -> Result<(), ExecutionError> {
    validate_stateless_with_config(tx, chain_id, None)
}

/// Validate a signed transaction without accessing any state, using the full
/// chain config for additional checks (size limits, input validation, etc.).
pub fn validate_stateless_with_config(
    tx: &SignedTransaction,
    chain_id: &str,
    config: Option<&ChainConfig>,
) -> Result<(), ExecutionError> {
    let inner = &tx.transaction;

    // Chain ID must match.
    if inner.chain_id != chain_id {
        return Err(ExecutionError::InvalidChainId {
            expected: chain_id.to_string(),
            got: inner.chain_id.clone(),
        });
    }

    // Signer must not be zero address.
    if inner.signer.is_zero() {
        return Err(ExecutionError::ZeroAddressSigner);
    }

    // Fee must be positive.
    if inner.max_fee == 0 {
        return Err(ExecutionError::FeeTooLow);
    }

    // Sponsor validation (stateless).
    if let Some(sponsor) = &inner.sponsor {
        // Sponsor must not be the zero address.
        if sponsor.is_zero() {
            return Err(ExecutionError::ZeroAddressSponsor);
        }
        // Sponsor must not be the signer (can't sponsor yourself).
        if *sponsor == inner.signer {
            return Err(ExecutionError::SponsorIsSigner);
        }
    }

    // Transaction size limit.
    if let Some(cfg) = config {
        input_validation::validate_tx_size(tx, cfg.max_transaction_size_bytes)?;
    }

    // Recompute tx_hash and verify it matches.
    let signing_bytes = inner.signing_bytes();
    let sig_bytes = tx.signature.as_bytes();
    let mut payload = Vec::with_capacity(signing_bytes.len() + sig_bytes.len());
    payload.extend_from_slice(&signing_bytes);
    payload.extend_from_slice(sig_bytes);
    let computed_hash = sha256(&payload);
    if computed_hash != tx.tx_hash {
        return Err(ExecutionError::InvalidTxHash);
    }

    // Verify signer_pubkey is 32 bytes.
    if tx.signer_pubkey.len() != 32 {
        return Err(ExecutionError::InvalidSignerPubkey(format!(
            "expected 32 bytes, got {}",
            tx.signer_pubkey.len(),
        )));
    }

    // Derive address from pubkey and verify it matches the expected identity.
    let pubkey_bytes: [u8; 32] = tx.signer_pubkey[..32].try_into().map_err(|_| {
        ExecutionError::InvalidSignerPubkey(
            "failed to convert signer_pubkey to [u8; 32]".to_string(),
        )
    })?;
    let pubkey = PolayPublicKey::from_bytes(&pubkey_bytes).map_err(|e| {
        ExecutionError::InvalidSignerPubkey(format!("invalid Ed25519 public key: {e}"))
    })?;
    let derived_address = pubkey.address();

    if let Some(session_addr) = &inner.session {
        // Session-signed transaction: the pubkey should derive to the session
        // address, not the signer (granting account).
        if derived_address != *session_addr {
            return Err(ExecutionError::InvalidSignerPubkey(format!(
                "session pubkey derives address {}, but transaction.session is {}",
                derived_address, session_addr,
            )));
        }
    } else {
        // Normal transaction: pubkey must derive to the signer address.
        if derived_address != inner.signer {
            return Err(ExecutionError::InvalidSignerPubkey(format!(
                "pubkey derives address {}, but transaction signer is {}",
                derived_address, inner.signer,
            )));
        }
    }

    // Verify the Ed25519 signature over the signing payload.
    let tx_signing_payload = polay_crypto::build_tx_signing_payload(inner).map_err(|e| {
        ExecutionError::InvalidSignature(format!("failed to build signing payload: {e}"))
    })?;
    pubkey
        .verify(&tx_signing_payload, &tx.signature)
        .map_err(|e| {
            ExecutionError::InvalidSignature(format!("Ed25519 verification failed: {e}"))
        })?;

    // Action-specific input validation.
    if let Some(cfg) = config {
        input_validation::validate_transaction_input(inner, cfg)?;
    }

    debug!(
        tx_hash = %tx.tx_hash,
        action = tx.action_label(),
        "stateless validation passed"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Stateful validation
// ---------------------------------------------------------------------------

/// Validate a signed transaction against the current on-chain state.
///
/// Checks:
/// - Sender account exists
/// - Nonce matches `account.nonce`
/// - Balance covers `max_fee` (and, for transfers, amount + fee)
/// - If session-signed: session grant is valid, action permitted, spending limit ok
pub fn validate_stateful(
    tx: &SignedTransaction,
    store: &dyn StateStore,
) -> Result<(), ExecutionError> {
    validate_stateful_at_height(tx, store, 0)
}

/// Validate a signed transaction against the current on-chain state at a
/// specific block height. When `current_height` is 0, session expiry checks
/// use the chain height from state.
pub fn validate_stateful_at_height(
    tx: &SignedTransaction,
    store: &dyn StateStore,
    current_height: u64,
) -> Result<(), ExecutionError> {
    let view = StateView::new(store);
    let signer = &tx.transaction.signer;

    let account = view
        .get_account(signer)?
        .ok_or_else(|| ExecutionError::AccountNotFound(signer.to_hex()))?;

    // Nonce must match exactly.
    if account.nonce != tx.transaction.nonce {
        return Err(ExecutionError::InvalidNonce {
            expected: account.nonce,
            got: tx.transaction.nonce,
        });
    }

    // Determine who pays the gas fee.
    if let Some(sponsor) = &tx.transaction.sponsor {
        // Sponsor pays the fee -- verify sponsor account exists and has sufficient balance.
        let sponsor_account = view
            .get_account(sponsor)?
            .ok_or_else(|| ExecutionError::SponsorAccountNotFound(sponsor.to_hex()))?;

        if sponsor_account.balance < tx.transaction.max_fee {
            return Err(ExecutionError::InsufficientBalance {
                required: tx.transaction.max_fee,
                available: sponsor_account.balance,
            });
        }

        // Signer only needs to cover action amounts (not fee).
        match &tx.transaction.action {
            TransactionAction::Transfer { amount, .. } => {
                if account.balance < *amount {
                    return Err(ExecutionError::InsufficientBalance {
                        required: *amount,
                        available: account.balance,
                    });
                }
            }
            TransactionAction::DelegateStake { amount, .. } => {
                if account.balance < *amount {
                    return Err(ExecutionError::InsufficientBalance {
                        required: *amount,
                        available: account.balance,
                    });
                }
            }
            _ => {}
        }
    } else {
        // No sponsor — signer pays everything.
        // Balance must cover max_fee at a minimum.
        if account.balance < tx.transaction.max_fee {
            return Err(ExecutionError::InsufficientBalance {
                required: tx.transaction.max_fee,
                available: account.balance,
            });
        }

        // Action-specific balance checks.
        match &tx.transaction.action {
            TransactionAction::Transfer { amount, .. } => {
                let total_required = amount.saturating_add(tx.transaction.max_fee);
                if account.balance < total_required {
                    return Err(ExecutionError::InsufficientBalance {
                        required: total_required,
                        available: account.balance,
                    });
                }
            }
            TransactionAction::DelegateStake { amount, .. } => {
                let total_required = amount.saturating_add(tx.transaction.max_fee);
                if account.balance < total_required {
                    return Err(ExecutionError::InsufficientBalance {
                        required: total_required,
                        available: account.balance,
                    });
                }
            }
            // Other actions are checked during execution.
            _ => {}
        }
    }

    // Session grant validation.
    if let Some(session_addr) = &tx.transaction.session {
        let height = if current_height > 0 {
            current_height
        } else {
            view.get_chain_height()?
        };

        let grant = view
            .get_session(signer, session_addr)?
            .ok_or(ExecutionError::SessionNotFound)?;

        if grant.revoked {
            return Err(ExecutionError::SessionRevoked);
        }

        if !grant.is_valid(height) {
            return Err(ExecutionError::SessionExpired);
        }

        let action_label = tx.transaction.action.label();
        if !grant.is_action_permitted(action_label) {
            return Err(ExecutionError::SessionActionNotPermitted);
        }

        // Check spending limit (use max_fee as the estimated cost).
        if !grant.can_spend(tx.transaction.max_fee) {
            return Err(ExecutionError::SessionSpendingLimitExceeded);
        }
    }

    debug!(
        tx_hash = %tx.tx_hash,
        action = tx.action_label(),
        signer = %signer,
        "stateful validation passed"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_crypto::{sha256, PolayKeypair};
    use polay_state::MemoryStore;
    use polay_types::{
        AccountState, Address, Hash, Signature, SignedTransaction, Transaction, TransactionAction,
    };

    const CHAIN_ID: &str = "polay-test-1";

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    /// Build a signed transaction with a real Ed25519 signature and valid tx_hash.
    ///
    /// The tx_hash follows the validator's scheme: `sha256(signing_bytes || sig_bytes)`.
    fn make_signed_tx_with_keypair(tx: Transaction, kp: &PolayKeypair) -> SignedTransaction {
        let signing_bytes = tx.signing_bytes();
        let sig = kp.sign(&signing_bytes);

        let mut payload = Vec::with_capacity(signing_bytes.len() + 64);
        payload.extend_from_slice(&signing_bytes);
        payload.extend_from_slice(sig.as_bytes());
        let tx_hash = sha256(&payload);

        SignedTransaction::new(tx, sig, tx_hash, kp.public_key().to_bytes().to_vec())
    }

    /// Build a signed tx with a fake (invalid) signature.
    /// Useful for tests that check errors before the signature verification step.
    fn make_fake_signed_tx(tx: Transaction) -> SignedTransaction {
        let sig = Signature::new([0xAB; 64]);
        let signing_bytes = tx.signing_bytes();
        let mut payload = Vec::with_capacity(signing_bytes.len() + 64);
        payload.extend_from_slice(&signing_bytes);
        payload.extend_from_slice(sig.as_bytes());
        let tx_hash = sha256(&payload);
        SignedTransaction::new(tx, sig, tx_hash, vec![0u8; 32])
    }

    fn sample_keypair_and_tx() -> (PolayKeypair, SignedTransaction) {
        let kp = PolayKeypair::generate();
        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: kp.address(),
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 500,
            },
            max_fee: 100,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };
        let stx = make_signed_tx_with_keypair(tx, &kp);
        (kp, stx)
    }

    #[test]
    fn stateless_valid() {
        let (_kp, stx) = sample_keypair_and_tx();
        assert!(validate_stateless(&stx, CHAIN_ID).is_ok());
    }

    #[test]
    fn stateless_wrong_chain_id() {
        let (_kp, stx) = sample_keypair_and_tx();
        let err = validate_stateless(&stx, "other-chain").unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidChainId { .. }));
    }

    #[test]
    fn stateless_zero_signer() {
        // Zero-signer check runs before signature check, so a fake sig is fine.
        let stx = make_fake_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: Address::ZERO,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 500,
            },
            max_fee: 100,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        });
        let err = validate_stateless(&stx, CHAIN_ID).unwrap_err();
        assert!(matches!(err, ExecutionError::ZeroAddressSigner));
    }

    #[test]
    fn stateless_zero_fee() {
        // Fee check runs before signature check, so a fake sig is fine.
        let stx = make_fake_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: test_addr(1),
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 500,
            },
            max_fee: 0,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        });
        let err = validate_stateless(&stx, CHAIN_ID).unwrap_err();
        assert!(matches!(err, ExecutionError::FeeTooLow));
    }

    #[test]
    fn stateless_bad_hash() {
        let (_kp, mut stx) = sample_keypair_and_tx();
        stx.tx_hash = Hash::ZERO; // tamper
        let err = validate_stateless(&stx, CHAIN_ID).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidTxHash));
    }

    #[test]
    fn stateful_valid() {
        let (kp, stx) = sample_keypair_and_tx();
        let store = MemoryStore::new();
        let account = AccountState::with_balance(kp.address(), 10_000, 0);
        polay_state::StateWriter::new(&store)
            .set_account(&account)
            .unwrap();

        assert!(validate_stateful(&stx, &store).is_ok());
    }

    #[test]
    fn stateful_account_not_found() {
        let store = MemoryStore::new();
        let (_kp, stx) = sample_keypair_and_tx();
        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::AccountNotFound(_)));
    }

    #[test]
    fn stateful_wrong_nonce() {
        let (kp, stx) = sample_keypair_and_tx();
        let store = MemoryStore::new();
        let mut account = AccountState::with_balance(kp.address(), 10_000, 0);
        account.nonce = 5;
        polay_state::StateWriter::new(&store)
            .set_account(&account)
            .unwrap();

        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(
            err,
            ExecutionError::InvalidNonce {
                expected: 5,
                got: 0
            }
        ));
    }

    #[test]
    fn stateful_insufficient_balance_for_fee() {
        let (kp, stx) = sample_keypair_and_tx();
        let store = MemoryStore::new();
        let account = AccountState::with_balance(kp.address(), 50, 0); // only 50, fee=100
        polay_state::StateWriter::new(&store)
            .set_account(&account)
            .unwrap();

        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }

    #[test]
    fn stateful_insufficient_balance_for_transfer() {
        let (kp, stx) = sample_keypair_and_tx();
        let store = MemoryStore::new();
        // Balance 200: covers fee (100) but not amount (500) + fee (100) = 600
        let account = AccountState::with_balance(kp.address(), 200, 0);
        polay_state::StateWriter::new(&store)
            .set_account(&account)
            .unwrap();

        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }

    // =======================================================================
    // Session key validation tests
    // =======================================================================

    fn make_session_grant(
        granter: Address,
        session_pubkey: &[u8],
        session_address: Address,
        permissions: polay_types::SessionPermission,
        expires_at: u64,
        spending_limit: u64,
    ) -> polay_types::SessionGrant {
        polay_types::SessionGrant {
            granter,
            session_pubkey: session_pubkey.to_vec(),
            session_address,
            permissions,
            expires_at,
            spending_limit,
            amount_spent: 0,
            revoked: false,
            created_at: 1,
        }
    }

    #[test]
    fn stateless_session_key_address_matches_session_field() {
        // For a session-signed tx, the pubkey must derive to session address,
        // not the signer.
        let session_kp = PolayKeypair::generate();
        let session_address = session_kp.address();
        let granter = test_addr(0x01);

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: granter,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: Some(session_address),
            sponsor: None,
        };

        let stx = make_signed_tx_with_keypair(tx, &session_kp);
        assert!(validate_stateless(&stx, CHAIN_ID).is_ok());
    }

    #[test]
    fn stateless_session_key_wrong_address_rejected() {
        // If session field doesn't match the derived address, reject.
        let session_kp = PolayKeypair::generate();
        let granter = test_addr(0x01);

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: granter,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: Some(test_addr(0xFF)), // wrong session address
            sponsor: None,
        };

        let stx = make_signed_tx_with_keypair(tx, &session_kp);
        let err = validate_stateless(&stx, CHAIN_ID).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidSignerPubkey(_)));
    }

    #[test]
    fn stateful_session_not_found_rejected() {
        let session_kp = PolayKeypair::generate();
        let granter_kp = PolayKeypair::generate();
        let granter = granter_kp.address();
        let session_address = session_kp.address();

        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(granter, 10_000, 0))
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: granter,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: Some(session_address),
            sponsor: None,
        };

        let stx = make_signed_tx_with_keypair(tx, &session_kp);
        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::SessionNotFound));
    }

    #[test]
    fn stateful_session_expired_rejected() {
        let session_kp = PolayKeypair::generate();
        let session_pubkey = session_kp.public_key().to_bytes();
        let session_address = session_kp.address();
        let granter_kp = PolayKeypair::generate();
        let granter = granter_kp.address();

        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(granter, 10_000, 0))
            .unwrap();

        // Create a session that expires at height 10.
        let grant = make_session_grant(
            granter,
            &session_pubkey,
            session_address,
            polay_types::SessionPermission::All,
            10, // expires at height 10
            1_000_000,
        );
        polay_state::StateWriter::new(&store)
            .set_session(&grant)
            .unwrap();

        // Set chain height to 100 (past expiry).
        polay_state::StateWriter::new(&store)
            .set_chain_height(100)
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: granter,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: Some(session_address),
            sponsor: None,
        };

        let stx = make_signed_tx_with_keypair(tx, &session_kp);
        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::SessionExpired));
    }

    #[test]
    fn stateful_session_revoked_rejected() {
        let session_kp = PolayKeypair::generate();
        let session_pubkey = session_kp.public_key().to_bytes();
        let session_address = session_kp.address();
        let granter_kp = PolayKeypair::generate();
        let granter = granter_kp.address();

        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(granter, 10_000, 0))
            .unwrap();

        let mut grant = make_session_grant(
            granter,
            &session_pubkey,
            session_address,
            polay_types::SessionPermission::All,
            10_000,
            1_000_000,
        );
        grant.revoked = true;
        polay_state::StateWriter::new(&store)
            .set_session(&grant)
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: granter,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: Some(session_address),
            sponsor: None,
        };

        let stx = make_signed_tx_with_keypair(tx, &session_kp);
        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::SessionRevoked));
    }

    #[test]
    fn stateful_session_action_not_permitted() {
        let session_kp = PolayKeypair::generate();
        let session_pubkey = session_kp.public_key().to_bytes();
        let session_address = session_kp.address();
        let granter_kp = PolayKeypair::generate();
        let granter = granter_kp.address();

        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(granter, 10_000, 0))
            .unwrap();

        // Only allow "buy_listing", not "transfer".
        let grant = make_session_grant(
            granter,
            &session_pubkey,
            session_address,
            polay_types::SessionPermission::Actions(vec!["buy_listing".into()]),
            10_000,
            1_000_000,
        );
        polay_state::StateWriter::new(&store)
            .set_session(&grant)
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: granter,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: Some(session_address),
            sponsor: None,
        };

        let stx = make_signed_tx_with_keypair(tx, &session_kp);
        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::SessionActionNotPermitted));
    }

    #[test]
    fn stateful_session_spending_limit_exceeded() {
        let session_kp = PolayKeypair::generate();
        let session_pubkey = session_kp.public_key().to_bytes();
        let session_address = session_kp.address();
        let granter_kp = PolayKeypair::generate();
        let granter = granter_kp.address();

        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(granter, 10_000, 0))
            .unwrap();

        let mut grant = make_session_grant(
            granter,
            &session_pubkey,
            session_address,
            polay_types::SessionPermission::All,
            10_000,
            100, // very low spending limit
        );
        grant.amount_spent = 90; // already spent 90 out of 100
        polay_state::StateWriter::new(&store)
            .set_session(&grant)
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: granter,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 50,
            },
            max_fee: 1000, // 90 + 1000 > 100
            timestamp: 1_700_000_000,
            session: Some(session_address),
            sponsor: None,
        };

        let stx = make_signed_tx_with_keypair(tx, &session_kp);
        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::SessionSpendingLimitExceeded));
    }

    #[test]
    fn stateful_session_valid_passes() {
        let session_kp = PolayKeypair::generate();
        let session_pubkey = session_kp.public_key().to_bytes();
        let session_address = session_kp.address();
        let granter_kp = PolayKeypair::generate();
        let granter = granter_kp.address();

        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(granter, 10_000, 0))
            .unwrap();

        let grant = make_session_grant(
            granter,
            &session_pubkey,
            session_address,
            polay_types::SessionPermission::All,
            10_000,
            1_000_000,
        );
        polay_state::StateWriter::new(&store)
            .set_session(&grant)
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: granter,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: Some(session_address),
            sponsor: None,
        };

        let stx = make_signed_tx_with_keypair(tx, &session_kp);
        assert!(validate_stateful(&stx, &store).is_ok());
    }

    // =======================================================================
    // Gas Sponsorship validation tests
    // =======================================================================

    #[test]
    fn stateless_sponsor_is_signer_rejected() {
        let kp = PolayKeypair::generate();
        let signer = kp.address();
        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: Some(signer), // can't sponsor yourself
        };
        let stx = make_signed_tx_with_keypair(tx, &kp);
        let err = validate_stateless(&stx, CHAIN_ID).unwrap_err();
        assert!(matches!(err, ExecutionError::SponsorIsSigner));
    }

    #[test]
    fn stateless_sponsor_zero_address_rejected() {
        // Zero-sponsor check runs before signature, so fake sig is fine.
        let stx = make_fake_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: test_addr(1),
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: Some(Address::ZERO),
        });
        let err = validate_stateless(&stx, CHAIN_ID).unwrap_err();
        assert!(matches!(err, ExecutionError::ZeroAddressSponsor));
    }

    #[test]
    fn stateful_sponsor_account_not_found_rejected() {
        let kp = PolayKeypair::generate();
        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(kp.address(), 10_000, 0))
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: kp.address(),
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: Some(test_addr(99)), // nonexistent sponsor
        };
        let stx = make_signed_tx_with_keypair(tx, &kp);
        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::SponsorAccountNotFound(_)));
    }

    #[test]
    fn stateful_sponsor_insufficient_balance_rejected() {
        let kp = PolayKeypair::generate();
        let sponsor = test_addr(99);
        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(kp.address(), 10_000, 0))
            .unwrap();
        // Sponsor exists but has insufficient balance.
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(sponsor, 50, 0))
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: kp.address(),
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: Some(sponsor),
        };
        let stx = make_signed_tx_with_keypair(tx, &kp);
        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }

    #[test]
    fn stateful_sponsored_signer_only_needs_transfer_amount() {
        // Signer has 200 balance: enough for transfer (100) but NOT for
        // transfer + fee (1000). With a sponsor, this should pass.
        let kp = PolayKeypair::generate();
        let sponsor = test_addr(99);
        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(kp.address(), 200, 0))
            .unwrap();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(sponsor, 50_000, 0))
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: kp.address(),
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: Some(sponsor),
        };
        let stx = make_signed_tx_with_keypair(tx, &kp);
        assert!(validate_stateful(&stx, &store).is_ok());
    }

    #[test]
    fn stateful_sponsored_signer_insufficient_for_transfer_rejected() {
        // Signer has 50 balance but transfer is 100. Even with a sponsor for the fee,
        // the signer can't afford the transfer action amount.
        let kp = PolayKeypair::generate();
        let sponsor = test_addr(99);
        let store = MemoryStore::new();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(kp.address(), 50, 0))
            .unwrap();
        polay_state::StateWriter::new(&store)
            .set_account(&AccountState::with_balance(sponsor, 50_000, 0))
            .unwrap();

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: kp.address(),
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: Some(sponsor),
        };
        let stx = make_signed_tx_with_keypair(tx, &kp);
        let err = validate_stateful(&stx, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }
}
