# Tournaments

POLAY provides a native tournament system for competitive gaming. Tournaments handle entry fees, prize pools, result reporting, and prize distribution entirely on-chain.

## Tournament Lifecycle

```
Registration -> Active -> Completed
                  |
                  +-> Cancelled (refunds all entry fees)
```

## Creating a Tournament

```typescript
const tx = new TransactionBuilder()
  .action(Actions.createTournament({
    name: 'Weekly Arena Championship',
    gameId: 'battle-royale-v2',
    entryFee: 1_000n,       // 1000 microPOL per participant
    maxParticipants: 64,
    prizeDistribution: [50, 30, 20],  // percentages for 1st, 2nd, 3rd
  }))
  .nonce(await client.getNonce(organizer.address()))
  .gasPrice(1n)
  .chainId('polay-devnet-1')
  .sign(organizer);
```

The creator is the tournament **organizer** and has exclusive control over starting, reporting, and canceling.

### Prize Distribution

The `prizeDistribution` array defines how the prize pool is split. Values are percentages that must sum to 100:

| Position | Example Split |
|---|---|
| 1st place | 50% |
| 2nd place | 30% |
| 3rd place | 20% |

The prize pool is the sum of all entry fees collected.

## Joining a Tournament

```typescript
Actions.joinTournament({ tournamentId: 'tourney-001' })
```

- The entry fee is deducted from the joiner's balance and added to the prize pool
- Joining is only allowed while the tournament is in **Registration** status
- A participant cannot join the same tournament twice
- Joining fails if `maxParticipants` has been reached

## Starting a Tournament

```typescript
Actions.startTournament({ tournamentId: 'tourney-001' })
```

Only the organizer can start the tournament. This transitions the status from **Registration** to **Active**. No more participants can join after this point.

Requirements:
- At least 2 participants must have joined
- Tournament must be in Registration status

## Reporting Results

```typescript
Actions.reportResult({
  tournamentId: 'tourney-001',
  matchId: 'match-final',
  results: [
    { player: 'polay1winner...', score: 2500, placement: 1 },
    { player: 'polay1second...', score: 2100, placement: 2 },
    { player: 'polay1third...', score: 1800, placement: 3 },
  ],
})
```

Only the organizer can report results. In a full deployment, results would typically be submitted by an [attestor](./attestation.md) for trustless verification.

When the final result is reported, the tournament transitions to **Completed** status and prizes become claimable.

## Claiming Prizes

```typescript
Actions.claimPrize({ tournamentId: 'tourney-001' })
```

Each winner can claim their share of the prize pool based on the `prizeDistribution` and their final placement:

```
prize = prize_pool * prizeDistribution[placement - 1] / 100
```

Example with 64 participants at 1000 microPOL entry fee:

| Place | Share | Prize |
|---|---|---|
| 1st | 50% | 32,000 microPOL |
| 2nd | 30% | 19,200 microPOL |
| 3rd | 20% | 12,800 microPOL |

## Canceling a Tournament

```typescript
Actions.cancelTournament({ tournamentId: 'tourney-001' })
```

The organizer can cancel a tournament at any time before completion. When canceled:

- All entry fees are refunded to participants
- The tournament status becomes **Cancelled**
- No further actions are possible on this tournament

## Querying Tournament State

```bash
curl -s http://localhost:9944 -d '{
  "jsonrpc":"2.0","id":1,"method":"state_getTournament","params":["tourney-001"]
}'
```

Response:

```json
{
  "tournament_id": "tourney-001",
  "name": "Weekly Arena Championship",
  "game_id": "battle-royale-v2",
  "organizer": "polay1org...",
  "status": "Active",
  "entry_fee": 1000,
  "prize_pool": 64000,
  "max_participants": 64,
  "participants": ["polay1a...", "polay1b...", "..."],
  "prize_distribution": [50, 30, 20],
  "results": [],
  "created_at": 500,
  "started_at": 550
}
```

## Integration with Attestation

For trustless tournaments, combine with the [attestation system](./attestation.md). Registered attestors can verify match results using anti-cheat scoring and replay references before results are accepted on-chain.
