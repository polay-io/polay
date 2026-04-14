//! Tournament module — creation, registration, starting, results, prizes, cancellation.

use sha2::{Digest, Sha256};

use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{AccountState, Address, Event, Hash, Tournament, TournamentStatus};

use crate::error::ExecutionError;

// ---------------------------------------------------------------------------
// Create tournament
// ---------------------------------------------------------------------------

/// Create a new tournament.
pub fn execute_create_tournament(
    signer: &Address,
    name: &str,
    game_id: &str,
    entry_fee: u64,
    max_participants: u32,
    min_participants: u32,
    start_height: u64,
    prize_distribution: &[u32],
    store: &dyn StateStore,
    timestamp: u64,
) -> Result<(Hash, Vec<Event>), ExecutionError> {
    let writer = StateWriter::new(store);

    // Bound prize_distribution size to prevent unbounded vector abuse.
    if prize_distribution.len() > 100 {
        return Err(ExecutionError::InvalidInput(
            "prize_distribution must have at most 100 entries".into(),
        ));
    }

    // Generate tournament_id = sha256(organizer || name || start_height bytes).
    let mut hasher = Sha256::new();
    hasher.update(signer.as_bytes());
    hasher.update(name.as_bytes());
    hasher.update(start_height.to_le_bytes());
    let digest = hasher.finalize();
    let mut id_bytes = [0u8; 32];
    id_bytes.copy_from_slice(&digest);
    let tournament_id = Hash::new(id_bytes);

    let tournament = Tournament {
        tournament_id,
        name: name.to_string(),
        organizer: *signer,
        game_id: game_id.to_string(),
        entry_fee,
        prize_pool: 0,
        max_participants,
        min_participants,
        participants: vec![],
        status: TournamentStatus::Registration,
        start_height,
        end_height: None,
        prize_distribution: prize_distribution.to_vec(),
        rankings: vec![],
        prizes_claimed: vec![],
        created_at: timestamp,
    };

    writer.set_tournament(&tournament)?;

    Ok((
        tournament_id,
        vec![Event::tournament_created(signer, &tournament_id, name)],
    ))
}

// ---------------------------------------------------------------------------
// Join tournament
// ---------------------------------------------------------------------------

/// Join a tournament.
pub fn execute_join_tournament(
    signer: &Address,
    tournament_id: &Hash,
    store: &dyn StateStore,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Load tournament.
    let mut tournament = view
        .get_tournament(tournament_id)?
        .ok_or(ExecutionError::TournamentNotFound)?;

    // Verify status is Registration.
    if tournament.status != TournamentStatus::Registration {
        return Err(ExecutionError::TournamentNotInRegistration);
    }

    // Verify registration still open (current_height < start_height).
    let current_height = view.get_chain_height()?;
    if current_height >= tournament.start_height {
        return Err(ExecutionError::RegistrationClosed);
    }

    // Verify signer not already a participant.
    if view.is_tournament_participant(tournament_id, signer)? {
        return Err(ExecutionError::AlreadyRegistered);
    }

    // Verify room for more participants.
    if tournament.participants.len() >= tournament.max_participants as usize {
        return Err(ExecutionError::TournamentFull);
    }

    // Deduct entry_fee from signer (if non-zero).
    if tournament.entry_fee > 0 {
        let mut account = view
            .get_account(signer)?
            .ok_or_else(|| ExecutionError::AccountNotFound(signer.to_hex()))?;
        if account.balance < tournament.entry_fee {
            return Err(ExecutionError::InsufficientBalance {
                required: tournament.entry_fee,
                available: account.balance,
            });
        }
        account.balance = account.balance.checked_sub(tournament.entry_fee).ok_or(
            ExecutionError::InsufficientBalance {
                required: tournament.entry_fee,
                available: account.balance,
            },
        )?;
        writer.set_account(&account)?;
    }

    // Add entry fee to prize pool.
    tournament.prize_pool += tournament.entry_fee;

    // Add signer to participants.
    tournament.participants.push(*signer);

    // Store tournament and participant index.
    writer.set_tournament(&tournament)?;
    writer.set_tournament_participant(tournament_id, signer)?;

    Ok(vec![Event::tournament_joined(signer, tournament_id)])
}

