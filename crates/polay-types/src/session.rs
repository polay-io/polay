use crate::Address;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Defines which transaction actions a session key is allowed to perform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum SessionPermission {
    /// Can do everything (dangerous but simple for MVP).
    All,
    /// Can only perform specific action types (e.g. ["transfer", "transfer_asset", "buy_listing"]).
    Actions(Vec<String>),
}

/// A session key grant from an account owner to a temporary key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SessionGrant {
    /// The account that created this session.
    pub granter: Address,
    /// The session public key (temporary key authorized to sign).
    pub session_pubkey: Vec<u8>, // 32 bytes, Ed25519 pubkey
    /// The derived address of the session key.
    pub session_address: Address,
    /// What actions this session can perform.
    pub permissions: SessionPermission,
    /// Block height when this session expires.
    pub expires_at: u64,
    /// Maximum POL this session can spend (cumulative).
    pub spending_limit: u64,
    /// POL already spent by this session.
    pub amount_spent: u64,
    /// Whether this session has been revoked.
    pub revoked: bool,
    /// Block height when created.
    pub created_at: u64,
}

impl SessionGrant {
    /// Check if this session is active and valid at the given height.
    pub fn is_valid(&self, current_height: u64) -> bool {
        !self.revoked && current_height <= self.expires_at
    }

    /// Check if this session is allowed to perform the given action.
    pub fn is_action_permitted(&self, action_label: &str) -> bool {
        match &self.permissions {
            SessionPermission::All => true,
            SessionPermission::Actions(allowed) => allowed.iter().any(|a| a == action_label),
        }
    }

    /// Check if this session has enough spending allowance.
    pub fn can_spend(&self, amount: u64) -> bool {
        self.amount_spent.saturating_add(amount) <= self.spending_limit
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_grant() -> SessionGrant {
        SessionGrant {
            granter: Address::new([1u8; 32]),
            session_pubkey: vec![2u8; 32],
            session_address: Address::new([3u8; 32]),
            permissions: SessionPermission::All,
            expires_at: 1000,
            spending_limit: 500_000,
            amount_spent: 0,
            revoked: false,
            created_at: 100,
        }
    }

    #[test]
    fn is_valid_active_session() {
        let grant = sample_grant();
        assert!(grant.is_valid(500));
        assert!(grant.is_valid(1000)); // at expiry, still valid
    }

    #[test]
    fn is_valid_expired_session() {
        let grant = sample_grant();
        assert!(!grant.is_valid(1001));
    }

    #[test]
    fn is_valid_revoked_session() {
        let mut grant = sample_grant();
        grant.revoked = true;
        assert!(!grant.is_valid(500));
    }

    #[test]
    fn permission_all_allows_anything() {
        let grant = sample_grant();
        assert!(grant.is_action_permitted("transfer"));
        assert!(grant.is_action_permitted("buy_listing"));
        assert!(grant.is_action_permitted("anything"));
    }

    #[test]
    fn permission_specific_actions() {
        let mut grant = sample_grant();
        grant.permissions =
            SessionPermission::Actions(vec!["transfer".into(), "buy_listing".into()]);
        assert!(grant.is_action_permitted("transfer"));
        assert!(grant.is_action_permitted("buy_listing"));
        assert!(!grant.is_action_permitted("create_listing"));
    }

    #[test]
    fn can_spend_within_limit() {
        let grant = sample_grant();
        assert!(grant.can_spend(500_000));
        assert!(grant.can_spend(1));
    }

    #[test]
    fn can_spend_over_limit() {
        let grant = sample_grant();
        assert!(!grant.can_spend(500_001));
    }

    #[test]
    fn can_spend_with_prior_spending() {
        let mut grant = sample_grant();
        grant.amount_spent = 400_000;
        assert!(grant.can_spend(100_000));
        assert!(!grant.can_spend(100_001));
    }

    #[test]
    fn serde_round_trip() {
        let grant = sample_grant();
        let json = serde_json::to_string(&grant).unwrap();
        let parsed: SessionGrant = serde_json::from_str(&json).unwrap();
        assert_eq!(grant, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let grant = sample_grant();
        let encoded = borsh::to_vec(&grant).unwrap();
        let decoded = SessionGrant::try_from_slice(&encoded).unwrap();
        assert_eq!(grant, decoded);
    }

    #[test]
    fn serde_round_trip_permission_actions() {
        let perm = SessionPermission::Actions(vec!["transfer".into(), "buy_listing".into()]);
        let json = serde_json::to_string(&perm).unwrap();
        let parsed: SessionPermission = serde_json::from_str(&json).unwrap();
        assert_eq!(perm, parsed);
    }

    #[test]
    fn borsh_round_trip_permission_all() {
        let perm = SessionPermission::All;
        let encoded = borsh::to_vec(&perm).unwrap();
        let decoded = SessionPermission::try_from_slice(&encoded).unwrap();
        assert_eq!(perm, decoded);
    }
}
