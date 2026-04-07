//! Marketplace module — listing, cancelling, and buying game assets.

use sha2::{Digest, Sha256};

use polay_config::ChainConfig;
use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{AccountState, Address, Event, Hash, Listing, ListingStatus};
use tracing::debug;

use crate::error::ExecutionError;

// ---------------------------------------------------------------------------
// Create listing
// ---------------------------------------------------------------------------

/// List assets for sale at a fixed per-unit price.
///
/// The listed assets are moved from the seller into escrow (i.e., debited
/// from the seller's asset balance and held inside the listing record).
pub fn execute_create_listing(
    signer: &Address,
    asset_class_id: &Hash,
    amount: u64,
    price_per_unit: u64,
    currency: &Hash,
    store: &dyn StateStore,
    config: &ChainConfig,
    timestamp: u64,
) -> Result<(Hash, Vec<Event>), ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Verify asset class exists and fetch royalty info.
    let asset_class = view
        .get_asset_class(asset_class_id)?
        .ok_or(ExecutionError::AssetClassNotFound)?;

    // Check seller has enough of the asset.
    let seller_balance = view.get_asset_balance(asset_class_id, signer)?;
    if seller_balance < amount {
        return Err(ExecutionError::InsufficientBalance {
            required: amount,
            available: seller_balance,
        });
    }

    // Escrow: debit the asset from the seller.
    writer.set_asset_balance(asset_class_id, signer, seller_balance - amount)?;

    // Derive a deterministic listing ID.
    let mut hasher = Sha256::new();
    hasher.update(signer.as_bytes());
    hasher.update(asset_class_id.as_bytes());
    hasher.update(amount.to_le_bytes());
    hasher.update(price_per_unit.to_le_bytes());
    hasher.update(timestamp.to_le_bytes());
    let digest = hasher.finalize();
    let mut id_bytes = [0u8; 32];
    id_bytes.copy_from_slice(&digest);
    let listing_id = Hash::new(id_bytes);

    // Default royalty: use protocol_fee_bps as a baseline for the creator.
    // In a production system this would be configurable per-asset-class.
    let royalty_bps = config.protocol_fee_bps;

    let listing = Listing {
        id: listing_id,
        seller: *signer,
        asset_class_id: *asset_class_id,
        amount,
        price_per_unit,
        currency: *currency,
        status: ListingStatus::Active,
        royalty_bps,
        created_at: timestamp,
    };

    writer.set_listing(&listing)?;

    debug!(
        listing_id = %listing_id,
        seller = %signer,
        asset_class_id = %asset_class_id,
        amount,
        price_per_unit,
        "listing created"
    );

    let _ = asset_class; // used earlier

    Ok((
        listing_id,
        vec![Event::listing_created(
            &listing_id,
            signer,
            asset_class_id,
            amount,
            price_per_unit,
        )],
    ))
}

// ---------------------------------------------------------------------------
// Cancel listing
// ---------------------------------------------------------------------------

/// Cancel an active listing and return the escrowed assets to the seller.
pub fn execute_cancel_listing(
    signer: &Address,
    listing_id: &Hash,
    store: &dyn StateStore,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    let mut listing = view
        .get_listing(listing_id)?
        .ok_or(ExecutionError::ListingNotFound)?;

    if !listing.is_active() {
        return Err(ExecutionError::ListingNotActive);
    }
    if listing.seller != *signer {
        return Err(ExecutionError::ListingOwnerMismatch);
    }

    // Return escrowed assets to seller.
    let seller_balance = view.get_asset_balance(&listing.asset_class_id, signer)?;
    writer.set_asset_balance(
        &listing.asset_class_id,
        signer,
        seller_balance.saturating_add(listing.amount),
    )?;

    listing.status = ListingStatus::Cancelled;
    writer.set_listing(&listing)?;

    debug!(listing_id = %listing_id, seller = %signer, "listing cancelled");

    Ok(vec![Event::listing_cancelled(listing_id, signer)])
}

// ---------------------------------------------------------------------------
// Buy listing
// ---------------------------------------------------------------------------

