# Transaction Types

POLAY has 40 native transaction types (actions) grouped by domain. Each action is a first-class operation processed directly by the execution engine -- there is no smart contract VM.

## Transaction Structure

Every transaction wraps a single action:

```rust
pub struct Transaction {
    pub sender: Address,
    pub nonce: u64,
    pub action: Action,
    pub gas_price: u64,
    pub chain_id: String,
    pub signature: Signature,
    pub session_key: Option<Address>,  // if using delegated signing
}
```

## Core

| # | Action | Description | Key Fields |
|---|---|---|---|
| 1 | `Transfer` | Send POL tokens to another account | `to`, `amount` |
| 2 | `CreateAccount` | Explicitly create a new account (optional; transfers auto-create) | `address`, `initial_balance` |

## Assets

| # | Action | Description | Key Fields |
|---|---|---|---|
| 3 | `MintAsset` | Create a new game asset | `asset_id`, `metadata` |
| 4 | `TransferAsset` | Transfer asset ownership | `asset_id`, `to` |
| 5 | `BurnAsset` | Permanently destroy an asset | `asset_id` |
| 6 | `UpdateAssetMetadata` | Update mutable metadata fields | `asset_id`, `metadata` |

## Marketplace

| # | Action | Description | Key Fields |
|---|---|---|---|
| 7 | `CreateListing` | List an asset for fixed-price sale | `asset_id`, `price` |
| 8 | `Purchase` | Buy a listed asset | `listing_id` |
| 9 | `CancelListing` | Remove a listing | `listing_id` |
| 10 | `CreateAuction` | List an asset for auction | `asset_id`, `min_bid`, `duration_blocks` |
| 11 | `PlaceBid` | Bid on an active auction | `auction_id`, `amount` |
| 12 | `SettleAuction` | Finalize an auction and transfer asset to winner | `auction_id` |

## Identity

| # | Action | Description | Key Fields |
|---|---|---|---|
| 13 | `RegisterUsername` | Claim a unique username | `username` |
| 14 | `UpdateProfile` | Set display name, avatar, bio | `display_name`, `avatar_url`, `bio` |

## Staking

| # | Action | Description | Key Fields |
|---|---|---|---|
| 15 | `RegisterValidator` | Register as a validator candidate | `pub_key`, `commission_rate` |
| 16 | `Delegate` | Delegate POL to a validator | `validator`, `amount` |
| 17 | `Undelegate` | Begin unbonding from a validator | `validator`, `amount` |
| 18 | `ClaimRewards` | Claim accumulated staking rewards | (none) |
| 19 | `UpdateCommission` | Change validator commission rate | `new_rate` |
| 20 | `Unjail` | Request unjailing after jail period | (none) |

## Attestation

| # | Action | Description | Key Fields |
|---|---|---|---|
| 21 | `RegisterAttestor` | Register as an attestor for a game | `game_id`, `stake` |
| 22 | `DeregisterAttestor` | Remove attestor registration | `game_id` |
| 23 | `SubmitAttestation` | Submit match results | `game_id`, `match_id`, `players`, `scores`, `winners`, `anti_cheat_score`, `replay_ref` |
| 24 | `DistributeRewards` | Distribute rewards based on attested results | `match_id`, `rewards` |

## Governance

| # | Action | Description | Key Fields |
|---|---|---|---|
| 25 | `SubmitProposal` | Submit a governance proposal | `title`, `description`, `proposal_type`, `params` |
| 26 | `Vote` | Vote on an active proposal | `proposal_id`, `vote` (Yes/No/Abstain) |
| 27 | `ExecuteProposal` | Execute a passed proposal | `proposal_id` |

## Session Keys

| # | Action | Description | Key Fields |
|---|---|---|---|
| 28 | `CreateSessionKey` | Create a delegated signing key | `session_public_key`, `permissions`, `expires_at`, `spending_limit` |
| 29 | `RevokeSessionKey` | Revoke a session key | `session_public_key` |

## Rentals

| # | Action | Description | Key Fields |
|---|---|---|---|
| 30 | `ListForRent` | List an asset for rental | `asset_id`, `price_per_block`, `deposit`, `min_duration`, `max_duration` |
| 31 | `Rent` | Rent a listed asset | `listing_id`, `duration` |
| 32 | `ReturnRental` | Return a rented asset early | `rental_id` |
| 33 | `ClaimExpiredRental` | Reclaim an asset after rental expires | `rental_id` |
| 34 | `CancelRentalListing` | Remove a rental listing | `listing_id` |

## Guilds

| # | Action | Description | Key Fields |
|---|---|---|---|
| 35 | `CreateGuild` | Create a new guild | `name`, `description` |
| 36 | `JoinGuild` | Request to join a guild | `guild_id` |
| 37 | `LeaveGuild` | Leave a guild | `guild_id` |
| 38 | `DepositToTreasury` | Deposit POL to guild treasury | `guild_id`, `amount` |
| 39 | `WithdrawFromTreasury` | Withdraw from guild treasury (Leader/Officer only) | `guild_id`, `amount`, `to` |
| 40 | `PromoteMember` | Change a member's role | `guild_id`, `member`, `new_role` |

## Tournaments

| # | Action | Description | Key Fields |
|---|---|---|---|
| 41 | `CreateTournament` | Create a tournament | `name`, `game_id`, `entry_fee`, `prize_distribution`, `max_participants` |
| 42 | `JoinTournament` | Enter a tournament (pays entry fee) | `tournament_id` |
| 43 | `StartTournament` | Transition to active (creator only) | `tournament_id` |
| 44 | `ReportResult` | Report match results within a tournament | `tournament_id`, `match_id`, `results` |
| 45 | `ClaimPrize` | Claim tournament winnings | `tournament_id` |
| 46 | `CancelTournament` | Cancel and refund entry fees | `tournament_id` |

> **Note:** The table above lists 46 rows, but several actions (41-46) map to the same domain slots as other actions internally. The protocol defines 40 unique action discriminants in the `Action` enum, with some tournament and guild operations sharing discriminant space via sub-actions.

## Gas Costs

Gas costs vary by action complexity. See the [Execution Engine](../architecture/execution.md) page for the full gas schedule.

## Using with the SDK

All action types are available as builder functions in the TypeScript SDK:

```typescript
import { Actions } from '@polay/sdk';

Actions.transfer({ to, amount });
Actions.mintAsset({ assetId, metadata });
Actions.createGuild({ name, description });
// ... etc
```

See the [TypeScript SDK](./sdk.md) page for full usage examples.
