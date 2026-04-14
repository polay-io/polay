//! `polay-wallet` -- CLI wallet for the POLAY gaming blockchain.
//!
//! Supports the full transaction lifecycle: key management, transaction
//! building/signing/submission, and chain-state queries.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use polay_crypto::{sign_transaction, PolayKeypair};
use polay_rpc::types::{
    AccountResponse, AssetBalanceResponse, AssetClassResponse, BlockResponse, ChainInfoResponse,
    ListingResponse, ProfileResponse, SubmitTransactionResponse, ValidatorResponse,
};
use polay_types::{Address, AssetType, Hash, SignedTransaction, Transaction, TransactionAction};

// ===========================================================================
// CLI definition
// ===========================================================================

/// POLAY Wallet -- manage keys, build transactions, and query the chain.
#[derive(Parser)]
#[command(
    name = "polay-wallet",
    about = "CLI wallet for the POLAY gaming blockchain"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new Ed25519 keypair.
    Keygen {
        /// Where to write the secret key file.
        #[arg(long, default_value = "./keys/validator.key")]
        output: PathBuf,
    },

    /// Show the address for a key file.
    Address {
        /// Path to the secret key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
    },

    /// Query the native POL balance of an address.
    Balance {
        /// The address to query (hex).
        address: String,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },

    /// Query full account info (balance, nonce).
    Account {
        /// The address to query (hex).
        address: String,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },

    /// Build, sign, and submit a POL transfer transaction.
    Transfer {
        /// Recipient address (hex).
        #[arg(long)]
        to: String,
        /// Amount of POL to transfer (raw units).
        #[arg(long)]
        amount: u64,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee willing to pay (raw units).
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Asset management commands.
    Asset {
        #[command(subcommand)]
        command: AssetCommands,
    },

    /// Marketplace commands.
    Market {
        #[command(subcommand)]
        command: MarketCommands,
    },

    /// Player profile commands.
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },

    /// Staking commands.
    Stake {
        #[command(subcommand)]
        command: StakeCommands,
    },

    /// Validator commands.
    Validator {
        #[command(subcommand)]
        command: ValidatorCommands,
    },

    /// Chain information commands.
    Chain {
        #[command(subcommand)]
        command: ChainCommands,
    },

    /// Transaction status.
    Tx {
        #[command(subcommand)]
        command: TxCommands,
    },
}

