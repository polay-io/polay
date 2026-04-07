use std::path::Path;

use polay_types::economics::{FeeDistribution, InflationParams};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// NetworkProfile
// ---------------------------------------------------------------------------

/// Identifies which network profile a configuration was built for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NetworkProfile {
    Devnet,
    Testnet,
    Mainnet,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("failed to parse config JSON: {0}")]
    ParseError(#[from] serde_json::Error),
}

pub type ConfigResult<T> = Result<T, ConfigError>;

// ---------------------------------------------------------------------------
// ChainConfig
// ---------------------------------------------------------------------------

/// On-chain parameters that define the behaviour of the POLAY blockchain.
///
/// These values are fixed at genesis and (in future) can only change through
/// governance proposals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainConfig {
    /// Unique identifier for this chain instance (e.g. "polay-devnet-1").
    pub chain_id: String,

    /// Human-readable name (e.g. "POLAY Devnet").
    pub chain_name: String,

    /// The denomination string for the native token.
    #[serde(default = "default_native_denom")]
    pub native_denom: String,

    /// Target block time in milliseconds.
    #[serde(default = "default_block_time_ms")]
    pub block_time_ms: u64,

    /// Maximum number of transactions allowed in a single block.
    #[serde(default = "default_max_block_transactions")]
    pub max_block_transactions: usize,

    /// Maximum serialized size (bytes) of a single transaction.
    #[serde(default = "default_max_transaction_size_bytes")]
    pub max_transaction_size_bytes: usize,

    /// Minimum stake (in native token base units) required to register as a
    /// validator.
    #[serde(default = "default_min_stake")]
    pub min_stake: u64,

    /// Maximum number of active validators in the validator set.
    #[serde(default = "default_max_validators")]
    pub max_validators: usize,

    /// Number of blocks a validator must wait after unbonding before their
    /// stake is returned (~8 hours at 2 s blocks).
    #[serde(default = "default_unbonding_period_blocks")]
    pub unbonding_period_blocks: u64,

    /// Slash fraction for downtime, in basis points (1 % = 100 bps).
    #[serde(default = "default_slash_fraction_downtime_bps")]
    pub slash_fraction_downtime_bps: u16,

    /// Slash fraction for double signing, in basis points.
    #[serde(default = "default_slash_fraction_double_sign_bps")]
    pub slash_fraction_double_sign_bps: u16,

    /// Protocol fee taken from marketplace trades, in basis points (2.5 %).
    #[serde(default = "default_protocol_fee_bps")]
    pub protocol_fee_bps: u16,

    /// Maximum commission a validator can charge, in basis points (20 %).
    #[serde(default = "default_max_commission_bps")]
    pub max_commission_bps: u16,

    /// Length of an epoch in blocks (~4 hours at 2 s blocks).
    #[serde(default = "default_epoch_length")]
    pub epoch_length: u64,

    /// Quorum threshold for consensus in basis points (2/3 + 1).
    #[serde(default = "default_quorum_threshold_bps")]
    pub quorum_threshold_bps: u16,

    /// Anti-cheat score below which a player is quarantined.
    #[serde(default = "default_attestation_quarantine_threshold")]
    pub attestation_quarantine_threshold: u8,

    /// Base gas cost per transaction (overhead for signature verification, state access).
    #[serde(default = "default_base_gas")]
    pub base_gas: u64,

    /// Gas per byte of transaction data.
    #[serde(default = "default_gas_per_byte")]
    pub gas_per_byte: u64,

    /// Minimum gas price in POL sub-units (1 POL = 1_000_000 sub-units).
    #[serde(default = "default_min_gas_price")]
    pub min_gas_price: u64,

    /// Maximum gas per block.
    #[serde(default = "default_max_block_gas")]
    pub max_block_gas: u64,

    /// Minimum deposit (in native tokens) required to submit a governance proposal.
    #[serde(default = "default_min_proposal_deposit")]
    pub min_proposal_deposit: u64,

    /// Number of blocks the voting period lasts (~8 hours at 2s blocks).
    #[serde(default = "default_voting_period_blocks")]
    pub voting_period_blocks: u64,

    /// Quorum in basis points: fraction of total stake that must vote for a
    /// proposal to be valid (3333 = 33.33%).
    #[serde(default = "default_governance_quorum_bps")]
    pub governance_quorum_bps: u16,

    /// Pass threshold in basis points: fraction of (yes / (yes + no)) required
    /// to pass (5000 = 50%).
    #[serde(default = "default_pass_threshold_bps")]
    pub pass_threshold_bps: u16,

    /// Enable parallel transaction execution.  When `false` (the default),
    /// block production uses the sequential executor.  Set to `true` to
    /// activate the parallel scheduler.
    #[serde(default)]
    pub parallel_execution: bool,

    // -- Economics settings --------------------------------------------------

    /// Fee distribution configuration (50% burn, 20% treasury, 30% validators).
    #[serde(default)]
    pub fee_distribution: FeeDistribution,

    /// Inflation parameters (8% initial, 2% floor, 5% decay).
    #[serde(default)]
    pub inflation_params: InflationParams,

    /// Hex-encoded treasury address (defaults to Address::ZERO).
    #[serde(default = "default_treasury_address")]
    pub treasury_address: String,

    // -- Security settings ---------------------------------------------------

    /// Maximum age of a transaction in seconds before it is rejected.
    #[serde(default = "default_tx_max_age_seconds")]
    pub tx_max_age_seconds: u64,

    /// Maximum allowed nonce gap per sender in the mempool.
    #[serde(default = "default_max_nonce_gap")]
    pub max_nonce_gap: u64,

    /// Maximum RPC transaction submissions per second (global).
    #[serde(default = "default_rpc_max_submissions_per_second")]
    pub rpc_max_submissions_per_second: u32,

    /// Maximum P2P message size in bytes (10 MB default).
    #[serde(default = "default_max_p2p_message_size")]
    pub max_p2p_message_size: usize,
}

