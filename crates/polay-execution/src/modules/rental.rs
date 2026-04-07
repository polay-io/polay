//! Rental module — listing, renting, returning, and claiming expired game asset rentals.

use sha2::{Digest, Sha256};

use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{AccountState, Address, Event, Hash, Rental, RentalStatus};
use tracing::debug;

use crate::error::ExecutionError;

// ---------------------------------------------------------------------------
// List asset for rent
// ---------------------------------------------------------------------------

/// List an asset for rent.
///
/// The `rental_id` is derived as SHA-256(owner || asset_id || timestamp)
/// so it is deterministic and collision-resistant.
pub fn execute_list_for_rent(
    signer: &Address,
    asset_class_id: &Hash,
    asset_id: &Hash,
    price_per_block: u64,
    deposit: u64,
    min_duration: u64,
    max_duration: u64,
    store: &dyn StateStore,
    timestamp: u64,
) -> Result<(Hash, Vec<Event>), ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Verify the asset class exists.
    let _asset_class = view
        .get_asset_class(asset_class_id)?
        .ok_or(ExecutionError::AssetClassNotFound)?;

    // Verify signer owns at least one unit of this asset class.
    let balance = view.get_asset_balance(asset_class_id, signer)?;
    if balance == 0 {
        return Err(ExecutionError::Unauthorized);
    }

    // Derive deterministic rental ID.
    let mut hasher = Sha256::new();
    hasher.update(signer.as_bytes());
    hasher.update(asset_id.as_bytes());
    hasher.update(timestamp.to_le_bytes());
    let digest = hasher.finalize();
    let mut id_bytes = [0u8; 32];
    id_bytes.copy_from_slice(&digest);
    let rental_id = Hash::new(id_bytes);

    let rental = Rental {
        rental_id,
        owner: *signer,
        renter: None,
        asset_class_id: *asset_class_id,
        asset_id: *asset_id,
        price_per_block,
        deposit,
        min_duration,
        max_duration,
        start_height: None,
        end_height: None,
        status: RentalStatus::Listed,
        created_at: timestamp,
    };

    writer.set_rental(&rental)?;

    debug!(
        rental_id = %rental_id,
        owner = %signer,
        asset_id = %asset_id,
        price_per_block,
        deposit,
        "rental listed"
    );

    Ok((
        rental_id,
        vec![Event::rental_listed(signer, &rental_id, asset_id)],
    ))
}

// ---------------------------------------------------------------------------
// Rent asset
// ---------------------------------------------------------------------------

/// Rent a listed asset.
///
/// The renter pays `price_per_block * duration` to the owner immediately,
/// plus a deposit that is held until the rental ends.
pub fn execute_rent_asset(
    signer: &Address,
    rental_id: &Hash,
    duration: u64,
    store: &dyn StateStore,
    block_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    let mut rental = view
        .get_rental(rental_id)?
        .ok_or(ExecutionError::RentalNotFound)?;

    // Must be in Listed status.
    if rental.status != RentalStatus::Listed {
        return Err(ExecutionError::RentalNotListed);
    }

    // Validate duration within [min_duration, max_duration].
    if duration < rental.min_duration {
        return Err(ExecutionError::InvalidRentalDuration {
            reason: format!(
                "duration {} is below minimum {}",
                duration, rental.min_duration
            ),
        });
    }
    if duration > rental.max_duration {
        return Err(ExecutionError::InvalidRentalDuration {
            reason: format!(
                "duration {} exceeds maximum {}",
                duration, rental.max_duration
            ),
        });
    }

    // Calculate cost.
    let rental_cost = rental.price_per_block.saturating_mul(duration);
    let total_cost = rental_cost.saturating_add(rental.deposit);

    // Load renter account and verify balance.
    let mut renter_account = view
        .get_account(signer)?
        .ok_or_else(|| ExecutionError::AccountNotFound(signer.to_hex()))?;

    if renter_account.balance < total_cost {
        return Err(ExecutionError::InsufficientBalance {
            required: total_cost,
            available: renter_account.balance,
        });
    }

    // Deduct total cost from renter.
    renter_account.balance = renter_account.balance
        .checked_sub(total_cost)
        .ok_or(ExecutionError::InsufficientBalance {
            required: total_cost,
            available: renter_account.balance,
        })?;
    writer.set_account(&renter_account)?;

    // Credit rental cost (price_per_block * duration) to owner immediately.
    let mut owner_account = view
        .get_account(&rental.owner)?
        .unwrap_or_else(|| AccountState::new(rental.owner, 0));
    owner_account.balance = owner_account.balance.saturating_add(rental_cost);
    writer.set_account(&owner_account)?;

    // Update rental record.
    rental.renter = Some(*signer);
    rental.start_height = Some(block_height);
    rental.end_height = Some(block_height.saturating_add(duration));
    rental.status = RentalStatus::Active;

    writer.set_rental(&rental)?;

    debug!(
        rental_id = %rental_id,
        renter = %signer,
        duration,
        total_cost,
        start_height = block_height,
        end_height = block_height + duration,
        "asset rented"
    );

    Ok(vec![Event::asset_rented(signer, rental_id, duration)])
}

