# Consensus

POLAY uses a Tendermint-style BFT consensus protocol implemented in the `polay-consensus` crate. It provides single-slot finality: once a block is committed, it is final and will never be reverted.

## Round Lifecycle

Each block height goes through one or more rounds. A round proceeds through five phases:

```
NewRound -> Propose -> Prevote -> Precommit -> Commit
```

### 1. NewRound

A new round begins. The proposer for this round is selected deterministically based on validator stake weights (weighted round-robin). All validators reset their round state.

### 2. Propose

The selected proposer builds a block by pulling transactions from the mempool, then broadcasts a `Proposal` message containing the block to all validators via gossipsub.

If the proposer fails to send a proposal within the timeout, validators proceed to prevote nil.

### 3. Prevote

Each validator validates the proposed block:

- Verify the proposer is correct for this round
- Verify all transaction signatures
- Verify the parent hash matches the previous block
- Verify the block does not exceed size/tx limits

If valid, the validator broadcasts a `Prevote` for the block hash. If invalid or missing, the validator broadcasts a `Prevote` for nil.

### 4. Precommit

Once a validator sees prevotes for the same block hash from validators representing **2/3+ of total stake**, it broadcasts a `Precommit` for that hash.

If 2/3+ of stake prevoted nil, the validator precommits nil and the round advances.

### 5. Commit

Once a validator sees precommits for the same block hash from validators representing **2/3+ of total stake**, the block is committed:

1. Execute all transactions in the block
2. Apply state changes
3. Store the commit (set of precommit signatures)
4. Advance to the next height

## Quorum Requirement

The safety threshold is **2/3+ of total active stake**. This means:

- Up to 1/3 of stake can be offline (liveness)
- Up to 1/3 of stake can be Byzantine (safety)
- As long as 2/3+ is honest and online, the chain progresses

## Timeouts

Each phase has a configurable timeout that increases with each successive round to ensure liveness:

| Phase | Base Timeout | Increment per Round |
|---|---|---|
| Propose | 3000 ms | +500 ms |
| Prevote | 1000 ms | +500 ms |
| Precommit | 1000 ms | +500 ms |

```
timeout(phase, round) = base_timeout(phase) + round * increment(phase)
```

If a timeout expires, the validator moves to the next phase with a nil vote.

## Proposer Selection

The proposer for each round is chosen via weighted round-robin. Each validator has a `proposer_priority` that is incremented by its stake weight each round. The validator with the highest priority is selected and then penalized by the total stake weight:

```
for each validator:
    priority += stake_weight

proposer = max_priority(validators)
proposer.priority -= total_stake
```

This ensures that validators propose blocks proportional to their stake over time.

## Equivocation Evidence

If a validator signs two different prevotes or precommits for the same height and round, this is **equivocation** -- proof of Byzantine behavior. Any node that observes conflicting votes creates an `Evidence` object:

```rust
pub struct Evidence {
    pub evidence_type: EvidenceType,  // DuplicateVote
    pub validator: Address,
    pub height: u64,
    pub round: u32,
    pub vote_a: SignedVote,
    pub vote_b: SignedVote,
}
```

Evidence is included in subsequent blocks. When processed, the offending validator is:

1. **Slashed** -- 5% of their total stake is burned
2. **Jailed** -- removed from the active set for 1800 blocks

## Consensus Messages

All consensus messages are broadcast over the `consensus` gossipsub topic:

```rust
pub enum ConsensusMessage {
    Proposal(Proposal),
    Prevote(SignedVote),
    Precommit(SignedVote),
}
```

Messages are wrapped in a `MessageEnvelope` with protocol version and sender identity for forward compatibility.

## Finality

POLAY has **single-slot finality**. Once a block receives 2/3+ precommits and is committed, it is irreversible. There are no forks, no reorganizations, and no confirmation wait times. This is critical for gaming where asset transfers and match results must be immediately authoritative.
