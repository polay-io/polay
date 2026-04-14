use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;

/// A player's on-chain profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PlayerProfile {
    /// The player's blockchain address (also the primary key).
    pub address: Address,
    /// Unique username (enforced at the execution layer).
    pub username: String,
    /// Display name shown in UIs.
    pub display_name: String,
    /// Aggregate reputation score (can be negative).
    pub reputation: i64,
    /// Optional JSON-encoded metadata blob.
    pub metadata: Option<String>,
    /// Unix timestamp (seconds) when the profile was created.
    pub created_at: u64,
}

impl PlayerProfile {
    /// Create a new profile with zero reputation.
    pub fn new(
        address: Address,
        username: String,
        display_name: String,
        metadata: Option<String>,
        created_at: u64,
    ) -> Self {
        Self {
            address,
            username,
            display_name,
            reputation: 0,
            metadata,
            created_at,
        }
    }

    /// Adjust reputation by `delta` (can be negative).
    pub fn adjust_reputation(&mut self, delta: i64) {
        self.reputation = self.reputation.saturating_add(delta);
    }
}

/// A soulbound or transferable achievement awarded to a player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Achievement {
    /// Application-defined unique identifier for this achievement type.
    pub id: String,
    /// The player who earned it.
    pub player: Address,
    /// Human-readable achievement name.
    pub name: String,
    /// JSON-encoded metadata (icon, description, rarity, etc.).
    pub metadata: String,
    /// Unix timestamp (seconds) when the achievement was awarded.
    pub awarded_at: u64,
    /// If `true`, the achievement is non-transferable.
    pub soulbound: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reputation_adjustment() {
        let mut profile =
            PlayerProfile::new(Address::ZERO, "alice".into(), "Alice".into(), None, 1000);
        assert_eq!(profile.reputation, 0);
        profile.adjust_reputation(10);
        assert_eq!(profile.reputation, 10);
        profile.adjust_reputation(-25);
        assert_eq!(profile.reputation, -15);
    }

    #[test]
    fn reputation_saturation() {
        let mut profile = PlayerProfile::new(Address::ZERO, "bob".into(), "Bob".into(), None, 0);
        profile.reputation = i64::MAX;
        profile.adjust_reputation(1);
        assert_eq!(profile.reputation, i64::MAX);

        profile.reputation = i64::MIN;
        profile.adjust_reputation(-1);
        assert_eq!(profile.reputation, i64::MIN);
    }

    #[test]
    fn serde_round_trip_profile() {
        let profile = PlayerProfile::new(
            Address::ZERO,
            "charlie".into(),
            "Charlie".into(),
            Some("{\"avatar\":\"dragon\"}".into()),
            1700000000,
        );
        let json = serde_json::to_string(&profile).unwrap();
        let parsed: PlayerProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile, parsed);
    }

    #[test]
    fn borsh_round_trip_profile() {
        let profile = PlayerProfile::new(Address::ZERO, "dave".into(), "Dave".into(), None, 0);
        let encoded = borsh::to_vec(&profile).unwrap();
        let decoded = PlayerProfile::try_from_slice(&encoded).unwrap();
        assert_eq!(profile, decoded);
    }

    #[test]
    fn serde_round_trip_achievement() {
        let ach = Achievement {
            id: "first_blood".into(),
            player: Address::ZERO,
            name: "First Blood".into(),
            metadata: "{\"rarity\":\"common\"}".into(),
            awarded_at: 1700000000,
            soulbound: true,
        };
        let json = serde_json::to_string(&ach).unwrap();
        let parsed: Achievement = serde_json::from_str(&json).unwrap();
        assert_eq!(ach, parsed);
    }

    #[test]
    fn borsh_round_trip_achievement() {
        let ach = Achievement {
            id: "mvp".into(),
            player: Address::ZERO,
            name: "Most Valuable Player".into(),
            metadata: "{}".into(),
            awarded_at: 0,
            soulbound: false,
        };
        let encoded = borsh::to_vec(&ach).unwrap();
        let decoded = Achievement::try_from_slice(&encoded).unwrap();
        assert_eq!(ach, decoded);
    }
}
