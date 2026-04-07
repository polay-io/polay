# Architecture

This document describes the system architecture of POLAY: the monorepo structure, crate dependency graph, data flow through the system, and the key design decisions that shaped the implementation.

## Monorepo Structure

```
polay/
|
|-- polay-types/          Core data types, traits, error types, serialization
|-- polay-crypto/         Ed25519 key pairs, signing, verification, hashing (BLAKE3)
|-- polay-consensus/      DPoS + BFT consensus engine, validator set, proposer rotation
|-- polay-execution/      Transaction processing pipeline, module dispatch, fee model
|-- polay-state/          State storage abstraction, RocksDB backend, key scheme
|-- polay-network/        libp2p-based P2P networking, gossipsub, peer discovery
|-- polay-rpc/            JSON-RPC server (axum + jsonrpsee), external client interface
|-- polay-node/           Node binary, component wiring, startup sequence
|-- polay-genesis/        Genesis configuration, initial state generation
|-- polay-cli/            Developer CLI for key generation, genesis, tx submission
|-- docs/                 Architecture documentation (this directory)
|-- Cargo.toml            Workspace root
|-- docker-compose.yml    Local devnet orchestration
```

## Crate Dependency Graph

Dependencies flow downward. Higher-level crates depend on lower-level ones. No circular dependencies exist.

```
                        polay-node
                       /    |     \
                      /     |      \
               polay-rpc  polay-network  polay-consensus
                  |         |              |
                  |         |              |
               polay-execution         polay-execution
                  |                       |
                  |                       |
               polay-state            polay-state
                  |                       |
                  |                       |
               polay-types            polay-types
                  |                       |
               polay-crypto           polay-crypto
```

### Detailed dependencies

| Crate | Depends on | Purpose |
|---|---|---|
| `polay-types` | `polay-crypto` | Defines `Transaction`, `Block`, `Account`, `AssetClass`, `Listing`, `Profile`, `MatchResult`, `ValidatorInfo`, and all shared enums. Uses `polay-crypto` for `PublicKey`, `Signature`, `Hash` types. |
| `polay-crypto` | (external only: `ed25519-dalek`, `blake3`) | Pure cryptographic primitives. No chain-specific logic. |
| `polay-state` | `polay-types` | State storage with RocksDB. Defines the key namespace scheme. Implements `get`/`put`/`delete` over typed keys. |
| `polay-execution` | `polay-types`, `polay-state`, `polay-crypto` | The execution engine. Decodes transactions, validates them, dispatches to module handlers, applies state transitions, collects events and fee payments. |
| `polay-consensus` | `polay-types`, `polay-execution`, `polay-state`, `polay-crypto` | Drives the block lifecycle (Propose/Prevote/Precommit/Commit). Calls into `polay-execution` to execute blocks. Reads validator set from `polay-state`. |
| `polay-network` | `polay-types`, `polay-crypto` | P2P message serialization and transport. Publishes and subscribes to gossipsub topics for transactions and blocks. Does not execute or validate -- just relays. |
| `polay-rpc` | `polay-types`, `polay-state`, `polay-execution` | Exposes the JSON-RPC interface. Reads state for queries. Accepts transaction submissions and forwards them to the mempool. |
| `polay-node` | All crates | The final binary. Wires together consensus, networking, RPC, execution, and state. Manages the event loop and component lifecycle. |
| `polay-genesis` | `polay-types`, `polay-state`, `polay-crypto` | Reads a genesis configuration (JSON) and produces the initial state: genesis accounts, initial validator set, chain parameters. |
| `polay-cli` | `polay-types`, `polay-crypto`, `polay-genesis` | Developer tooling. Key generation, genesis file creation, transaction construction and submission. |

## Data Flow

### Transaction lifecycle

A transaction flows through the system in the following stages:

```
Client (game server, wallet, CLI)
  |
  | JSON-RPC: polay_submitTransaction
  v
RPC Server (polay-rpc)
  |
  | Stateless validation (signature, format, nonce format)
  v
Mempool (in polay-node)
  |
  | Gossip to peers via polay-network
  v
Block Proposer (polay-consensus)
  |
  | Select transactions from mempool, build block proposal
  v
Consensus (DPoS + BFT rounds)
  |
  | Prevote -> Precommit -> Commit (2/3+ stake quorum)
  v
Execution (polay-execution)
  |
  | For each tx: decode -> stateful validate -> execute -> emit events
  v
State Commit (polay-state)
  |
  | Write state diffs to RocksDB
  v
Event Emission
  |
  | Structured events for indexer consumption
  v
Indexer (external, reads events)
  |
  | Populates PostgreSQL for rich queries
  v
Query Clients (dashboards, game backends, explorers)
```

### Block production flow

1. The consensus module determines the proposer for the current height (round-robin by validator stake order).
2. The proposer selects pending transactions from the mempool, ordered by nonce per sender and fee priority.
3. The proposer constructs a `Block` containing the transaction list, previous block hash, height, timestamp, and proposer signature.
4. The block proposal is broadcast to all validators via the P2P network.
5. Validators execute the block locally (via `polay-execution`) to verify the state transition.
6. Validators cast prevotes if the block is valid.
7. Upon receiving 2/3+ prevotes by stake weight, validators cast precommits.
8. Upon receiving 2/3+ precommits, the block is committed: state diffs are written to RocksDB, events are emitted, and the block is appended to the chain.

