use std::collections::HashMap;

use tracing::{debug, info, warn};

use polay_types::address::Address;
use polay_types::hash::Hash;
use polay_types::signature::Signature;

use crate::error::{ConsensusError, ConsensusResult};
use crate::types::{
    CommitProof, ConsensusAction, ConsensusState, Proposal, ValidatorSet, Vote, VoteType,
};

// ---------------------------------------------------------------------------
// Timeout durations (milliseconds)
// ---------------------------------------------------------------------------

/// Default timeout for the Propose step.
const PROPOSE_TIMEOUT_MS: u64 = 3000;
/// Default timeout for the Prevote step.
const PREVOTE_TIMEOUT_MS: u64 = 2000;
/// Default timeout for the Precommit step.
const PRECOMMIT_TIMEOUT_MS: u64 = 2000;

// ---------------------------------------------------------------------------
// ConsensusStateMachine
// ---------------------------------------------------------------------------

/// A pure, deterministic BFT consensus state machine.
///
/// This struct tracks the local view of a single consensus height and round.
/// It does **not** perform any I/O; instead, its methods return
/// [`ConsensusAction`] values that the surrounding runtime is responsible for
/// executing (broadcasting messages, persisting blocks, scheduling timers).
///
/// The protocol follows a simplified Tendermint-style flow:
///
/// ```text
/// NewRound -> Propose -> Prevote -> Precommit -> Commit
///                 |           |            |
///              timeout     timeout      timeout -> new round
/// ```
#[derive(Debug)]
pub struct ConsensusStateMachine {
    /// Current block height being decided.
    pub height: u64,
    /// Current round within the height (incremented on timeouts).
    pub round: u32,
    /// Current step in the consensus round.
    pub step: ConsensusState,
    /// The active set of validators and their weights.
    pub validator_set: ValidatorSet,
    /// This node's own address (used to determine proposer duty).
    pub local_address: Address,
    /// The proposal received for the current round, if any.
    pub proposal: Option<Proposal>,
    /// Prevotes collected for the current round, keyed by voter address.
    pub prevotes: HashMap<Address, Vote>,
    /// Precommits collected for the current round, keyed by voter address.
    pub precommits: HashMap<Address, Vote>,
    /// BFT locking: if a prevote quorum was observed for a block, the node
    /// locks on `(round, block_hash)` and must not prevote for a different
    /// block in later rounds (unless unlocked by a newer quorum for a
    /// different block at a higher round).
    pub locked_block: Option<(u32, Hash)>,
    /// The most recent valid block that achieved a prevote quorum, tracked
    /// across rounds within the same height. Used to re-propose when moving
    /// to a new round.
    pub valid_block: Option<(u32, Hash)>,
}

impl ConsensusStateMachine {
    /// Create a new state machine positioned at the beginning of the given
    /// `height`.
    pub fn new(height: u64, validator_set: ValidatorSet, local_address: Address) -> Self {
        debug!(
            height,
            validators = validator_set.len(),
            "initializing consensus state machine"
        );
        Self {
            height,
            round: 0,
            step: ConsensusState::NewRound,
            validator_set,
            local_address,
            proposal: None,
            prevotes: HashMap::new(),
            precommits: HashMap::new(),
            locked_block: None,
            valid_block: None,
        }
    }

    // -----------------------------------------------------------------
    // Proposer helpers
    // -----------------------------------------------------------------

    /// Returns `true` if this node is the proposer for the current
    /// `(height, round)`.
    pub fn is_proposer(&self) -> bool {
        self.current_proposer() == self.local_address
    }

    /// The address of the expected proposer for the current `(height, round)`.
    pub fn current_proposer(&self) -> Address {
        self.validator_set
            .get_proposer(self.height, self.round)
            .address
    }

    // -----------------------------------------------------------------
    // Event handlers
    // -----------------------------------------------------------------

