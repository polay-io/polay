-- Blocks
CREATE TABLE IF NOT EXISTS blocks (
    height BIGINT PRIMARY KEY,
    hash VARCHAR(64) NOT NULL UNIQUE,
    parent_hash VARCHAR(64) NOT NULL,
    state_root VARCHAR(64) NOT NULL,
    transactions_root VARCHAR(64) NOT NULL,
    proposer VARCHAR(64) NOT NULL,
    chain_id VARCHAR(64) NOT NULL,
    timestamp BIGINT NOT NULL,
    tx_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Transactions
CREATE TABLE IF NOT EXISTS transactions (
    tx_hash VARCHAR(64) PRIMARY KEY,
    block_height BIGINT NOT NULL REFERENCES blocks(height),
    signer VARCHAR(64) NOT NULL,
    action_type VARCHAR(64) NOT NULL,
    action_data JSONB NOT NULL,
    nonce BIGINT NOT NULL,
    max_fee BIGINT NOT NULL,
    gas_used BIGINT NOT NULL DEFAULT 0,
    success BOOLEAN NOT NULL DEFAULT true,
    error TEXT,
    timestamp BIGINT NOT NULL,
    tx_index INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_txs_signer ON transactions(signer);
CREATE INDEX IF NOT EXISTS idx_txs_block ON transactions(block_height);
CREATE INDEX IF NOT EXISTS idx_txs_action ON transactions(action_type);

-- Accounts
CREATE TABLE IF NOT EXISTS accounts (
    address VARCHAR(64) PRIMARY KEY,
    balance BIGINT NOT NULL DEFAULT 0,
    nonce BIGINT NOT NULL DEFAULT 0,
    first_seen_height BIGINT,
    last_active_height BIGINT,
    tx_count INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Asset Classes
CREATE TABLE IF NOT EXISTS asset_classes (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(256) NOT NULL,
    symbol VARCHAR(32) NOT NULL,
    asset_type VARCHAR(32) NOT NULL,
    total_supply BIGINT NOT NULL DEFAULT 0,
    max_supply BIGINT,
    creator VARCHAR(64) NOT NULL,
    metadata_uri TEXT,
    created_at_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_assets_creator ON asset_classes(creator);
CREATE INDEX IF NOT EXISTS idx_assets_symbol ON asset_classes(symbol);

-- Asset Balances
CREATE TABLE IF NOT EXISTS asset_balances (
    asset_class_id VARCHAR(64) NOT NULL,
    owner VARCHAR(64) NOT NULL,
    amount BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (asset_class_id, owner)
);
CREATE INDEX IF NOT EXISTS idx_asset_bal_owner ON asset_balances(owner);

-- Marketplace Listings
CREATE TABLE IF NOT EXISTS listings (
    id VARCHAR(64) PRIMARY KEY,
    seller VARCHAR(64) NOT NULL,
    asset_class_id VARCHAR(64) NOT NULL,
    amount BIGINT NOT NULL,
    price_per_unit BIGINT NOT NULL,
    currency VARCHAR(64) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'Active',
    royalty_bps INTEGER NOT NULL DEFAULT 0,
    buyer VARCHAR(64),
    created_at_height BIGINT NOT NULL,
    updated_at_height BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_listings_seller ON listings(seller);
CREATE INDEX IF NOT EXISTS idx_listings_asset ON listings(asset_class_id);
CREATE INDEX IF NOT EXISTS idx_listings_status ON listings(status);

-- Player Profiles
CREATE TABLE IF NOT EXISTS player_profiles (
    address VARCHAR(64) PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    display_name VARCHAR(256) NOT NULL,
    reputation BIGINT NOT NULL DEFAULT 0,
    metadata TEXT,
    achievement_count INTEGER NOT NULL DEFAULT 0,
    created_at_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_profiles_username ON player_profiles(username);
CREATE INDEX IF NOT EXISTS idx_profiles_reputation ON player_profiles(reputation DESC);

-- Achievements
CREATE TABLE IF NOT EXISTS achievements (
    id VARCHAR(256) NOT NULL,
    player VARCHAR(64) NOT NULL,
    name VARCHAR(256) NOT NULL,
    metadata TEXT,
    soulbound BOOLEAN NOT NULL DEFAULT true,
    awarded_at_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (player, id)
);
CREATE INDEX IF NOT EXISTS idx_achievements_player ON achievements(player);

-- Validators
CREATE TABLE IF NOT EXISTS validators (
    address VARCHAR(64) PRIMARY KEY,
    stake BIGINT NOT NULL DEFAULT 0,
    commission_bps INTEGER NOT NULL DEFAULT 0,
    status VARCHAR(32) NOT NULL DEFAULT 'Active',
    blocks_produced BIGINT NOT NULL DEFAULT 0,
    jailed_until BIGINT,
    created_at_height BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Delegations
CREATE TABLE IF NOT EXISTS delegations (
    delegator VARCHAR(64) NOT NULL,
    validator VARCHAR(64) NOT NULL,
    amount BIGINT NOT NULL DEFAULT 0,
    reward_debt BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (delegator, validator)
);
CREATE INDEX IF NOT EXISTS idx_delegations_validator ON delegations(validator);

-- Match Results
CREATE TABLE IF NOT EXISTS match_results (
    match_id VARCHAR(64) PRIMARY KEY,
    game_id VARCHAR(128) NOT NULL,
    timestamp BIGINT NOT NULL,
    players TEXT[] NOT NULL,
    winners TEXT[] NOT NULL,
    reward_pool BIGINT NOT NULL DEFAULT 0,
    settled BOOLEAN NOT NULL DEFAULT false,
    quarantined BOOLEAN NOT NULL DEFAULT false,
    attestor VARCHAR(64) NOT NULL,
    submitted_at_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_matches_game ON match_results(game_id);
CREATE INDEX IF NOT EXISTS idx_matches_attestor ON match_results(attestor);

-- Events (catch-all for chain events)
CREATE TABLE IF NOT EXISTS events (
    id BIGSERIAL PRIMARY KEY,
    block_height BIGINT NOT NULL,
    tx_hash VARCHAR(64),
    tx_index INTEGER,
    event_index INTEGER NOT NULL,
    module VARCHAR(64) NOT NULL,
    action VARCHAR(128) NOT NULL,
    attributes JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_events_block ON events(block_height);
CREATE INDEX IF NOT EXISTS idx_events_module ON events(module);
CREATE INDEX IF NOT EXISTS idx_events_action ON events(action);

-- Indexer state
CREATE TABLE IF NOT EXISTS indexer_state (
    key VARCHAR(64) PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
