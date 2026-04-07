# Introduction

POLAY is a gaming-native Layer 1 blockchain built from scratch in Rust. It is designed to give game developers a purpose-built chain with fast finality, low fees, and first-class primitives for in-game economies.

## Key Properties

| Property | Value |
|---|---|
| Language | Rust (monorepo, 16 crates) |
| Consensus | DPoS + BFT (Tendermint-style) |
| Block time | Sub-2 second target |
| Transaction types | 40 specialized action types |
| Serialization | Borsh |
| State store | RocksDB with Merkle roots |
| Networking | libp2p (gossipsub, Noise, Yamux) |
| Token | POL |

## Why POLAY?

General-purpose blockchains force game developers to shoehorn gameplay mechanics into generic smart contract VMs. POLAY takes a different approach: every game-relevant operation -- asset transfers, marketplace listings, guild management, tournaments, session keys, attestation -- is a native transaction type processed by a purpose-built execution engine.

This means:

- **No smart contract overhead.** Game actions execute directly against native state.
- **Parallel execution.** Independent transactions run concurrently via rayon with conflict detection.
- **Gas metering per action type.** Each of the 40 transaction types has a tuned gas cost.
- **Session keys.** Players can delegate signing to ephemeral keys so gameplay is seamless and gasless from their perspective.
- **On-chain attestation.** Match results are submitted and verified by registered attestors with anti-cheat scoring.

## Architecture at a Glance

```
Client / SDK
    |
    v
polay-rpc  (JSON-RPC + WebSocket)
    |
    v
polay-mempool  (tx validation, priority queue)
    |
    v
polay-consensus  (BFT round: Propose -> Prevote -> Precommit -> Commit)
    |
    v
polay-execution  (parallel tx execution, gas metering)
    |
    v
polay-state  (RocksDB, overlay store, Merkle roots)
```

## Quick Start

```bash
# Build
cargo build --release

# Initialize a single-validator devnet
./target/release/polay init --chain devnet

# Run
./target/release/polay run

# Check node health
curl http://localhost:9944 -d '{"jsonrpc":"2.0","id":1,"method":"system_health","params":[]}'
```

## Documentation Structure

- **Architecture** -- internals of each subsystem (state, execution, consensus, networking)
- **Developer Guide** -- getting started, SDK usage, transaction reference
- **Staking & Economics** -- DPoS mechanics, tokenomics
- **Gaming Features** -- guilds, tournaments, rentals, session keys, attestation
- **Operations** -- running validators, monitoring, Docker/cloud deployment

Read on to explore each area in detail.
