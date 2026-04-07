//! `polay-staking` — extended staking utilities for the POLAY gaming blockchain.
//!
//! This crate re-exports the core staking types from [`polay_types::staking`]
//! and provides higher-level operations such as epoch reward calculation,
//! reward distribution, and slashing that go beyond the basic execution-layer
//! primitives.

use polay_config::ChainConfig;
use polay_state::{store_get, store_put, StateStore, StateView, StateWriter};
use polay_types::economics::InflationParams;
use polay_types::{AccountState, Address};
use thiserror::Error;
use tracing::debug;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

/// Re-export all staking types for downstream convenience.
pub use polay_types::staking::{
    Delegation, EquivocationEvidence, SlashEvent, ValidatorInfo, ValidatorStatus,
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors specific to the staking module.
#[derive(Debug, Error)]
pub enum StakingError {
    /// An error propagated from the state layer.
    #[error("state error: {0}")]
    State(#[from] polay_state::StateError),

    /// The requested validator was not found in state.
    #[error("validator not found: {0}")]
    ValidatorNotFound(String),

    /// A numeric overflow or underflow occurred during reward math.
    #[error("arithmetic overflow in staking calculation")]
    ArithmeticOverflow,
}

pub type StakingResult<T> = Result<T, StakingError>;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The well-known state key that stores the list of registered validator
/// addresses. Because the low-level KV store does not support prefix
/// iteration without a dedicated index, we maintain this list explicitly.
const VALIDATOR_LIST_KEY: &[u8] = b"staking:validator_addresses";

// (BASE_REWARD_PER_BLOCK removed -- replaced by inflation-based rewards)

/// Duration (in blocks) that a slashed validator remains jailed.
/// At 2-second block times, 1800 blocks = 1 hour.
const DEFAULT_JAIL_DURATION_BLOCKS: u64 = 1800;

// ---------------------------------------------------------------------------
// Validator-list helpers
// ---------------------------------------------------------------------------

/// Load the global list of registered validator addresses from state.
fn load_validator_list(store: &dyn StateStore) -> StakingResult<Vec<Address>> {
    let list: Option<Vec<Address>> = store_get(store, VALIDATOR_LIST_KEY)?;
    Ok(list.unwrap_or_default())
}

/// Persist the global list of registered validator addresses.
fn save_validator_list(store: &dyn StateStore, list: &[Address]) -> StakingResult<()> {
    store_put(store, VALIDATOR_LIST_KEY, &list.to_vec())?;
    Ok(())
}

/// Ensure `addr` is present in the global validator list. Idempotent.
pub fn register_in_validator_list(
    store: &dyn StateStore,
    addr: &Address,
) -> StakingResult<()> {
    let mut list = load_validator_list(store)?;
    if !list.contains(addr) {
        list.push(*addr);
        save_validator_list(store, &list)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// StakingModule
// ---------------------------------------------------------------------------

/// Extended staking logic that operates over the state store.
pub struct StakingModule;

impl StakingModule {
    // -- Queries -------------------------------------------------------------

    /// Return the full validator set by loading each validator referenced in
    /// the global validator list.
    pub fn get_validator_set(store: &dyn StateStore) -> StakingResult<Vec<ValidatorInfo>> {
        let addresses = load_validator_list(store)?;
        let view = StateView::new(store);
        let mut validators = Vec::with_capacity(addresses.len());
        for addr in &addresses {
            if let Some(v) = view.get_validator(addr)? {
                validators.push(v);
            }
        }
        Ok(validators)
    }

    /// Return the total amount of native tokens staked across all validators.
    pub fn get_total_staked(store: &dyn StateStore) -> StakingResult<u64> {
        let validators = Self::get_validator_set(store)?;
        let total = validators.iter().map(|v| v.stake).sum();
        Ok(total)
    }

    /// Return delegations for a given validator.
    ///
    /// Because the underlying KV store does not support prefix iteration,
    /// we maintain a per-validator delegation list key. If the list has not
    /// been populated yet, an empty vec is returned.
    pub fn get_delegations_for_validator(
        store: &dyn StateStore,
        validator: &Address,
    ) -> StakingResult<Vec<Delegation>> {
        let key = delegation_list_key(validator);
        let delegator_addrs: Option<Vec<Address>> = store_get(store, &key)?;
        let delegator_addrs = delegator_addrs.unwrap_or_default();

        let view = StateView::new(store);
        let mut delegations = Vec::with_capacity(delegator_addrs.len());
        for delegator in &delegator_addrs {
            if let Some(d) = view.get_delegation(delegator, validator)? {
                delegations.push(d);
            }
        }
        Ok(delegations)
    }

    /// Register a delegator in the per-validator delegation list. Idempotent.
    pub fn register_delegation(
        store: &dyn StateStore,
        validator: &Address,
        delegator: &Address,
    ) -> StakingResult<()> {
        let key = delegation_list_key(validator);
        let mut list: Vec<Address> = store_get(store, &key)?.unwrap_or_default();
        if !list.contains(delegator) {
            list.push(*delegator);
            store_put(store, &key, &list)?;
        }
        Ok(())
    }

    // -- Epoch reward calculation --------------------------------------------

    /// Calculate the total reward for an epoch based on inflation parameters,
    /// the current total supply, total staked amount, and epoch length.
    ///
    /// Uses the inflation rate (clamped to `min_rate_bps` floor) to compute
    /// the annual reward, then scales to a single epoch.
    pub fn calculate_epoch_rewards(
        total_supply: u64,
        _total_staked: u64,
        inflation_params: &InflationParams,
        epoch_length: u64,
    ) -> u64 {
        if total_supply == 0 || epoch_length == 0 {
            return 0;
        }

        // ~15_768_000 blocks per year at 2s block time.
        let blocks_per_year: u64 = 365 * 24 * 3600 / 2;
        let epochs_per_year = blocks_per_year / epoch_length;
        if epochs_per_year == 0 {
            return 0;
        }

        let rate = inflation_params
            .initial_rate_bps
            .max(inflation_params.min_rate_bps);
        let annual_reward = (total_supply as u128 * rate as u128 / 10_000) as u64;
        annual_reward / epochs_per_year
    }

    // -- Reward distribution -------------------------------------------------

    /// Distribute epoch rewards to a set of validators, proportional to their
    /// stake. Commission is retained by the validator; the remainder is
    /// credited proportionally to delegators.
    ///
    /// Delegations created in the current epoch (where `last_reward_epoch >= current_epoch`)
    /// are skipped — a delegation must be active for at least one full epoch to earn rewards.
    ///
    /// Returns a list of `(address, amount)` tuples indicating credits made
    /// to individual accounts (both validators and delegators).
    pub fn distribute_epoch_rewards(
        store: &dyn StateStore,
        validators: &[ValidatorInfo],
        current_epoch: u64,
        config: &ChainConfig,
    ) -> StakingResult<Vec<(Address, u64)>> {
        let total_stake: u64 = validators.iter().map(|v| v.stake).sum();
        if total_stake == 0 {
            return Ok(Vec::new());
        }

        let view = StateView::new(store);
        let supply = view.get_supply_info()?.unwrap_or_default();
        let total_reward = Self::calculate_epoch_rewards(
            supply.total_supply,
            total_stake,
            &config.inflation_params,
            config.epoch_length,
        );

        let writer = StateWriter::new(store);
        let mut payouts: Vec<(Address, u64)> = Vec::new();

        for validator in validators {
            if validator.stake == 0 {
                continue;
            }

            // Validator's share of the total reward, proportional to stake.
            let validator_reward = (total_reward as u128)
                .checked_mul(validator.stake as u128)
                .and_then(|v| v.checked_div(total_stake as u128))
                .ok_or(StakingError::ArithmeticOverflow)? as u64;

            // Commission goes to the validator operator.
            let commission = validator.commission_on(validator_reward);
            let delegator_pool = validator_reward.saturating_sub(commission);

            // Credit commission to validator account.
            if commission > 0 {
                let mut account = view
                    .get_account(&validator.address)?
                    .unwrap_or_else(|| AccountState::new(validator.address, 0));
                account.balance = account.balance.saturating_add(commission);
                writer.set_account(&account)?;
                payouts.push((validator.address, commission));
            }

            // Distribute the delegator pool proportionally.
            // Only include delegations that have been active for at least one full epoch.
            let delegations = Self::get_delegations_for_validator(store, &validator.address)?;
            let eligible_delegations: Vec<&Delegation> = delegations
                .iter()
                .filter(|d| d.amount > 0 && d.last_reward_epoch < current_epoch)
                .collect();
            let total_delegated: u64 = eligible_delegations.iter().map(|d| d.amount).sum();

            if total_delegated > 0 && delegator_pool > 0 {
                for delegation in &eligible_delegations {
                    if delegation.amount == 0 {
                        continue;
                    }
                    let share = (delegator_pool as u128)
                        .checked_mul(delegation.amount as u128)
                        .and_then(|v| v.checked_div(total_delegated as u128))
                        .ok_or(StakingError::ArithmeticOverflow)?
                        as u64;

                    if share > 0 {
                        let mut account = view
                            .get_account(&delegation.delegator)?
                            .unwrap_or_else(|| AccountState::new(delegation.delegator, 0));
                        account.balance = account.balance.saturating_add(share);
                        writer.set_account(&account)?;
                        payouts.push((delegation.delegator, share));
                    }
                }
            }

            debug!(
                validator = %validator.address,
                validator_reward,
                commission,
                delegator_pool,
                "epoch rewards distributed for validator"
            );
        }

        // Update SupplyInfo: total_minted and total_supply increase by the
        // total reward distributed.
        let total_distributed: u64 = payouts.iter().map(|(_, amt)| amt).sum();
        if total_distributed > 0 {
            let mut supply = view.get_supply_info()?.unwrap_or_default();
            supply.total_minted = supply.total_minted.saturating_add(total_distributed);
            supply.total_supply = supply.total_supply.saturating_add(total_distributed);
            supply.recompute_circulating();
            writer.set_supply_info(&supply)?;
        }

        Ok(payouts)
    }

    // -- Slashing ------------------------------------------------------------

    /// Slash a validator by reducing their stake by a fraction (expressed in
    /// basis points, where 10_000 = 100%).
    ///
    /// Side-effects:
    /// - Validator stake is reduced.
    /// - All delegations for the validator are proportionally slashed.
    /// - The validator is jailed for `DEFAULT_JAIL_DURATION_BLOCKS`.
    /// - A [`SlashEvent`] record is returned (total slashed = validator + delegators).
    pub fn process_slashing(
        store: &dyn StateStore,
        validator_addr: &Address,
        fraction_bps: u16,
        reason: &str,
        height: u64,
    ) -> StakingResult<SlashEvent> {
        let view = StateView::new(store);
        let writer = StateWriter::new(store);

        let mut validator = view
            .get_validator(validator_addr)?
            .ok_or_else(|| StakingError::ValidatorNotFound(validator_addr.to_hex()))?;

        // Calculate slash amount on validator's own stake.
        let validator_slash =
            ((validator.stake as u128) * fraction_bps as u128 / 10_000u128) as u64;

        // Reduce validator stake.
        validator.stake = validator.stake.saturating_sub(validator_slash);

        // Jail the validator for DEFAULT_JAIL_DURATION_BLOCKS blocks.
        validator.jail(height.saturating_add(DEFAULT_JAIL_DURATION_BLOCKS));

        writer.set_validator(&validator)?;

        // Slash all delegations proportionally.
        let delegations = Self::get_delegations_for_validator(store, validator_addr)?;
        let mut delegation_slash_total: u64 = 0;

        for mut delegation in delegations {
            if delegation.amount == 0 {
                continue;
            }
            let delegation_slash =
                ((delegation.amount as u128) * fraction_bps as u128 / 10_000u128) as u64;
            delegation.amount = delegation.amount.saturating_sub(delegation_slash);
            writer.set_delegation(&delegation)?;
            delegation_slash_total = delegation_slash_total.saturating_add(delegation_slash);
        }

        let total_slashed = validator_slash.saturating_add(delegation_slash_total);

        let event = SlashEvent {
            validator: *validator_addr,
            amount: total_slashed,
            reason: reason.to_string(),
            height,
        };

        debug!(
            validator = %validator_addr,
            validator_slash,
            delegation_slash_total,
            total_slashed,
            fraction_bps,
            reason,
            height,
            "validator slashed (including delegations)"
        );

        Ok(event)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Per-validator key that stores the list of delegator addresses.
fn delegation_list_key(validator: &Address) -> Vec<u8> {
    let mut key = b"staking:delegators:".to_vec();
    key.extend_from_slice(validator.as_bytes());
    key
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::MemoryStore;
    use polay_types::SupplyInfo;

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    #[test]
    fn validator_list_round_trip() {
        let store = MemoryStore::new();
        let a = test_addr(1);
        let b = test_addr(2);

        register_in_validator_list(&store, &a).unwrap();
        register_in_validator_list(&store, &b).unwrap();
        // Idempotent.
        register_in_validator_list(&store, &a).unwrap();

        let list = load_validator_list(&store).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&a));
        assert!(list.contains(&b));
    }

    #[test]
    fn get_validator_set_empty() {
        let store = MemoryStore::new();
        let set = StakingModule::get_validator_set(&store).unwrap();
        assert!(set.is_empty());
    }

    #[test]
    fn get_validator_set_returns_stored_validators() {
        let store = MemoryStore::new();
        let addr_a = test_addr(1);
        let addr_b = test_addr(2);

        let mut v_a = ValidatorInfo::new(addr_a, 500);
        v_a.stake = 10_000;
        let v_b = ValidatorInfo::new(addr_b, 300);

        StateWriter::new(&store).set_validator(&v_a).unwrap();
        StateWriter::new(&store).set_validator(&v_b).unwrap();
        register_in_validator_list(&store, &addr_a).unwrap();
        register_in_validator_list(&store, &addr_b).unwrap();

        let set = StakingModule::get_validator_set(&store).unwrap();
        assert_eq!(set.len(), 2);
        assert_eq!(set[0].stake, 10_000);
    }

    #[test]
    fn get_total_staked() {
        let store = MemoryStore::new();
        let addr_a = test_addr(1);
        let addr_b = test_addr(2);

        let mut v_a = ValidatorInfo::new(addr_a, 500);
        v_a.stake = 3000;
        let mut v_b = ValidatorInfo::new(addr_b, 300);
        v_b.stake = 7000;

        StateWriter::new(&store).set_validator(&v_a).unwrap();
        StateWriter::new(&store).set_validator(&v_b).unwrap();
        register_in_validator_list(&store, &addr_a).unwrap();
        register_in_validator_list(&store, &addr_b).unwrap();

        let total = StakingModule::get_total_staked(&store).unwrap();
        assert_eq!(total, 10_000);
    }

    #[test]
    fn calculate_epoch_rewards_basic() {
        let ip = InflationParams::default(); // 8% initial
        // 100M supply, 8% annual = 8M annual. Epochs per year = 15_768_000 / 7200 = 2190.
        // Epoch reward = 8_000_000 / 2190 = 3652 (integer div).
        let reward = StakingModule::calculate_epoch_rewards(100_000_000, 40_000_000, &ip, 7200);
        assert_eq!(reward, 3652);
    }

    #[test]
    fn distribute_epoch_rewards_proportional() {
        let store = MemoryStore::new();
        let config = ChainConfig::default();
        let addr_a = test_addr(1);
        let addr_b = test_addr(2);
        let delegator = test_addr(10);

        // Seed supply info so inflation calculation works.
        let supply = SupplyInfo {
            total_supply: 100_000_000,
            ..Default::default()
        };
        StateWriter::new(&store).set_supply_info(&supply).unwrap();

        // Validator A: 7000 stake, 10% commission (1000 bps).
        let mut v_a = ValidatorInfo::new(addr_a, 1000);
        v_a.stake = 7000;

        // Validator B: 3000 stake, 5% commission (500 bps).
        let mut v_b = ValidatorInfo::new(addr_b, 500);
        v_b.stake = 3000;

        StateWriter::new(&store).set_validator(&v_a).unwrap();
        StateWriter::new(&store).set_validator(&v_b).unwrap();

        // Register a delegation for validator A.
        let delegation = Delegation {
            delegator,
            validator: addr_a,
            amount: 7000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        StateWriter::new(&store).set_delegation(&delegation).unwrap();
        StakingModule::register_delegation(&store, &addr_a, &delegator).unwrap();

        let validators = vec![v_a, v_b];
        let payouts = StakingModule::distribute_epoch_rewards(&store, &validators, 1, &config).unwrap();

        // Payouts should be non-empty.
        assert!(!payouts.is_empty());

        // Every payout amount should be > 0.
        for (_, amount) in &payouts {
            assert!(*amount > 0);
        }

        // SupplyInfo should reflect minted rewards.
        let updated_supply = StateView::new(&store).get_supply_info().unwrap().unwrap();
        assert!(updated_supply.total_minted > 0);
        assert_eq!(updated_supply.total_supply, 100_000_000 + updated_supply.total_minted);
    }

    #[test]
    fn distribute_epoch_rewards_no_stake() {
        let store = MemoryStore::new();
        let config = ChainConfig::default();
        let validators: Vec<ValidatorInfo> = vec![];
        let payouts = StakingModule::distribute_epoch_rewards(&store, &validators, 0, &config).unwrap();
        assert!(payouts.is_empty());
    }

    #[test]
    fn process_slashing_reduces_stake_and_jails() {
        let store = MemoryStore::new();
        let addr = test_addr(1);

        let mut v = ValidatorInfo::new(addr, 500);
        v.stake = 100_000;
        StateWriter::new(&store).set_validator(&v).unwrap();

        // Slash 10% (1000 bps) at height 42.
        let event =
            StakingModule::process_slashing(&store, &addr, 1000, "downtime", 42).unwrap();

        // No delegations, so total slashed = validator slash only.
        assert_eq!(event.amount, 10_000); // 10% of 100_000
        assert_eq!(event.reason, "downtime");
        assert_eq!(event.height, 42);
        assert_eq!(event.validator, addr);

        let updated = StateView::new(&store).get_validator(&addr).unwrap().unwrap();
        assert_eq!(updated.stake, 90_000);
        assert_eq!(updated.status, ValidatorStatus::Jailed);
        // Jailed until height + DEFAULT_JAIL_DURATION_BLOCKS (1800).
        assert_eq!(updated.jailed_until, Some(42 + 1800));
    }

    #[test]
    fn process_slashing_jail_duration_is_blocks() {
        let store = MemoryStore::new();
        let addr = test_addr(1);

        let mut v = ValidatorInfo::new(addr, 500);
        v.stake = 50_000;
        StateWriter::new(&store).set_validator(&v).unwrap();

        let _event =
            StakingModule::process_slashing(&store, &addr, 500, "test", 100).unwrap();

        let updated = StateView::new(&store).get_validator(&addr).unwrap().unwrap();
        // With 2s blocks: 1800 blocks = 1 hour. Jailed from height 100 to 1900.
        assert_eq!(updated.jailed_until, Some(100 + 1800));
        // At height 1899 (just before): still jailed.
        let mut v2 = updated.clone();
        assert!(!v2.try_unjail(1899));
        // At height 1900: can unjail.
        assert!(v2.try_unjail(1900));
    }

    #[test]
    fn process_slashing_slashes_delegations_proportionally() {
        let store = MemoryStore::new();
        let val_addr = test_addr(1);
        let del_a = test_addr(10);
        let del_b = test_addr(11);

        // Validator with 50_000 stake.
        let mut v = ValidatorInfo::new(val_addr, 500);
        v.stake = 50_000;
        StateWriter::new(&store).set_validator(&v).unwrap();

        // Register delegations: del_a=20_000, del_b=10_000.
        let d_a = Delegation {
            delegator: del_a,
            validator: val_addr,
            amount: 20_000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        let d_b = Delegation {
            delegator: del_b,
            validator: val_addr,
            amount: 10_000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        StateWriter::new(&store).set_delegation(&d_a).unwrap();
        StateWriter::new(&store).set_delegation(&d_b).unwrap();
        StakingModule::register_delegation(&store, &val_addr, &del_a).unwrap();
        StakingModule::register_delegation(&store, &val_addr, &del_b).unwrap();

        // Slash 10% (1000 bps).
        let event =
            StakingModule::process_slashing(&store, &val_addr, 1000, "downtime", 42).unwrap();

        // Validator: 10% of 50_000 = 5_000
        // del_a: 10% of 20_000 = 2_000
        // del_b: 10% of 10_000 = 1_000
        // Total slashed = 5_000 + 2_000 + 1_000 = 8_000
        assert_eq!(event.amount, 8_000);

        // Verify individual amounts.
        let updated_v = StateView::new(&store).get_validator(&val_addr).unwrap().unwrap();
        assert_eq!(updated_v.stake, 45_000);

        let updated_da = StateView::new(&store)
            .get_delegation(&del_a, &val_addr)
            .unwrap()
            .unwrap();
        assert_eq!(updated_da.amount, 18_000);

        let updated_db = StateView::new(&store)
            .get_delegation(&del_b, &val_addr)
            .unwrap()
            .unwrap();
        assert_eq!(updated_db.amount, 9_000);
    }

    #[test]
    fn last_reward_epoch_prevents_same_epoch_reward() {
        let store = MemoryStore::new();
        let config = ChainConfig::default();
        let val_addr = test_addr(1);
        let del_old = test_addr(10);
        let del_new = test_addr(11);

        // Seed supply info so inflation calculation works.
        let supply = SupplyInfo {
            total_supply: 100_000_000,
            ..Default::default()
        };
        StateWriter::new(&store).set_supply_info(&supply).unwrap();

        // Validator with 10_000 stake, 10% commission.
        let mut v = ValidatorInfo::new(val_addr, 1000);
        v.stake = 10_000;
        StateWriter::new(&store).set_validator(&v).unwrap();

        // Old delegation: created in epoch 0 (eligible for rewards in epoch 1+).
        let d_old = Delegation {
            delegator: del_old,
            validator: val_addr,
            amount: 5000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        // New delegation: created in current epoch 5 (not eligible yet).
        let d_new = Delegation {
            delegator: del_new,
            validator: val_addr,
            amount: 5000,
            reward_debt: 0,
            last_reward_epoch: 5,
        };
        StateWriter::new(&store).set_delegation(&d_old).unwrap();
        StateWriter::new(&store).set_delegation(&d_new).unwrap();
        StakingModule::register_delegation(&store, &val_addr, &del_old).unwrap();
        StakingModule::register_delegation(&store, &val_addr, &del_new).unwrap();

        // Distribute at epoch 5 -- only del_old should get rewards.
        let payouts =
            StakingModule::distribute_epoch_rewards(&store, &[v.clone()], 5, &config).unwrap();

        // del_old should receive a payout, del_new should not.
        let old_payout: Vec<_> = payouts.iter().filter(|(a, _)| *a == del_old).collect();
        let new_payout: Vec<_> = payouts.iter().filter(|(a, _)| *a == del_new).collect();
        assert!(!old_payout.is_empty(), "old delegator should receive rewards");
        assert!(new_payout.is_empty(), "new delegator should NOT receive rewards in same epoch");
    }

    #[test]
    fn process_slashing_validator_not_found() {
        let store = MemoryStore::new();
        let err =
            StakingModule::process_slashing(&store, &test_addr(99), 500, "test", 1).unwrap_err();
        assert!(matches!(err, StakingError::ValidatorNotFound(_)));
    }

    #[test]
    fn delegation_list_round_trip() {
        let store = MemoryStore::new();
        let validator = test_addr(1);
        let del_a = test_addr(10);
        let del_b = test_addr(11);

        StakingModule::register_delegation(&store, &validator, &del_a).unwrap();
        StakingModule::register_delegation(&store, &validator, &del_b).unwrap();
        // Idempotent.
        StakingModule::register_delegation(&store, &validator, &del_a).unwrap();

        // Write actual delegation records.
        let d_a = Delegation {
            delegator: del_a,
            validator,
            amount: 5000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        let d_b = Delegation {
            delegator: del_b,
            validator,
            amount: 3000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        StateWriter::new(&store).set_delegation(&d_a).unwrap();
        StateWriter::new(&store).set_delegation(&d_b).unwrap();

        let delegations =
            StakingModule::get_delegations_for_validator(&store, &validator).unwrap();
        assert_eq!(delegations.len(), 2);
        assert_eq!(delegations[0].amount, 5000);
        assert_eq!(delegations[1].amount, 3000);
    }

    // -----------------------------------------------------------------------
    // Inflation / economics tests
    // -----------------------------------------------------------------------

    #[test]
    fn inflation_reward_decreases_with_lower_rate() {
        // Verify that a lower initial_rate_bps produces a smaller epoch reward.
        let epoch_len = 7200u64;
        let total_supply = 100_000_000u64;
        let total_staked = 40_000_000u64;

        let high = InflationParams {
            initial_rate_bps: 800, // 8%
            min_rate_bps: 200,
            decay_rate_bps: 500,
            target_staking_ratio_bps: 6700,
        };
        let low = InflationParams {
            initial_rate_bps: 400, // 4%
            ..high.clone()
        };

        let reward_high =
            StakingModule::calculate_epoch_rewards(total_supply, total_staked, &high, epoch_len);
        let reward_low =
            StakingModule::calculate_epoch_rewards(total_supply, total_staked, &low, epoch_len);

        assert!(reward_high > reward_low, "higher rate should produce larger reward");
        // 8% should be roughly 2x 4%.
        assert!(reward_high >= reward_low * 2 - 1); // allow 1 unit rounding
        assert!(reward_high <= reward_low * 2 + 1);
    }

    #[test]
    fn inflation_min_rate_floor_enforced() {
        // When initial_rate_bps < min_rate_bps, the floor should apply.
        let ip = InflationParams {
            initial_rate_bps: 100, // below min
            min_rate_bps: 200,
            decay_rate_bps: 500,
            target_staking_ratio_bps: 6700,
        };
        let reward = StakingModule::calculate_epoch_rewards(100_000_000, 40_000_000, &ip, 7200);

        // Should use min_rate_bps (200 = 2%), not initial_rate_bps (100 = 1%).
        let ip_at_min = InflationParams {
            initial_rate_bps: 200,
            ..ip.clone()
        };
        let reward_at_min =
            StakingModule::calculate_epoch_rewards(100_000_000, 40_000_000, &ip_at_min, 7200);
        assert_eq!(reward, reward_at_min);
    }

    #[test]
    fn inflation_zero_supply_zero_reward() {
        let ip = InflationParams::default();
        let reward = StakingModule::calculate_epoch_rewards(0, 0, &ip, 7200);
        assert_eq!(reward, 0);
    }

    #[test]
    fn inflation_zero_epoch_length_zero_reward() {
        let ip = InflationParams::default();
        let reward = StakingModule::calculate_epoch_rewards(100_000_000, 50_000_000, &ip, 0);
        assert_eq!(reward, 0);
    }

    #[test]
    fn minting_increases_total_supply() {
        // After distributing epoch rewards, total_supply should increase by
        // exactly total_minted.
        let store = MemoryStore::new();
        let config = ChainConfig::default();
        let initial_supply: u64 = 200_000_000;

        let supply = SupplyInfo {
            total_supply: initial_supply,
            circulating_supply: initial_supply,
            ..Default::default()
        };
        StateWriter::new(&store).set_supply_info(&supply).unwrap();

        let val = test_addr(1);
        let delegator = test_addr(10);
        let mut v = ValidatorInfo::new(val, 1000); // 10% commission
        v.stake = 50_000;
        StateWriter::new(&store).set_validator(&v).unwrap();

        // Register a delegation so rewards actually get distributed.
        let delegation = Delegation {
            delegator,
            validator: val,
            amount: 50_000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        StateWriter::new(&store).set_delegation(&delegation).unwrap();
        StakingModule::register_delegation(&store, &val, &delegator).unwrap();

        let _payouts =
            StakingModule::distribute_epoch_rewards(&store, &[v], 1, &config).unwrap();

        let updated = StateView::new(&store).get_supply_info().unwrap().unwrap();
        assert!(updated.total_minted > 0, "rewards should have been minted");
        assert_eq!(
            updated.total_supply,
            initial_supply + updated.total_minted,
            "total_supply should increase by total_minted"
        );
    }
}