/// Purchase an active listing. The full quantity is bought.
///
/// Fee breakdown:
/// 1. `protocol_fee` = total_price * protocol_fee_bps / 10_000
/// 2. `royalty`       = total_price * royalty_bps / 10_000
/// 3. `seller_proceeds` = total_price - protocol_fee - royalty
///
/// The protocol fee goes to the zero address (treasury).
/// The royalty goes to the asset class creator.
pub fn execute_buy_listing(
    buyer: &Address,
    listing_id: &Hash,
    store: &dyn StateStore,
    config: &ChainConfig,
    timestamp: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    let mut listing = view
        .get_listing(listing_id)?
        .ok_or(ExecutionError::ListingNotFound)?;

    if !listing.is_active() {
        return Err(ExecutionError::ListingNotActive);
    }
    if listing.seller == *buyer {
        return Err(ExecutionError::CannotBuyOwnListing);
    }

    let total_price = listing.total_price();

    // Calculate fees.
    let protocol_fee = ((total_price as u128) * config.protocol_fee_bps as u128 / 10_000) as u64;
    let royalty = listing.royalty_amount();
    let seller_proceeds = total_price.saturating_sub(protocol_fee).saturating_sub(royalty);

    // Debit buyer's native balance.
    let mut buyer_account = view
        .get_account(buyer)?
        .ok_or_else(|| ExecutionError::AccountNotFound(buyer.to_hex()))?;
    if buyer_account.balance < total_price {
        return Err(ExecutionError::InsufficientBalance {
            required: total_price,
            available: buyer_account.balance,
        });
    }
    buyer_account.balance = buyer_account.balance
        .checked_sub(total_price)
        .ok_or(ExecutionError::InsufficientBalance {
            required: total_price,
            available: buyer_account.balance,
        })?;
    writer.set_account(&buyer_account)?;

    // Credit seller.
    let mut seller_account = view
        .get_account(&listing.seller)?
        .unwrap_or_else(|| AccountState::new(listing.seller, timestamp));
    seller_account.balance = seller_account.balance.saturating_add(seller_proceeds);
    writer.set_account(&seller_account)?;

    // Credit protocol treasury (Address::ZERO).
    if protocol_fee > 0 {
        let mut treasury = view
            .get_account(&Address::ZERO)?
            .unwrap_or_else(|| AccountState::new(Address::ZERO, timestamp));
        treasury.balance = treasury.balance.saturating_add(protocol_fee);
        writer.set_account(&treasury)?;
    }

    // Credit royalty to asset class creator.
    if royalty > 0 {
        let asset_class = view
            .get_asset_class(&listing.asset_class_id)?
            .ok_or(ExecutionError::AssetClassNotFound)?;
        let mut creator_account = view
            .get_account(&asset_class.creator)?
            .unwrap_or_else(|| AccountState::new(asset_class.creator, timestamp));
        creator_account.balance = creator_account.balance.saturating_add(royalty);
        writer.set_account(&creator_account)?;
    }

    // Transfer the escrowed assets to the buyer.
    let buyer_asset_balance = view.get_asset_balance(&listing.asset_class_id, buyer)?;
    writer.set_asset_balance(
        &listing.asset_class_id,
        buyer,
        buyer_asset_balance.saturating_add(listing.amount),
    )?;

    // Mark listing as sold.
    listing.status = ListingStatus::Sold;
    writer.set_listing(&listing)?;

    debug!(
        listing_id = %listing_id,
        buyer = %buyer,
        seller = %listing.seller,
        total_price,
        protocol_fee,
        royalty,
        "listing purchased"
    );

    Ok(vec![Event::listing_sold(listing_id, buyer, &listing.seller)])
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

    fn setup_listing(store: &MemoryStore) -> (Hash, Hash) {
        let creator = test_addr(1);
        let config = ChainConfig::default();

        // Create asset class and mint to seller.
        let (asset_id, _) = assets::execute_create_asset_class(
            &creator,
            "Sword",
            "SWD",
            AssetType::Fungible,
            Some(1000),
            "https://meta",
            store,
            100,
        )
        .unwrap();
        assets::execute_mint_asset(&creator, &asset_id, &creator, 100, None, store).unwrap();

        // Create seller account with native balance.
        StateWriter::new(store)
            .set_account(&AccountState::with_balance(creator, 50_000, 0))
            .unwrap();

        let (listing_id, _) =
            execute_create_listing(&creator, &asset_id, 10, 500, &Hash::ZERO, store, &config, 200)
                .unwrap();

        (asset_id, listing_id)
    }

    #[test]
    fn create_listing_happy_path() {
        let store = MemoryStore::new();
        let (asset_id, listing_id) = setup_listing(&store);

        let view = StateView::new(&store);
        let listing = view.get_listing(&listing_id).unwrap().unwrap();
        assert_eq!(listing.amount, 10);
        assert_eq!(listing.price_per_unit, 500);
        assert!(listing.is_active());

        // Seller's asset balance should be debited by 10.
        let seller_bal = view.get_asset_balance(&asset_id, &test_addr(1)).unwrap();
        assert_eq!(seller_bal, 90); // 100 - 10
    }

    #[test]
    fn create_listing_insufficient_assets() {
        let store = MemoryStore::new();
        let creator = test_addr(1);
        let config = ChainConfig::default();

        let (asset_id, _) = assets::execute_create_asset_class(
            &creator,
            "Sword",
            "SWD",
            AssetType::Fungible,
            Some(1000),
            "https://meta",
            &store,
            100,
        )
        .unwrap();
        assets::execute_mint_asset(&creator, &asset_id, &creator, 5, None, &store).unwrap();

        let err = execute_create_listing(
            &creator,
            &asset_id,
            10,
            500,
            &Hash::ZERO,
            &store,
            &config,
            200,
        )
        .unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }

    #[test]
    fn cancel_listing_happy_path() {
        let store = MemoryStore::new();
        let (_asset_id, listing_id) = setup_listing(&store);
        let seller = test_addr(1);

        let events = execute_cancel_listing(&seller, &listing_id, &store).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "listing_cancelled");

        let listing = StateView::new(&store)
            .get_listing(&listing_id)
            .unwrap()
            .unwrap();
        assert_eq!(listing.status, ListingStatus::Cancelled);
    }

    #[test]
    fn cancel_listing_wrong_owner() {
        let store = MemoryStore::new();
        let (_asset_id, listing_id) = setup_listing(&store);

        let err = execute_cancel_listing(&test_addr(99), &listing_id, &store).unwrap_err();
        assert!(matches!(err, ExecutionError::ListingOwnerMismatch));
    }

    #[test]
    fn buy_listing_happy_path() {
        let store = MemoryStore::new();
        let (asset_id, listing_id) = setup_listing(&store);
        let buyer = test_addr(2);
        let seller = test_addr(1);
        let config = ChainConfig::default();

        // Give buyer enough native tokens.
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(buyer, 100_000, 0))
            .unwrap();

        let events =
            execute_buy_listing(&buyer, &listing_id, &store, &config, 300).unwrap();
        assert!(!events.is_empty());

        let view = StateView::new(&store);

        // Listing should be sold.
        let listing = view.get_listing(&listing_id).unwrap().unwrap();
        assert_eq!(listing.status, ListingStatus::Sold);

        // Buyer should have the assets.
        let buyer_asset_bal = view.get_asset_balance(&asset_id, &buyer).unwrap();
        assert_eq!(buyer_asset_bal, 10);

        // Buyer's balance should be reduced by total price (10 * 500 = 5000).
        let buyer_acct = view.get_account(&buyer).unwrap().unwrap();
        assert_eq!(buyer_acct.balance, 100_000 - 5000);

        // Seller should have received proceeds (minus fees).
        // In this test the seller IS also the asset creator, so seller
        // receives both the sale proceeds AND the royalty.
        let seller_acct = view.get_account(&seller).unwrap().unwrap();
        // protocol_fee = 5000 * 250 / 10000 = 125
        // royalty = 5000 * 250 / 10000 = 125 (royalty_bps == protocol_fee_bps in our setup)
        // seller_proceeds = 5000 - 125 - 125 = 4750
        // seller also gets royalty (125) because they are the asset creator
        // total = 50_000 + 4750 + 125 = 54875
        assert_eq!(seller_acct.balance, 50_000 + 4750 + 125);
    }

    #[test]
    fn buy_own_listing_rejected() {
        let store = MemoryStore::new();
        let (_asset_id, listing_id) = setup_listing(&store);
        let seller = test_addr(1);
        let config = ChainConfig::default();

        let err =
            execute_buy_listing(&seller, &listing_id, &store, &config, 300).unwrap_err();
        assert!(matches!(err, ExecutionError::CannotBuyOwnListing));
    }

    #[test]
    fn buy_listing_insufficient_balance() {
        let store = MemoryStore::new();
        let (_asset_id, listing_id) = setup_listing(&store);
        let buyer = test_addr(2);
        let config = ChainConfig::default();

        // Buyer with only 100 tokens, listing costs 5000.
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(buyer, 100, 0))
            .unwrap();

        let err =
            execute_buy_listing(&buyer, &listing_id, &store, &config, 300).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }
}
