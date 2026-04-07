# Attestation (Polay Guard)

Polay Guard is the attestation and anti-cheat system that bridges offchain gameplay to onchain economic settlement. It allows game servers to sign match results, submit them to the chain, and trigger automated reward distribution -- while providing a framework for detecting and quarantining fraudulent results.

## The Problem

Blockchain-based gaming faces a fundamental trust gap: the game runs on a centralized server, but rewards settle on a decentralized chain. How does the chain know the match result is legitimate?

Without an attestation system, a game server could claim any arbitrary result -- awarding prizes to colluding accounts, fabricating tournament outcomes, or inflating player statistics. The blockchain would have no way to distinguish a legitimate result from a fraudulent one.

Polay Guard addresses this with a structured trust model:

1. **Game studios register attestors** -- the servers authorized to submit results for their game.
2. **Attestors sign match results** with their private keys, creating a cryptographic link between the game server and the onchain record.
3. **Validators verify signatures** before settling rewards, ensuring only authorized servers can trigger economic outcomes.
4. **Anti-cheat scores** provide a signal for result trustworthiness, enabling quarantine of suspicious results.

This does not solve all cheating problems. A compromised game server can still submit false results. But it provides accountability (attestor identity is onchain), detection mechanisms (anti-cheat scoring), and a framework for escalation (quarantine and dispute resolution).

## Attestor Registration

### Who can register an attestor?

Any address can submit a `RegisterAttestor` transaction. In practice, this will be the game studio's management address. The transaction specifies:

```rust
RegisterAttestor {
    game_id: String,          // identifier for the game (e.g., "battle-royale-v1")
    attestor_public_key: PublicKey,  // the key the game server will use to sign results
}
```

The `sender` of the transaction becomes the `registered_by` address in the attestor record. This creates an onchain ownership chain: studio address -> attestor -> match results.

### Attestor state

```rust
pub struct AttestorInfo {
    pub address: Address,           // derived from attestor_public_key
    pub game_id: String,
    pub public_key: PublicKey,
    pub registered_by: Address,     // the studio that registered this attestor
    pub is_active: bool,            // can be deactivated by the registering studio
    pub registered_at: u64,         // block height of registration
}
```

### Deactivation

A studio can deactivate an attestor by submitting an `UpdateAttestor` transaction (future). Deactivated attestors cannot submit match results. This allows studios to rotate server keys, retire game servers, or revoke compromised keys.

### Fees

Registering an attestor costs 10,000 base units of POL. This is intentionally higher than most transaction fees to discourage spam registration and ensure only serious game operators register attestors.

## Match Result Structure

When a game match concludes, the game server constructs a match result and signs it:

```rust
pub struct MatchResult {
    pub match_id: Hash,             // unique identifier (derived from game_id + match data)
    pub game_id: String,            // must match the attestor's registered game_id
    pub attestor: Address,          // the attestor submitting this result
    pub players: Vec<Address>,      // participating player addresses
    pub result_data: String,        // JSON-encoded match outcome
    pub rewards: Vec<(Address, u64)>, // reward recipients and amounts in POL
    pub anti_cheat_score: u64,      // 0-100, server's confidence in result integrity
    pub attestor_signature: Signature, // attestor's signature over the result payload
    pub is_quarantined: bool,       // set by the chain if score is below threshold
    pub submitted_at: u64,          // block height when committed
}
```

### result_data format

The `result_data` field is a JSON string containing game-specific match outcomes. The chain does not interpret this data -- it stores it as an opaque blob. Game clients and indexers parse it according to the game's schema.

Example for a battle royale game:

```json
{
  "match_type": "ranked",
  "duration_seconds": 1200,
  "map": "desert_storm",
  "placement": {
    "0xabc...": 1,
    "0xdef...": 2,
    "0x123...": 3
  },
  "kills": {
    "0xabc...": 12,
    "0xdef...": 8,
    "0x123...": 5
  }
}
```

### Rewards

The `rewards` field specifies POL amounts to distribute to players. The total reward amount must be available in the attestor's account (or a designated reward pool address). Rewards are transferred atomically during match result settlement.

Reward amounts are determined by the game server based on its own rules (placement prizes, kill bonuses, participation rewards). The chain enforces only that the funds are available and the transfers are valid.

## Verification Pipeline

When a `SubmitMatchResult` transaction is executed, the attestation module runs this verification pipeline:

### Step 1: Attestor verification

- Is the `attestor` address a registered attestor? (Check `0x09 ++ attestor` key exists in state.)
- Is the attestor active? (`is_active == true`)
- Does the `game_id` in the match result match the attestor's registered `game_id`?

If any check fails: **reject** with `AttestorNotFound`, `AttestorInactive`, or `GameIdMismatch`.

### Step 2: Signature verification

- Reconstruct the signed payload: all fields of the match result except `attestor_signature`, `is_quarantined`, and `submitted_at`.
- Verify the `attestor_signature` against the attestor's registered `public_key` and the payload.

If verification fails: **reject** with `InvalidAttestorSignature`.

