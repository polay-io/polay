use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::hash::Hash;

/// Lifecycle status of an attestor node.
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
pub enum AttestorStatus {
    /// The attestor is active and may submit match results.
    Active,
    /// Temporarily suspended (e.g., missed heartbeats).
    Suspended,
    /// Permanently revoked (e.g., caught submitting fraudulent results).
    Revoked,
}

/// An attestor is a trusted game-server oracle that submits verified match
/// results to the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Attestor {
    /// The attestor's on-chain address.
    pub address: Address,
    /// The game this attestor is authorized for.
    pub game_id: String,
    /// Network endpoint where the attestor can be reached.
    pub endpoint: String,
    /// JSON-encoded metadata (version, capabilities, etc.).
    pub metadata: String,
    /// Current lifecycle status.
    pub status: AttestorStatus,
    /// Unix timestamp (seconds) when the attestor was registered.
    pub registered_at: u64,
}

impl Attestor {
    /// Returns `true` if the attestor is in a state that allows submitting
    /// match results.
    pub fn can_submit(&self) -> bool {
        self.status == AttestorStatus::Active
    }
}

/// The outcome of a completed game match, as attested by a game-server oracle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MatchResult {
    /// Unique match identifier.
    pub match_id: Hash,
    /// The game this match belongs to.
    pub game_id: String,
    /// Unix timestamp (seconds) when the match ended.
    pub timestamp: u64,
    /// Ordered list of participants.
    pub players: Vec<Address>,
    /// Scores corresponding to each player (same order as `players`).
    pub scores: Vec<u64>,
    /// Addresses of the winners.
    pub winners: Vec<Address>,
    /// Total reward pool (native tokens) to distribute among winners.
    pub reward_pool: u64,
    /// Cryptographic signature from the game server proving authenticity.
    pub server_signature: Vec<u8>,
    /// Optional anti-cheat confidence score (0-100).
    pub anti_cheat_score: Option<u8>,
    /// Optional reference to a replay file / blob.
    pub replay_ref: Option<String>,
}

impl MatchResult {
    /// Returns `true` if the players and scores vectors have equal length.
    pub fn is_well_formed(&self) -> bool {
        self.players.len() == self.scores.len()
            && !self.players.is_empty()
            && !self.winners.is_empty()
            && self.winners.iter().all(|w| self.players.contains(w))
    }
}

/// Settlement record after a match result has been processed by the execution
/// layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MatchSettlement {
    /// The match that was settled.
    pub match_id: Hash,
    /// Whether settlement completed successfully.
    pub settled: bool,
    /// Individual reward payouts.
    pub rewards_distributed: Vec<(Address, u64)>,
    /// If `true`, the match is under investigation and rewards are frozen.
    pub quarantined: bool,
    /// Unix timestamp (seconds) when settlement occurred.
    pub settled_at: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_match_result() -> MatchResult {
        let player_a = Address::new([1u8; 32]);
        let player_b = Address::new([2u8; 32]);
        MatchResult {
            match_id: Hash::ZERO,
            game_id: "battle_royale".into(),
            timestamp: 1700000000,
            players: vec![player_a, player_b],
            scores: vec![1500, 1200],
            winners: vec![player_a],
            reward_pool: 10_000,
            server_signature: vec![0xAB; 64],
            anti_cheat_score: Some(95),
            replay_ref: Some("ipfs://Qm123".into()),
        }
    }

    #[test]
    fn well_formed_match() {
        let mr = sample_match_result();
        assert!(mr.is_well_formed());
    }

    #[test]
    fn mismatched_lengths_not_well_formed() {
        let mut mr = sample_match_result();
        mr.scores.pop();
        assert!(!mr.is_well_formed());
    }

    #[test]
    fn winner_not_in_players_not_well_formed() {
        let mut mr = sample_match_result();
        mr.winners = vec![Address::new([99u8; 32])];
        assert!(!mr.is_well_formed());
    }

    #[test]
    fn attestor_can_submit() {
        let att = Attestor {
            address: Address::ZERO,
            game_id: "chess".into(),
            endpoint: "https://attestor.example.com".into(),
            metadata: "{}".into(),
            status: AttestorStatus::Active,
            registered_at: 0,
        };
        assert!(att.can_submit());

        let suspended = Attestor {
            status: AttestorStatus::Suspended,
            ..att.clone()
        };
        assert!(!suspended.can_submit());
    }

    #[test]
    fn serde_round_trip_match_result() {
        let mr = sample_match_result();
        let json = serde_json::to_string(&mr).unwrap();
        let parsed: MatchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(mr, parsed);
    }

    #[test]
    fn borsh_round_trip_match_result() {
        let mr = sample_match_result();
        let encoded = borsh::to_vec(&mr).unwrap();
        let decoded = MatchResult::try_from_slice(&encoded).unwrap();
        assert_eq!(mr, decoded);
    }

    #[test]
    fn serde_round_trip_settlement() {
        let settlement = MatchSettlement {
            match_id: Hash::ZERO,
            settled: true,
            rewards_distributed: vec![
                (Address::new([1u8; 32]), 7000),
                (Address::new([2u8; 32]), 3000),
            ],
            quarantined: false,
            settled_at: 1700001000,
        };
        let json = serde_json::to_string(&settlement).unwrap();
        let parsed: MatchSettlement = serde_json::from_str(&json).unwrap();
        assert_eq!(settlement, parsed);
    }

    #[test]
    fn borsh_round_trip_settlement() {
        let settlement = MatchSettlement {
            match_id: Hash::ZERO,
            settled: false,
            rewards_distributed: vec![],
            quarantined: true,
            settled_at: 0,
        };
        let encoded = borsh::to_vec(&settlement).unwrap();
        let decoded = MatchSettlement::try_from_slice(&encoded).unwrap();
        assert_eq!(settlement, decoded);
    }
}
