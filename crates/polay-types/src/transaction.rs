use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::asset::AssetType;
use crate::attestation::MatchResult;
use crate::event::Event;
use crate::governance::{ProposalAction, VoteOption};
use crate::hash::Hash;
use crate::session::SessionPermission;
use crate::signature::Signature;

// ---------------------------------------------------------------------------
// TransactionAction -- every possible on-chain operation
// ---------------------------------------------------------------------------

/// Enumerates every transaction type supported by the POLAY blockchain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum TransactionAction {
    // -- Token transfers ---------------------------------------------------
    /// Transfer native tokens from the signer to `to`.
    Transfer { to: Address, amount: u64 },

    // -- Asset management --------------------------------------------------
    /// Create a new asset class (fungible, NFT, or semi-fungible).
    CreateAssetClass {
        name: String,
        symbol: String,
        asset_type: AssetType,
        max_supply: Option<u64>,
        metadata_uri: String,
    },

    /// Mint new units of an existing asset class.
    MintAsset {
        asset_class_id: Hash,
        to: Address,
        amount: u64,
        metadata: Option<String>,
    },

    /// Transfer asset units from the signer to `to`.
    TransferAsset {
        asset_class_id: Hash,
        to: Address,
        amount: u64,
    },

    /// Burn (destroy) asset units owned by the signer.
    BurnAsset { asset_class_id: Hash, amount: u64 },

    // -- Marketplace -------------------------------------------------------
    /// List assets for sale at a fixed price.
    CreateListing {
        asset_class_id: Hash,
        amount: u64,
        price_per_unit: u64,
        currency: Hash,
    },

    /// Cancel an active listing. Only the seller may do this.
    CancelListing { listing_id: Hash },

    /// Purchase an active listing. The full price is transferred from the
    /// buyer to the seller minus royalties.
    BuyListing { listing_id: Hash },

    // -- Identity / social -------------------------------------------------
    /// Create an on-chain player profile.
    CreateProfile {
        username: String,
        display_name: String,
        metadata: Option<String>,
    },

    /// Award a soulbound achievement to a player.
    AddAchievement {
        player: Address,
        achievement_id: String,
        name: String,
        metadata: String,
    },

    /// Adjust a player's reputation score.
    UpdateReputation {
        player: Address,
        delta: i64,
        reason: String,
    },

    // -- Staking / consensus -----------------------------------------------
    /// Register the signer as a validator.
    RegisterValidator { commission_bps: u16 },

    /// Delegate native tokens to a validator.
    DelegateStake { validator: Address, amount: u64 },

    /// Begin undelegating tokens from a validator (starts cooldown).
    UndelegateStake { validator: Address, amount: u64 },

    // -- Game attestation --------------------------------------------------
    /// Register the signer as an attestor for a specific game.
    RegisterAttestor {
        game_id: String,
        endpoint: String,
        metadata: String,
    },

    /// Submit a verified match result (only authorized attestors).
    SubmitMatchResult { match_result: MatchResult },

    /// Distribute rewards from a settled match.
    DistributeReward {
        match_id: Hash,
        rewards: Vec<(Address, u64)>,
    },

    // -- Governance -----------------------------------------------------------
    /// Submit a new governance proposal.
    SubmitProposal {
        action: ProposalAction,
        title: String,
        description: String,
        deposit: u64,
    },

    /// Vote on an active governance proposal.
    VoteProposal {
        proposal_id: Hash,
        option: VoteOption,
    },

    /// Execute a passed governance proposal after voting ends.
    ExecuteProposal { proposal_id: Hash },

    // -- Session keys ---------------------------------------------------------
    /// Create a session key -- must be signed by the account owner.
    CreateSession {
        /// The Ed25519 public key of the session (32 bytes).
        session_pubkey: Vec<u8>,
        /// What actions the session is allowed to perform.
        permissions: SessionPermission,
        /// Block height when the session expires.
        expires_at: u64,
        /// Maximum POL the session can spend (cumulative).
        spending_limit: u64,
    },

    /// Revoke a session key -- must be signed by the account owner.
    RevokeSession {
        /// The derived address of the session key to revoke.
        session_address: Address,
    },

    // -- Asset Rentals --------------------------------------------------------
    /// List an asset for rent at a per-block price with a required deposit.
    ListForRent {
        asset_class_id: Hash,
        asset_id: Hash,
        price_per_block: u64,
        deposit: u64,
        min_duration: u64,
        max_duration: u64,
    },

    /// Rent an asset that is currently listed.
    RentAsset { rental_id: Hash, duration: u64 },

    /// Return a rented asset before it expires.
    ReturnRental { rental_id: Hash },

    /// Claim an asset back from an expired rental (owner action).
    ClaimExpiredRental { rental_id: Hash },

    /// Cancel a rental listing that has not yet been rented.
    CancelRentalListing { rental_id: Hash },

    // -- Guilds ---------------------------------------------------------------
    /// Create a new guild. The signer becomes the leader.
    CreateGuild {
        name: String,
        description: String,
        max_members: u32,
    },

    /// Join an existing guild as a regular member.
    JoinGuild { guild_id: Hash },

    /// Leave a guild the signer is currently a member of.
    LeaveGuild { guild_id: Hash },

    /// Deposit native tokens into the guild treasury.
    GuildDeposit { guild_id: Hash, amount: u64 },

    /// Withdraw native tokens from the guild treasury (leader/officer only).
    GuildWithdraw { guild_id: Hash, amount: u64 },

    /// Promote a guild member to a new role (leader only).
    GuildPromote {
        guild_id: Hash,
        member: Address,
        role: String,
    },

    /// Kick a member from the guild (leader/officer only).
    GuildKick { guild_id: Hash, member: Address },

    // -- Tournaments ----------------------------------------------------------
    /// Create a new tournament with an entry fee and prize distribution.
    CreateTournament {
        name: String,
        game_id: String,
        entry_fee: u64,
        max_participants: u32,
        min_participants: u32,
        start_height: u64,
        prize_distribution: Vec<u32>,
    },

    /// Join a tournament that is in the registration phase.
    JoinTournament { tournament_id: Hash },

    /// Start a tournament once enough participants have registered.
    StartTournament { tournament_id: Hash },

    /// Report final rankings for a completed tournament (organizer only).
    ReportTournamentResults {
        tournament_id: Hash,
        rankings: Vec<Address>,
    },

    /// Claim a tournament prize based on ranking.
    ClaimTournamentPrize { tournament_id: Hash },

    /// Cancel a tournament and refund all entry fees.
    CancelTournament { tournament_id: Hash },
}

