# Architecture Overview

POLAY is structured as a Rust monorepo with 16 crates. Each crate has a single responsibility and well-defined dependency boundaries.

## Crate Map

| Crate | Purpose |
|---|---|
| `polay-types` | Core types: `Address`, `Hash`, `Signature`, `Transaction`, `Block`, `Action` enums |
| `polay-crypto` | Ed25519 signing/verification, hashing (Blake3), Merkle tree construction |
| `polay-config` | Chain configuration, genesis parameters, network settings |
| `polay-genesis` | Genesis block builder: initial accounts, validators, staking state |
| `polay-state` | RocksDB-backed state store, `OverlayStore`, snapshot/sync |
| `polay-mempool` | Transaction pool with priority ordering, validation, eviction |
| `polay-execution` | Parallel transaction execution, gas metering, fee distribution |
| `polay-consensus` | BFT consensus engine (Tendermint-style rounds) |
| `polay-network` | libp2p networking: gossipsub, peer discovery, rate limiting |
| `polay-rpc` | JSON-RPC and WebSocket server for client interaction |
| `polay-validator` | Validator lifecycle: key management, block proposal, voting |
| `polay-staking` | DPoS staking: delegation, unbonding, slashing, epoch transitions |
| `polay-attestation` | Game result attestation: attestor registry, result submission |
| `polay-market` | Marketplace: listings, purchases, auctions, rentals |
| `polay-identity` | Identity system: usernames, profiles, reputation |
| `polay-node` | Top-level binary: wires all crates together, CLI interface |

## Dependency Graph (simplified)

```
polay-node
  +-- polay-rpc
  +-- polay-validator
  +-- polay-consensus
  |     +-- polay-execution
  |     |     +-- polay-staking
  |     |     +-- polay-attestation
  |     |     +-- polay-market
  |     |     +-- polay-identity
  |     |     +-- polay-state
  |     +-- polay-network
  |     +-- polay-mempool
  +-- polay-genesis
  +-- polay-config
  +-- polay-crypto
  +-- polay-types
```

All crates depend on `polay-types` and `polay-crypto` at the bottom of the graph.

## Data Flow: Transaction Lifecycle

1. **Submission.** A client sends a signed transaction via JSON-RPC (`submit_transaction`) or WebSocket.
2. **Mempool.** `polay-rpc` forwards the transaction to `polay-mempool`. The mempool validates the signature, checks the nonce, verifies the sender has sufficient balance for gas, and inserts it into the priority queue.
3. **Block proposal.** When this validator is the proposer for the current round, `polay-consensus` pulls transactions from the mempool and constructs a candidate block.
4. **Consensus.** The BFT engine runs: Propose -> Prevote -> Precommit -> Commit. Validators exchange votes over gossipsub. A block is finalized when 2/3+ of stake signs precommit.
5. **Execution.** `polay-execution` processes the finalized block. Independent transactions run in parallel (rayon). Each action is metered for gas. State changes are applied to an `OverlayStore`.
6. **State commit.** The overlay is flushed to RocksDB. A new Merkle state root is computed and stored in the block header. The mempool is pruned of included transactions.

## Block Structure

```rust
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub evidence: Vec<Evidence>,      // equivocation proofs
    pub last_commit: Option<Commit>,  // votes for previous block
}

pub struct BlockHeader {
    pub height: u64,
    pub timestamp: u64,
    pub prev_hash: Hash,
    pub state_root: Hash,
    pub tx_root: Hash,
    pub proposer: Address,
    pub chain_id: String,
}
```

## Configuration

Chain parameters live in `polay-config` and are loaded from `config.toml`:

```toml
[chain]
chain_id = "polay-devnet-1"
block_time_ms = 1500
max_block_size = 1048576  # 1 MB
max_txs_per_block = 1000

[staking]
epoch_length = 100
unbonding_period = 100
slash_fraction = 0.05
jail_duration = 1800

[network]
listen_addr = "/ip4/0.0.0.0/tcp/26656"
max_peers = 50
```

See the individual architecture pages for deep dives into each subsystem.
