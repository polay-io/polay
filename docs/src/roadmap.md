# Roadmap

POLAY development is organized into seven phases. Each phase builds on the previous one, progressing from local development tooling to a production mainnet.

## Phase 1: Local Devnet -- DONE

Foundational chain implementation running as a single-validator local node.

- [x] Core types and crypto primitives (`polay-types`, `polay-crypto`)
- [x] RocksDB state store with overlay (`polay-state`)
- [x] Transaction execution engine with gas metering (`polay-execution`)
- [x] BFT consensus engine (`polay-consensus`)
- [x] libp2p networking with gossipsub (`polay-network`)
- [x] JSON-RPC server (`polay-rpc`)
- [x] Genesis builder (`polay-genesis`)
- [x] All 40 transaction types implemented
- [x] Parallel execution with conflict detection
- [x] Fee distribution (50% burn / 20% treasury / 30% validator)
- [x] Single-validator devnet runs and produces blocks

## Phase 2: CI/CD and Tooling -- DONE

Development infrastructure and code quality tooling.

- [x] GitHub Actions CI pipeline (build, test, lint, fmt)
- [x] Comprehensive test suite across all crates
- [x] State invariant checker
- [x] Genesis configuration templates
- [x] `init-devnet.sh` script for local multi-validator setup
- [x] Docker build (multi-stage Dockerfile)
- [x] Code coverage reporting

## Phase 3: Multi-Node Testnet -- DONE

Multi-validator consensus and networking in a realistic environment.

- [x] 4-validator docker-compose testnet
- [x] BFT consensus with real multi-node voting
- [x] Peer discovery (mDNS + static boot nodes)
- [x] Peer scoring and rate limiting
- [x] Equivocation detection and slashing
- [x] Validator jailing and unjailing
- [x] Epoch transitions with validator set updates
- [x] State snapshot and sync protocol
- [x] Terraform deployment scripts for AWS

## Phase 4: SDK and Documentation -- DONE

Developer-facing tools and documentation.

- [x] TypeScript SDK (`@polay/sdk`)
  - [x] Transaction builder with all 40 action types
  - [x] Keypair management (generate, from seed)
  - [x] Transaction signing and submission
  - [x] State queries (accounts, assets, validators, etc.)
  - [x] WebSocket subscriptions
  - [x] Error handling with typed error codes
- [x] Prometheus metrics exporter
- [x] Grafana dashboard
- [x] mdBook documentation site
- [x] JSON-RPC API reference

## Phase 5: Public Testnet -- DONE

Open the testnet to external validators and developers.

- [x] Public boot nodes with stable DNS
- [x] Faucet web service with rate limiting
- [x] Block explorer frontend
- [x] Testnet validator onboarding guide
- [x] SDK published to npm registry
- [x] Developer tutorials (mint assets, run tournaments, session keys)
- [x] Bug bounty program
- [x] Load testing and performance benchmarks
- [x] Target: 1000+ TPS sustained throughput

## Phase 6: Security Audit -- DONE

Full audit of all security-critical components.

- [x] Consensus protocol audit
- [x] Cryptography review (Ed25519, SHA-256, Merkle trees)
- [x] State machine correctness verification
- [x] Networking layer DoS resilience
- [x] Staking and slashing logic audit
- [x] Fee distribution and inflation math
- [x] Session key permission enforcement
- [x] Remediation of all critical and high findings
- [x] Public audit report (`docs/SECURITY_AUDIT.md`)

## Phase 7: Mainnet -- DONE

Production launch of the POLAY network.

- [x] Genesis ceremony with initial validator set
- [x] Mainnet genesis file distribution
- [x] Validator coordination and launch sequence
- [x] Chain monitoring and incident response runbook
- [x] Governance module activation
- [x] Treasury funding and grant program
- [x] Game developer partnerships
- [x] SDK and tooling stable releases (v1.0)

---

## Timeline

| Phase | Status | Completed |
|---|---|---|
| Phase 1: Local Devnet | **DONE** | -- |
| Phase 2: CI/CD + Tooling | **DONE** | -- |
| Phase 3: Multi-Node Testnet | **DONE** | -- |
| Phase 4: SDK + Docs | **DONE** | -- |
| Phase 5: Public Testnet | **DONE** | -- |
| Phase 6: Security Audit | **DONE** | -- |
| Phase 7: Mainnet | **DONE** | Q2 2026 |

## Contributing

POLAY is open to contributions. See the repository's `CONTRIBUTING.md` for guidelines on submitting issues, feature requests, and pull requests.
