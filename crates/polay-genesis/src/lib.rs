use std::collections::HashSet;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use polay_config::ChainConfig;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum GenesisError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("validation error: {0}")]
    Validation(String),
}

pub type GenesisResult<T> = Result<T, GenesisError>;

// ---------------------------------------------------------------------------
// Genesis sub-types
// ---------------------------------------------------------------------------

/// An account that exists at genesis with a pre-funded balance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenesisAccount {
    /// Hex-encoded 32-byte address.
    pub address: String,
    /// Balance in base units of the native token (POL).
    pub balance: u64,
}

/// A validator that is part of the initial validator set at genesis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenesisValidator {
    /// Hex-encoded 32-byte address.
    pub address: String,
    /// Hex-encoded Ed25519 public key.
    pub pubkey: String,
    /// Amount of native token staked.
    pub stake: u64,
    /// Commission rate in basis points.
    pub commission_bps: u16,
}

/// An attestor registered at genesis for a specific game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenesisAttestor {
    /// Hex-encoded 32-byte address.
    pub address: String,
    /// Identifier of the game this attestor is authorized for.
    pub game_id: String,
    /// Endpoint (URL) where the attestation service can be reached.
    pub endpoint: String,
}

// ---------------------------------------------------------------------------
// Genesis
// ---------------------------------------------------------------------------

/// Complete genesis state for a POLAY blockchain instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Genesis {
    /// Chain-wide configuration parameters.
    pub chain_config: ChainConfig,

    /// ISO 8601 timestamp of genesis.
    pub genesis_time: String,

    /// Pre-funded accounts.
    pub accounts: Vec<GenesisAccount>,

    /// Initial validator set.
    pub validators: Vec<GenesisValidator>,

    /// Initial attestors.
    pub attestors: Vec<GenesisAttestor>,

    /// Total initial supply (should equal sum of all account balances).
    pub initial_supply: u64,
}

impl Genesis {
    // -- Persistence ---------------------------------------------------------

    /// Load a genesis document from a JSON file.
    pub fn load(path: impl AsRef<Path>) -> GenesisResult<Self> {
        let data = std::fs::read_to_string(path)?;
        let genesis: Genesis = serde_json::from_str(&data)?;
        Ok(genesis)
    }

    /// Write this genesis document to a JSON file.
    pub fn save(&self, path: impl AsRef<Path>) -> GenesisResult<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    // -- Devnet generator ----------------------------------------------------

    /// Generate a sensible devnet genesis with sample validators and accounts.
    ///
    /// Produces:
    /// - 4 validators with 10M POL staked each (40M total validator stake)
    /// - 10 regular accounts with 6M POL each (60M total)
    /// - 2 attestors for sample games
    /// - Initial supply = 100M POL
    pub fn generate_devnet() -> Self {
        let chain_config = ChainConfig::default();

        // Deterministic "random" addresses for reproducible devnets.
        // We use a simple counter-based scheme: SHA-256(prefix || index).
        let make_addr = |prefix: &str, idx: u32| -> String {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(prefix.as_bytes());
            hasher.update(idx.to_le_bytes());
            hex::encode(hasher.finalize())
        };

        let make_pubkey = |idx: u32| -> String {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(b"validator-pubkey");
            hasher.update(idx.to_le_bytes());
            hex::encode(hasher.finalize())
        };

        let validator_stake: u64 = 10_000_000;
        let account_balance: u64 = 6_000_000;

        let validators: Vec<GenesisValidator> = (0..4)
            .map(|i| GenesisValidator {
                address: make_addr("validator", i),
                pubkey: make_pubkey(i),
                stake: validator_stake,
                commission_bps: 500, // 5 %
            })
            .collect();

        // Validator addresses also appear as funded accounts so they can pay
        // for transactions.
        let mut accounts: Vec<GenesisAccount> = validators
            .iter()
            .map(|v| GenesisAccount {
                address: v.address.clone(),
                balance: validator_stake,
            })
            .collect();

        for i in 0..10 {
            accounts.push(GenesisAccount {
                address: make_addr("account", i),
                balance: account_balance,
            });
        }

        let initial_supply: u64 = accounts.iter().map(|a| a.balance).sum();

        let attestors = vec![
            GenesisAttestor {
                address: make_addr("attestor", 0),
                game_id: "game-battle-arena".to_string(),
                endpoint: "http://localhost:8081/attest".to_string(),
            },
            GenesisAttestor {
                address: make_addr("attestor", 1),
                game_id: "game-card-clash".to_string(),
                endpoint: "http://localhost:8082/attest".to_string(),
            },
        ];

        Genesis {
            chain_config,
            genesis_time: Utc::now().to_rfc3339(),
            accounts,
            validators,
            attestors,
            initial_supply,
        }
    }

