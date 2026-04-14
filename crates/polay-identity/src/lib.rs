//! `polay-identity` — extended identity utilities for the POLAY gaming
//! blockchain.
//!
//! This crate re-exports the core identity types from
//! [`polay_types::identity`] and provides higher-level operations such as
//! username validation, reputation level calculation, and player summaries.

use polay_state::{StateStore, StateView};
use polay_types::Address;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

/// Re-export all identity types for downstream convenience.
pub use polay_types::identity::{Achievement, PlayerProfile};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors specific to the identity module.
#[derive(Debug, Error)]
pub enum IdentityError {
    /// An error propagated from the state layer.
    #[error("state error: {0}")]
    State(#[from] polay_state::StateError),

    /// The username is invalid.
    #[error("invalid username: {0}")]
    InvalidUsername(String),

    /// The player profile was not found.
    #[error("profile not found for address: {0}")]
    ProfileNotFound(String),
}

pub type IdentityResult<T> = Result<T, IdentityError>;

// ---------------------------------------------------------------------------
// ReputationLevel
// ---------------------------------------------------------------------------

/// Tiered reputation level derived from a player's raw reputation score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReputationLevel {
    /// Reputation is below zero.
    Negative,
    /// Reputation 0 - 99.
    Newcomer,
    /// Reputation 100 - 499.
    Bronze,
    /// Reputation 500 - 999.
    Silver,
    /// Reputation 1000 - 4999.
    Gold,
    /// Reputation 5000+.
    Diamond,
}

impl std::fmt::Display for ReputationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReputationLevel::Negative => write!(f, "Negative"),
            ReputationLevel::Newcomer => write!(f, "Newcomer"),
            ReputationLevel::Bronze => write!(f, "Bronze"),
            ReputationLevel::Silver => write!(f, "Silver"),
            ReputationLevel::Gold => write!(f, "Gold"),
            ReputationLevel::Diamond => write!(f, "Diamond"),
        }
    }
}

// ---------------------------------------------------------------------------
// PlayerSummary
// ---------------------------------------------------------------------------

/// A read-only summary of a player's on-chain identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSummary {
    /// The player's blockchain address.
    pub address: Address,
    /// Unique username.
    pub username: String,
    /// Display name shown in UIs.
    pub display_name: String,
    /// Raw reputation score.
    pub reputation: i64,
    /// Derived reputation tier.
    pub reputation_level: ReputationLevel,
    /// Number of achievements earned.
    ///
    /// TODO: Requires an achievement-count index. For MVP, this is always 0.
    pub achievement_count: u64,
}

// ---------------------------------------------------------------------------
// IdentityModule
// ---------------------------------------------------------------------------

/// Extended identity logic.
pub struct IdentityModule;

impl IdentityModule {
    /// Validate a username string.
    ///
    /// Rules:
    /// - Must be 3 to 32 characters long.
    /// - Only alphanumeric characters and underscores are allowed.
    /// - Must not start or end with an underscore.
    pub fn validate_username(username: &str) -> IdentityResult<()> {
        let len = username.len();

        if !(3..=32).contains(&len) {
            return Err(IdentityError::InvalidUsername(format!(
                "username must be 3-32 characters, got {}",
                len
            )));
        }

        if !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(IdentityError::InvalidUsername(
                "username may only contain alphanumeric characters and underscores".to_string(),
            ));
        }

        if username.starts_with('_') || username.ends_with('_') {
            return Err(IdentityError::InvalidUsername(
                "username must not start or end with an underscore".to_string(),
            ));
        }

