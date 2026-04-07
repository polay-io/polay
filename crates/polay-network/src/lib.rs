//! # polay-network
//!
//! P2P networking for the POLAY gaming blockchain.
//!
//! This crate provides real libp2p networking using gossipsub for message
//! propagation and mDNS for local peer discovery. It also retains a
//! channel-based `LocalNetwork` for in-process testing.
//!
//! ## Key components
//!
//! - [`P2PService`] -- Real libp2p networking service that broadcasts
//!   transactions, blocks, and consensus votes over gossipsub.
//!
//! - [`P2PConfig`] -- Configuration for the P2P layer (listen address,
//!   boot nodes, optional identity keypair).
//!
//! - [`P2PEvent`] / [`P2PCommand`] -- The event/command types flowing
//!   between the swarm background task and the node logic.
//!
//! - [`NetworkMessage`] -- The set of messages that flow between nodes.
//!
//! - [`NetworkService`] / [`NetworkHandle`] -- Channel-based interface
//!   for the `LocalNetwork` test harness.
//!
//! - [`LocalNetwork`] -- An in-process network for testing without sockets.

pub mod behaviour;
pub mod error;
pub mod message;
pub mod peer_manager;
pub mod rate_limiter;
pub mod service;
pub mod topics;

// Re-exports
pub use error::NetworkError;
pub use message::{ConsensusVoteMsg, MessageEnvelope, NetworkMessage, PROTOCOL_VERSION};
pub use peer_manager::{PeerInfo, PeerManager};
pub use rate_limiter::PeerRateLimiter;
pub use service::{
    NetworkHandle, NetworkService, P2PCommand, P2PConfig, P2PEvent, P2PService,
};
pub use topics::{TOPIC_BLOCKS, TOPIC_CONSENSUS, TOPIC_TRANSACTIONS};

#[cfg(any(test, feature = "test-utils"))]
pub use service::LocalNetwork;