    /// Handle an incoming block proposal.
    ///
    /// Verifies that:
    /// 1. The proposal targets the current height and round.
    /// 2. The proposer is the expected validator for this round.
    ///
    /// On success, stores the proposal, moves to the `Prevote` step, and
    /// returns a [`ConsensusAction::SendPrevote`] so the runtime can
    /// broadcast this node's prevote.
    pub fn on_proposal(&mut self, proposal: Proposal) -> ConsensusResult<ConsensusAction> {
        // Height check.
        if proposal.height != self.height {
            return Err(ConsensusError::WrongHeight {
                expected: self.height,
                got: proposal.height,
            });
        }

        // Round check.
        if proposal.round != self.round {
            return Err(ConsensusError::WrongRound {
                expected: self.round,
                got: proposal.round,
            });
        }

        // Proposer check.
        let expected_proposer = self.current_proposer();
        if proposal.proposer != expected_proposer {
            warn!(
                expected = %expected_proposer,
                got = %proposal.proposer,
                "rejecting proposal from wrong proposer"
            );
            return Err(ConsensusError::InvalidProposer);
        }

        // Verify proposer is in the validator set.
        if !self.validator_set.contains(&proposal.proposer) {
            return Err(ConsensusError::NotValidator);
        }

        let block_hash = *proposal.block.hash();

        info!(
            height = self.height,
            round = self.round,
            block_hash = %block_hash,
            "accepted proposal"
        );

        self.proposal = Some(proposal);

        // Determine what to prevote for.  If we are locked on a block from a
        // prior round, we prevote for the locked block unless the proposal
        // matches it.
        let prevote_hash = if let Some((_, locked_hash)) = &self.locked_block {
            if &block_hash == locked_hash {
                block_hash
            } else {
                // Locked on a different block: send nil prevote.
                Hash::ZERO
            }
        } else {
            block_hash
        };

        self.step = ConsensusState::Prevote;

        let prevote = Vote {
            height: self.height,
            round: self.round,
            vote_type: VoteType::Prevote,
            block_hash: prevote_hash,
            voter: self.local_address,
            signature: Signature::ZERO, // Runtime fills in real signature.
        };

        Ok(ConsensusAction::SendPrevote(prevote))
    }

    /// Handle an incoming prevote.
    ///
    /// Verifies that the vote:
    /// 1. Targets the current height and round.
    /// 2. Comes from a validator in the set.
    /// 3. Is not a duplicate.
    ///
    /// After storing the vote, if a prevote quorum is observed for any
    /// block hash, the machine transitions to the `Precommit` step and
    /// returns a [`ConsensusAction::SendPrecommit`].
    pub fn on_prevote(&mut self, vote: Vote) -> ConsensusResult<Option<ConsensusAction>> {
        self.validate_vote(&vote, VoteType::Prevote)?;

        // Duplicate check.
        if self.prevotes.contains_key(&vote.voter) {
            return Err(ConsensusError::DuplicateVote);
        }

        debug!(
            voter = %vote.voter,
            block_hash = %vote.block_hash,
            "received prevote"
        );

        self.prevotes.insert(vote.voter, vote);

        // Check if we have a quorum for any block hash (including nil).
        // We check for both a specific block quorum and nil quorum.
        let quorum_hash = self.find_prevote_quorum();

        if let Some(hash) = quorum_hash {
            // Only transition if we haven't already moved past Prevote.
            if self.step == ConsensusState::Prevote || self.step == ConsensusState::Propose {
                self.step = ConsensusState::Precommit;

                if !hash.is_zero() {
                    // Lock on this block.
                    self.locked_block = Some((self.round, hash));
                    self.valid_block = Some((self.round, hash));

                    info!(
                        height = self.height,
                        round = self.round,
                        block_hash = %hash,
                        "prevote quorum reached, locked on block"
                    );
                } else {
                    info!(
                        height = self.height,
                        round = self.round,
                        "nil prevote quorum reached"
                    );
                }

                let precommit = Vote {
                    height: self.height,
                    round: self.round,
                    vote_type: VoteType::Precommit,
                    block_hash: hash,
                    voter: self.local_address,
                    signature: Signature::ZERO,
                };

                return Ok(Some(ConsensusAction::SendPrecommit(precommit)));
            }
        }

        Ok(None)
    }

