# POLAY

A gaming-native Layer 1 blockchain.

## Overview

POLAY is a sovereign Layer 1 blockchain purpose-built for the economics of interactive entertainment. It provides the settlement layer for game asset ownership, player rewards, marketplace trading, and competitive integrity -- while leaving gameplay simulation entirely offchain where it belongs.

The core principle: **games run offchain, truth lives onchain.** A first-person shooter does not need consensus to register a headshot, but it does need consensus to award the tournament prize, transfer the rare weapon skin, and record the match result. POLAY draws this line deliberately. Gameplay stays fast and server-authoritative. Economic outcomes settle onchain with finality.

The native token is **POL**, used for transaction fees, staking, validator rewards, and marketplace settlement.

## Architecture

POLAY uses a module-based architecture instead of a smart contract VM. Each subsystem (assets, marketplace, identity, attestation, staking) is a native Rust module with its own transaction handlers, state namespace, and event types. The execution engine dispatches transactions to the appropriate module, enforces deterministic processing, and produces state diffs that get committed after consensus.

### Crate Map

| Crate | Description |
|---|---|
| `polay-types` | Shared data types, traits, error types, and serialization formats |
| `polay-crypto` | Ed25519 key generation, signing, verification, and BLAKE3 hashing |
| `polay-config` | Node and chain configuration, CLI argument parsing |
| `polay-genesis` | Genesis block generation and initial state setup |
| `polay-state` | State storage abstraction with RocksDB backend and prefix key scheme |
| `polay-mempool` | Transaction pool with ordering, deduplication, and eviction |
| `polay-execution` | Transaction processing pipeline, module dispatch, and fee model |
| `polay-consensus` | DPoS + BFT consensus engine with round-robin proposer rotation |
| `polay-network` | libp2p-based P2P networking with gossipsub and mDNS discovery |
| `polay-rpc` | JSON-RPC 2.0 server built on jsonrpsee for external clients |
| `polay-validator` | Validator node logic, block production, and vote management |
| `polay-staking` | Stake delegation, reward distribution, and epoch transitions |
| `polay-attestation` | Game server attestor registration, match result settlement, anti-cheat |
| `polay-market` | Onchain marketplace: listings, purchases, fee splitting |
| `polay-identity` | Player profiles, display names, achievements, metadata |
| `polay-node` | Node binary, component wiring, and startup orchestration |

## Getting Started

### Prerequisites

