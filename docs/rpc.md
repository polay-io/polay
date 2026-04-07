# RPC Interface

POLAY exposes a JSON-RPC 2.0 interface for external clients to query chain state and submit transactions. The RPC server is built with `axum` and `jsonrpsee` and runs on each node (default port: 9944).

## Connection

```
HTTP:  http://localhost:9944
```

All requests use HTTP POST with `Content-Type: application/json`. The request body is a standard JSON-RPC 2.0 envelope:

```json
{
  "jsonrpc": "2.0",
  "method": "polay_methodName",
  "params": { ... },
  "id": 1
}
```

## Methods

### polay_submitTransaction

Submit a signed transaction for inclusion in a future block.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `sender` | `string` | Hex-encoded sender address (64 chars) |
| `nonce` | `number` | Sender's current nonce |
| `action` | `object` | Transaction action (see below) |
| `max_fee` | `string` | Maximum fee in base units (decimal string) |
| `signature` | `string` | Hex-encoded Ed25519 signature (128 chars) |

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "polay_submitTransaction",
  "params": {
    "sender": "a1b2c3d4...64hex",
    "nonce": 0,
    "action": {
      "type": "Transfer",
      "to": "e5f6a7b8...64hex",
      "amount": "1000000"
    },
    "max_fee": "100",
    "signature": "deadbeef...128hex"
  },
  "id": 1
}
```

**Response (success):**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "tx_hash": "abc123...64hex",
    "status": "accepted"
  },
  "id": 1
}
```

**Response (error):**

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32001,
    "message": "Invalid signature"
  },
  "id": 1
}
```

The `accepted` status means the transaction passed stateless validation and was added to the mempool. It does not guarantee inclusion in a block. The transaction may still fail during stateful validation or execution.

---

### polay_getBlock

Retrieve a block by height.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `height` | `number` | Block height |

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "polay_getBlock",
  "params": { "height": 42 },
  "id": 2
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "height": 42,
    "hash": "def456...64hex",
    "previous_hash": "789abc...64hex",
    "timestamp": 1700000042,
    "proposer": "a1b2c3d4...64hex",
    "state_root": "111222...64hex",
    "transaction_count": 5,
    "transactions": ["tx_hash_1", "tx_hash_2", "..."]
  },
  "id": 2
}
```

---

### polay_getBlockByHash

Retrieve a block by its hash.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `hash` | `string` | Hex-encoded block hash |

**Response:** Same format as `polay_getBlock`.

---

### polay_getLatestBlock

Retrieve the most recently committed block.

**Parameters:** None.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "polay_getLatestBlock",
  "params": {},
  "id": 3
}
```

**Response:** Same format as `polay_getBlock`.

---

### polay_getTransaction

Retrieve a transaction by hash.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `hash` | `string` | Hex-encoded transaction hash |

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "polay_getTransaction",
  "params": { "hash": "abc123...64hex" },
  "id": 4
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "hash": "abc123...64hex",
    "sender": "a1b2c3d4...64hex",
    "nonce": 0,
    "action": {
      "type": "Transfer",
      "to": "e5f6a7b8...64hex",
      "amount": "1000000"
    },
    "max_fee": "100",
    "fee_paid": "100",
    "status": "committed",
    "block_height": 42,
    "block_hash": "def456...64hex",
    "events": [
      {
        "module": "assets",
        "action": "transfer",
        "attributes": [
          ["from", "a1b2c3d4..."],
          ["to", "e5f6a7b8..."],
          ["amount", "1000000"]
        ]
      }
    ]
  },
  "id": 4
}
```

---

### polay_getBalance

Get the POL balance of an address.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `address` | `string` | Hex-encoded address |

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "polay_getBalance",
  "params": { "address": "a1b2c3d4...64hex" },
  "id": 5
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "address": "a1b2c3d4...64hex",
    "balance": "10000000000"
  },
  "id": 5
}
```

---

### polay_getAccount

Get the full account record for an address.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `address` | `string` | Hex-encoded address |

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "address": "a1b2c3d4...64hex",
    "balance": "10000000000",
    "nonce": 5
  },
  "id": 6
}
```

