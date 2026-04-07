//! # polay-rpc
//!
//! JSON-RPC server for the POLAY gaming blockchain. Provides HTTP endpoints
//! that game clients, wallets, and explorers use to query on-chain state and
//! submit transactions.
//!
//! The server is built on [`jsonrpsee`] with CORS enabled so that browser-based
//! game frontends can interact with the chain directly.

pub mod error;
pub mod event_bus;
pub mod rate_limiter;
pub mod server;
pub mod types;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use error::RpcError;
pub use event_bus::{ChainEvent, EventBus};
pub use rate_limiter::SubmissionThrottle;
pub use server::{build_rpc_module, start_rpc_server, RpcServer};
pub use types::{
    AccountResponse, AssetBalanceResponse, AssetClassResponse, BlockResponse, ChainInfoResponse,
    EventResponse, GasEstimateResponse, HealthResponse, ListingResponse, MatchResultResponse,
    NetworkStatsResponse, NodeInfoResponse, ProfileResponse, ProposalResponse, ReceiptResponse,
    SubmitTransactionRequest, SubmitTransactionResponse, TransactionWithStatus,
    UnbondingEntryResponse, ValidatorResponse,
};
