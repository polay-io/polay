# Attestation

The attestation system enables trustless verification of game results on-chain. Registered attestors submit match outcomes with anti-cheat scoring, and the protocol distributes rewards based on verified results.

## Overview

In traditional gaming, match results are reported by centralized game servers. POLAY's attestation system decentralizes this:

1. **Attestors** register for specific games by staking POL
2. Game servers or oracles submit match results through attestors
3. Results include anti-cheat scores and replay references for verification
4. Rewards are distributed based on attested outcomes

## Registering an Attestor

```typescript
const tx = new TransactionBuilder()
  .action(Actions.registerAttestor({
    gameId: 'battle-royale-v2',
    stake: 50_000n,  // stake POL as collateral
  }))
  .nonce(await client.getNonce(attestor.address()))
  .gasPrice(1n)
  .chainId('polay-devnet-1')
  .sign(attestor);
```

Requirements:

- Minimum attestor stake: 50,000 POL
- Each attestor is registered per game (can register for multiple games)
- Stake acts as collateral -- can be slashed for fraudulent attestations

## Deregistering

```typescript
Actions.deregisterAttestor({ gameId: 'battle-royale-v2' })
```

Stake is returned after an unbonding period (same as validator unbonding: 100 blocks). An attestor cannot deregister while they have pending unresolved attestations.

## Submitting an Attestation

```typescript
Actions.submitAttestation({
  gameId: 'battle-royale-v2',
  matchId: 'match-2024-001',
  players: [
    'polay1alice...',
    'polay1bob...',
    'polay1carol...',
  ],
  scores: [2500, 2100, 1800],
  winners: ['polay1alice...'],
  antiCheatScore: 98,       // 0-100, higher = more confident in legitimacy
  replayRef: 'ipfs://QmXyz...', // reference to replay data for disputes
})
```

### Attestation Fields

| Field | Type | Description |
|---|---|---|
| `gameId` | string | Identifier of the game |
| `matchId` | string | Unique match identifier |
| `players` | Address[] | All participants in the match |
| `scores` | u64[] | Score for each player (parallel array with players) |
| `winners` | Address[] | Winner(s) of the match |
| `antiCheatScore` | u8 | Confidence score (0-100) that the match was legitimate |
| `replayRef` | string | URI pointing to replay data (IPFS, Arweave, etc.) |

### Anti-Cheat Score

The `antiCheatScore` is a 0-100 value indicating how confident the attestor is that the match result is legitimate:

| Score Range | Meaning |
|---|---|
| 90-100 | High confidence, no anomalies detected |
| 70-89 | Normal, minor anomalies (may be lag or edge cases) |
| 50-69 | Suspicious, some indicators of manipulation |
| 0-49 | Likely fraudulent, strong evidence of cheating |

Results with an anti-cheat score below a configurable threshold (default: 50) are flagged and may not be eligible for reward distribution.

## Distributing Rewards

After a match is attested, rewards can be distributed to participants:

```typescript
Actions.distributeRewards({
  matchId: 'match-2024-001',
  rewards: [
    { player: 'polay1alice...', amount: 5000n },
    { player: 'polay1bob...', amount: 2000n },
    { player: 'polay1carol...', amount: 1000n },
  ],
})
```

Reward distribution requires:

- A valid attestation for the match
- The attestation's anti-cheat score is above the threshold
- The caller has authority to distribute (organizer or attestor)
- Sufficient funds in the source (tournament prize pool, game treasury, etc.)

## Querying Attestations

```bash
curl -s http://localhost:9944 -d '{
  "jsonrpc":"2.0","id":1,"method":"state_getAttestation","params":["match-2024-001"]
}'
```

Response:

```json
{
  "match_id": "match-2024-001",
  "game_id": "battle-royale-v2",
  "attestor": "polay1attestor...",
  "players": ["polay1alice...", "polay1bob...", "polay1carol..."],
  "scores": [2500, 2100, 1800],
  "winners": ["polay1alice..."],
  "anti_cheat_score": 98,
  "replay_ref": "ipfs://QmXyz...",
  "attested_at": 1200,
  "rewards_distributed": true
}
```

## Integration Pattern

A typical game integration looks like:

```
Game Server           Attestor Node             POLAY Chain
    |                      |                        |
    |-- match result ----->|                        |
    |                      |-- verify anti-cheat -->|
    |                      |-- submitAttestation -->|
    |                      |                        |-- validate & store
    |                      |                        |
    |                      |-- distributeRewards -->|
    |                      |                        |-- transfer prizes
```

The attestor node can be:

- A trusted game server with attestor keys
- An independent oracle service
- A decentralized network of game validators (future)

## Dispute Resolution

If a player believes an attestation is fraudulent:

1. The `replayRef` provides a link to match replay data
2. The community or a governance proposal can review the evidence
3. If fraud is confirmed, the attestor's stake can be slashed via governance

This mechanism incentivizes attestors to submit accurate results.