### Query flow

```
Client
  |
  | JSON-RPC: polay_getBalance, polay_getAssetClass, etc.
  v
RPC Server
  |
  | Direct read from polay-state (RocksDB)
  v
Response to client
```

Queries are read-only and do not go through consensus. They reflect the latest committed state.

## Module System

POLAY uses **native modules** instead of a smart contract VM. Each module is a Rust implementation that handles a subset of transaction types.

| Module | Transaction actions handled | State domains |
|---|---|---|
| **Assets** | `CreateAssetClass`, `MintAsset`, `TransferAsset`, `BurnAsset` | `asset_classes`, `asset_balances` |
| **Market** | `ListAsset`, `BuyAsset`, `DelistAsset` | `listings` |
| **Identity** | `RegisterProfile`, `UpdateProfile`, `RecordAchievement` | `profiles`, `achievements` |
| **Staking** | `RegisterValidator`, `Delegate`, `Undelegate`, `ClaimRewards` | `validators`, `delegations`, `unbonding` |
| **Attestation** | `RegisterAttestor`, `SubmitMatchResult` | `attestors`, `match_results` |

The execution engine reads the `action` field of each transaction and dispatches to the corresponding module. Modules receive a mutable reference to the state store and return a `Vec<Event>` on success or an `ExecutionError` on failure.

This design has tradeoffs:

- **Advantage:** No VM overhead, no gas metering complexity, no reentrancy risks, simple auditing.
- **Advantage:** Module code is compiled Rust, giving maximum throughput for the core feature set.
- **Disadvantage:** Adding new functionality requires a chain upgrade (new node binary). Third parties cannot deploy custom logic.
- **Mitigation:** Phase 2 introduces a WASM VM for user-deployed contracts, while core modules remain native for performance.

## Storage Layer

### RocksDB

Node state is stored in RocksDB, a high-performance embedded key-value store. The state layer (`polay-state`) provides a typed interface over raw byte keys.

Key design:
- All keys use a **prefix byte** to namespace state domains (e.g., `0x01` for accounts, `0x02` for asset classes).
- Values are serialized with **Borsh** (Binary Object Representation Serializer for Hashing) for deterministic, compact encoding.
- State reads and writes happen through a `StateStore` trait, making the storage backend swappable for testing (in-memory `HashMap` implementation exists).

### PostgreSQL Indexer (planned)

The RPC layer serves basic key-value lookups. For complex queries (list all assets owned by a player, search marketplace listings by price range, leaderboard queries), an external indexer consumes events from committed blocks and populates a PostgreSQL database.

The indexer is not part of consensus and does not affect chain correctness. It is a read-only derived view of chain state.

## Networking

### P2P with libp2p

Node-to-node communication uses libp2p with the following configuration:

- **Transport:** TCP with Noise encryption for authenticated, encrypted channels.
- **Peer discovery:** Bootstrap nodes in genesis config. mDNS for local devnet discovery. Kademlia DHT planned for mainnet.
- **Message propagation:** Gossipsub protocol with two topics:
  - `/polay/txs/1` -- new transactions broadcast from any node
  - `/polay/blocks/1` -- new block proposals and consensus messages
- **Message validation:** Nodes validate message format before relaying. Invalid messages are dropped and the peer is scored down.

### RPC

The JSON-RPC server runs on each node (configurable port, default 9944). Built with `axum` for HTTP and `jsonrpsee` for JSON-RPC 2.0 compliance.

- Read methods query committed state directly from RocksDB.
- Write methods (`polay_submitTransaction`) inject transactions into the local mempool.
- No authentication in MVP. Rate limiting and API keys planned for public testnet.

gRPC for internal node-to-node communication (separate from P2P gossip) is designed but not yet implemented. It would be used for state sync, snapshot transfer, and validator coordination.

## Key Design Decisions

### Why Borsh over Protobuf or Bincode?

Borsh provides **deterministic serialization** -- the same struct always produces the same bytes. This is critical for consensus: all validators must compute the same state root from the same block. Protobuf does not guarantee field ordering. Bincode is deterministic but Borsh has better cross-language support (important for future SDK work) and is battle-tested in the Solana/NEAR ecosystem.

### Why RocksDB over LevelDB or SQLite?

RocksDB handles the write-heavy workload of blockchain state better than LevelDB (more tunable compaction) and provides better concurrent read performance than SQLite. It is the standard choice for blockchain node storage (used by Ethereum, Solana, Sui).

### Why native modules over a VM from day one?

Iteration speed. Building the gaming feature set (assets, market, identity, attestation) directly in Rust lets the team move fast, profile easily, and change the data model without migration tooling. The VM layer adds complexity that is not justified until third-party extensibility is needed.

### Why DPoS + BFT over Nakamoto consensus?

Gaming needs fast finality. Nakamoto consensus (longest chain) provides probabilistic finality -- a trade is not truly final for minutes. DPoS + BFT provides deterministic finality in a single slot. The validator set is smaller but the trust model is appropriate for a gaming chain where validators are known entities (game studios, infrastructure providers, ecosystem partners).

### Why a single binary (`polay-node`) instead of microservices?

Simplicity for the MVP. A single process with in-memory channels between components avoids the complexity of service discovery, network partitions between internal services, and deployment orchestration. The crate boundaries enforce modularity at the code level even within a single binary.
