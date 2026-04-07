use std::sync::Arc;

use tokio::time::{self, Duration};
use tracing::{debug, error, info, warn};

use polay_config::ChainConfig;
use polay_consensus::{
    ConsensusAction, ConsensusStateMachine, Proposal, ValidatorSet,
    ValidatorWeight, Vote, VoteType,
};
use polay_crypto::PolayKeypair;
use polay_execution::Executor;
use polay_genesis::Genesis;
use polay_mempool::{Mempool, MempoolConfig};
use polay_network::{ConsensusVoteMsg, P2PEvent, P2PService};
use polay_rpc::event_bus::{ChainEvent, EventBus};
use polay_state::StateStore;
use polay_types::address::Address;
use polay_types::hash::Hash;
use polay_types::signature::Signature;

use crate::block_producer::BlockProducer;
use crate::block_validator::BlockValidator;
use crate::chain::ChainState;
use crate::epoch::EpochManager;
use crate::error::ValidatorResult;

// ---------------------------------------------------------------------------
// ValidatorNode
// ---------------------------------------------------------------------------

/// The top-level validator node orchestrator.
///
/// Supports two modes of operation:
///
/// - **Single-validator mode** (`run_single_validator`): The original simple
///   loop that produces blocks on a timer. Suitable for local devnet use.
///
/// - **Multi-validator consensus mode** (`run`): Event-driven loop that
///   integrates the BFT consensus state machine with P2P networking for
///   multi-node operation.
pub struct ValidatorNode {
    /// Chain-level state manager.
    chain_state: ChainState,
    /// The transaction mempool (shared via `Arc` with the RPC layer).
    mempool: Arc<Mempool>,
    /// The transaction execution engine.
    executor: Executor,
    /// The block production helper.
    block_producer: BlockProducer,
    /// Pre-consensus block validator -- verifies block contents before voting.
    block_validator: BlockValidator,
    /// The validator's signing keypair.
    keypair: PolayKeypair,
    /// Chain-wide configuration parameters.
    chain_config: ChainConfig,
    /// BFT consensus state machine (initialized in consensus mode).
    consensus: Option<ConsensusStateMachine>,
    /// The validator set for consensus.
    validator_set: Option<ValidatorSet>,
    /// P2P networking service (present when running in multi-validator mode).
    network: Option<P2PService>,
    /// Event bus for broadcasting chain events to WebSocket subscribers.
    event_bus: Option<Arc<EventBus>>,
    /// Epoch manager for automatic validator set rotation.
    epoch_manager: EpochManager,
}

