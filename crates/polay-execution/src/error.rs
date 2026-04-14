use thiserror::Error;

/// Errors that can arise during transaction validation or execution.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ExecutionError {
    #[error("insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: u64, available: u64 },

    #[error("invalid nonce: expected {expected}, got {got}")]
    InvalidNonce { expected: u64, got: u64 },

    #[error("account not found: {0}")]
    AccountNotFound(String),

    #[error("asset class not found")]
    AssetClassNotFound,

    #[error("asset class already exists")]
    AssetClassAlreadyExists,

    #[error("max supply exceeded: max {max}, current {current}, requested {requested}")]
    MaxSupplyExceeded {
        max: u64,
        current: u64,
        requested: u64,
    },

    #[error("listing not found")]
    ListingNotFound,

    #[error("listing is not active")]
    ListingNotActive,

    #[error("listing owner mismatch")]
    ListingOwnerMismatch,

    #[error("cannot buy own listing")]
    CannotBuyOwnListing,

    #[error("profile already exists")]
    ProfileAlreadyExists,

    #[error("profile not found")]
    ProfileNotFound,

    #[error("validator already registered")]
    ValidatorAlreadyRegistered,

    #[error("validator not found")]
    ValidatorNotFound,

    #[error("invalid commission")]
    InvalidCommission,

    #[error("insufficient stake")]
    InsufficientStake,

    #[error("attestor already registered")]
    AttestorAlreadyRegistered,

    #[error("attestor not found")]
    AttestorNotFound,

    #[error("attestor not active")]
    AttestorNotActive,

    #[error("invalid match result: {0}")]
    InvalidMatchResult(String),

    #[error("match already settled")]
    MatchAlreadySettled,

    #[error("unauthorized")]
    Unauthorized,

    #[error("fee too low")]
    FeeTooLow,

    #[error("invalid chain id: expected {expected}, got {got}")]
    InvalidChainId { expected: String, got: String },

    #[error("invalid signer: zero address")]
    ZeroAddressSigner,

    #[error("invalid tx hash")]
    InvalidTxHash,

    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    #[error("invalid signer pubkey: {0}")]
    InvalidSignerPubkey(String),

    #[error("match quarantined")]
    MatchQuarantined,

    #[error("match settlement not found")]
    MatchSettlementNotFound,

    #[error("match result not found")]
    MatchResultNotFound,

    #[error("reward pool exceeded: pool {pool}, total_distributed {total_distributed}")]
    RewardPoolExceeded { pool: u64, total_distributed: u64 },

    #[error("invalid transaction: {0}")]
    InvalidTransaction(String),

    #[error("state error: {0}")]
    StateError(String),

    #[error("insufficient deposit: required {required}, provided {provided}")]
    InsufficientDeposit { required: u64, provided: u64 },

    #[error("proposal not found")]
    ProposalNotFound,

    #[error("proposal is not in active voting state")]
    ProposalNotActive,

    #[error("voting period has ended")]
    VotingPeriodEnded,

    #[error("voting period has not ended yet")]
    VotingPeriodNotEnded,

    #[error("no stake to vote")]
    NoStakeToVote,

    #[error("transaction too large: max {max} bytes, actual {actual} bytes")]
    TransactionTooLarge { max: usize, actual: usize },

    #[error("transaction expired: max age {max_age_secs}s, tx age {tx_age_secs}s")]
    TransactionExpired { max_age_secs: u64, tx_age_secs: u64 },

    #[error("invalid input: {0}")]
    InvalidInput(String),

    // -- Session key errors ---------------------------------------------------
    #[error("session not found")]
    SessionNotFound,

    #[error("session expired")]
    SessionExpired,

    #[error("session revoked")]
    SessionRevoked,

    #[error("session action not permitted")]
    SessionActionNotPermitted,

    #[error("session spending limit exceeded")]
    SessionSpendingLimitExceeded,

    #[error("session already exists")]
    SessionAlreadyExists,

    #[error("invalid session pubkey: {0}")]
    InvalidSessionPubkey(String),

    // -- Gas sponsorship errors -------------------------------------------------
    #[error("sponsor cannot be the signer")]
    SponsorIsSigner,

    #[error("sponsor address is the zero address")]
    ZeroAddressSponsor,

    #[error("sponsor account not found: {0}")]
    SponsorAccountNotFound(String),

    // -- Rental errors ---------------------------------------------------------
    #[error("rental not found")]
    RentalNotFound,

    #[error("rental is not in Listed status")]
    RentalNotListed,

    #[error("rental is not in Active status")]
    RentalNotActive,

    #[error("rental owner mismatch")]
    RentalOwnerMismatch,

    #[error("rental renter mismatch")]
    RentalRenterMismatch,

    #[error("rental not expired")]
    RentalNotExpired,

    #[error("invalid rental duration: {reason}")]
    InvalidRentalDuration { reason: String },

    // -- Guild errors ----------------------------------------------------------
    #[error("guild not found")]
    GuildNotFound,

    #[error("already a guild member")]
    AlreadyGuildMember,

    #[error("not a guild member")]
    NotGuildMember,

    #[error("guild is full: max {max} members")]
    GuildFull { max: u32 },

    #[error("leader cannot leave the guild")]
    LeaderCannotLeave,

    #[error("insufficient treasury balance: required {required}, available {available}")]
    InsufficientTreasuryBalance { required: u64, available: u64 },

    #[error("not authorized for this guild action")]
    NotAuthorized,

    #[error("invalid guild role: {0}")]
    InvalidGuildRole(String),

    #[error("cannot kick the guild leader")]
    CannotKickLeader,

    // -- Tournament errors ----------------------------------------------------
    #[error("tournament not found")]
    TournamentNotFound,

    #[error("tournament is not in registration state")]
    TournamentNotInRegistration,

    #[error("tournament is not active")]
    TournamentNotActive,

    #[error("tournament is not completed")]
    TournamentNotCompleted,

    #[error("already registered for tournament")]
    AlreadyRegistered,

    #[error("tournament is full")]
    TournamentFull,

    #[error("registration is closed")]
    RegistrationClosed,

    #[error("not enough participants to start")]
    NotEnoughParticipants,

    #[error("not the tournament organizer")]
    NotOrganizer,

    #[error("signer is not ranked in this tournament")]
    NotRanked,

    #[error("prize already claimed")]
    PrizeAlreadyClaimed,

    #[error("invalid rankings")]
    InvalidRankings,

    #[error("cannot cancel an active tournament")]
    CannotCancelActiveTournament,

    #[error("{0}")]
    Custom(String),
}

/// Convert state errors into execution errors so we can use `?` in handlers.
impl From<polay_state::StateError> for ExecutionError {
    fn from(err: polay_state::StateError) -> Self {
        ExecutionError::StateError(err.to_string())
    }
}
