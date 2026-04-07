//! Epoch management — automatic validator set rotation and reward distribution.
//!
//! At every `epoch_length` blocks, the chain snapshots the staking state,
//! selects the top N validators by stake, activates the new validator set,
//! distributes epoch rewards, and processes jailed validators.

use tracing::{debug, info};

use polay_config::ChainConfig;
use polay_consensus::types::{ValidatorSet, ValidatorWeight};
use polay_staking::{StakingModule, ValidatorInfo, ValidatorStatus};
use polay_state::{StateStore, StateWriter};
use polay_types::address::Address;
use polay_types::epoch::EpochInfo;
use polay_types::event::Event;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur during epoch processing.
#[derive(Debug, thiserror::Error)]
pub enum EpochError {
    /// An error propagated from the state layer.
    #[error("state error: {0}")]
    StateError(#[from] polay_state::StateError),
    /// An error from the staking module.
    #[error("staking error: {0}")]
    StakingError(String),
}

impl From<polay_staking::StakingError> for EpochError {
    fn from(err: polay_staking::StakingError) -> Self {
        EpochError::StakingError(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// EpochManager
// ---------------------------------------------------------------------------

/// Manages epoch transitions for the POLAY blockchain.
///
/// At every `epoch_length` blocks the manager:
/// 1. Processes jailed validators (unjail if jail period elapsed)
/// 2. Selects the top N validators by stake
/// 3. Builds a new `ValidatorSet` for consensus
/// 4. Distributes epoch rewards
/// 5. Stores epoch info and the active validator set
/// 6. Emits epoch transition events
pub struct EpochManager {
    config: ChainConfig,
}

impl EpochManager {
    /// Create a new `EpochManager` with the given chain configuration.
    pub fn new(config: ChainConfig) -> Self {
        Self { config }
    }

    /// Check if the given block height is an epoch boundary.
    ///
    /// Height 0 is never an epoch boundary (it is genesis).
    pub fn is_epoch_boundary(&self, height: u64) -> bool {
        height > 0 && height % self.config.epoch_length == 0
    }

    /// Get the epoch number for a given height.
    pub fn epoch_for_height(&self, height: u64) -> u64 {
        height / self.config.epoch_length
    }

    /// Process an epoch transition.
    ///
    /// Called when `is_epoch_boundary(height)` returns `true`. Returns the new
    /// `ValidatorSet` for consensus and a list of events emitted during the
    /// transition.
    pub fn process_epoch_transition(
        &self,
        height: u64,
        store: &dyn StateStore,
    ) -> Result<(ValidatorSet, Vec<Event>), EpochError> {
        let writer = StateWriter::new(store);
        let epoch = self.epoch_for_height(height);
        let mut events = Vec::new();

        info!(epoch, height, "processing epoch transition");

        // 1. Get all registered validators from state.
        let all_validators = StakingModule::get_validator_set(store)?;

        // 2. Process jailed validators -- unjail those whose jail period has
        //    elapsed. We use `height` as the time reference (matching how
        //    process_slashing jails validators using height-based offsets).
        for v in &all_validators {
            if v.status == ValidatorStatus::Jailed {
                if let Some(jailed_until) = v.jailed_until {
                    if height >= jailed_until {
                        let mut updated = v.clone();
                        updated.status = ValidatorStatus::Active;
                        updated.jailed_until = None;
                        writer.set_validator(&updated)?;
                        events.push(Event::validator_unjailed(&v.address));
                        debug!(
                            validator = %v.address,
                            jailed_until,
                            height,
                            "validator unjailed at epoch boundary"
                        );
                    }
                }
            }
        }

        // 3. Re-read validators after unjailing so the filter below sees
        //    the updated statuses.
        let all_validators = StakingModule::get_validator_set(store)?;

        // 4. Filter to Active validators with stake >= min_stake.
        let mut eligible: Vec<&ValidatorInfo> = all_validators
            .iter()
            .filter(|v| v.status == ValidatorStatus::Active && v.stake >= self.config.min_stake)
            .collect();

        // 5. Sort by stake descending, take top max_validators.
        eligible.sort_by(|a, b| b.stake.cmp(&a.stake));
        eligible.truncate(self.config.max_validators);

        // 6. Build the new ValidatorSet for consensus.
        let validator_weights: Vec<ValidatorWeight> = eligible
            .iter()
            .map(|v| ValidatorWeight {
                address: v.address,
                stake: v.stake,
            })
            .collect();
        let new_set = ValidatorSet::new(validator_weights);
        let total_staked = new_set.total_stake;

        // 7. Store the active validator set.
        let active_addrs: Vec<Address> = eligible.iter().map(|v| v.address).collect();
        writer.set_active_validator_set(&active_addrs)?;

        events.push(Event::validator_set_updated(epoch, active_addrs.len()));

        // 8. Distribute epoch rewards to active validators.
        let active_validators: Vec<ValidatorInfo> =
            eligible.iter().map(|v| (*v).clone()).collect();
        let rewards = StakingModule::distribute_epoch_rewards(store, &active_validators, epoch, &self.config)?;
        let total_rewards: u64 = rewards.iter().map(|(_, amt)| amt).sum();

        // 9. Store epoch info.
        let epoch_info = EpochInfo {
            epoch,
            start_height: epoch * self.config.epoch_length,
            end_height: (epoch + 1) * self.config.epoch_length - 1,
            validator_set: active_addrs.clone(),
            total_staked,
            rewards_distributed: total_rewards,
        };
        writer.set_epoch_info(&epoch_info)?;

        // 10. Emit the epoch transition event.
        events.push(Event::epoch_transition(
            epoch,
            active_addrs.len(),
            total_staked,
            total_rewards,
        ));

        info!(
            epoch,
            validators = active_addrs.len(),
            total_staked,
            total_rewards,
            "epoch transition complete"
        );

        Ok((new_set, events))
    }

    /// Initialize the validator set from genesis (epoch 0).
    ///
    /// Stores the genesis validators as the active set and returns the
    /// `ValidatorSet` for consensus.
    pub fn init_from_genesis(
        &self,
        genesis_validators: &[(Address, u64)],
        store: &dyn StateStore,
    ) -> Result<ValidatorSet, EpochError> {
        let weights: Vec<ValidatorWeight> = genesis_validators
            .iter()
            .map(|(addr, stake)| ValidatorWeight {
                address: *addr,
                stake: *stake,
            })
            .collect();
        let set = ValidatorSet::new(weights);

        let writer = StateWriter::new(store);
        let active: Vec<Address> = genesis_validators.iter().map(|(a, _)| *a).collect();
        writer.set_active_validator_set(&active)?;

        let total_staked = set.total_stake;
        let epoch_info = EpochInfo {
            epoch: 0,
            start_height: 0,
            end_height: self.config.epoch_length - 1,
            validator_set: active,
            total_staked,
            rewards_distributed: 0,
        };
        writer.set_epoch_info(&epoch_info)?;

        info!(
            validators = genesis_validators.len(),
            total_staked,
            "epoch 0 initialized from genesis"
        );

        Ok(set)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::{MemoryStore, StateView};
    use polay_staking::register_in_validator_list;

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn make_config(epoch_length: u64, max_validators: usize, min_stake: u64) -> ChainConfig {
        ChainConfig {
            epoch_length,
            max_validators,
            min_stake,
            ..ChainConfig::default()
        }
    }

    /// Helper to register a validator in state with a given stake and status.
    fn register_validator(
        store: &dyn StateStore,
        addr: Address,
        stake: u64,
        status: ValidatorStatus,
    ) {
        let writer = StateWriter::new(store);
        let mut info = ValidatorInfo::new(addr, 500);
        info.stake = stake;
        info.status = status;
        writer.set_validator(&info).unwrap();
        register_in_validator_list(store, &addr).unwrap();
    }

    fn register_jailed_validator(
        store: &dyn StateStore,
        addr: Address,
        stake: u64,
        jailed_until: u64,
    ) {
        let writer = StateWriter::new(store);
        let mut info = ValidatorInfo::new(addr, 500);
        info.stake = stake;
        info.jail(jailed_until);
        writer.set_validator(&info).unwrap();
        register_in_validator_list(store, &addr).unwrap();
    }

    // -- is_epoch_boundary ---------------------------------------------------

    #[test]
    fn epoch_boundary_at_correct_heights() {
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        assert!(!em.is_epoch_boundary(0)); // genesis is not an epoch boundary
        assert!(!em.is_epoch_boundary(1));
        assert!(!em.is_epoch_boundary(99));
        assert!(em.is_epoch_boundary(100));
        assert!(!em.is_epoch_boundary(101));
        assert!(em.is_epoch_boundary(200));
        assert!(em.is_epoch_boundary(7200));
    }

    // -- epoch_for_height ----------------------------------------------------

    #[test]
    fn epoch_for_height_calculates_correctly() {
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        assert_eq!(em.epoch_for_height(0), 0);
        assert_eq!(em.epoch_for_height(1), 0);
        assert_eq!(em.epoch_for_height(99), 0);
        assert_eq!(em.epoch_for_height(100), 1);
        assert_eq!(em.epoch_for_height(199), 1);
        assert_eq!(em.epoch_for_height(200), 2);
    }

    // -- process_epoch_transition: top validators by stake -------------------

    #[test]
    fn selects_top_validators_by_stake() {
        let store = MemoryStore::new();
        let config = make_config(100, 2, 0); // max 2 validators
        let em = EpochManager::new(config);

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);
        register_validator(&store, test_addr(2), 3000, ValidatorStatus::Active);
        register_validator(&store, test_addr(3), 7000, ValidatorStatus::Active);

        let (new_set, _events) = em.process_epoch_transition(100, &store).unwrap();

        assert_eq!(new_set.len(), 2);
        // Top 2 should be addr(3)=7000, addr(1)=5000
        assert_eq!(new_set.validators[0].address, test_addr(3));
        assert_eq!(new_set.validators[1].address, test_addr(1));
    }

    // -- Validators below min_stake are excluded ----------------------------

    #[test]
    fn validators_below_min_stake_excluded() {
        let store = MemoryStore::new();
        let config = make_config(100, 10, 1000);
        let em = EpochManager::new(config);

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);
        register_validator(&store, test_addr(2), 500, ValidatorStatus::Active); // below min
        register_validator(&store, test_addr(3), 2000, ValidatorStatus::Active);

        let (new_set, _events) = em.process_epoch_transition(100, &store).unwrap();

        assert_eq!(new_set.len(), 2);
        // addr(2) with 500 stake should be excluded
        assert!(!new_set.contains(&test_addr(2)));
    }

    // -- Jailed validators are excluded ------------------------------------

    #[test]
    fn jailed_validators_excluded() {
        let store = MemoryStore::new();
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);
        // Jailed until height 200 -- won't be unjailed at height 100
        register_jailed_validator(&store, test_addr(2), 8000, 200);

        let (new_set, _events) = em.process_epoch_transition(100, &store).unwrap();

        assert_eq!(new_set.len(), 1);
        assert!(!new_set.contains(&test_addr(2)));
    }

    // -- Unjailing works when jail period elapses --------------------------

    #[test]
    fn unjailing_works_when_jail_period_elapses() {
        let store = MemoryStore::new();
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);
        // Jailed until height 100 -- should be unjailed at epoch boundary 100
        register_jailed_validator(&store, test_addr(2), 8000, 100);

        let (new_set, events) = em.process_epoch_transition(100, &store).unwrap();

        // Validator 2 should now be active and in the set
        assert_eq!(new_set.len(), 2);
        assert!(new_set.contains(&test_addr(2)));

        // Should have an unjail event
        let unjail_events: Vec<_> = events
            .iter()
            .filter(|e| e.action == "validator_unjailed")
            .collect();
        assert_eq!(unjail_events.len(), 1);
    }

