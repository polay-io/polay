//! Main execution engine — dispatches transactions to the appropriate module
//! and produces receipts.

use sha2::{Digest, Sha256};

use polay_config::ChainConfig;
use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{
    AccountState, Address, Event, Hash, SignedTransaction, TransactionAction, TransactionReceipt,
};
use tracing::{debug, error, info, warn};

use crate::error::ExecutionError;
use crate::gas::GasSchedule;
use crate::modules::{
    assets, attestation, governance, guild, identity, market, rental, session, staking, tournament,
    transfer,
};

// ---------------------------------------------------------------------------
// State change tracking (for observability / debugging)
// ---------------------------------------------------------------------------

/// A lightweight record of what changed during execution, useful for tracing
/// and debugging. These are not persisted on-chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateChange {
    AccountUpdated(Address),
    BalanceChanged(Address, i128),
    AssetCreated(Hash),
    AssetMinted(Hash, Address, u64),
    AssetTransferred(Hash, Address, Address, u64),
    AssetBurned(Hash, Address, u64),
    ListingCreated(Hash),
    ListingCancelled(Hash),
    ListingSold(Hash),
    ProfileCreated(Address),
    AchievementAwarded(Address, String),
    ReputationChanged(Address, i64),
    ValidatorRegistered(Address),
    StakeDelegated(Address, Address, u64),
    StakeUndelegated(Address, Address, u64),
    AttestorRegistered(Address),
    MatchResultSubmitted(Hash),
    RewardsDistributed(Hash),
    ProposalSubmitted(Hash),
    ProposalVoted(Hash, Address),
    ProposalExecuted(Hash),
    SessionCreated(Address, Address),
    SessionRevoked(Address, Address),
    SessionSpendingUpdated(Address, Address, u64),
    NonceIncremented(Address),
    FeeDeducted(Address, u64),
}

// ---------------------------------------------------------------------------
// ExecutionResult
// ---------------------------------------------------------------------------

/// The outcome of executing a single transaction.
#[derive(Debug)]
pub struct ExecutionResult {
    /// The transaction receipt (persisted in the block).
    pub receipt: TransactionReceipt,
    /// State changes for observability (not persisted).
    pub state_changes: Vec<StateChange>,
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

/// The main transaction execution engine.
///
/// Processes transactions by dispatching to the appropriate module, deducting
/// fees, incrementing nonces, and producing receipts.
pub struct Executor {
    chain_config: ChainConfig,
}

impl Executor {
    /// Create a new executor with the given chain configuration.
    ///
    /// # Panics
    ///
    /// Panics if `burn_bps + treasury_bps > 10_000` (fee split exceeds 100%).
    pub fn new(chain_config: ChainConfig) -> Self {
        let fd = &chain_config.fee_distribution;
        assert!(
            fd.burn_bps as u32 + fd.treasury_bps as u32 <= 10_000,
            "fee_distribution: burn_bps ({}) + treasury_bps ({}) exceeds 10,000",
            fd.burn_bps,
            fd.treasury_bps,
        );
        Self { chain_config }
    }

    /// Execute a single signed transaction against the state store.
    ///
    /// On success, the receipt has `success: true` and all state mutations
    /// are committed. On failure, the receipt has `success: false` and the
    /// error message is recorded. Fee deduction and nonce increment happen
    /// regardless of the outcome of the action itself.
    pub fn execute_transaction(
        &self,
        tx: &SignedTransaction,
        store: &dyn StateStore,
        block_height: u64,
        block_proposer: &Address,
    ) -> Result<ExecutionResult, ExecutionError> {
        let signer = *tx.signer();
        let timestamp = tx.transaction.timestamp;
        let mut state_changes = Vec::new();

        // Calculate gas needed.
        let gas_used = GasSchedule::total_gas(
            &tx.transaction,
            self.chain_config.base_gas,
            self.chain_config.gas_per_byte,
        );
        let actual_fee = GasSchedule::fee(gas_used, self.chain_config.min_gas_price);

        // Verify max_fee covers the actual cost.
        if tx.transaction.max_fee < actual_fee {
            return Err(ExecutionError::FeeTooLow);
        }

        // Determine who pays the fee: sponsor if present, otherwise signer.
        let fee_payer = tx.transaction.sponsor.unwrap_or(signer);

        // Deduct the actual gas fee from the fee payer and increment signer nonce.
        {
            let view = StateView::new(store);
            let writer = StateWriter::new(store);

            // Always load signer account and increment nonce.
            let mut signer_account = view
                .get_account(&signer)?
                .ok_or_else(|| ExecutionError::AccountNotFound(signer.to_hex()))?;

            if fee_payer == signer {
                // No sponsor — signer pays the fee.
                if signer_account.balance < actual_fee {
                    return Err(ExecutionError::InsufficientBalance {
                        required: actual_fee,
                        available: signer_account.balance,
                    });
                }
                signer_account.balance = signer_account.balance.checked_sub(actual_fee).ok_or(
                    ExecutionError::InsufficientBalance {
                        required: actual_fee,
                        available: signer_account.balance,
                    },
                )?;
                signer_account.increment_nonce();
                writer.set_account(&signer_account)?;

                state_changes.push(StateChange::FeeDeducted(signer, actual_fee));
                state_changes.push(StateChange::NonceIncremented(signer));
            } else {
                // Sponsor pays the fee. Load sponsor account separately.
                let mut sponsor_account = view
                    .get_account(&fee_payer)?
                    .ok_or_else(|| ExecutionError::AccountNotFound(fee_payer.to_hex()))?;

                if sponsor_account.balance < actual_fee {
                    return Err(ExecutionError::InsufficientBalance {
                        required: actual_fee,
                        available: sponsor_account.balance,
                    });
                }
                sponsor_account.balance = sponsor_account.balance.checked_sub(actual_fee).ok_or(
                    ExecutionError::InsufficientBalance {
                        required: actual_fee,
                        available: sponsor_account.balance,
                    },
                )?;
                writer.set_account(&sponsor_account)?;

                // Only increment signer's nonce (sponsor nonce is NOT incremented).
                signer_account.increment_nonce();
                writer.set_account(&signer_account)?;

                state_changes.push(StateChange::FeeDeducted(fee_payer, actual_fee));
                state_changes.push(StateChange::NonceIncremented(signer));
            }
        }

        // Distribute the collected fee: burn / treasury / block producer.
        if actual_fee > 0 {
            let fd = &self.chain_config.fee_distribution;
            let burn_amount = (actual_fee as u128 * fd.burn_bps as u128 / 10_000) as u64;
            let treasury_amount = (actual_fee as u128 * fd.treasury_bps as u128 / 10_000) as u64;
            let validator_amount = actual_fee
                .saturating_sub(burn_amount)
                .saturating_sub(treasury_amount);

            let view = StateView::new(store);
            let writer = StateWriter::new(store);

            // Credit treasury.
            if treasury_amount > 0 {
                let treasury_addr = parse_treasury_address(&self.chain_config.treasury_address);
                let mut treasury_acct = view
                    .get_account(&treasury_addr)?
                    .unwrap_or_else(|| AccountState::new(treasury_addr, 0));
                treasury_acct.balance = treasury_acct.balance.saturating_add(treasury_amount);
                writer.set_account(&treasury_acct)?;
            }

            // Credit block producer.
            if validator_amount > 0 {
                let mut proposer_acct = view
                    .get_account(block_proposer)?
                    .unwrap_or_else(|| AccountState::new(*block_proposer, 0));
                proposer_acct.balance = proposer_acct.balance.saturating_add(validator_amount);
                writer.set_account(&proposer_acct)?;
            }

            // burn_amount: not credited anywhere (truly burned).

            // Update supply info.
            let mut supply = view.get_supply_info()?.unwrap_or_default();
            supply.total_fees_collected = supply.total_fees_collected.saturating_add(actual_fee);
            supply.total_burned = supply.total_burned.saturating_add(burn_amount);
            supply.total_supply = supply.total_supply.saturating_sub(burn_amount);
            supply.treasury_balance = supply.treasury_balance.saturating_add(treasury_amount);
            supply.recompute_circulating();
            writer.set_supply_info(&supply)?;
        }

        // Dispatch to the appropriate module handler.
        let result = self.dispatch_action(
            &tx.transaction.action,
            &signer,
            store,
            timestamp,
            block_height,
        );

        match result {
            Ok((mut events, mut action_changes)) => {
                state_changes.append(&mut action_changes);

                // Emit gas_sponsored event if a sponsor paid.
                if tx.transaction.sponsor.is_some() {
                    events.push(Event::gas_sponsored(&fee_payer, &signer, actual_fee));
                }

                // If this was a session-signed transaction, update the
                // session's cumulative spending counter.
                if let Some(session_addr) = &tx.transaction.session {
                    let view = StateView::new(store);
                    if let Some(mut grant) = view.get_session(&signer, session_addr)? {
                        grant.amount_spent += actual_fee;
                        StateWriter::new(store).set_session(&grant)?;
                        state_changes.push(StateChange::SessionSpendingUpdated(
                            signer,
                            *session_addr,
                            grant.amount_spent,
                        ));
                    }
                }

                debug!(
                    tx_hash = %tx.tx_hash,
                    action = tx.action_label(),
                    events = events.len(),
                    gas_used,
                    actual_fee,
                    "transaction executed successfully"
                );

                let receipt = TransactionReceipt::success(
                    tx.tx_hash,
                    block_height,
                    actual_fee,
                    gas_used,
                    fee_payer,
                    events,
                );

                Ok(ExecutionResult {
                    receipt,
                    state_changes,
                })
            }
            Err(exec_err) => {
                warn!(
                    tx_hash = %tx.tx_hash,
                    action = tx.action_label(),
                    error = %exec_err,
                    "transaction execution failed"
                );

                let receipt = TransactionReceipt::failure(
                    tx.tx_hash,
                    block_height,
                    actual_fee,
                    gas_used,
                    fee_payer,
                    exec_err.to_string(),
                );

                Ok(ExecutionResult {
                    receipt,
                    state_changes,
                })
            }
        }
    }

