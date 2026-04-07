use serde::{Deserialize, Serialize};

use crate::types::{Proposal, Vote};

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

/// Evidence of validator misbehavior that can be submitted on-chain to trigger
/// slashing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    /// A validator cast two votes of the same type for the same height and
    /// round but with different block hashes. This is a clear protocol
    /// violation since a validator must vote at most once per step.
    DuplicateVote {
        /// The first vote observed.
        vote1: Vote,
        /// The conflicting second vote with a different `block_hash`.
        vote2: Vote,
    },

    /// A validator broadcast a proposal that violates the protocol rules.
    InvalidProposal {
        /// The offending proposal.
        proposal: Proposal,
        /// Human-readable explanation of why the proposal is invalid.
        reason: String,
    },
}

impl Evidence {
    /// Create `DuplicateVote` evidence after verifying the basic invariant
    /// that the two votes share the same `(height, round, vote_type, voter)`
    /// but differ in `block_hash`.
    ///
    /// Returns `None` if the votes do not actually conflict.
    pub fn new_duplicate_vote(vote1: Vote, vote2: Vote) -> Option<Self> {
        if vote1.height != vote2.height
            || vote1.round != vote2.round
            || vote1.vote_type != vote2.vote_type
            || vote1.voter != vote2.voter
        {
            // Not the same context, so not a duplicate.
            return None;
        }
        if vote1.block_hash == vote2.block_hash {
            // Same hash means it is the same vote, not a conflict.
            return None;
        }
        Some(Evidence::DuplicateVote { vote1, vote2 })
    }

    /// Returns `true` if this evidence is a `DuplicateVote`.
    pub fn is_duplicate_vote(&self) -> bool {
        matches!(self, Evidence::DuplicateVote { .. })
    }

    /// Returns `true` if this evidence is an `InvalidProposal`.
    pub fn is_invalid_proposal(&self) -> bool {
        matches!(self, Evidence::InvalidProposal { .. })
    }
}

// ---------------------------------------------------------------------------
// EvidencePool
// ---------------------------------------------------------------------------

/// A staging area for evidence of misbehavior that has been detected locally
/// but not yet included in a block.
///
/// During block proposal, the proposer drains the pool and includes the
/// evidence in the proposed block so that validators can verify and apply
/// slashing.
#[derive(Debug, Default)]
pub struct EvidencePool {
    /// Accumulated evidence items.
    evidence: Vec<Evidence>,
}

impl EvidencePool {
    /// Create a new, empty evidence pool.
    pub fn new() -> Self {
        Self {
            evidence: Vec::new(),
        }
    }

    /// Add a piece of evidence to the pool.
    pub fn add(&mut self, ev: Evidence) {
        self.evidence.push(ev);
    }

    /// Remove and return all evidence from the pool.
    ///
    /// After this call, the pool is empty. The caller (typically the block
    /// proposer) is responsible for including the returned evidence in a
    /// block.
    pub fn drain(&mut self) -> Vec<Evidence> {
        std::mem::take(&mut self.evidence)
    }

    /// The number of evidence items currently in the pool.
    pub fn len(&self) -> usize {
        self.evidence.len()
    }

    /// Returns `true` if the pool contains no evidence.
    pub fn is_empty(&self) -> bool {
        self.evidence.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VoteType;
    use polay_types::address::Address;
    use polay_types::block::{Block, BlockHeader};
    use polay_types::hash::Hash;
    use polay_types::signature::Signature;

    fn make_vote(block_hash: Hash) -> Vote {
        Vote {
            height: 10,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash,
            voter: Address::new([0x01; 32]),
            signature: Signature::ZERO,
        }
    }

    #[test]
    fn duplicate_vote_evidence_creation() {
        let v1 = make_vote(Hash::new([0xAA; 32]));
        let v2 = make_vote(Hash::new([0xBB; 32]));

        let ev = Evidence::new_duplicate_vote(v1, v2);
        assert!(ev.is_some());
        let ev = ev.unwrap();
        assert!(ev.is_duplicate_vote());
        assert!(!ev.is_invalid_proposal());
    }

    #[test]
    fn same_hash_not_duplicate() {
        let v1 = make_vote(Hash::new([0xAA; 32]));
        let v2 = make_vote(Hash::new([0xAA; 32]));

        assert!(Evidence::new_duplicate_vote(v1, v2).is_none());
    }

    #[test]
    fn different_voter_not_duplicate() {
        let v1 = Vote {
            height: 10,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Hash::new([0xAA; 32]),
            voter: Address::new([0x01; 32]),
            signature: Signature::ZERO,
        };
        let v2 = Vote {
            height: 10,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Hash::new([0xBB; 32]),
            voter: Address::new([0x02; 32]), // Different voter.
            signature: Signature::ZERO,
        };

        assert!(Evidence::new_duplicate_vote(v1, v2).is_none());
    }

    #[test]
    fn different_round_not_duplicate() {
        let v1 = Vote {
            height: 10,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Hash::new([0xAA; 32]),
            voter: Address::new([0x01; 32]),
            signature: Signature::ZERO,
        };
        let v2 = Vote {
            height: 10,
            round: 1, // Different round.
            vote_type: VoteType::Prevote,
            block_hash: Hash::new([0xBB; 32]),
            voter: Address::new([0x01; 32]),
            signature: Signature::ZERO,
        };

        assert!(Evidence::new_duplicate_vote(v1, v2).is_none());
    }

    #[test]
    fn invalid_proposal_evidence() {
        let block = Block::new(
            BlockHeader {
                height: 5,
                timestamp: 1_700_000_000,
                parent_hash: Hash::ZERO,
                state_root: Hash::ZERO,
                transactions_root: Hash::ZERO,
                proposer: Address::ZERO,
                chain_id: "test".into(),
                hash: Hash::ZERO,
            },
            vec![],
        );

        let proposal = Proposal {
            height: 5,
            round: 0,
            block,
            proposer: Address::ZERO,
            signature: Signature::ZERO,
        };

        let ev = Evidence::InvalidProposal {
            proposal,
            reason: "proposer not in validator set".into(),
        };

        assert!(ev.is_invalid_proposal());
        assert!(!ev.is_duplicate_vote());
    }

    #[test]
    fn evidence_pool_add_and_drain() {
        let mut pool = EvidencePool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);

        let v1 = make_vote(Hash::new([0xAA; 32]));
        let v2 = make_vote(Hash::new([0xBB; 32]));
        let ev = Evidence::new_duplicate_vote(v1, v2).unwrap();

        pool.add(ev);
        assert_eq!(pool.len(), 1);
        assert!(!pool.is_empty());

        let drained = pool.drain();
        assert_eq!(drained.len(), 1);
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn evidence_pool_drain_is_complete() {
        let mut pool = EvidencePool::new();

        // Add three pieces of evidence.
        for i in 0..3u8 {
            let v1 = make_vote(Hash::new([i; 32]));
            let v2 = make_vote(Hash::new([i + 100; 32]));
            pool.add(Evidence::new_duplicate_vote(v1, v2).unwrap());
        }

        assert_eq!(pool.len(), 3);

        let drained = pool.drain();
        assert_eq!(drained.len(), 3);
        assert_eq!(pool.len(), 0);

        // Second drain returns nothing.
        let empty = pool.drain();
        assert!(empty.is_empty());
    }
}