    // -- Testnet generator ---------------------------------------------------

    /// Generate a testnet genesis with the given number of validators.
    ///
    /// Produces:
    /// - `validator_count` validators with 10M POL staked each
    /// - 20 funded accounts with 5M POL each
    /// - 4 attestors for test games
    /// - Uses `ChainConfig::testnet()`
    pub fn generate_testnet(validator_count: u32) -> Self {
        let chain_config = ChainConfig::testnet();

        let make_addr = |prefix: &str, idx: u32| -> String {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(prefix.as_bytes());
            hasher.update(idx.to_le_bytes());
            hex::encode(hasher.finalize())
        };

        let make_pubkey = |idx: u32| -> String {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(b"validator-pubkey");
            hasher.update(idx.to_le_bytes());
            hex::encode(hasher.finalize())
        };

        let validator_stake: u64 = 10_000_000;
        let account_balance: u64 = 5_000_000;

        let validators: Vec<GenesisValidator> = (0..validator_count)
            .map(|i| GenesisValidator {
                address: make_addr("testnet-validator", i),
                pubkey: make_pubkey(i),
                stake: validator_stake,
                commission_bps: 500,
            })
            .collect();

        // Validator addresses also appear as funded accounts.
        let mut accounts: Vec<GenesisAccount> = validators
            .iter()
            .map(|v| GenesisAccount {
                address: v.address.clone(),
                balance: validator_stake,
            })
            .collect();

        for i in 0..20 {
            accounts.push(GenesisAccount {
                address: make_addr("testnet-account", i),
                balance: account_balance,
            });
        }

        let initial_supply: u64 = accounts.iter().map(|a| a.balance).sum();

        let attestors = vec![
            GenesisAttestor {
                address: make_addr("testnet-attestor", 0),
                game_id: "game-battle-arena".to_string(),
                endpoint: "https://testnet-attestor-1.polay.io/attest".to_string(),
            },
            GenesisAttestor {
                address: make_addr("testnet-attestor", 1),
                game_id: "game-card-clash".to_string(),
                endpoint: "https://testnet-attestor-2.polay.io/attest".to_string(),
            },
            GenesisAttestor {
                address: make_addr("testnet-attestor", 2),
                game_id: "game-rpg-quest".to_string(),
                endpoint: "https://testnet-attestor-3.polay.io/attest".to_string(),
            },
            GenesisAttestor {
                address: make_addr("testnet-attestor", 3),
                game_id: "game-racing-league".to_string(),
                endpoint: "https://testnet-attestor-4.polay.io/attest".to_string(),
            },
        ];

        Genesis {
            chain_config,
            genesis_time: Utc::now().to_rfc3339(),
            accounts,
            validators,
            attestors,
            initial_supply,
        }
    }

    // -- Mainnet generator ---------------------------------------------------

    /// Generate a mainnet genesis from explicit validator and account lists.
    ///
    /// Unlike devnet/testnet generators, mainnet does not create synthetic
    /// addresses. The caller must provide the exact set of validators and
    /// accounts that will be present at launch. This method calculates
    /// `initial_supply` from the provided data and runs full validation before
    /// returning.
    pub fn generate_mainnet(
        validators: Vec<GenesisValidator>,
        accounts: Vec<GenesisAccount>,
    ) -> Result<Self, GenesisError> {
        let chain_config = ChainConfig::mainnet();
        let initial_supply: u64 = accounts.iter().map(|a| a.balance).sum();

        let genesis = Genesis {
            chain_config,
            genesis_time: Utc::now().to_rfc3339(),
            accounts,
            validators,
            attestors: Vec::new(),
            initial_supply,
        };

        genesis.validate()?;
        Ok(genesis)
    }

    // -- Validation ----------------------------------------------------------

