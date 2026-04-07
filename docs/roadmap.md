# Roadmap

This document outlines the development roadmap for POLAY, organized into phases from the current MVP through mainnet launch and ecosystem growth.

## Phase 0: MVP Local Devnet (Current)

**Status:** In progress
**Goal:** Prove the core architecture works end-to-end on a local network.

### Deliverables

- **Monorepo structure.** All crates (`polay-types`, `polay-crypto`, `polay-consensus`, `polay-execution`, `polay-state`, `polay-network`, `polay-rpc`, `polay-node`, `polay-genesis`, `polay-cli`) are defined with clear boundaries and dependencies.

- **Core type system.** Transaction, Block, Account, AssetClass, Listing, Profile, Achievement, ValidatorInfo, Delegation, AttestorInfo, MatchResult types are defined with Borsh and serde serialization.

- **Cryptography.** Ed25519 key generation, signing, and verification. BLAKE3 hashing for block hashes, transaction hashes, and state roots.

- **State storage.** RocksDB-backed state store with prefix-based key namespacing. In-memory implementation for testing.

- **Execution engine.** Full transaction processing pipeline: decode, stateless validate, stateful validate, execute, emit events. All five module handlers (Assets, Market, Identity, Staking, Attestation) implemented.

- **Consensus.** Simplified DPoS consensus with round-robin proposer selection. Single-validator mode for local development. Multi-validator support with basic block validation.

- **P2P networking.** libp2p-based peer-to-peer networking with gossipsub for transaction and block propagation. Bootnode-based peer discovery.

- **JSON-RPC.** Full RPC interface with all query methods and transaction submission. See [rpc.md](./rpc.md) for the complete specification.

- **Genesis.** Genesis configuration generation with initial accounts, validators, and chain parameters. CLI tool for genesis management.

- **Local devnet.** Single-node and multi-node local deployment via Docker Compose and manual setup.

- **Documentation.** Architecture documentation for developers and contributors (this document set).

### What Phase 0 does NOT include

- Frontend applications (block explorer, wallet, marketplace UI).
- Token launch, tokenomics, or any mainnet economic design.
- Bridge to other chains.
- Zero-knowledge proofs or privacy features.
- Mobile client SDKs.
- Production-grade security audit.
- Smart contract VM.

These are explicitly out of scope for the MVP to keep focus on the core chain functionality.

## Phase 1: Public Testnet

**Target:** 3-4 months after Phase 0 completion
**Goal:** Run a public testnet with external validators, fix issues under real network conditions, and begin parallel execution research.

### Deliverables

- **Full BFT consensus.** Complete prevote/precommit round implementation with timeout handling, round escalation, and nil vote processing. The devnet's simplified consensus is replaced with the production BFT protocol.

- **Slashing implementation.** Double-signing detection with evidence submission. Downtime detection with missed-block tracking. Jailing, unjailing, and tombstoning.

- **Validator economics.** Commission rate enforcement, reward distribution with the cumulative-reward-ratio model, epoch-based validator set updates.

- **Mempool improvements.** Priority ordering by fee, per-sender nonce ordering, mempool size limits, transaction expiration (TTL).

- **RPC hardening.** Rate limiting, request validation, error handling improvements. Pagination for list endpoints. Health check endpoint.

- **PostgreSQL indexer.** External indexer service that consumes block events and populates a PostgreSQL database for rich queries (marketplace search, leaderboards, asset ownership history).

- **Block explorer.** Basic web-based block explorer for the testnet: block viewer, transaction viewer, account viewer, validator list.

- **Testnet faucet.** Web endpoint that distributes testnet POL to developers for testing.

- **Parallel execution research.** Prototype of parallel transaction execution with conflict detection. Benchmarking against sequential execution for gaming workload patterns.

- **Load testing.** Sustained throughput testing with simulated gaming workloads (asset minting bursts, marketplace activity, match settlement waves).

### Success criteria

- Testnet runs continuously for 30+ days with external validators.
- Sustained throughput of 1,000+ transactions per second under load testing.
- No consensus failures under normal conditions with 10+ validators.
- At least 3 external game studios run attestors on the testnet.

## Phase 2: Smart Contracts and Governance

