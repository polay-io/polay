# JSON-RPC API Reference

POLAY exposes a JSON-RPC 2.0 API over HTTP and WebSocket on port **9944** (default).

## Transport

```bash
# HTTP
curl -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"polay_health","params":{},"id":1}'

# WebSocket
wscat -c ws://localhost:9944
```

---

## Transaction Methods

### `polay_submitTransaction`

Submit a signed transaction to the mempool.

| Param | Type | Description |
|-------|------|-------------|
| `sender` | `string` | Hex-encoded sender address (64 chars) |
| `nonce` | `number` | Account nonce |
| `action` | `object` | Transaction action (see [Transaction Types](./transaction-types.md)) |
| `max_fee` | `string` | Maximum fee in base units |
| `signature` | `string` | Hex-encoded Ed25519 signature (128 chars) |

**Returns:** `string` — hex-encoded transaction hash.

### `polay_getTransaction`

Look up a transaction by hash.

| Param | Type | Description |
|-------|------|-------------|
| `tx_hash` | `string` | Hex-encoded transaction hash |

**Returns:** `TransactionWithStatus | null`

### `polay_getTransactionReceipt`

Fetch the execution receipt for a confirmed transaction.

| Param | Type | Description |
|-------|------|-------------|
| `tx_hash` | `string` | Hex-encoded transaction hash |

**Returns:** `{ tx_hash, block_height, success, gas_used, fee, fee_payer, events[] }`

---

## Block Methods

### `polay_getBlock`

| Param | Type | Description |
|-------|------|-------------|
| `height` | `number` | Block height |

**Returns:** `Block | null`

### `polay_getLatestBlock`

No params. **Returns:** `Block | null`

### `polay_getBlockReceipts`

| Param | Type | Description |
|-------|------|-------------|
| `height` | `number` | Block height |

**Returns:** `TransactionReceipt[]`

### `polay_getBlockEvents`

| Param | Type | Description |
|-------|------|-------------|
| `height` | `number` | Block height |

**Returns:** `Event[]`

---

## Account Methods

### `polay_getAccount`

| Param | Type | Description |
|-------|------|-------------|
| `address` | `string` | Hex-encoded address |

**Returns:** `{ address, nonce, balance, created_at } | null`

### `polay_getBalance`

| Param | Type | Description |
|-------|------|-------------|
| `address` | `string` | Hex-encoded address |

**Returns:** `number` — balance in base units.

---

## Asset Methods

### `polay_getAssetClass`

| Param | Type | Description |
|-------|------|-------------|
| `id` | `string` | Hex-encoded asset class ID |

**Returns:** `AssetClass | null`

### `polay_getAssetBalance`

| Param | Type | Description |
|-------|------|-------------|
| `asset_class_id` | `string` | Asset class ID |
| `owner` | `string` | Owner address |

**Returns:** `{ owner, asset_class_id, amount }`

---

## Marketplace, Identity, Attestation

### `polay_getListing`

| Param | Type |
|-------|------|
| `id` | `string` |

**Returns:** `Listing | null`

### `polay_getProfile`

| Param | Type |
|-------|------|
| `address` | `string` |

**Returns:** `PlayerProfile | null`

### `polay_getValidator`

| Param | Type |
|-------|------|
| `address` | `string` |

**Returns:** `ValidatorInfo | null`

### `polay_getActiveValidatorSet`

No params. **Returns:** `ValidatorInfo[]`

### `polay_getUnbondingEntries`

| Param | Type |
|-------|------|
| `address` | `string` |

**Returns:** `UnbondingEntry[]`

### `polay_getAttestor`

| Param | Type |
|-------|------|
| `address` | `string` |

**Returns:** `Attestor | null`

### `polay_getMatchResult`

| Param | Type |
|-------|------|
| `match_id` | `string` |

**Returns:** `MatchResult | null`

---

## Governance

### `polay_getProposal`

| Param | Type |
|-------|------|
| `id` | `string` |

**Returns:** `Proposal | null`

### `polay_getProposals`

No params. **Returns:** `Proposal[]`

---

## Session Keys

### `polay_getSession`

| Param | Type |
|-------|------|
| `granter` | `string` |
| `session_address` | `string` |

**Returns:** `SessionInfo | null`

### `polay_getActiveSessions`

| Param | Type |
|-------|------|
| `granter` | `string` |

**Returns:** `SessionInfo[]`

---

## Economics & Epoch

### `polay_getSupplyInfo`

No params. **Returns:** `{ total_supply, circulating, staked, burned, treasury, minted, fees_collected }`

### `polay_getInflationRate`

No params. **Returns:** `{ annual_rate_bps, epoch_reward }`

### `polay_getBlockReward`

No params. **Returns:** `number`

### `polay_getEpochInfo`

| Param | Type |
|-------|------|
| `epoch` | `number` |

**Returns:** `EpochInfo | null`

### `polay_getCurrentEpoch`

No params. **Returns:** `number`

### `polay_estimateGas`

| Param | Type |
|-------|------|
| `transaction` | `Transaction` |

**Returns:** `{ gas_cost, max_fee_suggestion }`

---

## Chain Metadata

### `polay_getChainInfo`

No params. **Returns:** `{ chain_id, height, latest_hash, block_time }`

### `polay_getMempoolSize`

No params. **Returns:** `number`

---

## Health & Monitoring

### `polay_health`

No params. **Returns:** `{ status: "healthy", height, syncing }`

### `polay_getNodeInfo`

No params. **Returns:** `{ chain_id, node_version, height, latest_hash, state_root, peer_count, mempool_size, uptime_seconds, block_time_ms }`

### `polay_getNetworkStats`

No params. **Returns:** `{ height, total_transactions, active_validators, total_staked, epoch, block_time_ms }`

---

## WebSocket Subscriptions

Connect via `ws://localhost:9944` and subscribe to real-time events.

### `polay_subscribeNewBlocks`

Receive a notification each time a new block is committed.

```json
{"jsonrpc":"2.0","method":"polay_subscribeNewBlocks","params":[],"id":1}
```

**Notifications:** `Block` objects on each new block.

**Unsubscribe:** `polay_unsubscribeNewBlocks`

### `polay_subscribeNewTransactions`

Receive notifications for new transactions entering the mempool.

**Unsubscribe:** `polay_unsubscribeNewTransactions`

### `polay_subscribeEvents`

Receive notifications for all on-chain events (transfers, staking, gaming, etc).

**Unsubscribe:** `polay_unsubscribeEvents`

---

## Error Codes

| Code | Meaning |
|------|---------|
| `-32600` | Invalid request |
| `-32601` | Method not found |
| `-32602` | Invalid params |
| `-32603` | Internal error |
| `-32000` | Transaction rejected (signature, nonce, balance) |
| `-32001` | Resource not found |