    /// Validate the genesis document:
    ///
    /// 1. Sum of all account balances must equal `initial_supply`.
    /// 2. Every validator stake must be >= `chain_config.min_stake`.
    /// 3. Every validator commission must be <= `chain_config.max_commission_bps`.
    /// 4. No duplicate account addresses.
    /// 5. No duplicate validator addresses.
    /// 6. Each validator address must have a corresponding funded account.
    pub fn validate(&self) -> GenesisResult<()> {
        // 1. Balance sum == initial supply
        let balance_sum: u64 = self.accounts.iter().map(|a| a.balance).sum();
        if balance_sum != self.initial_supply {
            return Err(GenesisError::Validation(format!(
                "account balance sum ({}) does not equal initial_supply ({})",
                balance_sum, self.initial_supply,
            )));
        }

        // 2. Validator minimum stake
        for v in &self.validators {
            if v.stake < self.chain_config.min_stake {
                return Err(GenesisError::Validation(format!(
                    "validator {} stake ({}) is below minimum ({})",
                    v.address, v.stake, self.chain_config.min_stake,
                )));
            }
        }

        // 3. Validator commission cap
        for v in &self.validators {
            if v.commission_bps > self.chain_config.max_commission_bps {
                return Err(GenesisError::Validation(format!(
                    "validator {} commission ({} bps) exceeds maximum ({} bps)",
                    v.address, v.commission_bps, self.chain_config.max_commission_bps,
                )));
            }
        }

        // 4. No duplicate account addresses
        let mut seen_accounts = HashSet::new();
        for a in &self.accounts {
            if !seen_accounts.insert(&a.address) {
                return Err(GenesisError::Validation(format!(
                    "duplicate account address: {}",
                    a.address,
                )));
            }
        }

        // 5. No duplicate validator addresses
        let mut seen_validators = HashSet::new();
        for v in &self.validators {
            if !seen_validators.insert(&v.address) {
                return Err(GenesisError::Validation(format!(
                    "duplicate validator address: {}",
                    v.address,
                )));
            }
        }

        // 6. Each validator must have a funded account
        for v in &self.validators {
            if !seen_accounts.contains(&v.address) {
                return Err(GenesisError::Validation(format!(
                    "validator {} has no corresponding account",
                    v.address,
                )));
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devnet_genesis_is_valid() {
        let genesis = Genesis::generate_devnet();
        genesis.validate().expect("devnet genesis should be valid");
    }

    #[test]
    fn devnet_genesis_has_expected_shape() {
        let genesis = Genesis::generate_devnet();
        assert_eq!(genesis.validators.len(), 4);
        // 4 validator accounts + 10 regular accounts = 14
        assert_eq!(genesis.accounts.len(), 14);
        assert_eq!(genesis.attestors.len(), 2);

        let balance_sum: u64 = genesis.accounts.iter().map(|a| a.balance).sum();
        assert_eq!(balance_sum, genesis.initial_supply);
        // 4 * 10M + 10 * 6M = 100M
        assert_eq!(genesis.initial_supply, 100_000_000);
    }

    #[test]
    fn validate_catches_supply_mismatch() {
        let mut genesis = Genesis::generate_devnet();
        genesis.initial_supply += 1;
        let err = genesis.validate().unwrap_err();
        assert!(err.to_string().contains("does not equal initial_supply"));
    }

    #[test]
    fn validate_catches_low_stake() {
        let mut genesis = Genesis::generate_devnet();
        genesis.validators[0].stake = 1; // way below min_stake
        let err = genesis.validate().unwrap_err();
        assert!(err.to_string().contains("below minimum"));
    }

    #[test]
    fn validate_catches_high_commission() {
        let mut genesis = Genesis::generate_devnet();
        genesis.validators[0].commission_bps = 10_000; // 100 %
        let err = genesis.validate().unwrap_err();
        assert!(err.to_string().contains("exceeds maximum"));
    }

    #[test]
    fn validate_catches_duplicate_accounts() {
        let mut genesis = Genesis::generate_devnet();
        let dup = genesis.accounts[0].clone();
        genesis.accounts.push(dup);
        // Fix supply so that check passes before the duplicate check
        genesis.initial_supply = genesis.accounts.iter().map(|a| a.balance).sum();
        let err = genesis.validate().unwrap_err();
        assert!(err.to_string().contains("duplicate account"));
    }

    #[test]
    fn validate_catches_duplicate_validators() {
        let mut genesis = Genesis::generate_devnet();
        let dup = genesis.validators[0].clone();
        genesis.validators.push(dup);
        let err = genesis.validate().unwrap_err();
        assert!(err.to_string().contains("duplicate validator"));
    }

    #[test]
    fn serde_round_trip() {
        let genesis = Genesis::generate_devnet();
        let json = serde_json::to_string_pretty(&genesis).unwrap();
        let parsed: Genesis = serde_json::from_str(&json).unwrap();
        assert_eq!(genesis, parsed);
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("genesis.json");
        let genesis = Genesis::generate_devnet();
        genesis.save(&path).unwrap();
        let loaded = Genesis::load(&path).unwrap();
        assert_eq!(genesis, loaded);
    }

    // -- Testnet genesis tests -----------------------------------------------

    #[test]
    fn testnet_genesis_is_valid() {
        let genesis = Genesis::generate_testnet(4);
        genesis.validate().expect("testnet genesis should be valid");
    }

    #[test]
    fn testnet_genesis_has_correct_chain_id() {
        let genesis = Genesis::generate_testnet(4);
        assert_eq!(genesis.chain_config.chain_id, "polay-testnet-1");
    }

    #[test]
    fn testnet_genesis_has_expected_shape() {
        let validator_count = 6;
        let genesis = Genesis::generate_testnet(validator_count);
        assert_eq!(genesis.validators.len(), validator_count as usize);
        // validator_count validator accounts + 20 regular accounts
        assert_eq!(genesis.accounts.len(), validator_count as usize + 20);
        assert_eq!(genesis.attestors.len(), 4);

        let balance_sum: u64 = genesis.accounts.iter().map(|a| a.balance).sum();
        assert_eq!(balance_sum, genesis.initial_supply);
        // 6 * 10M + 20 * 5M = 160M
        assert_eq!(
            genesis.initial_supply,
            validator_count as u64 * 10_000_000 + 20 * 5_000_000
        );
    }

    #[test]
    fn testnet_has_different_parameters_than_devnet() {
        let devnet = Genesis::generate_devnet();
        let testnet = Genesis::generate_testnet(4);
        assert_ne!(devnet.chain_config.chain_id, testnet.chain_config.chain_id);
        assert_ne!(
            devnet.chain_config.block_time_ms,
            testnet.chain_config.block_time_ms
        );
    }

    // -- Mainnet genesis tests -----------------------------------------------

    #[test]
    fn mainnet_genesis_with_explicit_data_is_valid() {
        use sha2::{Digest, Sha256};
        let make_addr = |prefix: &str, idx: u32| -> String {
            let mut hasher = Sha256::new();
            hasher.update(prefix.as_bytes());
            hasher.update(idx.to_le_bytes());
            hex::encode(hasher.finalize())
        };
        let make_pubkey = |idx: u32| -> String {
            let mut hasher = Sha256::new();
            hasher.update(b"mainnet-pubkey");
            hasher.update(idx.to_le_bytes());
            hex::encode(hasher.finalize())
        };

        let stake = 100_000_000u64;
        let validators: Vec<GenesisValidator> = (0..3)
            .map(|i| GenesisValidator {
                address: make_addr("mainnet-validator", i),
                pubkey: make_pubkey(i),
                stake,
                commission_bps: 500,
            })
            .collect();

        let mut accounts: Vec<GenesisAccount> = validators
            .iter()
            .map(|v| GenesisAccount {
                address: v.address.clone(),
                balance: stake,
            })
            .collect();
        // Add one extra funded account.
        accounts.push(GenesisAccount {
            address: make_addr("mainnet-treasury", 0),
            balance: 500_000_000,
        });

        let genesis = Genesis::generate_mainnet(validators, accounts).unwrap();
        assert_eq!(genesis.chain_config.chain_id, "polay-mainnet-1");
        assert_eq!(genesis.initial_supply, 3 * stake + 500_000_000);
        genesis.validate().expect("mainnet genesis should be valid");
    }

    #[test]
    fn mainnet_genesis_rejects_invalid_data() {
        // Validator with stake below mainnet min_stake (100M).
        let validators = vec![GenesisValidator {
            address: "aa".repeat(32),
            pubkey: "bb".repeat(32),
            stake: 1_000, // way below 100M
            commission_bps: 500,
        }];
        let accounts = vec![GenesisAccount {
            address: "aa".repeat(32),
            balance: 1_000,
        }];
        let result = Genesis::generate_mainnet(validators, accounts);
        assert!(result.is_err());
    }
}
