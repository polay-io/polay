//! Governance module -- proposal submission, voting, and execution.

use polay_config::ChainConfig;
use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{
    governance::{Proposal, ProposalAction, ProposalStatus, Vote, VoteOption},
    Address, Event, Hash,
};
use sha2::{Digest, Sha256};
use tracing::debug;

use crate::error::ExecutionError;

// ---------------------------------------------------------------------------
// Submit proposal
// ---------------------------------------------------------------------------

/// Submit a new governance proposal.
///
/// - Verifies deposit >= min_proposal_deposit
/// - Debits deposit from signer
/// - Creates Proposal with Voting status
/// - Stores proposal and adds to proposal list
/// - Returns (proposal_id, events)
pub fn execute_submit_proposal(
    signer: &Address,
    action: ProposalAction,
    title: String,
    description: String,
    deposit: u64,
    store: &dyn StateStore,
    config: &ChainConfig,
    current_height: u64,
) -> Result<(Hash, Vec<Event>), ExecutionError> {
    // Validate deposit.
    if deposit < config.min_proposal_deposit {
        return Err(ExecutionError::InsufficientDeposit {
            required: config.min_proposal_deposit,
            provided: deposit,
        });
    }

    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Debit deposit from signer.
    let mut account = view
        .get_account(signer)?
        .ok_or_else(|| ExecutionError::AccountNotFound(signer.to_hex()))?;
    if account.balance < deposit {
        return Err(ExecutionError::InsufficientBalance {
            required: deposit,
            available: account.balance,
        });
    }
    account.balance =
        account
            .balance
            .checked_sub(deposit)
            .ok_or(ExecutionError::InsufficientBalance {
                required: deposit,
                available: account.balance,
            })?;
    writer.set_account(&account)?;

    // Generate proposal ID = sha256(signer || title || current_height).
    let proposal_id = {
        let mut hasher = Sha256::new();
        hasher.update(signer.as_bytes());
        hasher.update(title.as_bytes());
        hasher.update(current_height.to_be_bytes());
        let result = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&result);
        Hash::new(bytes)
    };

    let voting_end_height = current_height + config.voting_period_blocks;

    let proposal = Proposal {
        id: proposal_id,
        proposer: *signer,
        action,
        title: title.clone(),
        description,
        deposit,
        status: ProposalStatus::Voting,
        yes_votes: 0,
        no_votes: 0,
        abstain_votes: 0,
        voting_start_height: current_height,
        voting_end_height,
        created_at: current_height,
    };

    writer.set_proposal(&proposal)?;

    // Add to proposal list.
    let mut list = view.get_proposal_list()?;
    list.push(proposal_id);
    writer.set_proposal_list(&list)?;

    debug!(
        proposal_id = %proposal_id,
        proposer = %signer,
        title = %title,
        "governance proposal submitted"
    );

    Ok((
        proposal_id,
        vec![Event::proposal_submitted(&proposal_id, signer, &title)],
    ))
}

// ---------------------------------------------------------------------------
// Vote on proposal
// ---------------------------------------------------------------------------