// ---------------------------------------------------------------------------
// Start tournament
// ---------------------------------------------------------------------------

/// Start a tournament.
pub fn execute_start_tournament(
    _signer: &Address,
    tournament_id: &Hash,
    store: &dyn StateStore,
    block_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Load tournament.
    let mut tournament = view
        .get_tournament(tournament_id)?
        .ok_or(ExecutionError::TournamentNotFound)?;

    // Verify status is Registration.
    if tournament.status != TournamentStatus::Registration {
        return Err(ExecutionError::TournamentNotInRegistration);
    }

    // Verify current_height >= start_height.
    if block_height < tournament.start_height {
        return Err(ExecutionError::RegistrationClosed);
    }

    // Verify enough participants.
    if tournament.participants.len() < tournament.min_participants as usize {
        return Err(ExecutionError::NotEnoughParticipants);
    }

    // Set status to Active.
    tournament.status = TournamentStatus::Active;
    writer.set_tournament(&tournament)?;

    Ok(vec![Event::tournament_started(
        tournament_id,
        tournament.participants.len() as u32,
    )])
}

// ---------------------------------------------------------------------------
// Report tournament results
// ---------------------------------------------------------------------------

/// Report final tournament rankings.
pub fn execute_report_tournament_results(
    signer: &Address,
    tournament_id: &Hash,
    rankings: &[Address],
    store: &dyn StateStore,
    block_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Load tournament.
    let mut tournament = view
        .get_tournament(tournament_id)?
        .ok_or(ExecutionError::TournamentNotFound)?;

    // Verify status is Active.
    if tournament.status != TournamentStatus::Active {
        return Err(ExecutionError::TournamentNotActive);
    }

    // Verify signer is organizer.
    if *signer != tournament.organizer {
        return Err(ExecutionError::NotOrganizer);
    }

    // Verify every address in rankings is a participant.
    for addr in rankings {
        if !tournament.participants.contains(addr) {
            return Err(ExecutionError::InvalidRankings);
        }
    }

    // Verify rankings.len() <= prize_distribution.len().
    if rankings.len() > tournament.prize_distribution.len() {
        return Err(ExecutionError::InvalidRankings);
    }

    // Set results.
    tournament.rankings = rankings.to_vec();
    tournament.prizes_claimed = vec![false; rankings.len()];
    tournament.end_height = Some(block_height);
    tournament.status = TournamentStatus::Completed;

    writer.set_tournament(&tournament)?;

    let winner = &rankings[0];
    Ok(vec![Event::tournament_results_reported(
        tournament_id,
        winner,
    )])
}

// ---------------------------------------------------------------------------
// Claim tournament prize
// ---------------------------------------------------------------------------

/// Claim a tournament prize.
pub fn execute_claim_tournament_prize(
    signer: &Address,
    tournament_id: &Hash,
    store: &dyn StateStore,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Load tournament.
    let mut tournament = view
        .get_tournament(tournament_id)?
        .ok_or(ExecutionError::TournamentNotFound)?;

    // Verify status is Completed.
    if tournament.status != TournamentStatus::Completed {
        return Err(ExecutionError::TournamentNotCompleted);
    }

    // Find signer's index in rankings.
    let index = tournament
        .rankings
        .iter()
        .position(|addr| addr == signer)
        .ok_or(ExecutionError::NotRanked)?;

    // Verify not already claimed.
    if tournament.prizes_claimed[index] {
        return Err(ExecutionError::PrizeAlreadyClaimed);
    }

    // Calculate prize = prize_pool * prize_distribution[index] / 100.
    let prize = tournament.prize_pool * tournament.prize_distribution[index] as u64 / 100;

    // Credit prize to signer account.
    let mut account = view
        .get_account(signer)?
        .unwrap_or_else(|| AccountState::with_balance(*signer, 0, 0));
    account.balance += prize;
    writer.set_account(&account)?;

    // Mark claimed.
    tournament.prizes_claimed[index] = true;
    writer.set_tournament(&tournament)?;

    Ok(vec![Event::tournament_prize_claimed(
        signer,
        tournament_id,
        prize,
    )])
}

// ---------------------------------------------------------------------------
// Cancel tournament
// ---------------------------------------------------------------------------

