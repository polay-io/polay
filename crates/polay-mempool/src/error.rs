use thiserror::Error;

/// Errors that can occur when interacting with the transaction mempool.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MempoolError {
    /// The transaction already exists in the mempool.
    #[error("transaction already exists in the mempool")]
    TransactionAlreadyExists,

    /// The mempool has reached its maximum capacity.
    #[error("mempool is full (capacity reached)")]
    MempoolFull,

    /// The transaction failed validation.
    #[error("invalid transaction: {0}")]
    InvalidTransaction(String),

    /// The transaction nonce is below the expected minimum for the sender.
    #[error("nonce too low: expected >= {expected}, got {got}")]
    NonceTooLow {
        /// Minimum acceptable nonce.
        expected: u64,
        /// Nonce provided in the transaction.
        got: u64,
    },

    /// The transaction fee is below the mempool's minimum threshold.
    #[error("fee too low: minimum {minimum}, got {got}")]
    FeeTooLow {
        /// Minimum acceptable fee.
        minimum: u64,
        /// Fee provided in the transaction.
        got: u64,
    },

    /// The transaction signature is invalid.
    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    /// The nonce gap between the submitted transaction and the sender's
    /// last known nonce is too large.
    #[error("nonce gap too large: max gap {max_gap}, actual gap {gap}")]
    NonceGapTooLarge {
        /// Maximum allowed gap.
        max_gap: u64,
        /// Actual gap encountered.
        gap: u64,
    },

    /// The transaction's chain_id does not match the expected chain.
    #[error("chain id mismatch: expected {expected}, got {got}")]
    ChainIdMismatch { expected: String, got: String },

    /// The transaction has already been seen (duplicate).
    #[error("duplicate transaction")]
    DuplicateTransaction,
}
