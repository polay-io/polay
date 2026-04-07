//! `polay-attestation` — extended attestation utilities for the POLAY gaming
//! blockchain.
//!
//! This crate re-exports the core attestation types from
//! [`polay_types::attestation`] and provides higher-level operations such as
//! signature verification on match results, anti-cheat quarantine checks,
//! and reward distribution calculation.

use sha2::{Digest, Sha256};

use polay_crypto::PolayPublicKey;
use polay_state::{StateStore, StateView};
use polay_types::{Address, Hash};
use thiserror::Error;
use tracing::debug;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

/// Re-export all attestation types for downstream convenience.
pub use polay_types::attestation::{
    Attestor, AttestorStatus, MatchResult, MatchSettlement,
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors specific to the attestation module.
#[derive(Debug, Error)]
pub enum AttestationError {
    /// An error propagated from the state layer.
    #[error("state error: {0}")]
    State(#[from] polay_state::StateError),

    /// An error propagated from the crypto layer.
    #[error("crypto error: {0}")]
    Crypto(#[from] polay_crypto::CryptoError),

    /// The signature on a match result is invalid.
    #[error("invalid match result signature")]
    InvalidSignature,

    /// The match result was not found in state.
    #[error("match result not found")]
    MatchResultNotFound,
}

pub type AttestationResult<T> = Result<T, AttestationError>;

// ---------------------------------------------------------------------------
// AttestationModule
// ---------------------------------------------------------------------------

/// Extended attestation logic that operates over match results and the state
/// store.
pub struct AttestationModule;

impl AttestationModule {
    /// Verify the `server_signature` field of a [`MatchResult`] against the
    /// attestor's Ed25519 public key.
    ///
    /// The signed message is constructed by hashing the canonical match data:
    /// `SHA-256(match_id || game_id || timestamp || players || scores || winners)`.
    pub fn verify_match_result_signature(
        result: &MatchResult,
        attestor_pubkey: &[u8; 32],
    ) -> AttestationResult<bool> {
        let pubkey = PolayPublicKey::from_bytes(attestor_pubkey)?;

        // Build the canonical message to verify against.
        let message = Self::canonical_match_hash(result);

        // The server_signature is stored as Vec<u8>. We need exactly 64 bytes
        // for an Ed25519 signature.
        if result.server_signature.len() != 64 {
            return Ok(false);
        }

        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&result.server_signature);
        let signature = polay_types::Signature::new(sig_bytes);

        match pubkey.verify(message.as_bytes(), &signature) {
            Ok(()) => {
                debug!(
                    match_id = %result.match_id,
                    "match result signature verified"
                );
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// Check whether a match result should be quarantined based on its
    /// anti-cheat score.
    ///
    /// Returns `true` if the `anti_cheat_score` is present and falls below
    /// the given `threshold`.
    pub fn check_quarantine(result: &MatchResult, threshold: u8) -> bool {
        match result.anti_cheat_score {
            Some(score) if score < threshold => {
                debug!(
                    match_id = %result.match_id,
                    score,
                    threshold,
                    "match flagged for quarantine"
                );
                true
            }
            _ => false,
        }
    }

    /// Retrieve the match history (match IDs) for a given player.
    ///
    /// TODO: This requires a player-to-match index that has not been
    /// implemented yet. For the MVP this returns an empty vector.
    pub fn get_match_history_for_player(
        _store: &dyn StateStore,
        _player: &Address,
    ) -> AttestationResult<Vec<Hash>> {
        // TODO: Implement player-match index. Currently there is no secondary
        // index mapping player addresses to match IDs. A future iteration
        // should maintain an append-only list key per player:
        //   `attestation:player_matches:<address>` -> Vec<Hash>
        Ok(Vec::new())
    }

    /// Calculate the reward distribution for a completed match.
    ///
    /// Default policy:
    /// - Winners collectively receive 60% of the `reward_pool`.
    ///   Each winner gets an equal share of that 60%.
    /// - The remaining 40% is split equally among all other players.
    /// - If there are no winners (should not happen with a well-formed result),
    ///   the entire pool is split equally among all players.
    pub fn calculate_reward_distribution(result: &MatchResult) -> Vec<(Address, u64)> {
        if result.players.is_empty() || result.reward_pool == 0 {
            return Vec::new();
        }

        // If no winners, split everything equally.
        if result.winners.is_empty() {
            let share = result.reward_pool / result.players.len() as u64;
            return result
                .players
                .iter()
                .map(|p| (*p, share))
                .collect();
        }

        let winner_pool = (result.reward_pool as u128 * 60 / 100) as u64;
        let loser_pool = result.reward_pool.saturating_sub(winner_pool);

        let mut payouts: Vec<(Address, u64)> = Vec::with_capacity(result.players.len());

        // Winner share.
        let winner_share = if !result.winners.is_empty() {
            winner_pool / result.winners.len() as u64
        } else {
            0
        };

        // Losers (players who are not winners).
        let losers: Vec<&Address> = result
            .players
            .iter()
            .filter(|p| !result.winners.contains(p))
            .collect();

        let loser_share = if !losers.is_empty() {
            loser_pool / losers.len() as u64
        } else {
            0
        };

        for player in &result.players {
            if result.winners.contains(player) {
                payouts.push((*player, winner_share));
            } else {
                payouts.push((*player, loser_share));
            }
        }

        // If all players are winners and there's a loser_pool with no losers,
        // redistribute it among winners.
        if losers.is_empty() && loser_pool > 0 && !result.winners.is_empty() {
            let extra_per_winner = loser_pool / result.winners.len() as u64;
            for payout in &mut payouts {
                payout.1 = payout.1.saturating_add(extra_per_winner);
            }
        }

        payouts
    }

    /// Retrieve a [`MatchSettlement`] by match ID.
    pub fn get_settlement(
        store: &dyn StateStore,
        match_id: &Hash,
    ) -> AttestationResult<Option<MatchSettlement>> {
        let view = StateView::new(store);
        Ok(view.get_match_settlement(match_id)?)
    }

    // -- Internal helpers ----------------------------------------------------

    /// Compute the canonical SHA-256 hash of a match result's core data fields.
    /// This is the message that the attestor signs.
    fn canonical_match_hash(result: &MatchResult) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(result.match_id.as_bytes());
        hasher.update(result.game_id.as_bytes());
        hasher.update(result.timestamp.to_le_bytes());
        for player in &result.players {
            hasher.update(player.as_bytes());
        }
        for score in &result.scores {
            hasher.update(score.to_le_bytes());
        }
        for winner in &result.winners {
            hasher.update(winner.as_bytes());
        }
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Hash::new(out)
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

    fn sample_match_result() -> MatchResult {
        let player_a = test_addr(10);
        let player_b = test_addr(11);
        MatchResult {
            match_id: test_hash(0x42),
            game_id: "chess".to_string(),
            timestamp: 1_700_000_000,
            players: vec![player_a, player_b],
            scores: vec![1500, 1200],
            winners: vec![player_a],
            reward_pool: 10_000,
            server_signature: vec![0xAB; 64],
            anti_cheat_score: Some(95),
            replay_ref: None,
        }
    }

    #[test]
    fn check_quarantine_below_threshold() {
        let mut mr = sample_match_result();
        mr.anti_cheat_score = Some(10);
        assert!(AttestationModule::check_quarantine(&mr, 30));
    }

    #[test]
    fn check_quarantine_above_threshold() {
        let mr = sample_match_result(); // score = 95
        assert!(!AttestationModule::check_quarantine(&mr, 30));
    }

    #[test]
    fn check_quarantine_none_score() {
        let mut mr = sample_match_result();
        mr.anti_cheat_score = None;
        assert!(!AttestationModule::check_quarantine(&mr, 30));
    }

    #[test]
    fn calculate_reward_distribution_with_winner() {
        let mr = sample_match_result(); // 1 winner, 1 loser, pool = 10_000
        let dist = AttestationModule::calculate_reward_distribution(&mr);
        assert_eq!(dist.len(), 2);

        // Winner gets 60% = 6000.
        let winner_payout = dist.iter().find(|(a, _)| *a == test_addr(10)).unwrap();
        assert_eq!(winner_payout.1, 6000);

        // Loser gets 40% = 4000.
        let loser_payout = dist.iter().find(|(a, _)| *a == test_addr(11)).unwrap();
        assert_eq!(loser_payout.1, 4000);
    }

    #[test]
    fn calculate_reward_distribution_no_winners() {
        let mut mr = sample_match_result();
        mr.winners = vec![];
        let dist = AttestationModule::calculate_reward_distribution(&mr);
        assert_eq!(dist.len(), 2);
        // Equal split: 10_000 / 2 = 5000 each.
        for (_, amount) in &dist {
            assert_eq!(*amount, 5000);
        }
    }

    #[test]
    fn calculate_reward_distribution_all_winners() {
        let mut mr = sample_match_result();
        mr.winners = mr.players.clone(); // both players are winners
        let dist = AttestationModule::calculate_reward_distribution(&mr);
        assert_eq!(dist.len(), 2);
        // 60% split among 2 = 3000 each, plus 40% redistributed = 2000 each.
        // Total per winner: 5000.
        for (_, amount) in &dist {
            assert_eq!(*amount, 5000);
        }
    }

    #[test]
    fn calculate_reward_distribution_empty() {
        let mut mr = sample_match_result();
        mr.players = vec![];
        mr.winners = vec![];
        let dist = AttestationModule::calculate_reward_distribution(&mr);
        assert!(dist.is_empty());
    }

    #[test]
    fn calculate_reward_distribution_zero_pool() {
        let mut mr = sample_match_result();
        mr.reward_pool = 0;
        let dist = AttestationModule::calculate_reward_distribution(&mr);
        assert!(dist.is_empty());
    }

    #[test]
    fn verify_signature_rejects_wrong_length() {
        let mr = MatchResult {
            server_signature: vec![0xAB; 32], // wrong length
            ..sample_match_result()
        };
        // Use a dummy pubkey (all zeros won't be a valid Ed25519 point, but
        // the length check happens first).
        let result =
            AttestationModule::verify_match_result_signature(&mr, &[1u8; 32]);
        // Should either return Ok(false) for bad sig or Err for bad key.
        // Either way, it should not panic.
        match result {
            Ok(valid) => assert!(!valid),
            Err(_) => {} // also acceptable
        }
    }

    #[test]
    fn get_match_history_returns_empty() {
        let store = MemoryStore::new();
        let history =
            AttestationModule::get_match_history_for_player(&store, &test_addr(1)).unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn canonical_match_hash_deterministic() {
        let mr = sample_match_result();
        let h1 = AttestationModule::canonical_match_hash(&mr);
        let h2 = AttestationModule::canonical_match_hash(&mr);
        assert_eq!(h1, h2);
    }

    #[test]
    fn canonical_match_hash_changes_with_data() {
        let mr1 = sample_match_result();
        let mut mr2 = sample_match_result();
        mr2.scores = vec![1600, 1100]; // different scores
        assert_ne!(
            AttestationModule::canonical_match_hash(&mr1),
            AttestationModule::canonical_match_hash(&mr2)
        );
    }
}