impl TransactionAction {
    /// Return a human-readable label for the action variant.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Transfer { .. } => "transfer",
            Self::CreateAssetClass { .. } => "create_asset_class",
            Self::MintAsset { .. } => "mint_asset",
            Self::TransferAsset { .. } => "transfer_asset",
            Self::BurnAsset { .. } => "burn_asset",
            Self::CreateListing { .. } => "create_listing",
            Self::CancelListing { .. } => "cancel_listing",
            Self::BuyListing { .. } => "buy_listing",
            Self::CreateProfile { .. } => "create_profile",
            Self::AddAchievement { .. } => "add_achievement",
            Self::UpdateReputation { .. } => "update_reputation",
            Self::RegisterValidator { .. } => "register_validator",
            Self::DelegateStake { .. } => "delegate_stake",
            Self::UndelegateStake { .. } => "undelegate_stake",
            Self::RegisterAttestor { .. } => "register_attestor",
            Self::SubmitMatchResult { .. } => "submit_match_result",
            Self::DistributeReward { .. } => "distribute_reward",
            Self::SubmitProposal { .. } => "submit_proposal",
            Self::VoteProposal { .. } => "vote_proposal",
            Self::ExecuteProposal { .. } => "execute_proposal",
            Self::CreateSession { .. } => "create_session",
            Self::RevokeSession { .. } => "revoke_session",
            // Rentals
            Self::ListForRent { .. } => "list_for_rent",
            Self::RentAsset { .. } => "rent_asset",
            Self::ReturnRental { .. } => "return_rental",
            Self::ClaimExpiredRental { .. } => "claim_expired_rental",
            Self::CancelRentalListing { .. } => "cancel_rental_listing",
            // Guilds
            Self::CreateGuild { .. } => "create_guild",
            Self::JoinGuild { .. } => "join_guild",
            Self::LeaveGuild { .. } => "leave_guild",
            Self::GuildDeposit { .. } => "guild_deposit",
            Self::GuildWithdraw { .. } => "guild_withdraw",
            Self::GuildPromote { .. } => "guild_promote",
            Self::GuildKick { .. } => "guild_kick",
            // Tournaments
            Self::CreateTournament { .. } => "create_tournament",
            Self::JoinTournament { .. } => "join_tournament",
            Self::StartTournament { .. } => "start_tournament",
            Self::ReportTournamentResults { .. } => "report_tournament_results",
            Self::ClaimTournamentPrize { .. } => "claim_tournament_prize",
            Self::CancelTournament { .. } => "cancel_tournament",
        }
    }
}

