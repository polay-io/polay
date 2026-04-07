use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use polay_types::address::Address;
use polay_types::block::Block;
use polay_types::hash::Hash;
use polay_types::signature::Signature;

// ---------------------------------------------------------------------------
// ConsensusState
// ---------------------------------------------------------------------------

/// The current step within a single consensus round.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ConsensusState {
    /// Waiting to start a new round.
    NewRound,
    /// Waiting for the proposer to broadcast a block proposal.
    Propose,
    /// Collecting prevote messages from validators.
    Prevote,
    /// Collecting precommit messages from validators.
    Precommit,
    /// The round has completed and a block is being committed.
    Commit,
}

impl std::fmt::Display for ConsensusState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NewRound => write!(f, "NewRound"),
            Self::Propose => write!(f, "Propose"),
            Self::Prevote => write!(f, "Prevote"),
            Self::Precommit => write!(f, "Precommit"),
            Self::Commit => write!(f, "Commit"),
        }
    }
}

// ---------------------------------------------------------------------------
// ValidatorWeight
// ---------------------------------------------------------------------------

/// A validator identified by address together with its voting weight (stake).
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ValidatorWeight {
    /// The validator's on-chain address.
    pub address: Address,
    /// The amount of stake backing this validator, which determines its
    /// voting power in the BFT quorum.
    pub stake: u64,
}

impl ValidatorWeight {
    /// Create a new `ValidatorWeight`.
    pub fn new(address: Address, stake: u64) -> Self {
        Self { address, stake }
    }
}

// ---------------------------------------------------------------------------
// ValidatorSet
// ---------------------------------------------------------------------------

/// The current set of validators eligible to participate in consensus,
/// together with cached aggregate information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorSet {
    /// Ordered list of validators with their stake weights.
    pub validators: Vec<ValidatorWeight>,
    /// Sum of all validator stakes. Pre-computed for efficiency.
    pub total_stake: u64,
}

impl ValidatorSet {
    /// Build a new `ValidatorSet`, computing `total_stake` from the provided
    /// list of validators.
    ///
    /// An empty validator list is allowed (e.g. at genesis before any
    /// validators register). Consensus operations that require a non-empty
    /// set (like `get_proposer`) will panic if called on an empty set.
    ///
    /// # Panics
    ///
    /// Panics if the set contains duplicate addresses or if total stake
    /// overflows `u64`.
    pub fn new(validators: Vec<ValidatorWeight>) -> Self {
        if !validators.is_empty() {
            // Verify no duplicate addresses.
            let mut seen = std::collections::HashSet::new();
            for v in &validators {
                assert!(
                    seen.insert(v.address),
                    "duplicate validator address: {}",
                    v.address,
                );
            }
        }

        // Use checked arithmetic for total stake.
        let total_stake = validators
            .iter()
            .try_fold(0u64, |acc, v| acc.checked_add(v.stake))
            .expect("total validator stake overflowed u64");

        Self {
            validators,
            total_stake,
        }
    }

    /// Determine the proposer for a given `(height, round)` pair using
    /// deterministic round-robin rotation.
    ///
    /// # Panics
    ///
    /// Panics if the validator set is empty.
    pub fn get_proposer(&self, height: u64, round: u32) -> &ValidatorWeight {
        assert!(!self.validators.is_empty(), "validator set must not be empty");
        let index = ((height as u128 + round as u128) % self.validators.len() as u128) as usize;
        &self.validators[index]
    }

    /// Returns `true` if the given address is part of this validator set.
    pub fn contains(&self, addr: &Address) -> bool {
        self.validators.iter().any(|v| &v.address == addr)
    }

    /// Look up the stake associated with the given address.
    /// Returns `0` if the address is not in the set.
    pub fn get_stake(&self, addr: &Address) -> u64 {
        self.validators
            .iter()
            .find(|v| &v.address == addr)
            .map(|v| v.stake)
            .unwrap_or(0)
    }

