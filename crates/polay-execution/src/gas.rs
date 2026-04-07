//! Gas metering — deterministic cost calculation for transactions.
//!
//! Every transaction consumes gas proportional to its computational and storage
//! cost. The [`GasSchedule`] provides fixed gas costs per transaction type,
//! and helpers to compute total gas and fees.

use polay_types::{Transaction, TransactionAction};

/// Fixed gas costs per transaction action type.
pub struct GasSchedule;

impl GasSchedule {
    /// Get the gas cost for a transaction action (excluding base gas).
    pub fn action_gas(action: &TransactionAction) -> u64 {
        match action {
            TransactionAction::Transfer { .. } => 5_000,
            TransactionAction::CreateAssetClass { .. } => 50_000,
            TransactionAction::MintAsset { .. } => 30_000,
            TransactionAction::TransferAsset { .. } => 10_000,
            TransactionAction::BurnAsset { .. } => 10_000,
            TransactionAction::CreateListing { .. } => 40_000,
            TransactionAction::CancelListing { .. } => 20_000,
            TransactionAction::BuyListing { .. } => 60_000,
            TransactionAction::CreateProfile { .. } => 30_000,
            TransactionAction::AddAchievement { .. } => 20_000,
            TransactionAction::UpdateReputation { .. } => 15_000,
            TransactionAction::RegisterValidator { .. } => 100_000,
            TransactionAction::DelegateStake { .. } => 30_000,
            TransactionAction::UndelegateStake { .. } => 30_000,
            TransactionAction::RegisterAttestor { .. } => 50_000,
            TransactionAction::SubmitMatchResult { .. } => 80_000,
            TransactionAction::DistributeReward { rewards, .. } => {
                40_000 + rewards.len() as u64 * 5_000
            }
            TransactionAction::SubmitProposal { .. } => 100_000,
            TransactionAction::VoteProposal { .. } => 30_000,
            TransactionAction::ExecuteProposal { .. } => 50_000,
            TransactionAction::CreateSession { .. } => 50_000,
            TransactionAction::RevokeSession { .. } => 20_000,
            // Rentals
            TransactionAction::ListForRent { .. } => 30_000,
            TransactionAction::RentAsset { .. } => 40_000,
            TransactionAction::ReturnRental { .. } => 25_000,
            TransactionAction::ClaimExpiredRental { .. } => 25_000,
            TransactionAction::CancelRentalListing { .. } => 20_000,
            // Guilds
            TransactionAction::CreateGuild { .. } => 50_000,
            TransactionAction::JoinGuild { .. } => 20_000,
            TransactionAction::LeaveGuild { .. } => 20_000,
            TransactionAction::GuildDeposit { .. } => 25_000,
            TransactionAction::GuildWithdraw { .. } => 25_000,
            TransactionAction::GuildPromote { .. } => 15_000,
            TransactionAction::GuildKick { .. } => 20_000,
            // Tournaments
            TransactionAction::CreateTournament { .. } => 50_000,
            TransactionAction::JoinTournament { .. } => 25_000,
            TransactionAction::StartTournament { .. } => 30_000,
            TransactionAction::ReportTournamentResults { rankings, .. } => {
                40_000 + rankings.len() as u64 * 5_000
            }
            TransactionAction::ClaimTournamentPrize { .. } => 25_000,
            TransactionAction::CancelTournament { tournament_id: _ } => {
                // Variable gas based on participants — but we only have the ID
                // at gas estimation time, so use the base cost. The actual
                // per-participant refund cost is accounted for during execution.
                30_000
            }
        }
    }

    /// Calculate total gas for a transaction.
    ///
    /// Total gas = base_gas + action_gas + (serialized_size * gas_per_byte)
    pub fn total_gas(tx: &Transaction, base_gas: u64, gas_per_byte: u64) -> u64 {
        let action_gas = Self::action_gas(&tx.action);
        let tx_size = borsh::to_vec(tx).map(|v| v.len() as u64).unwrap_or(0);
        base_gas + action_gas + tx_size * gas_per_byte
    }

