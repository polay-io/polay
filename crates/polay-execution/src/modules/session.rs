//! Session key management -- create and revoke temporary signing keys.

use sha2::{Digest, Sha256};

use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{Address, Event, SessionGrant, SessionPermission};
use tracing::debug;

use crate::error::ExecutionError;

/// Execute a `CreateSession` action.
///
/// The signer (account owner) authorizes a temporary public key to sign
/// transactions on their behalf within the specified constraints.
pub fn execute_create_session(
    signer: &Address,
    session_pubkey: Vec<u8>,
    permissions: SessionPermission,
    expires_at: u64,
    spending_limit: u64,
    store: &dyn StateStore,
    current_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    // Validate pubkey is 32 bytes.
    if session_pubkey.len() != 32 {
        return Err(ExecutionError::InvalidSessionPubkey(format!(
            "expected 32 bytes, got {}",
            session_pubkey.len(),
        )));
    }

    // Derive session_address from session_pubkey (SHA-256).
    let digest = Sha256::digest(&session_pubkey);
    let mut addr_bytes = [0u8; 32];
    addr_bytes.copy_from_slice(&digest[..32]);
    let session_address = Address::new(addr_bytes);

    // Check expires_at is in the future.
    if expires_at <= current_height {
        return Err(ExecutionError::InvalidInput(format!(
            "session expires_at ({}) must be greater than current height ({})",
            expires_at, current_height,
        )));
    }

    // Check no existing active session for this pubkey.
    let view = StateView::new(store);
    if let Some(existing) = view.get_session(signer, &session_address)? {
        if existing.is_valid(current_height) {
            return Err(ExecutionError::SessionAlreadyExists);
        }
    }

    // Create and store the grant.
    let grant = SessionGrant {
        granter: *signer,
        session_pubkey,
        session_address,
        permissions,
        expires_at,
        spending_limit,
        amount_spent: 0,
        revoked: false,
        created_at: current_height,
    };

    StateWriter::new(store).set_session(&grant)?;

    debug!(
        granter = %signer,
        session_address = %session_address,
        expires_at,
        "session key created"
    );

    Ok(vec![Event::session_created(
        signer,
        &session_address,
        expires_at,
    )])
}

