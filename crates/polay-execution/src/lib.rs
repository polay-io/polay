//! `polay-execution` — game-aware transaction execution engine for the POLAY
//! gaming blockchain.
//!
//! This crate is the heart of POLAY's state machine. It receives validated
//! transactions and applies them against the current state, producing receipts
//! and emitting events.
//!
//! # Modules
//!
//! - **transfer** — native token transfers
//! - **assets** — asset class creation, minting, transfer, burning
//! - **market** — fixed-price marketplace with escrow, royalties, and protocol fees
//! - **identity** — player profiles, achievements, reputation
//! - **staking** — validator registration, delegation, undelegation
//! - **attestation** — game-server attestor registration, match results, reward distribution

pub mod access_set;
pub mod error;
pub mod executor;
pub mod gas;
pub mod input_validation;
pub mod invariants;
pub mod modules;
pub mod parallel;
pub mod scheduler;
pub mod validator;

// Re-exports for convenience.
pub use error::ExecutionError;
pub use executor::{ExecutionResult, Executor, StateChange};
pub use gas::GasSchedule;
pub use parallel::ParallelExecutor;
pub use scheduler::{ExecutionBatch, ScheduleStats};
pub use validator::{validate_stateful, validate_stateless, validate_stateless_with_config};