        debug!(username, "username validated");
        Ok(())
    }

    /// Determine the [`ReputationLevel`] for a given raw reputation score.
    pub fn calculate_reputation_level(reputation: i64) -> ReputationLevel {
        if reputation < 0 {
            ReputationLevel::Negative
        } else if reputation < 100 {
            ReputationLevel::Newcomer
        } else if reputation < 500 {
            ReputationLevel::Bronze
        } else if reputation < 1000 {
            ReputationLevel::Silver
        } else if reputation < 5000 {
            ReputationLevel::Gold
        } else {
            ReputationLevel::Diamond
        }
    }

    /// Check whether an achievement is soulbound (non-transferable).
    ///
    /// In the current design, all achievements are soulbound. This function
    /// exists so that future changes to transferability can be gated here.
    pub fn is_achievement_soulbound(_achievement: &Achievement) -> bool {
        // All achievements are non-transferable for now.
        true
    }

    /// Build a [`PlayerSummary`] from on-chain state.
    ///
    /// Returns `None` if the player has no profile.
    pub fn get_player_summary(
        store: &dyn StateStore,
        address: &Address,
    ) -> IdentityResult<Option<PlayerSummary>> {
        let view = StateView::new(store);

        let profile = match view.get_profile(address)? {
            Some(p) => p,
            None => return Ok(None),
        };

        let reputation_level = Self::calculate_reputation_level(profile.reputation);

        // TODO: Achievement count requires a per-player achievement index.
        // For MVP we report 0. A future iteration should maintain:
        //   `identity:achievement_count:<address>` -> u64
        let achievement_count = 0u64;

        Ok(Some(PlayerSummary {
            address: *address,
            username: profile.username,
            display_name: profile.display_name,
            reputation: profile.reputation,
            reputation_level,
            achievement_count,
        }))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::{MemoryStore, StateWriter};

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    // -- Username validation -------------------------------------------------

    #[test]
    fn validate_username_valid() {
        assert!(IdentityModule::validate_username("alice").is_ok());
        assert!(IdentityModule::validate_username("bob_123").is_ok());
        assert!(IdentityModule::validate_username("PLAYER1").is_ok());
        assert!(IdentityModule::validate_username("a_b").is_ok());
        assert!(IdentityModule::validate_username("abc").is_ok()); // min length
    }

    #[test]
    fn validate_username_too_short() {
        let err = IdentityModule::validate_username("ab").unwrap_err();
        assert!(matches!(err, IdentityError::InvalidUsername(_)));
    }

    #[test]
    fn validate_username_too_long() {
        let long = "a".repeat(33);
        let err = IdentityModule::validate_username(&long).unwrap_err();
        assert!(matches!(err, IdentityError::InvalidUsername(_)));
    }

    #[test]
    fn validate_username_invalid_chars() {
        let err = IdentityModule::validate_username("alice!").unwrap_err();
        assert!(matches!(err, IdentityError::InvalidUsername(_)));

        let err2 = IdentityModule::validate_username("bob jones").unwrap_err();
        assert!(matches!(err2, IdentityError::InvalidUsername(_)));

        let err3 = IdentityModule::validate_username("alice@bob").unwrap_err();
        assert!(matches!(err3, IdentityError::InvalidUsername(_)));
    }

    #[test]
    fn validate_username_leading_underscore() {
        let err = IdentityModule::validate_username("_alice").unwrap_err();
        assert!(matches!(err, IdentityError::InvalidUsername(_)));
    }

    #[test]
    fn validate_username_trailing_underscore() {
        let err = IdentityModule::validate_username("alice_").unwrap_err();
        assert!(matches!(err, IdentityError::InvalidUsername(_)));
    }

    #[test]
    fn validate_username_max_length() {
        let max_name = "a".repeat(32);
        assert!(IdentityModule::validate_username(&max_name).is_ok());
    }

    // -- Reputation levels ---------------------------------------------------

    #[test]
    fn reputation_level_negative() {
        assert_eq!(
            IdentityModule::calculate_reputation_level(-1),
            ReputationLevel::Negative
        );
        assert_eq!(
            IdentityModule::calculate_reputation_level(-1000),
            ReputationLevel::Negative
        );
    }

    #[test]
    fn reputation_level_newcomer() {
        assert_eq!(
            IdentityModule::calculate_reputation_level(0),
            ReputationLevel::Newcomer
        );
        assert_eq!(
            IdentityModule::calculate_reputation_level(99),
            ReputationLevel::Newcomer
        );
    }

    #[test]
    fn reputation_level_bronze() {
        assert_eq!(
            IdentityModule::calculate_reputation_level(100),
            ReputationLevel::Bronze
        );
        assert_eq!(
            IdentityModule::calculate_reputation_level(499),
            ReputationLevel::Bronze
        );
    }

    #[test]
    fn reputation_level_silver() {
        assert_eq!(
            IdentityModule::calculate_reputation_level(500),
            ReputationLevel::Silver
        );
        assert_eq!(
            IdentityModule::calculate_reputation_level(999),
            ReputationLevel::Silver
        );
    }

    #[test]
    fn reputation_level_gold() {
        assert_eq!(
            IdentityModule::calculate_reputation_level(1000),
            ReputationLevel::Gold
        );
        assert_eq!(
            IdentityModule::calculate_reputation_level(4999),
            ReputationLevel::Gold
        );
    }

    #[test]
    fn reputation_level_diamond() {
        assert_eq!(
            IdentityModule::calculate_reputation_level(5000),
            ReputationLevel::Diamond
        );
        assert_eq!(
            IdentityModule::calculate_reputation_level(100_000),
            ReputationLevel::Diamond
        );
    }

    // -- Achievement soulbound -----------------------------------------------

    #[test]
    fn achievement_is_always_soulbound() {
        let ach = Achievement {
            id: "first_win".to_string(),
            player: test_addr(1),
            name: "First Win".to_string(),
            metadata: "{}".to_string(),
            awarded_at: 100,
            soulbound: true,
        };
        assert!(IdentityModule::is_achievement_soulbound(&ach));

        // Even if the struct field says false, the module considers it soulbound.
        let ach2 = Achievement {
            soulbound: false,
            ..ach
        };
        assert!(IdentityModule::is_achievement_soulbound(&ach2));
    }

    // -- Player summary ------------------------------------------------------

    #[test]
    fn get_player_summary_no_profile() {
        let store = MemoryStore::new();
        let summary = IdentityModule::get_player_summary(&store, &test_addr(1)).unwrap();
        assert!(summary.is_none());
    }

    #[test]
    fn get_player_summary_with_profile() {
        let store = MemoryStore::new();
        let addr = test_addr(1);

        let mut profile =
            PlayerProfile::new(addr, "alice".to_string(), "Alice".to_string(), None, 100);
        profile.adjust_reputation(250); // Bronze level
        StateWriter::new(&store).set_profile(&profile).unwrap();

        let summary = IdentityModule::get_player_summary(&store, &addr)
            .unwrap()
            .unwrap();
        assert_eq!(summary.address, addr);
        assert_eq!(summary.username, "alice");
        assert_eq!(summary.display_name, "Alice");
        assert_eq!(summary.reputation, 250);
        assert_eq!(summary.reputation_level, ReputationLevel::Bronze);
        assert_eq!(summary.achievement_count, 0); // MVP placeholder
    }

    #[test]
    fn get_player_summary_negative_reputation() {
        let store = MemoryStore::new();
        let addr = test_addr(2);

        let mut profile = PlayerProfile::new(
            addr,
            "toxic_player".to_string(),
            "Toxic".to_string(),
            None,
            100,
        );
        profile.adjust_reputation(-50);
        StateWriter::new(&store).set_profile(&profile).unwrap();

        let summary = IdentityModule::get_player_summary(&store, &addr)
            .unwrap()
            .unwrap();
        assert_eq!(summary.reputation, -50);
        assert_eq!(summary.reputation_level, ReputationLevel::Negative);
    }

    // -- ReputationLevel serde -----------------------------------------------

    #[test]
    fn reputation_level_serde_round_trip() {
        for level in [
            ReputationLevel::Negative,
            ReputationLevel::Newcomer,
            ReputationLevel::Bronze,
            ReputationLevel::Silver,
            ReputationLevel::Gold,
            ReputationLevel::Diamond,
        ] {
            let json = serde_json::to_string(&level).unwrap();
            let parsed: ReputationLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(level, parsed);
        }
    }

    // -- PlayerSummary serde -------------------------------------------------

    #[test]
    fn player_summary_serde_round_trip() {
        let summary = PlayerSummary {
            address: test_addr(1),
            username: "alice".to_string(),
            display_name: "Alice".to_string(),
            reputation: 500,
            reputation_level: ReputationLevel::Silver,
            achievement_count: 3,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let parsed: PlayerSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(summary, parsed);
    }

    // -- ReputationLevel Display ---------------------------------------------

    #[test]
    fn reputation_level_display() {
        assert_eq!(format!("{}", ReputationLevel::Diamond), "Diamond");
        assert_eq!(format!("{}", ReputationLevel::Negative), "Negative");
    }
}
