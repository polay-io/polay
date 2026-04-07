# State Management

POLAY's state layer is implemented in the `polay-state` crate. It provides durable storage via RocksDB, an overlay mechanism for parallel execution, and Merkle state roots for integrity verification.

## Storage Backend: RocksDB

All on-chain state is persisted in a RocksDB instance. The store uses column families to partition data by domain:

| Column Family | Contents |
|---|---|
| `accounts` | Account balances, nonces |
| `assets` | Asset metadata, ownership |
| `staking` | Validators, delegations, unbonding |
| `market` | Listings, auctions, rentals |
| `identity` | Usernames, profiles |
| `attestation` | Attestors, match results |
| `guilds` | Guild state, membership, treasury |
| `tournaments` | Tournament state, entries, results |
| `sessions` | Session keys and permissions |
| `blocks` | Block headers and bodies |
| `state_roots` | Merkle roots per height |

## OverlayStore

The `OverlayStore` is a write-ahead layer that sits on top of the base RocksDB store. During block execution, all reads go through the overlay (falling through to disk on miss) and all writes are buffered in memory.

```rust
pub struct OverlayStore {
    base: Arc<RocksStore>,
    writes: HashMap<Vec<u8>, Option<Vec<u8>>>,  // None = deletion
    reads: HashSet<Vec<u8>>,                     // tracked for conflict detection
}
```

Key properties:

- **Isolation.** Each transaction executes against its own overlay clone. If two transactions touch disjoint keys, they can run in parallel without conflict.
- **Conflict detection.** After parallel execution, the engine checks whether any transaction's write set overlaps another's read or write set. Conflicting transactions are re-executed sequentially.
- **Atomic commit.** Once all transactions in a block have been executed and validated, the overlay is flushed to RocksDB in a single `WriteBatch`.
- **Rollback.** If execution fails (e.g., invariant violation), the overlay is discarded. The base store is unchanged.

## Serialization: Borsh

All state values are serialized using [Borsh](https://borsh.io/) (Binary Object Representation Serializer for Hashing). Borsh is deterministic, compact, and fast -- important properties for a blockchain where every node must arrive at identical state.

```rust
use borsh::{BorshSerialize, BorshDeserialize};

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Account {
    pub balance: u64,
    pub nonce: u64,
    pub created_at: u64,
}
```

## Merkle State Roots

After each block is committed, a Merkle root is computed over the entire state tree. This root is included in the block header, allowing light clients and peers to verify state integrity without downloading the full database.

The Merkle tree uses Blake3 hashing and is constructed over sorted key-value pairs within each column family. The per-family roots are then combined into a single state root:

```
state_root = blake3(
    accounts_root || assets_root || staking_root || market_root ||
    identity_root || attestation_root || guilds_root || tournaments_root ||
    sessions_root
)
```

## Snapshot and Sync

For new nodes joining the network, POLAY supports state snapshot sync:

1. **Snapshot creation.** Every `snapshot_interval` blocks (default: 1000), a validator creates a RocksDB checkpoint. The checkpoint is a consistent, read-only copy of the database.
2. **Snapshot advertisement.** The validator announces the snapshot height and hash via the p2p network.
3. **Snapshot download.** A syncing node requests chunks of the snapshot from peers, verifying each chunk against the Merkle proof.
4. **Catch-up.** After restoring the snapshot, the node replays blocks from the snapshot height to the current tip using normal block sync.

## Key Design Decisions

- **No Merkle Patricia Trie.** POLAY uses a flat key-value store with a separate Merkle tree computed at commit time. This is simpler and faster for the write patterns typical of gaming workloads.
- **Column families over prefixed keys.** RocksDB column families provide better isolation, independent compaction, and cleaner code than prefix-based namespacing.
- **Borsh over Protobuf.** Borsh's deterministic encoding eliminates the need for canonical serialization logic. It is also simpler to use in Rust with derive macros.
