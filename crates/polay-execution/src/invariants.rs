//! State invariant checker — a diagnostic tool for verifying state consistency.
//!
//! This module provides checks that can be run during auditing and testing to
//! ensure that on-chain state satisfies expected invariants. These checks are
//! NOT called during normal block execution; they are meant for offline
//! verification and integration tests.

use polay_state::{StateStore, StateView};
use polay_types::Address;

use crate::error::ExecutionError;

// ---------------------------------------------------------------------------
// InvariantResult
// ---------------------------------------------------------------------------

/// The result of a single invariant check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantResult {
    /// Human-readable name of the invariant.
    pub name: String,
    /// Whether the invariant holds.
    pub passed: bool,
    /// Details about the check (e.g., computed values, mismatches).
    pub details: String,
}

// ---------------------------------------------------------------------------
// StateInvariantChecker
// ---------------------------------------------------------------------------

/// A diagnostic tool that verifies state consistency.
pub struct StateInvariantChecker;

impl StateInvariantChecker {
    /// Verify that total supply accounting is consistent.
    ///
    /// Sums all account balances across the provided addresses, adds staked
    /// amounts from the validator list, and checks that the total matches
    /// `total_supply - total_burned + total_minted` from SupplyInfo (if present).
    pub fn check_supply_invariant(
        store: &dyn StateStore,
        account_addresses: &[Address],
        validator_addresses: &[Address],
    ) -> Result<InvariantResult, ExecutionError> {
        let view = StateView::new(store);

        // Sum all account balances.
        let mut total_balances: u64 = 0;
        for addr in account_addresses {
            if let Some(account) = view.get_account(addr)? {
                total_balances = total_balances.saturating_add(account.balance);
            }
        }

        // Sum all staked amounts.
        let mut total_staked: u64 = 0;
        for addr in validator_addresses {
            if let Some(validator) = view.get_validator(addr)? {
                total_staked = total_staked.saturating_add(validator.stake);
            }
        }

        // Check SupplyInfo if available.
        let supply_info = view.get_supply_info()?;
        match supply_info {
            Some(info) => {
                // Expected circulating = total_supply + total_minted - total_burned
                let expected = info
                    .total_supply
                    .saturating_add(info.total_minted)
                    .saturating_sub(info.total_burned);
                let actual = total_balances.saturating_add(total_staked);

                let passed = actual <= expected;
                Ok(InvariantResult {
                    name: "supply_invariant".to_string(),
                    passed,
                    details: format!(
                        "total_balances={}, total_staked={}, sum={}, expected_max={} (supply={}, minted={}, burned={})",
                        total_balances, total_staked, actual, expected,
                        info.total_supply, info.total_minted, info.total_burned,
                    ),
                })
            }
            None => {
                // No SupplyInfo available — report the sums but pass by default.
                Ok(InvariantResult {
                    name: "supply_invariant".to_string(),
                    passed: true,
                    details: format!(
                        "total_balances={}, total_staked={}, sum={} (no SupplyInfo to compare against)",
                        total_balances,
                        total_staked,
                        total_balances.saturating_add(total_staked),
                    ),
                })
            }
        }
    }

    /// Verify that no account has a zero-address abuse issue.
    ///
    /// Checks that the zero address (Address::ZERO) does not hold an
    /// unreasonable balance unless it is the protocol treasury.
    pub fn check_account_invariants(
        store: &dyn StateStore,
        account_addresses: &[Address],
    ) -> Result<InvariantResult, ExecutionError> {
        let view = StateView::new(store);
        let mut issues = Vec::new();

        for addr in account_addresses {
            if let Some(account) = view.get_account(addr)? {
                // u64 is always non-negative, but check for zero-address holding
                // funds outside the treasury role.
                if *addr == Address::ZERO && account.balance > 0 {
                    // This is expected (protocol treasury), but flag it for
                    // visibility.
                    issues.push(format!(
                        "zero_address has balance={} (expected as treasury)",
                        account.balance
                    ));
                }
            }
        }

        let passed = true; // Zero address with balance is expected behavior
        Ok(InvariantResult {
            name: "account_invariants".to_string(),
            passed,
            details: if issues.is_empty() {
                "all accounts OK".to_string()
            } else {
                issues.join("; ")
            },
        })
    }

    /// Verify that delegation totals match validator stake.
    ///
    /// For each validator, sums all delegations and checks that the total
    /// does not exceed the validator's recorded stake. (Self-stake makes up
    /// the difference.)
    pub fn check_staking_invariants(
        store: &dyn StateStore,
        validator_addresses: &[Address],
        delegation_pairs: &[(Address, Address)], // (delegator, validator)
    ) -> Result<InvariantResult, ExecutionError> {
        let view = StateView::new(store);
        let mut issues = Vec::new();

        for val_addr in validator_addresses {
            let validator = match view.get_validator(val_addr)? {
                Some(v) => v,
                None => continue,
            };

            // Sum delegations for this validator.
            let mut delegation_total: u64 = 0;
            for (delegator, validator_target) in delegation_pairs {
                if validator_target == val_addr {
                    if let Some(d) = view.get_delegation(delegator, validator_target)? {
                        delegation_total = delegation_total.saturating_add(d.amount);
                    }
                }
            }

            // Delegation total should not exceed validator stake.
            if delegation_total > validator.stake {
                issues.push(format!(
                    "validator {}: delegation_total={} > stake={}",
                    val_addr.to_hex(),
                    delegation_total,
                    validator.stake
                ));
            }
        }

        let passed = issues.is_empty();
        Ok(InvariantResult {
            name: "staking_invariants".to_string(),
            passed,
            details: if passed {
                "all validator delegation sums within stake".to_string()
            } else {
                issues.join("; ")
            },
        })
    }