    /// The minimum total stake required to form a quorum: strictly more than
    /// two-thirds of the total stake.
    ///
    /// Computed as `(total_stake * 2) / 3 + 1`, which is the smallest integer
    /// strictly greater than `total_stake * 2 / 3`. Uses u128 intermediate
    /// to prevent overflow for large total_stake values.
    pub fn quorum_threshold(&self) -> u64 {
        ((self.total_stake as u128 * 2) / 3 + 1) as u64
    }

    /// Number of validators in the set.
    pub fn len(&self) -> usize {
        self.validators.len()
    }

    /// Returns `true` if the validator set is empty.
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }
}

// ---------------------------------------------------------------------------
// VoteType
// ---------------------------------------------------------------------------

/// Distinguishes prevote messages from precommit messages.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum VoteType {
    /// First voting phase: validators indicate whether they received a valid
    /// proposal.
    Prevote,
    /// Second voting phase: validators commit to a specific block hash once a
    /// prevote quorum is observed.
    Precommit,
}

impl std::fmt::Display for VoteType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prevote => write!(f, "Prevote"),
            Self::Precommit => write!(f, "Precommit"),
        }
    }
}

// ---------------------------------------------------------------------------
// Vote
// ---------------------------------------------------------------------------

/// A signed vote cast by a validator during a consensus round.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct Vote {
    /// Block height this vote applies to.
    pub height: u64,
    /// Consensus round within the height.
    pub round: u32,
    /// Whether this is a prevote or precommit.
    pub vote_type: VoteType,
    /// Hash of the block being voted on. `Hash::ZERO` represents a nil vote
    /// (the validator did not see a valid proposal).
    pub block_hash: Hash,
    /// Address of the validator casting this vote.
    pub voter: Address,
    /// Cryptographic signature over the vote payload.
    pub signature: Signature,
}

impl Vote {
    /// Returns `true` if this is a nil vote (block_hash is zero).
    pub fn is_nil(&self) -> bool {
        self.block_hash.is_zero()
    }
}

// ---------------------------------------------------------------------------
// Proposal
// ---------------------------------------------------------------------------

/// A block proposal broadcast by the designated proposer for a round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    /// Block height being proposed.
    pub height: u64,
    /// Consensus round within the height.
    pub round: u32,
    /// The proposed block (header + transactions).
    pub block: Block,
    /// Address of the proposing validator.
    pub proposer: Address,
    /// Cryptographic signature over the proposal payload.
    pub signature: Signature,
}

// ---------------------------------------------------------------------------
// CommitProof
// ---------------------------------------------------------------------------

/// Evidence that a block was committed by a quorum of validators.
///
/// Contains the collected precommit votes that together exceed the
/// two-thirds-plus-one stake threshold required by the BFT protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitProof {
    /// The block height that was committed.
    pub height: u64,
    /// The round in which consensus was reached.
    pub round: u32,
    /// Hash of the committed block.
    pub block_hash: Hash,
    /// The precommit votes that form the quorum proof.
    pub votes: Vec<Vote>,
}

impl CommitProof {
    /// Create a new `CommitProof`.
    pub fn new(height: u64, round: u32, block_hash: Hash, votes: Vec<Vote>) -> Self {
        Self {
            height,
            round,
            block_hash,
            votes,
        }
    }

    /// Number of votes in the proof.
    pub fn vote_count(&self) -> usize {
        self.votes.len()
    }
}

// ---------------------------------------------------------------------------
// ConsensusAction
// ---------------------------------------------------------------------------