**Target:** 3-4 months after Phase 1 completion
**Goal:** Enable third-party extensibility via WASM smart contracts, establish onchain governance, and begin cross-chain interoperability research.

### Deliverables

- **WASM smart contract VM.** WebAssembly-based virtual machine for user-deployed contracts. Contracts can interact with native modules (query asset balances, create listings) through a host function interface. Gas metering for resource accounting.

- **Contract SDK.** Rust SDK for writing POLAY smart contracts. Compile to WASM, deploy via transaction, interact via RPC. Tutorials and example contracts.

- **Onchain governance.** Proposal submission, voting by staked POL, parameter changes (epoch length, fee schedule, quarantine threshold). Timelock between vote passage and execution.

- **Bridge scaffold.** Design and prototype of a bridge protocol for asset transfers between POLAY and EVM chains (Ethereum, Polygon, Arbitrum). Light client verification or validator-attested bridge model.

- **State tree upgrade.** Replace the full-state hash with a Sparse Merkle Tree or Jellyfish Merkle Tree for incremental state root computation and Merkle proof generation. Required for light clients and bridge verification.

- **Parallel execution (production).** Ship the parallel execution engine based on Phase 1 research. Transactions with non-overlapping state access execute concurrently.

- **Enhanced attestation.** Multi-attestor quorum for match results. Attestor reputation scoring based on quarantine rate and dispute history.

### Success criteria

- At least 5 third-party smart contracts deployed on testnet.
- Governance processes a parameter change proposal end-to-end.
- Bridge prototype transfers assets between testnet and a Goerli/Sepolia testnet.
- Parallel execution shows 2x+ throughput improvement for representative workloads.

## Phase 3: Mainnet Launch

**Target:** 4-6 months after Phase 2 completion
**Goal:** Launch the production network with audited security, economic design, and validator onboarding.

### Deliverables

- **Security audit.** Comprehensive audit of consensus, execution, cryptography, and smart contract VM by a reputable security firm. All critical and high findings resolved.

- **Tokenomics design.** Final POL token economics: initial supply, inflation schedule, fee burn mechanism, treasury allocation, validator reward curve. Published economic whitepaper.

- **Genesis ceremony.** Coordinated genesis generation with mainnet validators. Multi-party key generation for the protocol treasury.

- **Validator onboarding program.** Documentation, tooling, and support for professional validators to join the network. Minimum hardware requirements published. Monitoring and alerting recommendations.

- **Mainnet launch.** Genesis block produced. Network begins processing transactions. Initial validator set of 21+ validators.

- **Token distribution.** Initial POL distribution to ecosystem participants, early contributors, and the protocol treasury per the tokenomics design.

- **Bridge (production).** Production bridge with mainnet EVM chains. Audited bridge contracts. Rate limits and circuit breakers for security.

- **Enhanced monitoring.** Prometheus metrics export from nodes. Grafana dashboards for validator operators. Alerting for missed blocks, peer count drops, and consensus stalls.

### Success criteria

- Zero critical vulnerabilities in audit report at launch.
- 21+ independent validators with geographic distribution.
- Network finalizes blocks within 2 seconds under normal load.
- Successful bridge transfer between POLAY mainnet and Ethereum mainnet.

## Phase 4: Gaming SDK Ecosystem

**Target:** Ongoing, beginning immediately after mainnet launch
**Goal:** Make it trivially easy for game studios to integrate with POLAY.

### Deliverables

- **Game SDK (Rust).** High-level Rust library for game servers: attestor registration, match result submission, asset management, reward distribution. Handles key management, transaction construction, and RPC communication.

- **Game SDK (TypeScript).** TypeScript/JavaScript SDK for web-based games and game backends running on Node.js. Same feature set as the Rust SDK.

- **Game SDK (Unity).** C# SDK for Unity game engine integration. Wallet connection, asset display, marketplace integration within game UIs.

- **Game SDK (Unreal).** C++ SDK for Unreal Engine integration. Similar scope to the Unity SDK.

- **Marketplace aggregation API.** A high-level API (backed by the PostgreSQL indexer) for marketplace queries: search by game, filter by rarity, sort by price, trending items, sales history. Powers marketplace frontends without game studios building their own query infrastructure.