/// Cancel a tournament and refund entry fees.
pub fn execute_cancel_tournament(
    signer: &Address,
    tournament_id: &Hash,
    store: &dyn StateStore,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Load tournament.
    let mut tournament = view
        .get_tournament(tournament_id)?
        .ok_or(ExecutionError::TournamentNotFound)?;

    // Can only cancel during Registration.
    if tournament.status != TournamentStatus::Registration {
        return Err(ExecutionError::CannotCancelActiveTournament);
    }

    // Verify signer is organizer.
    if *signer != tournament.organizer {
        return Err(ExecutionError::NotOrganizer);
    }

    // Refund each participant their entry_fee.
    let refunded_count = tournament.participants.len() as u32;
    if tournament.entry_fee > 0 {
        for participant in &tournament.participants {
            let mut account = view
                .get_account(participant)?
                .unwrap_or_else(|| AccountState::with_balance(*participant, 0, 0));
            account.balance += tournament.entry_fee;
            writer.set_account(&account)?;
        }
    }

    // Set status to Cancelled, zero out prize_pool.
    tournament.status = TournamentStatus::Cancelled;
    tournament.prize_pool = 0;
    writer.set_tournament(&tournament)?;

    Ok(vec![Event::tournament_cancelled(
        tournament_id,
        refunded_count,
    )])
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

    /// Helper: create a tournament via execute_create_tournament and return its id.
    fn create_test_tournament(
        store: &MemoryStore,
        organizer: &Address,
        entry_fee: u64,
        max_participants: u32,
        min_participants: u32,
        start_height: u64,
    ) -> Hash {
        let (id, events) = execute_create_tournament(
            organizer,
            "Test Cup",
            "arena",
            entry_fee,
            max_participants,
            min_participants,
            start_height,
            &[50, 30, 20],
            store,
            1000,
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "tournament_created");
        id
    }

    /// Helper: set up an account with a given balance.
    fn setup_account(store: &MemoryStore, addr: &Address, balance: u64) {
        let account = AccountState::with_balance(*addr, balance, 1);
        StateWriter::new(store).set_account(&account).unwrap();
    }

    /// Helper: set chain height.
    fn set_height(store: &MemoryStore, height: u64) {
        StateWriter::new(store).set_chain_height(height).unwrap();
    }

    // -- 1. Create tournament --------------------------------------------------

    #[test]
    fn test_create_tournament() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);

        let (id, events) = execute_create_tournament(
            &organizer,
            "Grand Arena",
            "arena",
            1000,
            32,
            2,
            100,
            &[70, 30],
            &store,
            5000,
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "tournament");
        assert_eq!(events[0].action, "tournament_created");

        let view = StateView::new(&store);
        let t = view.get_tournament(&id).unwrap().unwrap();
        assert_eq!(t.tournament_id, id);
        assert_eq!(t.name, "Grand Arena");
        assert_eq!(t.organizer, organizer);
        assert_eq!(t.game_id, "arena");
        assert_eq!(t.entry_fee, 1000);
        assert_eq!(t.prize_pool, 0);
        assert_eq!(t.max_participants, 32);
        assert_eq!(t.min_participants, 2);
        assert!(t.participants.is_empty());
        assert_eq!(t.status, TournamentStatus::Registration);
        assert_eq!(t.start_height, 100);
        assert!(t.end_height.is_none());
        assert_eq!(t.prize_distribution, vec![70, 30]);
        assert!(t.rankings.is_empty());
        assert!(t.prizes_claimed.is_empty());
        assert_eq!(t.created_at, 5000);
    }

    // -- 2. Join tournament — entry fee deducted, participant added, prize pool increased --

    #[test]
    fn test_join_tournament() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let player = test_addr(2);

        let tid = create_test_tournament(&store, &organizer, 1000, 32, 2, 100);
        setup_account(&store, &player, 5000);
        set_height(&store, 50); // before start_height

        let events = execute_join_tournament(&player, &tid, &store).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "tournament_joined");

        let view = StateView::new(&store);
        let t = view.get_tournament(&tid).unwrap().unwrap();
        assert_eq!(t.participants.len(), 1);
        assert_eq!(t.participants[0], player);
        assert_eq!(t.prize_pool, 1000);

        // Balance deducted.
        let acct = view.get_account(&player).unwrap().unwrap();
        assert_eq!(acct.balance, 4000);

        // Participant index set.
        assert!(view.is_tournament_participant(&tid, &player).unwrap());
    }

    // -- 3. Join full tournament — rejected --

    #[test]
    fn test_join_full_tournament() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);

        let tid = create_test_tournament(&store, &organizer, 100, 2, 2, 100);
        set_height(&store, 50);

        // Fill the tournament with 2 participants (max).
        for i in 10..12u8 {
            let p = test_addr(i);
            setup_account(&store, &p, 1000);
            execute_join_tournament(&p, &tid, &store).unwrap();
        }

        // Third participant should fail.
        let extra = test_addr(20);
        setup_account(&store, &extra, 1000);
        let err = execute_join_tournament(&extra, &tid, &store).unwrap_err();
        assert_eq!(err, ExecutionError::TournamentFull);
    }

    // -- 4. Join after start_height — rejected --

    #[test]
    fn test_join_after_start_height() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let player = test_addr(2);

        let tid = create_test_tournament(&store, &organizer, 100, 32, 2, 100);
        setup_account(&store, &player, 5000);
        set_height(&store, 100); // at start_height, registration closed

        let err = execute_join_tournament(&player, &tid, &store).unwrap_err();
        assert_eq!(err, ExecutionError::RegistrationClosed);
    }

    // -- 5. Double join — rejected --

    #[test]
    fn test_double_join() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let player = test_addr(2);

        let tid = create_test_tournament(&store, &organizer, 100, 32, 2, 100);
        setup_account(&store, &player, 5000);
        set_height(&store, 50);

        execute_join_tournament(&player, &tid, &store).unwrap();
        let err = execute_join_tournament(&player, &tid, &store).unwrap_err();
        assert_eq!(err, ExecutionError::AlreadyRegistered);
    }

    // -- 6. Start tournament — status becomes Active --

    #[test]
    fn test_start_tournament() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);

        let tid = create_test_tournament(&store, &organizer, 100, 32, 2, 100);
        set_height(&store, 50);

        // Add two participants.
        for i in 10..12u8 {
            let p = test_addr(i);
            setup_account(&store, &p, 1000);
            execute_join_tournament(&p, &tid, &store).unwrap();
        }

        // Start at block_height >= start_height.
        let events = execute_start_tournament(&organizer, &tid, &store, 100).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "tournament_started");

        let view = StateView::new(&store);
        let t = view.get_tournament(&tid).unwrap().unwrap();
        assert_eq!(t.status, TournamentStatus::Active);
    }

    // -- 7. Start with too few participants — rejected --

    #[test]
    fn test_start_too_few_participants() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);

        let tid = create_test_tournament(&store, &organizer, 100, 32, 2, 100);
        set_height(&store, 50);

        // Only one participant.
        let p = test_addr(10);
        setup_account(&store, &p, 1000);
        execute_join_tournament(&p, &tid, &store).unwrap();

        let err = execute_start_tournament(&organizer, &tid, &store, 100).unwrap_err();
        assert_eq!(err, ExecutionError::NotEnoughParticipants);
    }

    // -- 8. Report results by organizer — rankings stored, status Completed --

    #[test]
    fn test_report_results() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let p1 = test_addr(10);
        let p2 = test_addr(11);

        let tid = create_test_tournament(&store, &organizer, 100, 32, 2, 100);
        set_height(&store, 50);

        setup_account(&store, &p1, 1000);
        setup_account(&store, &p2, 1000);
        execute_join_tournament(&p1, &tid, &store).unwrap();
        execute_join_tournament(&p2, &tid, &store).unwrap();

        execute_start_tournament(&organizer, &tid, &store, 100).unwrap();

        let events =
            execute_report_tournament_results(&organizer, &tid, &[p1, p2], &store, 150).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "tournament_results_reported");

        let view = StateView::new(&store);
        let t = view.get_tournament(&tid).unwrap().unwrap();
        assert_eq!(t.status, TournamentStatus::Completed);
        assert_eq!(t.rankings, vec![p1, p2]);
        assert_eq!(t.prizes_claimed, vec![false, false]);
        assert_eq!(t.end_height, Some(150));
    }

    // -- 9. Report by non-organizer — rejected --

    #[test]
    fn test_report_by_non_organizer() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let impostor = test_addr(99);
        let p1 = test_addr(10);
        let p2 = test_addr(11);

        let tid = create_test_tournament(&store, &organizer, 100, 32, 2, 100);
        set_height(&store, 50);

        setup_account(&store, &p1, 1000);
        setup_account(&store, &p2, 1000);
        execute_join_tournament(&p1, &tid, &store).unwrap();
        execute_join_tournament(&p2, &tid, &store).unwrap();

        execute_start_tournament(&organizer, &tid, &store, 100).unwrap();

        let err =
            execute_report_tournament_results(&impostor, &tid, &[p1, p2], &store, 150).unwrap_err();
        assert_eq!(err, ExecutionError::NotOrganizer);
    }

    // -- 10. Claim prize — correct amount credited --

    #[test]
    fn test_claim_prize() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let p1 = test_addr(10);
        let p2 = test_addr(11);

        // prize_distribution = [50, 30, 20], entry_fee = 1000
        let tid = create_test_tournament(&store, &organizer, 1000, 32, 2, 100);
        set_height(&store, 50);

        setup_account(&store, &p1, 5000);
        setup_account(&store, &p2, 5000);
        execute_join_tournament(&p1, &tid, &store).unwrap();
        execute_join_tournament(&p2, &tid, &store).unwrap();

        // prize_pool = 2000
        execute_start_tournament(&organizer, &tid, &store, 100).unwrap();
        execute_report_tournament_results(&organizer, &tid, &[p1, p2], &store, 150).unwrap();

        // p1 claims 1st: 2000 * 50 / 100 = 1000
        let events = execute_claim_tournament_prize(&p1, &tid, &store).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "tournament_prize_claimed");
        assert_eq!(events[0].get_attribute("amount"), Some("1000"));

        let view = StateView::new(&store);
        // p1 started with 5000, paid 1000 entry, received 1000 prize => 5000
        let acct = view.get_account(&p1).unwrap().unwrap();
        assert_eq!(acct.balance, 5000);

        // p2 claims 2nd: 2000 * 30 / 100 = 600
        let events = execute_claim_tournament_prize(&p2, &tid, &store).unwrap();
        assert_eq!(events[0].get_attribute("amount"), Some("600"));
        let acct = view.get_account(&p2).unwrap().unwrap();
        assert_eq!(acct.balance, 4600); // 5000 - 1000 + 600
    }

    // -- 11. Double claim — rejected --

    #[test]
    fn test_double_claim() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let p1 = test_addr(10);
        let p2 = test_addr(11);

        let tid = create_test_tournament(&store, &organizer, 1000, 32, 2, 100);
        set_height(&store, 50);

        setup_account(&store, &p1, 5000);
        setup_account(&store, &p2, 5000);
        execute_join_tournament(&p1, &tid, &store).unwrap();
        execute_join_tournament(&p2, &tid, &store).unwrap();

        execute_start_tournament(&organizer, &tid, &store, 100).unwrap();
        execute_report_tournament_results(&organizer, &tid, &[p1, p2], &store, 150).unwrap();

        execute_claim_tournament_prize(&p1, &tid, &store).unwrap();
        let err = execute_claim_tournament_prize(&p1, &tid, &store).unwrap_err();
        assert_eq!(err, ExecutionError::PrizeAlreadyClaimed);
    }

    // -- 12. Non-ranked player claims — rejected --

    #[test]
    fn test_non_ranked_claim() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let p1 = test_addr(10);
        let p2 = test_addr(11);
        let p3 = test_addr(12);

        let tid = create_test_tournament(&store, &organizer, 1000, 32, 2, 100);
        set_height(&store, 50);

        setup_account(&store, &p1, 5000);
        setup_account(&store, &p2, 5000);
        setup_account(&store, &p3, 5000);
        execute_join_tournament(&p1, &tid, &store).unwrap();
        execute_join_tournament(&p2, &tid, &store).unwrap();
        execute_join_tournament(&p3, &tid, &store).unwrap();

        execute_start_tournament(&organizer, &tid, &store, 100).unwrap();
        // Only p1 and p2 are ranked.
        execute_report_tournament_results(&organizer, &tid, &[p1, p2], &store, 150).unwrap();

        let err = execute_claim_tournament_prize(&p3, &tid, &store).unwrap_err();
        assert_eq!(err, ExecutionError::NotRanked);
    }

    // -- 13. Cancel during registration — all participants refunded --

    #[test]
    fn test_cancel_tournament() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let p1 = test_addr(10);
        let p2 = test_addr(11);

        let tid = create_test_tournament(&store, &organizer, 1000, 32, 2, 100);
        set_height(&store, 50);

        setup_account(&store, &p1, 5000);
        setup_account(&store, &p2, 5000);
        execute_join_tournament(&p1, &tid, &store).unwrap();
        execute_join_tournament(&p2, &tid, &store).unwrap();

        // Verify balances after joining.
        let view = StateView::new(&store);
        assert_eq!(view.get_account(&p1).unwrap().unwrap().balance, 4000);
        assert_eq!(view.get_account(&p2).unwrap().unwrap().balance, 4000);

        // Cancel.
        let events = execute_cancel_tournament(&organizer, &tid, &store).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "tournament_cancelled");
        assert_eq!(events[0].get_attribute("refunded_count"), Some("2"));

        // Verify refunds.
        let view = StateView::new(&store);
        assert_eq!(view.get_account(&p1).unwrap().unwrap().balance, 5000);
        assert_eq!(view.get_account(&p2).unwrap().unwrap().balance, 5000);

        // Verify tournament state.
        let t = view.get_tournament(&tid).unwrap().unwrap();
        assert_eq!(t.status, TournamentStatus::Cancelled);
        assert_eq!(t.prize_pool, 0);
    }

    // -- 14. Cancel active tournament — rejected --

    #[test]
    fn test_cancel_active_tournament() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let p1 = test_addr(10);
        let p2 = test_addr(11);

        let tid = create_test_tournament(&store, &organizer, 100, 32, 2, 100);
        set_height(&store, 50);

        setup_account(&store, &p1, 1000);
        setup_account(&store, &p2, 1000);
        execute_join_tournament(&p1, &tid, &store).unwrap();
        execute_join_tournament(&p2, &tid, &store).unwrap();

        execute_start_tournament(&organizer, &tid, &store, 100).unwrap();

        let err = execute_cancel_tournament(&organizer, &tid, &store).unwrap_err();
        assert_eq!(err, ExecutionError::CannotCancelActiveTournament);
    }

    // -- 15. Free tournament (entry_fee=0) — works correctly --

    #[test]
    fn test_free_tournament() {
        let store = MemoryStore::new();
        let organizer = test_addr(1);
        let p1 = test_addr(10);
        let p2 = test_addr(11);

        // entry_fee = 0
        let tid = create_test_tournament(&store, &organizer, 0, 32, 2, 100);
        set_height(&store, 50);

        // Players don't need balance for a free tournament, but we still
        // need accounts for prize claims.
        setup_account(&store, &p1, 0);
        setup_account(&store, &p2, 0);

        // Join succeeds without paying.
        execute_join_tournament(&p1, &tid, &store).unwrap();
        execute_join_tournament(&p2, &tid, &store).unwrap();

        let view = StateView::new(&store);
        let t = view.get_tournament(&tid).unwrap().unwrap();
        assert_eq!(t.prize_pool, 0);
        assert_eq!(t.participants.len(), 2);

        // Balances unchanged.
        assert_eq!(view.get_account(&p1).unwrap().unwrap().balance, 0);
        assert_eq!(view.get_account(&p2).unwrap().unwrap().balance, 0);

        // Start and report.
        execute_start_tournament(&organizer, &tid, &store, 100).unwrap();
        execute_report_tournament_results(&organizer, &tid, &[p1, p2], &store, 150).unwrap();

        // Claim prizes — prize is 0 * X / 100 = 0 each, should succeed.
        let events = execute_claim_tournament_prize(&p1, &tid, &store).unwrap();
        assert_eq!(events[0].get_attribute("amount"), Some("0"));

        let events = execute_claim_tournament_prize(&p2, &tid, &store).unwrap();
        assert_eq!(events[0].get_attribute("amount"), Some("0"));

        // Verify prizes_claimed all true.
        let t = view.get_tournament(&tid).unwrap().unwrap();
        assert!(t.prizes_claimed.iter().all(|&c| c));
    }
}
