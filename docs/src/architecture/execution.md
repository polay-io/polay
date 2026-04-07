# Execution Engine

The `polay-execution` crate is responsible for processing transactions within a finalized block. It supports parallel execution, gas metering for all 40 action types, fee distribution, and state invariant checking.

## Parallel Execution

POLAY uses [rayon](https://docs.rs/rayon) to execute independent transactions concurrently. The process works as follows:

1. **Access set prediction.** Before execution, each transaction's action type is analyzed to predict which state keys it will read and write (its "access set").
2. **Partition.** Transactions are grouped into conflict-free batches. Two transactions conflict if one's predicted write set intersects the other's read or write set.
3. **Parallel run.** Each batch is executed in parallel using rayon's thread pool. Every transaction gets its own `OverlayStore` clone.
4. **Conflict check.** After execution, the actual read/write sets are compared. If a misprediction caused a conflict, the conflicting transactions are re-executed sequentially.
5. **Merge.** The per-transaction overlays are merged into a single block overlay in transaction-index order, preserving determinism.

```
Block [tx0, tx1, tx2, tx3, tx4]
         |           |
  Batch A [tx0, tx2, tx4]    Batch B [tx1, tx3]
         |                        |
    parallel exec            parallel exec
         |                        |
         +--- merge in order -----+
                    |
              commit to RocksDB
```

## Gas Metering

Every action type has a fixed base gas cost. Gas prevents spam and funds the network through fees.

| Domain | Actions | Base Gas |
|---|---|---|
| Core | Transfer, CreateAccount | 1000 - 2000 |
| Assets | MintAsset, TransferAsset, BurnAsset | 2000 - 5000 |
| Marketplace | CreateListing, Purchase, CancelListing, CreateAuction, PlaceBid, SettleAuction | 3000 - 8000 |
| Identity | RegisterUsername, UpdateProfile | 5000 - 10000 |
| Staking | RegisterValidator, Delegate, Undelegate, ClaimRewards | 5000 - 10000 |
| Attestation | RegisterAttestor, SubmitAttestation, DistributeRewards | 5000 - 15000 |
| Governance | SubmitProposal, Vote | 5000 - 10000 |
| Sessions | CreateSessionKey, RevokeSessionKey | 3000 - 5000 |
| Rentals | ListForRent, Rent, ReturnRental, ClaimExpiredRental, CancelRentalListing | 3000 - 8000 |
| Guilds | CreateGuild, JoinGuild, LeaveGuild, DepositToTreasury, WithdrawFromTreasury, PromoteMember, KickMember | 3000 - 10000 |
| Tournaments | CreateTournament, JoinTournament, StartTournament, ReportResult, ClaimPrize, CancelTournament | 5000 - 15000 |

The total gas for a transaction is:

```
total_gas = base_gas(action_type) + per_byte_gas * tx_size_bytes
```

The fee in POL is:

```
fee = total_gas * gas_price
```

## Fee Distribution

Collected fees are split every block:

| Recipient | Share | Mechanism |
|---|---|---|
| Burn | 50% | Permanently removed from supply (deflationary pressure) |
| Treasury | 20% | Accumulated in the chain treasury for governance-directed spending |
| Block proposer | 30% | Direct reward to the validator who proposed the block |

This is enforced in `distribute_fees()` at the end of block execution.

## State Invariant Checker

After every block execution, a set of invariant checks run to detect bugs before state is committed:

- **Total supply conservation.** Sum of all balances + treasury + staked + unbonding must equal `total_supply - total_burned`.
- **Non-negative balances.** No account may have a negative balance.
- **Nonce monotonicity.** Every executed transaction must have `nonce == account.nonce + 1`.
- **Validator set consistency.** Active validator stakes must match their delegation totals.
- **Asset ownership.** Every asset must have exactly one owner.

If any invariant fails, the block is rejected and the overlay is discarded. In devnet mode, the node panics with a detailed diagnostic. In production, the node logs the failure and halts gracefully.

## Execution Pipeline Summary

```
Finalized Block
    |
    v
1. Predict access sets
2. Partition into conflict-free batches
3. Execute batches in parallel (rayon)
4. Check for conflicts, re-execute if needed
5. Merge overlays in deterministic order
6. Distribute fees (50% burn / 20% treasury / 30% proposer)
7. Run invariant checks
8. Commit overlay to RocksDB
9. Compute and store new Merkle state root
```