// ---------------------------------------------------------------------------
// Return rental
// ---------------------------------------------------------------------------

/// Return a rented asset early. The renter receives a refund for the remaining
/// blocks plus their full deposit.
pub fn execute_return_rental(
    signer: &Address,
    rental_id: &Hash,
    store: &dyn StateStore,
    block_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    let mut rental = view
        .get_rental(rental_id)?
        .ok_or(ExecutionError::RentalNotFound)?;

    // Must be active.
    if rental.status != RentalStatus::Active {
        return Err(ExecutionError::RentalNotActive);
    }

    // Only the renter can return.
    let renter_addr = rental
        .renter
        .as_ref()
        .ok_or(ExecutionError::RentalNotActive)?;
    if signer != renter_addr {
        return Err(ExecutionError::RentalRenterMismatch);
    }

    // Calculate refund for remaining blocks.
    let end_height = rental.end_height.unwrap_or(block_height);
    let remaining_blocks = if block_height < end_height {
        end_height - block_height
    } else {
        0
    };
    let rental_refund = remaining_blocks.saturating_mul(rental.price_per_block);
    let total_refund = rental_refund.saturating_add(rental.deposit);

    // Credit refund + deposit to renter.
    let mut renter_account = view
        .get_account(signer)?
        .unwrap_or_else(|| AccountState::new(*signer, 0));
    renter_account.balance = renter_account.balance.saturating_add(total_refund);
    writer.set_account(&renter_account)?;

    // Debit the rental refund from the owner (they were pre-paid).
    // The deposit was never credited to the owner, so only debit rental_refund.
    if rental_refund > 0 {
        let mut owner_account = view
            .get_account(&rental.owner)?
            .unwrap_or_else(|| AccountState::new(rental.owner, 0));
        owner_account.balance = owner_account.balance.saturating_sub(rental_refund);
        writer.set_account(&owner_account)?;
    }

    // Update rental status.
    rental.status = RentalStatus::Returned;
    writer.set_rental(&rental)?;

    debug!(
        rental_id = %rental_id,
        renter = %signer,
        remaining_blocks,
        rental_refund,
        deposit_refund = rental.deposit,
        "rental returned"
    );

    Ok(vec![Event::rental_returned(signer, rental_id)])
}

// ---------------------------------------------------------------------------
// Claim expired rental
// ---------------------------------------------------------------------------

/// Claim an asset back from an expired rental. The deposit is returned to
/// the renter.
pub fn execute_claim_expired_rental(
    signer: &Address,
    rental_id: &Hash,
    store: &dyn StateStore,
    block_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    let mut rental = view
        .get_rental(rental_id)?
        .ok_or(ExecutionError::RentalNotFound)?;

    // Must be active.
    if rental.status != RentalStatus::Active {
        return Err(ExecutionError::RentalNotActive);
    }

    // Must be expired: current_height >= end_height.
    let end_height = rental.end_height.unwrap_or(0);
    if block_height < end_height {
        return Err(ExecutionError::RentalNotExpired);
    }

    // Refund deposit to renter.
    if let Some(renter_addr) = &rental.renter {
        if rental.deposit > 0 {
            let mut renter_account = view
                .get_account(renter_addr)?
                .unwrap_or_else(|| AccountState::new(*renter_addr, 0));
            renter_account.balance = renter_account.balance.saturating_add(rental.deposit);
            writer.set_account(&renter_account)?;
        }
    }

    // Update rental status.
    rental.status = RentalStatus::Expired;
    writer.set_rental(&rental)?;

    debug!(
        rental_id = %rental_id,
        signer = %signer,
        deposit_refunded = rental.deposit,
        "expired rental claimed"
    );

    Ok(vec![Event::rental_expired(rental_id)])
}

