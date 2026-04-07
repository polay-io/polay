# Tokenomics

The POLAY network uses the **POL** token for staking, gas fees, governance, and in-game economies.

## Token Basics

| Property | Value |
|---|---|
| Token symbol | POL |
| Decimals | 6 |
| Genesis supply | 100,000,000 POL |
| Smallest unit | 1 microPOL (0.000001 POL) |

## Genesis Distribution

The initial 100M POL is allocated at genesis:

| Allocation | Amount | Percentage | Notes |
|---|---|---|---|
| Validator staking | 40,000,000 | 40% | Distributed to initial validators |
| Ecosystem fund | 25,000,000 | 25% | For grants, partnerships, developer incentives |
| Treasury | 15,000,000 | 15% | Governance-controlled spending |
| Team & advisors | 15,000,000 | 15% | 2-year vesting with 6-month cliff |
| Community airdrop | 5,000,000 | 5% | Early community participants |

## Inflation Schedule

New POL is minted each epoch to reward validators and delegators. The inflation rate decays over time:

| Year | Annual Inflation Rate |
|---|---|
| 1 | 8.00% |
| 2 | 7.60% |
| 3 | 7.22% |
| 4 | 6.86% |
| 5 | 6.52% |
| ... | ... (5% decay each year) |
| Floor | 2.00% (reached ~year 28) |

The formula:

```
rate(year) = max(initial_rate * (1 - decay_rate)^(year - 1), floor_rate)
rate(year) = max(0.08 * 0.95^(year - 1), 0.02)
```

Inflation is calculated and distributed at each epoch transition (every 100 blocks).

## Fee Structure

Every transaction pays a gas fee in POL:

```
fee = gas_used * gas_price
```

The minimum `gas_price` is set by the network (currently 1 microPOL per gas unit). Users can set a higher gas price for priority.

## Fee Distribution

Collected fees are split each block:

```
+------------------+
|   Collected Fee   |
+------------------+
         |
    +----+----+----+
    |         |    |
   50%       20%  30%
    |         |    |
  Burned   Treasury  Validator
```

| Recipient | Share | Purpose |
|---|---|---|
| **Burn** | 50% | Permanently removed from circulating supply; creates deflationary pressure to offset inflation |
| **Treasury** | 20% | Accumulated for governance-directed spending (grants, upgrades, marketing) |
| **Block proposer** | 30% | Direct reward to the validator who proposed the block |

## Supply Tracking

The chain tracks supply information in state via `SupplyInfo`:

```rust
pub struct SupplyInfo {
    pub total_supply: u64,       // genesis + minted - burned
    pub circulating_supply: u64, // total - staked - unbonding - treasury
    pub total_staked: u64,       // all active delegations
    pub total_unbonding: u64,    // tokens in unbonding period
    pub treasury_balance: u64,   // governance treasury
    pub total_burned: u64,       // cumulative burned tokens
    pub total_minted: u64,       // cumulative minted (inflation)
}
```

Query via RPC:

```bash
curl -s http://localhost:9944 -d '{
  "jsonrpc":"2.0","id":1,"method":"state_getSupplyInfo","params":[]
}'
```

## Economic Model

The dual mechanism of inflation (rewarding stakers) and fee burning (reducing supply) creates a balanced economic model:

- **High network activity** -- more fees burned, potentially net-deflationary
- **Low network activity** -- inflation dominates, maintaining validator incentives
- **Equilibrium** -- at sufficient usage, burn rate offsets inflation, stabilizing supply

### Example Scenario (Year 1)

```
Genesis supply:           100,000,000 POL
Inflation (8%):          +  8,000,000 POL
Fees collected:             1,000,000 POL
  Burned (50%):          -    500,000 POL
  Treasury (20%):             200,000 POL
  Validators (30%):           300,000 POL

End of year supply:       107,500,000 POL
Effective inflation:               7.5%
```

## Governance Over Economics

The following economic parameters can be changed through governance proposals:

- Inflation rate and decay schedule
- Fee distribution ratios
- Minimum gas price
- Treasury spending
- Validator commission bounds

This ensures the economic model can adapt to network conditions over time.
