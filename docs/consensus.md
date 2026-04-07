# Consensus

POLAY uses a **Delegated Proof-of-Stake (DPoS) + Byzantine Fault Tolerant (BFT)** consensus protocol. This document describes the design, validator management, block lifecycle, finality properties, and slashing conditions.

## Overview

The consensus protocol achieves the following properties:

- **Safety:** No two conflicting blocks are finalized at the same height, as long as fewer than 1/3 of total stake is controlled by Byzantine validators.
- **Liveness:** The chain continues to produce blocks as long as more than 2/3 of total stake is online and honest.
- **Single-slot finality:** A block is final once committed. There are no forks, no uncle blocks, and no reorgs under the Byzantine threshold.
- **Deterministic proposer selection:** Every validator can independently compute who the block proposer is for any given height.

These properties make the protocol suitable for gaming, where asset trades and reward settlements must be final immediately and cannot be reversed by a chain reorganization.

## Validator Set

### Genesis validators

The initial validator set is defined in the genesis configuration. Each genesis validator entry includes:

- `address`: The validator's account address (derived from their public key)
- `public_key`: Ed25519 public key for block signing and consensus message verification
- `stake`: Initial self-stake in POL
- `commission_rate`: Percentage of delegator rewards taken as commission (e.g., 10%)

A minimum of 4 validators is required for BFT safety (tolerating 1 Byzantine fault with n >= 3f + 1).

### Staking-based validator set

After genesis, the validator set is determined by staking. Any account can register as a validator by submitting a `RegisterValidator` transaction with a minimum self-stake. Delegators can then stake POL to validators, increasing their total stake.

The **active validator set** consists of the top N validators by total stake (self-stake + delegated stake), where N is a chain parameter (default: 21 for MVP, configurable at genesis). This set is updated at epoch boundaries.

### Epoch transitions

An epoch is a fixed number of blocks (default: 100). At the end of each epoch:

1. The staking module computes the new validator set based on current stakes.
2. Validators that fell below the minimum stake or were jailed are removed.
3. New validators that meet the threshold are added.
4. The new validator set takes effect at the first block of the next epoch.

This batched update avoids the complexity of mid-epoch validator set changes and gives delegators a predictable window for staking decisions.

## Proposer Rotation

The block proposer for each height is selected by **deterministic round-robin** ordered by stake weight.

```
proposer_index = block_height % active_validator_count
proposer = active_validators_sorted_by_stake[proposer_index]
```

Validators are sorted by total stake in descending order, with ties broken by address (lexicographic). This ordering is deterministic and computable by every node.

Round-robin is simple and fair: every validator proposes blocks proportional to their position in the rotation. Validators with more stake do not propose more often -- stake weight affects voting power in the BFT rounds, not proposal frequency.

### Future: weighted proposer selection

A future upgrade may weight proposer selection by stake to give higher-stake validators more proposal slots. This aligns incentives (more stake = more responsibility = more rewards) but adds complexity to the rotation logic.

## Block Lifecycle

Each block goes through four phases:

### Phase 1: Propose

The designated proposer for the current height:

1. Selects transactions from the mempool (ordered by sender nonce, prioritized by fee).
2. Constructs a `Block` with:
   - `height`: Previous height + 1
   - `previous_hash`: Hash of the previous committed block
   - `timestamp`: Current Unix timestamp (must be >= previous block timestamp)
   - `proposer`: Proposer's address
   - `transactions`: Ordered list of transactions
   - `state_root`: Merkle root after executing all transactions (scaffold: hash of serialized state)
   - `signature`: Proposer's Ed25519 signature over the block header
3. Broadcasts the block proposal to all validators via the P2P network.

### Phase 2: Prevote

Each validator receives the block proposal and:

1. Verifies the proposer is correct for this height.
2. Verifies the proposer's signature.
3. Executes all transactions in the block against their local state.
4. Verifies the state root matches.
5. If valid: broadcasts a `Prevote(height, block_hash)` signed with their key.
6. If invalid: broadcasts a `Prevote(height, nil)` -- a nil vote indicating rejection.

### Phase 3: Precommit

Each validator collects prevotes. Once a validator has received prevotes for the same `block_hash` from validators representing **>2/3 of total stake**:

1. Broadcasts a `Precommit(height, block_hash)` signed with their key.

If a validator receives >2/3 nil prevotes, it broadcasts a `Precommit(height, nil)` and the round fails (timeout triggers a new round with the next proposer).

### Phase 4: Commit

Each validator collects precommits. Once a validator has received precommits for the same `block_hash` from validators representing **>2/3 of total stake**:

1. Writes the block's state diffs to RocksDB.
2. Emits events for the indexer.
3. Updates the chain head to the new block.
4. Removes committed transactions from the mempool.
5. Advances to the next height.

The set of precommit signatures constitutes the **commit proof** and is stored with the block for later verification by light clients or syncing nodes.

## Quorum Requirements