// -- serde defaults ----------------------------------------------------------

fn default_native_denom() -> String {
    "POL".to_string()
}
fn default_block_time_ms() -> u64 {
    2000
}
fn default_max_block_transactions() -> usize {
    10_000
}
fn default_max_transaction_size_bytes() -> usize {
    65_536
}
fn default_min_stake() -> u64 {
    1_000_000
}
fn default_max_validators() -> usize {
    100
}
fn default_unbonding_period_blocks() -> u64 {
    14_400
}
fn default_slash_fraction_downtime_bps() -> u16 {
    100
}
fn default_slash_fraction_double_sign_bps() -> u16 {
    500
}
fn default_protocol_fee_bps() -> u16 {
    250
}
fn default_max_commission_bps() -> u16 {
    2000
}
fn default_epoch_length() -> u64 {
    7200
}
fn default_quorum_threshold_bps() -> u16 {
    6667
}
fn default_attestation_quarantine_threshold() -> u8 {
    30
}
fn default_base_gas() -> u64 {
    21_000
}
fn default_gas_per_byte() -> u64 {
    16
}
fn default_min_gas_price() -> u64 {
    1
}
fn default_max_block_gas() -> u64 {
    100_000_000
}
fn default_min_proposal_deposit() -> u64 {
    100_000
}
fn default_voting_period_blocks() -> u64 {
    14_400
}
fn default_governance_quorum_bps() -> u16 {
    3333
}
fn default_pass_threshold_bps() -> u16 {
    5000
}
fn default_tx_max_age_seconds() -> u64 {
    300
}
fn default_max_nonce_gap() -> u64 {
    16
}
fn default_rpc_max_submissions_per_second() -> u32 {
    100
}
fn default_max_p2p_message_size() -> usize {
    10_485_760
}
fn default_treasury_address() -> String {
    "00".repeat(32) // Address::ZERO as hex
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_id: "polay-devnet-1".to_string(),
            chain_name: "POLAY Devnet".to_string(),
            native_denom: default_native_denom(),
            block_time_ms: default_block_time_ms(),
            max_block_transactions: default_max_block_transactions(),
            max_transaction_size_bytes: default_max_transaction_size_bytes(),
            min_stake: default_min_stake(),
            max_validators: default_max_validators(),
            unbonding_period_blocks: default_unbonding_period_blocks(),
            slash_fraction_downtime_bps: default_slash_fraction_downtime_bps(),
            slash_fraction_double_sign_bps: default_slash_fraction_double_sign_bps(),
            protocol_fee_bps: default_protocol_fee_bps(),
            max_commission_bps: default_max_commission_bps(),
            epoch_length: default_epoch_length(),
            quorum_threshold_bps: default_quorum_threshold_bps(),
            attestation_quarantine_threshold: default_attestation_quarantine_threshold(),
            base_gas: default_base_gas(),
            gas_per_byte: default_gas_per_byte(),
            min_gas_price: default_min_gas_price(),
            max_block_gas: default_max_block_gas(),
            min_proposal_deposit: default_min_proposal_deposit(),
            voting_period_blocks: default_voting_period_blocks(),
            governance_quorum_bps: default_governance_quorum_bps(),
            pass_threshold_bps: default_pass_threshold_bps(),
            parallel_execution: false,
            fee_distribution: FeeDistribution::default(),
            inflation_params: InflationParams::default(),
            treasury_address: default_treasury_address(),
            tx_max_age_seconds: default_tx_max_age_seconds(),
            max_nonce_gap: default_max_nonce_gap(),
            rpc_max_submissions_per_second: default_rpc_max_submissions_per_second(),
            max_p2p_message_size: default_max_p2p_message_size(),
        }
    }
}

