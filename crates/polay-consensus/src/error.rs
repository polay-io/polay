use thiserror::Error;

/// Errors that can occur during consensus operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConsensusError {
    /// The block proposer is not the expected validator for this height/round.
    #[error("invalid proposer for this height/round")]
    InvalidProposer,

    /// The proposed block failed validation.
    #[error("invalid block: {0}")]
    InvalidBlock(String),

    /// A vote failed validation.
    #[error("invalid vote: {0}")]
    InvalidVote(String),

    /// The required quorum of votes was not reached.
    #[error("quorum not reached")]
    QuorumNotReached,

    /// A validator submitted a duplicate vote for the same height/round/step.
    #[error("duplicate vote from validator")]
    DuplicateVote,

    /// The sender is not a member of the current validator set.
    #[error("sender is not a validator")]
    NotValidator,

    /// A consensus timeout expired without sufficient progress.
    #[error("consensus timeout expired")]
    TimeoutExpired,

    /// The message targets an unexpected block height.
    #[error("wrong height: expected {expected}, got {got}")]
    WrongHeight { expected: u64, got: u64 },

    /// The message targets an unexpected consensus round.
    #[error("wrong round: expected {expected}, got {got}")]
    WrongRound { expected: u32, got: u32 },
}

/// Convenience alias for consensus results.
pub type ConsensusResult<T> = Result<T, ConsensusError>;
