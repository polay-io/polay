use thiserror::Error;

/// Errors that can occur during cryptographic operations.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    #[error("invalid public key: {0}")]
    InvalidPublicKey(String),

    #[error("invalid secret key: {0}")]
    InvalidSecretKey(String),

    #[error("hash error: {0}")]
    HashError(String),

    #[error("serialization error: {0}")]
    SerializationError(String),
}

/// Convenience alias for crypto results.
pub type CryptoResult<T> = Result<T, CryptoError>;

impl From<ed25519_dalek::SignatureError> for CryptoError {
    fn from(err: ed25519_dalek::SignatureError) -> Self {
        CryptoError::InvalidSignature(err.to_string())
    }
}

impl From<borsh::io::Error> for CryptoError {
    fn from(err: borsh::io::Error) -> Self {
        CryptoError::SerializationError(err.to_string())
    }
}
