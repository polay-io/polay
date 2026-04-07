# State Model

This document describes how POLAY stores, organizes, and commits chain state. The state layer is the foundation that all modules read from and write to.

## Overview

POLAY's state is a flat key-value store. Every piece of chain data -- account balances, asset classes, marketplace listings, player profiles, validator records -- is stored as a key-value pair in RocksDB. Keys are structured byte arrays with prefix-based namespacing. Values are Borsh-serialized Rust structs.

There is no account-level Merkle trie (as in Ethereum) in the current implementation. State commitment uses a hash over the serialized state snapshot. A full Merkle tree for per-key proofs is planned for Phase 2 (required for light clients and state proofs).

## Key Namespacing Scheme

Every key begins with a single **prefix byte** that identifies the state domain. This allows efficient range scans within a domain and clear separation between unrelated data.

```
Key format: [prefix_byte][domain_specific_key_bytes]
```

### Prefix allocation

| Prefix | Domain | Key structure | Value type |
|---|---|---|---|
| `0x01` | Accounts | `0x01 ++ address(32)` | `Account` |
| `0x02` | Asset Classes | `0x02 ++ asset_class_id(32)` | `AssetClass` |
| `0x03` | Asset Balances | `0x03 ++ address(32) ++ asset_class_id(32)` | `u64` (balance) |
| `0x04` | Listings | `0x04 ++ listing_id(32)` | `Listing` |
| `0x05` | Profiles | `0x05 ++ address(32)` | `Profile` |
| `0x06` | Achievements | `0x06 ++ address(32) ++ achievement_id(32)` | `Achievement` |
| `0x07` | Validators | `0x07 ++ address(32)` | `ValidatorInfo` |
| `0x08` | Delegations | `0x08 ++ delegator(32) ++ validator(32)` | `Delegation` |
| `0x09` | Attestors | `0x09 ++ address(32)` | `AttestorInfo` |
| `0x0A` | Match Results | `0x0A ++ match_id(32)` | `MatchResult` |
| `0x0B` | Chain Metadata | `0x0B ++ meta_key(variable)` | (varies) |
| `0x0C` | Unbonding Entries | `0x0C ++ delegator(32) ++ validator(32) ++ epoch(8)` | `UnbondingEntry` |

### Key derivation

Addresses are 32-byte Ed25519 public keys. Asset class IDs, listing IDs, match IDs, and achievement IDs are 32-byte BLAKE3 hashes derived from their creation parameters:

```
asset_class_id = blake3(creator_address ++ asset_class_name ++ nonce)
listing_id     = blake3(seller_address ++ asset_class_id ++ nonce)
match_id       = blake3(game_id ++ match_data ++ attestor_signature)
achievement_id = blake3(address ++ game_id ++ achievement_name)
```

Using hashes as IDs ensures uniqueness, avoids sequential ID enumeration attacks, and produces fixed-length keys suitable for RocksDB.

## State Domains

### Accounts (`0x01`)

Every address that has received POL or submitted a transaction has an account record.

```rust
pub struct Account {
    pub address: Address,       // 32-byte public key
    pub balance: u64,           // POL balance in base units
    pub nonce: u64,             // next expected transaction nonce
}
```

The account is the fundamental identity on the chain. Balances are modified by transfers, fee deductions, staking operations, and marketplace trades. The nonce is incremented with each transaction to prevent replay attacks.

### Asset Classes (`0x02`)

An asset class is a template for a game asset -- analogous to an ERC-1155 token type but without the smart contract overhead.

```rust
pub struct AssetClass {
    pub id: Hash,               // derived from creator + name + nonce
    pub name: String,           // human-readable name (e.g., "Legendary Sword")
    pub creator: Address,       // the studio/address that created this class
    pub total_supply: u64,      // current total supply across all holders
    pub max_supply: u64,        // maximum supply (0 = unlimited)
    pub metadata: String,       // JSON metadata (image URL, attributes, etc.)
}
```

The `creator` field is authoritative: only this address can mint new instances of the asset class. This gives game studios permanent control over their asset supply.

