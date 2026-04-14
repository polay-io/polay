use std::sync::Arc;

use tracing::{debug, info};

use polay_config::ChainConfig;
use polay_genesis::{Genesis, GenesisAttestor, GenesisValidator};
use polay_state::{StateStore, StateView, StateWriter};
use polay_types::address::Address;
use polay_types::block::Block;
use polay_types::hash::Hash;
use polay_types::transaction::{TransactionReceipt, TxLocation};
use polay_types::{AccountState, Attestor, AttestorStatus, ValidatorInfo};

use crate::error::{ValidatorError, ValidatorResult};

// ---------------------------------------------------------------------------
// ChainState
// ---------------------------------------------------------------------------

/// Manages chain-level metadata and applies committed blocks to the store.
pub struct ChainState {
    /// The underlying key-value state store.
    store: Arc<dyn StateStore>,
    /// Chain-wide configuration parameters.
    chain_config: ChainConfig,
}

impl ChainState {
    /// Create a new `ChainState` backed by the given store.
    pub fn new(store: Arc<dyn StateStore>, chain_config: ChainConfig) -> Self {
        Self {
            store,
            chain_config,
        }
    }

    /// Return a reference to the underlying store.
    pub fn store(&self) -> &dyn StateStore {
        self.store.as_ref()
    }

    /// Return a clone of the `Arc` store for shared ownership.
    pub fn store_arc(&self) -> Arc<dyn StateStore> {
        Arc::clone(&self.store)
    }

    /// Return a reference to the chain configuration.
    pub fn chain_config(&self) -> &ChainConfig {
        &self.chain_config
    }

    // -- Genesis initialization ---------------------------------------------

    /// Initialize the chain state from a genesis document.
    ///
    /// This creates:
    /// - Funded accounts for every genesis account.
    /// - `ValidatorInfo` entries for every genesis validator.
    /// - `Attestor` entries for every genesis attestor.
    /// - Sets chain height to 0 and latest hash to `Hash::ZERO`.
    ///
    /// This method is idempotent: if the chain height is already > 0 it
    /// returns immediately without modifying state.
    pub fn init_from_genesis(&self, genesis: &Genesis) -> ValidatorResult<()> {
        let view = StateView::new(self.store.as_ref());
        let current_height = view.get_chain_height()?;

        if current_height > 0 {
            info!(
                current_height,
                "chain already initialized, skipping genesis"
            );
            return Ok(());
        }

        let writer = StateWriter::new(self.store.as_ref());

        // Create funded accounts.
        for ga in &genesis.accounts {
            let addr = parse_address(&ga.address)?;
            let account = AccountState::with_balance(addr, ga.balance, 0);
            writer.set_account(&account)?;
            debug!(address = %addr, balance = ga.balance, "created genesis account");
        }

        // Create validators.
        for gv in &genesis.validators {
            self.create_genesis_validator(gv)?;
        }

        // Create attestors.
        for ga in &genesis.attestors {
            self.create_genesis_attestor(ga)?;
        }

        // Set initial chain metadata.
        writer.set_chain_height(0)?;
        writer.set_latest_hash(&Hash::ZERO)?;

        // Initialize supply tracking.
        let total_staked: u64 = genesis.validators.iter().map(|v| v.stake).sum();
        let supply = polay_types::SupplyInfo {
            total_supply: genesis.initial_supply,
            circulating_supply: genesis.initial_supply.saturating_sub(total_staked),
            total_staked,
            total_burned: 0,
            treasury_balance: 0,
            total_minted: 0,
            total_fees_collected: 0,
        };
        writer.set_supply_info(&supply)?;

        info!(
            chain_id = %self.chain_config.chain_id,
            accounts = genesis.accounts.len(),
            validators = genesis.validators.len(),
            attestors = genesis.attestors.len(),
            initial_supply = genesis.initial_supply,
            total_staked,
            "chain initialized from genesis"
        );

        Ok(())
    }

