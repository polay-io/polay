//! Parallel scheduling — partition transactions into non-conflicting batches.
//!
//! The scheduler uses a greedy first-fit algorithm: for each transaction it
//! tries to place it in the earliest batch that has no access-set conflicts.
//! If no such batch exists a new batch is created.

use crate::access_set::{predict_access_set, AccessSet};
use polay_types::SignedTransaction;

/// A group of transactions that can be executed in parallel (no mutual
/// conflicts).
#[derive(Debug)]
pub struct ExecutionBatch {
    /// Pairs of (original_index, transaction).  The original index is needed to
    /// reassemble receipts in block order after parallel execution.
    pub transactions: Vec<(usize, SignedTransaction)>,
}

/// Schedule transactions into parallel batches.
///
/// Uses a greedy algorithm:
/// 1. For each transaction, predict its access set.
/// 2. Try to add it to the earliest batch that has no conflicts.
/// 3. If it conflicts with every existing batch, start a new one.
///
/// Returns ordered batches.  Transactions *within* a batch can run in parallel.
/// Batches themselves must be executed sequentially.
pub fn schedule_parallel(transactions: &[SignedTransaction]) -> Vec<ExecutionBatch> {
    if transactions.is_empty() {
        return vec![];
    }

    let mut batches: Vec<ExecutionBatch> = Vec::new();
    // Mirror vec: one AccessSet vec per batch.
    let mut batch_access_sets: Vec<Vec<AccessSet>> = Vec::new();

    for (idx, tx) in transactions.iter().enumerate() {
        let tx_access = predict_access_set(tx);
        let mut placed = false;

        for (batch_idx, batch) in batches.iter_mut().enumerate() {
            let conflicts = batch_access_sets[batch_idx]
                .iter()
                .any(|s| s.conflicts_with(&tx_access));

            if !conflicts {
                batch.transactions.push((idx, tx.clone()));
                batch_access_sets[batch_idx].push(tx_access.clone());
                placed = true;
                break;
            }
        }

        if !placed {
            batch_access_sets.push(vec![tx_access]);
            batches.push(ExecutionBatch {
                transactions: vec![(idx, tx.clone())],
            });
        }
    }

    batches
}

// ---------------------------------------------------------------------------
// ScheduleStats
// ---------------------------------------------------------------------------

/// Summary statistics about a parallel schedule.
#[derive(Debug, Clone)]
pub struct ScheduleStats {
    /// Total number of transactions across all batches.
    pub total_transactions: usize,
    /// Number of sequential batches.
    pub batch_count: usize,
    /// Size of the largest batch.
    pub max_batch_size: usize,
    /// `total_transactions / batch_count` — higher means more parallelism.
    pub parallelism_ratio: f64,
}

