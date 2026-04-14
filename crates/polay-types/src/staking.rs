use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;

/// Lifecycle status of a validator.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub enum ValidatorStatus {
    /// Validator is participating in consensus.
    Active,
    /// Validator is temporarily jailed for misbehavior (e.g., downtime).
    Jailed,
    /// Validator has initiated unbonding and is in the cooldown period.
    Unbonding,
    /// Validator has been permanently removed from the set (e.g., double signing).
    Tombstoned,
}

/// On-chain state for a validator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorInfo {
    /// The validator's address.
    pub address: Address,
    /// Total staked amount (self-stake + delegations).
    pub stake: u64,
    /// Commission rate in basis points (1 bps = 0.01%).
    /// For example, 500 = 5%.
    pub commission_bps: u16,
    /// Current lifecycle status.
    pub status: ValidatorStatus,
    /// If jailed, the earliest time (unix seconds) the validator can unjail.
    /// `None` when not jailed.
    pub jailed_until: Option<u64>,
    /// Cumulative number of blocks proposed by this validator.
    pub blocks_produced: u64,
}

impl ValidatorInfo {
    /// Create a new validator that starts in `Active` status.
    pub fn new(address: Address, commission_bps: u16) -> Self {
        Self {
            address,
            stake: 0,
            commission_bps,
            status: ValidatorStatus::Active,
            jailed_until: None,
            blocks_produced: 0,
        }
    }

    /// Returns `true` if the validator is currently participating in consensus.
    pub fn is_active(&self) -> bool {
        self.status == ValidatorStatus::Active
    }

    /// Returns `true` if the validator can be selected as a block proposer.
    pub fn can_propose(&self) -> bool {
        self.is_active() && self.stake > 0
    }

    /// Compute the commission amount on a given reward.
    pub fn commission_on(&self, reward: u64) -> u64 {
        let r = reward as u128;
        let c = (r * self.commission_bps as u128) / 10_000u128;
        c as u64
    }

    /// Jail the validator until `until` (unix seconds).
    pub fn jail(&mut self, until: u64) {
        self.status = ValidatorStatus::Jailed;
        self.jailed_until = Some(until);
    }

    /// Unjail the validator if the jail period has elapsed.
    pub fn try_unjail(&mut self, now: u64) -> bool {
        match self.jailed_until {
            Some(until) if now >= until => {
                self.status = ValidatorStatus::Active;
                self.jailed_until = None;
                true
            }
            _ => false,
        }
    }

    /// Tombstone the validator (permanent removal).
    pub fn tombstone(&mut self) {
        self.status = ValidatorStatus::Tombstoned;
        self.jailed_until = None;
    }
}

/// A delegation from a delegator to a validator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Delegation {
    /// The delegator's address.
    pub delegator: Address,
    /// The validator's address.
    pub validator: Address,
    /// Amount currently delegated.
    pub amount: u64,
    /// Accumulated reward debt used in the reward-distribution accounting.
    pub reward_debt: u64,
    /// Epoch in which this delegation was last created or modified.
    /// Delegations created in the current epoch do not earn rewards
    /// until the next epoch (prevents same-epoch reward gaming).
    #[serde(default)]
    pub last_reward_epoch: u64,
}

impl Delegation {
    /// Create a new delegation with zero amount and zero reward debt.
    pub fn new(delegator: Address, validator: Address) -> Self {
        Self {
            delegator,
            validator,
            amount: 0,
            reward_debt: 0,
            last_reward_epoch: 0,
        }
    }

    /// Add more stake.
    pub fn add_stake(&mut self, amount: u64) {
        self.amount = self.amount.saturating_add(amount);
    }

    /// Remove stake, returning an error if insufficient.
    pub fn remove_stake(&mut self, amount: u64) -> Result<(), String> {
        if self.amount < amount {
            return Err(format!(
                "insufficient delegation: requested {}, delegated {}",
                amount, self.amount
            ));
        }
        self.amount -= amount;
        Ok(())
    }
}

