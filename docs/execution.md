# Execution Engine

The execution engine (`polay-execution`) processes transactions, dispatches them to module handlers, applies state transitions, and produces events. It is the core of POLAY's state machine.

## Design Principles

### Game-aware, not game-running

The execution engine understands gaming concepts (assets with classes, match results, player profiles) but never simulates gameplay. It processes the **economic consequences** of gameplay events that have already occurred offchain. This keeps execution fast, deterministic, and bounded in resource consumption.

### Deterministic execution

Every validator must produce the same state root from the same block. The execution engine guarantees determinism by:

- Using no floating-point arithmetic (all amounts are `u64` with fixed precision).
- Using no external I/O (no network calls, no filesystem reads, no randomness).
- Processing transactions in the exact order they appear in the block.
- Using Borsh serialization for all state reads and writes (deterministic byte representation).
- Sorting any internally generated collections by deterministic keys before iteration.

If any of these invariants were violated, validators would compute different state roots and consensus would fail.

### Module dispatch, not VM interpretation

Instead of interpreting bytecode in a virtual machine, the execution engine dispatches each transaction to a compiled Rust module handler based on the transaction's `action` field. This eliminates VM overhead (opcode dispatch, gas metering, memory management) and gives module authors the full power of Rust's type system and standard library.

## Transaction Processing Pipeline

Each transaction goes through five stages:

```
Raw bytes / JSON
      |
      v
 1. DECODE          Parse and deserialize the transaction
      |
      v
 2. STATELESS        Verify signature, check format, validate fields
    VALIDATE          (no state reads required)
      |
      v
 3. STATEFUL         Check nonce, verify balances, check permissions
    VALIDATE          (reads state but does not modify it)
      |
      v
 4. EXECUTE          Apply the state transition, modify balances/assets/etc.
                     (writes state)
      |
      v
 5. EMIT EVENTS      Produce structured events describing what changed
```

### Stage 1: Decode

The transaction is deserialized from Borsh bytes (from P2P) or JSON (from RPC). The decoder validates the structural format: all required fields are present, enum variants are valid, byte lengths are correct.

A malformed transaction is rejected at this stage with `DecodeError`. It never reaches the mempool.

### Stage 2: Stateless Validation

Checks that do not require reading chain state:

- **Signature verification:** The transaction's `signature` is verified against its `sender` public key and the signed payload (all fields except the signature itself).
- **Field validation:** Amounts are non-zero where required. String fields (display names, asset class names) are within length limits. Addresses are valid public key encodings.
- **Fee sanity:** The `max_fee` field is non-zero and within a sane upper bound.
- **Nonce format:** The nonce is a non-negative integer (actual ordering is checked statefully).

Stateless validation is performed when a transaction enters the mempool (via RPC or gossip) and again when a block is being validated. The first check prevents spam from entering the mempool; the second ensures consensus safety.

### Stage 3: Stateful Validation

Checks that require reading the current committed state:

- **Nonce ordering:** The transaction's nonce must equal the sender's current nonce in state. This prevents replay attacks and ensures transactions from the same sender execute in order.
- **Balance sufficiency:** The sender must have enough POL to cover the `max_fee`. For specific actions (transfers, listings, staking), additional balance checks are performed.
- **Permission checks:** Only the creator of an asset class can mint. Only the owner of a listing can delist. Only a registered attestor can submit match results. These are verified against state.
- **Existence checks:** The asset class being minted must exist. The listing being purchased must exist and be active. The validator being delegated to must be registered.

A transaction that fails stateful validation is dropped from the mempool (if it arrived via gossip) or rejected with an error (if it arrived via RPC).

### Stage 4: Execute

The transaction is dispatched to the appropriate module handler based on its `action` field:

```rust
match transaction.action {
    TransactionAction::Transfer { .. }            => assets_module.transfer(..),
    TransactionAction::CreateAssetClass { .. }    => assets_module.create_class(..),
    TransactionAction::MintAsset { .. }           => assets_module.mint(..),
    TransactionAction::BurnAsset { .. }           => assets_module.burn(..),
    TransactionAction::TransferAsset { .. }       => assets_module.transfer_asset(..),
    TransactionAction::ListAsset { .. }           => market_module.list(..),
    TransactionAction::BuyAsset { .. }            => market_module.buy(..),
    TransactionAction::DelistAsset { .. }         => market_module.delist(..),
    TransactionAction::RegisterProfile { .. }     => identity_module.register(..),
    TransactionAction::UpdateProfile { .. }       => identity_module.update(..),
    TransactionAction::RecordAchievement { .. }   => identity_module.achievement(..),
    TransactionAction::RegisterValidator { .. }   => staking_module.register(..),
    TransactionAction::Delegate { .. }            => staking_module.delegate(..),
    TransactionAction::Undelegate { .. }          => staking_module.undelegate(..),
    TransactionAction::ClaimRewards { .. }        => staking_module.claim(..),
    TransactionAction::RegisterAttestor { .. }    => attestation_module.register(..),
    TransactionAction::SubmitMatchResult { .. }   => attestation_module.submit(..),
}
```

Each module handler receives a mutable reference to the `StateStore` and the transaction data. It performs the state transition (modifying balances, creating entries, updating fields) and returns a `Vec<Event>` on success or an `ExecutionError` on failure.

