//! Native token transfer execution.

use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{AccountState, Address, Event};
use tracing::debug;

use crate::error::ExecutionError;

/// Execute a native token transfer from `signer` to `to`.
///
/// - Debits `amount` from the sender's balance.
/// - Credits `amount` to the receiver (creating the account if it does not exist).
/// - Emits a transfer event.
pub fn execute_transfer(
    signer: &Address,
    to: &Address,
    amount: u64,
    store: &dyn StateStore,
    timestamp: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Load sender account.
    let mut sender = view
        .get_account(signer)?
        .ok_or_else(|| ExecutionError::AccountNotFound(signer.to_hex()))?;

    if sender.balance < amount {
        return Err(ExecutionError::InsufficientBalance {
            required: amount,
            available: sender.balance,
        });
    }

    // Debit sender.
    sender.balance = sender.balance
        .checked_sub(amount)
        .ok_or(ExecutionError::InsufficientBalance {
            required: amount,
            available: sender.balance,
        })?;
    writer.set_account(&sender)?;

    // Credit receiver (create if not exists).
    let mut receiver = view
        .get_account(to)?
        .unwrap_or_else(|| AccountState::new(*to, timestamp));
    receiver.balance = receiver.balance.saturating_add(amount);
    writer.set_account(&receiver)?;

    debug!(
        from = %signer,
        to = %to,
        amount,
        "transfer executed"
    );

    Ok(vec![Event::transfer(signer, to, amount)])
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

    #[test]
    fn transfer_happy_path() {
        let store = MemoryStore::new();
        let writer = StateWriter::new(&store);

        let sender = test_addr(1);
        let receiver = test_addr(2);

        // Seed sender with 1000.
        writer
            .set_account(&AccountState::with_balance(sender, 1000, 0))
            .unwrap();

        let events = execute_transfer(&sender, &receiver, 300, &store, 100).unwrap();

        let view = StateView::new(&store);
        assert_eq!(view.get_account(&sender).unwrap().unwrap().balance, 700);
        assert_eq!(view.get_account(&receiver).unwrap().unwrap().balance, 300);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "bank");
        assert_eq!(events[0].action, "transfer");
    }

    #[test]
    fn transfer_creates_receiver_account() {
        let store = MemoryStore::new();
        let sender = test_addr(1);
        let receiver = test_addr(2);

        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(sender, 500, 0))
            .unwrap();

        // Receiver has no account yet.
        assert!(StateView::new(&store).get_account(&receiver).unwrap().is_none());

        execute_transfer(&sender, &receiver, 100, &store, 42).unwrap();

        let recv_acct = StateView::new(&store).get_account(&receiver).unwrap().unwrap();
        assert_eq!(recv_acct.balance, 100);
        assert_eq!(recv_acct.created_at, 42);
    }

    #[test]
    fn transfer_insufficient_balance() {
        let store = MemoryStore::new();
        let sender = test_addr(1);
        let receiver = test_addr(2);

        StateWriter::new(&store)
            .set_account(&AccountState::with_balance(sender, 50, 0))
            .unwrap();

        let err = execute_transfer(&sender, &receiver, 100, &store, 0).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { required: 100, available: 50 }));
    }

    #[test]
    fn transfer_sender_not_found() {
        let store = MemoryStore::new();
        let err = execute_transfer(&test_addr(1), &test_addr(2), 100, &store, 0).unwrap_err();
        assert!(matches!(err, ExecutionError::AccountNotFound(_)));
    }
}