    /// Handle an incoming precommit.
    ///
    /// Same validation as prevotes. If a precommit quorum is reached for a
    /// non-nil block hash, the machine transitions to the `Commit` step and
    /// returns a [`ConsensusAction::CommitBlock`] together with a
    /// [`CommitProof`].
    pub fn on_precommit(&mut self, vote: Vote) -> ConsensusResult<Option<ConsensusAction>> {
        self.validate_vote(&vote, VoteType::Precommit)?;

        // Duplicate check.
        if self.precommits.contains_key(&vote.voter) {
            return Err(ConsensusError::DuplicateVote);
        }

        debug!(
            voter = %vote.voter,
            block_hash = %vote.block_hash,
            "received precommit"
        );

        self.precommits.insert(vote.voter, vote);

        // Check for precommit quorum on a non-nil block.
        let quorum_hash = self.find_precommit_quorum();

        if let Some(hash) = quorum_hash {
            if !hash.is_zero() && self.step != ConsensusState::Commit {
                self.step = ConsensusState::Commit;

                // Collect the precommit votes for this block into the proof.
                let proof_votes: Vec<Vote> = self
                    .precommits
                    .values()
                    .filter(|v| v.block_hash == hash)
                    .cloned()
                    .collect();

                let proof = CommitProof::new(self.height, self.round, hash, proof_votes);

                info!(
                    height = self.height,
                    round = self.round,
                    block_hash = %hash,
                    votes = proof.vote_count(),
                    "precommit quorum reached, committing block"
                );

                return Ok(Some(ConsensusAction::CommitBlock {
                    height: self.height,
                    block_hash: hash,
                    proof,
                }));
            }
        }

        Ok(None)
    }

    /// Handle a timeout event for the current step.
    ///
    /// - **Propose timeout**: send a nil prevote and advance to `Prevote`.
    /// - **Prevote timeout**: send a nil precommit and advance to `Precommit`.
    /// - **Precommit timeout**: increment the round and start a new round.
    pub fn on_timeout(&mut self) -> ConsensusAction {
        match self.step {
            ConsensusState::NewRound | ConsensusState::Propose => {
                info!(
                    height = self.height,
                    round = self.round,
                    timeout_ms = PROPOSE_TIMEOUT_MS,
                    "propose timeout, sending nil prevote"
                );
                self.step = ConsensusState::Prevote;
                ConsensusAction::SendPrevote(Vote {
                    height: self.height,
                    round: self.round,
                    vote_type: VoteType::Prevote,
                    block_hash: Hash::ZERO,
                    voter: self.local_address,
                    signature: Signature::ZERO,
                })
            }
            ConsensusState::Prevote => {
                info!(
                    height = self.height,
                    round = self.round,
                    timeout_ms = PREVOTE_TIMEOUT_MS,
                    "prevote timeout, sending nil precommit"
                );
                self.step = ConsensusState::Precommit;
                ConsensusAction::SendPrecommit(Vote {
                    height: self.height,
                    round: self.round,
                    vote_type: VoteType::Precommit,
                    block_hash: Hash::ZERO,
                    voter: self.local_address,
                    signature: Signature::ZERO,
                })
            }
            ConsensusState::Precommit => {
                self.round = self.round.saturating_add(1);
                info!(
                    height = self.height,
                    round = self.round,
                    timeout_ms = PRECOMMIT_TIMEOUT_MS,
                    "precommit timeout, advancing to new round"
                );
                self.start_new_round()
            }
            ConsensusState::Commit => {
                // Should not normally happen; return a no-op timeout schedule.
                ConsensusAction::ScheduleTimeout {
                    step: ConsensusState::Commit,
                    duration_ms: PROPOSE_TIMEOUT_MS,
                }
            }
        }
    }

    /// Reset the state machine for a new height (after a block has been
    /// committed). Clears all round-specific state and the BFT lock.
    pub fn advance_height(&mut self, new_height: u64) {
        info!(
            old_height = self.height,
            new_height, "advancing to new height"
        );
        self.height = new_height;
        self.round = 0;
        self.step = ConsensusState::NewRound;
        self.proposal = None;
        self.prevotes.clear();
        self.precommits.clear();
        self.locked_block = None;
        self.valid_block = None;
    }