// ---------------------------------------------------------------------------
// Asset subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum AssetCommands {
    /// Create a new asset class.
    Create {
        /// Human-readable name.
        #[arg(long)]
        name: String,
        /// Short ticker symbol.
        #[arg(long)]
        symbol: String,
        /// Asset type.
        #[arg(long, rename_all = "lowercase", value_enum)]
        r#type: AssetTypeArg,
        /// Optional max supply.
        #[arg(long)]
        max_supply: Option<u64>,
        /// URI pointing to off-chain metadata.
        #[arg(long)]
        metadata_uri: String,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Mint units of an existing asset class.
    Mint {
        /// Asset class ID (hex hash).
        #[arg(long)]
        asset_id: String,
        /// Recipient address (hex).
        #[arg(long)]
        to: String,
        /// Number of units to mint.
        #[arg(long)]
        amount: u64,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Transfer asset units to another address.
    Transfer {
        /// Asset class ID (hex hash).
        #[arg(long)]
        asset_id: String,
        /// Recipient address (hex).
        #[arg(long)]
        to: String,
        /// Number of units to transfer.
        #[arg(long)]
        amount: u64,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Burn asset units.
    Burn {
        /// Asset class ID (hex hash).
        #[arg(long)]
        asset_id: String,
        /// Number of units to burn.
        #[arg(long)]
        amount: u64,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Query asset balance.
    Balance {
        /// Asset class ID (hex hash).
        #[arg(long)]
        asset_id: String,
        /// Owner address (hex).
        #[arg(long)]
        owner: String,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },

    /// Query asset class details.
    Info {
        /// Asset class ID (hex hash).
        asset_id: String,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },
}

// ---------------------------------------------------------------------------
// Market subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum MarketCommands {
    /// Create a marketplace listing.
    List {
        /// Asset class ID (hex hash).
        #[arg(long)]
        asset_id: String,
        /// Number of units to list.
        #[arg(long)]
        amount: u64,
        /// Price per unit in native POL.
        #[arg(long)]
        price: u64,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Buy a listing.
    Buy {
        /// Listing ID (hex hash).
        #[arg(long)]
        listing_id: String,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Cancel a listing.
    Cancel {
        /// Listing ID (hex hash).
        #[arg(long)]
        listing_id: String,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Show listing details.
    Show {
        /// Listing ID (hex hash).
        listing_id: String,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },
}

// ---------------------------------------------------------------------------
// Profile subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum ProfileCommands {
    /// Create a player profile.
    Create {
        /// Username.
        #[arg(long)]
        username: String,
        /// Display name.
        #[arg(long)]
        display_name: String,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Show a player profile.
    Show {
        /// Player address (hex).
        address: String,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },
}

// ---------------------------------------------------------------------------
// Stake subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum StakeCommands {
    /// Delegate stake to a validator.
    Delegate {
        /// Validator address (hex).
        #[arg(long)]
        validator: String,
        /// Amount to delegate.
        #[arg(long)]
        amount: u64,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Undelegate stake from a validator.
    Undelegate {
        /// Validator address (hex).
        #[arg(long)]
        validator: String,
        /// Amount to undelegate.
        #[arg(long)]
        amount: u64,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },
}

// ---------------------------------------------------------------------------
// Validator subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum ValidatorCommands {
    /// Register as a validator.
    Register {
        /// Commission rate in basis points (0-10000).
        #[arg(long)]
        commission: u16,
        /// Path to the signing key file.
        #[arg(long, default_value = "./keys/validator.key")]
        key: PathBuf,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
        /// Maximum fee.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
    },

    /// Show validator info.
    Show {
        /// Validator address (hex).
        address: String,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },
}

// ---------------------------------------------------------------------------
// Chain subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum ChainCommands {
    /// Show chain info (height, latest hash, etc.).
    Info {
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },

    /// Show block details.
    Block {
        /// Block height.
        height: u64,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },
}

// ---------------------------------------------------------------------------
// Tx subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum TxCommands {
    /// Check transaction status (pending in mempool or confirmed).
    Status {
        /// Transaction hash (hex).
        hash: String,
        /// RPC endpoint URL.
        #[arg(long, default_value = "http://localhost:9944")]
        rpc: String,
    },
}

// ---------------------------------------------------------------------------
// AssetType CLI argument
// ---------------------------------------------------------------------------

#[derive(Clone, ValueEnum)]
enum AssetTypeArg {
    Fungible,
    Nft,
    Sft,
}

impl From<AssetTypeArg> for AssetType {
    fn from(a: AssetTypeArg) -> Self {
        match a {
            AssetTypeArg::Fungible => AssetType::Fungible,
            AssetTypeArg::Nft => AssetType::NonFungible,
            AssetTypeArg::Sft => AssetType::SemiFungible,
        }
    }
}

// ===========================================================================
// RPC Client
// ===========================================================================

struct RpcClient {
    url: String,
    client: reqwest::blocking::Client,
}

/// A JSON-RPC 2.0 request envelope.
#[derive(serde::Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: serde_json::Value,
    id: u64,
}

/// A JSON-RPC 2.0 response envelope.
#[derive(serde::Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    result: Option<T>,
    error: Option<JsonRpcError>,
    #[allow(dead_code)]
    id: Option<u64>,
}

#[derive(serde::Deserialize, Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RPC error {}: {}", self.code, self.message)
    }
}

