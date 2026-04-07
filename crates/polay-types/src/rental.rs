use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::hash::Hash;

/// Describes the current status of a rental listing.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum RentalStatus {
    /// The rental is listed and available to be rented.
    Listed,
    /// The asset is currently rented out.
    Active,
    /// The renter has returned the asset.
    Returned,
    /// The rental duration expired without return.
    Expired,
    /// The owner cancelled the listing before it was rented.
    Cancelled,
}

/// An asset rental record — tracks a single asset being rented from an owner
/// to a renter for a given duration at a per-block price.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct Rental {
    /// Content-addressed identifier for this rental.
    pub rental_id: Hash,
    /// The address of the asset owner who listed the rental.
    pub owner: Address,
    /// The address of the current renter, if any.
    pub renter: Option<Address>,
    /// The asset class being rented.
    pub asset_class_id: Hash,
    /// The specific asset instance being rented.
    pub asset_id: Hash,
    /// Price per block in native tokens.
    pub price_per_block: u64,
    /// Deposit required from the renter.
    pub deposit: u64,
    /// Minimum rental duration in blocks.
    pub min_duration: u64,
    /// Maximum rental duration in blocks.
    pub max_duration: u64,
    /// Block height at which the rental started.
    pub start_height: Option<u64>,
    /// Block height at which the rental ends.
    pub end_height: Option<u64>,
    /// Current status of the rental.
    pub status: RentalStatus,
    /// Block height at which the rental was created.
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rental() -> Rental {
        Rental {
            rental_id: Hash::ZERO,
            owner: Address::ZERO,
            renter: None,
            asset_class_id: Hash::ZERO,
            asset_id: Hash::ZERO,
            price_per_block: 100,
            deposit: 1000,
            min_duration: 10,
            max_duration: 100,
            start_height: None,
            end_height: None,
            status: RentalStatus::Listed,
            created_at: 1,
        }
    }

    #[test]
    fn serde_round_trip() {
        let r = sample_rental();
        let json = serde_json::to_string(&r).unwrap();
        let parsed: Rental = serde_json::from_str(&json).unwrap();
        assert_eq!(r, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let r = sample_rental();
        let encoded = borsh::to_vec(&r).unwrap();
        let decoded = Rental::try_from_slice(&encoded).unwrap();
        assert_eq!(r, decoded);
    }

    #[test]
    fn rental_status_serde() {
        for s in [
            RentalStatus::Listed,
            RentalStatus::Active,
            RentalStatus::Returned,
            RentalStatus::Expired,
            RentalStatus::Cancelled,
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let parsed: RentalStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, parsed);
        }
    }
}