    /// Create a single genesis validator.
    fn create_genesis_validator(&self, gv: &GenesisValidator) -> ValidatorResult<()> {
        let addr = parse_address(&gv.address)?;
        let writer = StateWriter::new(self.store.as_ref());

        let mut info = ValidatorInfo::new(addr, gv.commission_bps);
        info.stake = gv.stake;
        writer.set_validator(&info)?;

        debug!(
            address = %addr,
            stake = gv.stake,
            commission_bps = gv.commission_bps,
            "created genesis validator"
        );
        Ok(())
    }

    /// Create a single genesis attestor.
    fn create_genesis_attestor(&self, ga: &GenesisAttestor) -> ValidatorResult<()> {
        let addr = parse_address(&ga.address)?;
        let writer = StateWriter::new(self.store.as_ref());

        let attestor = Attestor {
            address: addr,
            game_id: ga.game_id.clone(),
            endpoint: ga.endpoint.clone(),
            metadata: String::new(),
            status: AttestorStatus::Active,
            registered_at: 0,
        };
        writer.set_attestor(&attestor)?;

        debug!(
            address = %addr,
            game_id = %ga.game_id,
            "created genesis attestor"
        );
        Ok(())
    }

    // -- Block application --------------------------------------------------

    /// Apply a committed block and its receipts to the state store.
    ///
    /// Updates:
    /// - Stores the block keyed by height.
    /// - Stores each transaction receipt keyed by tx_hash.
    /// - Stores a TxLocation for each transaction.
    /// - Aggregates and stores all events emitted in the block.
    /// - Updates the chain height.
    /// - Updates the latest block hash.
    pub fn apply_block(
        &self,
        block: &Block,
        receipts: &[TransactionReceipt],
    ) -> ValidatorResult<()> {
        let writer = StateWriter::new(self.store.as_ref());

        // Store block.
        writer.store_block(block)?;

        // Store each receipt and tx location.
        let mut all_events = Vec::new();
        for (idx, receipt) in receipts.iter().enumerate() {
            writer.set_receipt(receipt)?;
            writer.set_tx_location(
                &receipt.tx_hash,
                &TxLocation {
                    block_height: block.header.height,
                    tx_index: idx as u32,
                },
            )?;
            all_events.extend(receipt.events.clone());
        }

        // Store block events.
        if !all_events.is_empty() {
            writer.set_block_events(block.header.height, &all_events)?;
        }

        // Update chain metadata.
        writer.set_chain_height(block.height())?;
        writer.set_latest_hash(block.hash())?;

        debug!(
            height = block.height(),
            hash = %block.hash(),
            receipts = receipts.len(),
            events = all_events.len(),
            "block applied to chain state"
        );

        Ok(())
    }

    // -- Queries ------------------------------------------------------------

    /// Return the current chain height (0 if pre-genesis or at genesis).
    pub fn get_height(&self) -> ValidatorResult<u64> {
        let view = StateView::new(self.store.as_ref());
        Ok(view.get_chain_height()?)
    }