If execution fails, the transaction's state changes are discarded. The fee is still deducted (the sender pays for the failed execution attempt). This prevents spam -- submitting invalid transactions is not free.

### Stage 5: Emit Events

Events produced by module handlers are collected into the block's event log. Each event is a structured record:

```rust
pub struct Event {
    pub block_height: u64,
    pub tx_index: u32,
    pub module: String,        // "assets", "market", "identity", etc.
    pub action: String,        // "transfer", "mint", "buy", etc.
    pub attributes: Vec<(String, String)>,  // key-value pairs
}
```

Events are not stored in the state tree. They are emitted alongside the committed block for consumption by indexers, explorers, and game backends. A game server polling for match settlement events would subscribe to events where `module = "attestation"` and `action = "match_settled"`.

## Fee Model

### Base fees

Each transaction type has a **base fee** in POL:

| Transaction type | Base fee (POL) |
|---|---|
| Transfer (POL) | 100 |
| CreateAssetClass | 10,000 |
| MintAsset | 500 |
| BurnAsset | 200 |
| TransferAsset | 300 |
| ListAsset | 500 |
| BuyAsset | 500 |
| DelistAsset | 200 |
| RegisterProfile | 1,000 |
| UpdateProfile | 500 |
| RecordAchievement | 300 |
| RegisterValidator | 50,000 |
| Delegate | 500 |
| Undelegate | 500 |
| ClaimRewards | 300 |
| RegisterAttestor | 10,000 |
| SubmitMatchResult | 1,000 |

Fees are denominated in the smallest unit of POL (1 POL = 10^8 base units, similar to satoshis). The values above are in base units.

### Max fee cap

Each transaction specifies a `max_fee` field. If the base fee for the transaction type exceeds `max_fee`, the transaction is rejected during stateful validation. This protects users from unexpected fee changes during chain upgrades.

### Fee distribution

Collected fees are distributed at the end of each block:

- **70%** to the block proposer's validator reward pool (shared with delegators based on commission).
- **30%** to a protocol treasury address (controlled by governance, future).

### Future: dynamic fees

The MVP uses static base fees. A future upgrade will introduce dynamic fee pricing based on block utilization, similar to EIP-1559. When blocks are full, base fees increase; when blocks are underutilized, fees decrease. This provides automatic congestion pricing for burst gaming workloads.

## Module Architecture

### Assets Module

Handles fungible and semi-fungible game asset management.

- **CreateAssetClass:** Creates a new asset template (e.g., "Legendary Sword") with a name, max supply, and creator address. The creator is the only address authorized to mint.
- **MintAsset:** Increases the balance of a specific asset class for a target address. Enforces max supply. Only the creator can mint.
- **TransferAsset:** Moves a quantity of an asset class from one address to another.
- **BurnAsset:** Destroys a quantity of an asset class from the sender's balance. Decreases total supply.

### Market Module

Native marketplace for asset trading.

- **ListAsset:** Creates a sell listing. The listed assets are escrowed (deducted from the seller's balance and held in the listing state). The listing specifies the asset class, quantity, and price in POL.
- **BuyAsset:** Executes a purchase. POL is transferred from buyer to seller, escrowed assets are transferred to buyer. Atomic -- if any step fails, the entire transaction reverts.
- **DelistAsset:** Cancels an active listing. Escrowed assets are returned to the seller. Only the original lister can delist.

### Identity Module

Onchain player profiles.

- **RegisterProfile:** Creates a profile with a display name. One profile per address.
- **UpdateProfile:** Modifies the display name or metadata fields.
- **RecordAchievement:** Adds an achievement record (game ID, achievement name, timestamp) to the profile. Typically submitted by a game server's attestor.

### Staking Module

Validator and delegation management. See [staking.md](./staking.md) for full details.

### Attestation Module

Game server match result attestation. See [attestation.md](./attestation.md) for full details.

## State Transition Function

At its core, the execution engine implements a single function:

```
execute_block(state, block) -> (new_state, events, error?)
```

This function:

1. Validates the block header (height, previous hash, proposer, signature).
2. Iterates over transactions in order.
3. For each transaction: validates, executes, collects events, deducts fees.
4. Computes the new state root.
5. Returns the updated state, accumulated events, and any errors.

If any transaction fails, its state changes are rolled back but the block continues processing remaining transactions. The failed transaction is included in the block (so the fee is paid) but its events include a `status: "failed"` attribute.

The state root is a commitment over the entire state after all transactions in the block have been processed. Validators compare this root during consensus to ensure they agree on the resulting state.

## Future: Parallel Execution

The current execution model is sequential: transactions are processed one at a time in block order. This is simple and correct but leaves performance on the table for blocks with independent transactions.

A future upgrade will introduce **parallel execution with conflict detection**:

1. Analyze the read/write sets of transactions (which state keys they access).
2. Group transactions with non-overlapping state access into parallel batches.
3. Execute batches concurrently.
4. Detect and re-execute conflicting transactions sequentially.

This is similar to the approach used by Aptos (Block-STM) and Sei. For gaming workloads where many transactions touch different player accounts, parallelism could increase throughput significantly without changing the programming model.
