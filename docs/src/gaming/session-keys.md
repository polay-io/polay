# Session Keys

Session keys enable gasless, seamless gameplay by allowing players to delegate transaction signing to temporary keys with scoped permissions and spending limits.

## The Problem

Without session keys, every in-game action requires the player to:

1. Hold POL tokens for gas
2. Approve each transaction with their main wallet key
3. Wait for wallet confirmation popups

This creates terrible UX for real-time games.

## The Solution

Session keys are ephemeral Ed25519 keypairs that can sign transactions on behalf of a player's main account. The main account pays gas, and the session key has limited, revocable permissions.

## Creating a Session Key

```typescript
import { Keypair, TransactionBuilder, Actions } from '@polay/sdk';

// Generate an ephemeral key (typically done by the game client)
const sessionKey = Keypair.generate();

// The player signs a transaction with their main key to authorize the session
const tx = new TransactionBuilder()
  .action(Actions.createSessionKey({
    sessionPublicKey: sessionKey.publicKey(),
    permissions: ['Gaming'],        // scoped permissions
    expiresAt: Date.now() + 3_600_000,  // 1 hour from now
    spendingLimit: 100_000n,        // max microPOL this key can spend
  }))
  .nonce(await client.getNonce(player.address()))
  .gasPrice(1n)
  .chainId('polay-devnet-1')
  .sign(player);  // signed by the player's main wallet

await client.submitTransaction(tx);
```

## Permission Types

| Permission | Allowed Actions |
|---|---|
| `Transfer` | `Transfer` only, up to spending limit |
| `Gaming` | `TransferAsset`, `MintAsset`, `JoinTournament`, `JoinGuild`, `Rent`, `ReturnRental` |
| `All` | Every action type (use with caution) |

Permissions are enforced by the execution engine. If a session key attempts an action outside its permissions, the transaction is rejected.

## Using a Session Key

Once created, the game client uses the session key to sign transactions. The transaction includes the player's address as the `session_key` field, indicating it should be billed to the player:

```typescript
// Game client signs with the session key
const gameTx = new TransactionBuilder()
  .action(Actions.transferAsset({
    assetId: 'sword-001',
    to: 'polay1otherplayer...',
  }))
  .sessionKey(player.address())  // gas is charged to the player's account
  .nonce(await client.getSessionNonce(sessionKey.address()))
  .sign(sessionKey);  // signed by the session key, NOT the player's main key

await client.submitTransaction(gameTx);
```

The execution engine verifies:

1. The session key is registered for the given player
2. The session key has not expired
3. The action is within the session key's permissions
4. The cumulative spending has not exceeded the spending limit

## Spending Limits

The `spendingLimit` caps the total POL that can be spent (in fees + transfers) by this session key. Each transaction's gas cost and any POL transfers are deducted from the remaining limit.

```
remaining_limit = spending_limit - sum(gas_fees) - sum(transfers)
```

When the limit is reached, subsequent transactions are rejected until the player creates a new session key.

## Revoking a Session Key

```typescript
Actions.revokeSessionKey({
  sessionPublicKey: sessionKey.publicKey(),
})
```

Revocation is immediate. Any pending transactions signed by the revoked key will fail. Players should revoke session keys when:

- A gaming session ends
- A device is lost or compromised
- The spending limit needs to be changed (revoke + create new)

## Session Key State

Query active session keys for an account:

```bash
curl -s http://localhost:9944 -d '{
  "jsonrpc":"2.0","id":1,"method":"state_getSessionKeys","params":["polay1player..."]
}'
```

Response:

```json
[
  {
    "session_public_key": "ed25519:abc...",
    "permissions": ["Gaming"],
    "expires_at": 1700000000000,
    "spending_limit": 100000,
    "spent": 15000,
    "created_at": 500,
    "active": true
  }
]
```

## Security Considerations

- **Short expiry.** Set session keys to expire within hours, not days. If a game session lasts 2 hours, set a 2-hour expiry.
- **Minimal permissions.** Use `Gaming` instead of `All` unless the game specifically needs transfer or staking capabilities.
- **Low spending limits.** Set the limit to the expected gas cost for a gaming session, with some margin. This caps exposure if the key is compromised.
- **Revoke on logout.** When a player logs out of a game, immediately revoke the session key.
- **Rotate frequently.** Create a new session key for each gaming session rather than reusing long-lived keys.

## Game Developer Integration

Typical integration pattern:

1. Player connects wallet to game
2. Game generates a session key and asks the player to sign the `CreateSessionKey` transaction
3. Game stores the session key locally (browser localStorage, secure enclave on mobile)
4. During gameplay, the game signs transactions with the session key -- no wallet popups
5. On logout or session end, the game revokes the session key
