use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::hash::Hash;

/// Lifecycle status of a marketplace listing.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ListingStatus {
    /// The listing is live and available for purchase.
    Active,
    /// The listing has been fully purchased.
    Sold,
    /// The seller cancelled the listing before it was purchased.
    Cancelled,
}

/// A marketplace listing offering a quantity of an asset for sale at a
/// fixed per-unit price.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct Listing {
    /// Unique listing identifier (content-addressed).
    pub id: Hash,
    /// The address of the seller.
    pub seller: Address,
    /// The asset class being sold.
    pub asset_class_id: Hash,
    /// Number of units offered.
    pub amount: u64,
    /// Price per unit in the `currency` token.
    pub price_per_unit: u64,
    /// The asset class ID of the currency used for payment.
    /// `Hash::ZERO` means native token.
    pub currency: Hash,
    /// Current lifecycle status.
    pub status: ListingStatus,
    /// Royalty in basis points (1 bps = 0.01%) paid to the asset creator on
    /// each sale.
    pub royalty_bps: u16,
    /// Unix timestamp (seconds) when the listing was created.
    pub created_at: u64,
}

impl Listing {
    /// Total cost for buying the entire listing (amount * price_per_unit).
    pub fn total_price(&self) -> u64 {
        self.amount.saturating_mul(self.price_per_unit)
    }

    /// Compute the royalty amount that should go to the asset creator.
    pub fn royalty_amount(&self) -> u64 {
        let total = self.total_price() as u128;
        let royalty = (total * self.royalty_bps as u128) / 10_000u128;
        royalty as u64
    }

    /// Returns `true` if the listing is still purchasable.
    pub fn is_active(&self) -> bool {
        self.status == ListingStatus::Active
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_listing() -> Listing {
        Listing {
            id: Hash::ZERO,
            seller: Address::ZERO,
            asset_class_id: Hash::ZERO,
            amount: 10,
            price_per_unit: 100,
            currency: Hash::ZERO,
            status: ListingStatus::Active,
            royalty_bps: 250, // 2.5%
            created_at: 1700000000,
        }
    }

    #[test]
    fn total_price() {
        let listing = sample_listing();
        assert_eq!(listing.total_price(), 1000);
    }

    #[test]
    fn royalty_amount() {
        let listing = sample_listing();
        // 1000 * 250 / 10_000 = 25
        assert_eq!(listing.royalty_amount(), 25);
    }

    #[test]
    fn is_active() {
        let mut listing = sample_listing();
        assert!(listing.is_active());
        listing.status = ListingStatus::Sold;
        assert!(!listing.is_active());
    }

    #[test]
    fn serde_round_trip() {
        let listing = sample_listing();
        let json = serde_json::to_string(&listing).unwrap();
        let parsed: Listing = serde_json::from_str(&json).unwrap();
        assert_eq!(listing, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let listing = sample_listing();
        let encoded = borsh::to_vec(&listing).unwrap();
        let decoded = Listing::try_from_slice(&encoded).unwrap();
        assert_eq!(listing, decoded);
    }

    #[test]
    fn listing_status_variants() {
        for status in [ListingStatus::Active, ListingStatus::Sold, ListingStatus::Cancelled] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: ListingStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }
}
