//! RPC-specific request and response types.
//!
//! These are JSON-friendly wrappers around the core `polay-types` domain
//! objects. Fields that are raw byte arrays in the core types are exposed as
//! hex-encoded strings here so that JSON clients can consume them directly.

use serde::{Deserialize, Serialize};

use polay_types::{
    Address, AssetClass, AssetType, Block, Event, Listing, ListingStatus, MatchResult,
    PlayerProfile, Proposal, ProposalStatus, SignedTransaction, TransactionReceipt, ValidatorInfo,
    ValidatorStatus,
};

// ---------------------------------------------------------------------------
// Submit transaction
// ---------------------------------------------------------------------------

/// Request body for `polay_submitTransaction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTransactionRequest {
    /// The fully signed transaction (JSON-serialized `SignedTransaction`).
    pub signed_transaction: SignedTransaction,
}

/// Response for `polay_submitTransaction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTransactionResponse {
    /// Hex-encoded transaction hash.
    pub tx_hash: String,
}

// ---------------------------------------------------------------------------
// Block
// ---------------------------------------------------------------------------

/// JSON-friendly representation of a block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockResponse {
    pub height: u64,
    pub timestamp: u64,
    pub hash: String,
    pub parent_hash: String,
    pub state_root: String,
    pub transactions_root: String,
    pub proposer: String,
    pub chain_id: String,
    pub tx_count: usize,
    pub transactions: Vec<SignedTransaction>,
}