    /// Calculate gas for a transaction without executing it.
    ///
    /// Used by the gas estimation RPC.
    pub fn estimate_gas(&self, tx: &polay_types::Transaction) -> u64 {
        GasSchedule::total_gas(
            tx,
            self.chain_config.base_gas,
            self.chain_config.gas_per_byte,
        )
    }

    /// Return the current minimum gas price from configuration.
    pub fn min_gas_price(&self) -> u64 {
        self.chain_config.min_gas_price
    }

    /// Execute a single transaction with panic safety.
    ///
    /// Wraps `execute_transaction` in `catch_unwind` so that a panicking
    /// transaction does not crash the entire node.
    pub fn execute_transaction_safe(
        &self,
        tx: &SignedTransaction,
        store: &dyn StateStore,
        block_height: u64,
        block_proposer: &Address,
    ) -> Result<ExecutionResult, ExecutionError> {
        let proposer = *block_proposer;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.execute_transaction(tx, store, block_height, &proposer)
        }))
        .unwrap_or_else(|_| {
            error!(
                tx_hash = %tx.tx_hash,
                "transaction execution panicked"
            );
            Err(ExecutionError::Custom(
                "transaction execution panicked".into(),
            ))
        })
    }

    /// Execute all transactions in a block sequentially.
    ///
    /// Each transaction gets its own receipt. Failures do not abort the block;
    /// the failed transaction's receipt records `success: false`.
    ///
    /// Transactions that would cause the cumulative gas to exceed
    /// `max_block_gas` are skipped with a failure receipt.
    ///
    /// This method also processes mature unbonding entries at the start of the
    /// block (before executing any transactions).
    ///
    /// Individual transaction executions are wrapped in `catch_unwind` for
    /// panic safety.
    pub fn execute_block(
        &self,
        transactions: &[SignedTransaction],
        store: &dyn StateStore,
        height: u64,
        block_proposer: &Address,
    ) -> Vec<TransactionReceipt> {
        let mut receipts = Vec::with_capacity(transactions.len());
        let mut cumulative_gas: u64 = 0;
        let max_block_gas = self.chain_config.max_block_gas;

        // Process mature unbonding entries first.
        if let Err(e) = staking::process_mature_unbondings(store, height) {
            error!(
                error = %e,
                "failed to process mature unbondings at height {}",
                height
            );
        }

        info!(
            height,
            num_transactions = transactions.len(),
            "executing block"
        );

        for tx in transactions {
            // Pre-check: calculate gas for this tx and check block gas limit.
            let tx_gas = GasSchedule::total_gas(
                &tx.transaction,
                self.chain_config.base_gas,
                self.chain_config.gas_per_byte,
            );
            if cumulative_gas.saturating_add(tx_gas) > max_block_gas {
                warn!(
                    tx_hash = %tx.tx_hash,
                    tx_gas,
                    cumulative_gas,
                    max_block_gas,
                    "transaction skipped: would exceed block gas limit"
                );
                let block_fee_payer = tx.transaction.sponsor.unwrap_or(*tx.signer());
                receipts.push(TransactionReceipt::failure(
                    tx.tx_hash,
                    height,
                    0,
                    0,
                    block_fee_payer,
                    "block gas limit exceeded".to_string(),
                ));
                continue;
            }

            match self.execute_transaction_safe(tx, store, height, block_proposer) {
                Ok(result) => {
                    cumulative_gas = cumulative_gas.saturating_add(result.receipt.gas_used);
                    receipts.push(result.receipt);
                }
                Err(err) => {
                    // This means even fee deduction failed (e.g., account not found).
                    // Produce a failure receipt with zero fee.
                    error!(
                        tx_hash = %tx.tx_hash,
                        error = %err,
                        "transaction rejected during execution"
                    );
                    let rejected_fee_payer = tx.transaction.sponsor.unwrap_or(*tx.signer());
                    receipts.push(TransactionReceipt::failure(
                        tx.tx_hash,
                        height,
                        0,
                        0,
                        rejected_fee_payer,
                        err.to_string(),
                    ));
                }
            }
        }

        info!(
            height,
            total = receipts.len(),
            success = receipts.iter().filter(|r| r.success).count(),
            failed = receipts.iter().filter(|r| !r.success).count(),
            "block execution complete"
        );

        receipts
    }

    // -----------------------------------------------------------------------
    // Private: action dispatch
    // -----------------------------------------------------------------------

    fn dispatch_action(
        &self,
        action: &TransactionAction,
        signer: &Address,
        store: &dyn StateStore,
        timestamp: u64,
        block_height: u64,
    ) -> Result<(Vec<Event>, Vec<StateChange>), ExecutionError> {
        match action {
            // -- Transfer --
            TransactionAction::Transfer { to, amount } => {
                let events = transfer::execute_transfer(signer, to, *amount, store, timestamp)?;
                let changes = vec![
                    StateChange::BalanceChanged(*signer, -(*amount as i128)),
                    StateChange::BalanceChanged(*to, *amount as i128),
                ];
                Ok((events, changes))
            }

            // -- Assets --
            TransactionAction::CreateAssetClass {
                name,
                symbol,
                asset_type,
                max_supply,
                metadata_uri,
            } => {
                let (id, events) = assets::execute_create_asset_class(
                    signer,
                    name,
                    symbol,
                    *asset_type,
                    *max_supply,
                    metadata_uri,
                    store,
                    timestamp,
                )?;
                Ok((events, vec![StateChange::AssetCreated(id)]))
            }

            TransactionAction::MintAsset {
                asset_class_id,
                to,
                amount,
                metadata,
            } => {
                let events = assets::execute_mint_asset(
                    signer,
                    asset_class_id,
                    to,
                    *amount,
                    metadata.as_deref(),
                    store,
                )?;
                Ok((
                    events,
                    vec![StateChange::AssetMinted(*asset_class_id, *to, *amount)],
                ))
            }

            TransactionAction::TransferAsset {
                asset_class_id,
                to,
                amount,
            } => {
                let events =
                    assets::execute_transfer_asset(signer, asset_class_id, to, *amount, store)?;
                Ok((
                    events,
                    vec![StateChange::AssetTransferred(
                        *asset_class_id,
                        *signer,
                        *to,
                        *amount,
                    )],
                ))
            }

            TransactionAction::BurnAsset {
                asset_class_id,
                amount,
            } => {
                let events = assets::execute_burn_asset(signer, asset_class_id, *amount, store)?;
                Ok((
                    events,
                    vec![StateChange::AssetBurned(*asset_class_id, *signer, *amount)],
                ))
            }

            // -- Marketplace --
            TransactionAction::CreateListing {
                asset_class_id,
                amount,
                price_per_unit,
                currency,
            } => {
                let (id, events) = market::execute_create_listing(
                    signer,
                    asset_class_id,
                    *amount,
                    *price_per_unit,
                    currency,
                    store,
                    &self.chain_config,
                    timestamp,
                )?;
                Ok((events, vec![StateChange::ListingCreated(id)]))
            }

            TransactionAction::CancelListing { listing_id } => {
                let events = market::execute_cancel_listing(signer, listing_id, store)?;
                Ok((events, vec![StateChange::ListingCancelled(*listing_id)]))
            }

            TransactionAction::BuyListing { listing_id } => {
                let events = market::execute_buy_listing(
                    signer,
                    listing_id,
                    store,
                    &self.chain_config,
                    timestamp,
                )?;
                Ok((events, vec![StateChange::ListingSold(*listing_id)]))
            }

            // -- Identity --
            TransactionAction::CreateProfile {
                username,
                display_name,
                metadata,
            } => {
                let events = identity::execute_create_profile(
                    signer,
                    username,
                    display_name,
                    metadata.as_deref(),
                    store,
                    timestamp,
                )?;
                Ok((events, vec![StateChange::ProfileCreated(*signer)]))
            }

            TransactionAction::AddAchievement {
                player,
                achievement_id,
                name,
                metadata,
            } => {
                let events = identity::execute_add_achievement(
                    signer,
                    player,
                    achievement_id,
                    name,
                    metadata,
                    store,
                    timestamp,
                )?;
                Ok((
                    events,
                    vec![StateChange::AchievementAwarded(
                        *player,
                        achievement_id.clone(),
                    )],
                ))
            }

            TransactionAction::UpdateReputation {
                player,
                delta,
                reason,
            } => {
                let events = identity::execute_update_reputation(
                    signer, player, *delta, reason, store, timestamp,
                )?;
                Ok((
                    events,
                    vec![StateChange::ReputationChanged(*player, *delta)],
                ))
            }

            // -- Staking --
            TransactionAction::RegisterValidator { commission_bps } => {
                let events = staking::execute_register_validator(
                    signer,
                    *commission_bps,
                    store,
                    &self.chain_config,
                )?;
                Ok((events, vec![StateChange::ValidatorRegistered(*signer)]))
            }

            TransactionAction::DelegateStake { validator, amount } => {
                let events = staking::execute_delegate_stake(
                    signer,
                    validator,
                    *amount,
                    store,
                    &self.chain_config,
                    timestamp,
                    block_height,
                )?;
                Ok((
                    events,
                    vec![StateChange::StakeDelegated(*signer, *validator, *amount)],
                ))
            }

            TransactionAction::UndelegateStake { validator, amount } => {
                let events = staking::execute_undelegate_stake(
                    signer,
                    validator,
                    *amount,
                    store,
                    block_height,
                    &self.chain_config,
                )?;
                Ok((
                    events,
                    vec![StateChange::StakeUndelegated(*signer, *validator, *amount)],
                ))
            }

            // -- Attestation --
            TransactionAction::RegisterAttestor {
                game_id,
                endpoint,
                metadata,
            } => {
                let events = attestation::execute_register_attestor(
                    signer, game_id, endpoint, metadata, store, timestamp,
                )?;
                Ok((events, vec![StateChange::AttestorRegistered(*signer)]))
            }

            TransactionAction::SubmitMatchResult { match_result } => {
                let events = attestation::execute_submit_match_result(
                    signer,
                    match_result,
                    store,
                    &self.chain_config,
                    timestamp,
                )?;
                Ok((
                    events,
                    vec![StateChange::MatchResultSubmitted(match_result.match_id)],
                ))
            }

            TransactionAction::DistributeReward { match_id, rewards } => {
                let events = attestation::execute_distribute_reward(
                    signer, match_id, rewards, store, timestamp,
                )?;
                Ok((events, vec![StateChange::RewardsDistributed(*match_id)]))
            }

            // -- Governance --
            TransactionAction::SubmitProposal {
                action,
                title,
                description,
                deposit,
            } => {
                let (id, events) = governance::execute_submit_proposal(
                    signer,
                    action.clone(),
                    title.clone(),
                    description.clone(),
                    *deposit,
                    store,
                    &self.chain_config,
                    block_height,
                )?;
                Ok((events, vec![StateChange::ProposalSubmitted(id)]))
            }

            TransactionAction::VoteProposal {
                proposal_id,
                option,
            } => {
                let events = governance::execute_vote_proposal(
                    signer,
                    proposal_id,
                    option.clone(),
                    store,
                    block_height,
                )?;
                Ok((
                    events,
                    vec![StateChange::ProposalVoted(*proposal_id, *signer)],
                ))
            }

            TransactionAction::ExecuteProposal { proposal_id } => {
                let events = governance::execute_execute_proposal(
                    signer,
                    proposal_id,
                    store,
                    &self.chain_config,
                    block_height,
                )?;
                Ok((events, vec![StateChange::ProposalExecuted(*proposal_id)]))
            }

            // -- Session keys --
            TransactionAction::CreateSession {
                session_pubkey,
                permissions,
                expires_at,
                spending_limit,
            } => {
                let events = session::execute_create_session(
                    signer,
                    session_pubkey.clone(),
                    permissions.clone(),
                    *expires_at,
                    *spending_limit,
                    store,
                    block_height,
                )?;
                // Derive session address for the state change record.
                let digest = Sha256::digest(session_pubkey);
                let mut addr_bytes = [0u8; 32];
                addr_bytes.copy_from_slice(&digest[..32]);
                let session_addr = Address::new(addr_bytes);
                Ok((
                    events,
                    vec![StateChange::SessionCreated(*signer, session_addr)],
                ))
            }

            TransactionAction::RevokeSession { session_address } => {
                let events = session::execute_revoke_session(signer, session_address, store)?;
                Ok((
                    events,
                    vec![StateChange::SessionRevoked(*signer, *session_address)],
                ))
            }

            // -- Rentals --
            TransactionAction::ListForRent {
                asset_class_id,
                asset_id,
                price_per_block,
                deposit,
                min_duration,
                max_duration,
            } => {
                let (id, events) = rental::execute_list_for_rent(
                    signer,
                    asset_class_id,
                    asset_id,
                    *price_per_block,
                    *deposit,
                    *min_duration,
                    *max_duration,
                    store,
                    timestamp,
                )?;
                let _ = id;
                Ok((events, vec![]))
            }

            TransactionAction::RentAsset {
                rental_id,
                duration,
            } => {
                let events =
                    rental::execute_rent_asset(signer, rental_id, *duration, store, block_height)?;
                Ok((events, vec![]))
            }

            TransactionAction::ReturnRental { rental_id } => {
                let events = rental::execute_return_rental(signer, rental_id, store, block_height)?;
                Ok((events, vec![]))
            }

            TransactionAction::ClaimExpiredRental { rental_id } => {
                let events =
                    rental::execute_claim_expired_rental(signer, rental_id, store, block_height)?;
                Ok((events, vec![]))
            }

            TransactionAction::CancelRentalListing { rental_id } => {
                let events = rental::execute_cancel_rental_listing(signer, rental_id, store)?;
                Ok((events, vec![]))
            }

            // -- Guilds --
            TransactionAction::CreateGuild {
                name,
                description,
                max_members,
            } => {
                let (id, events) = guild::execute_create_guild(
                    signer,
                    name,
                    description,
                    *max_members,
                    store,
                    timestamp,
                )?;
                let _ = id;
                Ok((events, vec![]))
            }

            TransactionAction::JoinGuild { guild_id } => {
                let events = guild::execute_join_guild(signer, guild_id, store, block_height)?;
                Ok((events, vec![]))
            }

            TransactionAction::LeaveGuild { guild_id } => {
                let events = guild::execute_leave_guild(signer, guild_id, store)?;
                Ok((events, vec![]))
            }

            TransactionAction::GuildDeposit { guild_id, amount } => {
                let events = guild::execute_guild_deposit(signer, guild_id, *amount, store)?;
                Ok((events, vec![]))
            }

            TransactionAction::GuildWithdraw { guild_id, amount } => {
                let events = guild::execute_guild_withdraw(signer, guild_id, *amount, store)?;
                Ok((events, vec![]))
            }

            TransactionAction::GuildPromote {
                guild_id,
                member,
                role,
            } => {
                let events = guild::execute_guild_promote(signer, guild_id, member, role, store)?;
                Ok((events, vec![]))
            }

            TransactionAction::GuildKick { guild_id, member } => {
                let events = guild::execute_guild_kick(signer, guild_id, member, store)?;
                Ok((events, vec![]))
            }

            // -- Tournaments --
            TransactionAction::CreateTournament {
                name,
                game_id,
                entry_fee,
                max_participants,
                min_participants,
                start_height,
                prize_distribution,
            } => {
                let (id, events) = tournament::execute_create_tournament(
                    signer,
                    name,
                    game_id,
                    *entry_fee,
                    *max_participants,
                    *min_participants,
                    *start_height,
                    prize_distribution,
                    store,
                    timestamp,
                )?;
                let _ = id;
                Ok((events, vec![]))
            }

            TransactionAction::JoinTournament { tournament_id } => {
                let events = tournament::execute_join_tournament(signer, tournament_id, store)?;
                Ok((events, vec![]))
            }

            TransactionAction::StartTournament { tournament_id } => {
                let events = tournament::execute_start_tournament(
                    signer,
                    tournament_id,
                    store,
                    block_height,
                )?;
                Ok((events, vec![]))
            }

            TransactionAction::ReportTournamentResults {
                tournament_id,
                rankings,
            } => {
                let events = tournament::execute_report_tournament_results(
                    signer,
                    tournament_id,
                    rankings,
                    store,
                    block_height,
                )?;
                Ok((events, vec![]))
            }

            TransactionAction::ClaimTournamentPrize { tournament_id } => {
                let events =
                    tournament::execute_claim_tournament_prize(signer, tournament_id, store)?;
                Ok((events, vec![]))
            }

            TransactionAction::CancelTournament { tournament_id } => {
                let events = tournament::execute_cancel_tournament(signer, tournament_id, store)?;
                Ok((events, vec![]))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse the treasury address from a hex string. Falls back to Address::ZERO
/// if the hex is invalid (defensive).
fn parse_treasury_address(hex: &str) -> Address {
    let bytes = hex::decode(hex).unwrap_or_else(|_| vec![0u8; 32]);
    if bytes.len() != 32 {
        return Address::ZERO;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Address::new(arr)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_crypto::sha256;
    use polay_state::MemoryStore;
    use polay_types::{AccountState, Signature, Transaction};

    const CHAIN_ID: &str = "polay-devnet-1";

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn make_config() -> ChainConfig {
        ChainConfig::default()
    }

    /// Build a signed transaction with a valid tx_hash for testing.
    fn make_signed_tx(tx: Transaction) -> SignedTransaction {
        let sig = Signature::new([0xAB; 64]);
        let signing_bytes = tx.signing_bytes();
        let mut payload = Vec::with_capacity(signing_bytes.len() + 64);
        payload.extend_from_slice(&signing_bytes);
        payload.extend_from_slice(sig.as_bytes());
        let tx_hash = sha256(&payload);
        SignedTransaction::new(tx, sig, tx_hash, vec![0u8; 32])
    }

    fn seed_account(store: &dyn StateStore, addr: Address, balance: u64) {
        StateWriter::new(store)
            .set_account(&AccountState::with_balance(addr, balance, 0))
            .unwrap();
    }

    #[test]
    fn execute_transfer_happy_path() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        let receiver = test_addr(2);
        seed_account(&store, sender, 1_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: receiver,
                amount: 500,
            },
            max_fee: 100_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);
        assert!(result.receipt.fee_used > 0);
        assert!(!result.receipt.events.is_empty());

        let view = StateView::new(&store);
        let sender_acct = view.get_account(&sender).unwrap().unwrap();
        assert_eq!(
            sender_acct.balance,
            1_000_000 - result.receipt.fee_used - 500
        );
        assert_eq!(sender_acct.nonce, 1);

        // Receiver: 500
        let recv_acct = view.get_account(&receiver).unwrap().unwrap();
        assert_eq!(recv_acct.balance, 500);
    }

    #[test]
    fn execute_transfer_insufficient_for_action() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        // Balance covers the gas fee but not the transfer amount (500_000).
        seed_account(&store, sender, 50_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 500_000,
            },
            max_fee: 100_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        // Fee is deducted, but action fails.
        assert!(!result.receipt.success);
        assert!(result.receipt.fee_used > 0);
        assert!(result.receipt.error.is_some());
    }

    #[test]
    fn execute_block_mixed_success_and_failure() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        seed_account(&store, sender, 1_000_000);

        // Good transaction.
        let tx1 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 100_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        // Bad transaction: nonexistent account, so fee deduction fails.
        let tx2 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 99,
            signer: test_addr(99), // nonexistent account
            action: TransactionAction::Transfer {
                to: test_addr(3),
                amount: 200,
            },
            max_fee: 100_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let receipts = executor.execute_block(&[tx1, tx2], &store, 5, &Address::ZERO);
        assert_eq!(receipts.len(), 2);
        assert!(receipts[0].success);
        assert!(!receipts[1].success);
        assert_eq!(receipts[0].block_height, 5);
        assert_eq!(receipts[1].block_height, 5);
    }

    #[test]
    fn execute_create_profile_via_executor() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let addr = test_addr(1);
        seed_account(&store, addr, 1_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: addr,
            action: TransactionAction::CreateProfile {
                username: "alice".into(),
                display_name: "Alice".into(),
                metadata: None,
            },
            max_fee: 100_000,
            timestamp: 2_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);

        let profile = StateView::new(&store).get_profile(&addr).unwrap().unwrap();
        assert_eq!(profile.username, "alice");
    }

    #[test]
    fn execute_register_validator_via_executor() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let addr = test_addr(1);
        seed_account(&store, addr, 1_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: addr,
            action: TransactionAction::RegisterValidator {
                commission_bps: 500,
            },
            max_fee: 200_000,
            timestamp: 3_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);

        let v = StateView::new(&store)
            .get_validator(&addr)
            .unwrap()
            .unwrap();
        assert!(v.is_active());
        assert_eq!(v.commission_bps, 500);
    }

    #[test]
    fn execute_block_sequential_nonces() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        seed_account(&store, sender, 10_000_000);

        let tx1 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::CreateProfile {
                username: "alice".into(),
                display_name: "Alice".into(),
                metadata: None,
            },
            max_fee: 200_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let tx2 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 1,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 1000,
            },
            max_fee: 200_000,
            timestamp: 1_001,
            session: None,
            sponsor: None,
        });

        let receipts = executor.execute_block(&[tx1, tx2], &store, 10, &Address::ZERO);
        assert_eq!(receipts.len(), 2);
        assert!(receipts[0].success, "tx1 should succeed");
        assert!(receipts[1].success, "tx2 should succeed");

        let view = StateView::new(&store);
        let acct = view.get_account(&sender).unwrap().unwrap();
        assert_eq!(acct.nonce, 2);
        // Balance = starting - fee1 - fee2 - transfer_amount
        let expected = 10_000_000 - receipts[0].fee_used - receipts[1].fee_used - 1000;
        assert_eq!(acct.balance, expected);
    }

    #[test]
    fn gas_used_is_recorded_in_receipt() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        seed_account(&store, sender, 10_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);
        assert!(result.receipt.gas_used > 0);
        // For a transfer: base_gas (21000) + action_gas (5000) + data_size * 16
        assert!(result.receipt.gas_used >= 26_000);
    }

    #[test]
    fn fee_too_low_rejected() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        seed_account(&store, sender, 10_000_000);

        // max_fee=1 is far too low for any transaction
        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 1,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let err = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap_err();
        assert!(matches!(err, ExecutionError::FeeTooLow));
    }

    #[test]
    fn block_gas_limit_enforcement() {
        let store = MemoryStore::new();
        let mut config = make_config();
        // Set a very low block gas limit so we can trigger it.
        config.max_block_gas = 30_000;
        let executor = Executor::new(config);

        let sender = test_addr(1);
        seed_account(&store, sender, 100_000_000);

        // Even a simple transfer needs ~26000+ gas, so with max_block_gas=30000,
        // only one should fit.
        let tx1 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let tx2 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 1,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(3),
                amount: 200,
            },
            max_fee: 500_000,
            timestamp: 1_001,
            session: None,
            sponsor: None,
        });

        let receipts = executor.execute_block(&[tx1, tx2], &store, 1, &Address::ZERO);
        assert_eq!(receipts.len(), 2);
        // First tx should succeed; second should be skipped (block gas limit).
        assert!(receipts[0].success);
        assert!(!receipts[1].success);
        assert!(receipts[1]
            .error
            .as_deref()
            .unwrap()
            .contains("block gas limit"));
    }

    #[test]
    fn estimate_gas_matches_execution() {
        let config = make_config();
        let executor = Executor::new(config.clone());

        let tx = Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: test_addr(1),
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        };

        let estimated_gas = executor.estimate_gas(&tx);

        // Execute the tx and check gas_used matches.
        let store = MemoryStore::new();
        seed_account(&store, test_addr(1), 10_000_000);
        let stx = make_signed_tx(tx);
        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert_eq!(result.receipt.gas_used, estimated_gas);
    }

    #[test]
    fn unbonding_processed_in_execute_block() {
        let store = MemoryStore::new();
        let mut config = make_config();
        config.unbonding_period_blocks = 10; // short for testing
        let executor = Executor::new(config.clone());

        let validator_addr = test_addr(1);
        let delegator_addr = test_addr(2);

        // Register validator and delegate.
        crate::modules::staking::execute_register_validator(&validator_addr, 500, &store, &config)
            .unwrap();
        seed_account(&store, delegator_addr, 100_000_000);
        crate::modules::staking::execute_delegate_stake(
            &delegator_addr,
            &validator_addr,
            50_000,
            &store,
            &config,
            100,
            100,
        )
        .unwrap();

        // Create an unbonding entry at height 100.
        crate::modules::staking::execute_undelegate_stake(
            &delegator_addr,
            &validator_addr,
            20_000,
            &store,
            100,
            &config,
        )
        .unwrap();

        let view = StateView::new(&store);
        let balance_before = view.get_account(&delegator_addr).unwrap().unwrap().balance;

        // Execute an empty block at the completion height (100 + 10 = 110).
        // The unbonding should be processed.
        let receipts = executor.execute_block(&[], &store, 110, &Address::ZERO);
        assert!(receipts.is_empty());

        let balance_after = view.get_account(&delegator_addr).unwrap().unwrap().balance;
        assert_eq!(balance_after, balance_before + 20_000);
    }

    #[test]
    fn execute_transaction_safe_returns_error_on_bad_account() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        // Transaction from a nonexistent sender -- execute_transaction_safe
        // should return an error, not panic.
        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: test_addr(99),
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 100_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor.execute_transaction_safe(&stx, &store, 1, &Address::ZERO);
        assert!(result.is_err());
    }

    #[test]
    fn execute_block_uses_panic_safe_execution() {
        // This test verifies that execute_block produces failure receipts
        // for invalid txs rather than crashing.
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        seed_account(&store, sender, 1_000_000);

        let good_tx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 100_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let bad_tx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: test_addr(99), // nonexistent
            action: TransactionAction::Transfer {
                to: test_addr(3),
                amount: 100,
            },
            max_fee: 100_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let receipts = executor.execute_block(&[good_tx, bad_tx], &store, 1, &Address::ZERO);
        assert_eq!(receipts.len(), 2);
        assert!(receipts[0].success);
        assert!(!receipts[1].success);
    }

    #[test]
    fn checked_sub_prevents_fee_underflow() {
        // Verify that fee deduction uses checked_sub so that an account whose
        // balance is smaller than the computed fee gets a clean
        // InsufficientBalance error instead of a u64 underflow / wrap-around.
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(50);
        // Seed with a tiny balance that is less than the minimum fee.
        seed_account(&store, sender, 1);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(51),
                amount: 0, // zero-value transfer; the fee itself should exceed balance
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let err = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .expect_err("should fail with InsufficientBalance");

        match err {
            ExecutionError::InsufficientBalance {
                required,
                available,
            } => {
                assert!(
                    required > available,
                    "required ({required}) must exceed available ({available})"
                );
                assert_eq!(available, 1, "available should equal the seeded balance");
            }
            other => panic!("expected InsufficientBalance, got: {other:?}"),
        }

        // Confirm the account balance was NOT wrapped around to u64::MAX.
        let acc = StateView::new(&store)
            .get_account(&sender)
            .unwrap()
            .expect("account must still exist");
        assert_eq!(
            acc.balance, 1,
            "balance must remain unchanged after failed tx"
        );
    }

    // =======================================================================
    // Session key integration tests
    // =======================================================================

    #[test]
    fn create_session_happy_path() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        seed_account(&store, sender, 10_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::CreateSession {
                session_pubkey: vec![0xAA; 32],
                permissions: polay_types::SessionPermission::All,
                expires_at: 1000,
                spending_limit: 500_000,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);
        assert!(result
            .receipt
            .events
            .iter()
            .any(|e| e.module == "session" && e.action == "session_created"));
    }

    #[test]
    fn create_session_with_specific_permissions() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        seed_account(&store, sender, 10_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::CreateSession {
                session_pubkey: vec![0xBB; 32],
                permissions: polay_types::SessionPermission::Actions(vec![
                    "transfer".into(),
                    "buy_listing".into(),
                ]),
                expires_at: 500,
                spending_limit: 100_000,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);

        // Verify the grant was stored with the right permissions.
        let session_addr = {
            use sha2::{Digest, Sha256};
            let d = Sha256::digest([0xBB; 32]);
            let mut b = [0u8; 32];
            b.copy_from_slice(&d[..32]);
            Address::new(b)
        };
        let view = StateView::new(&store);
        let grant = view.get_session(&sender, &session_addr).unwrap().unwrap();
        assert!(grant.is_action_permitted("transfer"));
        assert!(grant.is_action_permitted("buy_listing"));
        assert!(!grant.is_action_permitted("create_listing"));
    }

    #[test]
    fn revoke_session_happy_path() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        seed_account(&store, sender, 10_000_000);

        // Create session first.
        let stx1 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::CreateSession {
                session_pubkey: vec![0xCC; 32],
                permissions: polay_types::SessionPermission::All,
                expires_at: 1000,
                spending_limit: 500_000,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });
        let r1 = executor
            .execute_transaction(&stx1, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(r1.receipt.success);

        // Derive session address.
        let session_addr = {
            use sha2::{Digest, Sha256};
            let d = Sha256::digest([0xCC; 32]);
            let mut b = [0u8; 32];
            b.copy_from_slice(&d[..32]);
            Address::new(b)
        };

        // Revoke it.
        let stx2 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 1,
            signer: sender,
            action: TransactionAction::RevokeSession {
                session_address: session_addr,
            },
            max_fee: 500_000,
            timestamp: 1_001,
            session: None,
            sponsor: None,
        });
        let r2 = executor
            .execute_transaction(&stx2, &store, 2, &Address::ZERO)
            .unwrap();
        assert!(r2.receipt.success);
        assert!(r2
            .receipt
            .events
            .iter()
            .any(|e| e.module == "session" && e.action == "session_revoked"));

        // Verify it is revoked.
        let view = StateView::new(&store);
        let grant = view.get_session(&sender, &session_addr).unwrap().unwrap();
        assert!(grant.revoked);
    }

    #[test]
    fn session_spending_tracked_after_execution() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let granter = test_addr(1);
        let receiver = test_addr(2);
        seed_account(&store, granter, 10_000_000);

        // Create a session.
        let session_pubkey = vec![0xDD; 32];
        let session_addr = {
            use sha2::{Digest, Sha256};
            let d = Sha256::digest(&session_pubkey);
            let mut b = [0u8; 32];
            b.copy_from_slice(&d[..32]);
            Address::new(b)
        };

        let stx1 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: granter,
            action: TransactionAction::CreateSession {
                session_pubkey: session_pubkey.clone(),
                permissions: polay_types::SessionPermission::All,
                expires_at: 1000,
                spending_limit: 5_000_000,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });
        let r1 = executor
            .execute_transaction(&stx1, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(r1.receipt.success);

        // Now simulate a session-signed transfer. Since this is an integration
        // test via the executor (which doesn't re-verify signatures), we can
        // just set session = Some(session_addr).
        let stx2 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 1,
            signer: granter,
            action: TransactionAction::Transfer {
                to: receiver,
                amount: 100,
            },
            max_fee: 500_000,
            timestamp: 1_001,
            session: Some(session_addr),
            sponsor: None,
        });
        let r2 = executor
            .execute_transaction(&stx2, &store, 2, &Address::ZERO)
            .unwrap();
        assert!(r2.receipt.success);

        // Check the session's amount_spent was updated.
        let view = StateView::new(&store);
        let grant = view.get_session(&granter, &session_addr).unwrap().unwrap();
        assert_eq!(grant.amount_spent, r2.receipt.fee_used);
        assert!(grant.amount_spent > 0);
    }

    #[test]
    fn normal_non_session_transactions_still_work() {
        // Ensure no regression: normal transactions without session field work
        // exactly as before.
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        let receiver = test_addr(2);
        seed_account(&store, sender, 10_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: receiver,
                amount: 1000,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);

        let view = StateView::new(&store);
        let sender_acct = view.get_account(&sender).unwrap().unwrap();
        assert_eq!(
            sender_acct.balance,
            10_000_000 - result.receipt.fee_used - 1000
        );
    }

    // =======================================================================
    // Gas Sponsorship tests
    // =======================================================================

    #[test]
    fn sponsored_transfer_sponsor_pays_fee() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let signer = test_addr(1);
        let sponsor = test_addr(2);
        let receiver = test_addr(3);

        seed_account(&store, signer, 10_000); // only enough for transfer amount
        seed_account(&store, sponsor, 5_000_000); // plenty to cover fee

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer,
            action: TransactionAction::Transfer {
                to: receiver,
                amount: 500,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: Some(sponsor),
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);
        assert!(result.receipt.fee_used > 0);
        assert_eq!(result.receipt.fee_payer, sponsor);

        let view = StateView::new(&store);

        // Signer: balance = initial - transfer_amount (fee NOT deducted from signer)
        let signer_acct = view.get_account(&signer).unwrap().unwrap();
        assert_eq!(signer_acct.balance, 10_000 - 500);
        assert_eq!(signer_acct.nonce, 1); // nonce IS incremented

        // Sponsor: balance = initial - fee (fee deducted from sponsor)
        let sponsor_acct = view.get_account(&sponsor).unwrap().unwrap();
        assert_eq!(sponsor_acct.balance, 5_000_000 - result.receipt.fee_used);
        assert_eq!(sponsor_acct.nonce, 0); // nonce NOT incremented

        // Receiver got the transfer.
        let recv_acct = view.get_account(&receiver).unwrap().unwrap();
        assert_eq!(recv_acct.balance, 500);

        // Check gas_sponsored event was emitted.
        let sponsored_events: Vec<_> = result
            .receipt
            .events
            .iter()
            .filter(|e| e.action == "gas_sponsored")
            .collect();
        assert_eq!(sponsored_events.len(), 1);
        assert_eq!(
            sponsored_events[0].get_attribute("sponsor").unwrap(),
            sponsor.to_hex()
        );
    }

    #[test]
    fn sponsored_tx_failure_still_deducts_from_sponsor() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let signer = test_addr(1);
        let sponsor = test_addr(2);
        let receiver = test_addr(3);

        // Signer has zero balance — transfer will fail, but sponsor still pays fee.
        seed_account(&store, signer, 0);
        seed_account(&store, sponsor, 5_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer,
            action: TransactionAction::Transfer {
                to: receiver,
                amount: 500,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: Some(sponsor),
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(!result.receipt.success); // action failed
        assert!(result.receipt.fee_used > 0);
        assert_eq!(result.receipt.fee_payer, sponsor);

        let view = StateView::new(&store);

        // Sponsor still paid the fee even though the action failed.
        let sponsor_acct = view.get_account(&sponsor).unwrap().unwrap();
        assert_eq!(sponsor_acct.balance, 5_000_000 - result.receipt.fee_used);
    }

    #[test]
    fn normal_tx_no_sponsor_regression() {
        // Ensure the normal (non-sponsored) path still works identically.
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        let receiver = test_addr(2);
        seed_account(&store, sender, 1_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: receiver,
                amount: 500,
            },
            max_fee: 100_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);
        assert_eq!(result.receipt.fee_payer, sender);

        let view = StateView::new(&store);
        let sender_acct = view.get_account(&sender).unwrap().unwrap();
        assert_eq!(
            sender_acct.balance,
            1_000_000 - result.receipt.fee_used - 500
        );
        assert_eq!(sender_acct.nonce, 1);

        // No gas_sponsored event.
        let sponsored_events: Vec<_> = result
            .receipt
            .events
            .iter()
            .filter(|e| e.action == "gas_sponsored")
            .collect();
        assert!(sponsored_events.is_empty());
    }

    #[test]
    fn sponsored_delegate_stake_signer_pays_amount_sponsor_pays_fee() {
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let signer = test_addr(1);
        let sponsor = test_addr(2);
        let validator = test_addr(3);

        seed_account(&store, signer, 10_000);
        seed_account(&store, sponsor, 5_000_000);

        // Register the validator first.
        StateWriter::new(&store)
            .set_validator(&polay_types::ValidatorInfo::new(validator, 500))
            .unwrap();

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer,
            action: TransactionAction::DelegateStake {
                validator,
                amount: 5_000,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: Some(sponsor),
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);
        assert_eq!(result.receipt.fee_payer, sponsor);

        let view = StateView::new(&store);
        // Signer paid the stake amount but NOT the fee.
        let signer_acct = view.get_account(&signer).unwrap().unwrap();
        assert_eq!(signer_acct.balance, 10_000 - 5_000);

        // Sponsor paid the fee.
        let sponsor_acct = view.get_account(&sponsor).unwrap().unwrap();
        assert_eq!(sponsor_acct.balance, 5_000_000 - result.receipt.fee_used);
    }

    // -----------------------------------------------------------------------
    // Economics / fee-distribution tests
    // -----------------------------------------------------------------------

    /// Seed a SupplyInfo into the store so fee distribution can update it.
    fn seed_supply(store: &dyn StateStore, total_supply: u64) {
        use polay_types::SupplyInfo;
        let supply = SupplyInfo {
            total_supply,
            circulating_supply: total_supply,
            ..Default::default()
        };
        StateWriter::new(store).set_supply_info(&supply).unwrap();
    }

    #[test]
    fn fee_distribution_correct_split() {
        // Default FeeDistribution: 50% burn, 20% treasury, 30% validator.
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config.clone());

        let sender = test_addr(1);
        let proposer = test_addr(99);
        seed_account(&store, sender, 10_000_000);
        seed_supply(&store, 100_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &proposer)
            .unwrap();
        assert!(result.receipt.success);
        let fee = result.receipt.fee_used;
        assert!(fee > 0);

        let expected_burn = (fee as u128 * 5000 / 10_000) as u64;
        let expected_treasury = (fee as u128 * 2000 / 10_000) as u64;
        let expected_validator = fee - expected_burn - expected_treasury;

        // Treasury address
        let treasury_addr = parse_treasury_address(&config.treasury_address);
        let view = StateView::new(&store);

        let treasury_acct = view.get_account(&treasury_addr).unwrap().unwrap();
        assert_eq!(treasury_acct.balance, expected_treasury);

        let proposer_acct = view.get_account(&proposer).unwrap().unwrap();
        assert_eq!(proposer_acct.balance, expected_validator);

        // SupplyInfo should reflect the burn + treasury credit + fee collection.
        let supply = view.get_supply_info().unwrap().unwrap();
        assert_eq!(supply.total_burned, expected_burn);
        assert_eq!(supply.total_fees_collected, fee);
        assert_eq!(supply.total_supply, 100_000_000 - expected_burn);
        assert_eq!(supply.treasury_balance, expected_treasury);
    }

    #[test]
    fn fee_distribution_zero_fee_no_changes() {
        // When a transaction has 0 actual_fee, no distribution should happen
        // and no SupplyInfo changes should occur.
        let store = MemoryStore::new();
        let mut config = make_config();
        config.base_gas = 0;
        config.gas_per_byte = 0;
        config.min_gas_price = 0;
        let executor = Executor::new(config.clone());

        let sender = test_addr(1);
        seed_account(&store, sender, 10_000_000);
        let initial_supply: u64 = 100_000_000;
        seed_supply(&store, initial_supply);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 0,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &Address::ZERO)
            .unwrap();
        assert!(result.receipt.success);
        assert_eq!(result.receipt.fee_used, 0);

        let view = StateView::new(&store);
        let supply = view.get_supply_info().unwrap().unwrap();
        // Nothing should have changed.
        assert_eq!(supply.total_supply, initial_supply);
        assert_eq!(supply.total_burned, 0);
        assert_eq!(supply.total_fees_collected, 0);
        assert_eq!(supply.treasury_balance, 0);
    }

    #[test]
    fn fee_distribution_rounding_no_dust_lost() {
        // With a fee that doesn't divide evenly by bps, the validator gets
        // the remainder so no dust is lost.
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config.clone());

        let sender = test_addr(1);
        let proposer = test_addr(99);
        seed_account(&store, sender, 10_000_000);
        seed_supply(&store, 100_000_000);

        let stx = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 1,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });

        let result = executor
            .execute_transaction(&stx, &store, 1, &proposer)
            .unwrap();
        assert!(result.receipt.success);
        let fee = result.receipt.fee_used;

        let burn = (fee as u128 * 5000 / 10_000) as u64;
        let treasury = (fee as u128 * 2000 / 10_000) as u64;
        let validator = fee - burn - treasury;

        // The three parts must exactly reconstruct the fee.
        assert_eq!(burn + treasury + validator, fee);

        let treasury_addr = parse_treasury_address(&config.treasury_address);
        let view = StateView::new(&store);
        let t_bal = view.get_account(&treasury_addr).unwrap().unwrap().balance;
        let v_bal = view.get_account(&proposer).unwrap().unwrap().balance;
        assert_eq!(t_bal + v_bal + burn, fee);
    }

    #[test]
    fn supply_tracking_burn_increases_total_burned() {
        // Execute multiple transactions and verify total_burned accumulates.
        let store = MemoryStore::new();
        let config = make_config();
        let executor = Executor::new(config);

        let sender = test_addr(1);
        let proposer = test_addr(99);
        seed_account(&store, sender, 50_000_000);
        seed_supply(&store, 200_000_000);

        // First transaction.
        let stx1 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 0,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(2),
                amount: 100,
            },
            max_fee: 500_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        });
        let r1 = executor
            .execute_transaction(&stx1, &store, 1, &proposer)
            .unwrap();
        assert!(r1.receipt.success);
        let burn1 = (r1.receipt.fee_used as u128 * 5000 / 10_000) as u64;

        // Second transaction.
        let stx2 = make_signed_tx(Transaction {
            chain_id: CHAIN_ID.into(),
            nonce: 1,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(3),
                amount: 200,
            },
            max_fee: 500_000,
            timestamp: 1_001,
            session: None,
            sponsor: None,
        });
        let r2 = executor
            .execute_transaction(&stx2, &store, 2, &proposer)
            .unwrap();
        assert!(r2.receipt.success);
        let burn2 = (r2.receipt.fee_used as u128 * 5000 / 10_000) as u64;

        let view = StateView::new(&store);
        let supply = view.get_supply_info().unwrap().unwrap();
        assert_eq!(supply.total_burned, burn1 + burn2);
        assert_eq!(supply.total_supply, 200_000_000 - burn1 - burn2);
        assert_eq!(
            supply.total_fees_collected,
            r1.receipt.fee_used + r2.receipt.fee_used
        );
    }
}