// ---------------------------------------------------------------------------
// Transaction
// ---------------------------------------------------------------------------

/// An unsigned transaction ready to be signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Transaction {
    /// Chain identifier to prevent cross-chain replay.
    pub chain_id: String,
    /// Monotonically increasing per-account nonce.
    pub nonce: u64,
    /// The account that authorizes this transaction.
    pub signer: Address,
    /// The operation to execute.
    pub action: TransactionAction,
    /// Maximum fee (in native tokens) the signer is willing to pay.
    pub max_fee: u64,
    /// Unix timestamp (seconds) when the transaction was created.
    pub timestamp: u64,
    /// If `Some`, this transaction is signed by a session key.
    ///
    /// The value is the session address (derived from the session public key).
    /// When present:
    /// - `signer` is the granting account (the real player who pays fees)
    /// - `signer_pubkey` in `SignedTransaction` is the session's public key
    /// - The Ed25519 signature is from the session private key
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Address>,
    /// If `Some`, the given address pays the gas fee instead of the signer.
    ///
    /// This enables meta-transactions / gas sponsorship where a third party
    /// (e.g. a game studio) covers the gas cost so that new players can
    /// transact without holding tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sponsor: Option<Address>,
}

impl Transaction {
    /// Serialize the transaction to Borsh bytes (the canonical signing payload).
    pub fn signing_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("borsh serialization of Transaction should not fail")
    }
}

// ---------------------------------------------------------------------------
// SignedTransaction
// ---------------------------------------------------------------------------

/// A transaction that has been signed by the signer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SignedTransaction {
    /// The underlying transaction.
    pub transaction: Transaction,
    /// The Ed25519 (or similar) signature over `transaction.signing_bytes()`.
    pub signature: Signature,
    /// The hash of the signed transaction (computed externally, typically
    /// SHA-256 over the borsh encoding of `(transaction, signature)`).
    pub tx_hash: Hash,
    /// The Ed25519 public key of the signer (32 bytes).
    ///
    /// Storing the public key alongside the transaction allows anyone to verify
    /// the signature without an external lookup. The address in
    /// `transaction.signer` must equal `SHA-256(signer_pubkey)`.
    pub signer_pubkey: Vec<u8>,
}

impl SignedTransaction {
    /// Create a new signed transaction with a precomputed hash.
    pub fn new(
        transaction: Transaction,
        signature: Signature,
        tx_hash: Hash,
        signer_pubkey: Vec<u8>,
    ) -> Self {
        Self {
            transaction,
            signature,
            tx_hash,
            signer_pubkey,
        }
    }

    /// Convenience: the signer address.
    pub fn signer(&self) -> &Address {
        &self.transaction.signer
    }

    /// Convenience: the action label.
    pub fn action_label(&self) -> &'static str {
        self.transaction.action.label()
    }

    /// Convenience: the nonce.
    pub fn nonce(&self) -> u64 {
        self.transaction.nonce
    }
}

// ---------------------------------------------------------------------------
// TxLocation
// ---------------------------------------------------------------------------

/// Maps a transaction hash to its location in the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TxLocation {
    /// The block height at which the transaction was included.
    pub block_height: u64,
    /// The index of the transaction within the block.
    pub tx_index: u32,
}

// ---------------------------------------------------------------------------
// TransactionReceipt
// ---------------------------------------------------------------------------

/// Emitted after a transaction has been executed, recording the outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TransactionReceipt {
    /// Hash of the executed transaction.
    pub tx_hash: Hash,
    /// Block height at which the transaction was included.
    pub block_height: u64,
    /// `true` if execution succeeded.
    pub success: bool,
    /// Actual fee consumed.
    pub fee_used: u64,
    /// Gas units consumed by this transaction.
    pub gas_used: u64,
    /// The address that actually paid the gas fee (sponsor if present, else signer).
    pub fee_payer: Address,
    /// Events emitted during execution.
    pub events: Vec<Event>,
    /// If execution failed, the error message.
    pub error: Option<String>,
}

