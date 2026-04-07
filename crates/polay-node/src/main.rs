use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::EnvFilter;

use polay_crypto::PolayKeypair;
use polay_genesis::Genesis;
use polay_network::{P2PConfig, P2PService};
use polay_rpc::{start_rpc_server, EventBus};
use polay_state::RocksDbStore;
use polay_validator::ValidatorNode;

mod bench_runner;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

/// POLAY — a gaming blockchain node.
#[derive(Parser)]
#[command(name = "polay", about = "POLAY gaming blockchain node")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a validator node.
    Run {
        /// Path to the genesis JSON file.
        #[arg(long, default_value = "genesis.json")]
        genesis: PathBuf,

        /// Path to the data directory (RocksDB state).
        #[arg(long, default_value = "./data")]
        data_dir: PathBuf,

        /// Address for the JSON-RPC server to listen on.
        #[arg(long, default_value = "0.0.0.0:9944")]
        rpc_addr: String,

        /// Path to the validator secret key file (32 hex-encoded bytes).
        #[arg(long, default_value = "./keys/validator.key")]
        validator_key: PathBuf,

        /// Target block production interval in milliseconds.
        #[arg(long, default_value_t = 2000)]
        block_time: u64,

        /// Log level filter (e.g. info, debug, trace).
        #[arg(long, default_value = "info")]
        log_level: String,

        /// libp2p listen address (multiaddr). If set, enables P2P networking
        /// and multi-validator BFT consensus.
        #[arg(long, default_value = None)]
        p2p_addr: Option<String>,

        /// Comma-separated boot node multiaddrs to connect to on startup.
        #[arg(long, default_value = None)]
        boot_nodes: Option<String>,
    },

    /// Initialize a new chain: generate genesis, keys, and data directory.
    Init {
        /// Where to write the genesis JSON file.
        #[arg(long, default_value = "genesis.json")]
        output: PathBuf,

        /// Number of validators to include in the genesis.
        #[arg(long, default_value_t = 4)]
        validators: u32,

        /// Data directory (will be created if absent).
        #[arg(long, default_value = "./data")]
        data_dir: PathBuf,

        /// Network profile: "devnet", "testnet", or "mainnet".
        #[arg(long, default_value = "devnet")]
        network: String,
    },

    /// Generate a new Ed25519 keypair and print the address.
    Keygen {
        /// Where to write the secret key (hex-encoded 32 bytes).
        #[arg(long, default_value = "./keys/validator.key")]
        output: PathBuf,
    },

    /// Run an in-memory execution benchmark.
    Bench {
        /// Number of transactions to execute.
        #[arg(long, default_value_t = 10_000)]
        txs: usize,

        /// Number of accounts to generate.
        #[arg(long, default_value_t = 1_000)]
        accounts: usize,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            genesis,
            data_dir,
            rpc_addr,
            validator_key,
            block_time,
            log_level,
            p2p_addr,
            boot_nodes,
        } => cmd_run(genesis, data_dir, rpc_addr, validator_key, block_time, log_level, p2p_addr, boot_nodes).await,
        Commands::Init {
            output,
            validators,
            data_dir,
            network,
        } => cmd_init(output, validators, data_dir, network),
        Commands::Keygen { output } => cmd_keygen(output),
        Commands::Bench { txs, accounts } => {
            bench_runner::run_bench(txs, accounts);
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// run command
// ---------------------------------------------------------------------------

async fn cmd_run(
    genesis_path: PathBuf,
    data_dir: PathBuf,
    rpc_addr: String,
    validator_key_path: PathBuf,
    block_time: u64,
    log_level: String,
    p2p_addr: Option<String>,
    boot_nodes: Option<String>,
) -> Result<()> {
    // Initialize tracing.
    let filter = EnvFilter::try_new(&log_level)
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    info!("starting POLAY node");

    // Load genesis.
    let genesis = Genesis::load(&genesis_path)
        .with_context(|| format!("failed to load genesis from {:?}", genesis_path))?;
    genesis
        .validate()
        .context("genesis validation failed")?;
    info!(
        chain_id = %genesis.chain_config.chain_id,
        validators = genesis.validators.len(),
        accounts = genesis.accounts.len(),
        "genesis loaded"
    );

    // Load validator key.
    let keypair = load_keypair(&validator_key_path)
        .with_context(|| format!("failed to load validator key from {:?}", validator_key_path))?;
    info!(address = %keypair.address(), "validator key loaded");

    // Open the state store.
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("failed to create data dir {:?}", data_dir))?;
    let db_path = data_dir.join("state");
    let store = Arc::new(
        RocksDbStore::new(db_path.to_str().unwrap())
            .context("failed to open RocksDB state store")?,
    );

    // Create the shared event bus for WebSocket subscriptions.
    let event_bus = Arc::new(EventBus::new(1024));

    // Create the validator node.
    let chain_config = genesis.chain_config.clone();
    let mut node = ValidatorNode::new(Arc::clone(&store) as Arc<dyn polay_state::StateStore>, &genesis, keypair, chain_config)
        .context("failed to initialize validator node")?;
    node.set_event_bus(Arc::clone(&event_bus));

    // Start the RPC server (HTTP + WebSocket).
    let rpc_store = node.store_arc();
    let rpc_mempool = node.mempool();
    let rpc_chain_config = genesis.chain_config.clone();
    let rpc_handle = start_rpc_server(&rpc_addr, rpc_store, rpc_mempool, rpc_chain_config, Arc::clone(&event_bus))
        .await
        .map_err(|e| anyhow::anyhow!("failed to start RPC server: {}", e))?;
    info!(addr = %rpc_addr, "RPC server started (HTTP + WS)");

    // Determine whether to run in P2P multi-validator mode.
    let use_p2p = p2p_addr.is_some();

    if use_p2p {
        let listen_addr = p2p_addr.unwrap_or_else(|| "/ip4/0.0.0.0/tcp/30333".to_string());
        let boot_node_list: Vec<String> = boot_nodes
            .map(|s| s.split(',').map(|b| b.trim().to_string()).filter(|b| !b.is_empty()).collect())
            .unwrap_or_default();

        info!(%listen_addr, boot_nodes = boot_node_list.len(), "starting P2P networking");

        let p2p_config = P2PConfig {
            listen_addr,
            boot_nodes: boot_node_list,
            node_keypair: None,
            ..Default::default()
        };

        let p2p_service = P2PService::start(p2p_config)
            .await
            .map_err(|e| anyhow::anyhow!("failed to start P2P service: {}", e))?;

        node.set_network(p2p_service);

        // Initialize consensus with the genesis validator set.
        let validator_set = ValidatorNode::validator_set_from_genesis(&genesis);
        info!(validators = validator_set.len(), total_stake = validator_set.total_stake, "consensus initialized from genesis");
        node.init_consensus(validator_set);
    }

    // Start the validator loop in a background task.
    let validator_task = if use_p2p {
        tokio::spawn(async move {
            node.run().await;
        })
    } else {
        tokio::spawn(async move {
            node.run_single_validator(block_time).await;
        })
    };

    // Wait for shutdown signal (Ctrl+C).
    info!("node is running — press Ctrl+C to shut down");
    tokio::signal::ctrl_c()
        .await
        .context("failed to listen for shutdown signal")?;

    info!("shutting down...");

    // Stop the RPC server.
    rpc_handle.stop().map_err(|e| anyhow::anyhow!("RPC shutdown error: {}", e))?;

    // Abort the validator loop.
    validator_task.abort();

    info!("POLAY node shut down cleanly");
    Ok(())
}

