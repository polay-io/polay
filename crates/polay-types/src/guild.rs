use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::hash::Hash;

/// Role of a member within a guild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum GuildRole {
    /// The guild creator/owner with full permissions.
    Leader,
    /// Promoted member with management permissions.
    Officer,
    /// Regular guild member.
    Member,
}

/// An on-chain guild — a player-run organization with a shared treasury.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Guild {
    /// Content-addressed identifier for this guild.
    pub guild_id: Hash,
    /// Human-readable guild name.
    pub name: String,
    /// Description of the guild.
    pub description: String,
    /// Address of the guild leader.
    pub leader: Address,
    /// Current treasury balance in native tokens.
    pub treasury_balance: u64,
    /// Current number of members.
    pub member_count: u32,
    /// Maximum allowed members.
    pub max_members: u32,
    /// Block height at which the guild was created.
    pub created_at: u64,
}

/// Tracks a single member's participation in a guild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct GuildMembership {
    /// Which guild this membership belongs to.
    pub guild_id: Hash,
    /// The member's address.
    pub member: Address,
    /// The member's role in the guild.
    pub role: GuildRole,
    /// Block height at which the member joined.
    pub joined_at: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_guild() -> Guild {
        Guild {
            guild_id: Hash::ZERO,
            name: "TestGuild".into(),
            description: "A test guild".into(),
            leader: Address::ZERO,
            treasury_balance: 0,
            member_count: 1,
            max_members: 100,
            created_at: 1,
        }
    }

    fn sample_membership() -> GuildMembership {
        GuildMembership {
            guild_id: Hash::ZERO,
            member: Address::ZERO,
            role: GuildRole::Leader,
            joined_at: 1,
        }
    }

    #[test]
    fn guild_serde_round_trip() {
        let g = sample_guild();
        let json = serde_json::to_string(&g).unwrap();
        let parsed: Guild = serde_json::from_str(&json).unwrap();
        assert_eq!(g, parsed);
    }

    #[test]
    fn guild_borsh_round_trip() {
        let g = sample_guild();
        let encoded = borsh::to_vec(&g).unwrap();
        let decoded = Guild::try_from_slice(&encoded).unwrap();
        assert_eq!(g, decoded);
    }

    #[test]
    fn membership_serde_round_trip() {
        let m = sample_membership();
        let json = serde_json::to_string(&m).unwrap();
        let parsed: GuildMembership = serde_json::from_str(&json).unwrap();
        assert_eq!(m, parsed);
    }

    #[test]
    fn membership_borsh_round_trip() {
        let m = sample_membership();
        let encoded = borsh::to_vec(&m).unwrap();
        let decoded = GuildMembership::try_from_slice(&encoded).unwrap();
        assert_eq!(m, decoded);
    }

    #[test]
    fn guild_role_serde() {
        for r in [GuildRole::Leader, GuildRole::Officer, GuildRole::Member] {
            let json = serde_json::to_string(&r).unwrap();
            let parsed: GuildRole = serde_json::from_str(&json).unwrap();
            assert_eq!(r, parsed);
        }
    }
}
