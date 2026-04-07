//! # polay-mempool
//!
//! A concurrent, fee-prioritized transaction mempool for the POLAY gaming
//! blockchain. Transactions are indexed by hash for O(1) lookup and grouped
//! by sender with nonce-ordered queues to support correct sequencing during
//! block production.
//!
//! The implementation uses [`dashmap::DashMap`] for lock-free concurrent
//! access, making it safe for use from multiple async tasks without external
//! synchronization.

pub mod error;
pub mod pool;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use error::MempoolError;
pub use pool::{Mempool, MempoolConfig};