impl RpcClient {
    fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }

    /// Make a JSON-RPC call and deserialize the result.
    fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };

        let resp = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .with_context(|| format!("failed to connect to RPC at {}", self.url))?;

        if !resp.status().is_success() {
            anyhow::bail!("RPC returned HTTP {}", resp.status());
        }

        let rpc_resp: JsonRpcResponse<T> =
            resp.json().context("failed to parse JSON-RPC response")?;

        if let Some(err) = rpc_resp.error {
            anyhow::bail!("{}", err);
        }

        rpc_resp
            .result
            .ok_or_else(|| anyhow::anyhow!("RPC returned null result"))
    }

    /// Like `call` but allows a null result (returns `Option<T>`).
    fn call_optional<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<Option<T>> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };

        let resp = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .with_context(|| format!("failed to connect to RPC at {}", self.url))?;

        if !resp.status().is_success() {
            anyhow::bail!("RPC returned HTTP {}", resp.status());
        }

        let rpc_resp: JsonRpcResponse<T> =
            resp.json().context("failed to parse JSON-RPC response")?;

        if let Some(err) = rpc_resp.error {
            anyhow::bail!("{}", err);
        }

        Ok(rpc_resp.result)
    }

    fn get_account(&self, address: &str) -> Result<Option<AccountResponse>> {
        self.call_optional("polay_getAccount", serde_json::json!([address]))
    }

    fn get_balance(&self, address: &str) -> Result<u64> {
        self.call("polay_getBalance", serde_json::json!([address]))
    }

    fn get_chain_info(&self) -> Result<ChainInfoResponse> {
        self.call("polay_getChainInfo", serde_json::json!([]))
    }

    fn get_block(&self, height: u64) -> Result<Option<BlockResponse>> {
        self.call_optional("polay_getBlock", serde_json::json!([height]))
    }

    fn get_asset_class(&self, asset_id: &str) -> Result<Option<AssetClassResponse>> {
        self.call_optional("polay_getAssetClass", serde_json::json!([asset_id]))
    }

    fn get_asset_balance(&self, asset_id: &str, owner: &str) -> Result<AssetBalanceResponse> {
        self.call(
            "polay_getAssetBalance",
            serde_json::json!([asset_id, owner]),
        )
    }

    fn get_listing(&self, listing_id: &str) -> Result<Option<ListingResponse>> {
        self.call_optional("polay_getListing", serde_json::json!([listing_id]))
    }

    fn get_profile(&self, address: &str) -> Result<Option<ProfileResponse>> {
        self.call_optional("polay_getProfile", serde_json::json!([address]))
    }

    fn get_validator(&self, address: &str) -> Result<Option<ValidatorResponse>> {
        self.call_optional("polay_getValidator", serde_json::json!([address]))
    }

    fn get_mempool_tx(&self, tx_hash: &str) -> Result<Option<SignedTransaction>> {
        self.call_optional("polay_getTransaction", serde_json::json!([tx_hash]))
    }

    fn submit_transaction(&self, tx: &SignedTransaction) -> Result<SubmitTransactionResponse> {
        self.call("polay_submitTransaction", serde_json::json!([tx]))
    }
}

// ===========================================================================
// Transaction helpers
// ===========================================================================