impl ValidatorNode {
    /// Create a new validator node, initializing chain state from genesis.
    ///
    /// If the store already contains state at height > 0 the genesis
    /// initialization is skipped.
    pub fn new(
        store: Arc<dyn StateStore>,
        genesis: &Genesis,
        keypair: PolayKeypair,
        chain_config: ChainConfig,
    ) -> ValidatorResult<Self> {
        let chain_state = ChainState::new(Arc::clone(&store), chain_config.clone());
        chain_state.init_from_genesis(genesis)?;

        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));
        let executor = Executor::new(chain_config.clone());
        let block_producer = BlockProducer::new(chain_config.clone(), keypair.clone());
        let block_validator = BlockValidator::new(chain_config.clone());
        let epoch_manager = EpochManager::new(chain_config.clone());

        info!(
            address = %keypair.address(),
            chain_id = %chain_config.chain_id,
            "validator node initialized"
        );

        Ok(Self {
            chain_state,
            mempool,
            executor,
            block_producer,
            block_validator,
            keypair,
            chain_config,
            consensus: None,
            validator_set: None,
            network: None,
            event_bus: None,
            epoch_manager,
        })
    }

    /// Set the event bus for publishing chain events to WebSocket subscribers.
    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// Return a shared reference to the event bus, if set.
    pub fn event_bus(&self) -> Option<Arc<EventBus>> {
        self.event_bus.clone()
    }

    /// Set the P2P network service for multi-validator mode.
    pub fn set_network(&mut self, network: P2PService) {
        self.network = Some(network);
    }

    /// Initialize the consensus state machine with the given validator set.
    pub fn init_consensus(&mut self, validator_set: ValidatorSet) {
        let height = self.chain_state.get_height().unwrap_or(0) + 1;
        let local_address = self.keypair.address();

        info!(
            height,
            validators = validator_set.len(),
            address = %local_address,
            "initializing BFT consensus"
        );

        self.consensus = Some(ConsensusStateMachine::new(
            height,
            validator_set.clone(),
            local_address,
        ));
        self.validator_set = Some(validator_set);
    }

    /// Build a `ValidatorSet` from the genesis validators.
    pub fn validator_set_from_genesis(genesis: &Genesis) -> ValidatorSet {
        let weights: Vec<ValidatorWeight> = genesis
            .validators
            .iter()
            .filter_map(|gv| {
                let bytes = hex::decode(&gv.address).ok()?;
                if bytes.len() != 32 {
                    return None;
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Some(ValidatorWeight::new(Address::new(arr), gv.stake))
            })
            .collect();
        ValidatorSet::new(weights)
    }

    /// Return a shared reference to the mempool.
    pub fn mempool(&self) -> Arc<Mempool> {
        Arc::clone(&self.mempool)
    }

    /// Return a reference to the chain state manager.
    pub fn chain_state(&self) -> &ChainState {
        &self.chain_state
    }

    /// Return a reference to the chain configuration.
    pub fn chain_config(&self) -> &ChainConfig {
        &self.chain_config
    }

    /// Return the underlying store `Arc`.
    pub fn store_arc(&self) -> Arc<dyn StateStore> {
        self.chain_state.store_arc()
    }

    // =======================================================================
    // Multi-validator consensus loop
    // =======================================================================

    /// Run the consensus-driven multi-validator loop.
    ///
    /// This is the main entry point for multi-node operation. It integrates
    /// the BFT consensus state machine with P2P networking.
    pub async fn run(&mut self) {
        let block_interval = Duration::from_millis(self.chain_config.block_time_ms);
        let mut block_timer = time::interval(block_interval);

        info!(
            block_time_ms = self.chain_config.block_time_ms,
            address = %self.keypair.address(),
            "starting consensus-driven validator loop"
        );

        // If consensus is initialized, start the first round.
        if self.consensus.is_some() {
            self.on_block_timer().await;
        }

        loop {
            // If we have a network, use the full event-driven loop.
            if let Some(ref mut network) = self.network {
                tokio::select! {
                    _ = block_timer.tick() => {
                        self.on_block_timer_inner().await;
                    }
                    Some(event) = network.recv_event() => {
                        match event {
                            P2PEvent::TransactionReceived(tx) => {
                                self.on_received_transaction(tx).await;
                            }
                            P2PEvent::BlockReceived(block) => {
                                self.on_received_block(block).await;
                            }
                            P2PEvent::ConsensusMessageReceived(msg) => {
                                self.on_consensus_message(msg).await;
                            }
                            P2PEvent::PeerConnected(peer) => {
                                info!(%peer, "peer connected");
                            }
                            P2PEvent::PeerDisconnected(peer) => {
                                info!(%peer, "peer disconnected");
                            }
                            P2PEvent::PeerCount(count) => {
                                debug!(count, "peer count");
                            }
                        }
                    }
                }
            } else {
                // No network: fallback to single-validator with consensus.
                block_timer.tick().await;
                self.on_block_timer_inner().await;
            }
        }
    }

    /// Run the simplified single-validator block production loop.
    ///
    /// This is the original devnet loop: sleep, produce, apply, repeat.
    pub async fn run_single_validator(&self, block_interval_ms: u64) {
        let interval = Duration::from_millis(block_interval_ms);

        info!(
            block_interval_ms,
            address = %self.keypair.address(),
            "starting single-validator loop"
        );

        loop {
            time::sleep(interval).await;

            if let Err(e) = self.produce_and_apply_block().await {
                error!(error = %e, "block production failed");
            }
        }
    }

    // -- Block timer handler -----------------------------------------------

    async fn on_block_timer(&mut self) {
        self.on_block_timer_inner().await;
    }

    async fn on_block_timer_inner(&mut self) {
        // Check if consensus is available.
        let (is_proposer, is_start_state) = match self.consensus.as_ref() {
            Some(c) => (
                c.is_proposer(),
                c.step == polay_consensus::ConsensusState::NewRound
                    || c.step == polay_consensus::ConsensusState::Propose,
            ),
            None => {
                // No consensus engine: fall back to simple block production.
                if let Err(e) = self.produce_and_apply_block().await {
                    error!(error = %e, "block production failed");
                }
                return;
            }
        };

        // If we are the proposer for the current height/round, produce and
        // broadcast a proposal.
        if is_proposer && is_start_state {
            if let Some(c) = self.consensus.as_ref() {
                info!(
                    height = c.height,
                    round = c.round,
                    "we are the proposer, producing block"
                );
            }

            match self.produce_proposal().await {
                Ok((proposal, prevote)) => {
                    let proposal_clone = proposal.clone();
                    let block_for_broadcast = proposal.block.clone();

                    // Feed our own proposal into the state machine.
                    let proposal_action = self
                        .consensus
                        .as_mut()
                        .and_then(|c| c.on_proposal(proposal_clone).ok());

                    if let Some(action) = proposal_action {
                        self.handle_consensus_action(action).await;
                    }

                    // Broadcast the block as a proposal.
                    self.broadcast_block(block_for_broadcast).await;

                    // Broadcast our prevote.
                    self.broadcast_vote(&prevote).await;

                    // Feed our own prevote into the state machine.
                    let prevote_action = self
                        .consensus
                        .as_mut()
                        .and_then(|c| match c.on_prevote(prevote) {
                            Ok(Some(action)) => Some(action),
                            Ok(None) => None,
                            Err(e) => {
                                debug!(error = %e, "own prevote processing note");
                                None
                            }
                        });

                    if let Some(action) = prevote_action {
                        self.handle_consensus_action(action).await;
                    }
                }
                Err(e) => {
                    error!(error = %e, "failed to produce proposal");
                }
            }
        } else {
            // Not the proposer: set step to Propose to await a proposal.
            if let Some(c) = self.consensus.as_mut() {
                if c.step == polay_consensus::ConsensusState::NewRound {
                    c.step = polay_consensus::ConsensusState::Propose;
                }
            }
        }
    }

    // -- Received transaction handler --------------------------------------

    async fn on_received_transaction(
        &self,
        tx: polay_types::transaction::SignedTransaction,
    ) {
        debug!(hash = %tx.tx_hash, "received transaction from network");
        if let Err(e) = self.mempool.insert(tx.clone()) {
            debug!(error = %e, "failed to insert received tx into mempool");
        }
        // Rebroadcast to other peers (gossipsub handles dedup).
    }

    // -- Received block handler --------------------------------------------

    async fn on_received_block(&mut self, block: polay_types::block::Block) {
        if self.consensus.is_none() {
            return;
        }

        let block_hash = *block.hash();
        let proposer = block.header.proposer;
        let height = block.height();
        let round = self.consensus.as_ref().map(|c| c.round).unwrap_or(0);

        info!(
            height,
            block_hash = %block_hash,
            proposer = %proposer,
            "received block proposal from network"
        );

        // CRITICAL: Validate block contents before voting.
        // Without this check, a malicious proposer could get invalid blocks
        // committed by the network.
        let expected_height = self.chain_state.get_height().unwrap_or(0) + 1;
        let expected_parent_hash = self
            .chain_state
            .get_latest_hash()
            .unwrap_or(Hash::ZERO);

        if let Err(e) = self.block_validator.validate_proposed_block(
            &block,
            expected_height,
            &expected_parent_hash,
            self.chain_state.store(),
        ) {
            warn!(
                error = %e,
                height,
                block_hash = %block_hash,
                "rejecting invalid block proposal"
            );
            return; // Don't vote for this block.
        }

        // Block is valid -- feed to consensus.
        let proposal = Proposal {
            height,
            round,
            block,
            proposer,
            signature: Signature::ZERO,
        };

        let action = self
            .consensus
            .as_mut()
            .and_then(|c| match c.on_proposal(proposal) {
                Ok(action) => Some(action),
                Err(e) => {
                    warn!(error = %e, height, "received block proposal rejected");
                    None
                }
            });

        if let Some(action) = action {
            self.handle_consensus_action(action).await;
        }
    }

    // -- Received consensus message handler --------------------------------

    async fn on_consensus_message(&mut self, msg: ConsensusVoteMsg) {
        if self.consensus.is_none() {
            return;
        }

        debug!(
            height = msg.height,
            round = msg.round,
            vote_type = %msg.vote_type,
            voter = %msg.voter,
            "received consensus message"
        );

        let vote = Vote {
            height: msg.height,
            round: msg.round,
            vote_type: if msg.vote_type == "prevote" {
                VoteType::Prevote
            } else {
                VoteType::Precommit
            },
            block_hash: msg.block_hash,
            voter: msg.voter,
            signature: msg.voter_signature,
        };

        let action = {
            let consensus = self.consensus.as_mut().unwrap();
            match vote.vote_type {
                VoteType::Prevote => match consensus.on_prevote(vote) {
                    Ok(Some(action)) => Some(action),
                    Ok(None) => None,
                    Err(e) => {
                        debug!(error = %e, "prevote processing error");
                        None
                    }
                },
                VoteType::Precommit => match consensus.on_precommit(vote) {
                    Ok(Some(action)) => Some(action),
                    Ok(None) => None,
                    Err(e) => {
                        debug!(error = %e, "precommit processing error");
                        None
                    }
                },
            }
        };

        if let Some(action) = action {
            self.handle_consensus_action(action).await;
        }
    }

    // -- Consensus action handler ------------------------------------------

    async fn handle_consensus_action(&mut self, action: ConsensusAction) {
        match action {
            ConsensusAction::SendProposal(proposal) => {
                info!(
                    height = proposal.height,
                    round = proposal.round,
                    "broadcasting proposal"
                );
                self.broadcast_block(proposal.block).await;
            }
            ConsensusAction::SendPrevote(vote) => {
                info!(
                    height = vote.height,
                    round = vote.round,
                    block_hash = %vote.block_hash,
                    "broadcasting prevote"
                );
                self.broadcast_vote(&vote).await;
            }
            ConsensusAction::SendPrecommit(vote) => {
                info!(
                    height = vote.height,
                    round = vote.round,
                    block_hash = %vote.block_hash,
                    "broadcasting precommit"
                );
                self.broadcast_vote(&vote).await;

                // Feed our own precommit into the state machine, then
                // handle a possible commit action.
                let commit_action = self
                    .consensus
                    .as_mut()
                    .and_then(|c| match c.on_precommit(vote) {
                        Ok(Some(action)) => Some(action),
                        Ok(None) => None,
                        Err(e) => {
                            debug!(error = %e, "own precommit processing note");
                            None
                        }
                    });

                if let Some(ConsensusAction::CommitBlock {
                    height,
                    block_hash,
                    proof,
                }) = commit_action
                {
                    self.handle_commit(height, block_hash, &proof).await;
                }
            }
            ConsensusAction::CommitBlock {
                height,
                block_hash,
                proof,
            } => {
                self.handle_commit(height, block_hash, &proof).await;
            }
            ConsensusAction::ScheduleTimeout { step, duration_ms } => {
                debug!(?step, duration_ms, "consensus timeout scheduled (ignored in timer-driven mode)");
            }
        }
    }

    async fn handle_commit(
        &mut self,
        height: u64,
        block_hash: Hash,
        proof: &polay_consensus::CommitProof,
    ) {
        info!(
            height,
            block_hash = %block_hash,
            votes = proof.vote_count(),
            "committing block via consensus"
        );

        // Find the block from the proposal stored in the consensus engine.
        let block = {
            let consensus = self.consensus.as_ref().unwrap();
            consensus.proposal.as_ref().map(|p| p.block.clone())
        };

        if let Some(block) = block {
            // Even for our own blocks, verify structural integrity for
            // consistency (light validation only -- we produced this block
            // ourselves so full re-execution is redundant).
            let expected_parent = self
                .chain_state
                .get_latest_hash()
                .unwrap_or(Hash::ZERO);
            if let Err(e) = self.block_validator.validate_block_light(
                &block,
                height,
                &expected_parent,
            ) {
                error!(
                    error = %e,
                    height,
                    "own block failed light validation -- this is a bug"
                );
                return;
            }

            // Execute the block transactions.
            let receipts = self.executor.execute_block(
                &block.transactions,
                self.chain_state.store(),
                height,
                &block.header.proposer,
            );

            // Apply the block to state.
            if let Err(e) = self.chain_state.apply_block(&block, &receipts) {
                error!(error = %e, height, "failed to apply committed block");
                return;
            }

            // Prune executed transactions from the mempool.
            let tx_hashes: Vec<_> = block.transactions.iter().map(|tx| tx.tx_hash).collect();
            self.mempool.remove_batch(&tx_hashes);

            // Publish chain events to WebSocket subscribers.
            self.publish_block_events(&block, &receipts);

            info!(
                height,
                hash = %block.hash(),
                txs = block.tx_count(),
                mempool_remaining = self.mempool.size(),
                "block committed via BFT consensus"
            );

            // Check for epoch boundary and process transition.
            self.maybe_process_epoch(height);

            // Advance the consensus to the next height.
            if let Some(consensus) = self.consensus.as_mut() {
                consensus.advance_height(height + 1);
            }
        } else {
            warn!(height, "committed block not found in proposal cache");
        }
    }

    // -- Helper: produce proposal ------------------------------------------

    async fn produce_proposal(&self) -> ValidatorResult<(Proposal, Vote)> {
        let current_height = self.chain_state.get_height()?;
        let parent_hash = self.chain_state.get_latest_hash()?;
        let next_height = current_height + 1;

        let state_commitment = polay_state::compute_state_root(self.chain_state.store())?;
        let state_root = state_commitment.root;

        let (block, _receipts) = self.block_producer.produce_block(
            next_height,
            parent_hash,
            state_root,
            &self.mempool,
            &self.executor,
            self.chain_state.store(),
            &self.chain_config.chain_id,
        )?;

        let round = self
            .consensus
            .as_ref()
            .map(|c| c.round)
            .unwrap_or(0);

        let block_hash = *block.hash();

        let proposal = Proposal {
            height: next_height,
            round,
            block,
            proposer: self.keypair.address(),
            signature: Signature::ZERO,
        };

        // Create our own prevote for this block.
        let prevote = Vote {
            height: next_height,
            round,
            vote_type: VoteType::Prevote,
            block_hash,
            voter: self.keypair.address(),
            signature: Signature::ZERO,
        };

        Ok((proposal, prevote))
    }

    // -- Broadcast helpers -------------------------------------------------

    async fn broadcast_block(&self, block: polay_types::block::Block) {
        if let Some(ref network) = self.network {
            if let Err(e) = network.broadcast_block(block).await {
                warn!(error = %e, "failed to broadcast block");
            }
        }
    }

    async fn broadcast_vote(&self, vote: &Vote) {
        if let Some(ref network) = self.network {
            let msg = ConsensusVoteMsg {
                height: vote.height,
                round: vote.round,
                vote_type: match vote.vote_type {
                    VoteType::Prevote => "prevote".to_string(),
                    VoteType::Precommit => "precommit".to_string(),
                },
                block_hash: vote.block_hash,
                voter: vote.voter,
                voter_signature: vote.signature,
            };
            if let Err(e) = network.broadcast_consensus(msg).await {
                warn!(error = %e, "failed to broadcast consensus vote");
            }
        }
    }

    // -- Original simple block production ----------------------------------

    /// Produce a single block and apply it (for single-validator mode).
    async fn produce_and_apply_block(&self) -> ValidatorResult<()> {
        let current_height = self.chain_state.get_height()?;
        let parent_hash = self.chain_state.get_latest_hash()?;
        let next_height = current_height + 1;

        let state_commitment = polay_state::compute_state_root(self.chain_state.store())?;
        let state_root = state_commitment.root;

        let (block, receipts) = self.block_producer.produce_block(
            next_height,
            parent_hash,
            state_root,
            &self.mempool,
            &self.executor,
            self.chain_state.store(),
            &self.chain_config.chain_id,
        )?;

        self.chain_state.apply_block(&block, &receipts)?;

        let tx_hashes: Vec<_> = block.transactions.iter().map(|tx| tx.tx_hash).collect();
        self.mempool.remove_batch(&tx_hashes);

        // Publish chain events to WebSocket subscribers via the event bus.
        self.publish_block_events(&block, &receipts);

        if block.tx_count() > 0 || next_height % 10 == 0 {
            info!(
                height = next_height,
                hash = %block.hash(),
                txs = block.tx_count(),
                mempool_remaining = self.mempool.size(),
                "block committed"
            );
        }

        // Check for epoch boundary and process transition (state-only in
        // single-validator mode).
        self.maybe_process_epoch_readonly(next_height);

        Ok(())
    }

    // -- Epoch transition helper ---------------------------------------------

    /// Check if the given height is an epoch boundary and process the
    /// transition if so. Updates the consensus engine's validator set.
    fn maybe_process_epoch(&mut self, height: u64) {
        if !self.epoch_manager.is_epoch_boundary(height) {
            return;
        }

        match self
            .epoch_manager
            .process_epoch_transition(height, self.chain_state.store())
        {
            Ok((new_validator_set, epoch_events)) => {
                let epoch = self.epoch_manager.epoch_for_height(height);
                let validator_count = new_validator_set.len();

                // Update the consensus state machine's validator set.
                if let Some(ref mut consensus) = self.consensus {
                    consensus.update_validator_set(new_validator_set.clone());
                }
                self.validator_set = Some(new_validator_set);

                // Publish epoch transition event to the event bus.
                if let Some(ref event_bus) = self.event_bus {
                    // Extract info from the epoch_transition event.
                    if let Some(evt) = epoch_events
                        .iter()
                        .find(|e| e.action == "epoch_transition")
                    {
                        let total_staked: u64 = evt
                            .get_attribute("total_staked")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        let rewards: u64 = evt
                            .get_attribute("rewards_distributed")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);

                        event_bus.publish(ChainEvent::EpochTransition {
                            epoch,
                            validator_count,
                            total_staked,
                            rewards_distributed: rewards,
                        });
                    }
                }

                info!(
                    epoch,
                    validators = validator_count,
                    "epoch transition complete"
                );
            }
            Err(e) => {
                error!(error = %e, height, "epoch transition failed");
            }
        }
    }

    /// Non-mutating epoch check for the single-validator path.
    ///
    /// Only writes epoch state to the store (does not update in-memory
    /// consensus). Used by `produce_and_apply_block` which takes `&self`.
    fn maybe_process_epoch_readonly(&self, height: u64) {
        if !self.epoch_manager.is_epoch_boundary(height) {
            return;
        }

        match self
            .epoch_manager
            .process_epoch_transition(height, self.chain_state.store())
        {
            Ok((_, _)) => {
                info!(
                    epoch = self.epoch_manager.epoch_for_height(height),
                    "epoch transition complete (single-validator)"
                );
            }
            Err(e) => {
                error!(error = %e, height, "epoch transition failed");
            }
        }
    }

    /// Publish chain events for a committed block and its receipts.
    fn publish_block_events(
        &self,
        block: &polay_types::block::Block,
        receipts: &[polay_types::transaction::TransactionReceipt],
    ) {
        let event_bus = match &self.event_bus {
            Some(eb) => eb,
            None => return,
        };

        // NewBlock event.
        event_bus.publish(ChainEvent::NewBlock {
            height: block.header.height,
            hash: block.header.hash.to_hex(),
            tx_count: block.transactions.len(),
            timestamp: block.header.timestamp,
            proposer: block.header.proposer.to_hex(),
        });

        // NewTransaction + TransactionConfirmed events for each tx.
        for (tx, receipt) in block.transactions.iter().zip(receipts.iter()) {
            event_bus.publish(ChainEvent::NewTransaction {
                tx_hash: tx.tx_hash.to_hex(),
                signer: tx.transaction.signer.to_hex(),
                action_type: tx.transaction.action.label().to_string(),
                block_height: block.header.height,
            });

            event_bus.publish(ChainEvent::TransactionConfirmed {
                tx_hash: tx.tx_hash.to_hex(),
                block_height: receipt.block_height,
                success: receipt.success,
                gas_used: receipt.gas_used,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::MemoryStore;

    #[test]
    fn validator_node_creation() {
        let store = Arc::new(MemoryStore::new());
        let genesis = Genesis::generate_devnet();
        let keypair = PolayKeypair::generate();
        let config = genesis.chain_config.clone();

        let node = ValidatorNode::new(store, &genesis, keypair, config).unwrap();

        assert_eq!(node.chain_state().get_height().unwrap(), 0);
        assert_eq!(node.mempool().size(), 0);
    }

    #[tokio::test]
    async fn validator_produces_block() {
        let store = Arc::new(MemoryStore::new());
        let genesis = Genesis::generate_devnet();
        let keypair = PolayKeypair::generate();
        let config = genesis.chain_config.clone();

        let node = ValidatorNode::new(store, &genesis, keypair, config).unwrap();

        node.produce_and_apply_block().await.unwrap();
        assert_eq!(node.chain_state().get_height().unwrap(), 1);

        node.produce_and_apply_block().await.unwrap();
        assert_eq!(node.chain_state().get_height().unwrap(), 2);

        assert_ne!(node.chain_state().get_latest_hash().unwrap(), Hash::ZERO);
    }

    #[tokio::test]
    async fn validator_sequential_blocks_have_correct_parent() {
        let store: Arc<dyn polay_state::StateStore> = Arc::new(MemoryStore::new());
        let genesis = Genesis::generate_devnet();
        let keypair = PolayKeypair::generate();
        let config = genesis.chain_config.clone();

        let node = ValidatorNode::new(
            Arc::clone(&store),
            &genesis,
            keypair,
            config,
        )
        .unwrap();

        node.produce_and_apply_block().await.unwrap();
        let hash_1 = node.chain_state().get_latest_hash().unwrap();

        node.produce_and_apply_block().await.unwrap();

        let view = polay_state::StateView::new(store.as_ref());
        let block_2 = view.get_block(2).unwrap().unwrap();
        assert_eq!(block_2.header.parent_hash, hash_1);
    }

    #[test]
    fn validator_set_from_genesis_works() {
        let genesis = Genesis::generate_devnet();
        let vs = ValidatorNode::validator_set_from_genesis(&genesis);
        assert!(!vs.is_empty());
        assert!(vs.total_stake > 0);
    }

    #[tokio::test]
    async fn consensus_single_validator_produces_and_commits() {
        // Test the consensus-driven loop in single-validator mode.
        let store = Arc::new(MemoryStore::new());
        let genesis = Genesis::generate_devnet();
        let keypair = PolayKeypair::generate();
        let config = genesis.chain_config.clone();

        let mut node = ValidatorNode::new(
            Arc::clone(&store) as Arc<dyn StateStore>,
            &genesis,
            keypair.clone(),
            config,
        )
        .unwrap();

        // Create a single-validator set with our own address.
        let vs = ValidatorSet::new(vec![ValidatorWeight::new(keypair.address(), 100)]);
        node.init_consensus(vs);

        // Simulate a block timer tick, which should produce and self-commit
        // because we are the only validator (quorum of 1).
        node.on_block_timer().await;

        assert_eq!(node.chain_state().get_height().unwrap(), 1);
        assert_ne!(node.chain_state().get_latest_hash().unwrap(), Hash::ZERO);
    }

    #[tokio::test]
    async fn consensus_advances_height_after_commit() {
        let store = Arc::new(MemoryStore::new());
        let genesis = Genesis::generate_devnet();
        let keypair = PolayKeypair::generate();
        let config = genesis.chain_config.clone();

        let mut node = ValidatorNode::new(
            Arc::clone(&store) as Arc<dyn StateStore>,
            &genesis,
            keypair.clone(),
            config,
        )
        .unwrap();

        let vs = ValidatorSet::new(vec![ValidatorWeight::new(keypair.address(), 100)]);
        node.init_consensus(vs);

        // First block.
        node.on_block_timer().await;
        assert_eq!(node.chain_state().get_height().unwrap(), 1);

        // Second block.
        node.on_block_timer().await;
        assert_eq!(node.chain_state().get_height().unwrap(), 2);

        // Consensus height should be at 3 (next to produce).
        assert_eq!(node.consensus.as_ref().unwrap().height, 3);
    }
}