impl TransactionReceipt {
    /// Create a receipt for a successful transaction.
    pub fn success(
        tx_hash: Hash,
        block_height: u64,
        fee_used: u64,
        gas_used: u64,
        fee_payer: Address,
        events: Vec<Event>,
    ) -> Self {
        Self {
            tx_hash,
            block_height,
            success: true,
            fee_used,
            gas_used,
            fee_payer,
            events,
            error: None,
        }
    }

    /// Create a receipt for a failed transaction.
    pub fn failure(
        tx_hash: Hash,
        block_height: u64,
        fee_used: u64,
        gas_used: u64,
        fee_payer: Address,
        error: String,
    ) -> Self {
        Self {
            tx_hash,
            block_height,
            success: false,
            fee_used,
            gas_used,
            fee_payer,
            events: Vec::new(),
            error: Some(error),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_transaction() -> Transaction {
        Transaction {
            chain_id: "polay-testnet-1".into(),
            nonce: 42,
            signer: Address::ZERO,
            action: TransactionAction::Transfer {
                to: Address::new([1u8; 32]),
                amount: 1_000_000,
            },
            max_fee: 5000,
            timestamp: 1700000000,
            session: None,
            sponsor: None,
        }
    }

    #[test]
    fn action_label() {
        let tx = sample_transaction();
        assert_eq!(tx.action.label(), "transfer");
    }

    #[test]
    fn signing_bytes_deterministic() {
        let tx = sample_transaction();
        let a = tx.signing_bytes();
        let b = tx.signing_bytes();
        assert_eq!(a, b);
    }

    #[test]
    fn serde_round_trip_transaction() {
        let tx = sample_transaction();
        let json = serde_json::to_string(&tx).unwrap();
        let parsed: Transaction = serde_json::from_str(&json).unwrap();
        assert_eq!(tx, parsed);
    }

    #[test]
    fn borsh_round_trip_transaction() {
        let tx = sample_transaction();
        let encoded = borsh::to_vec(&tx).unwrap();
        let decoded = Transaction::try_from_slice(&encoded).unwrap();
        assert_eq!(tx, decoded);
    }

    #[test]
    fn serde_round_trip_signed_transaction() {
        let stx = SignedTransaction::new(
            sample_transaction(),
            Signature::ZERO,
            Hash::ZERO,
            vec![0u8; 32],
        );
        let json = serde_json::to_string(&stx).unwrap();
        let parsed: SignedTransaction = serde_json::from_str(&json).unwrap();
        assert_eq!(stx, parsed);
    }

    #[test]
    fn borsh_round_trip_signed_transaction() {
        let stx = SignedTransaction::new(
            sample_transaction(),
            Signature::ZERO,
            Hash::ZERO,
            vec![0u8; 32],
        );
        let encoded = borsh::to_vec(&stx).unwrap();
        let decoded = SignedTransaction::try_from_slice(&encoded).unwrap();
        assert_eq!(stx, decoded);
    }

    #[test]
    fn tx_location_serde_round_trip() {
        let loc = TxLocation {
            block_height: 42,
            tx_index: 7,
        };
        let json = serde_json::to_string(&loc).unwrap();
        let parsed: TxLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(loc, parsed);
    }

    #[test]
    fn tx_location_borsh_round_trip() {
        let loc = TxLocation {
            block_height: 100,
            tx_index: 3,
        };
        let encoded = borsh::to_vec(&loc).unwrap();
        let decoded = TxLocation::try_from_slice(&encoded).unwrap();
        assert_eq!(loc, decoded);
    }

    #[test]
    fn receipt_success() {
        let receipt =
            TransactionReceipt::success(Hash::ZERO, 100, 500, 21000, Address::ZERO, vec![]);
        assert!(receipt.success);
        assert!(receipt.error.is_none());
        assert_eq!(receipt.gas_used, 21000);
        assert_eq!(receipt.fee_payer, Address::ZERO);
    }

    #[test]
    fn receipt_failure() {
        let receipt =
            TransactionReceipt::failure(Hash::ZERO, 100, 200, 21000, Address::ZERO, "boom".into());
        assert!(!receipt.success);
        assert_eq!(receipt.error.as_deref(), Some("boom"));
        assert_eq!(receipt.gas_used, 21000);
    }

    #[test]
    fn all_action_variants_serialize() {
        // Ensure every variant can survive a serde round-trip.
        let actions: Vec<TransactionAction> = vec![
            TransactionAction::Transfer {
                to: Address::ZERO,
                amount: 1,
            },
            TransactionAction::CreateAssetClass {
                name: "Gold".into(),
                symbol: "GLD".into(),
                asset_type: AssetType::Fungible,
                max_supply: Some(1_000_000),
                metadata_uri: "https://example.com".into(),
            },
            TransactionAction::MintAsset {
                asset_class_id: Hash::ZERO,
                to: Address::ZERO,
                amount: 100,
                metadata: None,
            },
            TransactionAction::TransferAsset {
                asset_class_id: Hash::ZERO,
                to: Address::ZERO,
                amount: 50,
            },
            TransactionAction::BurnAsset {
                asset_class_id: Hash::ZERO,
                amount: 10,
            },
            TransactionAction::CreateListing {
                asset_class_id: Hash::ZERO,
                amount: 5,
                price_per_unit: 200,
                currency: Hash::ZERO,
            },
            TransactionAction::CancelListing {
                listing_id: Hash::ZERO,
            },
            TransactionAction::BuyListing {
                listing_id: Hash::ZERO,
            },
            TransactionAction::CreateProfile {
                username: "alice".into(),
                display_name: "Alice".into(),
                metadata: None,
            },
            TransactionAction::AddAchievement {
                player: Address::ZERO,
                achievement_id: "first_win".into(),
                name: "First Win".into(),
                metadata: "{}".into(),
            },
            TransactionAction::UpdateReputation {
                player: Address::ZERO,
                delta: -5,
                reason: "toxic behavior".into(),
            },
            TransactionAction::RegisterValidator {
                commission_bps: 500,
            },
            TransactionAction::DelegateStake {
                validator: Address::ZERO,
                amount: 10_000,
            },
            TransactionAction::UndelegateStake {
                validator: Address::ZERO,
                amount: 5_000,
            },
            TransactionAction::RegisterAttestor {
                game_id: "chess".into(),
                endpoint: "https://attestor.example.com".into(),
                metadata: "{}".into(),
            },
            TransactionAction::SubmitMatchResult {
                match_result: MatchResult {
                    match_id: Hash::ZERO,
                    game_id: "chess".into(),
                    timestamp: 0,
                    players: vec![Address::ZERO],
                    scores: vec![1],
                    winners: vec![Address::ZERO],
                    reward_pool: 0,
                    server_signature: vec![],
                    anti_cheat_score: None,
                    replay_ref: None,
                },
            },
            TransactionAction::DistributeReward {
                match_id: Hash::ZERO,
                rewards: vec![(Address::ZERO, 1000)],
            },
            TransactionAction::SubmitProposal {
                action: crate::governance::ProposalAction::TextProposal {
                    title: "Signal".into(),
                    description: "A signaling proposal".into(),
                },
                title: "Signal Proposal".into(),
                description: "Testing governance".into(),
                deposit: 100_000,
            },
            TransactionAction::VoteProposal {
                proposal_id: Hash::ZERO,
                option: crate::governance::VoteOption::Yes,
            },
            TransactionAction::ExecuteProposal {
                proposal_id: Hash::ZERO,
            },
            TransactionAction::CreateSession {
                session_pubkey: vec![0xAA; 32],
                permissions: SessionPermission::All,
                expires_at: 10_000,
                spending_limit: 1_000_000,
            },
            TransactionAction::RevokeSession {
                session_address: Address::ZERO,
            },
        ];

        for action in &actions {
            let json = serde_json::to_string(action).unwrap();
            let parsed: TransactionAction = serde_json::from_str(&json).unwrap();
            assert_eq!(action, &parsed, "failed for {}", action.label());
        }
    }

    #[test]
    fn all_action_variants_borsh() {
        let action = TransactionAction::DistributeReward {
            match_id: Hash::ZERO,
            rewards: vec![(Address::ZERO, 500), (Address::new([1u8; 32]), 500)],
        };
        let encoded = borsh::to_vec(&action).unwrap();
        let decoded = TransactionAction::try_from_slice(&encoded).unwrap();
        assert_eq!(action, decoded);
    }
}