    /// Return the hash of the most recently committed block.
    pub fn get_latest_hash(&self) -> ValidatorResult<Hash> {
        let view = StateView::new(self.store.as_ref());
        Ok(view.get_latest_hash()?)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a hex-encoded address string into an `Address`.
fn parse_address(hex_str: &str) -> ValidatorResult<Address> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| ValidatorError::Other(format!("invalid address hex '{}': {}", hex_str, e)))?;
    if bytes.len() != 32 {
        return Err(ValidatorError::Other(format!(
            "address must be 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(Address::new(arr))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::MemoryStore;

    #[test]
    fn init_from_genesis_creates_accounts_and_metadata() {
        let store = Arc::new(MemoryStore::new());
        let genesis = Genesis::generate_devnet();
        let chain = ChainState::new(store.clone(), genesis.chain_config.clone());

        chain.init_from_genesis(&genesis).unwrap();

        let view = StateView::new(store.as_ref());
        assert_eq!(view.get_chain_height().unwrap(), 0);
        assert_eq!(view.get_latest_hash().unwrap(), Hash::ZERO);

        // Check that at least one genesis account exists.
        let addr = parse_address(&genesis.accounts[0].address).unwrap();
        let acct = view.get_account(&addr).unwrap().unwrap();
        assert_eq!(acct.balance, genesis.accounts[0].balance);
    }

    #[test]
    fn init_from_genesis_creates_validators() {
        let store = Arc::new(MemoryStore::new());
        let genesis = Genesis::generate_devnet();
        let chain = ChainState::new(store.clone(), genesis.chain_config.clone());

        chain.init_from_genesis(&genesis).unwrap();

        let view = StateView::new(store.as_ref());
        let addr = parse_address(&genesis.validators[0].address).unwrap();
        let v = view.get_validator(&addr).unwrap().unwrap();
        assert_eq!(v.stake, genesis.validators[0].stake);
    }

    #[test]
    fn init_from_genesis_is_idempotent() {
        let store = Arc::new(MemoryStore::new());
        let genesis = Genesis::generate_devnet();
        let chain = ChainState::new(store.clone(), genesis.chain_config.clone());

        chain.init_from_genesis(&genesis).unwrap();

        // Bump height to simulate an already-running chain.
        StateWriter::new(store.as_ref())
            .set_chain_height(5)
            .unwrap();

        // Second call should be a no-op (height > 0).
        chain.init_from_genesis(&genesis).unwrap();
        assert_eq!(chain.get_height().unwrap(), 5);
    }

    #[test]
    fn apply_block_updates_metadata() {
        let store = Arc::new(MemoryStore::new());
        let config = ChainConfig::default();
        let chain = ChainState::new(store.clone(), config.clone());

        // Initialize.
        let genesis = Genesis::generate_devnet();
        chain.init_from_genesis(&genesis).unwrap();

        // Build a minimal block.
        let block = polay_consensus::BlockProposer::propose_block(
            1,
            0,
            Hash::ZERO,
            Hash::ZERO,
            vec![],
            config.chain_id.clone(),
            Address::ZERO,
            1_700_000_000,
        );

        chain.apply_block(&block, &[]).unwrap();

        assert_eq!(chain.get_height().unwrap(), 1);
        assert_eq!(chain.get_latest_hash().unwrap(), *block.hash());
    }

    #[test]
    fn apply_block_stores_receipts() {
        let store = Arc::new(MemoryStore::new());
        let config = ChainConfig::default();
        let chain = ChainState::new(store.clone(), config.clone());

        let genesis = Genesis::generate_devnet();
        chain.init_from_genesis(&genesis).unwrap();

        let block = polay_consensus::BlockProposer::propose_block(
            1,
            0,
            Hash::ZERO,
            Hash::ZERO,
            vec![],
            config.chain_id.clone(),
            Address::ZERO,
            1_700_000_000,
        );

        let tx_hash_1 = Hash::new([0x11; 32]);
        let tx_hash_2 = Hash::new([0x22; 32]);
        let receipts = vec![
            TransactionReceipt::success(
                tx_hash_1,
                1,
                500,
                21000,
                Address::ZERO,
                vec![polay_types::Event::new(
                    "bank",
                    "transfer",
                    vec![("amount".into(), "100".into())],
                )],
            ),
            TransactionReceipt::failure(
                tx_hash_2,
                1,
                200,
                10000,
                Address::ZERO,
                "some error".into(),
            ),
        ];

        chain.apply_block(&block, &receipts).unwrap();

        let view = StateView::new(store.as_ref());

        // Receipt 1 should be stored.
        let r1 = view.get_receipt(&tx_hash_1).unwrap().unwrap();
        assert!(r1.success);
        assert_eq!(r1.gas_used, 21000);
        assert_eq!(r1.events.len(), 1);

        // Receipt 2 should be stored.
        let r2 = view.get_receipt(&tx_hash_2).unwrap().unwrap();
        assert!(!r2.success);
        assert_eq!(r2.error.as_deref(), Some("some error"));
    }

    #[test]
    fn apply_block_stores_tx_locations() {
        let store = Arc::new(MemoryStore::new());
        let config = ChainConfig::default();
        let chain = ChainState::new(store.clone(), config.clone());

        let genesis = Genesis::generate_devnet();
        chain.init_from_genesis(&genesis).unwrap();

        let block = polay_consensus::BlockProposer::propose_block(
            5,
            0,
            Hash::ZERO,
            Hash::ZERO,
            vec![],
            config.chain_id.clone(),
            Address::ZERO,
            1_700_000_000,
        );

        let tx_hash_a = Hash::new([0xAA; 32]);
        let tx_hash_b = Hash::new([0xBB; 32]);
        let tx_hash_c = Hash::new([0xCC; 32]);
        let receipts = vec![
            TransactionReceipt::success(tx_hash_a, 5, 100, 1000, Address::ZERO, vec![]),
            TransactionReceipt::success(tx_hash_b, 5, 200, 2000, Address::ZERO, vec![]),
            TransactionReceipt::success(tx_hash_c, 5, 300, 3000, Address::ZERO, vec![]),
        ];

        chain.apply_block(&block, &receipts).unwrap();

        let view = StateView::new(store.as_ref());

        let loc_a = view.get_tx_location(&tx_hash_a).unwrap().unwrap();
        assert_eq!(loc_a.block_height, 5);
        assert_eq!(loc_a.tx_index, 0);

        let loc_b = view.get_tx_location(&tx_hash_b).unwrap().unwrap();
        assert_eq!(loc_b.block_height, 5);
        assert_eq!(loc_b.tx_index, 1);

        let loc_c = view.get_tx_location(&tx_hash_c).unwrap().unwrap();
        assert_eq!(loc_c.block_height, 5);
        assert_eq!(loc_c.tx_index, 2);
    }

    #[test]
    fn apply_block_stores_block_events() {
        let store = Arc::new(MemoryStore::new());
        let config = ChainConfig::default();
        let chain = ChainState::new(store.clone(), config.clone());

        let genesis = Genesis::generate_devnet();
        chain.init_from_genesis(&genesis).unwrap();

        let block = polay_consensus::BlockProposer::propose_block(
            3,
            0,
            Hash::ZERO,
            Hash::ZERO,
            vec![],
            config.chain_id.clone(),
            Address::ZERO,
            1_700_000_000,
        );

        let evt1 =
            polay_types::Event::new("bank", "transfer", vec![("amount".into(), "100".into())]);
        let evt2 = polay_types::Event::new("asset", "mint", vec![("amount".into(), "50".into())]);
        let evt3 = polay_types::Event::new("market", "listing_created", vec![]);

        let receipts = vec![
            TransactionReceipt::success(
                Hash::new([0x11; 32]),
                3,
                100,
                1000,
                Address::ZERO,
                vec![evt1.clone(), evt2.clone()],
            ),
            TransactionReceipt::success(
                Hash::new([0x22; 32]),
                3,
                200,
                2000,
                Address::ZERO,
                vec![evt3.clone()],
            ),
        ];

        chain.apply_block(&block, &receipts).unwrap();

        let view = StateView::new(store.as_ref());
        let block_events = view.get_block_events(3).unwrap().unwrap();
        assert_eq!(block_events.len(), 3);
        assert_eq!(block_events[0], evt1);
        assert_eq!(block_events[1], evt2);
        assert_eq!(block_events[2], evt3);
    }

    #[test]
    fn apply_block_no_events_for_empty_block() {
        let store = Arc::new(MemoryStore::new());
        let config = ChainConfig::default();
        let chain = ChainState::new(store.clone(), config.clone());

        let genesis = Genesis::generate_devnet();
        chain.init_from_genesis(&genesis).unwrap();

        let block = polay_consensus::BlockProposer::propose_block(
            1,
            0,
            Hash::ZERO,
            Hash::ZERO,
            vec![],
            config.chain_id.clone(),
            Address::ZERO,
            1_700_000_000,
        );

        chain.apply_block(&block, &[]).unwrap();

        let view = StateView::new(store.as_ref());
        // No block events stored for empty block.
        assert!(view.get_block_events(1).unwrap().is_none());
    }

    #[test]
    fn parse_address_valid() {
        let hex_str = "aa".repeat(32);
        let addr = parse_address(&hex_str).unwrap();
        assert_eq!(addr, Address::new([0xAA; 32]));
    }

    #[test]
    fn parse_address_invalid_hex() {
        assert!(parse_address("not_hex").is_err());
    }

    #[test]
    fn parse_address_wrong_length() {
        let short = "aa".repeat(16);
        assert!(parse_address(&short).is_err());
    }
}