- **Cross-game identity standard.** Specification for how games reference player profiles and achievements from other games on POLAY. A player's "Legendary Warrior" achievement in Game A can unlock a cosmetic in Game B by verifying the onchain achievement record.

- **Studio dashboard.** Web application for game studios to manage their POLAY integration: register/manage attestors, view match settlement history, monitor asset supply, track marketplace activity for their assets.

### Success criteria

- 10+ game studios actively using the SDK to integrate with POLAY.
- 100,000+ registered player profiles onchain.
- 1,000+ asset classes across multiple games.
- Active marketplace with daily trading volume.

## Phase 5: Advanced Features

**Target:** 6-12 months after mainnet launch
**Goal:** Build the features that make POLAY the definitive gaming blockchain.

### Deliverables

- **Asset rentals.** Time-limited asset access: a player rents a legendary weapon for 7 days. The asset is accessible to the renter but owned by the lender. Automatic return on expiration via an onchain timer.

- **Auctions.** English auctions (ascending bid) and Dutch auctions (descending price) for game assets. Onchain bid management with automatic settlement.

- **Asset bundles.** Atomic multi-asset listings: sell a "Starter Pack" containing a sword, shield, and potion as a single listing with a single price.

- **Guild treasuries.** Onchain multi-signature treasuries for gaming guilds. Guild members propose and vote on asset distributions, POL spending, and treasury management.

- **Esports brackets.** Onchain tournament bracket management: registration, seeding, match scheduling, result recording (via attestation), prize distribution. Transparent and verifiable competitive structures.

- **Achievement-gated access.** Smart contracts that gate access based on onchain achievements. A tournament requires players to have a "Top 100 Rank" achievement. A marketplace listing is only visible to players with a "Veteran" badge.

- **Dynamic metadata.** Asset classes with mutable metadata that changes based on onchain events. A sword that levels up as the player records more victories. Metadata updates are attested by the game server and recorded onchain.

- **Cross-chain asset portability.** Bridge-based or proof-based systems for representing POLAY assets on other chains (as NFTs, as tokens) while maintaining POLAY as the source of truth.

- **Privacy features.** Optional privacy for asset balances and marketplace activity using zero-knowledge proofs. A player can prove they own a rare item without revealing their full portfolio.

### Success criteria

- Rental, auction, and bundle features have active usage across multiple games.
- At least 3 guilds use onchain treasury management.
- At least 1 major esports tournament uses onchain bracket management.
- Cross-chain asset representation on at least 2 EVM chains.

## Non-Goals for MVP

The following are explicitly excluded from the MVP (Phase 0) to maintain focus:

| Non-goal | Rationale |
|---|---|
| **Frontend applications** | The MVP proves the chain works. Frontends come after the RPC and indexer are stable. |
| **Token launch / tokenomics** | Economic design requires real-world data from testnet usage patterns. Premature tokenomics leads to bad incentive design. |
| **Bridge to other chains** | Bridges are complex and security-critical. They require the chain to be stable and audited first. |
| **Zero-knowledge proofs** | ZK adds significant complexity. Privacy features are Phase 5, after core functionality is proven. |
| **Mobile SDKs** | Mobile integration depends on stable game SDKs (Phase 4). Mobile-specific concerns (key management, lightweight RPC) are addressed after the core SDK exists. |
| **Smart contract VM** | Native modules cover the core feature set. The VM is Phase 2 when third-party extensibility is needed. |
| **Production-grade security** | The MVP runs on a local devnet. Security hardening and auditing happen in Phases 1-3 as the chain approaches mainnet. |
| **Horizontal scalability (sharding)** | Single-shard throughput should be maximized first. Sharding is a future research direction if throughput becomes a bottleneck at mainnet scale. |

## Timeline Summary

```
Phase 0 (MVP Devnet)      |######|                          Current
Phase 1 (Testnet)                 |########|                 +3-4 months
Phase 2 (Contracts/Gov)                    |########|        +3-4 months
Phase 3 (Mainnet)                                   |##########|  +4-6 months
Phase 4 (SDK Ecosystem)                                        |############...
Phase 5 (Advanced)                                                   |############...
```

Phases 4 and 5 are ongoing workstreams that begin after mainnet and continue indefinitely as the ecosystem grows. Specific deliverables within these phases will be prioritized based on game studio demand and community governance.