    // -- max_validators is respected ---------------------------------------

    #[test]
    fn max_validators_respected() {
        let store = MemoryStore::new();
        let config = make_config(100, 2, 0); // max 2
        let em = EpochManager::new(config);

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);
        register_validator(&store, test_addr(2), 3000, ValidatorStatus::Active);
        register_validator(&store, test_addr(3), 7000, ValidatorStatus::Active);
        register_validator(&store, test_addr(4), 1000, ValidatorStatus::Active);

        let (new_set, _events) = em.process_epoch_transition(100, &store).unwrap();

        assert_eq!(new_set.len(), 2);
    }

    // -- Rewards distributed at epoch end ----------------------------------

    #[test]
    fn rewards_distributed_at_epoch_end() {
        let store = MemoryStore::new();
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        // Seed SupplyInfo so inflation-based reward calculation produces non-zero rewards.
        let supply = polay_types::SupplyInfo {
            total_supply: 100_000_000,
            ..Default::default()
        };
        StateWriter::new(&store).set_supply_info(&supply).unwrap();

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);
        register_validator(&store, test_addr(2), 5000, ValidatorStatus::Active);

        let (_new_set, events) = em.process_epoch_transition(100, &store).unwrap();

        // Should have an epoch_transition event with rewards > 0
        let epoch_event = events
            .iter()
            .find(|e| e.action == "epoch_transition")
            .unwrap();
        let rewards_str = epoch_event.get_attribute("rewards_distributed").unwrap();
        let rewards: u64 = rewards_str.parse().unwrap();
        assert!(rewards > 0, "rewards should be distributed");
    }

    // -- EpochInfo is stored and retrievable --------------------------------

    #[test]
    fn epoch_info_stored_and_retrievable() {
        let store = MemoryStore::new();
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);

        em.process_epoch_transition(100, &store).unwrap();

        let view = StateView::new(&store);
        let info = view.get_epoch_info(1).unwrap().unwrap();
        assert_eq!(info.epoch, 1);
        assert_eq!(info.start_height, 100);
        assert_eq!(info.end_height, 199);
        assert_eq!(info.validator_set.len(), 1);
        assert_eq!(info.validator_set[0], test_addr(1));
        assert!(info.total_staked > 0);
    }

    // -- Active validator set updated in state ------------------------------

    #[test]
    fn active_validator_set_updated_in_state() {
        let store = MemoryStore::new();
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);
        register_validator(&store, test_addr(2), 3000, ValidatorStatus::Active);

        em.process_epoch_transition(100, &store).unwrap();

        let view = StateView::new(&store);
        let active = view.get_active_validator_set().unwrap().unwrap();
        assert_eq!(active.len(), 2);
        // Sorted by stake descending, so addr(1)=5000 first
        assert_eq!(active[0], test_addr(1));
        assert_eq!(active[1], test_addr(2));
    }

    // -- Genesis initialization works correctly -----------------------------

    #[test]
    fn genesis_initialization_works() {
        let store = MemoryStore::new();
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        let genesis_validators = vec![(test_addr(1), 5000u64), (test_addr(2), 3000u64)];

        let set = em.init_from_genesis(&genesis_validators, &store).unwrap();

        assert_eq!(set.len(), 2);
        assert_eq!(set.total_stake, 8000);

        let view = StateView::new(&store);
        let active = view.get_active_validator_set().unwrap().unwrap();
        assert_eq!(active.len(), 2);

        let epoch_info = view.get_epoch_info(0).unwrap().unwrap();
        assert_eq!(epoch_info.epoch, 0);
        assert_eq!(epoch_info.start_height, 0);
        assert_eq!(epoch_info.end_height, 99);
        assert_eq!(epoch_info.validator_set.len(), 2);
        assert_eq!(epoch_info.total_staked, 8000);
        assert_eq!(epoch_info.rewards_distributed, 0);
    }

    // -- Edge case: fewer validators than max_validators --------------------

    #[test]
    fn fewer_validators_than_max() {
        let store = MemoryStore::new();
        let config = make_config(100, 100, 0); // max 100 but only 2 registered
        let em = EpochManager::new(config);

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);
        register_validator(&store, test_addr(2), 3000, ValidatorStatus::Active);

        let (new_set, _events) = em.process_epoch_transition(100, &store).unwrap();

        assert_eq!(new_set.len(), 2);
    }

    // -- Empty validator set -----------------------------------------------

    #[test]
    fn empty_validator_set_produces_empty_set() {
        let store = MemoryStore::new();
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        let (new_set, _events) = em.process_epoch_transition(100, &store).unwrap();

        assert!(new_set.is_empty());
        assert_eq!(new_set.total_stake, 0);
    }

    // -- Tombstoned validators are excluded --------------------------------

    #[test]
    fn tombstoned_validators_excluded() {
        let store = MemoryStore::new();
        let config = make_config(100, 10, 0);
        let em = EpochManager::new(config);

        register_validator(&store, test_addr(1), 5000, ValidatorStatus::Active);
        register_validator(&store, test_addr(2), 8000, ValidatorStatus::Tombstoned);

        let (new_set, _events) = em.process_epoch_transition(100, &store).unwrap();

        assert_eq!(new_set.len(), 1);
        assert!(!new_set.contains(&test_addr(2)));
    }
}
