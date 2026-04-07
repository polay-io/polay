//! Attestation module — attestor registration, match result submission,
//! and reward distribution.

use polay_config::ChainConfig;
use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{
    AccountState, Address, Attestor, AttestorStatus, Event, Hash, MatchResult,
    MatchSettlement,
};
use tracing::debug;

use crate::error::ExecutionError;

// ---------------------------------------------------------------------------
// Register attestor
// ---------------------------------------------------------------------------

/// Register the signer as a game-server attestor for a specific game.
pub fn execute_register_attestor(
    signer: &Address,
    game_id: &str,
    endpoint: &str,
    metadata: &str,
    store: &dyn StateStore,
    timestamp: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);

    if view.get_attestor(signer)?.is_some() {
        return Err(ExecutionError::AttestorAlreadyRegistered);
    }

    let attestor = Attestor {
        address: *signer,
        game_id: game_id.to_string(),
        endpoint: endpoint.to_string(),
        metadata: metadata.to_string(),
        status: AttestorStatus::Active,
        registered_at: timestamp,
    };

    StateWriter::new(store).set_attestor(&attestor)?;

    debug!(
        attestor = %signer,
        game_id,
        "attestor registered"
    );

    Ok(vec![Event::new(
        "attestation",
        "attestor_registered",
        vec![
            ("address".into(), signer.to_hex()),
            ("game_id".into(), game_id.to_string()),
        ],
    )])
}

// ---------------------------------------------------------------------------
// Submit match result
// ---------------------------------------------------------------------------

/// Submit a verified match result from an authorized attestor.
///
/// If the anti-cheat score is present and below the configured quarantine
/// threshold, the resulting settlement record is marked as quarantined.
pub fn execute_submit_match_result(
    signer: &Address,
    match_result: &MatchResult,
    store: &dyn StateStore,
    config: &ChainConfig,
    timestamp: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Verify signer is a registered and active attestor.
    let attestor = view
        .get_attestor(signer)?
        .ok_or(ExecutionError::AttestorNotFound)?;

    if !attestor.can_submit() {
        return Err(ExecutionError::AttestorNotActive);
    }

    // Game ID must match.
    if attestor.game_id != match_result.game_id {
        return Err(ExecutionError::Unauthorized);
    }

    // Match result must be well-formed.
    if !match_result.is_well_formed() {
        return Err(ExecutionError::InvalidMatchResult(
            "players/scores/winners inconsistent".to_string(),
        ));
    }

    // Store the match result.
    writer.set_match_result(match_result)?;

    // Determine quarantine status.
    let quarantined = match match_result.anti_cheat_score {
        Some(score) if score < config.attestation_quarantine_threshold => true,
        _ => false,
    };

    // Create the settlement record.
    let settlement = MatchSettlement {
        match_id: match_result.match_id,
        settled: false,
        rewards_distributed: Vec::new(),
        quarantined,
        settled_at: timestamp,
    };
    writer.set_match_settlement(&settlement)?;

    debug!(
        match_id = %match_result.match_id,
        game_id = match_result.game_id,
        quarantined,
        "match result submitted"
    );

    Ok(vec![Event::match_result_submitted(
        &match_result.match_id,
        &match_result.game_id,
    )])
}

// ---------------------------------------------------------------------------
// Distribute reward
// ---------------------------------------------------------------------------