// ---------------------------------------------------------------------------
// Network profile factory methods & validation
// ---------------------------------------------------------------------------

impl ChainConfig {
    /// Create a devnet configuration (same as `Default`).
    pub fn devnet() -> Self {
        Self::default()
    }

    /// Create a testnet configuration with more realistic parameters.
    pub fn testnet() -> Self {
        Self {
            chain_id: "polay-testnet-1".to_string(),
            chain_name: "POLAY Testnet".to_string(),
            block_time_ms: 3000,
            max_validators: 50,
            min_stake: 10_000_000,
            unbonding_period_blocks: 100_800,
            epoch_length: 28_800,
            voting_period_blocks: 100_800,
            min_proposal_deposit: 1_000_000,
            ..Self::default()
        }
    }

    /// Create a mainnet configuration with conservative, production-grade
    /// parameters.
    pub fn mainnet() -> Self {
        Self {
            chain_id: "polay-mainnet-1".to_string(),
            chain_name: "POLAY Mainnet".to_string(),
            block_time_ms: 4000,
            max_block_transactions: 5_000,
            max_validators: 100,
            min_stake: 100_000_000,
            unbonding_period_blocks: 604_800,
            epoch_length: 21_600,
            slash_fraction_double_sign_bps: 1000,
            voting_period_blocks: 302_400,
            min_proposal_deposit: 10_000_000,
            governance_quorum_bps: 4000,
            pass_threshold_bps: 6000,
            rpc_max_submissions_per_second: 50,
            ..Self::default()
        }
    }

    /// Validate the configuration, returning a list of all detected errors.
    ///
    /// Returns `Ok(())` when the configuration is sound, or `Err(errors)`
    /// with every problem found.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.chain_id.is_empty() {
            errors.push("chain_id cannot be empty".into());
        }
        if self.block_time_ms == 0 {
            errors.push("block_time_ms must be > 0".into());
        }
        if self.max_block_transactions == 0 {
            errors.push("max_block_transactions must be > 0".into());
        }
        if self.max_block_gas == 0 {
            errors.push("max_block_gas must be > 0".into());
        }
        if self.epoch_length == 0 {
            errors.push("epoch_length must be > 0".into());
        }
        if self.quorum_threshold_bps > 10_000 {
            errors.push("quorum_threshold_bps must be <= 10000".into());
        }
        if self.quorum_threshold_bps < 5000 {
            errors.push("quorum_threshold_bps should be >= 5000 for BFT safety".into());
        }
        if self.max_validators == 0 {
            errors.push("max_validators must be > 0".into());
        }
        if self.slash_fraction_double_sign_bps > 10_000 {
            errors.push("slash_fraction_double_sign_bps must be <= 10000".into());
        }
        if self.slash_fraction_downtime_bps > 10_000 {
            errors.push("slash_fraction_downtime_bps must be <= 10000".into());
        }
        if self.governance_quorum_bps > 10_000 {
            errors.push("governance_quorum_bps must be <= 10000".into());
        }
        if self.pass_threshold_bps > 10_000 {
            errors.push("pass_threshold_bps must be <= 10000".into());
        }
        if self.max_commission_bps > 10_000 {
            errors.push("max_commission_bps must be <= 10000".into());
        }
        if self.unbonding_period_blocks == 0 {
            errors.push("unbonding_period_blocks must be > 0".into());
        }
        if self.min_stake == 0 {
            errors.push("min_stake must be > 0".into());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// ---------------------------------------------------------------------------
// NodeConfig
// ---------------------------------------------------------------------------

/// Per-node configuration that varies across operators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Directory used for chain state, WAL, and other persistent data.
    pub data_dir: String,

    /// Address the JSON-RPC server listens on.
    #[serde(default = "default_rpc_addr")]
    pub rpc_addr: String,

    /// Address the libp2p transport listens on.
    #[serde(default = "default_p2p_addr")]
    pub p2p_addr: String,

    /// Path to the validator key file. `None` means the node runs in
    /// full-node (non-validating) mode.
    #[serde(default)]
    pub validator_key_path: Option<String>,

    /// Log verbosity level.
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Path to the genesis JSON file.
    pub genesis_path: String,

    /// Human-readable node name for telemetry / peer identification.
    pub node_name: String,

    /// Multiaddrs of bootstrap peers to connect to on startup.
    #[serde(default)]
    pub boot_nodes: Vec<String>,
}

