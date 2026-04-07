//! # polay-validator
//!
//! Validator orchestrator for the POLAY gaming blockchain.
//!
//! This crate ties the consensus, execution, mempool, and state layers
//! together into a single validator loop that produces blocks on a
//! configurable cadence.
//!
//! ## Key components
//!
//! - [`BlockProducer`] -- Pulls transactions from the mempool, executes
//!   them, and assembles a signed block via the consensus `BlockProposer`.
//!
//! - [`ChainState`] -- Manages chain metadata (height, latest hash) and
//!   applies committed blocks to the state store.
//!
//! - [`ValidatorNode`] -- The top-level orchestrator that initialises state
//!   from genesis and runs a block-production loop.

pub mod block_producer;
pub mod block_validator;
pub mod chain;
pub mod epoch;
pub mod error;
pub mod validator_loop;

// Re-exports
pub use block_producer::BlockProducer;
pub use block_validator::{BlockValidationError, BlockValidator};
pub use chain::ChainState;
pub use epoch::EpochManager;
pub use error::ValidatorError;
pub use validator_loop::ValidatorNode;
