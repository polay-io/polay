use thiserror::Error;

/// Errors that can occur during state operations.
#[derive(Debug, Error)]
pub enum StateError {
    /// An error originating from the underlying storage backend.
    #[error("storage error: {0}")]
    StorageError(String),

    /// An error during Borsh serialization or deserialization.
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// A required key was not found in state.
    #[error("key not found: {0}")]
    KeyNotFound(String),
}

/// Convenience alias for state operations.
pub type StateResult<T> = Result<T, StateError>;

// ---------------------------------------------------------------------------
// From impls for common error sources
// ---------------------------------------------------------------------------

impl From<std::io::Error> for StateError {
    fn from(err: std::io::Error) -> Self {
        StateError::SerializationError(err.to_string())
    }
}
