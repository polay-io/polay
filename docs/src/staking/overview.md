# Staking

POLAY uses Delegated Proof of Stake (DPoS) to secure the network. Validators stake POL tokens, and token holders can delegate to validators to participate in consensus rewards without running infrastructure.

## Validator Registration

To become a validator, an account submits a `RegisterValidator` transaction:

```typescript
Actions.registerValidator({
  pubKey: validatorKeypair.publicKey(),
  commissionRate: 0.10,  // 10% commission on delegator rewards
})
```

Requirements:
- Minimum self-stake: **100,000 POL**
- Valid Ed25519 public key
- Unique validator address

After registration, the validator enters the **Candidate** status and is not yet part of the active set.

## Delegation

Any POL holder can delegate tokens to a validator:

```typescript
Actions.delegate({
  validator: 'polay1val...',
  amount: 50_000n,
})
```

Delegated tokens are locked and contribute to the validator's total stake. Delegators earn a proportional share of the validator's rewards (minus the validator's commission).

## Active Validator Set

At each **epoch transition** (every 100 blocks), the active validator set is recomputed:

1. All validators are ranked by total stake (self-stake + delegations)
2. The top N validators (configurable, default 100) become the active set
3. Validators outside the top N move to **Candidate** status
4. Stake weights are updated for consensus proposer selection

Only active validators participate in block production and consensus voting.

## Epoch Transitions

Epochs are 100 blocks long. At each epoch boundary:

| Step | Action |
|---|---|
| 1 | Calculate and distribute block rewards |
| 2 | Process completed unbonding entries |
| 3 | Recompute active validator set |
| 4 | Update proposer priorities |
| 5 | Apply any governance parameter changes |

## Unbonding

To withdraw staked or delegated tokens, submit an `Undelegate` transaction:

```typescript
Actions.undelegate({
  validator: 'polay1val...',
  amount: 25_000n,
})
```

Undelegated tokens enter an **unbonding period** of **100 blocks** (~150 seconds on devnet). During this time:

- Tokens do not earn rewards
- Tokens are still subject to slashing
- Tokens cannot be transferred

After the unbonding period completes (processed at the next epoch transition), tokens are returned to the delegator's available balance.

## Slashing

Validators are slashed for provable misbehavior:

| Offense | Penalty | Evidence |
|---|---|---|
| Double voting (equivocation) | **5% of total stake** burned | Two conflicting signed votes at the same height/round |
| Downtime (future) | **0.1% of total stake** | Missing N consecutive blocks |

Slashing affects both the validator's self-stake and all delegations proportionally. Slashed tokens are permanently burned.

## Jailing

After being slashed, a validator is **jailed** -- removed from the active set for **1800 blocks** (~45 minutes). During jail:

- The validator does not participate in consensus
- The validator does not earn rewards
- Delegators can undelegate but cannot add new delegations

After the jail period, the validator can submit an `Unjail` transaction to rejoin the candidate pool.

## Reward Distribution

Block rewards come from newly minted POL (inflation) and are distributed each epoch:

```
epoch_reward = (annual_inflation_rate / epochs_per_year) * total_staked

for each active validator:
    validator_share = epoch_reward * (validator_stake / total_active_stake)
    commission = validator_share * validator.commission_rate
    delegator_pool = validator_share - commission

    validator receives: commission + (delegator_pool * self_stake / total_stake)
    each delegator receives: delegator_pool * their_delegation / total_stake
```

Rewards accumulate and must be explicitly claimed via `ClaimRewards`.

## Staking Parameters

| Parameter | Value | Governance-changeable |
|---|---|---|
| Epoch length | 100 blocks | Yes |
| Unbonding period | 100 blocks | Yes |
| Slash fraction (equivocation) | 5% | Yes |
| Jail duration | 1800 blocks | Yes |
| Max validators | 100 | Yes |
| Min self-stake | 100,000 POL | Yes |

See [Tokenomics](./tokenomics.md) for inflation and fee economics.
