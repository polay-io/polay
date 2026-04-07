//! # polay-consensus
//!
//! BFT-like consensus mechanism for the POLAY gaming blockchain.
//!
//! This crate implements a Tendermint-style consensus protocol with the
//! following phases:
//!
//! ```text
//! NewRound -> Propose -> Prevote -> Precommit -> Commit
//! ```
//!
//! ## Key components
//!
//! - [`ConsensusStateMachine`] -- Pure, deterministic state machine that drives
//!   a single consensus height through proposal, prevote, precommit, and commit
//!   phases. Returns [`ConsensusAction`] values for the runtime to execute.
//!
//! - [`ValidatorSet`] -- The active validator set with stake-weighted quorum
//!   computation and round-robin proposer selection.
//!
//! - [`BlockProposer`] -- Stateless helper to assemble a block from its parts,
//!   computing the Merkle transactions root and block hash.
//!
//! - [`EvidencePool`] -- Staging area for detected misbehavior evidence
//!   (duplicate votes, invalid proposals) before it is included in a block.
//!
//! ## Design philosophy
//!
//! The state machine performs **no I/O**. All networking, persistence, and timer
//! management is delegated to the surrounding runtime via `ConsensusAction`
//! return values.

pub mod error;
pub mod evidence;
pub mod proposer;
pub mod state_machine;
pub mod types;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use error::{ConsensusError, ConsensusResult};
pub use evidence::{Evidence, EvidencePool};
pub use proposer::BlockProposer;
pub use state_machine::ConsensusStateMachine;
pub use types::{
    CommitProof, ConsensusAction, ConsensusState, Proposal, ValidatorSet, ValidatorWeight, Vote,
    VoteType,
};