/// Execute a `RevokeSession` action.
///
/// The signer (account owner) revokes an existing session key.
pub fn execute_revoke_session(
    signer: &Address,
    session_address: &Address,
    store: &dyn StateStore,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);

    let mut grant = view
        .get_session(signer, session_address)?
        .ok_or(ExecutionError::SessionNotFound)?;

    // Verify the signer is the granter.
    if grant.granter != *signer {
        return Err(ExecutionError::Unauthorized);
    }

    if grant.revoked {
        return Err(ExecutionError::SessionRevoked);
    }

    grant.revoked = true;
    StateWriter::new(store).set_session(&grant)?;

    debug!(
        granter = %signer,
        session_address = %session_address,
        "session key revoked"
    );

    Ok(vec![Event::session_revoked(signer, session_address)])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::MemoryStore;

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn session_pubkey() -> Vec<u8> {
        vec![0xAA; 32]
    }

    fn session_addr_from_pubkey(pubkey: &[u8]) -> Address {
        let digest = Sha256::digest(pubkey);
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&digest[..32]);
        Address::new(buf)
    }

    #[test]
    fn create_session_happy_path() {
        let store = MemoryStore::new();
        let granter = test_addr(1);
        let pubkey = session_pubkey();
        let expected_session_addr = session_addr_from_pubkey(&pubkey);

        let events = execute_create_session(
            &granter,
            pubkey.clone(),
            SessionPermission::All,
            1000,
            500_000,
            &store,
            100,
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "session");
        assert_eq!(events[0].action, "session_created");

        let view = StateView::new(&store);
        let grant = view
            .get_session(&granter, &expected_session_addr)
            .unwrap()
            .unwrap();
        assert_eq!(grant.granter, granter);
        assert_eq!(grant.session_pubkey, pubkey);
        assert_eq!(grant.session_address, expected_session_addr);
        assert_eq!(grant.expires_at, 1000);
        assert_eq!(grant.spending_limit, 500_000);
        assert_eq!(grant.amount_spent, 0);
        assert!(!grant.revoked);
        assert_eq!(grant.created_at, 100);
    }

    #[test]
    fn create_session_invalid_pubkey_length() {
        let store = MemoryStore::new();
        let err = execute_create_session(
            &test_addr(1),
            vec![0u8; 16], // wrong length
            SessionPermission::All,
            1000,
            500_000,
            &store,
            100,
        )
        .unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidSessionPubkey(_)));
    }

    #[test]
    fn create_session_expired_height() {
        let store = MemoryStore::new();
        let err = execute_create_session(
            &test_addr(1),
            session_pubkey(),
            SessionPermission::All,
            50, // in the past
            500_000,
            &store,
            100,
        )
        .unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidInput(_)));
    }

    #[test]
    fn create_session_duplicate_rejected() {
        let store = MemoryStore::new();
        let granter = test_addr(1);
        let pubkey = session_pubkey();

        execute_create_session(
            &granter,
            pubkey.clone(),
            SessionPermission::All,
            1000,
            500_000,
            &store,
            100,
        )
        .unwrap();

        // Second creation should fail.
        let err = execute_create_session(
            &granter,
            pubkey,
            SessionPermission::All,
            2000,
            500_000,
            &store,
            200,
        )
        .unwrap_err();
        assert!(matches!(err, ExecutionError::SessionAlreadyExists));
    }

    #[test]
    fn create_session_with_specific_permissions() {
        let store = MemoryStore::new();
        let granter = test_addr(1);
        let pubkey = session_pubkey();
        let expected_addr = session_addr_from_pubkey(&pubkey);

        execute_create_session(
            &granter,
            pubkey,
            SessionPermission::Actions(vec!["transfer".into(), "buy_listing".into()]),
            1000,
            100_000,
            &store,
            50,
        )
        .unwrap();

        let view = StateView::new(&store);
        let grant = view.get_session(&granter, &expected_addr).unwrap().unwrap();
        assert!(grant.is_action_permitted("transfer"));
        assert!(grant.is_action_permitted("buy_listing"));
        assert!(!grant.is_action_permitted("create_listing"));
    }

    #[test]
    fn revoke_session_happy_path() {
        let store = MemoryStore::new();
        let granter = test_addr(1);
        let pubkey = session_pubkey();
        let session_addr = session_addr_from_pubkey(&pubkey);

        // Create first.
        execute_create_session(
            &granter,
            pubkey,
            SessionPermission::All,
            1000,
            500_000,
            &store,
            100,
        )
        .unwrap();

        // Revoke.
        let events = execute_revoke_session(&granter, &session_addr, &store).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "session");
        assert_eq!(events[0].action, "session_revoked");

        let view = StateView::new(&store);
        let grant = view.get_session(&granter, &session_addr).unwrap().unwrap();
        assert!(grant.revoked);
    }

    #[test]
    fn revoke_nonexistent_session() {
        let store = MemoryStore::new();
        let err = execute_revoke_session(&test_addr(1), &test_addr(99), &store).unwrap_err();
        assert!(matches!(err, ExecutionError::SessionNotFound));
    }

    #[test]
    fn revoke_already_revoked_session() {
        let store = MemoryStore::new();
        let granter = test_addr(1);
        let pubkey = session_pubkey();
        let session_addr = session_addr_from_pubkey(&pubkey);

        execute_create_session(
            &granter,
            pubkey,
            SessionPermission::All,
            1000,
            500_000,
            &store,
            100,
        )
        .unwrap();

        execute_revoke_session(&granter, &session_addr, &store).unwrap();

        // Second revoke should fail.
        let err = execute_revoke_session(&granter, &session_addr, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::SessionRevoked));
    }

    #[test]
    fn session_address_derivation_is_correct() {
        let pubkey = session_pubkey();
        let digest = Sha256::digest(&pubkey);
        let expected = Address::new(digest.into());
        let derived = session_addr_from_pubkey(&pubkey);
        assert_eq!(expected, derived);
    }
}
