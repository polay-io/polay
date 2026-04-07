use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;

/// On-chain account state tracking balance and nonce.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct AccountState {
    /// The account's address.
    pub address: Address,
    /// Monotonically increasing nonce — each accepted transaction increments it.
    pub nonce: u64,
    /// Native token balance (in the smallest unit).
    pub balance: u64,
    /// Unix timestamp (seconds) when this account was first seen on-chain.
    pub created_at: u64,
}

impl AccountState {
    /// Create a new account state with the given address, zero balance, and
    /// nonce 0.
    pub fn new(address: Address, created_at: u64) -> Self {
        Self {
            address,
            nonce: 0,
            balance: 0,
            created_at,
        }
    }

    /// Create a new account state with an initial balance (e.g., from genesis).
    pub fn with_balance(address: Address, balance: u64, created_at: u64) -> Self {
        Self {
            address,
            nonce: 0,
            balance,
            created_at,
        }
    }

    /// Returns `true` if the account has enough balance to cover `amount`.
    pub fn can_afford(&self, amount: u64) -> bool {
        self.balance >= amount
    }

    /// Debit the account, returning an error string if insufficient.
    pub fn debit(&mut self, amount: u64) -> Result<(), String> {
        self.balance = self.balance.checked_sub(amount).ok_or_else(|| {
            format!(
                "insufficient balance: required {}, available {}",
                amount, self.balance
            )
        })?;
        Ok(())
    }

    /// Credit the account.
    pub fn credit(&mut self, amount: u64) {
        self.balance = self.balance.saturating_add(amount);
    }

    /// Increment the nonce (call after a transaction is accepted).
    pub fn increment_nonce(&mut self) {
        self.nonce += 1;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_account_defaults() {
        let acct = AccountState::new(Address::ZERO, 1000);
        assert_eq!(acct.nonce, 0);
        assert_eq!(acct.balance, 0);
    }

    #[test]
    fn debit_and_credit() {
        let mut acct = AccountState::with_balance(Address::ZERO, 500, 1000);
        acct.credit(200);
        assert_eq!(acct.balance, 700);
        acct.debit(300).unwrap();
        assert_eq!(acct.balance, 400);
        assert!(acct.debit(1000).is_err());
    }

    #[test]
    fn nonce_increment() {
        let mut acct = AccountState::new(Address::ZERO, 0);
        acct.increment_nonce();
        acct.increment_nonce();
        assert_eq!(acct.nonce, 2);
    }

    #[test]
    fn serde_round_trip() {
        let acct = AccountState::with_balance(Address::ZERO, 1_000_000, 12345);
        let json = serde_json::to_string(&acct).unwrap();
        let parsed: AccountState = serde_json::from_str(&json).unwrap();
        assert_eq!(acct, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let acct = AccountState::with_balance(Address::ZERO, 42, 99);
        let encoded = borsh::to_vec(&acct).unwrap();
        let decoded = AccountState::try_from_slice(&encoded).unwrap();
        assert_eq!(acct, decoded);
    }
}