Returns `null` result if the account does not exist.

---

### polay_getAssetClass

Get an asset class by ID.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `asset_class_id` | `string` | Hex-encoded asset class ID |

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "id": "aaa111...64hex",
    "name": "Legendary Sword",
    "creator": "a1b2c3d4...64hex",
    "total_supply": 150,
    "max_supply": 1000,
    "metadata": "{\"image\": \"https://assets.game.io/sword.png\", \"rarity\": \"legendary\"}"
  },
  "id": 7
}
```

---

### polay_getAssetBalance

Get the balance of a specific asset class for an address.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `address` | `string` | Hex-encoded address |
| `asset_class_id` | `string` | Hex-encoded asset class ID |

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "address": "a1b2c3d4...64hex",
    "asset_class_id": "aaa111...64hex",
    "balance": 3
  },
  "id": 8
}
```

---

### polay_getListing

Get a marketplace listing by ID.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `listing_id` | `string` | Hex-encoded listing ID |

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "id": "bbb222...64hex",
    "seller": "a1b2c3d4...64hex",
    "asset_class_id": "aaa111...64hex",
    "quantity": 1,
    "price_per_unit": "50000",
    "is_active": true,
    "created_at": 100
  },
  "id": 9
}
```

---

### polay_getListings

Get marketplace listings with optional filters.

**Parameters:**

| Field | Type | Required | Description |
|---|---|---|---|
| `asset_class_id` | `string` | no | Filter by asset class |
| `seller` | `string` | no | Filter by seller address |
| `is_active` | `boolean` | no | Filter by active status (default: true) |
| `min_price` | `string` | no | Minimum price per unit |
| `max_price` | `string` | no | Maximum price per unit |
| `limit` | `number` | no | Max results (default: 50, max: 200) |
| `offset` | `number` | no | Pagination offset |

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "polay_getListings",
  "params": {
    "asset_class_id": "aaa111...64hex",
    "is_active": true,
    "limit": 10
  },
  "id": 10
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "listings": [
      {
        "id": "bbb222...64hex",
        "seller": "a1b2c3d4...64hex",
        "asset_class_id": "aaa111...64hex",
        "quantity": 1,
        "price_per_unit": "50000",
        "is_active": true,
        "created_at": 100
      }
    ],
    "total": 1,
    "limit": 10,
    "offset": 0
  },
  "id": 10
}
```

Note: Complex listing queries (price range, sorting) are best served by the PostgreSQL indexer. The RPC implementation does prefix scanning over RocksDB, which limits filtering capability.

---

### polay_getProfile

Get a player profile.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `address` | `string` | Hex-encoded address |

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "address": "a1b2c3d4...64hex",
    "display_name": "DragonSlayer99",
    "metadata": "{\"avatar\": \"https://avatars.game.io/123.png\"}",
    "created_at": 10,
    "updated_at": 50
  },
  "id": 11
}
```

---

### polay_getAchievements

Get achievements for a player.

**Parameters:**

| Field | Type | Required | Description |
|---|---|---|---|
| `address` | `string` | yes | Hex-encoded address |
| `game_id` | `string` | no | Filter by game |
| `limit` | `number` | no | Max results (default: 50) |
| `offset` | `number` | no | Pagination offset |

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "achievements": [
      {
        "id": "ccc333...64hex",
        "address": "a1b2c3d4...64hex",
        "game_id": "battle-royale-v1",
        "achievement_name": "First Blood",
        "data": "{\"matches_played\": 1}",
        "recorded_at": 42
      }
    ],
    "total": 1
  },
  "id": 12
}
```

---

### polay_getValidator

Get validator information.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `address` | `string` | Hex-encoded validator address |

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "address": "v1a2b3...64hex",
    "self_stake": "100000000000",
    "total_stake": "500000000000",
    "commission_rate": 1000,
    "status": "Active",
    "jailed_until": null,
    "missed_blocks": 0
  },
  "id": 13
}
```

---

### polay_getValidatorSet

Get the current active validator set.

**Parameters:** None.

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "epoch": 5,
    "validators": [
      {
        "address": "v1a2b3...64hex",
        "total_stake": "500000000000",
        "commission_rate": 1000,
        "status": "Active"
      },
      {
        "address": "v4d5e6...64hex",
        "total_stake": "300000000000",
        "commission_rate": 500,
        "status": "Active"
      }
    ],
    "total_stake": "800000000000",
    "validator_count": 2
  },
  "id": 14
}
```

