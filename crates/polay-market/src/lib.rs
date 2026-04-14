//! `polay-market` — extended marketplace utilities for the POLAY gaming
//! blockchain.
//!
//! This crate re-exports the core marketplace types from
//! [`polay_types::market`] and provides higher-level operations such as fee
//! calculation, listing validation, and aggregate statistics.

use polay_state::StateStore;
use polay_types::{Address, Hash};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

/// Re-export all market types for downstream convenience.
pub use polay_types::market::{Listing, ListingStatus};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors specific to the market module.
#[derive(Debug, Error)]
pub enum MarketError {
    /// An error propagated from the state layer.
    #[error("state error: {0}")]
    State(#[from] polay_state::StateError),

    /// A listing parameter is invalid.
    #[error("invalid listing parameter: {0}")]
    InvalidParam(String),

    /// The requested listing was not found.
    #[error("listing not found")]
    ListingNotFound,
}

pub type MarketResult<T> = Result<T, MarketError>;

// ---------------------------------------------------------------------------
// MarketplaceStats
// ---------------------------------------------------------------------------

/// Aggregate marketplace statistics.
///
/// TODO: These values require an indexer or aggregate-query infrastructure
/// that does not yet exist. For now the struct is defined so that downstream
/// code can depend on the shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MarketplaceStats {
    /// Total number of listings ever created.
    pub total_listings: u64,
    /// Number of currently active (purchasable) listings.
    pub active_listings: u64,
    /// Cumulative trade volume in native tokens.
    pub total_volume: u64,
}

// ---------------------------------------------------------------------------
// MarketModule
// ---------------------------------------------------------------------------

/// Extended marketplace logic.
pub struct MarketModule;

impl MarketModule {
    /// Calculate the fee breakdown for a sale.
    ///
    /// Given a `price` (total), a `protocol_fee_bps` (basis points taken by
    /// the protocol), and a `royalty_bps` (basis points paid to the asset
    /// creator), returns:
    ///
    /// `(seller_receives, protocol_fee, royalty)`
    ///
    /// All amounts are rounded down (truncated).
    pub fn calculate_fees(price: u64, protocol_fee_bps: u16, royalty_bps: u16) -> (u64, u64, u64) {
        let protocol_fee = ((price as u128) * protocol_fee_bps as u128 / 10_000u128) as u64;
        let royalty = ((price as u128) * royalty_bps as u128 / 10_000u128) as u64;
        let seller_receives = price.saturating_sub(protocol_fee).saturating_sub(royalty);

        debug!(
            price,
            protocol_fee, royalty, seller_receives, "fee breakdown calculated"
        );

        (seller_receives, protocol_fee, royalty)
    }

