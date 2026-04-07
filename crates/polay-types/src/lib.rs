//! # polay-types
//!
//! Foundational types for the POLAY gaming blockchain. Every other crate in the
//! workspace depends on this crate for shared data structures, serialization,
//! and domain primitives.

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

pub mod account;
pub mod address;
pub mod asset;
pub mod attestation;
pub mod block;
pub mod economics;
pub mod epoch;
pub mod error;
pub mod event;
pub mod governance;
pub mod guild;
pub mod hash;
pub mod identity;
pub mod market;
pub mod rental;
pub mod session;
pub mod signature;
pub mod staking;
pub mod tournament;
pub mod transaction;

// ---------------------------------------------------------------------------
// Re-exports — convenience imports for downstream crates
// ---------------------------------------------------------------------------

// Primitives
pub use address::{Address, AddressParseError};
pub use hash::{Hash, HashParseError};
pub use signature::{Signature, SignatureParseError};

// Error handling
pub use error::{PolayError, PolayResult};

// Account
pub use account::AccountState;

// Asset
pub use asset::{AssetBalance, AssetClass, AssetType};

// Attestation
pub use attestation::{Attestor, AttestorStatus, MatchResult, MatchSettlement};

// Block
pub use block::{Block, BlockHeader};

// Epoch
pub use epoch::EpochInfo;

// Event
pub use event::Event;

// Governance
pub use governance::{Proposal, ProposalAction, ProposalStatus, Vote, VoteOption};

// Identity
pub use identity::{Achievement, PlayerProfile};

// Marketplace
pub use market::{Listing, ListingStatus};

// Session
pub use session::{SessionGrant, SessionPermission};

// Economics
pub use economics::{FeeDistribution, InflationParams, SupplyInfo};

// Staking
pub use staking::{
    Delegation, EquivocationEvidence, SlashEvent, UnbondingEntry, ValidatorInfo, ValidatorStatus,
};

// Rental
pub use rental::{Rental, RentalStatus};

// Guild
pub use guild::{Guild, GuildMembership, GuildRole};

// Tournament
pub use tournament::{Tournament, TournamentStatus};

// Transaction
pub use transaction::{
    SignedTransaction, Transaction, TransactionAction, TransactionReceipt, TxLocation,
};