---

### polay_getAttestor

Get attestor information.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `address` | `string` | Hex-encoded attestor address |

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "address": "att1...64hex",
    "game_id": "battle-royale-v1",
    "registered_by": "studio1...64hex",
    "is_active": true,
    "registered_at": 5
  },
  "id": 15
}
```

---

### polay_getMatchResult

Get a settled match result.

**Parameters:**

| Field | Type | Description |
|---|---|---|
| `match_id` | `string` | Hex-encoded match ID |

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "match_id": "mmm999...64hex",
    "game_id": "battle-royale-v1",
    "attestor": "att1...64hex",
    "players": ["p1...64hex", "p2...64hex", "p3...64hex"],
    "result_data": "{\"placement\": {\"p1...\": 1, \"p2...\": 2, \"p3...\": 3}}",
    "rewards": [
      ["p1...64hex", "10000"],
      ["p2...64hex", "5000"],
      ["p3...64hex", "2500"]
    ],
    "anti_cheat_score": 95,
    "is_quarantined": false,
    "submitted_at": 150
  },
  "id": 16
}
```

---

### polay_getChainInfo

Get current chain status and metadata.

**Parameters:** None.

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "chain_id": "polay-devnet-1",
    "current_height": 1542,
    "current_epoch": 15,
    "latest_block_hash": "fff000...64hex",
    "latest_block_timestamp": 1700001542,
    "total_supply": "1000000000000000",
    "active_validators": 4,
    "total_staked": "2000000000000"
  },
  "id": 17
}
```

## Error Codes

| Code | Message | Description |
|---|---|---|
| `-32700` | Parse error | Invalid JSON |
| `-32600` | Invalid request | Missing required JSON-RPC fields |
| `-32601` | Method not found | Unknown method name |
| `-32602` | Invalid params | Missing or malformed parameters |
| `-32603` | Internal error | Unexpected server error |
| `-32001` | Invalid signature | Transaction signature verification failed |
| `-32002` | Invalid nonce | Nonce does not match sender's current nonce |
| `-32003` | Insufficient balance | Sender cannot cover fees or transfer amount |
| `-32004` | Not found | Requested resource (block, tx, account, etc.) does not exist |
| `-32005` | Already exists | Duplicate resource (asset class name, attestor, profile) |
| `-32006` | Unauthorized | Sender lacks permission for this action |
| `-32007` | Quarantined | Match result was quarantined (anti-cheat) |
| `-32008` | Mempool full | Transaction rejected, mempool at capacity |
| `-32009` | Fee too low | max_fee is below the base fee for this transaction type |

## Rate Limiting

The MVP has no rate limiting. For testnet and mainnet:

- **Per-IP rate limit:** 100 requests/second for read methods, 10 requests/second for write methods.
- **Burst allowance:** 2x the rate limit for 5 seconds.
- **API keys:** Optional API keys for higher rate limits (for game studios and infrastructure partners).

## Future: WebSocket Subscriptions

A WebSocket endpoint will support event subscriptions:

```json
{
  "jsonrpc": "2.0",
  "method": "polay_subscribe",
  "params": {
    "event_type": "new_block"
  },
  "id": 1
}
```

Subscription types planned:
- `new_block` -- emitted when a block is committed
- `new_transaction` -- emitted when a transaction is committed (with optional filter by sender or module)
- `match_result` -- emitted when a match result is settled (with optional filter by game_id)
- `listing_update` -- emitted when a listing is created, purchased, or delisted

## Future: gRPC

A gRPC interface is designed for internal and high-performance client use. It will mirror the JSON-RPC methods with Protobuf request/response types and support server-side streaming for subscriptions.

gRPC is preferred for:
- Game server backends that need low-latency, high-throughput access.
- Indexer connections that process every block.
- Internal node-to-node communication (state sync, snapshot transfer).
