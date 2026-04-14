//! Staking module — validator registration, delegation, and undelegation.

use polay_config::ChainConfig;
use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{AccountState, Address, Delegation, Event, UnbondingEntry, ValidatorInfo};
use tracing::debug;

use crate::error::ExecutionError;

// ---------------------------------------------------------------------------
// Register validator
// ---------------------------------------------------------------------------

/// Register the signer as a new validator.
pub fn execute_register_validator(
    signer: &Address,
    commission_bps: u16,
    store: &dyn StateStore,
    config: &ChainConfig,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);

    // Check not already registered.
    if view.get_validator(signer)?.is_some() {
        return Err(ExecutionError::ValidatorAlreadyRegistered);
    }

    // Check commission is within bounds.
    if commission_bps > config.max_commission_bps {
        return Err(ExecutionError::InvalidCommission);
    }

    let validator = ValidatorInfo::new(*signer, commission_bps);
    StateWriter::new(store).set_validator(&validator)?;

    debug!(
        validator = %signer,
        commission_bps,
        "validator registered"
    );

    Ok(vec![Event::validator_registered(signer, commission_bps)])
}

// ---------------------------------------------------------------------------
// Delegate stake
// ---------------------------------------------------------------------------

/// Delegate native tokens from the signer to a validator.
pub fn execute_delegate_stake(
    signer: &Address,
    validator_addr: &Address,
    amount: u64,
    store: &dyn StateStore,
    config: &ChainConfig,
    _timestamp: u64,
    block_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Validator must exist and be active.
    let mut validator = view
        .get_validator(validator_addr)?
        .ok_or(ExecutionError::ValidatorNotFound)?;

    if !validator.is_active() {
        return Err(ExecutionError::ValidatorNotFound);
    }

    // Debit signer balance.
    let mut account = view
        .get_account(signer)?
        .ok_or_else(|| ExecutionError::AccountNotFound(signer.to_hex()))?;
    if account.balance < amount {
        return Err(ExecutionError::InsufficientBalance {
            required: amount,
            available: account.balance,
        });
    }
    account.balance =
        account
            .balance
            .checked_sub(amount)
            .ok_or(ExecutionError::InsufficientBalance {
                required: amount,
                available: account.balance,
            })?;
    writer.set_account(&account)?;

    // Update validator stake.
    validator.stake = validator.stake.saturating_add(amount);
    writer.set_validator(&validator)?;

    // Create or update delegation record.
    let current_epoch = block_height / config.epoch_length;
    let mut delegation = view
        .get_delegation(signer, validator_addr)?
        .unwrap_or_else(|| Delegation::new(*signer, *validator_addr));
    delegation.add_stake(amount);
    // Mark the delegation as created/modified in the current epoch so it
    // cannot earn rewards until at least one full epoch has passed.
    delegation.last_reward_epoch = current_epoch;
    writer.set_delegation(&delegation)?;

    debug!(
        delegator = %signer,
        validator = %validator_addr,
        amount,
        "stake delegated"
    );

    Ok(vec![Event::stake_delegated(signer, validator_addr, amount)])
}

// ---------------------------------------------------------------------------
// Undelegate stake
// ---------------------------------------------------------------------------