/// Vote on an active governance proposal.
///
/// - Gets proposal, verifies it is in Voting status
/// - Verifies current_height <= voting_end_height
/// - Gets voter's stake (validator stake + delegated stake)
/// - Rejects if voter has no stake
/// - If voter already voted, updates the vote and adjusts tallies
/// - Records vote, updates proposal tallies
/// - Returns events
pub fn execute_vote_proposal(
    signer: &Address,
    proposal_id: &Hash,
    option: VoteOption,
    store: &dyn StateStore,
    current_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Get proposal.
    let mut proposal = view
        .get_proposal(proposal_id)?
        .ok_or(ExecutionError::ProposalNotFound)?;

    // Verify voting status.
    if proposal.status != ProposalStatus::Voting {
        return Err(ExecutionError::ProposalNotActive);
    }

    // Verify voting period has not ended.
    if current_height > proposal.voting_end_height {
        return Err(ExecutionError::VotingPeriodEnded);
    }

    // Calculate voter's stake weight.
    // Check if they are a validator (self-stake).
    let validator_stake = view.get_validator(signer)?.map(|v| v.stake).unwrap_or(0);

    // For a simple implementation, we use the validator's total stake
    // (which includes delegations) if they are a validator,
    // or check if they have a delegation to any validator.
    // We use a prefix scan on delegation keys to find all delegations.
    let weight = if validator_stake > 0 {
        validator_stake
    } else {
        // Check if they have any balance staked via the account balance.
        // In this MVP, only validators and delegators with explicit delegations
        // can vote. We look up if this address has a delegation.
        // Since we cannot easily iterate all delegations for a delegator,
        // we check the account balance as a proxy: the balance key stores
        // the available (non-staked) balance. For a proper implementation
        // we would scan delegations. For now, check if there is a validator
        // at this address, or use a simplified approach by checking if the
        // address has any balance at all as a fallback.
        //
        // Better approach: scan the PREFIX_DELEGATION prefix for this signer.
        let delegation_prefix = {
            let mut key = Vec::with_capacity(1 + Address::LEN);
            key.push(polay_state::PREFIX_DELEGATION);
            key.extend_from_slice(signer.as_bytes());
            key
        };
        let pairs = store.prefix_scan(&delegation_prefix)?;
        let mut total_delegated = 0u64;
        for (_key, value) in pairs {
            if let Ok(del) = borsh::from_slice::<polay_types::Delegation>(&value) {
                total_delegated = total_delegated.saturating_add(del.amount);
            }
        }
        total_delegated
    };

    if weight == 0 {
        return Err(ExecutionError::NoStakeToVote);
    }

    // Check if the voter has already voted; if so, undo the previous vote.
    if let Some(existing_vote) = view.get_vote(proposal_id, signer)? {
        // Subtract old vote weight from tallies.
        match existing_vote.option {
            VoteOption::Yes => {
                proposal.yes_votes = proposal.yes_votes.saturating_sub(existing_vote.weight);
            }
            VoteOption::No => {
                proposal.no_votes = proposal.no_votes.saturating_sub(existing_vote.weight);
            }
            VoteOption::Abstain => {
                proposal.abstain_votes =
                    proposal.abstain_votes.saturating_sub(existing_vote.weight);
            }
        }
    }

    // Record the new vote.
    let vote = Vote {
        proposal_id: *proposal_id,
        voter: *signer,
        option: option.clone(),
        weight,
        height: current_height,
    };
    writer.set_vote(&vote)?;

    // Update proposal tallies.
    match option {
        VoteOption::Yes => proposal.yes_votes = proposal.yes_votes.saturating_add(weight),
        VoteOption::No => proposal.no_votes = proposal.no_votes.saturating_add(weight),
        VoteOption::Abstain => {
            proposal.abstain_votes = proposal.abstain_votes.saturating_add(weight)
        }
    }
    writer.set_proposal(&proposal)?;

    debug!(
        proposal_id = %proposal_id,
        voter = %signer,
        option = vote.option.label(),
        weight,
        "vote cast"
    );

    Ok(vec![Event::vote_cast(
        proposal_id,
        signer,
        vote.option.label(),
        weight,
    )])
}

// ---------------------------------------------------------------------------
// Execute proposal
// ---------------------------------------------------------------------------

