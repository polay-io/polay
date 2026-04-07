# Guilds

POLAY has a native guild system that lets players form on-chain organizations with shared treasuries, role-based permissions, and membership management.

## Creating a Guild

Any account can create a guild:

```typescript
const tx = new TransactionBuilder()
  .action(Actions.createGuild({
    name: 'Dragon Slayers',
    description: 'Elite PvE raiders',
  }))
  .nonce(await client.getNonce(sender.address()))
  .gasPrice(1n)
  .chainId('polay-devnet-1')
  .sign(sender);
```

The creator automatically becomes the guild **Leader**. A unique `guild_id` is assigned.

## Roles

Guilds have three roles with ascending permissions:

| Role | Permissions |
|---|---|
| **Member** | View guild info, deposit to treasury, leave |
| **Officer** | All Member permissions + withdraw from treasury, kick members |
| **Leader** | All Officer permissions + promote/demote members, dissolve guild |

There is exactly one Leader per guild. If the Leader leaves, the longest-tenured Officer is auto-promoted. If no Officers exist, the longest-tenured Member is promoted.

## Joining a Guild

```typescript
Actions.joinGuild({ guildId: 'guild-abc123' })
```

Joining is currently open -- any account can join any guild. Future governance proposals may add invite-only or application-based modes.

## Leaving a Guild

```typescript
Actions.leaveGuild({ guildId: 'guild-abc123' })
```

When a member leaves:

- They lose access to guild treasury operations
- Any pending treasury withdrawal requests are canceled
- Their role is removed from the membership list

## Treasury

Each guild has an on-chain treasury that holds POL tokens.

### Deposit

Any member can deposit POL:

```typescript
Actions.depositToTreasury({
  guildId: 'guild-abc123',
  amount: 10_000n,
})
```

### Withdraw

Officers and Leaders can withdraw from the treasury:

```typescript
Actions.withdrawFromTreasury({
  guildId: 'guild-abc123',
  amount: 5_000n,
  to: 'polay1recipient...',
})
```

All treasury transactions are fully on-chain and auditable.

## Promoting Members

Leaders can change any member's role:

```typescript
Actions.promoteMember({
  guildId: 'guild-abc123',
  member: 'polay1member...',
  newRole: 'Officer',  // 'Member' | 'Officer' | 'Leader'
})
```

Promoting someone to Leader transfers leadership -- the current Leader is demoted to Officer.

## Kicking Members

Officers and Leaders can remove members from the guild:

```typescript
Actions.kickMember({
  guildId: 'guild-abc123',
  member: 'polay1troublemaker...',
})
```

Kicked members can rejoin unless future governance adds ban lists.

## Guild State

Query guild details via RPC:

```bash
curl -s http://localhost:9944 -d '{
  "jsonrpc":"2.0","id":1,"method":"state_getGuild","params":["guild-abc123"]
}'
```

Response:

```json
{
  "guild_id": "guild-abc123",
  "name": "Dragon Slayers",
  "description": "Elite PvE raiders",
  "leader": "polay1creator...",
  "members": [
    { "address": "polay1creator...", "role": "Leader", "joined_at": 100 },
    { "address": "polay1member1...", "role": "Officer", "joined_at": 150 },
    { "address": "polay1member2...", "role": "Member", "joined_at": 200 }
  ],
  "treasury_balance": 50000,
  "created_at": 100
}
```

## Design Rationale

- **On-chain governance.** Guild roles and treasury are enforced by the protocol, not by game servers. This means guilds work across multiple games on POLAY.
- **No smart contracts needed.** Guild operations are native transaction types with fixed gas costs.
- **Composability.** Guilds can participate in tournaments as a unit (future feature), and guild treasuries can fund tournament entry fees.