/// An entry in the unbonding queue.
///
/// When a delegator initiates undelegation, the funds are not returned
/// immediately. Instead, an `UnbondingEntry` is created and the funds
/// are released only after `completion_height` is reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct UnbondingEntry {
    /// The delegator whose funds are being unbonded.
    pub delegator: Address,
    /// The validator from which funds are being unbonded.
    pub validator: Address,
    /// Amount of tokens being unbonded.
    pub amount: u64,
    /// Block height when unbonding was initiated.
    pub initiated_at: u64,
    /// Block height when funds can be released.
    pub completion_height: u64,
}

/// A record of a slashing event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SlashEvent {
    /// The validator that was slashed.
    pub validator: Address,
    /// Amount slashed from the validator's total stake.
    pub amount: u64,
    /// Human-readable reason.
    pub reason: String,
    /// Block height at which the slash occurred.
    pub height: u64,
}

/// Evidence of validator equivocation (e.g., double signing or double proposing).
///
/// This type sets up the data model for a future equivocation detection and
/// punishment system. Evidence records are stored on-chain and can be used
/// to trigger automatic slashing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EquivocationEvidence {
    /// The validator accused of equivocation.
    pub validator: Address,
    /// Block height at which the equivocation occurred.
    pub height: u64,
    /// Type of equivocation: "double_sign", "double_propose", etc.
    pub evidence_type: String,
    /// Address of the entity that submitted the evidence.
    pub submitted_by: Address,
    /// Timestamp when the evidence was submitted.
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_validator_is_active() {
        let v = ValidatorInfo::new(Address::ZERO, 500);
        assert!(v.is_active());
        assert!(!v.can_propose()); // no stake yet
    }

    #[test]
    fn can_propose_with_stake() {
        let mut v = ValidatorInfo::new(Address::ZERO, 500);
        v.stake = 1000;
        assert!(v.can_propose());
    }

    #[test]
    fn commission_calculation() {
        let v = ValidatorInfo::new(Address::ZERO, 1000); // 10%
        assert_eq!(v.commission_on(10_000), 1000);
        assert_eq!(v.commission_on(1), 0); // rounds down
    }

    #[test]
    fn jail_and_unjail() {
        let mut v = ValidatorInfo::new(Address::ZERO, 500);
        v.jail(1000);
        assert_eq!(v.status, ValidatorStatus::Jailed);
        assert!(!v.try_unjail(999));
        assert!(v.try_unjail(1000));
        assert!(v.is_active());
    }

    #[test]
    fn tombstone_is_permanent() {
        let mut v = ValidatorInfo::new(Address::ZERO, 500);
        v.tombstone();
        assert_eq!(v.status, ValidatorStatus::Tombstoned);
        assert!(!v.try_unjail(u64::MAX));
    }

    #[test]
    fn delegation_add_remove() {
        let mut d = Delegation::new(Address::ZERO, Address::new([1u8; 32]));
        d.add_stake(500);
        d.add_stake(300);
        assert_eq!(d.amount, 800);
        d.remove_stake(200).unwrap();
        assert_eq!(d.amount, 600);
        assert!(d.remove_stake(1000).is_err());
    }

    #[test]
    fn serde_round_trip_validator() {
        let mut v = ValidatorInfo::new(Address::ZERO, 250);
        v.stake = 50_000;
        v.blocks_produced = 42;
        let json = serde_json::to_string(&v).unwrap();
        let parsed: ValidatorInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(v, parsed);
    }

    #[test]
    fn borsh_round_trip_validator() {
        let v = ValidatorInfo::new(Address::ZERO, 100);
        let encoded = borsh::to_vec(&v).unwrap();
        let decoded = ValidatorInfo::try_from_slice(&encoded).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn serde_round_trip_slash() {
        let slash = SlashEvent {
            validator: Address::ZERO,
            amount: 10_000,
            reason: "double signing".into(),
            height: 12345,
        };
        let json = serde_json::to_string(&slash).unwrap();
        let parsed: SlashEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(slash, parsed);
    }
}