    /// Validate listing creation parameters.
    ///
    /// Both `amount` and `price_per_unit` must be greater than zero.
    pub fn validate_listing_params(amount: u64, price_per_unit: u64) -> MarketResult<()> {
        if amount == 0 {
            return Err(MarketError::InvalidParam(
                "amount must be greater than zero".to_string(),
            ));
        }
        if price_per_unit == 0 {
            return Err(MarketError::InvalidParam(
                "price_per_unit must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }

    /// Retrieve all listings for a specific asset class.
    ///
    /// TODO: This requires a secondary index mapping asset class IDs to
    /// listing IDs. For the MVP this returns an empty vector. A future
    /// iteration should maintain an append-only list key per asset class:
    ///   `market:asset_listings:<asset_class_id>` -> Vec<Hash>
    pub fn get_listings_by_asset(
        _store: &dyn StateStore,
        _asset_class_id: &Hash,
    ) -> MarketResult<Vec<Listing>> {
        // TODO: Implement asset-listing index.
        Ok(Vec::new())
    }

    /// Retrieve all listings created by a specific seller.
    ///
    /// TODO: This requires a secondary index mapping seller addresses to
    /// listing IDs. For the MVP this returns an empty vector. A future
    /// iteration should maintain an append-only list key per seller:
    ///   `market:seller_listings:<address>` -> Vec<Hash>
    pub fn get_listings_by_seller(
        _store: &dyn StateStore,
        _seller: &Address,
    ) -> MarketResult<Vec<Listing>> {
        // TODO: Implement seller-listing index.
        Ok(Vec::new())
    }

    /// Return placeholder marketplace statistics.
    ///
    /// TODO: Aggregate queries require a dedicated indexer. For now this
    /// returns a default (zeroed) stats object.
    pub fn get_stats(_store: &dyn StateStore) -> MarketplaceStats {
        // TODO: Implement aggregate indexer for marketplace stats.
        MarketplaceStats::default()
    }
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

    fn test_hash(byte: u8) -> Hash {
        Hash::new([byte; 32])
    }

    #[test]
    fn calculate_fees_basic() {
        // Price 10_000, protocol 2.5% (250 bps), royalty 2.5% (250 bps).
        let (seller, protocol, royalty) = MarketModule::calculate_fees(10_000, 250, 250);
        assert_eq!(protocol, 250);
        assert_eq!(royalty, 250);
        assert_eq!(seller, 9500);
        assert_eq!(seller + protocol + royalty, 10_000);
    }

    #[test]
    fn calculate_fees_zero_price() {
        let (seller, protocol, royalty) = MarketModule::calculate_fees(0, 250, 250);
        assert_eq!(seller, 0);
        assert_eq!(protocol, 0);
        assert_eq!(royalty, 0);
    }

    #[test]
    fn calculate_fees_zero_bps() {
        let (seller, protocol, royalty) = MarketModule::calculate_fees(10_000, 0, 0);
        assert_eq!(seller, 10_000);
        assert_eq!(protocol, 0);
        assert_eq!(royalty, 0);
    }

    #[test]
    fn calculate_fees_high_bps() {
        // 50% protocol + 50% royalty = seller gets 0.
        let (seller, protocol, royalty) = MarketModule::calculate_fees(10_000, 5000, 5000);
        assert_eq!(protocol, 5000);
        assert_eq!(royalty, 5000);
        assert_eq!(seller, 0);
    }

    #[test]
    fn calculate_fees_rounding() {
        // 1 token with 250 bps = 0.025, truncated to 0.
        let (seller, protocol, royalty) = MarketModule::calculate_fees(1, 250, 250);
        assert_eq!(protocol, 0);
        assert_eq!(royalty, 0);
        assert_eq!(seller, 1);
    }

    #[test]
    fn validate_listing_params_valid() {
        assert!(MarketModule::validate_listing_params(10, 500).is_ok());
    }

    #[test]
    fn validate_listing_params_zero_amount() {
        let err = MarketModule::validate_listing_params(0, 500).unwrap_err();
        assert!(matches!(err, MarketError::InvalidParam(_)));
    }

    #[test]
    fn validate_listing_params_zero_price() {
        let err = MarketModule::validate_listing_params(10, 0).unwrap_err();
        assert!(matches!(err, MarketError::InvalidParam(_)));
    }

    #[test]
    fn get_listings_by_asset_returns_empty() {
        let store = MemoryStore::new();
        let listings = MarketModule::get_listings_by_asset(&store, &test_hash(1)).unwrap();
        assert!(listings.is_empty());
    }

    #[test]
    fn get_listings_by_seller_returns_empty() {
        let store = MemoryStore::new();
        let listings = MarketModule::get_listings_by_seller(&store, &test_addr(1)).unwrap();
        assert!(listings.is_empty());
    }

    #[test]
    fn get_stats_returns_default() {
        let store = MemoryStore::new();
        let stats = MarketModule::get_stats(&store);
        assert_eq!(stats.total_listings, 0);
        assert_eq!(stats.active_listings, 0);
        assert_eq!(stats.total_volume, 0);
    }

    #[test]
    fn marketplace_stats_serde_round_trip() {
        let stats = MarketplaceStats {
            total_listings: 42,
            active_listings: 10,
            total_volume: 1_000_000,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let parsed: MarketplaceStats = serde_json::from_str(&json).unwrap();
        assert_eq!(stats, parsed);
    }
}