impl From<Block> for BlockResponse {
    fn from(block: Block) -> Self {
        Self {
            height: block.header.height,
            timestamp: block.header.timestamp,
            hash: block.header.hash.to_hex(),
            parent_hash: block.header.parent_hash.to_hex(),
            state_root: block.header.state_root.to_hex(),
            transactions_root: block.header.transactions_root.to_hex(),
            proposer: block.header.proposer.to_hex(),
            chain_id: block.header.chain_id.clone(),
            tx_count: block.transactions.len(),
            transactions: block.transactions.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Account
// ---------------------------------------------------------------------------

/// JSON-friendly account state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountResponse {
    pub address: String,
    pub nonce: u64,
    pub balance: u64,
    pub created_at: u64,
}

impl From<polay_types::AccountState> for AccountResponse {
    fn from(acct: polay_types::AccountState) -> Self {
        Self {
            address: acct.address.to_hex(),
            nonce: acct.nonce,
            balance: acct.balance,
            created_at: acct.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Asset class
// ---------------------------------------------------------------------------

/// JSON-friendly asset class info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetClassResponse {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub asset_type: AssetType,
    pub total_supply: u64,
    pub max_supply: Option<u64>,
    pub creator: String,
    pub metadata_uri: String,
    pub created_at: u64,
}

impl From<AssetClass> for AssetClassResponse {
    fn from(ac: AssetClass) -> Self {
        Self {
            id: ac.id.to_hex(),
            name: ac.name,
            symbol: ac.symbol,
            asset_type: ac.asset_type,
            total_supply: ac.total_supply,
            max_supply: ac.max_supply,
            creator: ac.creator.to_hex(),
            metadata_uri: ac.metadata_uri,
            created_at: ac.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Asset balance
// ---------------------------------------------------------------------------

/// JSON-friendly asset balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBalanceResponse {
    pub owner: String,
    pub asset_class_id: String,
    pub amount: u64,
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

/// JSON-friendly marketplace listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListingResponse {
    pub id: String,
    pub seller: String,
    pub asset_class_id: String,
    pub amount: u64,
    pub price_per_unit: u64,
    pub currency: String,
    pub status: ListingStatus,
    pub royalty_bps: u16,
    pub created_at: u64,
}

impl From<Listing> for ListingResponse {
    fn from(l: Listing) -> Self {
        Self {
            id: l.id.to_hex(),
            seller: l.seller.to_hex(),
            asset_class_id: l.asset_class_id.to_hex(),
            amount: l.amount,
            price_per_unit: l.price_per_unit,
            currency: l.currency.to_hex(),
            status: l.status,
            royalty_bps: l.royalty_bps,
            created_at: l.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------

/// JSON-friendly player profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub address: String,
    pub username: String,
    pub display_name: String,
    pub reputation: i64,
    pub metadata: Option<String>,
    pub created_at: u64,
}

impl From<PlayerProfile> for ProfileResponse {
    fn from(p: PlayerProfile) -> Self {
        Self {
            address: p.address.to_hex(),
            username: p.username,
            display_name: p.display_name,
            reputation: p.reputation,
            metadata: p.metadata,
            created_at: p.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

/// JSON-friendly validator info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorResponse {
    pub address: String,
    pub stake: u64,
    pub commission_bps: u16,
    pub status: ValidatorStatus,
    pub jailed_until: Option<u64>,
    pub blocks_produced: u64,
}

impl From<ValidatorInfo> for ValidatorResponse {
    fn from(v: ValidatorInfo) -> Self {
        Self {
            address: v.address.to_hex(),
            stake: v.stake,
            commission_bps: v.commission_bps,
            status: v.status,
            jailed_until: v.jailed_until,
            blocks_produced: v.blocks_produced,
        }
    }
}

// ---------------------------------------------------------------------------
// Match result
// ---------------------------------------------------------------------------

/// JSON-friendly match result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResultResponse {
    pub match_id: String,
    pub game_id: String,
    pub timestamp: u64,
    pub players: Vec<String>,
    pub scores: Vec<u64>,
    pub winners: Vec<String>,
    pub reward_pool: u64,
    pub anti_cheat_score: Option<u8>,
    pub replay_ref: Option<String>,
}

impl From<MatchResult> for MatchResultResponse {
    fn from(mr: MatchResult) -> Self {
        Self {
            match_id: mr.match_id.to_hex(),
            game_id: mr.game_id,
            timestamp: mr.timestamp,
            players: mr.players.iter().map(Address::to_hex).collect(),
            scores: mr.scores,
            winners: mr.winners.iter().map(Address::to_hex).collect(),
            reward_pool: mr.reward_pool,
            anti_cheat_score: mr.anti_cheat_score,
            replay_ref: mr.replay_ref,
        }
    }
}

// ---------------------------------------------------------------------------
// Chain info
// ---------------------------------------------------------------------------

/// Top-level chain metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainInfoResponse {
    pub chain_id: String,
    pub height: u64,
    pub latest_hash: String,
    /// Hex-encoded Merkle root of the current chain state.
    pub state_root: String,
    /// Timestamp of the latest block (0 if no blocks yet).
    pub block_time: u64,
}

// ---------------------------------------------------------------------------
// Gas estimate
// ---------------------------------------------------------------------------

/// Response for `polay_estimateGas`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasEstimateResponse {
    /// Gas units this transaction would consume.
    pub gas: u64,
    /// Estimated fee in POL sub-units at the current minimum gas price.
    pub estimated_fee: u64,
    /// Current minimum gas price.
    pub gas_price: u64,
}

// ---------------------------------------------------------------------------
// Unbonding entry
// ---------------------------------------------------------------------------

/// JSON-friendly unbonding entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnbondingEntryResponse {
    pub delegator: String,
    pub validator: String,
    pub amount: u64,
    pub initiated_at: u64,
    pub completion_height: u64,
}

impl From<polay_types::UnbondingEntry> for UnbondingEntryResponse {
    fn from(e: polay_types::UnbondingEntry) -> Self {
        Self {
            delegator: e.delegator.to_hex(),
            validator: e.validator.to_hex(),
            amount: e.amount,
            initiated_at: e.initiated_at,
            completion_height: e.completion_height,
        }
    }
}

// ---------------------------------------------------------------------------
// Proposal
// ---------------------------------------------------------------------------

/// JSON-friendly governance proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalResponse {
    pub id: String,
    pub proposer: String,
    pub title: String,
    pub description: String,
    pub deposit: u64,
    pub status: ProposalStatus,
    pub yes_votes: u64,
    pub no_votes: u64,
    pub abstain_votes: u64,
    pub voting_start_height: u64,
    pub voting_end_height: u64,
    pub created_at: u64,
}

impl From<Proposal> for ProposalResponse {
    fn from(p: Proposal) -> Self {
        Self {
            id: p.id.to_hex(),
            proposer: p.proposer.to_hex(),
            title: p.title,
            description: p.description,
            deposit: p.deposit,
            status: p.status,
            yes_votes: p.yes_votes,
            no_votes: p.no_votes,
            abstain_votes: p.abstain_votes,
            voting_start_height: p.voting_start_height,
            voting_end_height: p.voting_end_height,
            created_at: p.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Receipt
// ---------------------------------------------------------------------------

/// JSON-friendly transaction receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptResponse {
    pub tx_hash: String,
    pub block_height: u64,
    pub tx_index: u32,
    pub success: bool,
    pub gas_used: u64,
    pub fee_used: u64,
    pub events: Vec<EventResponse>,
    pub error: Option<String>,
}

/// JSON-friendly event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventResponse {
    pub module: String,
    pub action: String,
    pub attributes: Vec<(String, String)>,
}

impl From<Event> for EventResponse {
    fn from(e: Event) -> Self {
        Self {
            module: e.module,
            action: e.action,
            attributes: e.attributes,
        }
    }
}

impl ReceiptResponse {
    /// Build a `ReceiptResponse` from a `TransactionReceipt` and its block index.
    pub fn from_receipt(receipt: &TransactionReceipt, tx_index: u32) -> Self {
        Self {
            tx_hash: receipt.tx_hash.to_hex(),
            block_height: receipt.block_height,
            tx_index,
            success: receipt.success,
            gas_used: receipt.gas_used,
            fee_used: receipt.fee_used,
            events: receipt
                .events
                .iter()
                .cloned()
                .map(EventResponse::from)
                .collect(),
            error: receipt.error.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Transaction with status
// ---------------------------------------------------------------------------

/// A transaction enriched with its confirmation status and optional receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionWithStatus {
    /// The full signed transaction.
    pub transaction: serde_json::Value,
    /// "pending" if still in the mempool, "confirmed" if committed in a block.
    pub status: String,
    /// The receipt, if the transaction has been confirmed.
    pub receipt: Option<ReceiptResponse>,
    /// The block height at which the transaction was included, if confirmed.
    pub block_height: Option<u64>,
}

// ---------------------------------------------------------------------------
// Epoch info
// ---------------------------------------------------------------------------

/// JSON-friendly epoch info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochInfoResponse {
    pub epoch: u64,
    pub start_height: u64,
    pub end_height: u64,
    pub validator_set: Vec<String>,
    pub total_staked: u64,
    pub rewards_distributed: u64,
}

impl From<polay_types::EpochInfo> for EpochInfoResponse {
    fn from(info: polay_types::EpochInfo) -> Self {
        Self {
            epoch: info.epoch,
            start_height: info.start_height,
            end_height: info.end_height,
            validator_set: info.validator_set.iter().map(Address::to_hex).collect(),
            total_staked: info.total_staked,
            rewards_distributed: info.rewards_distributed,
        }
    }
}

// ---------------------------------------------------------------------------
// Supply info
// ---------------------------------------------------------------------------

/// JSON-friendly supply info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyInfoResponse {
    pub total_supply: u64,
    pub circulating_supply: u64,
    pub total_staked: u64,
    pub total_burned: u64,
    pub treasury_balance: u64,
    pub total_minted: u64,
    pub total_fees_collected: u64,
}

impl From<polay_types::SupplyInfo> for SupplyInfoResponse {
    fn from(s: polay_types::SupplyInfo) -> Self {
        Self {
            total_supply: s.total_supply,
            circulating_supply: s.circulating_supply,
            total_staked: s.total_staked,
            total_burned: s.total_burned,
            treasury_balance: s.treasury_balance,
            total_minted: s.total_minted,
            total_fees_collected: s.total_fees_collected,
        }
    }
}

/// Inflation rate response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InflationRateResponse {
    /// Current annual inflation rate in basis points.
    pub rate_bps: u16,
    /// Current epoch block reward in POL sub-units.
    pub epoch_reward: u64,
}

// ---------------------------------------------------------------------------
// Health check
// ---------------------------------------------------------------------------

/// Response for `polay_health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub height: u64,
    pub syncing: bool,
}

// ---------------------------------------------------------------------------
// Node info
// ---------------------------------------------------------------------------

/// Response for `polay_getNodeInfo`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfoResponse {
    pub chain_id: String,
    pub node_version: String,
    pub height: u64,
    pub latest_hash: String,
    pub state_root: String,
    pub peer_count: u64,
    pub mempool_size: usize,
    pub uptime_seconds: u64,
    pub block_time_ms: u64,
}

// ---------------------------------------------------------------------------
// Network stats
// ---------------------------------------------------------------------------

/// Response for `polay_getNetworkStats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatsResponse {
    pub height: u64,
    pub total_transactions: u64,
    pub active_validators: usize,
    pub total_staked: u64,
    pub epoch: u64,
    pub block_time_ms: u64,
}
