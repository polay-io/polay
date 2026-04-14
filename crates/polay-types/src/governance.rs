use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::hash::Hash;

// ---------------------------------------------------------------------------
// ProposalStatus
// ---------------------------------------------------------------------------

/// Lifecycle status of a governance proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ProposalStatus {
    /// The proposal is currently accepting votes.
    Voting,
    /// The proposal passed quorum and threshold checks.
    Passed,
    /// The proposal was rejected (failed quorum or threshold).
    Rejected,
    /// The proposal passed and its action has been executed on-chain.
    Executed,
    /// The proposal was cancelled by the proposer.
    Cancelled,
}

// ---------------------------------------------------------------------------
// ProposalAction
// ---------------------------------------------------------------------------

/// The on-chain action a proposal will execute if it passes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ProposalAction {
    /// Change a chain configuration parameter.
    ParameterChange {
        /// Parameter name (e.g. "block_time_ms", "max_block_transactions").
        parameter: String,
        /// Serialized new value.
        new_value: String,
    },
    /// Add a new attestor to the approved list.
    ApproveAttestor { address: Address, game_id: String },
    /// Suspend an attestor.
    SuspendAttestor { address: Address },
    /// Slash a validator with evidence.
    SlashValidator {
        address: Address,
        /// Slash fraction in basis points (1 bps = 0.01%).
        fraction_bps: u16,
        reason: String,
    },
    /// Transfer from protocol treasury to a recipient.
    TreasurySpend {
        recipient: Address,
        amount: u64,
        reason: String,
    },
    /// Text proposal (no on-chain action, just signaling).
    TextProposal { title: String, description: String },
}

// ---------------------------------------------------------------------------
// Proposal
// ---------------------------------------------------------------------------

/// A governance proposal stored on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Proposal {
    /// Content-addressed proposal identifier.
    pub id: Hash,
    /// The address that submitted this proposal.
    pub proposer: Address,
    /// The action to execute if the proposal passes.
    pub action: ProposalAction,
    /// Short title for the proposal.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Deposit (in native tokens) locked with the proposal.
    pub deposit: u64,
    /// Current lifecycle status.
    pub status: ProposalStatus,
    /// Total stake-weighted "Yes" votes.
    pub yes_votes: u64,
    /// Total stake-weighted "No" votes.
    pub no_votes: u64,
    /// Total stake-weighted "Abstain" votes.
    pub abstain_votes: u64,
    /// Block height at which voting started.
    pub voting_start_height: u64,
    /// Block height at which voting ends.
    pub voting_end_height: u64,
    /// Unix timestamp when the proposal was created.
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// VoteOption
// ---------------------------------------------------------------------------

/// A voter's choice on a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum VoteOption {
    Yes,
    No,
    Abstain,
}

impl VoteOption {
    /// Return a human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Abstain => "abstain",
        }
    }
}

// ---------------------------------------------------------------------------
// Vote
// ---------------------------------------------------------------------------

/// A single vote record stored on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Vote {
    /// The proposal being voted on.
    pub proposal_id: Hash,
    /// The voter's address.
    pub voter: Address,
    /// The chosen option.
    pub option: VoteOption,
    /// Stake-weighted voting power at time of vote.
    pub weight: u64,
    /// Block height at which the vote was cast.
    pub height: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_proposal() -> Proposal {
        Proposal {
            id: Hash::ZERO,
            proposer: Address::ZERO,
            action: ProposalAction::TextProposal {
                title: "Test".into(),
                description: "A test proposal".into(),
            },
            title: "Test Proposal".into(),
            description: "A test proposal for unit testing".into(),
            deposit: 100_000,
            status: ProposalStatus::Voting,
            yes_votes: 0,
            no_votes: 0,
            abstain_votes: 0,
            voting_start_height: 100,
            voting_end_height: 14500,
            created_at: 1700000000,
        }
    }

    #[test]
    fn serde_round_trip_proposal() {
        let p = sample_proposal();
        let json = serde_json::to_string(&p).unwrap();
        let parsed: Proposal = serde_json::from_str(&json).unwrap();
        assert_eq!(p, parsed);
    }

    #[test]
    fn borsh_round_trip_proposal() {
        let p = sample_proposal();
        let encoded = borsh::to_vec(&p).unwrap();
        let decoded = Proposal::try_from_slice(&encoded).unwrap();
        assert_eq!(p, decoded);
    }

    #[test]
    fn serde_round_trip_vote() {
        let v = Vote {
            proposal_id: Hash::ZERO,
            voter: Address::ZERO,
            option: VoteOption::Yes,
            weight: 50_000,
            height: 101,
        };
        let json = serde_json::to_string(&v).unwrap();
        let parsed: Vote = serde_json::from_str(&json).unwrap();
        assert_eq!(v, parsed);
    }

    #[test]
    fn borsh_round_trip_vote() {
        let v = Vote {
            proposal_id: Hash::ZERO,
            voter: Address::ZERO,
            option: VoteOption::No,
            weight: 10_000,
            height: 200,
        };
        let encoded = borsh::to_vec(&v).unwrap();
        let decoded = Vote::try_from_slice(&encoded).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn all_proposal_actions_serialize() {
        let actions: Vec<ProposalAction> = vec![
            ProposalAction::ParameterChange {
                parameter: "block_time_ms".into(),
                new_value: "3000".into(),
            },
            ProposalAction::ApproveAttestor {
                address: Address::ZERO,
                game_id: "chess".into(),
            },
            ProposalAction::SuspendAttestor {
                address: Address::ZERO,
            },
            ProposalAction::SlashValidator {
                address: Address::ZERO,
                fraction_bps: 500,
                reason: "double signing".into(),
            },
            ProposalAction::TreasurySpend {
                recipient: Address::ZERO,
                amount: 1_000_000,
                reason: "development fund".into(),
            },
            ProposalAction::TextProposal {
                title: "Signaling".into(),
                description: "Just a signal".into(),
            },
        ];

        for action in &actions {
            let json = serde_json::to_string(action).unwrap();
            let parsed: ProposalAction = serde_json::from_str(&json).unwrap();
            assert_eq!(action, &parsed);
        }
    }

    #[test]
    fn all_statuses_serialize() {
        let statuses = vec![
            ProposalStatus::Voting,
            ProposalStatus::Passed,
            ProposalStatus::Rejected,
            ProposalStatus::Executed,
            ProposalStatus::Cancelled,
        ];
        for s in &statuses {
            let json = serde_json::to_string(s).unwrap();
            let parsed: ProposalStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, &parsed);
        }
    }

    #[test]
    fn vote_option_labels() {
        assert_eq!(VoteOption::Yes.label(), "yes");
        assert_eq!(VoteOption::No.label(), "no");
        assert_eq!(VoteOption::Abstain.label(), "abstain");
    }
}