    /// Replace the validator set used by the consensus engine.
    ///
    /// This is called at epoch boundaries to activate the newly elected
    /// validator set.
    pub fn update_validator_set(&mut self, new_set: ValidatorSet) {
        info!(
            old_count = self.validator_set.len(),
            new_count = new_set.len(),
            new_total_stake = new_set.total_stake,
            "updating consensus validator set"
        );
        self.validator_set = new_set;
    }

    // -----------------------------------------------------------------
    // Quorum queries
    // -----------------------------------------------------------------

    /// Returns `true` if a prevote quorum has been reached for the given
    /// `block_hash`.
    pub fn has_prevote_quorum(&self, block_hash: &Hash) -> bool {
        self.count_prevotes_for(block_hash) >= self.validator_set.quorum_threshold()
    }

    /// Returns `true` if a precommit quorum has been reached for the given
    /// `block_hash`.
    pub fn has_precommit_quorum(&self, block_hash: &Hash) -> bool {
        self.count_precommits_for(block_hash) >= self.validator_set.quorum_threshold()
    }

    /// Sum of stake behind prevotes for a specific block hash.
    pub fn count_prevotes_for(&self, block_hash: &Hash) -> u64 {
        self.prevotes
            .values()
            .filter(|v| &v.block_hash == block_hash)
            .map(|v| self.validator_set.get_stake(&v.voter))
            .sum()
    }

    /// Sum of stake behind precommits for a specific block hash.
    pub fn count_precommits_for(&self, block_hash: &Hash) -> u64 {
        self.precommits
            .values()
            .filter(|v| &v.block_hash == block_hash)
            .map(|v| self.validator_set.get_stake(&v.voter))
            .sum()
    }

    // -----------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------

    /// Start a new round: clear per-round state but preserve the lock.
    fn start_new_round(&mut self) -> ConsensusAction {
        self.step = ConsensusState::Propose;
        self.proposal = None;
        self.prevotes.clear();
        self.precommits.clear();

        ConsensusAction::ScheduleTimeout {
            step: ConsensusState::Propose,
            duration_ms: PROPOSE_TIMEOUT_MS,
        }
    }

    /// Validate common vote properties.
    fn validate_vote(&self, vote: &Vote, expected_type: VoteType) -> ConsensusResult<()> {
        if vote.height != self.height {
            return Err(ConsensusError::WrongHeight {
                expected: self.height,
                got: vote.height,
            });
        }
        if vote.round != self.round {
            return Err(ConsensusError::WrongRound {
                expected: self.round,
                got: vote.round,
            });
        }
        if vote.vote_type != expected_type {
            return Err(ConsensusError::InvalidVote(format!(
                "expected {:?}, got {:?}",
                expected_type, vote.vote_type
            )));
        }
        if !self.validator_set.contains(&vote.voter) {
            return Err(ConsensusError::NotValidator);
        }
        Ok(())
    }

    /// Scan prevotes to find a block hash (possibly nil) that has quorum.
    fn find_prevote_quorum(&self) -> Option<Hash> {
        let threshold = self.validator_set.quorum_threshold();
        let mut stake_by_hash: HashMap<Hash, u64> = HashMap::new();

        for vote in self.prevotes.values() {
            let stake = self.validator_set.get_stake(&vote.voter);
            let entry = stake_by_hash.entry(vote.block_hash).or_insert(0);
            *entry = entry.saturating_add(stake);
            if *entry >= threshold {
                return Some(vote.block_hash);
            }
        }

        None
    }

