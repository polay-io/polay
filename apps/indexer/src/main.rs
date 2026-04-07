//! POLAY Blockchain Indexer
//!
//! A standalone service that reads blocks from a POLAY node over JSON-RPC and
//! indexes them into PostgreSQL for efficient querying by explorers, wallets,
//! and game clients.

mod db;
mod indexer;
mod rpc_client;
mod types;

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// POLAY blockchain indexer -- reads blocks via RPC and writes to PostgreSQL.
#[derive(Parser, Debug)]
#[command(name = "polay-indexer", version, about)]
struct Cli {
    /// JSON-RPC endpoint of the POLAY node.
    #[arg(long, default_value = "http://localhost:9944", env = "RPC_URL")]
    rpc_url: String,

    /// PostgreSQL connection string.
    #[arg(
        long,
        default_value = "postgres://localhost/polay_indexer",
        env = "DATABASE_URL"
    )]
    database_url: String,

    /// Block height to start indexing from (only used on first run; afterwards
    /// the indexer resumes from the last indexed height stored in the DB).
    #[arg(long, default_value_t = 0, env = "START_HEIGHT")]
    start_height: u64,

    /// How often (in milliseconds) to poll the node for new blocks.
    #[arg(long, default_value_t = 1000, env = "POLL_INTERVAL_MS")]
    poll_interval_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with RUST_LOG env filter support.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    info!(
        rpc_url = %cli.rpc_url,
        database_url = %cli.database_url,
        start_height = cli.start_height,
        poll_interval_ms = cli.poll_interval_ms,
        "starting polay-indexer"
    );

    // Connect to PostgreSQL and run migrations.
    let db = db::Database::connect(&cli.database_url).await?;
    db.run_migrations().await?;

    // If this is a fresh database _and_ a non-zero start height was requested,
    // seed the initial height so the indexer skips earlier blocks.
    let current_height = db.get_indexed_height().await?;
    if current_height == 0 && cli.start_height > 0 {
        info!(
            start_height = cli.start_height,
            "seeding initial indexed height"
        );
        // Set to start_height - 1 so the loop will fetch start_height first.
        db.set_indexed_height(cli.start_height.saturating_sub(1))
            .await?;
    }

    // Build the RPC client.
    let rpc = rpc_client::RpcClient::new(&cli.rpc_url);

    // Build and run the indexer.
    let idx = indexer::Indexer::new(
        rpc,
        db,
        Duration::from_millis(cli.poll_interval_ms),
    );

    idx.run().await
}