// ---------------------------------------------------------------------------
// Cancel rental listing
// ---------------------------------------------------------------------------

/// Cancel a rental listing that has not been rented.
pub fn execute_cancel_rental_listing(
    signer: &Address,
    rental_id: &Hash,
    store: &dyn StateStore,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    let mut rental = view
        .get_rental(rental_id)?
        .ok_or(ExecutionError::RentalNotFound)?;

    // Must be in Listed status.
    if rental.status != RentalStatus::Listed {
        return Err(ExecutionError::RentalNotListed);
    }

    // Only the owner can cancel.
    if rental.owner != *signer {
        return Err(ExecutionError::RentalOwnerMismatch);
    }

    // Mark as cancelled.
    rental.status = RentalStatus::Cancelled;
    writer.set_rental(&rental)?;

    debug!(
        rental_id = %rental_id,
        owner = %signer,
        "rental listing cancelled"
    );

    Ok(vec![Event::rental_cancelled(signer, rental_id)])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::assets;
    use polay_state::MemoryStore;
    use polay_types::AssetType;

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    /// Set up a store with:
    /// - An asset class created by `owner` (addr byte 1)
    /// - Owner has 100 units of the asset
    /// - Owner has an account with 50_000 native balance
    /// Returns (asset_class_id, asset_id) where asset_id == asset_class_id for simplicity.
    fn setup_store(store: &MemoryStore) -> (Hash, Hash) {
        let owner = test_addr(1);

        // Create asset class.
        let (asset_class_id, _) = assets::execute_create_asset_class(
            &owner,
            "DragonSword",
            "DSW",
            AssetType::NonFungible,
            Some(1000),
            "https://meta",
            store,
            100,
        )
        .unwrap();

        // Mint some to owner.
        assets::execute_mint_asset(&owner, &asset_class_id, &owner, 100, None, store).unwrap();

        // Create owner account with native balance.
        StateWriter::new(store)
            .set_account(&AccountState::with_balance(owner, 50_000, 0))
            .unwrap();

        // Use asset_class_id as asset_id for simplicity.
        (asset_class_id, asset_class_id)
    }

    /// Create a rental listing and return (asset_class_id, asset_id, rental_id).
    fn setup_listing(store: &MemoryStore) -> (Hash, Hash, Hash) {
        let (asset_class_id, asset_id) = setup_store(store);
        let owner = test_addr(1);

        let (rental_id, _) = execute_list_for_rent(
            &owner,
            &asset_class_id,
            &asset_id,
            100,  // price_per_block
            1000, // deposit
            10,   // min_duration
            100,  // max_duration
            store,
            200,
        )
        .unwrap();

        (asset_class_id, asset_id, rental_id)
    }

    /// Create a listing and then rent it. Returns (asset_class_id, rental_id).
    fn setup_rented(store: &MemoryStore) -> (Hash, Hash) {
        let (asset_class_id, _asset_id, rental_id) = setup_listing(store);
        let renter = test_addr(2);

        // Give renter enough balance.
        StateWriter::new(store)
            .set_account(&AccountState::with_balance(renter, 100_000, 0))
            .unwrap();

        // Rent for 50 blocks at block_height=1000.
        execute_rent_asset(&renter, &rental_id, 50, store, 1000).unwrap();

        (asset_class_id, rental_id)
    }

    // -----------------------------------------------------------------------
    // 1. List asset for rent
    // -----------------------------------------------------------------------

    #[test]
    fn list_for_rent_happy_path() {
        let store = MemoryStore::new();
        let (asset_class_id, asset_id) = setup_store(&store);
        let owner = test_addr(1);

        let (rental_id, events) = execute_list_for_rent(
            &owner,
            &asset_class_id,
            &asset_id,
            100,
            1000,
            10,
            100,
            &store,
            200,
        )
        .unwrap();

        // Check event.
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "rental");
        assert_eq!(events[0].action, "rental_listed");

        // Verify rental in state.
        let view = StateView::new(&store);
        let rental = view.get_rental(&rental_id).unwrap().unwrap();
        assert_eq!(rental.owner, owner);
        assert_eq!(rental.asset_class_id, asset_class_id);
        assert_eq!(rental.asset_id, asset_id);
        assert_eq!(rental.price_per_block, 100);
        assert_eq!(rental.deposit, 1000);
        assert_eq!(rental.min_duration, 10);
        assert_eq!(rental.max_duration, 100);
        assert_eq!(rental.status, RentalStatus::Listed);
        assert!(rental.renter.is_none());
        assert!(rental.start_height.is_none());
        assert!(rental.end_height.is_none());
        assert_eq!(rental.created_at, 200);
    }

    // -----------------------------------------------------------------------
    // 2. Rent an asset
    // -----------------------------------------------------------------------

    #[test]
    fn rent_asset_happy_path() {
        let store = MemoryStore::new();
        let (_asset_class_id, _asset_id, rental_id) = setup_listing(&store);
        let renter = test_addr(2);
        let owner = test_addr(1);

        // Give renter enough balance.
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(renter, 100_000, 0))
            .unwrap();

        let duration = 50u64;
        let block_height = 1000u64;
        let events =
            execute_rent_asset(&renter, &rental_id, duration, &store, block_height).unwrap();

        // Check event.
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "rental");
        assert_eq!(events[0].action, "asset_rented");

        // Verify rental in state.
        let view = StateView::new(&store);
        let rental = view.get_rental(&rental_id).unwrap().unwrap();
        assert_eq!(rental.status, RentalStatus::Active);
        assert_eq!(rental.renter, Some(renter));
        assert_eq!(rental.start_height, Some(1000));
        assert_eq!(rental.end_height, Some(1050));

        // Verify funds transferred.
        // rental_cost = 100 * 50 = 5000, deposit = 1000, total_cost = 6000
        let renter_acct = view.get_account(&renter).unwrap().unwrap();
        assert_eq!(renter_acct.balance, 100_000 - 6000);

        // Owner receives rental_cost (5000) on top of existing 50_000.
        let owner_acct = view.get_account(&owner).unwrap().unwrap();
        assert_eq!(owner_acct.balance, 50_000 + 5000);
    }

    // -----------------------------------------------------------------------
    // 3. Return early
    // -----------------------------------------------------------------------

    #[test]
    fn return_rental_early() {
        let store = MemoryStore::new();
        let (_asset_class_id, rental_id) = setup_rented(&store);
        let renter = test_addr(2);
        let owner = test_addr(1);

        let view = StateView::new(&store);

        // Before return: renter balance = 100_000 - 6000 = 94_000
        // owner balance = 50_000 + 5000 = 55_000
        let renter_before = view.get_account(&renter).unwrap().unwrap().balance;
        let owner_before = view.get_account(&owner).unwrap().unwrap().balance;
        assert_eq!(renter_before, 94_000);
        assert_eq!(owner_before, 55_000);

        // Return at block 1020 (30 blocks remaining out of 50).
        let events = execute_return_rental(&renter, &rental_id, &store, 1020).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "rental_returned");

        let view = StateView::new(&store);
        let rental = view.get_rental(&rental_id).unwrap().unwrap();
        assert_eq!(rental.status, RentalStatus::Returned);

        // remaining = 1050 - 1020 = 30 blocks
        // rental_refund = 30 * 100 = 3000
        // total_refund = 3000 + 1000 (deposit) = 4000
        let renter_after = view.get_account(&renter).unwrap().unwrap().balance;
        assert_eq!(renter_after, 94_000 + 4000);

        // Owner loses the rental_refund (3000) from their pre-payment.
        let owner_after = view.get_account(&owner).unwrap().unwrap().balance;
        assert_eq!(owner_after, 55_000 - 3000);
    }

    // -----------------------------------------------------------------------
    // 4. Claim expired rental
    // -----------------------------------------------------------------------

    #[test]
    fn claim_expired_rental() {
        let store = MemoryStore::new();
        let (_asset_class_id, rental_id) = setup_rented(&store);
        let renter = test_addr(2);
        let owner = test_addr(1);

        let view = StateView::new(&store);
        let renter_before = view.get_account(&renter).unwrap().unwrap().balance;

        // Claim at block 1050 (exactly at end_height).
        let events =
            execute_claim_expired_rental(&owner, &rental_id, &store, 1050).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "rental_expired");

        let view = StateView::new(&store);
        let rental = view.get_rental(&rental_id).unwrap().unwrap();
        assert_eq!(rental.status, RentalStatus::Expired);

        // Deposit (1000) returned to renter.
        let renter_after = view.get_account(&renter).unwrap().unwrap().balance;
        assert_eq!(renter_after, renter_before + 1000);
    }

    // -----------------------------------------------------------------------
    // 5. Cancel listing
    // -----------------------------------------------------------------------

    #[test]
    fn cancel_listing_happy_path() {
        let store = MemoryStore::new();
        let (_asset_class_id, _asset_id, rental_id) = setup_listing(&store);
        let owner = test_addr(1);

        let events = execute_cancel_rental_listing(&owner, &rental_id, &store).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "rental_cancelled");

        let view = StateView::new(&store);
        let rental = view.get_rental(&rental_id).unwrap().unwrap();
        assert_eq!(rental.status, RentalStatus::Cancelled);
    }

    // -----------------------------------------------------------------------
    // 6. Rent with insufficient funds
    // -----------------------------------------------------------------------

    #[test]
    fn rent_insufficient_funds() {
        let store = MemoryStore::new();
        let (_asset_class_id, _asset_id, rental_id) = setup_listing(&store);
        let renter = test_addr(2);

        // Give renter only 100 tokens (need 6000 for 50 blocks).
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(renter, 100, 0))
            .unwrap();

        let err = execute_rent_asset(&renter, &rental_id, 50, &store, 1000).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }

    // -----------------------------------------------------------------------
    // 7. Rent duration below minimum
    // -----------------------------------------------------------------------

    #[test]
    fn rent_duration_below_minimum() {
        let store = MemoryStore::new();
        let (_asset_class_id, _asset_id, rental_id) = setup_listing(&store);
        let renter = test_addr(2);

        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(renter, 100_000, 0))
            .unwrap();

        // min_duration is 10, try with 5.
        let err = execute_rent_asset(&renter, &rental_id, 5, &store, 1000).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidRentalDuration { .. }));
    }

    // -----------------------------------------------------------------------
    // 8. Rent duration above maximum
    // -----------------------------------------------------------------------

    #[test]
    fn rent_duration_above_maximum() {
        let store = MemoryStore::new();
        let (_asset_class_id, _asset_id, rental_id) = setup_listing(&store);
        let renter = test_addr(2);

        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(renter, 1_000_000, 0))
            .unwrap();

        // max_duration is 100, try with 200.
        let err = execute_rent_asset(&renter, &rental_id, 200, &store, 1000).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidRentalDuration { .. }));
    }

    // -----------------------------------------------------------------------
    // 9. Return by non-renter
    // -----------------------------------------------------------------------

    #[test]
    fn return_by_non_renter() {
        let store = MemoryStore::new();
        let (_asset_class_id, rental_id) = setup_rented(&store);
        let imposter = test_addr(99);

        let err = execute_return_rental(&imposter, &rental_id, &store, 1020).unwrap_err();
        assert!(matches!(err, ExecutionError::RentalRenterMismatch));
    }

    // -----------------------------------------------------------------------
    // 10. Rent already active rental
    // -----------------------------------------------------------------------

    #[test]
    fn rent_already_active() {
        let store = MemoryStore::new();
        let (_asset_class_id, rental_id) = setup_rented(&store);
        let another_renter = test_addr(3);

        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(another_renter, 100_000, 0))
            .unwrap();

        // Rental is already Active, not Listed.
        let err = execute_rent_asset(&another_renter, &rental_id, 50, &store, 2000).unwrap_err();
        assert!(matches!(err, ExecutionError::RentalNotListed));
    }

    // -----------------------------------------------------------------------
    // 11. Cancel by non-owner
    // -----------------------------------------------------------------------

    #[test]
    fn cancel_by_non_owner() {
        let store = MemoryStore::new();
        let (_asset_class_id, _asset_id, rental_id) = setup_listing(&store);
        let imposter = test_addr(99);

        let err = execute_cancel_rental_listing(&imposter, &rental_id, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::RentalOwnerMismatch));
    }
}
