use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Tracks global supply metrics -- stored on-chain, updated every block/epoch.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Default,
)]
pub struct SupplyInfo {
    /// All POL in existence (initial + minted - burned).
    pub total_supply: u64,
    /// total_supply - staked - treasury - unbonding.
    pub circulating_supply: u64,
    /// All POL currently staked.
    pub total_staked: u64,
    /// Cumulative burned fees.
    pub total_burned: u64,
    /// Protocol treasury balance.
    pub treasury_balance: u64,
    /// Cumulative minted via inflation (block rewards).
    pub total_minted: u64,
    /// Cumulative gas fees collected.
    pub total_fees_collected: u64,
}

impl SupplyInfo {
    /// Recompute `circulating_supply` from the other fields.
    pub fn recompute_circulating(&mut self) {
        self.circulating_supply = self
            .total_supply
            .saturating_sub(self.total_staked)
            .saturating_sub(self.treasury_balance);
    }
}

/// Fee distribution configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct FeeDistribution {
    /// Basis points of fees to burn, e.g. 5000 = 50%.
    pub burn_bps: u16,
    /// Basis points of fees to send to treasury, e.g. 2000 = 20%.
    pub treasury_bps: u16,
    /// Basis points of fees to send to block producer, e.g. 3000 = 30%.
    pub validator_bps: u16,
    // Must sum to 10000.
}

impl Default for FeeDistribution {
    fn default() -> Self {
        Self {
            burn_bps: 5000,
            treasury_bps: 2000,
            validator_bps: 3000,
        }
    }
}

/// Inflation parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct InflationParams {
    /// Initial annual inflation rate in basis points, e.g. 800 = 8%.
    pub initial_rate_bps: u16,
    /// Minimum annual inflation rate in basis points, e.g. 200 = 2%.
    pub min_rate_bps: u16,
    /// Annual decay of the inflation rate in basis points, e.g. 500 = 5%.
    pub decay_rate_bps: u16,
    /// Target staking ratio in basis points, e.g. 6700 = 67%.
    pub target_staking_ratio_bps: u16,
}

impl Default for InflationParams {
    fn default() -> Self {
        Self {
            initial_rate_bps: 800,
            min_rate_bps: 200,
            decay_rate_bps: 500,
            target_staking_ratio_bps: 6700,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supply_info_default_is_zero() {
        let s = SupplyInfo::default();
        assert_eq!(s.total_supply, 0);
        assert_eq!(s.circulating_supply, 0);
        assert_eq!(s.total_staked, 0);
        assert_eq!(s.total_burned, 0);
        assert_eq!(s.treasury_balance, 0);
        assert_eq!(s.total_minted, 0);
        assert_eq!(s.total_fees_collected, 0);
    }

    #[test]
    fn supply_info_serde_round_trip() {
        let s = SupplyInfo {
            total_supply: 100_000_000,
            circulating_supply: 60_000_000,
            total_staked: 35_000_000,
            total_burned: 1_000,
            treasury_balance: 5_000_000,
            total_minted: 500,
            total_fees_collected: 2_000,
        };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: SupplyInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(s, parsed);
    }

    #[test]
    fn supply_info_borsh_round_trip() {
        let s = SupplyInfo {
            total_supply: 100_000_000,
            circulating_supply: 60_000_000,
            total_staked: 35_000_000,
            total_burned: 1_000,
            treasury_balance: 5_000_000,
            total_minted: 500,
            total_fees_collected: 2_000,
        };
        let encoded = borsh::to_vec(&s).unwrap();
        let decoded = SupplyInfo::try_from_slice(&encoded).unwrap();
        assert_eq!(s, decoded);
    }

    #[test]
    fn fee_distribution_default_sums_to_10000() {
        let fd = FeeDistribution::default();
        assert_eq!(
            fd.burn_bps as u32 + fd.treasury_bps as u32 + fd.validator_bps as u32,
            10_000
        );
    }

    #[test]
    fn fee_distribution_serde_round_trip() {
        let fd = FeeDistribution::default();
        let json = serde_json::to_string(&fd).unwrap();
        let parsed: FeeDistribution = serde_json::from_str(&json).unwrap();
        assert_eq!(fd, parsed);
    }

    #[test]
    fn inflation_params_default() {
        let ip = InflationParams::default();
        assert_eq!(ip.initial_rate_bps, 800);
        assert_eq!(ip.min_rate_bps, 200);
        assert_eq!(ip.decay_rate_bps, 500);
        assert_eq!(ip.target_staking_ratio_bps, 6700);
    }

    #[test]
    fn inflation_params_serde_round_trip() {
        let ip = InflationParams::default();
        let json = serde_json::to_string(&ip).unwrap();
        let parsed: InflationParams = serde_json::from_str(&json).unwrap();
        assert_eq!(ip, parsed);
    }

    #[test]
    fn recompute_circulating() {
        let mut s = SupplyInfo {
            total_supply: 100_000_000,
            circulating_supply: 0,
            total_staked: 40_000_000,
            total_burned: 0,
            treasury_balance: 5_000_000,
            total_minted: 0,
            total_fees_collected: 0,
        };
        s.recompute_circulating();
        assert_eq!(s.circulating_supply, 55_000_000);
    }
}
