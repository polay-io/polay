//! Error types for the RPC crate.

use thiserror::Error;

/// Errors that can occur when setting up or running the RPC server.
#[derive(Debug, Error)]
pub enum RpcError {
    /// Failed to register an RPC method on the module.
    #[error("RPC method registration error: {0}")]
    RegistrationError(String),

    /// Failed to start the HTTP server.
    #[error("RPC server start error: {0}")]
    ServerStartError(String),
}