/// An action that the consensus state machine instructs the caller to perform.
///
/// The state machine is pure logic; it never performs I/O itself. Instead it
/// returns `ConsensusAction` values that the surrounding runtime must execute
/// (e.g., broadcast a vote, persist a committed block, schedule a timer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsensusAction {
    /// Broadcast a block proposal to all validators.
    SendProposal(Proposal),
    /// Broadcast a prevote to all validators.
    SendPrevote(Vote),
    /// Broadcast a precommit to all validators.
    SendPrecommit(Vote),
    /// A block has achieved a precommit quorum and should be committed to
    /// storage.
    CommitBlock {
        height: u64,
        block_hash: Hash,
        proof: CommitProof,
    },
    /// Schedule a timeout that will fire after `duration_ms` milliseconds.
    /// When the timer fires, the runtime should call
    /// `ConsensusStateMachine::on_timeout`.
    ScheduleTimeout {
        step: ConsensusState,
        duration_ms: u64,
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_validators(n: usize) -> Vec<ValidatorWeight> {
        (0..n)
            .map(|i| {
                let mut bytes = [0u8; 32];
                bytes[0] = (i + 1) as u8;
                ValidatorWeight::new(Address::new(bytes), 100)
            })
            .collect()
    }

    #[test]
    fn validator_set_total_stake() {
        let vs = ValidatorSet::new(make_validators(4));
        assert_eq!(vs.total_stake, 400);
        assert_eq!(vs.len(), 4);
    }

    #[test]
    fn validator_set_quorum_threshold() {
        // 4 validators * 100 stake = 400 total
        // quorum = (400 * 2) / 3 + 1 = 267
        let vs = ValidatorSet::new(make_validators(4));
        assert_eq!(vs.quorum_threshold(), 267);
    }

    #[test]
    fn validator_set_contains() {
        let vs = ValidatorSet::new(make_validators(3));
        let mut in_set = [0u8; 32];
        in_set[0] = 1;
        assert!(vs.contains(&Address::new(in_set)));
        assert!(!vs.contains(&Address::ZERO));
    }

    #[test]
    fn validator_set_get_stake() {
        let vs = ValidatorSet::new(make_validators(3));
        let mut addr = [0u8; 32];
        addr[0] = 2;
        assert_eq!(vs.get_stake(&Address::new(addr)), 100);
        assert_eq!(vs.get_stake(&Address::ZERO), 0);
    }

    #[test]
    fn proposer_rotation() {
        let vs = ValidatorSet::new(make_validators(3));
        // height=0, round=0 -> index 0
        assert_eq!(vs.get_proposer(0, 0).address, vs.validators[0].address);
        // height=0, round=1 -> index 1
        assert_eq!(vs.get_proposer(0, 1).address, vs.validators[1].address);
        // height=1, round=0 -> index 1
        assert_eq!(vs.get_proposer(1, 0).address, vs.validators[1].address);
        // height=2, round=1 -> index 0 (3 % 3 = 0)
        assert_eq!(vs.get_proposer(2, 1).address, vs.validators[0].address);
    }

    #[test]
    fn vote_nil_detection() {
        let vote = Vote {
            height: 1,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Hash::ZERO,
            voter: Address::ZERO,
            signature: Signature::ZERO,
        };
        assert!(vote.is_nil());

        let vote2 = Vote {
            block_hash: Hash::new([0xAA; 32]),
            ..vote
        };
        assert!(!vote2.is_nil());
    }

    #[test]
    fn commit_proof_creation() {
        let proof = CommitProof::new(10, 0, Hash::new([0xBB; 32]), vec![]);
        assert_eq!(proof.height, 10);
        assert_eq!(proof.round, 0);
        assert_eq!(proof.vote_count(), 0);
    }

    #[test]
    fn consensus_state_display() {
        assert_eq!(ConsensusState::NewRound.to_string(), "NewRound");
        assert_eq!(ConsensusState::Propose.to_string(), "Propose");
        assert_eq!(ConsensusState::Prevote.to_string(), "Prevote");
        assert_eq!(ConsensusState::Precommit.to_string(), "Precommit");
        assert_eq!(ConsensusState::Commit.to_string(), "Commit");
    }

    #[test]
    fn vote_type_display() {
        assert_eq!(VoteType::Prevote.to_string(), "Prevote");
        assert_eq!(VoteType::Precommit.to_string(), "Precommit");
    }
}