/// Begin undelegating tokens from a validator.
///
/// Tokens are NOT returned immediately. Instead, an [`UnbondingEntry`] is
/// created and the funds are locked until `completion_height` is reached.
/// Call [`process_mature_unbondings`] at the start of each block to release
/// matured entries.
pub fn execute_undelegate_stake(
    signer: &Address,
    validator_addr: &Address,
    amount: u64,
    store: &dyn StateStore,
    current_height: u64,
    config: &ChainConfig,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Validator must exist.
    let mut validator = view
        .get_validator(validator_addr)?
        .ok_or(ExecutionError::ValidatorNotFound)?;

    // Delegation must exist and have enough stake.
    let mut delegation = view
        .get_delegation(signer, validator_addr)?
        .ok_or(ExecutionError::InsufficientStake)?;

    if delegation.amount < amount {
        return Err(ExecutionError::InsufficientStake);
    }

    // Reduce delegation.
    delegation.amount -= amount;
    writer.set_delegation(&delegation)?;

    // Reduce validator total stake.
    validator.stake = validator.stake.saturating_sub(amount);
    writer.set_validator(&validator)?;

    // Calculate completion height.
    let completion_height = current_height + config.unbonding_period_blocks;

    // Determine index: count existing entries for this delegator at the same
    // completion_height to avoid key collisions.
    let existing = view.get_unbonding_entries(signer)?;
    let index = existing
        .iter()
        .filter(|e| e.completion_height == completion_height)
        .count() as u8;

    // Create and store the unbonding entry.
    let entry = UnbondingEntry {
        delegator: *signer,
        validator: *validator_addr,
        amount,
        initiated_at: current_height,
        completion_height,
    };
    writer.set_unbonding_entry(&entry, index)?;

    debug!(
        delegator = %signer,
        validator = %validator_addr,
        amount,
        completion_height,
        "unbonding initiated"
    );

    Ok(vec![Event::unbonding_initiated(
        signer,
        validator_addr,
        amount,
        completion_height,
    )])
}

// ---------------------------------------------------------------------------
// Process mature unbondings
// ---------------------------------------------------------------------------

