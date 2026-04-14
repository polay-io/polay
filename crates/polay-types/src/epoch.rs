use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;

/// Information about a completed epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EpochInfo {
    /// Epoch number (height / epoch_length).
    pub epoch: u64,
    /// First block height of this epoch.
    pub start_height: u64,
    /// Last block height of this epoch.
    pub end_height: u64,
    /// Active validator addresses for this epoch.
    pub validator_set: Vec<Address>,
    /// Total staked amount in this epoch.
    pub total_staked: u64,
    /// Total rewards distributed at epoch end.
    pub rewards_distributed: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip() {
        let info = EpochInfo {
            epoch: 5,
            start_height: 36000,
            end_height: 43199,
            validator_set: vec![Address::ZERO],
            total_staked: 1_000_000,
            rewards_distributed: 720_000,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: EpochInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let info = EpochInfo {
            epoch: 1,
            start_height: 7200,
            end_height: 14399,
            validator_set: vec![Address::ZERO, Address::new([1u8; 32])],
            total_staked: 500_000,
            rewards_distributed: 360_000,
        };
        let encoded = borsh::to_vec(&info).unwrap();
        let decoded = EpochInfo::try_from_slice(&encoded).unwrap();
        assert_eq!(info, decoded);
    }
}
