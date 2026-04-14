use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::hash::Hash;

/// Describes the fungibility class of an asset.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub enum AssetType {
    /// Fully fungible tokens (e.g., in-game gold).
    Fungible,
    /// Unique, one-of-a-kind tokens (e.g., a legendary weapon).
    NonFungible,
    /// Tokens that share a class but each unit may carry distinct metadata
    /// (e.g., potions with varying potency).
    SemiFungible,
}

/// An asset class defines a *category* of tokens that can be minted.
///
/// Think of it as the "template": there is one `AssetClass` for "Gold Coin"
/// and many individual balances of that class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AssetClass {
    /// Content-addressed identifier derived from creation parameters.
    pub id: Hash,
    /// Human-readable name (e.g., "Dragon Scale Armor").
    pub name: String,
    /// Short ticker (e.g., "DSA").
    pub symbol: String,
    /// Fungibility type.
    pub asset_type: AssetType,
    /// Current total supply that has been minted.
    pub total_supply: u64,
    /// Optional hard cap. `None` means unlimited.
    pub max_supply: Option<u64>,
    /// Address of the creator who is authorized to mint.
    pub creator: Address,
    /// URI pointing to off-chain metadata (image, description, etc.).
    pub metadata_uri: String,
    /// Unix timestamp (seconds) of creation.
    pub created_at: u64,
}

impl AssetClass {
    /// Returns `true` if `additional` tokens can still be minted without
    /// exceeding `max_supply`.
    pub fn can_mint(&self, additional: u64) -> bool {
        match self.max_supply {
            Some(cap) => self.total_supply.saturating_add(additional) <= cap,
            None => true,
        }
    }

    /// Returns the remaining mintable supply, or `None` if uncapped.
    pub fn remaining_supply(&self) -> Option<u64> {
        self.max_supply
            .map(|cap| cap.saturating_sub(self.total_supply))
    }
}

/// Tracks how many units of a particular asset class an account owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AssetBalance {
    /// The owner's address.
    pub owner: Address,
    /// Which asset class this balance refers to.
    pub asset_class_id: Hash,
    /// Number of units owned.
    pub amount: u64,
}

impl AssetBalance {
    /// Create a new zero-balance entry.
    pub fn new(owner: Address, asset_class_id: Hash) -> Self {
        Self {
            owner,
            asset_class_id,
            amount: 0,
        }
    }

    /// Credit units.
    pub fn credit(&mut self, amount: u64) {
        self.amount = self.amount.saturating_add(amount);
    }

    /// Debit units, returning an error if the balance is insufficient.
    pub fn debit(&mut self, amount: u64) -> Result<(), String> {
        if self.amount < amount {
            return Err(format!(
                "insufficient asset balance: required {}, available {}",
                amount, self.amount
            ));
        }
        self.amount -= amount;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_asset_class() -> AssetClass {
        AssetClass {
            id: Hash::ZERO,
            name: "Gold Coin".into(),
            symbol: "GLD".into(),
            asset_type: AssetType::Fungible,
            total_supply: 1000,
            max_supply: Some(10_000),
            creator: Address::ZERO,
            metadata_uri: "https://example.com/gold.json".into(),
            created_at: 1700000000,
        }
    }

    #[test]
    fn can_mint_within_cap() {
        let ac = sample_asset_class();
        assert!(ac.can_mint(9000));
        assert!(!ac.can_mint(9001));
    }

    #[test]
    fn can_mint_uncapped() {
        let mut ac = sample_asset_class();
        ac.max_supply = None;
        assert!(ac.can_mint(u64::MAX));
    }

    #[test]
    fn remaining_supply() {
        let ac = sample_asset_class();
        assert_eq!(ac.remaining_supply(), Some(9000));
    }

    #[test]
    fn asset_balance_debit_credit() {
        let mut bal = AssetBalance::new(Address::ZERO, Hash::ZERO);
        bal.credit(50);
        assert_eq!(bal.amount, 50);
        bal.debit(30).unwrap();
        assert_eq!(bal.amount, 20);
        assert!(bal.debit(100).is_err());
    }

    #[test]
    fn serde_round_trip() {
        let ac = sample_asset_class();
        let json = serde_json::to_string(&ac).unwrap();
        let parsed: AssetClass = serde_json::from_str(&json).unwrap();
        assert_eq!(ac, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let ac = sample_asset_class();
        let encoded = borsh::to_vec(&ac).unwrap();
        let decoded = AssetClass::try_from_slice(&encoded).unwrap();
        assert_eq!(ac, decoded);
    }

    #[test]
    fn asset_type_serde() {
        for t in [
            AssetType::Fungible,
            AssetType::NonFungible,
            AssetType::SemiFungible,
        ] {
            let json = serde_json::to_string(&t).unwrap();
            let parsed: AssetType = serde_json::from_str(&json).unwrap();
            assert_eq!(t, parsed);
        }
    }
}
