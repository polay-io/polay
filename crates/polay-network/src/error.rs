use thiserror::Error;

/// Errors that can occur during network operations.
#[derive(Debug, Error)]
pub enum NetworkError {
    /// The outbound channel has been closed (receiver dropped).
    #[error("outbound channel closed")]
    ChannelClosed,

    /// Failed to serialize a message.
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// Failed to deserialize a message.
    #[error("deserialization error: {0}")]
    DeserializationError(String),

    /// A generic transport-level error.
    #[error("transport error: {0}")]
    TransportError(String),

    /// The peer is using an incompatible protocol version.
    #[error("incompatible protocol version (ours={ours}, theirs={theirs})")]
    IncompatibleVersion { ours: u32, theirs: u32 },

    /// The peer has been rate-limited.
    #[error("rate limited: {0}")]
    RateLimited(String),
}
