use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::hash::Hash;

/// Describes the current status of a tournament.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum TournamentStatus {
    /// The tournament is accepting registrations.
    Registration,
    /// The tournament is in progress.
    Active,
    /// The tournament has finished and results are recorded.
    Completed,
    /// The tournament was cancelled.
    Cancelled,
}

/// An on-chain tournament — tracks participants, prize pools, rankings, and
/// prize distribution.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct Tournament {
    /// Content-addressed identifier for this tournament.
    pub tournament_id: Hash,
    /// Human-readable tournament name.
    pub name: String,
    /// Address of the tournament organizer.
    pub organizer: Address,
    /// Identifier of the game being played.
    pub game_id: String,
    /// Entry fee in native tokens.
    pub entry_fee: u64,
    /// Total prize pool (entry fees minus protocol fees, plus any extra).
    pub prize_pool: u64,
    /// Maximum number of participants.
    pub max_participants: u32,
    /// Minimum number of participants required to start.
    pub min_participants: u32,
    /// List of participant addresses.
    pub participants: Vec<Address>,
    /// Current tournament status.
    pub status: TournamentStatus,
    /// Block height at which the tournament starts.
    pub start_height: u64,
    /// Block height at which the tournament ended.
    pub end_height: Option<u64>,
    /// Percentage per rank, must sum to 100.
    pub prize_distribution: Vec<u32>,
    /// Final rankings (ordered by placement, first = winner).
    pub rankings: Vec<Address>,
    /// Tracks whether each ranked participant has claimed their prize.
    pub prizes_claimed: Vec<bool>,
    /// Block height at which the tournament was created.
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tournament() -> Tournament {
        Tournament {
            tournament_id: Hash::ZERO,
            name: "Grand Arena".into(),
            organizer: Address::ZERO,
            game_id: "arena".into(),
            entry_fee: 1000,
            prize_pool: 0,
            max_participants: 32,
            min_participants: 2,
            participants: vec![],
            status: TournamentStatus::Registration,
            start_height: 100,
            end_height: None,
            prize_distribution: vec![70, 30],
            rankings: vec![],
            prizes_claimed: vec![],
            created_at: 1,
        }
    }

    #[test]
    fn serde_round_trip() {
        let t = sample_tournament();
        let json = serde_json::to_string(&t).unwrap();
        let parsed: Tournament = serde_json::from_str(&json).unwrap();
        assert_eq!(t, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let t = sample_tournament();
        let encoded = borsh::to_vec(&t).unwrap();
        let decoded = Tournament::try_from_slice(&encoded).unwrap();
        assert_eq!(t, decoded);
    }

    #[test]
    fn tournament_status_serde() {
        for s in [
            TournamentStatus::Registration,
            TournamentStatus::Active,
            TournamentStatus::Completed,
            TournamentStatus::Cancelled,
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let parsed: TournamentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, parsed);
        }
    }
}