// ---------------------------------------------------------------------------
// init command
// ---------------------------------------------------------------------------

fn cmd_init(output: PathBuf, validators: u32, data_dir: PathBuf, network: String) -> Result<()> {
    // Initialize tracing for the CLI command.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .init();

    info!(network = %network, "generating genesis");

    // Generate a validator keypair first so we can include it in genesis.
    let keys_dir = data_dir.join("keys");
    std::fs::create_dir_all(&keys_dir)
        .with_context(|| format!("failed to create keys dir {:?}", keys_dir))?;

    let keypair = PolayKeypair::generate();
    let key_path = keys_dir.join("validator.key");
    let key_hex = hex::encode(keypair.to_bytes());
    std::fs::write(&key_path, &key_hex)
        .with_context(|| format!("failed to write validator key to {:?}", key_path))?;

    let my_address = format!("{}", keypair.address());
    let my_pubkey = hex::encode(keypair.public_key().to_bytes());

    info!(
        address = %my_address,
        pubkey = %my_pubkey,
        key_file = ?key_path,
        "validator keypair generated"
    );

    // Generate genesis based on the selected network profile.
    let mut genesis = match network.as_str() {
        "testnet" => Genesis::generate_testnet(validators),
        "mainnet" => {
            info!("mainnet genesis requires explicit validator/account configuration");
            anyhow::bail!(
                "mainnet genesis cannot be auto-generated; provide explicit \
                 validator and account lists via a ceremony tool"
            );
        }
        _ => Genesis::generate_devnet(),
    };

    // Inject the generated validator into genesis so it has funds and can
    // participate as a validator immediately.
    use polay_genesis::{GenesisAccount, GenesisValidator};

    let operator_balance: u64 = 10_000_000;

    // Add as a funded account.
    genesis.accounts.push(GenesisAccount {
        address: my_address.clone(),
        balance: operator_balance,
    });

    // Add as a validator.
    genesis.validators.push(GenesisValidator {
        address: my_address.clone(),
        pubkey: my_pubkey,
        stake: genesis.chain_config.min_stake,
        commission_bps: 500,
    });

    // Update initial supply to include the new account.
    genesis.initial_supply += operator_balance;

    genesis
        .validate()
        .context("generated genesis failed validation")?;

    // Validate the chain config itself.
    genesis
        .chain_config
        .validate()
        .map_err(|errs| anyhow::anyhow!("chain config validation failed: {}", errs.join("; ")))?;

    // Create output directories.
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {:?}", parent))?;
    }
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("failed to create data dir {:?}", data_dir))?;

    // Write genesis to disk.
    genesis
        .save(&output)
        .with_context(|| format!("failed to write genesis to {:?}", output))?;
    info!(path = ?output, "genesis written");

    info!(
        chain_id = %genesis.chain_config.chain_id,
        network = %network,
        validators = genesis.validators.len(),
        accounts = genesis.accounts.len(),
        initial_supply = genesis.initial_supply,
        "{} initialized successfully", network
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// keygen command
// ---------------------------------------------------------------------------

fn cmd_keygen(output: PathBuf) -> Result<()> {
    // Initialize tracing for the CLI command.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .init();

    let keypair = PolayKeypair::generate();
    let address = keypair.address();
    let pubkey = keypair.public_key();
    let secret_hex = hex::encode(keypair.to_bytes());

    // Create parent directories.
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {:?}", parent))?;
    }

    // Write the secret key.
    std::fs::write(&output, &secret_hex)
        .with_context(|| format!("failed to write key to {:?}", output))?;

    info!(
        address = %address,
        pubkey = %hex::encode(pubkey.to_bytes()),
        key_file = ?output,
        "keypair generated"
    );

    // Also print to stdout for easy scripting.
    println!("Address:    {}", address.to_hex());
    println!("Public key: {}", hex::encode(pubkey.to_bytes()));
    println!("Key file:   {}", output.display());

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load a `PolayKeypair` from a file containing 32 hex-encoded secret key
/// bytes.
fn load_keypair(path: &PathBuf) -> Result<PolayKeypair> {
    let hex_str = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read key file {:?}", path))?;
    let hex_str = hex_str.trim();
    let bytes = hex::decode(hex_str)
        .with_context(|| "key file does not contain valid hex")?;
    if bytes.len() != 32 {
        anyhow::bail!(
            "expected 32 secret key bytes, got {} (from {:?})",
            bytes.len(),
            path
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    let keypair = PolayKeypair::from_bytes(&arr)
        .map_err(|e| anyhow::anyhow!("invalid Ed25519 key: {}", e))?;
    Ok(keypair)
}
