//! Asset class creation, minting, transfer, and burning.

use sha2::{Digest, Sha256};

use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{Address, AssetClass, AssetType, Event, Hash};
use tracing::debug;

use crate::error::ExecutionError;

// ---------------------------------------------------------------------------
// Create asset class
// ---------------------------------------------------------------------------

/// Create a new asset class.
///
/// The `asset_class_id` is derived as SHA-256(signer || name || timestamp)
/// so it is deterministic and collision-resistant.
pub fn execute_create_asset_class(
    signer: &Address,
    name: &str,
    symbol: &str,
    asset_type: AssetType,
    max_supply: Option<u64>,
    metadata_uri: &str,
    store: &dyn StateStore,
    timestamp: u64,
) -> Result<(Hash, Vec<Event>), ExecutionError> {
    // Derive deterministic ID.
    let mut hasher = Sha256::new();
    hasher.update(signer.as_bytes());
    hasher.update(name.as_bytes());
    hasher.update(timestamp.to_le_bytes());
    let digest = hasher.finalize();
    let mut id_bytes = [0u8; 32];
    id_bytes.copy_from_slice(&digest);
    let asset_class_id = Hash::new(id_bytes);

    // Ensure this class doesn't already exist.
    let view = StateView::new(store);
    if view.get_asset_class(&asset_class_id)?.is_some() {
        return Err(ExecutionError::AssetClassAlreadyExists);
    }

    let asset_class = AssetClass {
        id: asset_class_id,
        name: name.to_string(),
        symbol: symbol.to_string(),
        asset_type,
        total_supply: 0,
        max_supply,
        creator: *signer,
        metadata_uri: metadata_uri.to_string(),
        created_at: timestamp,
    };

    StateWriter::new(store).set_asset_class(&asset_class)?;

    debug!(
        asset_class_id = %asset_class_id,
        name,
        symbol,
        creator = %signer,
        "asset class created"
    );

    Ok((
        asset_class_id,
        vec![Event::asset_class_created(&asset_class_id, signer, name)],
    ))
}

// ---------------------------------------------------------------------------
// Mint asset
// ---------------------------------------------------------------------------

/// Mint new units of an existing asset class.
///
/// Only the original creator is authorized to mint.
pub fn execute_mint_asset(
    signer: &Address,
    asset_class_id: &Hash,
    to: &Address,
    amount: u64,
    _metadata: Option<&str>,
    store: &dyn StateStore,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    let mut asset_class = view
        .get_asset_class(asset_class_id)?
        .ok_or(ExecutionError::AssetClassNotFound)?;

    // Only creator can mint.
    if asset_class.creator != *signer {
        return Err(ExecutionError::Unauthorized);
    }

    // Check supply cap.
    if !asset_class.can_mint(amount) {
        return Err(ExecutionError::MaxSupplyExceeded {
            max: asset_class.max_supply.unwrap_or(u64::MAX),
            current: asset_class.total_supply,
            requested: amount,
        });
    }

    // Update supply.
    asset_class.total_supply = asset_class.total_supply.saturating_add(amount);
    writer.set_asset_class(&asset_class)?;

    // Credit recipient.
    let current_balance = view.get_asset_balance(asset_class_id, to)?;
    writer.set_asset_balance(asset_class_id, to, current_balance.saturating_add(amount))?;

    debug!(
        asset_class_id = %asset_class_id,
        to = %to,
        amount,
        "asset minted"
    );

    Ok(vec![Event::mint_asset(asset_class_id, to, amount)])
}

// ---------------------------------------------------------------------------
// Transfer asset
// ---------------------------------------------------------------------------

/// Transfer asset units from the signer to another address.
pub fn execute_transfer_asset(
    signer: &Address,
    asset_class_id: &Hash,
    to: &Address,
    amount: u64,
    store: &dyn StateStore,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Verify asset class exists.
    if view.get_asset_class(asset_class_id)?.is_none() {
        return Err(ExecutionError::AssetClassNotFound);
    }

    // Check sender balance.
    let sender_balance = view.get_asset_balance(asset_class_id, signer)?;
    if sender_balance < amount {
        return Err(ExecutionError::InsufficientBalance {
            required: amount,
            available: sender_balance,
        });
    }

    // Debit sender.
    writer.set_asset_balance(asset_class_id, signer, sender_balance - amount)?;

    // Credit receiver.
    let receiver_balance = view.get_asset_balance(asset_class_id, to)?;
    writer.set_asset_balance(asset_class_id, to, receiver_balance.saturating_add(amount))?;

    debug!(
        asset_class_id = %asset_class_id,
        from = %signer,
        to = %to,
        amount,
        "asset transferred"
    );

    Ok(vec![Event::transfer_asset(
        asset_class_id,
        signer,
        to,
        amount,
    )])
}

// ---------------------------------------------------------------------------
// Burn asset
// ---------------------------------------------------------------------------