fn default_rpc_addr() -> String {
    "0.0.0.0:9944".to_string()
}
fn default_p2p_addr() -> String {
    "0.0.0.0:30333".to_string()
}
fn default_log_level() -> String {
    "info".to_string()
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            data_dir: "./polay-data".to_string(),
            rpc_addr: default_rpc_addr(),
            p2p_addr: default_p2p_addr(),
            validator_key_path: None,
            log_level: default_log_level(),
            genesis_path: "./genesis.json".to_string(),
            node_name: "polay-node".to_string(),
            boot_nodes: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Loaders
// ---------------------------------------------------------------------------

/// Load a [`ChainConfig`] from a JSON file at `path`.
pub fn load_chain_config(path: impl AsRef<Path>) -> ConfigResult<ChainConfig> {
    let data = std::fs::read_to_string(path)?;
    let cfg: ChainConfig = serde_json::from_str(&data)?;
    Ok(cfg)
}

/// Load a [`NodeConfig`] from a JSON file at `path`.
pub fn load_node_config(path: impl AsRef<Path>) -> ConfigResult<NodeConfig> {
    let data = std::fs::read_to_string(path)?;
    let cfg: NodeConfig = serde_json::from_str(&data)?;
    Ok(cfg)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn chain_config_defaults() {
        let cfg = ChainConfig::default();
        assert_eq!(cfg.chain_id, "polay-devnet-1");
        assert_eq!(cfg.native_denom, "POL");
        assert_eq!(cfg.block_time_ms, 2000);
        assert_eq!(cfg.max_block_transactions, 10_000);
        assert_eq!(cfg.max_transaction_size_bytes, 65_536);
        assert_eq!(cfg.min_stake, 1_000_000);
        assert_eq!(cfg.max_validators, 100);
        assert_eq!(cfg.unbonding_period_blocks, 14_400);
        assert_eq!(cfg.slash_fraction_downtime_bps, 100);
        assert_eq!(cfg.slash_fraction_double_sign_bps, 500);
        assert_eq!(cfg.protocol_fee_bps, 250);
        assert_eq!(cfg.max_commission_bps, 2000);
        assert_eq!(cfg.epoch_length, 7200);
        assert_eq!(cfg.quorum_threshold_bps, 6667);
        assert_eq!(cfg.attestation_quarantine_threshold, 30);
        assert_eq!(cfg.base_gas, 21_000);
        assert_eq!(cfg.gas_per_byte, 16);
        assert_eq!(cfg.min_gas_price, 1);
        assert_eq!(cfg.max_block_gas, 100_000_000);
        assert_eq!(cfg.min_proposal_deposit, 100_000);
        assert_eq!(cfg.voting_period_blocks, 14_400);
        assert_eq!(cfg.governance_quorum_bps, 3333);
        assert_eq!(cfg.pass_threshold_bps, 5000);
    }

    #[test]
    fn node_config_defaults() {
        let cfg = NodeConfig::default();
        assert_eq!(cfg.rpc_addr, "0.0.0.0:9944");
        assert_eq!(cfg.p2p_addr, "0.0.0.0:30333");
        assert_eq!(cfg.log_level, "info");
        assert!(cfg.validator_key_path.is_none());
        assert!(cfg.boot_nodes.is_empty());
    }

    #[test]
    fn chain_config_serde_round_trip() {
        let cfg = ChainConfig::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let parsed: ChainConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn node_config_serde_round_trip() {
        let cfg = NodeConfig::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let parsed: NodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn chain_config_partial_json_fills_defaults() {
        let json = r#"{"chain_id":"test-1","chain_name":"Test"}"#;
        let cfg: ChainConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.chain_id, "test-1");
        assert_eq!(cfg.native_denom, "POL");
        assert_eq!(cfg.block_time_ms, 2000);
    }

    #[test]
    fn load_chain_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chain.json");
        let cfg = ChainConfig::default();
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(serde_json::to_string_pretty(&cfg).unwrap().as_bytes())
            .unwrap();
        let loaded = load_chain_config(&path).unwrap();
        assert_eq!(cfg, loaded);
    }

    #[test]
    fn load_node_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("node.json");
        let cfg = NodeConfig::default();
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(serde_json::to_string_pretty(&cfg).unwrap().as_bytes())
            .unwrap();
        let loaded = load_node_config(&path).unwrap();
        assert_eq!(cfg, loaded);
    }

    #[test]
    fn load_missing_file_gives_io_error() {
        let res = load_chain_config("/tmp/__nonexistent_polay_test__");
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), ConfigError::IoError(_)));
    }

    #[test]
    fn load_bad_json_gives_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not json").unwrap();
        let res = load_chain_config(&path);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), ConfigError::ParseError(_)));
    }

    // -- Network profile tests -----------------------------------------------

    #[test]
    fn profiles_have_distinct_chain_ids() {
        let devnet = ChainConfig::devnet();
        let testnet = ChainConfig::testnet();
        let mainnet = ChainConfig::mainnet();

        assert_ne!(devnet.chain_id, testnet.chain_id);
        assert_ne!(devnet.chain_id, mainnet.chain_id);
        assert_ne!(testnet.chain_id, mainnet.chain_id);
    }

    #[test]
    fn testnet_has_different_parameters_than_devnet() {
        let devnet = ChainConfig::devnet();
        let testnet = ChainConfig::testnet();

        assert_ne!(devnet.block_time_ms, testnet.block_time_ms);
        assert!(testnet.min_stake > devnet.min_stake);
        assert!(testnet.epoch_length > devnet.epoch_length);
    }

    #[test]
    fn mainnet_has_conservative_parameters() {
        let mainnet = ChainConfig::mainnet();

        assert_eq!(mainnet.chain_id, "polay-mainnet-1");
        assert_eq!(mainnet.block_time_ms, 4000);
        assert_eq!(mainnet.max_block_transactions, 5_000);
        assert_eq!(mainnet.min_stake, 100_000_000);
        assert_eq!(mainnet.slash_fraction_double_sign_bps, 1000);
        assert_eq!(mainnet.rpc_max_submissions_per_second, 50);
    }

    #[test]
    fn devnet_factory_matches_default() {
        assert_eq!(ChainConfig::devnet(), ChainConfig::default());
    }

    // -- Config validation tests ---------------------------------------------

    #[test]
    fn validation_passes_for_all_profiles() {
        ChainConfig::devnet().validate().expect("devnet should validate");
        ChainConfig::testnet().validate().expect("testnet should validate");
        ChainConfig::mainnet().validate().expect("mainnet should validate");
    }

    #[test]
    fn validation_catches_empty_chain_id() {
        let mut cfg = ChainConfig::default();
        cfg.chain_id = String::new();
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("chain_id")));
    }

    #[test]
    fn validation_catches_zero_epoch_length() {
        let mut cfg = ChainConfig::default();
        cfg.epoch_length = 0;
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("epoch_length")));
    }

    #[test]
    fn validation_catches_zero_block_time() {
        let mut cfg = ChainConfig::default();
        cfg.block_time_ms = 0;
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("block_time_ms")));
    }

    #[test]
    fn validation_catches_bps_over_10000() {
        let mut cfg = ChainConfig::default();
        cfg.slash_fraction_double_sign_bps = 10_001;
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("slash_fraction_double_sign_bps")));
    }

    #[test]
    fn validation_catches_governance_bps_over_10000() {
        let mut cfg = ChainConfig::default();
        cfg.governance_quorum_bps = 10_001;
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("governance_quorum_bps")));
    }

    #[test]
    fn validation_catches_zero_min_stake() {
        let mut cfg = ChainConfig::default();
        cfg.min_stake = 0;
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("min_stake")));
    }

    #[test]
    fn validation_catches_zero_max_validators() {
        let mut cfg = ChainConfig::default();
        cfg.max_validators = 0;
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("max_validators")));
    }

    #[test]
    fn validation_collects_multiple_errors() {
        let mut cfg = ChainConfig::default();
        cfg.chain_id = String::new();
        cfg.block_time_ms = 0;
        cfg.epoch_length = 0;
        let errs = cfg.validate().unwrap_err();
        assert!(errs.len() >= 3, "expected at least 3 errors, got {}", errs.len());
    }
}
