use thiserror::Error;

/// Top-level error type used throughout the POLAY blockchain.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PolayError {
    #[error("invalid signature")]
    InvalidSignature,

    #[error("insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: u64, available: u64 },

    #[error("nonce mismatch: expected {expected}, got {got}")]
    NonceError { expected: u64, got: u64 },

    #[error("account not found: {0}")]
    AccountNotFound(String),

    #[error("asset not found: {0}")]
    AssetNotFound(String),

    #[error("listing not found: {0}")]
    ListingNotFound(String),

    #[error("invalid action: {0}")]
    InvalidAction(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("duplicate entry: {0}")]
    DuplicateEntry(String),

    #[error("consensus error: {0}")]
    ConsensusError(String),

    #[error("storage error: {0}")]
    StorageError(String),

    #[error("serialization error: {0}")]
    SerializationError(String),

    #[error("{0}")]
    Custom(String),
}

/// Convenience alias used across the codebase.
pub type PolayResult<T> = Result<T, PolayError>;

impl From<serde_json::Error> for PolayError {
    fn from(err: serde_json::Error) -> Self {
        PolayError::SerializationError(err.to_string())
    }
}

impl From<std::io::Error> for PolayError {
    fn from(err: std::io::Error) -> Self {
        PolayError::StorageError(err.to_string())
    }
}

impl From<hex::FromHexError> for PolayError {
    fn from(err: hex::FromHexError) -> Self {
        PolayError::SerializationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = PolayError::InsufficientBalance {
            required: 100,
            available: 50,
        };
        assert_eq!(
            err.to_string(),
            "insufficient balance: required 100, available 50"
        );
    }

    #[test]
    fn error_equality() {
        assert_eq!(PolayError::InvalidSignature, PolayError::InvalidSignature);
        assert_ne!(
            PolayError::Custom("a".into()),
            PolayError::Custom("b".into()),
        );
    }
}
