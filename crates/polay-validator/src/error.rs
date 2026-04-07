use thiserror::Error;

/// Errors that can occur during validator operations.
#[derive(Debug, Error)]
pub enum ValidatorError {
    /// An error originating from the state store.
    #[error("state error: {0}")]
    StateError(#[from] polay_state::StateError),

    /// An error originating from the execution engine.
    #[error("execution error: {0}")]
    ExecutionError(#[from] polay_execution::ExecutionError),

    /// An error originating from genesis processing.
    #[error("genesis error: {0}")]
    GenesisError(#[from] polay_genesis::GenesisError),

    /// An error originating from the crypto layer.
    #[error("crypto error: {0}")]
    CryptoError(#[from] polay_crypto::CryptoError),

    /// A generic operational error with a descriptive message.
    #[error("validator error: {0}")]
    Other(String),
}

/// Convenience alias for validator results.
pub type ValidatorResult<T> = Result<T, ValidatorError>;