/// Distribute rewards from a settled match to players.
///
/// The signer must be the attestor who originally submitted the match.
/// Total rewards must not exceed the match's reward pool.
pub fn execute_distribute_reward(
    signer: &Address,
    match_id: &Hash,
    rewards: &[(Address, u64)],
    store: &dyn StateStore,
    timestamp: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Get settlement.
    let mut settlement = view
        .get_match_settlement(match_id)?
        .ok_or(ExecutionError::MatchSettlementNotFound)?;

    if settlement.settled {
        return Err(ExecutionError::MatchAlreadySettled);
    }
    if settlement.quarantined {
        return Err(ExecutionError::MatchQuarantined);
    }

    // Verify signer is the attestor who submitted this match.
    let attestor = view
        .get_attestor(signer)?
        .ok_or(ExecutionError::AttestorNotFound)?;

    // Get the match result to verify reward pool.
    let match_result = view
        .get_match_result(match_id)?
        .ok_or(ExecutionError::MatchResultNotFound)?;

    // Verify attestor's game matches.
    if attestor.game_id != match_result.game_id {
        return Err(ExecutionError::Unauthorized);
    }

    // Verify total rewards do not exceed the pool.
    let total_distributed: u64 = rewards.iter().map(|(_, amount)| amount).sum();
    if total_distributed > match_result.reward_pool {
        return Err(ExecutionError::RewardPoolExceeded {
            pool: match_result.reward_pool,
            total_distributed,
        });
    }

    // Distribute rewards.
    for (player, amount) in rewards {
        let mut account = view
            .get_account(player)?
            .unwrap_or_else(|| AccountState::new(*player, timestamp));
        account.balance = account.balance.saturating_add(*amount);
        writer.set_account(&account)?;
    }

    // Mark as settled.
    settlement.settled = true;
    settlement.rewards_distributed = rewards.to_vec();
    settlement.settled_at = timestamp;
    writer.set_match_settlement(&settlement)?;

    debug!(
        match_id = %match_id,
        total_distributed,
        num_recipients = rewards.len(),
        "rewards distributed"
    );

    Ok(vec![Event::rewards_distributed(match_id, total_distributed)])
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

    fn default_config() -> ChainConfig {
        ChainConfig::default()
    }

    fn register_test_attestor(store: &MemoryStore, addr: &Address, game_id: &str) {
        execute_register_attestor(addr, game_id, "https://attestor.test", "{}", store, 100)
            .unwrap();
    }

    fn sample_match_result(game_id: &str) -> MatchResult {
        let player_a = test_addr(10);
        let player_b = test_addr(11);
        MatchResult {
            match_id: test_hash(0x42),
            game_id: game_id.to_string(),
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
    fn register_attestor_happy_path() {
        let store = MemoryStore::new();
        let addr = test_addr(1);

        let events = execute_register_attestor(
            &addr,
            "chess",
            "https://attestor.test",
            "{}",
            &store,
            100,
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "attestation");

        let att = StateView::new(&store).get_attestor(&addr).unwrap().unwrap();
        assert_eq!(att.game_id, "chess");
        assert!(att.can_submit());
    }

    #[test]
    fn register_attestor_duplicate() {
        let store = MemoryStore::new();
        let addr = test_addr(1);

        register_test_attestor(&store, &addr, "chess");
        let err = execute_register_attestor(&addr, "chess", "url", "{}", &store, 200).unwrap_err();
        assert!(matches!(err, ExecutionError::AttestorAlreadyRegistered));
    }

    #[test]
    fn submit_match_result_happy_path() {
        let store = MemoryStore::new();
        let config = default_config();
        let attestor_addr = test_addr(1);

        register_test_attestor(&store, &attestor_addr, "chess");

        let mr = sample_match_result("chess");
        let events =
            execute_submit_match_result(&attestor_addr, &mr, &store, &config, 500).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "attestation");
        assert_eq!(events[0].action, "match_result_submitted");

        let settlement = StateView::new(&store)
            .get_match_settlement(&mr.match_id)
            .unwrap()
            .unwrap();
        assert!(!settlement.settled);
        assert!(!settlement.quarantined);
    }

    #[test]
    fn submit_match_result_quarantined() {
        let store = MemoryStore::new();
        let config = default_config(); // threshold = 30
        let attestor_addr = test_addr(1);

        register_test_attestor(&store, &attestor_addr, "chess");

        let mut mr = sample_match_result("chess");
        mr.anti_cheat_score = Some(10); // below threshold
        execute_submit_match_result(&attestor_addr, &mr, &store, &config, 500).unwrap();

        let settlement = StateView::new(&store)
            .get_match_settlement(&mr.match_id)
            .unwrap()
            .unwrap();
        assert!(settlement.quarantined);
    }

    #[test]
    fn submit_match_result_wrong_game() {
        let store = MemoryStore::new();
        let config = default_config();
        let attestor_addr = test_addr(1);

        register_test_attestor(&store, &attestor_addr, "chess");

        let mr = sample_match_result("poker"); // attestor is for chess
        let err =
            execute_submit_match_result(&attestor_addr, &mr, &store, &config, 500).unwrap_err();
        assert!(matches!(err, ExecutionError::Unauthorized));
    }

    #[test]
    fn submit_match_result_malformed() {
        let store = MemoryStore::new();
        let config = default_config();
        let attestor_addr = test_addr(1);

        register_test_attestor(&store, &attestor_addr, "chess");

        let mut mr = sample_match_result("chess");
        mr.scores.pop(); // make players/scores mismatch
        let err =
            execute_submit_match_result(&attestor_addr, &mr, &store, &config, 500).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidMatchResult(_)));
    }

    #[test]
    fn distribute_reward_happy_path() {
        let store = MemoryStore::new();
        let config = default_config();
        let attestor_addr = test_addr(1);
        let player_a = test_addr(10);
        let player_b = test_addr(11);

        register_test_attestor(&store, &attestor_addr, "chess");
        let mr = sample_match_result("chess"); // reward_pool = 10_000
        execute_submit_match_result(&attestor_addr, &mr, &store, &config, 500).unwrap();

        let rewards = vec![(player_a, 7000), (player_b, 3000)];
        let events =
            execute_distribute_reward(&attestor_addr, &mr.match_id, &rewards, &store, 600)
                .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "rewards_distributed");

        let view = StateView::new(&store);
        assert_eq!(view.get_account(&player_a).unwrap().unwrap().balance, 7000);
        assert_eq!(view.get_account(&player_b).unwrap().unwrap().balance, 3000);

        let settlement = view.get_match_settlement(&mr.match_id).unwrap().unwrap();
        assert!(settlement.settled);
        assert_eq!(settlement.rewards_distributed.len(), 2);
    }

    #[test]
    fn distribute_reward_already_settled() {
        let store = MemoryStore::new();
        let config = default_config();
        let attestor_addr = test_addr(1);

        register_test_attestor(&store, &attestor_addr, "chess");
        let mr = sample_match_result("chess");
        execute_submit_match_result(&attestor_addr, &mr, &store, &config, 500).unwrap();

        let rewards = vec![(test_addr(10), 5000)];
        execute_distribute_reward(&attestor_addr, &mr.match_id, &rewards, &store, 600).unwrap();

        // Try to distribute again.
        let err = execute_distribute_reward(&attestor_addr, &mr.match_id, &rewards, &store, 700)
            .unwrap_err();
        assert!(matches!(err, ExecutionError::MatchAlreadySettled));
    }

    #[test]
    fn distribute_reward_quarantined() {
        let store = MemoryStore::new();
        let config = default_config();
        let attestor_addr = test_addr(1);

        register_test_attestor(&store, &attestor_addr, "chess");
        let mut mr = sample_match_result("chess");
        mr.anti_cheat_score = Some(5); // quarantined
        execute_submit_match_result(&attestor_addr, &mr, &store, &config, 500).unwrap();

        let rewards = vec![(test_addr(10), 5000)];
        let err = execute_distribute_reward(&attestor_addr, &mr.match_id, &rewards, &store, 600)
            .unwrap_err();
        assert!(matches!(err, ExecutionError::MatchQuarantined));
    }

    #[test]
    fn distribute_reward_exceeds_pool() {
        let store = MemoryStore::new();
        let config = default_config();
        let attestor_addr = test_addr(1);

        register_test_attestor(&store, &attestor_addr, "chess");
        let mr = sample_match_result("chess"); // pool = 10_000
        execute_submit_match_result(&attestor_addr, &mr, &store, &config, 500).unwrap();

        let rewards = vec![(test_addr(10), 15_000)]; // exceeds pool
        let err = execute_distribute_reward(&attestor_addr, &mr.match_id, &rewards, &store, 600)
            .unwrap_err();
        assert!(matches!(err, ExecutionError::RewardPoolExceeded { .. }));
    }
}