/// Burn (destroy) asset units owned by the signer.
pub fn execute_burn_asset(
    signer: &Address,
    asset_class_id: &Hash,
    amount: u64,
    store: &dyn StateStore,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    let mut asset_class = view
        .get_asset_class(asset_class_id)?
        .ok_or(ExecutionError::AssetClassNotFound)?;

    // Check sender balance.
    let sender_balance = view.get_asset_balance(asset_class_id, signer)?;
    if sender_balance < amount {
        return Err(ExecutionError::InsufficientBalance {
            required: amount,
            available: sender_balance,
        });
    }

    // Debit sender.
    writer.set_asset_balance(asset_class_id, signer, sender_balance - amount)?;

    // Decrease total supply.
    asset_class.total_supply = asset_class.total_supply.saturating_sub(amount);
    writer.set_asset_class(&asset_class)?;

    debug!(
        asset_class_id = %asset_class_id,
        from = %signer,
        amount,
        "asset burned"
    );

    Ok(vec![Event::burn_asset(asset_class_id, signer, amount)])
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

    fn create_test_asset_class(store: &MemoryStore) -> Hash {
        let creator = test_addr(1);
        let (id, _) = execute_create_asset_class(
            &creator,
            "Gold Coin",
            "GLD",
            AssetType::Fungible,
            Some(10_000),
            "https://example.com/gold.json",
            store,
            1_000_000,
        )
        .unwrap();
        id
    }

    #[test]
    fn create_asset_class_happy_path() {
        let store = MemoryStore::new();
        let creator = test_addr(1);

        let (id, events) = execute_create_asset_class(
            &creator,
            "Gold Coin",
            "GLD",
            AssetType::Fungible,
            Some(10_000),
            "https://example.com/gold.json",
            &store,
            1_000_000,
        )
        .unwrap();

        assert!(!id.is_zero());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "asset");
        assert_eq!(events[0].action, "create_class");

        let ac = StateView::new(&store)
            .get_asset_class(&id)
            .unwrap()
            .unwrap();
        assert_eq!(ac.name, "Gold Coin");
        assert_eq!(ac.symbol, "GLD");
        assert_eq!(ac.creator, creator);
        assert_eq!(ac.total_supply, 0);
        assert_eq!(ac.max_supply, Some(10_000));
    }

    #[test]
    fn mint_asset_happy_path() {
        let store = MemoryStore::new();
        let creator = test_addr(1);
        let recipient = test_addr(2);
        let id = create_test_asset_class(&store);

        let events = execute_mint_asset(&creator, &id, &recipient, 500, None, &store).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "mint");

        let view = StateView::new(&store);
        assert_eq!(view.get_asset_balance(&id, &recipient).unwrap(), 500);
        assert_eq!(
            view.get_asset_class(&id).unwrap().unwrap().total_supply,
            500
        );
    }

    #[test]
    fn mint_asset_exceeds_supply() {
        let store = MemoryStore::new();
        let creator = test_addr(1);
        let id = create_test_asset_class(&store); // max_supply = 10_000

        let err =
            execute_mint_asset(&creator, &id, &test_addr(2), 10_001, None, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::MaxSupplyExceeded { .. }));
    }

    #[test]
    fn mint_asset_unauthorized() {
        let store = MemoryStore::new();
        let id = create_test_asset_class(&store);

        // Non-creator tries to mint.
        let err =
            execute_mint_asset(&test_addr(99), &id, &test_addr(2), 100, None, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::Unauthorized));
    }

    #[test]
    fn transfer_asset_happy_path() {
        let store = MemoryStore::new();
        let creator = test_addr(1);
        let alice = test_addr(2);
        let bob = test_addr(3);

        let id = create_test_asset_class(&store);
        execute_mint_asset(&creator, &id, &alice, 100, None, &store).unwrap();

        let events = execute_transfer_asset(&alice, &id, &bob, 40, &store).unwrap();
        assert_eq!(events.len(), 1);

        let view = StateView::new(&store);
        assert_eq!(view.get_asset_balance(&id, &alice).unwrap(), 60);
        assert_eq!(view.get_asset_balance(&id, &bob).unwrap(), 40);
    }

    #[test]
    fn transfer_asset_insufficient_balance() {
        let store = MemoryStore::new();
        let creator = test_addr(1);
        let alice = test_addr(2);
        let id = create_test_asset_class(&store);
        execute_mint_asset(&creator, &id, &alice, 50, None, &store).unwrap();

        let err = execute_transfer_asset(&alice, &id, &test_addr(3), 100, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }

    #[test]
    fn burn_asset_happy_path() {
        let store = MemoryStore::new();
        let creator = test_addr(1);
        let alice = test_addr(2);
        let id = create_test_asset_class(&store);
        execute_mint_asset(&creator, &id, &alice, 200, None, &store).unwrap();

        let events = execute_burn_asset(&alice, &id, 50, &store).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "burn");

        let view = StateView::new(&store);
        assert_eq!(view.get_asset_balance(&id, &alice).unwrap(), 150);
        assert_eq!(
            view.get_asset_class(&id).unwrap().unwrap().total_supply,
            150
        );
    }

    #[test]
    fn burn_asset_insufficient() {
        let store = MemoryStore::new();
        let creator = test_addr(1);
        let alice = test_addr(2);
        let id = create_test_asset_class(&store);
        execute_mint_asset(&creator, &id, &alice, 30, None, &store).unwrap();

        let err = execute_burn_asset(&alice, &id, 50, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }
}
