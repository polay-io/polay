use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{debug, info};

use polay_config::ChainConfig;
use polay_consensus::BlockProposer;
use polay_crypto::PolayKeypair;
use polay_execution::{Executor, ParallelExecutor};
use polay_mempool::Mempool;
use polay_state::StateStore;
use polay_types::block::Block;
use polay_types::hash::Hash;
use polay_types::transaction::TransactionReceipt;

use crate::error::ValidatorResult;

// ---------------------------------------------------------------------------
// BlockProducer
// ---------------------------------------------------------------------------

/// Ties the mempool, execution engine, and consensus `BlockProposer` together
/// to produce a complete block from the current chain state.
pub struct BlockProducer {
    /// Chain-wide configuration parameters.
    chain_config: ChainConfig,
    /// The validator's signing keypair.
    keypair: PolayKeypair,
}

impl BlockProducer {
    /// Create a new block producer.
    pub fn new(chain_config: ChainConfig, keypair: PolayKeypair) -> Self {
        Self {
            chain_config,
            keypair,
        }
    }

    /// Produce a new block at `height` with `parent_hash` as its parent.
    ///
    /// 1. Pulls up to `max_block_transactions` pending transactions from the
    ///    mempool (fee-prioritized).
    /// 2. Executes them against the state store via the executor.
    /// 3. Assembles the block using `BlockProposer::propose_block`.
    /// 4. Returns the block together with receipts for all included
    ///    transactions.
    #[allow(clippy::too_many_arguments)]
    pub fn produce_block(
        &self,
        height: u64,
        parent_hash: Hash,
        state_root: Hash,
        mempool: &Mempool,
        executor: &Executor,
        store: &dyn StateStore,
        chain_id: &str,
    ) -> ValidatorResult<(Block, Vec<TransactionReceipt>)> {
        let max_txs = self.chain_config.max_block_transactions;

        // 1. Pull pending transactions from the mempool.
        let pending_txs = mempool.get_pending_for_block(max_txs);
        debug!(
            height,
            pending = pending_txs.len(),
            "pulled transactions from mempool"
        );

        // 2. Execute the transactions against the current state.
        let receipts = if self.chain_config.parallel_execution {
            let par = ParallelExecutor::new(Executor::new(self.chain_config.clone()));
            let proposer_addr = self.keypair.address();
            let (par_receipts, stats) =
                par.execute_block_parallel(&pending_txs, store, height, &proposer_addr);
            info!(
                height,
                batches = stats.batch_count,
                max_batch = stats.max_batch_size,
                parallelism = format!("{:.2}", stats.parallelism_ratio),
                "parallel execution completed"
            );
            par_receipts
        } else {
            let proposer_addr = self.keypair.address();
            executor.execute_block(&pending_txs, store, height, &proposer_addr)
        };

        // 3. Compute the timestamp.
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 5. Assemble the block.
        let block = BlockProposer::propose_block(
            height,
            0, // round 0 for single-validator MVP
            parent_hash,
            state_root,
            pending_txs,
            chain_id.to_string(),
            self.keypair.address(),
            timestamp,
        );

        info!(
            height,
            hash = %block.hash(),
            txs = block.tx_count(),
            success = receipts.iter().filter(|r| r.success).count(),
            failed = receipts.iter().filter(|r| !r.success).count(),
            "block produced"
        );

        Ok((block, receipts))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_mempool::MempoolConfig;
    use polay_state::MemoryStore;
    use polay_types::{AccountState, Address, Signature, Transaction, TransactionAction};

    fn test_addr(b: u8) -> Address {
        Address::new([b; 32])
    }

    fn seed_account(store: &dyn StateStore, addr: Address, balance: u64) {
        polay_state::StateWriter::new(store)
            .set_account(&AccountState::with_balance(addr, balance, 0))
            .unwrap();
    }

    fn make_signed_tx(
        sender: Address,
        nonce: u64,
        fee: u64,
        hash_seed: u8,
    ) -> polay_types::SignedTransaction {
        let tx = Transaction {
            chain_id: "polay-devnet-1".into(),
            nonce,
            signer: sender,
            action: TransactionAction::Transfer {
                to: test_addr(0xBB),
                amount: 100,
            },
            max_fee: fee,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };
        let mut hash_bytes = [0u8; 32];
        hash_bytes[0] = hash_seed;
        polay_types::SignedTransaction::new(
            tx,
            Signature::ZERO,
            polay_types::Hash::new(hash_bytes),
            vec![0u8; 32],
        )
    }

    #[test]
    fn produce_empty_block() {
        let config = ChainConfig::default();
        let keypair = PolayKeypair::generate();
        let producer = BlockProducer::new(config.clone(), keypair);

        let store = MemoryStore::new();
        let mempool = Mempool::new(MempoolConfig::default());
        let executor = Executor::new(config.clone());

        let (block, receipts) = producer
            .produce_block(
                1,
                Hash::ZERO,
                Hash::ZERO,
                &mempool,
                &executor,
                &store,
                &config.chain_id,
            )
            .unwrap();

        assert_eq!(block.height(), 1);
        assert_eq!(block.tx_count(), 0);
        assert!(receipts.is_empty());
        assert!(!block.hash().is_zero());
    }

    #[test]
    fn produce_block_with_transactions() {
        let config = ChainConfig::default();
        let keypair = PolayKeypair::generate();
        let producer = BlockProducer::new(config.clone(), keypair);

        let store = MemoryStore::new();
        let sender = test_addr(0x01);
        seed_account(&store, sender, 100_000);

        let mempool = Mempool::new(MempoolConfig {
            min_fee: 100,
            verify_signature: false,
            ..MempoolConfig::default()
        });
        mempool.insert(make_signed_tx(sender, 0, 1_000, 1)).unwrap();
        mempool.insert(make_signed_tx(sender, 1, 2_000, 2)).unwrap();

        let executor = Executor::new(config.clone());

        let (block, receipts) = producer
            .produce_block(
                1,
                Hash::ZERO,
                Hash::ZERO,
                &mempool,
                &executor,
                &store,
                &config.chain_id,
            )
            .unwrap();

        assert_eq!(block.height(), 1);
        assert_eq!(block.tx_count(), 2);
        assert_eq!(receipts.len(), 2);
    }
}