/// Scan all unbonding entries and release funds for entries where
/// `completion_height <= current_height`.
///
/// This function should be called at the start of each block, before
/// executing transactions.
pub fn process_mature_unbondings(
    store: &dyn StateStore,
    current_height: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    let mature = view.get_mature_unbondings(current_height)?;
    let mut events = Vec::new();

    for (entry, index) in mature {
        // Credit the delegator's balance.
        let mut account = view
            .get_account(&entry.delegator)?
            .unwrap_or_else(|| AccountState::new(entry.delegator, 0));
        account.balance = account.balance.saturating_add(entry.amount);
        writer.set_account(&account)?;

        // Delete the unbonding entry.
        writer.delete_unbonding_entry(&entry.delegator, entry.completion_height, index)?;

        debug!(
            delegator = %entry.delegator,
            validator = %entry.validator,
            amount = entry.amount,
            "unbonding completed"
        );

        events.push(Event::unbonding_completed(
            &entry.delegator,
            &entry.validator,
            entry.amount,
        ));
    }

    Ok(events)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::MemoryStore;

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn default_config() -> ChainConfig {
        ChainConfig::default()
    }

    #[test]
    fn register_validator_happy_path() {
        let store = MemoryStore::new();
        let addr = test_addr(1);
        let config = default_config();

        let events = execute_register_validator(&addr, 500, &store, &config).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "staking");
        assert_eq!(events[0].action, "validator_registered");

        let v = StateView::new(&store)
            .get_validator(&addr)
            .unwrap()
            .unwrap();
        assert!(v.is_active());
        assert_eq!(v.commission_bps, 500);
        assert_eq!(v.stake, 0);
    }

    #[test]
    fn register_validator_already_exists() {
        let store = MemoryStore::new();
        let addr = test_addr(1);
        let config = default_config();

        execute_register_validator(&addr, 500, &store, &config).unwrap();
        let err = execute_register_validator(&addr, 500, &store, &config).unwrap_err();
        assert!(matches!(err, ExecutionError::ValidatorAlreadyRegistered));
    }

    #[test]
    fn register_validator_commission_too_high() {
        let store = MemoryStore::new();
        let config = default_config(); // max_commission_bps = 2000

        let err = execute_register_validator(&test_addr(1), 3000, &store, &config).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidCommission));
    }

    #[test]
    fn delegate_stake_happy_path() {
        let store = MemoryStore::new();
        let config = default_config();
        let validator_addr = test_addr(1);
        let delegator_addr = test_addr(2);

        // Register validator.
        execute_register_validator(&validator_addr, 500, &store, &config).unwrap();

        // Give delegator a balance.
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(delegator_addr, 10_000, 0))
            .unwrap();

        let events = execute_delegate_stake(
            &delegator_addr,
            &validator_addr,
            3000,
            &store,
            &config,
            100,
            100,
        )
        .unwrap();
        assert_eq!(events.len(), 1);

        let view = StateView::new(&store);
        let v = view.get_validator(&validator_addr).unwrap().unwrap();
        assert_eq!(v.stake, 3000);

        let d = view
            .get_delegation(&delegator_addr, &validator_addr)
            .unwrap()
            .unwrap();
        assert_eq!(d.amount, 3000);

        let acct = view.get_account(&delegator_addr).unwrap().unwrap();
        assert_eq!(acct.balance, 7000);
    }

    #[test]
    fn delegate_stake_insufficient_balance() {
        let store = MemoryStore::new();
        let config = default_config();
        let validator_addr = test_addr(1);
        let delegator_addr = test_addr(2);

        execute_register_validator(&validator_addr, 500, &store, &config).unwrap();
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(delegator_addr, 100, 0))
            .unwrap();

        let err = execute_delegate_stake(
            &delegator_addr,
            &validator_addr,
            500,
            &store,
            &config,
            100,
            100,
        )
        .unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }

    #[test]
    fn undelegate_stake_happy_path() {
        let store = MemoryStore::new();
        let config = default_config();
        let validator_addr = test_addr(1);
        let delegator_addr = test_addr(2);

        execute_register_validator(&validator_addr, 500, &store, &config).unwrap();
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(delegator_addr, 10_000, 0))
            .unwrap();
        execute_delegate_stake(
            &delegator_addr,
            &validator_addr,
            5000,
            &store,
            &config,
            100,
            100,
        )
        .unwrap();

        // Undelegate half.
        let events =
            execute_undelegate_stake(&delegator_addr, &validator_addr, 2000, &store, 200, &config)
                .unwrap();
        assert_eq!(events.len(), 1);

        let view = StateView::new(&store);
        let v = view.get_validator(&validator_addr).unwrap().unwrap();
        assert_eq!(v.stake, 3000);

        let d = view
            .get_delegation(&delegator_addr, &validator_addr)
            .unwrap()
            .unwrap();
        assert_eq!(d.amount, 3000);

        let acct = view.get_account(&delegator_addr).unwrap().unwrap();
        // Balance stays at 5000 (10000 - 5000 delegated); the 2000 is locked
        // in an unbonding entry until the unbonding period completes.
        assert_eq!(acct.balance, 5000);

        let unbondings = view.get_unbonding_entries(&delegator_addr).unwrap();
        assert_eq!(unbondings.len(), 1);
        assert_eq!(unbondings[0].amount, 2000);
    }

    #[test]
    fn undelegate_stake_insufficient() {
        let store = MemoryStore::new();
        let config = default_config();
        let validator_addr = test_addr(1);
        let delegator_addr = test_addr(2);

        execute_register_validator(&validator_addr, 500, &store, &config).unwrap();
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(delegator_addr, 10_000, 0))
            .unwrap();
        execute_delegate_stake(
            &delegator_addr,
            &validator_addr,
            1000,
            &store,
            &config,
            100,
            100,
        )
        .unwrap();

        let err =
            execute_undelegate_stake(&delegator_addr, &validator_addr, 5000, &store, 200, &config)
                .unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientStake));
    }

    #[test]
    fn unbonding_entry_created_with_correct_completion_height() {
        let store = MemoryStore::new();
        let config = default_config();
        let validator_addr = test_addr(1);
        let delegator_addr = test_addr(2);

        execute_register_validator(&validator_addr, 500, &store, &config).unwrap();
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(delegator_addr, 10_000, 0))
            .unwrap();
        execute_delegate_stake(
            &delegator_addr,
            &validator_addr,
            5000,
            &store,
            &config,
            100,
            100,
        )
        .unwrap();

        let current_height = 1000;
        let events = execute_undelegate_stake(
            &delegator_addr,
            &validator_addr,
            2000,
            &store,
            current_height,
            &config,
        )
        .unwrap();

        // Event should contain the completion height.
        assert_eq!(events[0].action, "unbonding_initiated");
        let completion_attr = events[0].get_attribute("completion_height").unwrap();
        let expected_completion = current_height + config.unbonding_period_blocks;
        assert_eq!(completion_attr, expected_completion.to_string());

        let view = StateView::new(&store);
        let entries = view.get_unbonding_entries(&delegator_addr).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].completion_height, expected_completion);
        assert_eq!(entries[0].initiated_at, current_height);
        assert_eq!(entries[0].amount, 2000);
    }

    #[test]
    fn process_mature_unbondings_releases_funds() {
        let store = MemoryStore::new();
        let config = default_config();
        let validator_addr = test_addr(1);
        let delegator_addr = test_addr(2);

        execute_register_validator(&validator_addr, 500, &store, &config).unwrap();
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(delegator_addr, 10_000, 0))
            .unwrap();
        execute_delegate_stake(
            &delegator_addr,
            &validator_addr,
            5000,
            &store,
            &config,
            100,
            100,
        )
        .unwrap();

        // Undelegate at height 100.
        execute_undelegate_stake(&delegator_addr, &validator_addr, 2000, &store, 100, &config)
            .unwrap();

        let view = StateView::new(&store);
        // Balance should still be 5000 (funds locked in unbonding).
        assert_eq!(
            view.get_account(&delegator_addr).unwrap().unwrap().balance,
            5000
        );

        // Process at a height BEFORE maturity: nothing should happen.
        let early_events = process_mature_unbondings(&store, 100).unwrap();
        assert!(early_events.is_empty());

        // Process at exactly the completion height.
        let completion_height = 100 + config.unbonding_period_blocks;
        let events = process_mature_unbondings(&store, completion_height).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "unbonding_completed");

        // Balance should now include the unbonded amount.
        let acct = view.get_account(&delegator_addr).unwrap().unwrap();
        assert_eq!(acct.balance, 7000); // 5000 + 2000

        // Unbonding entries should be empty.
        let entries = view.get_unbonding_entries(&delegator_addr).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn multiple_unbondings_processed_correctly() {
        let store = MemoryStore::new();
        let config = default_config();
        let validator_addr = test_addr(1);
        let delegator_addr = test_addr(2);

        execute_register_validator(&validator_addr, 500, &store, &config).unwrap();
        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(delegator_addr, 20_000, 0))
            .unwrap();
        execute_delegate_stake(
            &delegator_addr,
            &validator_addr,
            10_000,
            &store,
            &config,
            100,
            100,
        )
        .unwrap();

        // Two undelegations at different heights.
        execute_undelegate_stake(&delegator_addr, &validator_addr, 3000, &store, 100, &config)
            .unwrap();
        execute_undelegate_stake(&delegator_addr, &validator_addr, 2000, &store, 200, &config)
            .unwrap();

        let view = StateView::new(&store);
        let entries = view.get_unbonding_entries(&delegator_addr).unwrap();
        assert_eq!(entries.len(), 2);

        // Process at the first completion height.
        let completion1 = 100 + config.unbonding_period_blocks;
        let events1 = process_mature_unbondings(&store, completion1).unwrap();
        assert_eq!(events1.len(), 1);
        assert_eq!(
            view.get_account(&delegator_addr).unwrap().unwrap().balance,
            10_000 + 3000
        );

        // Process at the second completion height.
        let completion2 = 200 + config.unbonding_period_blocks;
        let events2 = process_mature_unbondings(&store, completion2).unwrap();
        assert_eq!(events2.len(), 1);
        assert_eq!(
            view.get_account(&delegator_addr).unwrap().unwrap().balance,
            10_000 + 3000 + 2000
        );

        // All entries are gone.
        let entries = view.get_unbonding_entries(&delegator_addr).unwrap();
        assert!(entries.is_empty());
    }
}