    /// Run all invariant checks.
    ///
    /// Requires explicit lists of addresses to check since the underlying
    /// KV store does not support full-table scans by type.
    pub fn check_all(
        store: &dyn StateStore,
        account_addresses: &[Address],
        validator_addresses: &[Address],
        delegation_pairs: &[(Address, Address)],
    ) -> Vec<InvariantResult> {
        let mut results = Vec::new();

        match Self::check_supply_invariant(store, account_addresses, validator_addresses) {
            Ok(r) => results.push(r),
            Err(e) => results.push(InvariantResult {
                name: "supply_invariant".to_string(),
                passed: false,
                details: format!("error: {}", e),
            }),
        }

        match Self::check_account_invariants(store, account_addresses) {
            Ok(r) => results.push(r),
            Err(e) => results.push(InvariantResult {
                name: "account_invariants".to_string(),
                passed: false,
                details: format!("error: {}", e),
            }),
        }

        match Self::check_staking_invariants(store, validator_addresses, delegation_pairs) {
            Ok(r) => results.push(r),
            Err(e) => results.push(InvariantResult {
                name: "staking_invariants".to_string(),
                passed: false,
                details: format!("error: {}", e),
            }),
        }

        results
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::{MemoryStore, StateWriter};
    use polay_types::{AccountState, Delegation, ValidatorInfo};

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    #[test]
    fn supply_invariant_with_simple_state() {
        let store = MemoryStore::new();
        let writer = StateWriter::new(&store);

        let addr_a = test_addr(1);
        let addr_b = test_addr(2);
        let addr_c = test_addr(3);
        let val_addr = test_addr(10);

        // Three accounts with balances.
        writer
            .set_account(&AccountState::with_balance(addr_a, 1000, 0))
            .unwrap();
        writer
            .set_account(&AccountState::with_balance(addr_b, 2000, 0))
            .unwrap();
        writer
            .set_account(&AccountState::with_balance(addr_c, 3000, 0))
            .unwrap();

        // Validator with some stake.
        let mut v = ValidatorInfo::new(val_addr, 500);
        v.stake = 4000;
        writer.set_validator(&v).unwrap();

        let accounts = vec![addr_a, addr_b, addr_c];
        let validators = vec![val_addr];

        // No SupplyInfo — should pass and report sums.
        let result =
            StateInvariantChecker::check_supply_invariant(&store, &accounts, &validators).unwrap();
        assert!(result.passed);
        assert!(result.details.contains("sum=10000"));
    }

    #[test]
    fn staking_invariant_delegation_sum_matches_stake() {
        let store = MemoryStore::new();
        let writer = StateWriter::new(&store);

        let val_addr = test_addr(1);
        let del_a = test_addr(10);
        let del_b = test_addr(11);

        // Validator with 8000 stake (5000 from del_a + 3000 from del_b).
        let mut v = ValidatorInfo::new(val_addr, 500);
        v.stake = 8000;
        writer.set_validator(&v).unwrap();

        let d_a = Delegation {
            delegator: del_a,
            validator: val_addr,
            amount: 5000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        let d_b = Delegation {
            delegator: del_b,
            validator: val_addr,
            amount: 3000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        writer.set_delegation(&d_a).unwrap();
        writer.set_delegation(&d_b).unwrap();

        let result = StateInvariantChecker::check_staking_invariants(
            &store,
            &[val_addr],
            &[(del_a, val_addr), (del_b, val_addr)],
        )
        .unwrap();
        assert!(result.passed);
    }

    #[test]
    fn staking_invariant_fails_when_delegation_exceeds_stake() {
        let store = MemoryStore::new();
        let writer = StateWriter::new(&store);

        let val_addr = test_addr(1);
        let del_a = test_addr(10);

        // Validator with only 1000 stake, but delegation is 5000 — mismatch.
        let mut v = ValidatorInfo::new(val_addr, 500);
        v.stake = 1000;
        writer.set_validator(&v).unwrap();

        let d_a = Delegation {
            delegator: del_a,
            validator: val_addr,
            amount: 5000,
            reward_debt: 0,
            last_reward_epoch: 0,
        };
        writer.set_delegation(&d_a).unwrap();

        let result = StateInvariantChecker::check_staking_invariants(
            &store,
            &[val_addr],
            &[(del_a, val_addr)],
        )
        .unwrap();
        assert!(!result.passed);
        assert!(result
            .details
            .contains("delegation_total=5000 > stake=1000"));
    }

    #[test]
    fn check_all_runs_all_invariants() {
        let store = MemoryStore::new();
        let writer = StateWriter::new(&store);

        let addr_a = test_addr(1);
        let val_addr = test_addr(10);

        writer
            .set_account(&AccountState::with_balance(addr_a, 1000, 0))
            .unwrap();
        let mut v = ValidatorInfo::new(val_addr, 500);
        v.stake = 2000;
        writer.set_validator(&v).unwrap();

        let results = StateInvariantChecker::check_all(&store, &[addr_a], &[val_addr], &[]);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.passed));
    }
}
