# POLAY: Gaming-Native Layer 1 Blockchain

## What is POLAY?

POLAY is a sovereign, gaming-native Layer 1 blockchain purpose-built for the economics of interactive entertainment. It provides the settlement layer for game asset ownership, player rewards, marketplace trading, and competitive integrity -- while leaving gameplay simulation entirely offchain where it belongs.

The core principle is simple: **games run offchain, truth lives onchain.**

A first-person shooter does not need consensus to register a headshot. But it does need consensus to award the tournament prize, transfer the rare weapon skin, and record the match result that updates a player's ranking. POLAY draws this line deliberately. Gameplay stays fast, responsive, and server-authoritative. Economic outcomes -- who owns what, who earned what, who traded what -- settle onchain with finality.

The native token is **POL**, used for transaction fees, staking, validator rewards, and marketplace settlement.

## Why Gaming Needs Its Own L1

General-purpose blockchains were designed for financial transactions and smart contract execution. They impose constraints that are hostile to gaming workloads:

- **Asset model mismatch.** Gaming assets are not ERC-20 tokens or ERC-721 NFTs bolted onto a general ledger. They have classes with shared metadata (a "Legendary Sword" template), per-instance quantities (a player owns 3 of them), and studio-controlled supply. POLAY models this natively with an `AssetClass` and per-account balance system rather than forcing game studios to deploy and manage smart contracts.

- **Throughput patterns.** A game studio with 100,000 concurrent players might submit tens of thousands of match settlement transactions per minute in bursts after tournament rounds. General-purpose chains either cannot absorb this or require expensive L2 infrastructure. POLAY's module-based execution avoids the overhead of VM interpretation and contract storage lookups.

- **Identity is richer than an address.** Gamers have display names, achievement histories, reputation scores, and cross-game identities. POLAY's Identity module stores this onchain rather than requiring external identity services.

- **Anti-cheat is a first-class concern.** No other blockchain has a native attestation system for game servers to sign match results, with onchain verification, anti-cheat scoring, and quarantine flagging. POLAY Guard makes competitive integrity a protocol-level feature.

- **Marketplace is native.** Listing, buying, and delisting game assets should not require deploying a DEX contract. POLAY Market is a built-in module with order management, fee distribution, and studio royalty support.

## The POL Token

POL serves four functions in the network:

1. **Transaction fees.** Every transaction (transfer, listing, match settlement) pays a fee in POL.
2. **Staking.** Validators and delegators lock POL to secure the network and earn rewards.
3. **Marketplace settlement.** Asset purchases on Polay Market are denominated in POL.
4. **Reward distribution.** Match results can trigger onchain reward payouts in POL or game-specific assets.

## Core Modules

POLAY is organized around five subsystems:

### Polay Chain
The base layer: consensus, block production, transaction processing, state management, P2P networking, and RPC. This is the blockchain itself -- a DPoS + BFT chain with single-slot finality targeting sub-second block times.

### Polay Guard (Attestation)
The anti-cheat and match integrity layer. Game servers register as **attestors**, sign match results with their private keys, and submit them to the chain. Validators verify these signatures, check anti-cheat scores, and settle rewards. Suspicious results are quarantined rather than settled, protecting the economic layer from fraudulent gameplay claims.

### Polay Market
A native onchain marketplace for game assets. Studios and players can list assets for sale at fixed prices, and buyers purchase them atomically -- the chain handles escrow, transfer, and fee distribution in a single state transition. No smart contracts, no external DEX, no bridge to a marketplace chain.

### Polay Identity
Onchain player profiles with display names, linked game accounts, achievement records, and reputation metadata. This gives every address a human-readable identity that persists across games, enabling cross-game reputation and portable player history.

### Polay Engine (Execution)
The transaction processing engine that dispatches each transaction to the appropriate module handler. It enforces deterministic execution, manages the fee model, validates transactions both statelessly (signature, format) and statefully (balance checks, permission checks), and produces the state diff that gets committed after consensus.

## Target Users

| Audience | What POLAY offers |
|---|---|
| **Game studios** | Native asset issuance, match result settlement, anti-cheat attestation, marketplace with royalty support, no smart contract deployment required |
| **Marketplaces** | RPC access to onchain listings, standardized asset metadata, atomic trades with fee splitting |
| **Guilds** | Onchain identity for members, achievement tracking, future treasury management |
| **Esports platforms** | Verifiable match results, anti-cheat attestation, onchain prize distribution, player ranking data |
| **Players** | True ownership of game assets, cross-game identity, transparent marketplace, verifiable competitive results |

## Design Philosophy

POLAY makes several deliberate architectural choices:

1. **Modules over smart contracts (for now).** The MVP uses native Rust modules instead of a smart contract VM. This gives maximum performance and auditability for the core feature set. A WASM VM is planned for Phase 2 to enable third-party extensibility.

2. **Offchain gameplay, onchain economics.** The chain never simulates game logic. It settles economic outcomes that game servers attest to. This keeps the chain lean and avoids the impossibility of running real-time game engines in consensus.

3. **Single-slot finality.** Gaming transactions (asset trades, reward payouts) need fast finality. Players should not wait for multiple confirmations to know their purchase is settled.

4. **Minimal external dependencies.** The chain is self-contained. It does not depend on an L1 for security, does not require bridges for core functionality, and does not need oracles for its primary use cases (game servers are the oracles, via the attestation system).

5. **Indexer-friendly design.** Every state change emits structured events. The storage layer is designed for a PostgreSQL indexer to consume, enabling rich queries that the RPC layer alone cannot support.

## Repository Structure

POLAY is developed as a Rust monorepo with the following top-level crates:

```
polay/
  polay-types/       -- Shared types, traits, serialization
  polay-crypto/      -- Key generation, signing, hashing
  polay-consensus/   -- DPoS + BFT consensus engine
  polay-execution/   -- Transaction processing, module dispatch
  polay-state/       -- State storage, key scheme, RocksDB
  polay-network/     -- P2P networking, gossipsub
  polay-rpc/         -- JSON-RPC server
  polay-node/        -- Node binary, startup, orchestration
  polay-genesis/     -- Genesis block generation
  polay-cli/         -- Developer CLI tools
  docs/              -- This documentation
```

Each crate has a focused responsibility and explicit dependency boundaries. See [architecture.md](./architecture.md) for the full dependency graph and data flow.