The BFT quorum threshold is **2/3+ of total active validator stake** (strictly greater than 2/3).

| Total validators | Byzantine tolerance | Minimum honest stake |
|---|---|---|
| 4 | 1 | 3 (75%) |
| 7 | 2 | 5 (71%) |
| 10 | 3 | 7 (70%) |
| 21 | 7 | 14 (67%) |

Quorum is measured by **stake weight**, not validator count. A single validator with 40% of stake has more voting power than 10 validators with 3% each.

## Timeout Handling

Liveness depends on timeouts to handle proposer failures and network delays.

| Timeout | Default | Purpose |
|---|---|---|
| `propose_timeout` | 3,000 ms | Time to wait for a block proposal from the designated proposer |
| `prevote_timeout` | 2,000 ms | Time to wait for prevotes after receiving a proposal |
| `precommit_timeout` | 2,000 ms | Time to wait for precommits after prevote quorum |

If the propose timeout fires without a valid proposal, validators prevote nil. If prevote or precommit timeouts fire without quorum, the round fails and a **new round** begins for the same height with an incremented round number.

The proposer for round R > 0 is:

```
proposer_index = (block_height + round) % active_validator_count
```

This ensures a different proposer is tried if the original one is offline or Byzantine.

### Timeout escalation

If multiple consecutive rounds fail at the same height, timeouts increase by 500ms per round up to a cap of 10,000ms. This handles transient network partitions without permanently degrading throughput.

## Finality

POLAY targets **single-slot finality**. Once a block is committed (2/3+ precommits collected), it is final. There is no fork choice rule, no longest-chain selection, and no confirmation depth.

This means:
- Asset trades are irreversible once committed.
- Match results are settled permanently.
- Reward distributions cannot be clawed back by a reorg.

The finality guarantee holds as long as fewer than 1/3 of total stake is Byzantine. If the Byzantine threshold is exceeded, the chain halts rather than producing conflicting finalized blocks (safety over liveness).

## Slashing

Slashing penalizes validators for behavior that threatens chain safety or liveness.

### Double signing

**Condition:** A validator signs two different blocks (or two different prevotes/precommits) at the same height and round.

**Evidence:** Two conflicting signed messages from the same validator at the same (height, round).

**Penalty:**
- 5% of the validator's total stake (self + delegated) is burned.
- The validator is **jailed** immediately and removed from the active set.
- A jailing period of 7 days (configurable) must pass before the validator can unjail.
- A maximum of 2 jailing events are allowed. A third offense results in **tombstoning** -- permanent removal from the validator set.

### Downtime

**Condition:** A validator misses more than 50 consecutive blocks (fails to prevote/precommit).

**Evidence:** The chain tracks a `missed_blocks_counter` per validator, reset when the validator participates.

**Penalty:**
- 0.1% of the validator's total stake is burned.
- The validator is jailed for 1 hour.
- The counter resets after unjailing.

### Evidence submission

Any node can submit slashing evidence via a `SubmitEvidence` transaction (future). In the MVP, evidence is detected automatically by validators observing the consensus messages.

Evidence includes:
- The two conflicting signed messages (for double signing)
- The block range during which the validator was absent (for downtime)
- The reporter's address (eligible for a reporter reward, planned)

## Validator Lifecycle

```
                      RegisterValidator
                            |
                            v
                      [ Registered ]
                            |
                       (enters active set at epoch boundary
                        if stake >= minimum and in top N)
                            |
                            v
                       [ Active ] <---------- Unjail (after cooldown)
                       /        \                    ^
                      /          \                   |
            (double sign)    (downtime)              |
                    /              \                  |
                   v                v                 |
              [ Jailed ] ---------> (cooldown expires)
                   |
              (3rd offense)
                   |
                   v
            [ Tombstoned ]  (permanent, cannot recover)
```

## Current Implementation

The MVP consensus implementation includes:

- Round-robin proposer selection by height
- Basic block validation (signature, height, previous hash, timestamp)
- Synchronous block execution during validation
- Single-round commit (simplified: propose and immediately commit for local devnet)
- Validator set from genesis configuration
- Epoch-based validator set updates (stake changes take effect at epoch boundaries)

The full BFT voting protocol (prevote/precommit rounds, timeout handling, evidence collection) is scaffolded in the crate structure but uses a simplified path for the devnet milestone.

## Future Work

### Parallel BFT
Pipelining block proposals so that block N+1 can begin consensus while block N is being committed. This increases throughput without changing the safety model.

### View changes
Formal view-change protocol for leader failure, replacing the current timeout-and-re-propose approach with a protocol that provides stronger liveness guarantees.

### Committee selection
For larger validator sets (>100), selecting a random committee per block rather than requiring all validators to participate. This reduces message complexity from O(n^2) to O(k^2) where k is the committee size.

### Light client proofs
Compact proofs (commit signatures + Merkle proofs) that allow light clients to verify state without running a full node. Essential for mobile wallets and game client integration.
