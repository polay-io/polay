//! PostgreSQL database layer for the POLAY indexer.
//!
//! Uses `tokio-postgres` directly with hand-written SQL. No ORM, no
//! compile-time checked queries -- just straightforward INSERT / UPSERT
//! statements.

use anyhow::{Context, Result};
use tokio_postgres::{Client, NoTls};
use tracing::info;

use crate::types::{action_type_from_value, BlockData, EventData, SignedTransactionData};

/// Wraps a `tokio_postgres::Client` and provides typed helpers for every
/// indexed table.
pub struct Database {
    client: Client,
}

impl Database {
    /// Connect to PostgreSQL using the provided connection string.
    ///
    /// The connection string should be a standard libpq URI, e.g.
    /// `postgres://user:pass@host/dbname`.
    pub async fn connect(url: &str) -> Result<Self> {
        let (client, connection) =
            tokio_postgres::connect(url, NoTls)
                .await
                .context("failed to connect to PostgreSQL")?;

        // Spawn the connection handler -- it must run in the background for
        // the client to work.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!("PostgreSQL connection error: {e}");
            }
        });

        info!("connected to PostgreSQL");
        Ok(Self { client })
    }

    /// Run all SQL migration files embedded at compile time.
    pub async fn run_migrations(&self) -> Result<()> {
        let migrations: &[(&str, &str)] = &[
            ("001_initial", include_str!("../migrations/001_initial.sql")),
            ("002_complete_coverage", include_str!("../migrations/002_complete_coverage.sql")),
        ];

        for (name, sql) in migrations {
            self.client
                .batch_execute(sql)
                .await
                .with_context(|| format!("failed to run migration {name}"))?;
            info!(migration = name, "migration applied");
        }

        info!("all migrations applied successfully");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Indexer state bookkeeping
    // -----------------------------------------------------------------------

    /// Read the last successfully indexed block height.
    ///
    /// Returns 0 if no blocks have been indexed yet (which means the indexer
    /// should start from block 0).
    pub async fn get_indexed_height(&self) -> Result<u64> {
        let row = self
            .client
            .query_opt(
                "SELECT value FROM indexer_state WHERE key = 'indexed_height'",
                &[],
            )
            .await
            .context("failed to query indexed_height")?;

        match row {
            Some(r) => {
                let val: String = r.get(0);
                val.parse::<u64>()
                    .context("indexed_height is not a valid u64")
            }
            None => Ok(0),
        }
    }

    /// Persist the last indexed height.
    pub async fn set_indexed_height(&self, height: u64) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO indexer_state (key, value, updated_at)
                 VALUES ('indexed_height', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()",
                &[&height.to_string()],
            )
            .await
            .context("failed to set indexed_height")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Blocks
    // -----------------------------------------------------------------------

    /// Insert a block header row.
    pub async fn insert_block(&self, block: &BlockData) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO blocks (height, hash, parent_hash, state_root,
                    transactions_root, proposer, chain_id, timestamp, tx_count)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (height) DO NOTHING",
                &[
                    &(block.height as i64),
                    &block.hash,
                    &block.parent_hash,
                    &block.state_root,
                    &block.transactions_root,
                    &block.proposer,
                    &block.chain_id,
                    &(block.timestamp as i64),
                    &(block.tx_count as i32),
                ],
            )
            .await
            .context("failed to insert block")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Transactions
    // -----------------------------------------------------------------------

    /// Insert a transaction row.
    pub async fn insert_transaction(
        &self,
        tx: &SignedTransactionData,
        block_height: u64,
        tx_index: i32,
        block_timestamp: u64,
    ) -> Result<()> {
        let action_type = action_type_from_value(&tx.transaction.action);
        let action_data = &tx.transaction.action;

        self.client
            .execute(
                "INSERT INTO transactions (tx_hash, block_height, signer, action_type,
                    action_data, nonce, max_fee, gas_used, success, error,
                    timestamp, tx_index)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT (tx_hash) DO NOTHING",
                &[
                    &tx.tx_hash,
                    &(block_height as i64),
                    &tx.transaction.signer,
                    &action_type,
                    &serde_json::to_value(action_data)
                        .unwrap_or(serde_json::Value::Null),
                    &(tx.transaction.nonce as i64),
                    &(tx.transaction.max_fee as i64),
                    &0i64,    // gas_used -- not available without receipt
                    &true,    // success -- assume true; would need receipt for actual status
                    &None::<String>, // error
                    &(block_timestamp as i64),
                    &tx_index,
                ],
            )
            .await
            .context("failed to insert transaction")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Accounts
    // -----------------------------------------------------------------------

    /// Upsert an account row. Increments tx_count by 1 on each call.
    pub async fn upsert_account(
        &self,
        address: &str,
        height: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO accounts (address, first_seen_height, last_active_height, tx_count, updated_at)
                 VALUES ($1, $2, $2, 1, NOW())
                 ON CONFLICT (address) DO UPDATE SET
                    last_active_height = $2,
                    tx_count = accounts.tx_count + 1,
                    updated_at = NOW()",
                &[&address, &(height as i64)],
            )
            .await
            .context("failed to upsert account")?;
        Ok(())
    }

    /// Update account balance and nonce from RPC data.
    pub async fn update_account_balance(
        &self,
        address: &str,
        balance: u64,
        nonce: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "UPDATE accounts SET balance = $2, nonce = $3, updated_at = NOW()
                 WHERE address = $1",
                &[&address, &(balance as i64), &(nonce as i64)],
            )
            .await
            .context("failed to update account balance")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Asset classes
    // -----------------------------------------------------------------------

    /// Insert a new asset class.
    pub async fn insert_asset_class(
        &self,
        id: &str,
        name: &str,
        symbol: &str,
        asset_type: &str,
        max_supply: Option<i64>,
        creator: &str,
        metadata_uri: Option<&str>,
        height: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO asset_classes (id, name, symbol, asset_type, max_supply,
                    creator, metadata_uri, created_at_height)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (id) DO NOTHING",
                &[
                    &id,
                    &name,
                    &symbol,
                    &asset_type,
                    &max_supply,
                    &creator,
                    &metadata_uri,
                    &(height as i64),
                ],
            )
            .await
            .context("failed to insert asset class")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Asset balances
    // -----------------------------------------------------------------------

    /// Credit an asset balance (used for mints and incoming transfers).
    pub async fn credit_asset_balance(
        &self,
        asset_class_id: &str,
        owner: &str,
        amount: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO asset_balances (asset_class_id, owner, amount, updated_at)
                 VALUES ($1, $2, $3, NOW())
                 ON CONFLICT (asset_class_id, owner) DO UPDATE SET
                    amount = asset_balances.amount + $3,
                    updated_at = NOW()",
                &[&asset_class_id, &owner, &amount],
            )
            .await
            .context("failed to credit asset balance")?;
        Ok(())
    }

    /// Debit an asset balance (used for burns and outgoing transfers).
    pub async fn debit_asset_balance(
        &self,
        asset_class_id: &str,
        owner: &str,
        amount: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "UPDATE asset_balances SET amount = amount - $3, updated_at = NOW()
                 WHERE asset_class_id = $1 AND owner = $2",
                &[&asset_class_id, &owner, &amount],
            )
            .await
            .context("failed to debit asset balance")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Listings
    // -----------------------------------------------------------------------

    /// Insert a new marketplace listing.
    pub async fn insert_listing(
        &self,
        id: &str,
        seller: &str,
        asset_class_id: &str,
        amount: i64,
        price_per_unit: i64,
        currency: &str,
        royalty_bps: i32,
        height: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO listings (id, seller, asset_class_id, amount, price_per_unit,
                    currency, royalty_bps, created_at_height)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (id) DO NOTHING",
                &[
                    &id,
                    &seller,
                    &asset_class_id,
                    &amount,
                    &price_per_unit,
                    &currency,
                    &royalty_bps,
                    &(height as i64),
                ],
            )
            .await
            .context("failed to insert listing")?;
        Ok(())
    }

    /// Mark a listing as sold.
    pub async fn mark_listing_sold(
        &self,
        listing_id: &str,
        buyer: &str,
        height: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "UPDATE listings SET status = 'Sold', buyer = $2,
                    updated_at_height = $3, updated_at = NOW()
                 WHERE id = $1",
                &[&listing_id, &buyer, &(height as i64)],
            )
            .await
            .context("failed to mark listing sold")?;
        Ok(())
    }

    /// Mark a listing as cancelled.
    pub async fn mark_listing_cancelled(
        &self,
        listing_id: &str,
        height: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "UPDATE listings SET status = 'Cancelled',
                    updated_at_height = $2, updated_at = NOW()
                 WHERE id = $1",
                &[&listing_id, &(height as i64)],
            )
            .await
            .context("failed to mark listing cancelled")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Player profiles
    // -----------------------------------------------------------------------

    /// Insert a new player profile.
    pub async fn insert_profile(
        &self,
        address: &str,
        username: &str,
        display_name: &str,
        metadata: Option<&str>,
        height: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO player_profiles (address, username, display_name, metadata,
                    created_at_height)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (address) DO NOTHING",
                &[
                    &address,
                    &username,
                    &display_name,
                    &metadata,
                    &(height as i64),
                ],
            )
            .await
            .context("failed to insert profile")?;
        Ok(())
    }

    /// Update a player's reputation.
    pub async fn update_reputation(
        &self,
        address: &str,
        delta: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "UPDATE player_profiles SET reputation = reputation + $2
                 WHERE address = $1",
                &[&address, &delta],
            )
            .await
            .context("failed to update reputation")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Achievements
    // -----------------------------------------------------------------------

    /// Insert an achievement and bump the player's achievement count.
    pub async fn insert_achievement(
        &self,
        player: &str,
        achievement_id: &str,
        name: &str,
        metadata: &str,
        soulbound: bool,
        height: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO achievements (id, player, name, metadata, soulbound, awarded_at_height)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (player, id) DO NOTHING",
                &[
                    &achievement_id,
                    &player,
                    &name,
                    &metadata,
                    &soulbound,
                    &(height as i64),
                ],
            )
            .await
            .context("failed to insert achievement")?;

        self.client
            .execute(
                "UPDATE player_profiles SET achievement_count = achievement_count + 1
                 WHERE address = $1",
                &[&player],
            )
            .await
            .context("failed to update achievement count")?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Validators
    // -----------------------------------------------------------------------

    /// Insert or update a validator.
    pub async fn upsert_validator(
        &self,
        address: &str,
        commission_bps: i32,
        height: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO validators (address, commission_bps, created_at_height, updated_at)
                 VALUES ($1, $2, $3, NOW())
                 ON CONFLICT (address) DO UPDATE SET
                    commission_bps = $2,
                    updated_at = NOW()",
                &[&address, &commission_bps, &(height as i64)],
            )
            .await
            .context("failed to upsert validator")?;
        Ok(())
    }

    /// Increment the blocks_produced counter for the proposer.
    pub async fn increment_blocks_produced(&self, address: &str) -> Result<()> {
        self.client
            .execute(
                "UPDATE validators SET blocks_produced = blocks_produced + 1, updated_at = NOW()
                 WHERE address = $1",
                &[&address],
            )
            .await
            .context("failed to increment blocks_produced")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Delegations
    // -----------------------------------------------------------------------

    /// Add delegation stake.
    pub async fn add_delegation(
        &self,
        delegator: &str,
        validator: &str,
        amount: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO delegations (delegator, validator, amount, updated_at)
                 VALUES ($1, $2, $3, NOW())
                 ON CONFLICT (delegator, validator) DO UPDATE SET
                    amount = delegations.amount + $3,
                    updated_at = NOW()",
                &[&delegator, &validator, &amount],
            )
            .await
            .context("failed to add delegation")?;
        Ok(())
    }

    /// Remove delegation stake.
    pub async fn remove_delegation(
        &self,
        delegator: &str,
        validator: &str,
        amount: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "UPDATE delegations SET amount = amount - $3, updated_at = NOW()
                 WHERE delegator = $1 AND validator = $2",
                &[&delegator, &validator, &amount],
            )
            .await
            .context("failed to remove delegation")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Attestors
    // -----------------------------------------------------------------------

    /// Insert a new attestor registration.
    pub async fn insert_attestor(
        &self,
        address: &str,
        game_id: &str,
        endpoint: &str,
        metadata: &str,
        height: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO attestors (address, game_id, endpoint, metadata, registered_at_height)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (address) DO UPDATE SET
                    game_id = $2,
                    endpoint = $3,
                    metadata = $4,
                    status = 'Active'",
                &[&address, &game_id, &endpoint, &metadata, &height],
            )
            .await
            .context("failed to insert attestor")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Reward distributions
    // -----------------------------------------------------------------------

    /// Record an individual reward distribution from a match settlement.
    pub async fn insert_reward_distribution(
        &self,
        match_id: &str,
        player: &str,
        amount: i64,
        height: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO reward_distributions (match_id, player, amount, distributed_at_height)
                 VALUES ($1, $2, $3, $4)",
                &[&match_id, &player, &amount, &height],
            )
            .await
            .context("failed to insert reward distribution")?;
        Ok(())
    }

    /// Mark a match as settled.
    pub async fn update_match_settled(
        &self,
        match_id: &str,
        settled: bool,
    ) -> Result<()> {
        self.client
            .execute(
                "UPDATE match_results SET settled = $2 WHERE match_id = $1",
                &[&match_id, &settled],
            )
            .await
            .context("failed to update match settled status")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Governance proposals
    // -----------------------------------------------------------------------

    /// Insert a new governance proposal.
    pub async fn insert_proposal(
        &self,
        id: &str,
        proposer: &str,
        title: &str,
        description: &str,
        action_type: &str,
        action_data: &serde_json::Value,
        deposit: i64,
        voting_start: i64,
        voting_end: i64,
        height: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO proposals (id, proposer, title, description, action_type,
                    action_data, deposit, voting_start_height, voting_end_height,
                    created_at_height)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT (id) DO NOTHING",
                &[
                    &id,
                    &proposer,
                    &title,
                    &description,
                    &action_type,
                    &action_data,
                    &deposit,
                    &voting_start,
                    &voting_end,
                    &height,
                ],
            )
            .await
            .context("failed to insert proposal")?;
        Ok(())
    }

    /// Insert or update a vote on a governance proposal.
    pub async fn insert_vote(
        &self,
        proposal_id: &str,
        voter: &str,
        option: &str,
        weight: i64,
        height: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO votes (proposal_id, voter, option, weight, height)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (proposal_id, voter) DO UPDATE SET
                    option = $3,
                    weight = $4,
                    height = $5",
                &[&proposal_id, &voter, &option, &weight, &height],
            )
            .await
            .context("failed to insert vote")?;

        // Update the proposal tallies by re-aggregating from votes.
        self.recompute_proposal_tallies(proposal_id).await?;

        Ok(())
    }

    /// Update the status of a governance proposal.
    pub async fn update_proposal_status(
        &self,
        proposal_id: &str,
        status: &str,
    ) -> Result<()> {
        self.client
            .execute(
                "UPDATE proposals SET status = $2, updated_at = NOW()
                 WHERE id = $1",
                &[&proposal_id, &status],
            )
            .await
            .context("failed to update proposal status")?;
        Ok(())
    }

    /// Recompute yes/no/abstain tallies for a proposal from the votes table.
    async fn recompute_proposal_tallies(&self, proposal_id: &str) -> Result<()> {
        self.client
            .execute(
                "UPDATE proposals SET
                    yes_votes = COALESCE((SELECT COUNT(*) FROM votes WHERE proposal_id = $1 AND option = 'Yes'), 0),
                    no_votes = COALESCE((SELECT COUNT(*) FROM votes WHERE proposal_id = $1 AND option = 'No'), 0),
                    abstain_votes = COALESCE((SELECT COUNT(*) FROM votes WHERE proposal_id = $1 AND option = 'Abstain'), 0),
                    updated_at = NOW()
                 WHERE id = $1",
                &[&proposal_id],
            )
            .await
            .context("failed to recompute proposal tallies")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Session keys
    // -----------------------------------------------------------------------

    /// Insert a new session key grant.
    pub async fn insert_session(
        &self,
        granter: &str,
        session_address: &str,
        session_pubkey: &str,
        permissions: &serde_json::Value,
        expires_at: i64,
        spending_limit: i64,
        height: i64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO sessions (granter, session_address, session_pubkey,
                    permissions, expires_at, spending_limit, created_at_height)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (granter, session_address) DO UPDATE SET
                    session_pubkey = $3,
                    permissions = $4,
                    expires_at = $5,
                    spending_limit = $6,
                    revoked = false,
                    updated_at = NOW()",
                &[
                    &granter,
                    &session_address,
                    &session_pubkey,
                    &permissions,
                    &expires_at,
                    &spending_limit,
                    &height,
                ],
            )
            .await
            .context("failed to insert session")?;
        Ok(())
    }

    /// Revoke an existing session key.
    pub async fn revoke_session(
        &self,
        granter: &str,
        session_address: &str,
    ) -> Result<()> {
        self.client
            .execute(
                "UPDATE sessions SET revoked = true, updated_at = NOW()
                 WHERE granter = $1 AND session_address = $2",
                &[&granter, &session_address],
            )
            .await
            .context("failed to revoke session")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Match results
    // -----------------------------------------------------------------------

    /// Insert a match result.
    pub async fn insert_match_result(
        &self,
        match_id: &str,
        game_id: &str,
        timestamp: i64,
        players: &[String],
        winners: &[String],
        reward_pool: i64,
        attestor: &str,
        height: u64,
    ) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO match_results (match_id, game_id, timestamp, players,
                    winners, reward_pool, attestor, submitted_at_height)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (match_id) DO NOTHING",
                &[
                    &match_id,
                    &game_id,
                    &timestamp,
                    &players,
                    &winners,
                    &reward_pool,
                    &attestor,
                    &(height as i64),
                ],
            )
            .await
            .context("failed to insert match result")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    /// Insert an event row.
    pub async fn insert_event(
        &self,
        event: &EventData,
        block_height: u64,
        tx_hash: Option<&str>,
        tx_index: Option<i32>,
        event_index: i32,
    ) -> Result<()> {
        // Convert Vec<(String, String)> attributes to a JSON object.
        let attrs: serde_json::Value = serde_json::Value::Object(
            event
                .attributes
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
        );

        self.client
            .execute(
                "INSERT INTO events (block_height, tx_hash, tx_index, event_index,
                    module, action, attributes)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &(block_height as i64),
                    &tx_hash,
                    &tx_index,
                    &event_index,
                    &event.module,
                    &event.action,
                    &attrs,
                ],
            )
            .await
            .context("failed to insert event")?;
        Ok(())
    }
}