/// Execute a proposal after its voting period ends.
///
/// - Verifies the voting period has ended
/// - Calculates quorum: (yes + no + abstain) / total_staked >= quorum_bps
/// - Calculates pass: yes / (yes + no) >= pass_threshold_bps
/// - If passed, executes the action and returns deposit
/// - If not passed, burns 50% of deposit and returns the rest
pub fn execute_execute_proposal(
    _signer: &Address,
    proposal_id: &Hash,
    store: &dyn StateStore,
    config: &ChainConfig,
    current_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Get proposal.
    let mut proposal = view
        .get_proposal(proposal_id)?
        .ok_or(ExecutionError::ProposalNotFound)?;

    // Verify proposal is in Voting status.
    if proposal.status != ProposalStatus::Voting {
        return Err(ExecutionError::ProposalNotActive);
    }

    // Verify voting period has ended.
    if current_height <= proposal.voting_end_height {
        return Err(ExecutionError::VotingPeriodNotEnded);
    }

    // Calculate total staked across all validators via prefix scan.
    let total_staked = calculate_total_staked(store)?;
    let total_votes = proposal
        .yes_votes
        .saturating_add(proposal.no_votes)
        .saturating_add(proposal.abstain_votes);

    let mut events = Vec::new();

    // Check quorum: total_votes / total_staked >= quorum_bps / 10_000
    // Rearranged to avoid floating point: total_votes * 10_000 >= quorum_bps * total_staked
    let quorum_met = if total_staked == 0 {
        false
    } else {
        (total_votes as u128) * 10_000
            >= (config.governance_quorum_bps as u128) * (total_staked as u128)
    };

    // Check pass threshold: yes / (yes + no) >= pass_threshold_bps / 10_000
    let yes_and_no = proposal.yes_votes.saturating_add(proposal.no_votes);
    let threshold_met = if yes_and_no == 0 {
        false
    } else {
        (proposal.yes_votes as u128) * 10_000
            >= (config.pass_threshold_bps as u128) * (yes_and_no as u128)
    };

    if quorum_met && threshold_met {
        // Proposal passed.
        proposal.status = ProposalStatus::Passed;
        events.push(Event::proposal_passed(proposal_id));

        // Execute the action.
        execute_proposal_action(&proposal.action, store, &writer)?;

        proposal.status = ProposalStatus::Executed;
        events.push(Event::proposal_executed(proposal_id));

        // Return full deposit to proposer.
        let mut proposer_account = view
            .get_account(&proposal.proposer)?
            .unwrap_or_else(|| polay_types::AccountState::new(proposal.proposer, current_height));
        proposer_account.credit(proposal.deposit);
        writer.set_account(&proposer_account)?;

        debug!(
            proposal_id = %proposal_id,
            "governance proposal executed"
        );
    } else {
        // Proposal rejected.
        proposal.status = ProposalStatus::Rejected;
        events.push(Event::proposal_rejected(proposal_id));

        // Burn 50% of deposit, return the rest to proposer.
        let return_amount = proposal.deposit / 2;
        if return_amount > 0 {
            let mut proposer_account = view.get_account(&proposal.proposer)?.unwrap_or_else(|| {
                polay_types::AccountState::new(proposal.proposer, current_height)
            });
            proposer_account.credit(return_amount);
            writer.set_account(&proposer_account)?;
        }

        debug!(
            proposal_id = %proposal_id,
            quorum_met,
            threshold_met,
            "governance proposal rejected"
        );
    }

    writer.set_proposal(&proposal)?;

    Ok(events)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Calculate total staked across all validators via prefix scan.
fn calculate_total_staked(store: &dyn StateStore) -> Result<u64, ExecutionError> {
    let prefix = vec![polay_state::PREFIX_VALIDATOR];
    let pairs = store.prefix_scan(&prefix)?;
    let mut total = 0u64;
    for (_key, value) in pairs {
        if let Ok(info) = borsh::from_slice::<polay_types::ValidatorInfo>(&value) {
            total = total.saturating_add(info.stake);
        }
    }
    Ok(total)
}

/// Execute the on-chain action of a passed proposal.
fn execute_proposal_action(
    action: &ProposalAction,
    store: &dyn StateStore,
    writer: &StateWriter<'_>,
) -> Result<(), ExecutionError> {
    match action {
        ProposalAction::ParameterChange {
            parameter,
            new_value,
        } => {
            // For MVP, log the change. Actual config hot-reload is TODO.
            debug!(
                parameter = %parameter,
                new_value = %new_value,
                "governance parameter change recorded (hot-reload TODO)"
            );
            Ok(())
        }
        ProposalAction::TreasurySpend {
            recipient,
            amount,
            reason,
        } => {
            // Transfer from Address::ZERO (treasury) to recipient.
            let view = StateView::new(store);
            let mut recipient_account = view
                .get_account(recipient)?
                .unwrap_or_else(|| polay_types::AccountState::new(*recipient, 0));
            recipient_account.credit(*amount);
            writer.set_account(&recipient_account)?;

            debug!(
                recipient = %recipient,
                amount,
                reason = %reason,
                "governance treasury spend executed"
            );
            Ok(())
        }
        ProposalAction::SlashValidator {
            address,
            fraction_bps,
            reason,
        } => {
            let view = StateView::new(store);
            let mut validator = view
                .get_validator(address)?
                .ok_or(ExecutionError::ValidatorNotFound)?;

            let slash_amount =
                ((validator.stake as u128) * (*fraction_bps as u128) / 10_000) as u64;
            validator.stake = validator.stake.saturating_sub(slash_amount);
            writer.set_validator(&validator)?;

            debug!(
                validator = %address,
                slash_amount,
                reason = %reason,
                "governance slash executed"
            );
            Ok(())
        }
        ProposalAction::ApproveAttestor { address, game_id } => {
            debug!(
                attestor = %address,
                game_id = %game_id,
                "governance attestor approval recorded"
            );
            Ok(())
        }
        ProposalAction::SuspendAttestor { address } => {
            let view = StateView::new(store);
            if let Some(mut attestor) = view.get_attestor(address)? {
                attestor.status = polay_types::attestation::AttestorStatus::Suspended;
                writer.set_attestor(&attestor)?;
            }
            debug!(
                attestor = %address,
                "governance attestor suspended"
            );
            Ok(())
        }
        ProposalAction::TextProposal { .. } => {
            // No-op for text proposals (signaling only).
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::MemoryStore;
    use polay_types::{AccountState, ValidatorInfo};

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn make_config() -> ChainConfig {
        let mut config = ChainConfig::default();
        config.min_proposal_deposit = 100_000;
        config.voting_period_blocks = 100;
        config.governance_quorum_bps = 3333; // 33.33%
        config.pass_threshold_bps = 5000; // 50%
        config
    }

    fn seed_account(store: &dyn StateStore, addr: Address, balance: u64) {
        StateWriter::new(store)
            .set_account(&AccountState::with_balance(addr, balance, 0))
            .unwrap();
    }

    fn seed_validator(store: &dyn StateStore, addr: Address, stake: u64) {
        let mut validator = ValidatorInfo::new(addr, 500);
        validator.stake = stake;
        StateWriter::new(store).set_validator(&validator).unwrap();
    }

    // -- Submit proposal tests -----------------------------------------------

    #[test]
    fn submit_proposal_happy_path() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        seed_account(&store, proposer, 1_000_000);

        let (proposal_id, events) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test Proposal".into(),
            "A test".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        assert!(!proposal_id.is_zero());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "governance");
        assert_eq!(events[0].action, "proposal_submitted");

        // Check proposal stored.
        let view = StateView::new(&store);
        let proposal = view.get_proposal(&proposal_id).unwrap().unwrap();
        assert_eq!(proposal.status, ProposalStatus::Voting);
        assert_eq!(proposal.deposit, 100_000);
        assert_eq!(proposal.voting_start_height, 100);
        assert_eq!(proposal.voting_end_height, 200); // 100 + 100

        // Check deposit deducted.
        let account = view.get_account(&proposer).unwrap().unwrap();
        assert_eq!(account.balance, 900_000);

        // Check proposal list.
        let list = view.get_proposal_list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], proposal_id);
    }

    #[test]
    fn submit_proposal_insufficient_deposit() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        seed_account(&store, proposer, 1_000_000);

        let result = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            50_000, // below minimum
            &store,
            &config,
            100,
        );

        assert!(matches!(
            result,
            Err(ExecutionError::InsufficientDeposit { .. })
        ));
    }

    #[test]
    fn submit_proposal_insufficient_balance() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        seed_account(&store, proposer, 50_000); // not enough for deposit

        let result = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            100_000,
            &store,
            &config,
            100,
        );

        assert!(matches!(
            result,
            Err(ExecutionError::InsufficientBalance { .. })
        ));
    }

    // -- Vote on proposal tests ----------------------------------------------

    #[test]
    fn vote_proposal_happy_path() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        seed_account(&store, proposer, 1_000_000);
        seed_validator(&store, voter, 50_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        let events =
            execute_vote_proposal(&voter, &proposal_id, VoteOption::Yes, &store, 105).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "vote_cast");

        let view = StateView::new(&store);
        let proposal = view.get_proposal(&proposal_id).unwrap().unwrap();
        assert_eq!(proposal.yes_votes, 50_000);

        let vote = view.get_vote(&proposal_id, &voter).unwrap().unwrap();
        assert_eq!(vote.option, VoteOption::Yes);
        assert_eq!(vote.weight, 50_000);
    }

    #[test]
    fn vote_proposal_wrong_timing() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        seed_account(&store, proposer, 1_000_000);
        seed_validator(&store, voter, 50_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        // Vote after the voting period ends (200 + 1).
        let result = execute_vote_proposal(&voter, &proposal_id, VoteOption::Yes, &store, 201);

        assert!(matches!(result, Err(ExecutionError::VotingPeriodEnded)));
    }

    #[test]
    fn vote_proposal_no_stake() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        seed_account(&store, proposer, 1_000_000);
        seed_account(&store, voter, 500_000); // has balance but no stake

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        let result = execute_vote_proposal(&voter, &proposal_id, VoteOption::Yes, &store, 105);

        assert!(matches!(result, Err(ExecutionError::NoStakeToVote)));
    }

    #[test]
    fn vote_proposal_double_vote_updates() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        seed_account(&store, proposer, 1_000_000);
        seed_validator(&store, voter, 50_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        // First vote: Yes
        execute_vote_proposal(&voter, &proposal_id, VoteOption::Yes, &store, 105).unwrap();

        let view = StateView::new(&store);
        let proposal = view.get_proposal(&proposal_id).unwrap().unwrap();
        assert_eq!(proposal.yes_votes, 50_000);
        assert_eq!(proposal.no_votes, 0);

        // Second vote: change to No
        execute_vote_proposal(&voter, &proposal_id, VoteOption::No, &store, 110).unwrap();

        let proposal = view.get_proposal(&proposal_id).unwrap().unwrap();
        assert_eq!(proposal.yes_votes, 0);
        assert_eq!(proposal.no_votes, 50_000);
    }

    // -- Execute proposal tests ----------------------------------------------

    #[test]
    fn execute_proposal_passed() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        seed_account(&store, proposer, 1_000_000);
        seed_validator(&store, voter, 100_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        // Vote Yes with enough stake.
        execute_vote_proposal(&voter, &proposal_id, VoteOption::Yes, &store, 105).unwrap();

        // Execute after voting period ends.
        let events =
            execute_execute_proposal(&proposer, &proposal_id, &store, &config, 201).unwrap();

        // Should have passed and executed events.
        assert!(events.iter().any(|e| e.action == "proposal_passed"));
        assert!(events.iter().any(|e| e.action == "proposal_executed"));

        let view = StateView::new(&store);
        let proposal = view.get_proposal(&proposal_id).unwrap().unwrap();
        assert_eq!(proposal.status, ProposalStatus::Executed);

        // Deposit returned to proposer.
        let account = view.get_account(&proposer).unwrap().unwrap();
        assert_eq!(account.balance, 1_000_000); // full balance restored
    }

    #[test]
    fn execute_proposal_rejected_quorum_not_met() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        seed_account(&store, proposer, 1_000_000);
        // Voter has 10K stake but total staked is 100K (need 33.33% = 33K).
        seed_validator(&store, voter, 10_000);
        // Add another validator with 90K stake who does NOT vote.
        seed_validator(&store, test_addr(3), 90_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        // Only voter with 10K votes (10% of 100K, below 33.33% quorum).
        execute_vote_proposal(&voter, &proposal_id, VoteOption::Yes, &store, 105).unwrap();

        let events =
            execute_execute_proposal(&proposer, &proposal_id, &store, &config, 201).unwrap();

        assert!(events.iter().any(|e| e.action == "proposal_rejected"));

        let view = StateView::new(&store);
        let proposal = view.get_proposal(&proposal_id).unwrap().unwrap();
        assert_eq!(proposal.status, ProposalStatus::Rejected);

        // 50% of deposit burned, 50% returned.
        let account = view.get_account(&proposer).unwrap().unwrap();
        assert_eq!(account.balance, 900_000 + 50_000); // original - deposit + half back
    }

    #[test]
    fn execute_proposal_rejected_threshold_not_met() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter_yes = test_addr(2);
        let voter_no = test_addr(3);
        seed_account(&store, proposer, 1_000_000);
        seed_validator(&store, voter_yes, 30_000);
        seed_validator(&store, voter_no, 70_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        // Quorum met (100% voted), but threshold not met (30K yes < 50% of 100K).
        execute_vote_proposal(&voter_yes, &proposal_id, VoteOption::Yes, &store, 105).unwrap();
        execute_vote_proposal(&voter_no, &proposal_id, VoteOption::No, &store, 106).unwrap();

        let events =
            execute_execute_proposal(&proposer, &proposal_id, &store, &config, 201).unwrap();

        assert!(events.iter().any(|e| e.action == "proposal_rejected"));

        let view = StateView::new(&store);
        let proposal = view.get_proposal(&proposal_id).unwrap().unwrap();
        assert_eq!(proposal.status, ProposalStatus::Rejected);
    }

    #[test]
    fn execute_proposal_voting_not_ended() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        seed_account(&store, proposer, 1_000_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Hello".into(),
                description: "World".into(),
            },
            "Test".into(),
            "Description".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        // Try to execute during voting period.
        let result = execute_execute_proposal(
            &proposer,
            &proposal_id,
            &store,
            &config,
            150, // voting ends at 200
        );

        assert!(matches!(result, Err(ExecutionError::VotingPeriodNotEnded)));
    }

    // -- Action execution tests -----------------------------------------------

    #[test]
    fn treasury_spend_execution() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        let recipient = test_addr(3);
        seed_account(&store, proposer, 1_000_000);
        seed_validator(&store, voter, 100_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TreasurySpend {
                recipient,
                amount: 50_000,
                reason: "dev fund".into(),
            },
            "Treasury".into(),
            "Fund development".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        execute_vote_proposal(&voter, &proposal_id, VoteOption::Yes, &store, 105).unwrap();

        execute_execute_proposal(&proposer, &proposal_id, &store, &config, 201).unwrap();

        // Recipient should have received 50_000.
        let view = StateView::new(&store);
        let recipient_account = view.get_account(&recipient).unwrap().unwrap();
        assert_eq!(recipient_account.balance, 50_000);
    }

    #[test]
    fn parameter_change_execution() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        seed_account(&store, proposer, 1_000_000);
        seed_validator(&store, voter, 100_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::ParameterChange {
                parameter: "block_time_ms".into(),
                new_value: "3000".into(),
            },
            "Param Change".into(),
            "Increase block time".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        execute_vote_proposal(&voter, &proposal_id, VoteOption::Yes, &store, 105).unwrap();

        let events =
            execute_execute_proposal(&proposer, &proposal_id, &store, &config, 201).unwrap();

        // Should succeed (parameter change is a no-op in MVP).
        assert!(events.iter().any(|e| e.action == "proposal_executed"));

        let view = StateView::new(&store);
        let proposal = view.get_proposal(&proposal_id).unwrap().unwrap();
        assert_eq!(proposal.status, ProposalStatus::Executed);
    }

    #[test]
    fn deposit_return_on_pass_and_burn_on_reject() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        seed_account(&store, proposer, 1_000_000);
        seed_validator(&store, voter, 100_000);

        // --- Passing proposal: full deposit returned ---
        let (id1, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Pass".into(),
                description: "Will pass".into(),
            },
            "Pass".into(),
            "Pass".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        execute_vote_proposal(&voter, &id1, VoteOption::Yes, &store, 105).unwrap();
        execute_execute_proposal(&proposer, &id1, &store, &config, 201).unwrap();

        let view = StateView::new(&store);
        let balance_after_pass = view.get_account(&proposer).unwrap().unwrap().balance;
        assert_eq!(balance_after_pass, 1_000_000); // full deposit returned

        // --- Rejecting proposal: 50% burned ---
        let (id2, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::TextProposal {
                title: "Reject".into(),
                description: "Will reject".into(),
            },
            "Reject".into(),
            "Reject".into(),
            100_000,
            &store,
            &config,
            300,
        )
        .unwrap();

        // Vote No.
        execute_vote_proposal(&voter, &id2, VoteOption::No, &store, 305).unwrap();
        execute_execute_proposal(&proposer, &id2, &store, &config, 401).unwrap();

        let balance_after_reject = view.get_account(&proposer).unwrap().unwrap().balance;
        // Started with 1M, paid 100K deposit, got 50K back = 950K.
        assert_eq!(balance_after_reject, 950_000);
    }

    #[test]
    fn slash_validator_via_governance() {
        let store = MemoryStore::new();
        let config = make_config();
        let proposer = test_addr(1);
        let voter = test_addr(2);
        let bad_validator = test_addr(3);
        seed_account(&store, proposer, 1_000_000);
        seed_validator(&store, voter, 100_000);
        seed_validator(&store, bad_validator, 200_000);

        let (proposal_id, _) = execute_submit_proposal(
            &proposer,
            ProposalAction::SlashValidator {
                address: bad_validator,
                fraction_bps: 500, // 5%
                reason: "misbehavior".into(),
            },
            "Slash".into(),
            "Slash bad validator".into(),
            100_000,
            &store,
            &config,
            100,
        )
        .unwrap();

        execute_vote_proposal(&voter, &proposal_id, VoteOption::Yes, &store, 105).unwrap();
        execute_execute_proposal(&proposer, &proposal_id, &store, &config, 201).unwrap();

        let view = StateView::new(&store);
        let slashed = view.get_validator(&bad_validator).unwrap().unwrap();
        // 200_000 * 5% = 10_000 slashed.
        assert_eq!(slashed.stake, 190_000);
    }
}