    /// Scan precommits to find a block hash (possibly nil) that has quorum.
    fn find_precommit_quorum(&self) -> Option<Hash> {
        let threshold = self.validator_set.quorum_threshold();
        let mut stake_by_hash: HashMap<Hash, u64> = HashMap::new();

        for vote in self.precommits.values() {
            let stake = self.validator_set.get_stake(&vote.voter);
            let entry = stake_by_hash.entry(vote.block_hash).or_insert(0);
            *entry = entry.saturating_add(stake);
            if *entry >= threshold {
                return Some(vote.block_hash);
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ValidatorWeight;
    use polay_types::block::{Block, BlockHeader};

    /// Build a validator set with `n` validators, each having 100 stake.
    fn make_validator_set(n: usize) -> (ValidatorSet, Vec<Address>) {
        let mut addrs = Vec::new();
        let mut weights = Vec::new();
        for i in 0..n {
            let mut bytes = [0u8; 32];
            bytes[0] = (i + 1) as u8;
            let addr = Address::new(bytes);
            addrs.push(addr);
            weights.push(ValidatorWeight::new(addr, 100));
        }
        (ValidatorSet::new(weights), addrs)
    }

    /// Build a minimal block for testing.
    fn make_block(height: u64, proposer: Address) -> Block {
        Block::new(
            BlockHeader {
                height,
                timestamp: 1_700_000_000,
                parent_hash: Hash::ZERO,
                state_root: Hash::ZERO,
                transactions_root: Hash::ZERO,
                proposer,
                chain_id: "polay-test".into(),
                hash: Hash::new([0xAA; 32]),
            },
            vec![],
        )
    }

    /// Make a prevote for a given block hash from a given voter.
    fn make_prevote(height: u64, round: u32, block_hash: Hash, voter: Address) -> Vote {
        Vote {
            height,
            round,
            vote_type: VoteType::Prevote,
            block_hash,
            voter,
            signature: Signature::ZERO,
        }
    }

    /// Make a precommit for a given block hash from a given voter.
    fn make_precommit(height: u64, round: u32, block_hash: Hash, voter: Address) -> Vote {
        Vote {
            height,
            round,
            vote_type: VoteType::Precommit,
            block_hash,
            voter,
            signature: Signature::ZERO,
        }
    }

    // -----------------------------------------------------------------
    // Proposer rotation
    // -----------------------------------------------------------------

    #[test]
    fn proposer_rotates_with_height_and_round() {
        let (vs, addrs) = make_validator_set(3);
        let sm = ConsensusStateMachine::new(0, vs, addrs[0]);

        // height=0, round=0 -> validator[0]
        assert!(sm.is_proposer());
        assert_eq!(sm.current_proposer(), addrs[0]);
    }

    #[test]
    fn proposer_is_false_for_non_proposer() {
        let (vs, addrs) = make_validator_set(3);
        // local_address is addrs[1], but at height=0, round=0 proposer is addrs[0]
        let sm = ConsensusStateMachine::new(0, vs, addrs[1]);
        assert!(!sm.is_proposer());
    }

    // -----------------------------------------------------------------
    // Proposal handling
    // -----------------------------------------------------------------

    #[test]
    fn on_proposal_valid() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Propose;

        let block = make_block(0, addrs[0]);
        let proposal = Proposal {
            height: 0,
            round: 0,
            block,
            proposer: addrs[0],
            signature: Signature::ZERO,
        };

        let action = sm.on_proposal(proposal).unwrap();
        assert!(matches!(action, ConsensusAction::SendPrevote(_)));
        assert_eq!(sm.step, ConsensusState::Prevote);
        assert!(sm.proposal.is_some());
    }

    #[test]
    fn on_proposal_wrong_proposer() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Propose;

        let block = make_block(0, addrs[1]);
        let proposal = Proposal {
            height: 0,
            round: 0,
            block,
            proposer: addrs[1], // Wrong! Should be addrs[0].
            signature: Signature::ZERO,
        };

        let err = sm.on_proposal(proposal).unwrap_err();
        assert_eq!(err, ConsensusError::InvalidProposer);
    }

    #[test]
    fn on_proposal_wrong_height() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Propose;

        let block = make_block(5, addrs[0]);
        let proposal = Proposal {
            height: 5,
            round: 0,
            block,
            proposer: addrs[0],
            signature: Signature::ZERO,
        };

        let err = sm.on_proposal(proposal).unwrap_err();
        assert!(matches!(
            err,
            ConsensusError::WrongHeight {
                expected: 0,
                got: 5
            }
        ));
    }

    #[test]
    fn on_proposal_wrong_round() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Propose;

        let block = make_block(0, addrs[0]);
        let proposal = Proposal {
            height: 0,
            round: 3,
            block,
            proposer: addrs[0],
            signature: Signature::ZERO,
        };