- **Rust 1.77+** -- install via [rustup](https://rustup.rs/)
- **Docker** (optional) -- for running the multi-validator devnet

### Build

```bash
cargo build --release
```

The binary is produced at `target/release/polay`.

### Initialize Devnet

Generate validator keys and the genesis configuration for a local 4-node devnet:

```bash
./scripts/init-devnet.sh
```

Or do it manually:

```bash
# Generate a key pair
cargo run --release --bin polay -- keygen --output my-key.key

# Generate genesis with that validator
cargo run --release --bin polay -- init \
    --chain-id polay-devnet-1 \
    --output genesis.json \
    --initial-supply 100000000000000000 \
    --block-time 2000 \
    --validator <ADDRESS>
```

### Run a Local Node

Start a single validator for local development:

```bash
./scripts/start-local.sh
```

Or run directly:

```bash
cargo run --release --bin polay -- run \
    --genesis devnet-data/genesis.json \
    --data-dir devnet-data/validator-1 \
    --rpc-addr 127.0.0.1:9944 \
    --validator-key devnet-data/keys/validator-1.key \
    --block-time 2000
```

### Run the 4-Node Devnet

```bash
# Initialize (if not done already)
./scripts/init-devnet.sh

# Start all four validators via Docker Compose
docker compose up --build
```

Validators are available at ports 9944-9947.

### Generate Keys

```bash
cargo run --release --bin polay -- keygen --output my-key.key
```

## RPC API

POLAY exposes a JSON-RPC 2.0 interface on each node (default port 9944). All requests use `POST` with `Content-Type: application/json`.

### Get Chain Info

```bash
curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "polay_getChainInfo",
    "params": {},
    "id": 1
  }' | jq
```

### Get Balance

```bash
curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "polay_getBalance",
    "params": {"address": "a1b2c3d4..."},
    "id": 2
  }' | jq
```

### Submit a Transaction

```bash
curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "polay_submitTransaction",
    "params": {
      "sender": "a1b2c3d4...",
      "nonce": 0,
      "action": {
        "type": "Transfer",
        "to": "e5f6a7b8...",
        "amount": "1000000"
      },
      "max_fee": "100",
      "signature": "..."
    },
    "id": 3
  }' | jq
```

See [docs/rpc.md](docs/rpc.md) for the complete RPC specification with all 17 methods, request/response formats, and error codes.

## Project Structure

```
polay/
+-- Cargo.toml                Workspace root
+-- Cargo.lock
+-- Dockerfile                Multi-stage build for the node binary
+-- docker-compose.yml        4-node local devnet orchestration
+-- README.md
|
+-- crates/
|   +-- polay-types/          Core data types, traits, serialization
|   +-- polay-crypto/         Ed25519 keys, signing, BLAKE3 hashing
|   +-- polay-config/         Node and chain configuration
|   +-- polay-genesis/        Genesis block generation
|   +-- polay-state/          State storage (RocksDB backend)
|   +-- polay-mempool/        Transaction pool
|   +-- polay-execution/      Transaction processing, module dispatch
|   +-- polay-consensus/      DPoS + BFT consensus engine
|   +-- polay-network/        libp2p P2P networking
|   +-- polay-rpc/            JSON-RPC 2.0 server
|   +-- polay-validator/      Validator block production and voting
|   +-- polay-staking/        Stake delegation and rewards
|   +-- polay-attestation/    Game server attestation, anti-cheat
|   +-- polay-market/         Onchain asset marketplace
|   +-- polay-identity/       Player profiles and achievements
|   +-- polay-node/           Node binary and startup orchestration
|
+-- sdk/
|   +-- ts/                   TypeScript SDK (@polay/sdk)
|
+-- apps/
|   +-- devnet/               Devnet configuration
|   +-- explorer-api/         Block explorer API backend
|   +-- indexer/              PostgreSQL indexer service
|
+-- scripts/
|   +-- init-devnet.sh        Initialize devnet keys and genesis
|   +-- start-local.sh        Start a single local validator
|   +-- sample-transactions.sh  Send sample RPC requests
|
+-- docs/
    +-- overview.md           What POLAY is and why
    +-- architecture.md       Crate dependency graph and data flow
    +-- consensus.md          DPoS + BFT consensus protocol
    +-- execution.md          Transaction processing pipeline
    +-- state.md              State storage and key scheme
    +-- staking.md            Validator economics and delegation
    +-- attestation.md        Game server attestation and anti-cheat
    +-- rpc.md                Full RPC specification
    +-- local-devnet.md       Local devnet setup guide
    +-- roadmap.md            Development roadmap (Phase 0-5)
```

## Core Modules

### Polay Chain (Consensus + Block Production)

The base blockchain layer. A DPoS + BFT consensus engine with round-robin proposer rotation and single-slot finality. Validators stake POL to participate in consensus, produce blocks on a 2-second cadence, and finalize blocks through a prevote/precommit round. The P2P layer uses libp2p with gossipsub for transaction and block propagation across the validator set.

### Polay Engine (Execution)

The transaction processing pipeline. Every transaction flows through: decode, stateless validation (signature, format), stateful validation (balance checks, permission checks), execution (state mutations), and event emission. The engine dispatches each transaction to the appropriate module handler based on the action type. Execution is deterministic -- the same transaction sequence always produces the same state.

### Polay Market (Marketplace)

A native onchain marketplace for game assets. Sellers list assets at fixed prices, buyers purchase them atomically. The chain handles escrow, asset transfer, POL payment, and fee distribution in a single state transition. Supports studio royalties on secondary sales. No smart contracts or external DEX required.

### Polay Identity (Player Profiles + Achievements)

Onchain player identities with display names, avatar metadata, and achievement histories. Every address can register a profile that persists across games. Game servers record achievements via attested transactions. Enables cross-game reputation: a player's rank in one game can gate access to content in another.

### Polay Guard (Attestation + Anti-Cheat)

The match integrity layer. Game studios register their servers as attestors, which sign match results with their private keys. Validators verify these signatures, check anti-cheat confidence scores, and either settle rewards or quarantine suspicious results. This makes competitive integrity a protocol-level concern rather than an afterthought.

## TypeScript SDK

The TypeScript SDK (`@polay/sdk`) provides a high-level client for interacting with POLAY nodes.

```typescript
import { PolayClient, Keypair } from '@polay/sdk';

// Connect to a local node
const client = new PolayClient('http://localhost:9944');

// Generate or load a keypair
const keypair = Keypair.generate();
console.log('Address:', keypair.address);

// Query chain info
const info = await client.getChainInfo();
console.log('Chain:', info.chain_id, 'Height:', info.current_height);

// Check balance
const balance = await client.getBalance(keypair.address);
console.log('Balance:', balance, 'POL');

// Transfer POL
const txHash = await client.transfer(keypair, recipientAddress, 1_000_000n);
console.log('Transfer tx:', txHash);

// Create an asset class
const assetClassId = await client.createAssetClass(keypair, {
  name: 'Legendary Sword',
  maxSupply: 1000,
  metadata: { image: 'https://assets.game.io/sword.png', rarity: 'legendary' },
});

// Mint assets
await client.mintAsset(keypair, assetClassId, recipientAddress, 5);

// List on marketplace
await client.createListing(keypair, assetClassId, 2, 50_000n);

// Create player profile
await client.createProfile(keypair, {
  displayName: 'DragonSlayer99',
  metadata: { avatar: 'https://avatars.game.io/dragon.png' },
});
```

## Development

### Run All Tests

```bash
cargo test
```

### Run Tests for a Specific Crate

```bash
cargo test -p polay-types
cargo test -p polay-execution
cargo test -p polay-consensus
```

### Check All Crates (No Build)

```bash
cargo check
```

### Format and Lint

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

### Build Documentation

```bash
cargo doc --no-deps --open
```

## Documentation

Architecture documentation, protocol specifications, and guides are in the [docs/](docs/) directory:

- [Overview](docs/overview.md) -- What POLAY is, why it exists, design philosophy
- [Architecture](docs/architecture.md) -- Crate dependency graph and data flow
- [Consensus](docs/consensus.md) -- DPoS + BFT protocol specification
- [Execution](docs/execution.md) -- Transaction processing pipeline
- [State](docs/state.md) -- Storage key scheme and RocksDB layout
- [Staking](docs/staking.md) -- Validator economics and delegation model
- [Attestation](docs/attestation.md) -- Game server attestation and anti-cheat
- [RPC](docs/rpc.md) -- Complete JSON-RPC 2.0 specification
- [Local Devnet](docs/local-devnet.md) -- Setup guide for local development
- [Roadmap](docs/roadmap.md) -- Development phases from MVP to mainnet

## Roadmap

POLAY is currently in **Phase 0: MVP Local Devnet**.

| Phase | Focus | Status |
|---|---|---|
| Phase 0 | MVP local devnet -- core chain, all modules, RPC, documentation | In progress |
| Phase 1 | Public testnet -- full BFT consensus, slashing, indexer, explorer | Planned |
| Phase 2 | Smart contracts and governance -- WASM VM, onchain governance, bridge scaffold | Planned |
| Phase 3 | Mainnet launch -- security audit, tokenomics, genesis ceremony | Planned |
| Phase 4 | Gaming SDK ecosystem -- Rust/TS/Unity/Unreal SDKs, studio dashboard | Planned |
| Phase 5 | Advanced features -- rentals, auctions, bundles, guild treasuries, esports brackets | Planned |

See [docs/roadmap.md](docs/roadmap.md) for the full roadmap with deliverables and success criteria for each phase.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
