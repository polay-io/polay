# Asset Rentals

POLAY's rental system allows asset owners to earn income by renting out their game items, while renters get temporary access without purchasing outright. All rental terms are enforced on-chain.

## How It Works

1. An asset owner **lists** their asset for rent, specifying price, deposit, and duration bounds
2. A renter **rents** the asset by paying the rental fee + deposit
3. The renter uses the asset in-game for the agreed duration
4. The renter **returns** the asset (or it expires), and the deposit is refunded

## Listing an Asset for Rent

```typescript
const tx = new TransactionBuilder()
  .action(Actions.listForRent({
    assetId: 'legendary-sword-42',
    pricePerBlock: 10n,      // 10 microPOL per block
    deposit: 5_000n,         // refundable deposit
    minDuration: 100,        // minimum rental: 100 blocks
    maxDuration: 10_000,     // maximum rental: 10,000 blocks
  }))
  .nonce(await client.getNonce(owner.address()))
  .gasPrice(1n)
  .chainId('polay-devnet-1')
  .sign(owner);
```

When listed:

- The asset is locked and cannot be transferred, sold, or burned by the owner
- The listing is visible to all participants via RPC queries
- The owner can cancel the listing at any time (if not currently rented)

## Renting an Asset

```typescript
Actions.rent({
  listingId: 'rental-listing-001',
  duration: 500,  // rent for 500 blocks
})
```

The renter pays upfront:

```
total_cost = (pricePerBlock * duration) + deposit
```

For the example above: `(10 * 500) + 5000 = 10,000 microPOL`

After renting:

- The asset's `current_user` is set to the renter
- The renter can use the asset in-game (e.g., equip, use in battles)
- The renter **cannot** transfer, sell, or burn the asset
- The rental payment goes to the asset owner immediately
- The deposit is held in escrow

## Returning a Rental

```typescript
Actions.returnRental({ rentalId: 'rental-001' })
```

The renter can return the asset early. When returned:

- The asset goes back to the owner (or back to the rental listing)
- The deposit is refunded in full to the renter
- No refund is given for unused rental time

## Claiming Expired Rentals

```typescript
Actions.claimExpiredRental({ rentalId: 'rental-001' })
```

If the rental duration has passed and the renter hasn't returned the asset, the owner can claim it back:

- The asset is returned to the owner
- The deposit is sent to the owner (as compensation for the delay)

This incentivizes renters to return assets on time.

## Canceling a Listing

```typescript
Actions.cancelRentalListing({ listingId: 'rental-listing-001' })
```

The owner can cancel a rental listing only if the asset is not currently rented. The asset is unlocked and returned to normal status.

## Rental State

| Field | Description |
|---|---|
| `rental_id` | Unique identifier for the active rental |
| `listing_id` | The original listing this rental was created from |
| `asset_id` | The rented asset |
| `owner` | Asset owner address |
| `renter` | Current renter address |
| `price_per_block` | Rental rate |
| `deposit` | Held deposit amount |
| `start_block` | Block height when rental began |
| `end_block` | Block height when rental expires |
| `status` | `Active`, `Returned`, or `Expired` |

## Pricing Guidance

Since rental price is per-block and blocks are ~1.5 seconds:

| Desired Duration | Blocks (approx) |
|---|---|
| 1 hour | 2,400 |
| 1 day | 57,600 |
| 1 week | 403,200 |

Game developers can present these as human-friendly durations in their UIs.

## Design Rationale

- **Deposit incentive.** The deposit mechanism ensures renters return assets promptly. Owners are compensated if they don't.
- **No custody risk.** Assets never leave the chain's control. The protocol enforces that renters cannot transfer or destroy rented items.
- **Composable.** Rented assets can be used in tournaments, attested matches, and guild activities just like owned assets.