### Asset Balances (`0x03`)

Per-address, per-asset-class balance. The composite key `address ++ asset_class_id` allows efficient lookups ("how much of asset X does player Y own?") and range scans ("what assets does player Y own?" via prefix scan on `0x03 ++ address`).

The value is a raw `u64` -- the quantity of the asset class owned by the address. A balance of 0 means the entry can be pruned (the address does not own any of that asset).

### Listings (`0x04`)

Active marketplace listings.

```rust
pub struct Listing {
    pub id: Hash,
    pub seller: Address,
    pub asset_class_id: Hash,
    pub quantity: u64,
    pub price_per_unit: u64,    // price in POL base units
    pub is_active: bool,
    pub created_at: u64,        // block height when listed
}
```

When a listing is created, the assets are escrowed (deducted from the seller's asset balance). When purchased, assets go to the buyer and POL goes to the seller. When delisted, assets return to the seller. The `is_active` flag distinguishes live listings from completed/cancelled ones.

### Profiles (`0x05`)

Player identity records.

```rust
pub struct Profile {
    pub address: Address,
    pub display_name: String,   // max 32 characters
    pub metadata: String,       // JSON blob for avatar URL, bio, etc.
    pub created_at: u64,        // block height
    pub updated_at: u64,        // block height of last update
}
```

One profile per address. The display name provides a human-readable identifier for marketplaces, leaderboards, and game UIs. Metadata is an open JSON field for future extensibility.

### Achievements (`0x06`)

Achievement records linked to a player profile.

```rust
pub struct Achievement {
    pub id: Hash,
    pub address: Address,
    pub game_id: String,        // identifies which game
    pub achievement_name: String,
    pub data: String,           // JSON metadata (score, rank, etc.)
    pub recorded_at: u64,       // block height
}
```

Achievements are append-only. Once recorded, they cannot be modified or deleted. This creates an immutable achievement history for the player, useful for cross-game reputation and credential verification.

### Validators (`0x07`)

Validator registration and status.

```rust
pub struct ValidatorInfo {
    pub address: Address,
    pub public_key: PublicKey,
    pub self_stake: u64,
    pub total_stake: u64,       // self_stake + sum of delegations
    pub commission_rate: u64,   // basis points (100 = 1%)
    pub status: ValidatorStatus, // Active, Jailed, Tombstoned
    pub jailed_until: Option<u64>, // epoch when jail period ends
    pub missed_blocks: u64,
}
```

### Delegations (`0x08`)

Delegation records between delegators and validators.

```rust
pub struct Delegation {
    pub delegator: Address,
    pub validator: Address,
    pub amount: u64,            // delegated POL
    pub reward_debt: u64,       // for proportional reward calculation
}
```

The composite key `delegator ++ validator` allows queries in both directions: "what has this delegator staked?" (prefix scan on delegator) and "who delegates to this validator?" (full scan with filter, or future secondary index).

### Attestors (`0x09`)

Registered game server attestors.

```rust
pub struct AttestorInfo {
    pub address: Address,
    pub game_id: String,
    pub public_key: PublicKey,
    pub registered_by: Address, // the game studio that registered this attestor
    pub is_active: bool,
    pub registered_at: u64,
}
```

### Match Results (`0x0A`)

Settled match result records.

```rust
pub struct MatchResult {
    pub match_id: Hash,
    pub game_id: String,
    pub attestor: Address,
    pub players: Vec<Address>,
    pub result_data: String,    // JSON: scores, rankings, etc.
    pub rewards: Vec<(Address, u64)>,  // (recipient, POL amount) pairs
    pub anti_cheat_score: u64,  // 0-100, higher = more trustworthy
    pub is_quarantined: bool,
    pub submitted_at: u64,
}
```

### Chain Metadata (`0x0B`)

System-level state that does not belong to any specific entity.

| Meta key | Value | Purpose |
|---|---|---|
| `chain_id` | `String` | Network identifier (e.g., "polay-devnet-1") |
| `current_height` | `u64` | Latest committed block height |
| `current_epoch` | `u64` | Current epoch number |
| `latest_block_hash` | `Hash` | Hash of the most recently committed block |
| `total_supply` | `u64` | Total POL in existence |
| `active_validator_set` | `Vec<Address>` | Ordered list of active validators |

### Unbonding Entries (`0x0C`)

Pending undelegation records that are in the unbonding period.

```rust
pub struct UnbondingEntry {
    pub delegator: Address,
    pub validator: Address,
    pub amount: u64,
    pub unbond_epoch: u64,      // epoch when the unbonding was initiated
    pub completion_epoch: u64,  // epoch when tokens can be withdrawn
}
```

## Serialization

### Storage: Borsh

All values written to RocksDB are serialized with Borsh. Borsh guarantees:

- **Determinism:** The same struct always produces the same bytes. This is critical for state root computation.
- **Compactness:** No field names or type tags in the encoding. Smaller than JSON or Protobuf for fixed-schema data.
- **Speed:** Serialization and deserialization are fast with no schema resolution at runtime.

Keys are raw byte arrays (prefix + fixed-length components), not Borsh-encoded. This allows RocksDB range scans to work correctly on key prefixes.

### RPC: JSON

The JSON-RPC layer serializes state values to JSON for external clients. This uses `serde_json` with the same Rust structs (which derive both `BorshSerialize` and `serde::Serialize`).

The JSON representation is human-readable and easier to work with for game developers. Addresses and hashes are hex-encoded strings. Amounts are decimal strings to avoid JSON integer precision issues with large u64 values.

### Wire format: Borsh

Transactions and blocks propagated over the P2P network use Borsh encoding for compactness and speed. The RPC layer accepts JSON-encoded transactions for developer convenience and converts them to Borsh internally.

## State Commitment

### Current implementation

The state root in each block is computed as:

```
state_root = blake3(borsh_serialize(all_state_keys_and_values_sorted_by_key))
```

This is a full-state hash, not a Merkle tree. It is correct (validators agree on state) but does not support per-key proofs. Recomputing it requires reading all state, which is acceptable for small devnet state but will not scale.

### Planned: Sparse Merkle Tree

Phase 2 will replace the full-state hash with a **Sparse Merkle Tree (SMT)** or **Jellyfish Merkle Tree** (as used by Aptos/Diem). This provides:

- **Incremental updates:** Only modified keys update the tree, so state root computation is O(log n) per modification instead of O(n) total.
- **Inclusion proofs:** A Merkle path proves that a specific key-value pair is part of the committed state. Required for light clients.
- **Exclusion proofs:** A Merkle path proves that a key does not exist in the state. Required for proving that an account has no balance.

## State Pruning (Future)

As the chain grows, historical state accumulates in RocksDB. Pruning strategies planned:

- **Completed listing pruning:** Listings with `is_active: false` older than N epochs can be archived to the indexer and removed from node state.
- **Zero-balance asset pruning:** Asset balance entries of 0 can be deleted. The absence of a key is equivalent to a zero balance.
- **Match result archival:** Settled match results older than N epochs can be moved to the indexer. The match ID remains in a compact archive set for deduplication.
- **Snapshot-based pruning:** Nodes can take periodic state snapshots and discard historical blocks/state diffs older than the snapshot. New nodes sync from the latest snapshot rather than replaying from genesis.

## Migration and Versioning

State schema changes (adding fields to structs, changing key formats) require migration logic. The approach:

1. Each state schema version is tracked in chain metadata (`state_version` key).
2. When a node starts with a state version older than its binary expects, it runs migration functions that transform state entries from the old format to the new one.
3. Migrations are deterministic -- all nodes produce the same result from the same input state.
4. The migration is triggered at a specific block height coordinated via governance (future) or hard-coded in the binary for the devnet phase.

This is similar to database migration systems in web development, applied to blockchain state. The key constraint is that all validators must apply the migration at exactly the same block to maintain consensus.