/// Load a keypair from a hex-encoded secret key file.
fn load_keypair(path: &PathBuf) -> Result<PolayKeypair> {
    let hex_str = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read key file {:?}", path))?;
    let hex_str = hex_str.trim();
    let bytes = hex::decode(hex_str).with_context(|| "key file does not contain valid hex")?;
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

/// Return the current Unix timestamp in seconds.
fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Fetch the account nonce from RPC. Returns 0 if the account does not exist.
fn fetch_nonce(rpc: &RpcClient, address: &str) -> Result<u64> {
    match rpc.get_account(address)? {
        Some(acct) => Ok(acct.nonce),
        None => Ok(0),
    }
}

/// Build a transaction, sign it, submit it, and print the result.
fn build_sign_submit(
    rpc: &RpcClient,
    keypair: &PolayKeypair,
    action: TransactionAction,
    max_fee: u64,
    label: &str,
) -> Result<()> {
    let address = keypair.address();
    let address_hex = address.to_hex();

    // 1. Get chain info for chain_id.
    let chain_info = rpc
        .get_chain_info()
        .context("failed to get chain info (is the node running?)")?;

    // 2. Get current nonce.
    let nonce = fetch_nonce(rpc, &address_hex)?;
    let next_nonce = nonce + 1;

    // 3. Build transaction.
    let tx = Transaction {
        chain_id: chain_info.chain_id,
        nonce: next_nonce,
        signer: address,
        action,
        max_fee,
        timestamp: now_timestamp(),
        session: None,
        sponsor: None,
    };

    // 4. Sign.
    let signed_tx =
        sign_transaction(keypair, tx).map_err(|e| anyhow::anyhow!("signing failed: {}", e))?;

    let tx_hash_hex = signed_tx.tx_hash.to_hex();

    // 5. Submit.
    let resp = rpc.submit_transaction(&signed_tx)?;

    // 6. Print result.
    println!();
    println!("=== {} ===", label);
    println!("From:    {}", address_hex);
    println!("Nonce:   {}", next_nonce);
    println!("Fee:     {} POL (max)", max_fee);
    println!("Tx Hash: {}", resp.tx_hash);
    println!("Status:  Submitted to mempool");
    println!();

    // Sanity check: the hash from the server should match ours.
    if resp.tx_hash != tx_hash_hex {
        eprintln!(
            "WARNING: server returned tx_hash {} but we computed {}",
            resp.tx_hash, tx_hash_hex
        );
    }

    Ok(())
}

/// Truncate a hex string to a short display form (first 8 + last 4 chars).
fn short_hex(s: &str) -> String {
    if s.len() <= 16 {
        s.to_string()
    } else {
        format!("{}...{}", &s[..8], &s[s.len() - 4..])
    }
}

// ===========================================================================
// Entry point
// ===========================================================================

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        // -------------------------------------------------------------------
        // Keygen
        // -------------------------------------------------------------------
        Commands::Keygen { output } => {
            let keypair = PolayKeypair::generate();
            let address = keypair.address();
            let pubkey = keypair.public_key();
            let secret_hex = hex::encode(keypair.to_bytes());

            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create directory {:?}", parent))?;
            }

            std::fs::write(&output, &secret_hex)
                .with_context(|| format!("failed to write key to {:?}", output))?;

            println!();
            println!("=== Keygen ===");
            println!("Address:    {}", address.to_hex());
            println!("Public key: {}", hex::encode(pubkey.to_bytes()));
            println!("Key file:   {}", output.display());
            println!();
        }

        // -------------------------------------------------------------------
        // Address
        // -------------------------------------------------------------------
        Commands::Address { key } => {
            let keypair = load_keypair(&key)?;
            let address = keypair.address();
            let pubkey = keypair.public_key();

            println!();
            println!("=== Address ===");
            println!("Address:    {}", address.to_hex());
            println!("Public key: {}", hex::encode(pubkey.to_bytes()));
            println!("Key file:   {}", key.display());
            println!();
        }

        // -------------------------------------------------------------------
        // Balance
        // -------------------------------------------------------------------
        Commands::Balance { address, rpc } => {
            let client = RpcClient::new(&rpc);
            let balance = client.get_balance(&address)?;

            println!();
            println!("=== Balance ===");
            println!("Address: {}", address);
            println!("Balance: {} POL", balance);
            println!();
        }

        // -------------------------------------------------------------------
        // Account
        // -------------------------------------------------------------------
        Commands::Account { address, rpc } => {
            let client = RpcClient::new(&rpc);
            match client.get_account(&address)? {
                Some(acct) => {
                    println!();
                    println!("=== Account ===");
                    println!("Address:    {}", acct.address);
                    println!("Balance:    {} POL", acct.balance);
                    println!("Nonce:      {}", acct.nonce);
                    println!("Created at: {}", acct.created_at);
                    println!();
                }
                None => {
                    println!();
                    println!("Account not found: {}", address);
                    println!();
                }
            }
        }

        // -------------------------------------------------------------------
        // Transfer
        // -------------------------------------------------------------------
        Commands::Transfer {
            to,
            amount,
            key,
            rpc,
            max_fee,
        } => {
            let keypair = load_keypair(&key)?;
            let to_addr = Address::from_hex(&to)
                .map_err(|e| anyhow::anyhow!("invalid recipient address: {}", e))?;
            let client = RpcClient::new(&rpc);

            let action = TransactionAction::Transfer {
                to: to_addr,
                amount,
            };

            // Use a more detailed printout for transfer.
            let address = keypair.address();
            let address_hex = address.to_hex();

            let chain_info = client
                .get_chain_info()
                .context("failed to get chain info (is the node running?)")?;

            let nonce = fetch_nonce(&client, &address_hex)?;
            let next_nonce = nonce + 1;

            let tx = Transaction {
                chain_id: chain_info.chain_id,
                nonce: next_nonce,
                signer: address,
                action,
                max_fee,
                timestamp: now_timestamp(),
                session: None,
                sponsor: None,
            };

            let signed_tx = sign_transaction(&keypair, tx)
                .map_err(|e| anyhow::anyhow!("signing failed: {}", e))?;

            let resp = client.submit_transaction(&signed_tx)?;

            println!();
            println!("=== Transfer ===");
            println!("From:    {}", address_hex);
            println!("To:      {}", to);
            println!("Amount:  {} POL", amount);
            println!("Fee:     {} POL (max)", max_fee);
            println!("Tx Hash: {}", resp.tx_hash);
            println!("Status:  Submitted to mempool");
            println!();
        }

        // -------------------------------------------------------------------
        // Asset commands
        // -------------------------------------------------------------------
        Commands::Asset { command } => match command {
            AssetCommands::Create {
                name,
                symbol,
                r#type,
                max_supply,
                metadata_uri,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);
                let action = TransactionAction::CreateAssetClass {
                    name: name.clone(),
                    symbol: symbol.clone(),
                    asset_type: r#type.into(),
                    max_supply,
                    metadata_uri: metadata_uri.clone(),
                };

                println!("Creating asset class: {} ({})", name, symbol);
                build_sign_submit(&client, &keypair, action, max_fee, "Create Asset")?;
            }

            AssetCommands::Mint {
                asset_id,
                to,
                amount,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);
                let asset_hash = Hash::from_hex(&asset_id)
                    .map_err(|e| anyhow::anyhow!("invalid asset ID: {}", e))?;
                let to_addr = Address::from_hex(&to)
                    .map_err(|e| anyhow::anyhow!("invalid recipient address: {}", e))?;

                let action = TransactionAction::MintAsset {
                    asset_class_id: asset_hash,
                    to: to_addr,
                    amount,
                    metadata: None,
                };

                println!("Minting {} units of asset {}", amount, short_hex(&asset_id));
                build_sign_submit(&client, &keypair, action, max_fee, "Mint Asset")?;
            }

            AssetCommands::Transfer {
                asset_id,
                to,
                amount,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);
                let asset_hash = Hash::from_hex(&asset_id)
                    .map_err(|e| anyhow::anyhow!("invalid asset ID: {}", e))?;
                let to_addr = Address::from_hex(&to)
                    .map_err(|e| anyhow::anyhow!("invalid recipient address: {}", e))?;

                let action = TransactionAction::TransferAsset {
                    asset_class_id: asset_hash,
                    to: to_addr,
                    amount,
                };

                build_sign_submit(&client, &keypair, action, max_fee, "Transfer Asset")?;
            }

            AssetCommands::Burn {
                asset_id,
                amount,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);
                let asset_hash = Hash::from_hex(&asset_id)
                    .map_err(|e| anyhow::anyhow!("invalid asset ID: {}", e))?;

                let action = TransactionAction::BurnAsset {
                    asset_class_id: asset_hash,
                    amount,
                };

                build_sign_submit(&client, &keypair, action, max_fee, "Burn Asset")?;
            }

            AssetCommands::Balance {
                asset_id,
                owner,
                rpc,
            } => {
                let client = RpcClient::new(&rpc);
                let resp = client.get_asset_balance(&asset_id, &owner)?;

                println!();
                println!("=== Asset Balance ===");
                println!("Asset:  {}", resp.asset_class_id);
                println!("Owner:  {}", resp.owner);
                println!("Amount: {}", resp.amount);
                println!();
            }

            AssetCommands::Info { asset_id, rpc } => {
                let client = RpcClient::new(&rpc);
                match client.get_asset_class(&asset_id)? {
                    Some(asset) => {
                        println!();
                        println!("=== Asset Class ===");
                        println!("ID:           {}", asset.id);
                        println!("Name:         {}", asset.name);
                        println!("Symbol:       {}", asset.symbol);
                        println!("Type:         {:?}", asset.asset_type);
                        println!("Total Supply: {}", asset.total_supply);
                        match asset.max_supply {
                            Some(max) => println!("Max Supply:   {}", max),
                            None => println!("Max Supply:   unlimited"),
                        }
                        println!("Creator:      {}", asset.creator);
                        println!("Metadata URI: {}", asset.metadata_uri);
                        println!("Created at:   {}", asset.created_at);
                        println!();
                    }
                    None => {
                        println!();
                        println!("Asset class not found: {}", asset_id);
                        println!();
                    }
                }
            }
        },

        // -------------------------------------------------------------------
        // Market commands
        // -------------------------------------------------------------------
        Commands::Market { command } => match command {
            MarketCommands::List {
                asset_id,
                amount,
                price,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);
                let asset_hash = Hash::from_hex(&asset_id)
                    .map_err(|e| anyhow::anyhow!("invalid asset ID: {}", e))?;

                let action = TransactionAction::CreateListing {
                    asset_class_id: asset_hash,
                    amount,
                    price_per_unit: price,
                    currency: Hash::ZERO, // native POL
                };

                println!(
                    "Creating listing: {} units of {} at {} POL each",
                    amount,
                    short_hex(&asset_id),
                    price
                );
                build_sign_submit(&client, &keypair, action, max_fee, "Create Listing")?;
            }

            MarketCommands::Buy {
                listing_id,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);
                let listing_hash = Hash::from_hex(&listing_id)
                    .map_err(|e| anyhow::anyhow!("invalid listing ID: {}", e))?;

                let action = TransactionAction::BuyListing {
                    listing_id: listing_hash,
                };

                build_sign_submit(&client, &keypair, action, max_fee, "Buy Listing")?;
            }

            MarketCommands::Cancel {
                listing_id,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);
                let listing_hash = Hash::from_hex(&listing_id)
                    .map_err(|e| anyhow::anyhow!("invalid listing ID: {}", e))?;

                let action = TransactionAction::CancelListing {
                    listing_id: listing_hash,
                };

                build_sign_submit(&client, &keypair, action, max_fee, "Cancel Listing")?;
            }

            MarketCommands::Show { listing_id, rpc } => {
                let client = RpcClient::new(&rpc);
                match client.get_listing(&listing_id)? {
                    Some(listing) => {
                        println!();
                        println!("=== Listing ===");
                        println!("ID:             {}", listing.id);
                        println!("Seller:         {}", listing.seller);
                        println!("Asset:          {}", listing.asset_class_id);
                        println!("Amount:         {}", listing.amount);
                        println!("Price per unit: {} POL", listing.price_per_unit);
                        println!("Currency:       {}", listing.currency);
                        println!("Status:         {:?}", listing.status);
                        println!("Royalty (bps):  {}", listing.royalty_bps);
                        println!("Created at:     {}", listing.created_at);
                        println!();
                    }
                    None => {
                        println!();
                        println!("Listing not found: {}", listing_id);
                        println!();
                    }
                }
            }
        },

        // -------------------------------------------------------------------
        // Profile commands
        // -------------------------------------------------------------------
        Commands::Profile { command } => match command {
            ProfileCommands::Create {
                username,
                display_name,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);

                let action = TransactionAction::CreateProfile {
                    username: username.clone(),
                    display_name: display_name.clone(),
                    metadata: None,
                };

                println!("Creating profile: {} ({})", display_name, username);
                build_sign_submit(&client, &keypair, action, max_fee, "Create Profile")?;
            }

            ProfileCommands::Show { address, rpc } => {
                let client = RpcClient::new(&rpc);
                match client.get_profile(&address)? {
                    Some(profile) => {
                        println!();
                        println!("=== Profile ===");
                        println!("Address:      {}", profile.address);
                        println!("Username:     {}", profile.username);
                        println!("Display name: {}", profile.display_name);
                        println!("Reputation:   {}", profile.reputation);
                        match &profile.metadata {
                            Some(meta) => println!("Metadata:     {}", meta),
                            None => println!("Metadata:     (none)"),
                        }
                        println!("Created at:   {}", profile.created_at);
                        println!();
                    }
                    None => {
                        println!();
                        println!("Profile not found: {}", address);
                        println!();
                    }
                }
            }
        },

        // -------------------------------------------------------------------
        // Stake commands
        // -------------------------------------------------------------------
        Commands::Stake { command } => match command {
            StakeCommands::Delegate {
                validator,
                amount,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);
                let validator_addr = Address::from_hex(&validator)
                    .map_err(|e| anyhow::anyhow!("invalid validator address: {}", e))?;

                let action = TransactionAction::DelegateStake {
                    validator: validator_addr,
                    amount,
                };

                println!(
                    "Delegating {} POL to validator {}",
                    amount,
                    short_hex(&validator)
                );
                build_sign_submit(&client, &keypair, action, max_fee, "Delegate Stake")?;
            }

            StakeCommands::Undelegate {
                validator,
                amount,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);
                let validator_addr = Address::from_hex(&validator)
                    .map_err(|e| anyhow::anyhow!("invalid validator address: {}", e))?;

                let action = TransactionAction::UndelegateStake {
                    validator: validator_addr,
                    amount,
                };

                println!(
                    "Undelegating {} POL from validator {}",
                    amount,
                    short_hex(&validator)
                );
                build_sign_submit(&client, &keypair, action, max_fee, "Undelegate Stake")?;
            }
        },

        // -------------------------------------------------------------------
        // Validator commands
        // -------------------------------------------------------------------
        Commands::Validator { command } => match command {
            ValidatorCommands::Register {
                commission,
                key,
                rpc,
                max_fee,
            } => {
                let keypair = load_keypair(&key)?;
                let client = RpcClient::new(&rpc);

                let action = TransactionAction::RegisterValidator {
                    commission_bps: commission,
                };

                println!("Registering validator with {} bps commission", commission);
                build_sign_submit(&client, &keypair, action, max_fee, "Register Validator")?;
            }

            ValidatorCommands::Show { address, rpc } => {
                let client = RpcClient::new(&rpc);
                match client.get_validator(&address)? {
                    Some(v) => {
                        println!();
                        println!("=== Validator ===");
                        println!("Address:         {}", v.address);
                        println!("Stake:           {} POL", v.stake);
                        println!("Commission:      {} bps", v.commission_bps);
                        println!("Status:          {:?}", v.status);
                        match v.jailed_until {
                            Some(h) => println!("Jailed until:    block {}", h),
                            None => println!("Jailed until:    (not jailed)"),
                        }
                        println!("Blocks produced: {}", v.blocks_produced);
                        println!();
                    }
                    None => {
                        println!();
                        println!("Validator not found: {}", address);
                        println!();
                    }
                }
            }
        },

        // -------------------------------------------------------------------
        // Chain commands
        // -------------------------------------------------------------------
        Commands::Chain { command } => match command {
            ChainCommands::Info { rpc } => {
                let client = RpcClient::new(&rpc);
                let info = client.get_chain_info()?;

                println!();
                println!("=== Chain Info ===");
                println!("Chain ID:    {}", info.chain_id);
                println!("Height:      {}", info.height);
                println!("Latest hash: {}", info.latest_hash);
                println!("State root:  {}", info.state_root);
                println!("Block time:  {}", info.block_time);
                println!();
            }

            ChainCommands::Block { height, rpc } => {
                let client = RpcClient::new(&rpc);
                match client.get_block(height)? {
                    Some(block) => {
                        println!();
                        println!("=== Block #{} ===", block.height);
                        println!("Height:       {}", block.height);
                        println!("Hash:         {}", block.hash);
                        println!("Parent:       {}", block.parent_hash);
                        println!("State root:   {}", block.state_root);
                        println!("Tx root:      {}", block.transactions_root);
                        println!("Proposer:     {}", block.proposer);
                        println!("Chain ID:     {}", block.chain_id);
                        println!("Transactions: {}", block.tx_count);
                        println!("Timestamp:    {}", block.timestamp);

                        if !block.transactions.is_empty() {
                            println!();
                            println!("  Transactions:");
                            for (i, tx) in block.transactions.iter().enumerate() {
                                println!(
                                    "    [{}] {} | {} | nonce={}",
                                    i,
                                    short_hex(&tx.tx_hash.to_hex()),
                                    tx.action_label(),
                                    tx.nonce()
                                );
                            }
                        }
                        println!();
                    }
                    None => {
                        println!();
                        println!("Block not found at height {}", height);
                        println!();
                    }
                }
            }
        },

        // -------------------------------------------------------------------
        // Tx commands
        // -------------------------------------------------------------------
        Commands::Tx { command } => match command {
            TxCommands::Status { hash, rpc } => {
                let client = RpcClient::new(&rpc);

                // First check mempool.
                match client.get_mempool_tx(&hash)? {
                    Some(tx) => {
                        println!();
                        println!("=== Transaction ===");
                        println!("Tx Hash: {}", tx.tx_hash.to_hex());
                        println!("Signer:  {}", tx.signer().to_hex());
                        println!("Action:  {}", tx.action_label());
                        println!("Nonce:   {}", tx.nonce());
                        println!("Status:  Pending (in mempool)");
                        println!();
                    }
                    None => {
                        // Not in mempool -- might have been confirmed already.
                        // Search recent blocks for the transaction.
                        let chain_info = client.get_chain_info()?;
                        let search_depth = std::cmp::min(chain_info.height, 50);
                        let mut found = false;

                        let tx_hash_parsed = Hash::from_hex(&hash).ok();

                        if let Some(target_hash) = tx_hash_parsed {
                            for h in (chain_info.height.saturating_sub(search_depth)
                                ..=chain_info.height)
                                .rev()
                            {
                                if h == 0 {
                                    continue;
                                }
                                if let Some(block) = client.get_block(h)? {
                                    for tx in &block.transactions {
                                        if tx.tx_hash == target_hash {
                                            println!();
                                            println!("=== Transaction ===");
                                            println!("Tx Hash: {}", tx.tx_hash.to_hex());
                                            println!("Signer:  {}", tx.signer().to_hex());
                                            println!("Action:  {}", tx.action_label());
                                            println!("Nonce:   {}", tx.nonce());
                                            println!(
                                                "Status:  Confirmed (block #{})",
                                                block.height
                                            );
                                            println!();
                                            found = true;
                                            break;
                                        }
                                    }
                                    if found {
                                        break;
                                    }
                                }
                            }
                        }

                        if !found {
                            println!();
                            println!("Transaction not found: {}", hash);
                            println!("(not in mempool or recent {} blocks)", search_depth);
                            println!();
                        }
                    }
                }
            }
        },
    }

    Ok(())
}