/// Compute summary statistics from a list of batches.
pub fn schedule_stats(batches: &[ExecutionBatch]) -> ScheduleStats {
    let total: usize = batches.iter().map(|b| b.transactions.len()).sum();
    let max_size = batches
        .iter()
        .map(|b| b.transactions.len())
        .max()
        .unwrap_or(0);
    ScheduleStats {
        total_transactions: total,
        batch_count: batches.len(),
        max_batch_size: max_size,
        parallelism_ratio: if batches.is_empty() {
            0.0
        } else {
            total as f64 / batches.len() as f64
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_types::{Address, Hash, Signature, Transaction, TransactionAction};

    fn addr(b: u8) -> Address {
        Address::new([b; 32])
    }

    fn make_stx(signer: Address, action: TransactionAction, hash_seed: u8) -> SignedTransaction {
        let mut h = [0u8; 32];
        h[0] = hash_seed;
        SignedTransaction::new(
            Transaction {
                chain_id: "test".into(),
                nonce: 0,
                signer,
                action,
                max_fee: 1_000_000,
                timestamp: 1,
                session: None,
                sponsor: None,
            },
            Signature::ZERO,
            Hash::new(h),
            vec![0u8; 32],
        )
    }

    #[test]
    fn empty_transactions_produce_no_batches() {
        let batches = schedule_parallel(&[]);
        assert!(batches.is_empty());
    }

    #[test]
    fn single_transaction_one_batch() {
        let tx = make_stx(
            addr(1),
            TransactionAction::Transfer {
                to: addr(2),
                amount: 10,
            },
            1,
        );
        let batches = schedule_parallel(&[tx]);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].transactions.len(), 1);
    }

    #[test]
    fn independent_transactions_single_batch() {
        // Four completely independent transfers: A->B, C->D, E->F, G->H
        let txs: Vec<SignedTransaction> = (0..4u8)
            .map(|i| {
                make_stx(
                    addr(i * 2 + 1),
                    TransactionAction::Transfer {
                        to: addr(i * 2 + 2),
                        amount: 10,
                    },
                    i,
                )
            })
            .collect();

        let batches = schedule_parallel(&txs);
        assert_eq!(
            batches.len(),
            1,
            "all independent transfers should fit in a single batch"
        );
        assert_eq!(batches[0].transactions.len(), 4);
    }

    #[test]
    fn conflicting_transactions_multiple_batches() {
        // Same signer: must be serialised.
        let txs = vec![
            make_stx(
                addr(1),
                TransactionAction::Transfer {
                    to: addr(2),
                    amount: 10,
                },
                1,
            ),
            make_stx(
                addr(1),
                TransactionAction::Transfer {
                    to: addr(3),
                    amount: 20,
                },
                2,
            ),
            make_stx(
                addr(1),
                TransactionAction::Transfer {
                    to: addr(4),
                    amount: 30,
                },
                3,
            ),
        ];
        let batches = schedule_parallel(&txs);
        assert_eq!(
            batches.len(),
            3,
            "three txs from the same signer need three batches"
        );
    }

    #[test]
    fn mixed_independent_and_conflicting() {
        // tx0: addr(1) -> addr(2)    (batch 0)
        // tx1: addr(3) -> addr(4)    (batch 0 — no conflict with tx0)
        // tx2: addr(1) -> addr(5)    (batch 1 — conflicts with tx0 via signer)
        // tx3: addr(6) -> addr(7)    (batch 0 — independent of everything else)
        let txs = vec![
            make_stx(
                addr(1),
                TransactionAction::Transfer {
                    to: addr(2),
                    amount: 10,
                },
                1,
            ),
            make_stx(
                addr(3),
                TransactionAction::Transfer {
                    to: addr(4),
                    amount: 10,
                },
                2,
            ),
            make_stx(
                addr(1),
                TransactionAction::Transfer {
                    to: addr(5),
                    amount: 10,
                },
                3,
            ),
            make_stx(
                addr(6),
                TransactionAction::Transfer {
                    to: addr(7),
                    amount: 10,
                },
                4,
            ),
        ];
        let batches = schedule_parallel(&txs);
        assert_eq!(batches.len(), 2, "expected 2 batches for mixed workload");
        // Batch 0 should contain indices 0, 1, 3.
        assert_eq!(batches[0].transactions.len(), 3);
        // Batch 1 should contain index 2.
        assert_eq!(batches[1].transactions.len(), 1);
    }

    #[test]
    fn original_indices_preserved() {
        let txs = vec![
            make_stx(
                addr(1),
                TransactionAction::Transfer {
                    to: addr(2),
                    amount: 10,
                },
                1,
            ),
            make_stx(
                addr(1),
                TransactionAction::Transfer {
                    to: addr(3),
                    amount: 20,
                },
                2,
            ),
        ];
        let batches = schedule_parallel(&txs);
        assert_eq!(batches[0].transactions[0].0, 0);
        assert_eq!(batches[1].transactions[0].0, 1);
    }

    #[test]
    fn schedule_stats_calculation() {
        let txs: Vec<SignedTransaction> = (0..5u8)
            .map(|i| {
                make_stx(
                    addr(i * 2 + 1),
                    TransactionAction::Transfer {
                        to: addr(i * 2 + 2),
                        amount: 10,
                    },
                    i,
                )
            })
            .collect();
        let batches = schedule_parallel(&txs);
        let stats = schedule_stats(&batches);
        assert_eq!(stats.total_transactions, 5);
        assert_eq!(stats.batch_count, 1);
        assert_eq!(stats.max_batch_size, 5);
        assert!((stats.parallelism_ratio - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn schedule_stats_empty() {
        let stats = schedule_stats(&[]);
        assert_eq!(stats.total_transactions, 0);
        assert_eq!(stats.batch_count, 0);
        assert_eq!(stats.max_batch_size, 0);
        assert!((stats.parallelism_ratio - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn transfers_to_same_address_sequentialised() {
        // A -> C and B -> C must go to different batches.
        let txs = vec![
            make_stx(
                addr(1),
                TransactionAction::Transfer {
                    to: addr(3),
                    amount: 100,
                },
                1,
            ),
            make_stx(
                addr(2),
                TransactionAction::Transfer {
                    to: addr(3),
                    amount: 200,
                },
                2,
            ),
        ];
        let batches = schedule_parallel(&txs);
        assert_eq!(
            batches.len(),
            2,
            "two transfers to the same address must be in separate batches"
        );
    }

    #[test]
    fn transfers_to_different_addresses_parallel() {
        // A -> B and C -> D should fit in one batch.
        let txs = vec![
            make_stx(
                addr(1),
                TransactionAction::Transfer {
                    to: addr(2),
                    amount: 100,
                },
                1,
            ),
            make_stx(
                addr(3),
                TransactionAction::Transfer {
                    to: addr(4),
                    amount: 200,
                },
                2,
            ),
        ];
        let batches = schedule_parallel(&txs);
        assert_eq!(
            batches.len(),
            1,
            "transfers to different addresses should be parallel"
        );
    }
}
