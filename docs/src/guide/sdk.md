# TypeScript SDK

The `@polay/sdk` package provides a TypeScript/JavaScript client for interacting with POLAY nodes. It supports transaction building, signing, submission, state queries, and real-time WebSocket subscriptions.

## Installation

```bash
npm install @polay/sdk
# or
yarn add @polay/sdk
# or
pnpm add @polay/sdk
```

## Quick Start

```typescript
import { PolayClient, Keypair, TransactionBuilder } from '@polay/sdk';

// Connect to a local devnet node
const client = new PolayClient('http://localhost:9944');

// Create a new keypair
const keypair = Keypair.generate();
console.log('Address:', keypair.address());

// Or restore from a seed phrase
const restored = Keypair.fromSeed('your 24 word mnemonic ...');
```

## Creating and Submitting Transactions

```typescript
import { PolayClient, Keypair, TransactionBuilder, Actions } from '@polay/sdk';

const client = new PolayClient('http://localhost:9944');
const sender = Keypair.fromSeed('...');

// Build a transfer transaction
const tx = new TransactionBuilder()
  .action(Actions.transfer({
    to: 'polay1recipient...',
    amount: 1_000_000n,  // 1 POL (6 decimals)
  }))
  .nonce(await client.getNonce(sender.address()))
  .gasPrice(1n)
  .chainId('polay-devnet-1')
  .sign(sender);

// Submit and wait for inclusion
const result = await client.submitTransaction(tx);
console.log('Included in block:', result.blockHeight);
console.log('Gas used:', result.gasUsed);
```

## Querying State

```typescript
// Account balance and nonce
const account = await client.getAccount('polay1abc...');
console.log('Balance:', account.balance);
console.log('Nonce:', account.nonce);

// Asset information
const asset = await client.getAsset('asset-id-123');
console.log('Owner:', asset.owner);
console.log('Metadata:', asset.metadata);

// Block by height
const block = await client.getBlock(42);
console.log('Transactions:', block.transactions.length);
console.log('State root:', block.header.stateRoot);

// Validator set
const validators = await client.getValidators();
for (const v of validators) {
  console.log(`${v.address}: ${v.stake} POL, ${v.status}`);
}
```

## WebSocket Subscriptions

```typescript
import { PolayClient } from '@polay/sdk';

const client = new PolayClient('http://localhost:9944', {
  wsUrl: 'ws://localhost:9945',
});

// Subscribe to new blocks
client.subscribeBlocks((block) => {
  console.log(`Block #${block.header.height}: ${block.transactions.length} txs`);
});

// Subscribe to transactions for a specific address
client.subscribeAddress('polay1abc...', (tx) => {
  console.log('New transaction:', tx.hash);
});

// Subscribe to specific event types
client.subscribeEvents(['Transfer', 'MintAsset'], (event) => {
  console.log(`${event.type}:`, event.data);
});

// Unsubscribe when done
client.unsubscribeAll();
```

## Gaming-Specific Operations

```typescript
// Mint a game asset
const mintTx = new TransactionBuilder()
  .action(Actions.mintAsset({
    assetId: 'sword-001',
    metadata: {
      name: 'Flame Sword',
      game: 'dragon-quest',
      attributes: { attack: 50, element: 'fire' },
    },
  }))
  .nonce(await client.getNonce(sender.address()))
  .gasPrice(1n)
  .chainId('polay-devnet-1')
  .sign(sender);

// Create a session key for gasless gameplay
const sessionKey = Keypair.generate();
const sessionTx = new TransactionBuilder()
  .action(Actions.createSessionKey({
    sessionPublicKey: sessionKey.publicKey(),
    permissions: ['Gaming'],
    expiresAt: Date.now() + 3600_000, // 1 hour
    spendingLimit: 100_000n,
  }))
  .nonce(await client.getNonce(sender.address()))
  .gasPrice(1n)
  .chainId('polay-devnet-1')
  .sign(sender);

// Use session key to sign gameplay transactions (no gas needed from player)
const gameTx = new TransactionBuilder()
  .action(Actions.transferAsset({
    assetId: 'sword-001',
    to: 'polay1otherplayer...',
  }))
  .sessionKey(sender.address()) // original account pays gas
  .nonce(await client.getSessionNonce(sessionKey.address()))
  .sign(sessionKey); // signed by session key
```

## Error Handling

```typescript
import { PolayError, ErrorCode } from '@polay/sdk';

try {
  const result = await client.submitTransaction(tx);
} catch (err) {
  if (err instanceof PolayError) {
    switch (err.code) {
      case ErrorCode.INSUFFICIENT_BALANCE:
        console.error('Not enough POL for this transaction');
        break;
      case ErrorCode.NONCE_MISMATCH:
        console.error('Nonce is stale, refetch and retry');
        break;
      case ErrorCode.GAS_LIMIT_EXCEEDED:
        console.error('Transaction exceeds gas limit');
        break;
      default:
        console.error('Transaction failed:', err.message);
    }
  }
}
```

## Configuration

```typescript
const client = new PolayClient('http://localhost:9944', {
  wsUrl: 'ws://localhost:9945',
  timeout: 30_000,        // request timeout in ms
  retries: 3,             // retry failed requests
  retryDelay: 1_000,      // ms between retries
});
```

## Further Reading

- [Transaction Types](./transaction-types.md) -- full reference for all 40 action types
- [JSON-RPC API](./rpc-api.md) -- raw RPC method reference
