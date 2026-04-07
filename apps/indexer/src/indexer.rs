//! Core indexing loop: polls the POLAY RPC for new blocks and indexes their
//! contents into PostgreSQL.

use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{debug, error, info};

use crate::db::Database;
use crate::rpc_client::RpcClient;
use crate::types::{action_type_from_value, BlockData, SignedTransactionData};

/// The indexer orchestrates the poll-index loop.
pub struct Indexer {
    rpc: RpcClient,
    db: Database,
    poll_interval: Duration,
}

impl Indexer {
    /// Create a new indexer.
    pub fn new(rpc: RpcClient, db: Database, poll_interval: Duration) -> Self {
        Self {
            rpc,
            db,
            poll_interval,
        }
    }

    /// Run the indexer loop forever.
    ///
    /// On each iteration the indexer:
    /// 1. Reads the last indexed height from the database.
    /// 2. Queries the chain for its current height.
    /// 3. Fetches and indexes any new blocks.
    /// 4. Sleeps for `poll_interval` before repeating.
    pub async fn run(&self) -> Result<()> {
        info!("starting indexer loop");

        loop {
            match self.tick().await {
                Ok(()) => {}
                Err(e) => {
                    error!("indexer tick failed: {e:#}");
                    // Back off a bit on transient errors rather than
                    // hammering the RPC / DB.
                    tokio::time::sleep(self.poll_interval * 2).await;
                    continue;
                }
            }

            tokio::time::sleep(self.poll_interval).await;
        }
    }

    /// A single tick of the indexer loop.
    async fn tick(&self) -> Result<()> {
        let indexed_height = self.db.get_indexed_height().await?;
        let chain_info = self.rpc.get_chain_info().await?;

        if chain_info.height <= indexed_height && indexed_height > 0 {
            debug!(
                indexed_height,
                chain_height = chain_info.height,
                "chain is up to date"
            );
            return Ok(());
        }

        let start = if indexed_height == 0 { 0 } else { indexed_height + 1 };
        let end = chain_info.height;

        if end >= start {
            info!(
                from = start,
                to = end,
                blocks = end - start + 1,
                "indexing new blocks"
            );
        }

        for height in start..=end {
            self.index_block(height).await.with_context(|| {
                format!("failed to index block at height {height}")
            })?;
        }

        Ok(())
    }

    /// Fetch a single block from the RPC and index all its data.
    async fn index_block(&self, height: u64) -> Result<()> {
        let block = self
            .rpc
            .get_block(height)
            .await?
            .ok_or_else(|| anyhow::anyhow!("block {height} not found on chain"))?;

        debug!(height, tx_count = block.tx_count, "indexing block");

        // 1. Insert the block header.
        self.db.insert_block(&block).await?;

        // 2. Track the proposer as a validator (increment blocks_produced).
        self.db.increment_blocks_produced(&block.proposer).await.ok();

        // 3. Process every transaction.
        for (tx_index, tx) in block.transactions.iter().enumerate() {
            self.index_transaction(tx, &block, tx_index as i32).await?;
        }

        // 4. Persist the indexed height.
        self.db.set_indexed_height(height).await?;

        if height % 100 == 0 || block.tx_count > 0 {
            info!(
                height,
                tx_count = block.tx_count,
                "indexed block"
            );
        }

        Ok(())
    }

    /// Index a single transaction: insert the tx row, touch the signer's
    /// account, and dispatch action-specific logic.
    async fn index_transaction(
        &self,
        tx: &SignedTransactionData,
        block: &BlockData,
        tx_index: i32,
    ) -> Result<()> {
        // Insert the transaction row.
        self.db
            .insert_transaction(tx, block.height, tx_index, block.timestamp)
            .await?;

        // Touch the signer's account.
        self.db
            .upsert_account(&tx.transaction.signer, block.height)
            .await?;

        // Try to refresh the signer's balance from the RPC. This is
        // best-effort -- the indexer keeps going even if RPC is flaky.
        if let Ok(Some(acct)) = self.rpc.get_account(&tx.transaction.signer).await {
            self.db
                .update_account_balance(&acct.address, acct.balance, acct.nonce)
                .await
                .ok();
        }

        // Dispatch on the action type for domain-specific indexing.
        let action_type = action_type_from_value(&tx.transaction.action);
        self.dispatch_action(&action_type, tx, block).await?;

        Ok(())
    }

