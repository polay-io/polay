-- Migration 002: Complete coverage for remaining transaction types
-- Adds tables for attestors, reward distributions, governance, and session keys.

-- Attestors
CREATE TABLE IF NOT EXISTS attestors (
    address VARCHAR(64) PRIMARY KEY,
    game_id VARCHAR(128) NOT NULL,
    endpoint VARCHAR(512) NOT NULL,
    metadata TEXT,
    status VARCHAR(32) NOT NULL DEFAULT 'Active',
    registered_at_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_attestors_game ON attestors(game_id);

-- Reward Distributions (individual reward records from match settlements)
CREATE TABLE IF NOT EXISTS reward_distributions (
    id BIGSERIAL PRIMARY KEY,
    match_id VARCHAR(64) NOT NULL,
    player VARCHAR(64) NOT NULL,
    amount BIGINT NOT NULL,
    distributed_at_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_rewards_match ON reward_distributions(match_id);
CREATE INDEX IF NOT EXISTS idx_rewards_player ON reward_distributions(player);

-- Governance Proposals
CREATE TABLE IF NOT EXISTS proposals (
    id VARCHAR(64) PRIMARY KEY,
    proposer VARCHAR(64) NOT NULL,
    title VARCHAR(256) NOT NULL,
    description TEXT NOT NULL,
    action_type VARCHAR(64) NOT NULL,
    action_data JSONB NOT NULL,
    deposit BIGINT NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'Voting',
    yes_votes BIGINT NOT NULL DEFAULT 0,
    no_votes BIGINT NOT NULL DEFAULT 0,
    abstain_votes BIGINT NOT NULL DEFAULT 0,
    voting_start_height BIGINT NOT NULL,
    voting_end_height BIGINT NOT NULL,
    created_at_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_proposals_proposer ON proposals(proposer);
CREATE INDEX IF NOT EXISTS idx_proposals_status ON proposals(status);

-- Governance Votes
CREATE TABLE IF NOT EXISTS votes (
    proposal_id VARCHAR(64) NOT NULL,
    voter VARCHAR(64) NOT NULL,
    option VARCHAR(16) NOT NULL,
    weight BIGINT NOT NULL DEFAULT 0,
    height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (proposal_id, voter)
);
CREATE INDEX IF NOT EXISTS idx_votes_voter ON votes(voter);

-- Session Keys
CREATE TABLE IF NOT EXISTS sessions (
    granter VARCHAR(64) NOT NULL,
    session_address VARCHAR(64) NOT NULL,
    session_pubkey VARCHAR(64) NOT NULL,
    permissions JSONB NOT NULL,
    expires_at BIGINT NOT NULL,
    spending_limit BIGINT NOT NULL,
    amount_spent BIGINT NOT NULL DEFAULT 0,
    revoked BOOLEAN NOT NULL DEFAULT false,
    created_at_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (granter, session_address)
);
CREATE INDEX IF NOT EXISTS idx_sessions_granter ON sessions(granter);
CREATE INDEX IF NOT EXISTS idx_sessions_active ON sessions(revoked, expires_at);

-- Add volume tracking to listings
ALTER TABLE listings ADD COLUMN IF NOT EXISTS total_price BIGINT NOT NULL DEFAULT 0;