### Step 3: Duplicate check

- Check if a match result with the same `match_id` already exists in state.

If duplicate: **reject** with `DuplicateMatchResult`.

### Step 4: Anti-cheat evaluation

- Read the `anti_cheat_score` from the match result.
- Compare against the **quarantine threshold** (chain parameter, default: 30).
- If `anti_cheat_score < quarantine_threshold`: set `is_quarantined = true`.

Quarantined results are stored onchain but rewards are **not distributed**. The match result is flagged for manual review.

### Step 5: Reward settlement

If the result is not quarantined:

- For each `(recipient, amount)` in `rewards`:
  - Deduct `amount` from the attestor's account (or reward pool).
  - Credit `amount` to the recipient's account.
  - Emit a `RewardDistributed` event.

If the attestor's account has insufficient funds for the total rewards: **reject** with `InsufficientRewardFunds`. No partial distribution occurs.

### Step 6: State commit

- Store the `MatchResult` at key `0x0A ++ match_id`.
- Emit a `MatchResultSubmitted` event with the match_id, game_id, player count, total rewards, and quarantine status.

## Anti-Cheat Integration

### What is the anti_cheat_score?

The `anti_cheat_score` is a value from 0 to 100 that represents the game server's confidence in the integrity of the match result. The game server computes this score based on its own anti-cheat systems:

| Score range | Interpretation |
|---|---|
| 80-100 | High confidence. Normal match, no anomalies detected. |
| 50-79 | Medium confidence. Minor anomalies (unusual performance spikes, borderline metrics). |
| 30-49 | Low confidence. Significant anomalies but not conclusive cheating. |
| 0-29 | Very low confidence. Strong indicators of cheating, exploits, or data manipulation. |

### Quarantine threshold

The chain parameter `quarantine_threshold` (default: 30) determines the cutoff. Results with `anti_cheat_score < quarantine_threshold` are quarantined.

The threshold is set conservatively to avoid false positives. Legitimate matches with unusual outcomes (a new player performing exceptionally well) should still clear the threshold. Only results with strong cheating indicators are quarantined.

### What happens to quarantined results?

Quarantined match results:

1. Are stored onchain with `is_quarantined: true`.
2. Do **not** trigger reward distribution.
3. Are visible via the RPC (`polay_getMatchResult` includes quarantine status).
4. Emit a `MatchQuarantined` event for the indexer.

Resolution of quarantined results is a future feature. Options under consideration:

- **Studio override:** The studio address that registered the attestor can submit a `ResolveQuarantine` transaction to either release rewards or permanently reject the result.
- **Governance vote:** For high-stakes results (e.g., tournament finals), a governance proposal could resolve the quarantine.
- **Expiration:** Quarantined results that are not resolved within N epochs are permanently rejected and the match is voided.

## Trust Model

The attestation system operates under a layered trust model:

```
Layer 1: Game Studio
  |  Registers attestors, responsible for server integrity
  |  Trust basis: business reputation, legal agreements
  v
Layer 2: Attestor (Game Server)
  |  Signs match results, computes anti-cheat scores
  |  Trust basis: cryptographic identity, studio authorization
  v
Layer 3: Validators
  |  Verify signatures, enforce quarantine rules
  |  Trust basis: consensus protocol, staking incentives
  v
Layer 4: Chain State
     Immutable record of match results and rewards
     Trust basis: BFT consensus finality
```

### What the attestation system does NOT guarantee

- It does not guarantee that the game server is running honest game logic. A compromised server can produce valid signatures on fabricated results.
- It does not guarantee that players did not cheat within the game. Anti-cheat is the game server's responsibility; the chain only acts on the score the server provides.
- It does not replace game server security. The private key of the attestor must be protected by the game studio's infrastructure.

### What the attestation system DOES guarantee

- Every onchain match result is signed by a registered attestor with a verified cryptographic signature.
- The attestor is authorized by a specific game studio for a specific game.
- The anti-cheat score is recorded immutably, creating accountability for the game server's integrity assessment.
- Rewards are only distributed for non-quarantined results.
- All match results, including quarantined ones, are permanently recorded for auditing.

## Future Enhancements

### Multi-attestor quorum

Require match results to be signed by multiple attestors (e.g., 2-of-3 game servers) before settlement. This protects against a single compromised server fabricating results.

### Reputation-based attestor scoring

Track attestor reliability over time. Attestors whose results are frequently quarantined or disputed have their trust score reduced. Results from low-reputation attestors could require additional verification or face higher quarantine thresholds.

### Player-signed acknowledgment

Require player signatures on match results in addition to the attestor signature. This proves that the game client agrees with the reported outcome, adding a second layer of verification (defense against server-side result manipulation).

### Asset rewards

Extend the reward system to distribute game assets (not just POL) as match rewards. A tournament could award a rare weapon skin to the winner, minted and transferred atomically as part of match settlement.

### Dispute resolution

A formal dispute system where players can challenge quarantined (or non-quarantined) results with evidence. Disputes could be resolved by governance vote, studio arbitration, or automated analysis.