    /// Route an action to the appropriate domain-specific handler.
    async fn dispatch_action(
        &self,
        action_type: &str,
        tx: &SignedTransactionData,
        block: &BlockData,
    ) -> Result<()> {
        let action = &tx.transaction.action;
        let height = block.height;
        let signer = &tx.transaction.signer;

        match action_type {
            "transfer" => {
                self.handle_transfer(action, signer, height).await?;
            }
            "create_asset_class" => {
                self.handle_create_asset_class(action, signer, height).await?;
            }
            "mint_asset" => {
                self.handle_mint_asset(action, height).await?;
            }
            "transfer_asset" => {
                self.handle_transfer_asset(action, signer, height).await?;
            }
            "burn_asset" => {
                self.handle_burn_asset(action, signer, height).await?;
            }
            "create_listing" => {
                self.handle_create_listing(action, signer, height).await?;
            }
            "cancel_listing" => {
                self.handle_cancel_listing(action, height).await?;
            }
            "buy_listing" => {
                self.handle_buy_listing(action, signer, height).await?;
            }
            "create_profile" => {
                self.handle_create_profile(action, signer, height).await?;
            }
            "add_achievement" => {
                self.handle_add_achievement(action, height).await?;
            }
            "update_reputation" => {
                self.handle_update_reputation(action).await?;
            }
            "register_validator" => {
                self.handle_register_validator(action, signer, height).await?;
            }
            "delegate_stake" => {
                self.handle_delegate_stake(action, signer).await?;
            }
            "undelegate_stake" => {
                self.handle_undelegate_stake(action, signer).await?;
            }
            "submit_match_result" => {
                self.handle_submit_match_result(action, signer, height).await?;
            }
            "register_attestor" => {
                self.handle_register_attestor(action, signer, height).await?;
            }
            "distribute_reward" => {
                self.handle_distribute_reward(action, height).await?;
            }
            "submit_proposal" => {
                self.handle_submit_proposal(action, signer, height).await?;
            }
            "vote_proposal" => {
                self.handle_vote_proposal(action, signer, height).await?;
            }
            "execute_proposal" => {
                self.handle_execute_proposal(action).await?;
            }
            "create_session" => {
                self.handle_create_session(action, signer, height).await?;
            }
            "revoke_session" => {
                self.handle_revoke_session(action, signer).await?;
            }
            other => {
                debug!(action_type = other, "unhandled action type");
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Action handlers
    // -----------------------------------------------------------------------

    async fn handle_transfer(
        &self,
        action: &serde_json::Value,
        _signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("Transfer") {
            if let Some(to) = inner.get("to").and_then(|v| v.as_str()) {
                self.db.upsert_account(to, height).await.ok();
                // Refresh recipient balance.
                if let Ok(Some(acct)) = self.rpc.get_account(to).await {
                    self.db
                        .update_account_balance(&acct.address, acct.balance, acct.nonce)
                        .await
                        .ok();
                }
            }
        }
        Ok(())
    }

    async fn handle_create_asset_class(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("CreateAssetClass") {
            let name = inner.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let symbol = inner.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
            let asset_type = inner
                .get("asset_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Fungible");
            let max_supply = inner
                .get("max_supply")
                .and_then(|v| v.as_u64())
                .map(|v| v as i64);
            let metadata_uri = inner.get("metadata_uri").and_then(|v| v.as_str());

            // The actual asset class ID is generated by the execution layer.
            // We use the tx_hash as an approximation since the real ID is
            // returned only via events/receipts. A production indexer would
            // use receipt events.
            let id = format!("pending-{signer}-{height}");

            self.db
                .insert_asset_class(
                    &id,
                    name,
                    symbol,
                    asset_type,
                    max_supply,
                    signer,
                    metadata_uri,
                    height,
                )
                .await?;
        }
        Ok(())
    }

    async fn handle_mint_asset(
        &self,
        action: &serde_json::Value,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("MintAsset") {
            let asset_class_id = inner
                .get("asset_class_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let to = inner.get("to").and_then(|v| v.as_str()).unwrap_or("");
            let amount = inner.get("amount").and_then(|v| v.as_u64()).unwrap_or(0);

            self.db
                .credit_asset_balance(asset_class_id, to, amount as i64)
                .await?;
            self.db.upsert_account(to, height).await.ok();
        }
        Ok(())
    }

    async fn handle_transfer_asset(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("TransferAsset") {
            let asset_class_id = inner
                .get("asset_class_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let to = inner.get("to").and_then(|v| v.as_str()).unwrap_or("");
            let amount = inner.get("amount").and_then(|v| v.as_u64()).unwrap_or(0);

            self.db
                .debit_asset_balance(asset_class_id, signer, amount as i64)
                .await?;
            self.db
                .credit_asset_balance(asset_class_id, to, amount as i64)
                .await?;
            self.db.upsert_account(to, height).await.ok();
        }
        Ok(())
    }

    async fn handle_burn_asset(
        &self,
        action: &serde_json::Value,
        signer: &str,
        _height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("BurnAsset") {
            let asset_class_id = inner
                .get("asset_class_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let amount = inner.get("amount").and_then(|v| v.as_u64()).unwrap_or(0);

            self.db
                .debit_asset_balance(asset_class_id, signer, amount as i64)
                .await?;
        }
        Ok(())
    }

    async fn handle_create_listing(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("CreateListing") {
            let asset_class_id = inner
                .get("asset_class_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let amount = inner.get("amount").and_then(|v| v.as_u64()).unwrap_or(0);
            let price_per_unit = inner
                .get("price_per_unit")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let currency = inner
                .get("currency")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Listing ID is generated by execution layer; use a placeholder.
            let id = format!("pending-listing-{signer}-{height}");

            self.db
                .insert_listing(
                    &id,
                    signer,
                    asset_class_id,
                    amount as i64,
                    price_per_unit as i64,
                    currency,
                    0, // royalty_bps set by execution layer
                    height,
                )
                .await?;
        }
        Ok(())
    }

    async fn handle_cancel_listing(
        &self,
        action: &serde_json::Value,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("CancelListing") {
            if let Some(listing_id) = inner.get("listing_id").and_then(|v| v.as_str()) {
                self.db.mark_listing_cancelled(listing_id, height).await?;
            }
        }
        Ok(())
    }

    async fn handle_buy_listing(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("BuyListing") {
            if let Some(listing_id) = inner.get("listing_id").and_then(|v| v.as_str()) {
                self.db.mark_listing_sold(listing_id, signer, height).await?;
            }
        }
        Ok(())
    }

    async fn handle_create_profile(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("CreateProfile") {
            let username = inner
                .get("username")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let display_name = inner
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let metadata = inner.get("metadata").and_then(|v| v.as_str());

            self.db
                .insert_profile(signer, username, display_name, metadata, height)
                .await?;
        }
        Ok(())
    }

    async fn handle_add_achievement(
        &self,
        action: &serde_json::Value,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("AddAchievement") {
            let player = inner
                .get("player")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let achievement_id = inner
                .get("achievement_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name = inner.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let metadata = inner
                .get("metadata")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");

            self.db
                .insert_achievement(player, achievement_id, name, metadata, true, height)
                .await?;
        }
        Ok(())
    }

    async fn handle_update_reputation(
        &self,
        action: &serde_json::Value,
    ) -> Result<()> {
        if let Some(inner) = action.get("UpdateReputation") {
            let player = inner
                .get("player")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let delta = inner.get("delta").and_then(|v| v.as_i64()).unwrap_or(0);

            self.db.update_reputation(player, delta).await?;
        }
        Ok(())
    }

    async fn handle_register_validator(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("RegisterValidator") {
            let commission_bps = inner
                .get("commission_bps")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as i32;

            self.db
                .upsert_validator(signer, commission_bps, height)
                .await?;
        }
        Ok(())
    }

    async fn handle_delegate_stake(
        &self,
        action: &serde_json::Value,
        signer: &str,
    ) -> Result<()> {
        if let Some(inner) = action.get("DelegateStake") {
            let validator = inner
                .get("validator")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let amount = inner.get("amount").and_then(|v| v.as_u64()).unwrap_or(0);

            self.db
                .add_delegation(signer, validator, amount as i64)
                .await?;
        }
        Ok(())
    }

    async fn handle_undelegate_stake(
        &self,
        action: &serde_json::Value,
        signer: &str,
    ) -> Result<()> {
        if let Some(inner) = action.get("UndelegateStake") {
            let validator = inner
                .get("validator")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let amount = inner.get("amount").and_then(|v| v.as_u64()).unwrap_or(0);

            self.db
                .remove_delegation(signer, validator, amount as i64)
                .await?;
        }
        Ok(())
    }

    async fn handle_submit_match_result(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("SubmitMatchResult") {
            if let Some(mr) = inner.get("match_result") {
                let match_id = mr
                    .get("match_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let game_id = mr
                    .get("game_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let timestamp = mr.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
                let players: Vec<String> = mr
                    .get("players")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let winners: Vec<String> = mr
                    .get("winners")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let reward_pool = mr
                    .get("reward_pool")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                self.db
                    .insert_match_result(
                        match_id,
                        game_id,
                        timestamp,
                        &players,
                        &winners,
                        reward_pool,
                        signer,
                        height,
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn handle_register_attestor(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("RegisterAttestor") {
            let game_id = inner
                .get("game_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let endpoint = inner
                .get("endpoint")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let metadata = inner
                .get("metadata")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            self.db
                .insert_attestor(signer, game_id, endpoint, metadata, height as i64)
                .await?;
        }
        Ok(())
    }

    async fn handle_distribute_reward(
        &self,
        action: &serde_json::Value,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("DistributeReward") {
            let match_id = inner
                .get("match_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if let Some(rewards) = inner.get("rewards").and_then(|v| v.as_array()) {
                for reward in rewards {
                    // Rewards are encoded as [player, amount] tuples.
                    if let Some(arr) = reward.as_array() {
                        let player = arr.get(0).and_then(|v| v.as_str()).unwrap_or("");
                        let amount = arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0);

                        self.db
                            .insert_reward_distribution(
                                match_id,
                                player,
                                amount as i64,
                                height as i64,
                            )
                            .await?;

                        // Touch the recipient's account so the indexer knows
                        // about them even if they have never signed a tx.
                        self.db.upsert_account(player, height).await.ok();
                    }
                }

                self.db.update_match_settled(match_id, true).await?;
            }
        }
        Ok(())
    }

    async fn handle_submit_proposal(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("SubmitProposal") {
            let title = inner
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let description = inner
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let deposit = inner
                .get("deposit")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let action_value = inner
                .get("action")
                .unwrap_or(&serde_json::Value::Null);

            // Extract the discriminant from the action envelope (same
            // pattern as TransactionAction: {"ParamChange": {...}}).
            let action_type = match action_value {
                serde_json::Value::Object(map) => {
                    map.keys().next().cloned().unwrap_or_else(|| "unknown".to_string())
                }
                serde_json::Value::String(s) => s.clone(),
                _ => "unknown".to_string(),
            };

            // Generate a deterministic proposal ID. A production indexer
            // would extract the real ID from transaction receipts/events.
            let proposal_id = format!("{}_{}", signer, height);

            // Default voting window: 14400 blocks (~8 hours at 2s blocks).
            let voting_end = height as i64 + 14400;

            self.db
                .insert_proposal(
                    &proposal_id,
                    signer,
                    title,
                    description,
                    &action_type,
                    action_value,
                    deposit as i64,
                    height as i64,
                    voting_end,
                    height as i64,
                )
                .await?;
        }
        Ok(())
    }

    async fn handle_vote_proposal(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("VoteProposal") {
            let proposal_id = inner
                .get("proposal_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let option = inner
                .get("option")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            self.db
                .insert_vote(proposal_id, signer, option, 0, height as i64)
                .await?;
        }
        Ok(())
    }

    async fn handle_execute_proposal(
        &self,
        action: &serde_json::Value,
    ) -> Result<()> {
        if let Some(inner) = action.get("ExecuteProposal") {
            let proposal_id = inner
                .get("proposal_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Without receipt data we cannot determine pass/fail, so we
            // optimistically mark the proposal as Executed.
            self.db
                .update_proposal_status(proposal_id, "Executed")
                .await?;
        }
        Ok(())
    }

    async fn handle_create_session(
        &self,
        action: &serde_json::Value,
        signer: &str,
        height: u64,
    ) -> Result<()> {
        if let Some(inner) = action.get("CreateSession") {
            let session_pubkey = inner
                .get("session_pubkey")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let permissions = inner
                .get("permissions")
                .unwrap_or(&serde_json::Value::Null);
            let expires_at = inner
                .get("expires_at")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let spending_limit = inner
                .get("spending_limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            // The session address is ideally derived from the pubkey via
            // SHA-256. If the transaction includes it directly, prefer that;
            // otherwise fall back to the raw pubkey as a placeholder.
            let session_address = inner
                .get("session_address")
                .and_then(|v| v.as_str())
                .unwrap_or(session_pubkey);

            self.db
                .insert_session(
                    signer,
                    session_address,
                    session_pubkey,
                    permissions,
                    expires_at as i64,
                    spending_limit as i64,
                    height as i64,
                )
                .await?;
        }
        Ok(())
    }

    async fn handle_revoke_session(
        &self,
        action: &serde_json::Value,
        signer: &str,
    ) -> Result<()> {
        if let Some(inner) = action.get("RevokeSession") {
            let session_address = inner
                .get("session_address")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            self.db
                .revoke_session(signer, session_address)
                .await?;
        }
        Ok(())
    }
}