    /// Calculate the fee in POL sub-units for a given gas amount and price.
    pub fn fee(gas: u64, gas_price: u64) -> u64 {
        gas.saturating_mul(gas_price)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_types::{Address, Hash, TransactionAction};

    fn sample_transaction(action: TransactionAction) -> Transaction {
        Transaction {
            chain_id: "polay-devnet-1".into(),
            nonce: 0,
            signer: Address::ZERO,
            action,
            max_fee: 1_000_000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        }
    }

    #[test]
    fn transfer_action_gas() {
        let gas = GasSchedule::action_gas(&TransactionAction::Transfer {
            to: Address::ZERO,
            amount: 1000,
        });
        assert_eq!(gas, 5_000);
    }

    #[test]
    fn register_validator_action_gas() {
        let gas = GasSchedule::action_gas(&TransactionAction::RegisterValidator {
            commission_bps: 500,
        });
        assert_eq!(gas, 100_000);
    }

    #[test]
    fn distribute_reward_scales_with_recipients() {
        let gas_1 = GasSchedule::action_gas(&TransactionAction::DistributeReward {
            match_id: Hash::ZERO,
            rewards: vec![(Address::ZERO, 100)],
        });
        assert_eq!(gas_1, 45_000); // 40_000 + 1 * 5_000

        let gas_3 = GasSchedule::action_gas(&TransactionAction::DistributeReward {
            match_id: Hash::ZERO,
            rewards: vec![
                (Address::ZERO, 100),
                (Address::ZERO, 200),
                (Address::ZERO, 300),
            ],
        });
        assert_eq!(gas_3, 55_000); // 40_000 + 3 * 5_000
    }

    #[test]
    fn total_gas_includes_base_and_size() {
        let tx = sample_transaction(TransactionAction::Transfer {
            to: Address::ZERO,
            amount: 1000,
        });
        let base_gas = 21_000;
        let gas_per_byte = 16;
        let total = GasSchedule::total_gas(&tx, base_gas, gas_per_byte);

        // base_gas (21000) + action_gas (5000) + serialized_size * 16
        let expected_size = borsh::to_vec(&tx).unwrap().len() as u64;
        assert_eq!(total, 21_000 + 5_000 + expected_size * 16);
        assert!(total > 26_000); // sanity check
    }

    #[test]
    fn fee_calculation() {
        let gas = 26_000u64;
        let gas_price = 10u64;
        assert_eq!(GasSchedule::fee(gas, gas_price), 260_000);
    }

    #[test]
    fn fee_saturating_on_overflow() {
        let gas = u64::MAX;
        let gas_price = 2;
        assert_eq!(GasSchedule::fee(gas, gas_price), u64::MAX);
    }

    #[test]
    fn all_action_types_have_nonzero_gas() {
        let actions: Vec<TransactionAction> = vec![
            TransactionAction::Transfer { to: Address::ZERO, amount: 1 },
            TransactionAction::CreateAssetClass {
                name: "X".into(), symbol: "X".into(),
                asset_type: polay_types::AssetType::Fungible,
                max_supply: None, metadata_uri: "".into(),
            },
            TransactionAction::MintAsset {
                asset_class_id: Hash::ZERO, to: Address::ZERO,
                amount: 1, metadata: None,
            },
            TransactionAction::TransferAsset {
                asset_class_id: Hash::ZERO, to: Address::ZERO, amount: 1,
            },
            TransactionAction::BurnAsset { asset_class_id: Hash::ZERO, amount: 1 },
            TransactionAction::CreateListing {
                asset_class_id: Hash::ZERO, amount: 1,
                price_per_unit: 1, currency: Hash::ZERO,
            },
            TransactionAction::CancelListing { listing_id: Hash::ZERO },
            TransactionAction::BuyListing { listing_id: Hash::ZERO },
            TransactionAction::CreateProfile {
                username: "a".into(), display_name: "A".into(), metadata: None,
            },
            TransactionAction::AddAchievement {
                player: Address::ZERO, achievement_id: "a".into(),
                name: "A".into(), metadata: "{}".into(),
            },
            TransactionAction::UpdateReputation {
                player: Address::ZERO, delta: 1, reason: "x".into(),
            },
            TransactionAction::RegisterValidator { commission_bps: 500 },
            TransactionAction::DelegateStake { validator: Address::ZERO, amount: 1 },
            TransactionAction::UndelegateStake { validator: Address::ZERO, amount: 1 },
            TransactionAction::RegisterAttestor {
                game_id: "g".into(), endpoint: "e".into(), metadata: "{}".into(),
            },
            TransactionAction::SubmitMatchResult {
                match_result: polay_types::MatchResult {
                    match_id: Hash::ZERO, game_id: "g".into(),
                    timestamp: 0, players: vec![], scores: vec![],
                    winners: vec![], reward_pool: 0,
                    server_signature: vec![], anti_cheat_score: None,
                    replay_ref: None,
                },
            },
            TransactionAction::DistributeReward {
                match_id: Hash::ZERO, rewards: vec![],
            },
            TransactionAction::SubmitProposal {
                action: polay_types::ProposalAction::TextProposal {
                    title: "t".into(), description: "d".into(),
                },
                title: "t".into(), description: "d".into(), deposit: 100_000,
            },
            TransactionAction::VoteProposal {
                proposal_id: Hash::ZERO,
                option: polay_types::VoteOption::Yes,
            },
            TransactionAction::ExecuteProposal {
                proposal_id: Hash::ZERO,
            },
            TransactionAction::CreateSession {
                session_pubkey: vec![0u8; 32],
                permissions: polay_types::SessionPermission::All,
                expires_at: 1000,
                spending_limit: 100_000,
            },
            TransactionAction::RevokeSession {
                session_address: Address::ZERO,
            },
        ];

        for action in &actions {
            assert!(
                GasSchedule::action_gas(action) > 0 || matches!(action, TransactionAction::DistributeReward { rewards, .. } if rewards.is_empty()),
                "action {} should have nonzero gas (except empty distribute)",
                action.label()
            );
        }
    }
}
