//! Broadcast-based event bus for real-time chain event notifications.
//!
//! The [`EventBus`] uses a [`tokio::sync::broadcast`] channel to fan out
//! chain events to all connected WebSocket subscribers. Events are published
//! by the validator loop after committing each block and consumed by the
//! RPC subscription handlers.

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// A chain event emitted by the validator and forwarded to WebSocket subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChainEvent {
    /// Emitted when a new block is committed.
    NewBlock {
        height: u64,
        hash: String,
        tx_count: usize,
        timestamp: u64,
        proposer: String,
    },
    /// Emitted for each transaction included in a committed block.
    NewTransaction {
        tx_hash: String,
        signer: String,
        action_type: String,
        block_height: u64,
    },
    /// Emitted after a transaction is executed with its result.
    TransactionConfirmed {
        tx_hash: String,
        block_height: u64,
        success: bool,
        gas_used: u64,
    },
    /// Emitted when a validator's status changes.
    ValidatorUpdate {
        address: String,
        /// One of "registered", "jailed", "slashed", "unjailed".
        event_type: String,
    },
    /// Emitted when a marketplace listing changes status.
    ListingUpdate {
        listing_id: String,
        /// One of "created", "sold", "cancelled".
        status: String,
        asset_class_id: String,
    },
    /// Emitted when a match result is submitted on-chain.
    MatchResultSubmitted {
        match_id: String,
        game_id: String,
        player_count: usize,
    },
    /// Emitted when a governance proposal is created.
    ProposalCreated {
        proposal_id: String,
        title: String,
        proposer: String,
    },
    /// Emitted when a vote is cast on a governance proposal.
    VoteCast {
        proposal_id: String,
        voter: String,
        option: String,
    },
    /// Emitted when a governance proposal is finalized (passed, rejected, or executed).
    ProposalFinalized { proposal_id: String, status: String },
    /// Emitted at every epoch boundary when the validator set is rotated.
    EpochTransition {
        epoch: u64,
        validator_count: usize,
        total_staked: u64,
        rewards_distributed: u64,
    },
}

/// A broadcast-based event bus that fans out [`ChainEvent`]s to subscribers.
pub struct EventBus {
    sender: broadcast::Sender<ChainEvent>,
}

impl EventBus {
    /// Create a new `EventBus` with the given channel capacity.
    ///
    /// If a subscriber falls behind by more than `capacity` events, it will
    /// start receiving `Lagged` errors and miss intermediate events.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish a chain event to all current subscribers.
    ///
    /// If there are no active subscribers the event is silently dropped.
    pub fn publish(&self, event: ChainEvent) {
        let _ = self.sender.send(event);
    }

    /// Create a new subscription receiver for chain events.
    pub fn subscribe(&self) -> broadcast::Receiver<ChainEvent> {
        self.sender.subscribe()
    }
}