        let err = sm.on_proposal(proposal).unwrap_err();
        assert!(matches!(
            err,
            ConsensusError::WrongRound {
                expected: 0,
                got: 3
            }
        ));
    }

    // -----------------------------------------------------------------
    // Prevote quorum
    // -----------------------------------------------------------------

    #[test]
    fn prevote_quorum_triggers_precommit() {
        let (vs, addrs) = make_validator_set(4);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Prevote;

        let block_hash = Hash::new([0xBB; 32]);

        // 4 validators * 100 = 400 total stake, quorum = 267.
        // Need 3 prevotes (300 stake) to reach quorum.
        let v1 = make_prevote(0, 0, block_hash, addrs[0]);
        let v2 = make_prevote(0, 0, block_hash, addrs[1]);
        let v3 = make_prevote(0, 0, block_hash, addrs[2]);

        assert!(sm.on_prevote(v1).unwrap().is_none());
        assert!(sm.on_prevote(v2).unwrap().is_none());

        let action = sm.on_prevote(v3).unwrap();
        assert!(action.is_some());
        let action = action.unwrap();
        assert!(matches!(action, ConsensusAction::SendPrecommit(_)));
        assert_eq!(sm.step, ConsensusState::Precommit);

        // Lock should be set.
        assert_eq!(sm.locked_block, Some((0, block_hash)));
    }

    #[test]
    fn prevote_no_quorum_without_enough_stake() {
        let (vs, addrs) = make_validator_set(4);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Prevote;

        let block_hash = Hash::new([0xBB; 32]);

        // Only 2 prevotes (200 stake) < 267 threshold.
        let v1 = make_prevote(0, 0, block_hash, addrs[0]);
        let v2 = make_prevote(0, 0, block_hash, addrs[1]);

        assert!(sm.on_prevote(v1).unwrap().is_none());
        assert!(sm.on_prevote(v2).unwrap().is_none());
        assert_eq!(sm.step, ConsensusState::Prevote);
    }

    // -----------------------------------------------------------------
    // Precommit quorum
    // -----------------------------------------------------------------

    #[test]
    fn precommit_quorum_triggers_commit() {
        let (vs, addrs) = make_validator_set(4);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Precommit;

        let block_hash = Hash::new([0xCC; 32]);

        let v1 = make_precommit(0, 0, block_hash, addrs[0]);
        let v2 = make_precommit(0, 0, block_hash, addrs[1]);
        let v3 = make_precommit(0, 0, block_hash, addrs[2]);

        assert!(sm.on_precommit(v1).unwrap().is_none());
        assert!(sm.on_precommit(v2).unwrap().is_none());

        let action = sm.on_precommit(v3).unwrap().unwrap();
        match &action {
            ConsensusAction::CommitBlock {
                height,
                block_hash: bh,
                proof,
            } => {
                assert_eq!(*height, 0);
                assert_eq!(*bh, block_hash);
                assert_eq!(proof.vote_count(), 3);
                assert_eq!(proof.height, 0);
                assert_eq!(proof.round, 0);
            }
            other => panic!("expected CommitBlock, got {:?}", other),
        }
        assert_eq!(sm.step, ConsensusState::Commit);
    }

    #[test]
    fn nil_precommit_quorum_does_not_commit() {
        let (vs, addrs) = make_validator_set(4);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Precommit;

        // All send nil precommits.
        let v1 = make_precommit(0, 0, Hash::ZERO, addrs[0]);
        let v2 = make_precommit(0, 0, Hash::ZERO, addrs[1]);
        let v3 = make_precommit(0, 0, Hash::ZERO, addrs[2]);

        assert!(sm.on_precommit(v1).unwrap().is_none());
        assert!(sm.on_precommit(v2).unwrap().is_none());
        // Nil quorum reached, but nil blocks are not committed.
        assert!(sm.on_precommit(v3).unwrap().is_none());
    }

    // -----------------------------------------------------------------
    // Full consensus round
    // -----------------------------------------------------------------

    #[test]
    fn full_consensus_round() {
        let (vs, addrs) = make_validator_set(4);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Propose;

        let block = make_block(0, addrs[0]);
        let block_hash = *block.hash();

        // 1. Proposal
        let proposal = Proposal {
            height: 0,
            round: 0,
            block,
            proposer: addrs[0],
            signature: Signature::ZERO,
        };
        let action = sm.on_proposal(proposal).unwrap();
        assert!(matches!(action, ConsensusAction::SendPrevote(_)));

        // 2. Prevotes (need 3 of 4 for quorum)
        let pv1 = make_prevote(0, 0, block_hash, addrs[1]);
        let pv2 = make_prevote(0, 0, block_hash, addrs[2]);
        let pv3 = make_prevote(0, 0, block_hash, addrs[3]);

        assert!(sm.on_prevote(pv1).unwrap().is_none());
        assert!(sm.on_prevote(pv2).unwrap().is_none());
        let action = sm.on_prevote(pv3).unwrap().unwrap();
        assert!(matches!(action, ConsensusAction::SendPrecommit(_)));

        // 3. Precommits (need 3 of 4 for quorum)
        let pc1 = make_precommit(0, 0, block_hash, addrs[0]);
        let pc2 = make_precommit(0, 0, block_hash, addrs[1]);
        let pc3 = make_precommit(0, 0, block_hash, addrs[2]);

        assert!(sm.on_precommit(pc1).unwrap().is_none());
        assert!(sm.on_precommit(pc2).unwrap().is_none());
        let action = sm.on_precommit(pc3).unwrap().unwrap();

        match action {
            ConsensusAction::CommitBlock {
                height,
                block_hash: bh,
                proof,
            } => {
                assert_eq!(height, 0);
                assert_eq!(bh, block_hash);
                assert_eq!(proof.vote_count(), 3);
            }
            other => panic!("expected CommitBlock, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------
    // Timeout transitions
    // -----------------------------------------------------------------

    #[test]
    fn timeout_during_propose_sends_nil_prevote() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Propose;

        let action = sm.on_timeout();
        match &action {
            ConsensusAction::SendPrevote(vote) => {
                assert!(vote.is_nil());
                assert_eq!(vote.vote_type, VoteType::Prevote);
            }
            other => panic!("expected SendPrevote, got {:?}", other),
        }
        assert_eq!(sm.step, ConsensusState::Prevote);
    }

    #[test]
    fn timeout_during_prevote_sends_nil_precommit() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Prevote;

        let action = sm.on_timeout();
        match &action {
            ConsensusAction::SendPrecommit(vote) => {
                assert!(vote.is_nil());
                assert_eq!(vote.vote_type, VoteType::Precommit);
            }
            other => panic!("expected SendPrecommit, got {:?}", other),
        }
        assert_eq!(sm.step, ConsensusState::Precommit);
    }

    #[test]
    fn timeout_during_precommit_advances_round() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Precommit;

        let action = sm.on_timeout();
        assert!(matches!(
            action,
            ConsensusAction::ScheduleTimeout {
                step: ConsensusState::Propose,
                ..
            }
        ));
        assert_eq!(sm.round, 1);
        assert_eq!(sm.step, ConsensusState::Propose);
    }

    // -----------------------------------------------------------------
    // Duplicate vote detection
    // -----------------------------------------------------------------

    #[test]
    fn duplicate_prevote_rejected() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Prevote;

        let vote = make_prevote(0, 0, Hash::new([0xAA; 32]), addrs[0]);
        sm.on_prevote(vote.clone()).unwrap();

        let err = sm.on_prevote(vote).unwrap_err();
        assert_eq!(err, ConsensusError::DuplicateVote);
    }

    #[test]
    fn duplicate_precommit_rejected() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Precommit;

        let vote = make_precommit(0, 0, Hash::new([0xAA; 32]), addrs[0]);
        sm.on_precommit(vote.clone()).unwrap();

        let err = sm.on_precommit(vote).unwrap_err();
        assert_eq!(err, ConsensusError::DuplicateVote);
    }

    // -----------------------------------------------------------------
    // Non-validator rejected
    // -----------------------------------------------------------------

    #[test]
    fn vote_from_non_validator_rejected() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Prevote;

        let outsider = Address::new([0xFF; 32]);
        let vote = make_prevote(0, 0, Hash::new([0xAA; 32]), outsider);
        let err = sm.on_prevote(vote).unwrap_err();
        assert_eq!(err, ConsensusError::NotValidator);
    }

    // -----------------------------------------------------------------
    // Height advance
    // -----------------------------------------------------------------

    #[test]
    fn advance_height_resets_state() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Commit;
        sm.round = 2;
        sm.locked_block = Some((1, Hash::new([0xAA; 32])));

        sm.advance_height(1);

        assert_eq!(sm.height, 1);
        assert_eq!(sm.round, 0);
        assert_eq!(sm.step, ConsensusState::NewRound);
        assert!(sm.proposal.is_none());
        assert!(sm.prevotes.is_empty());
        assert!(sm.precommits.is_empty());
        assert!(sm.locked_block.is_none());
        assert!(sm.valid_block.is_none());
    }

    // -----------------------------------------------------------------
    // Quorum counting
    // -----------------------------------------------------------------

    #[test]
    fn count_prevotes_for_hash() {
        let (vs, addrs) = make_validator_set(4);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Prevote;

        let hash_a = Hash::new([0xAA; 32]);
        let hash_b = Hash::new([0xBB; 32]);

        sm.on_prevote(make_prevote(0, 0, hash_a, addrs[0])).unwrap();
        sm.on_prevote(make_prevote(0, 0, hash_a, addrs[1])).unwrap();
        sm.on_prevote(make_prevote(0, 0, hash_b, addrs[2])).unwrap();

        assert_eq!(sm.count_prevotes_for(&hash_a), 200);
        assert_eq!(sm.count_prevotes_for(&hash_b), 100);
        assert!(!sm.has_prevote_quorum(&hash_a));
        assert!(!sm.has_prevote_quorum(&hash_b));
    }

    #[test]
    fn count_precommits_for_hash() {
        let (vs, addrs) = make_validator_set(4);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);
        sm.step = ConsensusState::Precommit;

        let hash_a = Hash::new([0xAA; 32]);

        sm.on_precommit(make_precommit(0, 0, hash_a, addrs[0]))
            .unwrap();
        sm.on_precommit(make_precommit(0, 0, hash_a, addrs[1]))
            .unwrap();
        sm.on_precommit(make_precommit(0, 0, hash_a, addrs[2]))
            .unwrap();

        assert_eq!(sm.count_precommits_for(&hash_a), 300);
        assert!(sm.has_precommit_quorum(&hash_a));
    }

    // -----------------------------------------------------------------
    // BFT locking: prevote with locked block
    // -----------------------------------------------------------------

    #[test]
    fn locked_block_prevote_nil_for_different_proposal() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);

        let locked_hash = Hash::new([0xDD; 32]);
        sm.locked_block = Some((0, locked_hash));
        sm.step = ConsensusState::Propose;

        // Proposer sends a different block.
        let different_block_hash = Hash::new([0xEE; 32]);
        let mut block = make_block(0, addrs[0]);
        block.header.hash = different_block_hash;

        let proposal = Proposal {
            height: 0,
            round: 0,
            block,
            proposer: addrs[0],
            signature: Signature::ZERO,
        };

        let action = sm.on_proposal(proposal).unwrap();
        match action {
            ConsensusAction::SendPrevote(vote) => {
                // Should send nil because we are locked on a different block.
                assert!(vote.is_nil());
            }
            other => panic!("expected SendPrevote, got {:?}", other),
        }
    }

    #[test]
    fn locked_block_prevote_for_matching_proposal() {
        let (vs, addrs) = make_validator_set(3);
        let mut sm = ConsensusStateMachine::new(0, vs, addrs[0]);

        let locked_hash = Hash::new([0xDD; 32]);
        sm.locked_block = Some((0, locked_hash));
        sm.step = ConsensusState::Propose;

        // Proposer sends the same block we are locked on.
        let mut block = make_block(0, addrs[0]);
        block.header.hash = locked_hash;

        let proposal = Proposal {
            height: 0,
            round: 0,
            block,
            proposer: addrs[0],
            signature: Signature::ZERO,
        };

        let action = sm.on_proposal(proposal).unwrap();
        match action {
            ConsensusAction::SendPrevote(vote) => {
                assert_eq!(vote.block_hash, locked_hash);
            }
            other => panic!("expected SendPrevote, got {:?}", other),
        }
    }
}
