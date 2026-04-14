//! Comprehensive input validation for all transaction action data.
//!
//! These checks are purely structural -- they validate field lengths, ranges,
//! and formats without accessing any on-chain state. They run as part of
//! stateless validation to reject malformed transactions as early as possible.

use polay_config::ChainConfig;
use polay_types::{SignedTransaction, Transaction, TransactionAction};

use crate::error::ExecutionError;

/// Validate all action-specific input fields of a transaction.
pub fn validate_transaction_input(
    tx: &Transaction,
    config: &ChainConfig,
) -> Result<(), ExecutionError> {
    let signer = &tx.signer;

    match &tx.action {
        TransactionAction::Transfer { to, amount } => {
            if *amount == 0 {
                return Err(ExecutionError::InvalidInput(
                    "transfer amount must be > 0".into(),
                ));
            }
            if to == signer {
                return Err(ExecutionError::InvalidInput(
                    "cannot transfer to self".into(),
                ));
            }
            if to.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "cannot transfer to zero address".into(),
                ));
            }
        }

        TransactionAction::CreateAssetClass {
            name,
            symbol,
            max_supply,
            metadata_uri,
            ..
        } => {
            if name.is_empty() || name.len() > 128 {
                return Err(ExecutionError::InvalidInput(
                    "asset class name must be 1-128 characters".into(),
                ));
            }
            if symbol.is_empty() || symbol.len() > 32 {
                return Err(ExecutionError::InvalidInput(
                    "asset class symbol must be 1-32 characters".into(),
                ));
            }
            if !symbol.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err(ExecutionError::InvalidInput(
                    "asset class symbol must be alphanumeric".into(),
                ));
            }
            if metadata_uri.len() > 2048 {
                return Err(ExecutionError::InvalidInput(
                    "metadata_uri must be < 2048 characters".into(),
                ));
            }
            if let Some(max) = max_supply {
                if *max == 0 {
                    return Err(ExecutionError::InvalidInput(
                        "max_supply must be > 0 if set".into(),
                    ));
                }
            }
        }

        TransactionAction::MintAsset { amount, .. } => {
            if *amount == 0 {
                return Err(ExecutionError::InvalidInput(
                    "mint amount must be > 0".into(),
                ));
            }
        }

        TransactionAction::TransferAsset { to, amount, .. } => {
            if *amount == 0 {
                return Err(ExecutionError::InvalidInput(
                    "transfer asset amount must be > 0".into(),
                ));
            }
            if to == signer {
                return Err(ExecutionError::InvalidInput(
                    "cannot transfer asset to self".into(),
                ));
            }
        }

        TransactionAction::BurnAsset { amount, .. } => {
            if *amount == 0 {
                return Err(ExecutionError::InvalidInput(
                    "burn amount must be > 0".into(),
                ));
            }
        }

        TransactionAction::CreateListing {
            amount,
            price_per_unit,
            ..
        } => {
            if *amount == 0 {
                return Err(ExecutionError::InvalidInput(
                    "listing amount must be > 0".into(),
                ));
            }
            if *price_per_unit == 0 {
                return Err(ExecutionError::InvalidInput(
                    "listing price_per_unit must be > 0".into(),
                ));
            }
        }

        TransactionAction::BuyListing { listing_id } => {
            if listing_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "listing_id must not be zero".into(),
                ));
            }
        }

        TransactionAction::CreateProfile {
            username,
            display_name,
            ..
        } => {
            validate_username(username)?;
            if display_name.is_empty() || display_name.len() > 128 {
                return Err(ExecutionError::InvalidInput(
                    "display_name must be 1-128 characters".into(),
                ));
            }
        }

        TransactionAction::AddAchievement {
            achievement_id,
            name,
            ..
        } => {
            if achievement_id.is_empty() || achievement_id.len() > 128 {
                return Err(ExecutionError::InvalidInput(
                    "achievement_id must be 1-128 characters".into(),
                ));
            }
            if name.is_empty() || name.len() > 256 {
                return Err(ExecutionError::InvalidInput(
                    "achievement name must be 1-256 characters".into(),
                ));
            }
        }

        TransactionAction::UpdateReputation { delta, reason, .. } => {
            if *delta == 0 {
                return Err(ExecutionError::InvalidInput(
                    "reputation delta must not be zero".into(),
                ));
            }
            if reason.is_empty() || reason.len() > 512 {
                return Err(ExecutionError::InvalidInput(
                    "reputation reason must be 1-512 characters".into(),
                ));
            }
        }

        TransactionAction::RegisterValidator { commission_bps } => {
            if *commission_bps > config.max_commission_bps {
                return Err(ExecutionError::InvalidInput(format!(
                    "commission_bps {} exceeds max {}",
                    commission_bps, config.max_commission_bps
                )));
            }
        }

        TransactionAction::DelegateStake { amount, .. } => {
            if *amount == 0 {
                return Err(ExecutionError::InvalidInput(
                    "delegate stake amount must be > 0".into(),
                ));
            }
        }

        TransactionAction::UndelegateStake { amount, .. } => {
            if *amount == 0 {
                return Err(ExecutionError::InvalidInput(
                    "undelegate stake amount must be > 0".into(),
                ));
            }
        }

        TransactionAction::RegisterAttestor {
            game_id, endpoint, ..
        } => {
            if game_id.is_empty() || game_id.len() > 64 {
                return Err(ExecutionError::InvalidInput(
                    "game_id must be 1-64 characters".into(),
                ));
            }
            if endpoint.len() > 512 {
                return Err(ExecutionError::InvalidInput(
                    "endpoint must be < 512 characters".into(),
                ));
            }
        }

        TransactionAction::SubmitMatchResult { match_result } => {
            if match_result.players.is_empty() {
                return Err(ExecutionError::InvalidInput(
                    "match must have at least 1 player".into(),
                ));
            }
            if match_result.winners.is_empty() {
                return Err(ExecutionError::InvalidInput(
                    "match must have at least 1 winner".into(),
                ));
            }
            if match_result.scores.len() != match_result.players.len() {
                return Err(ExecutionError::InvalidInput(
                    "scores length must equal players length".into(),
                ));
            }
            for winner in &match_result.winners {
                if !match_result.players.contains(winner) {
                    return Err(ExecutionError::InvalidInput(
                        "all winners must be in the players list".into(),
                    ));
                }
            }
        }

        TransactionAction::DistributeReward { rewards, .. } => {
            if rewards.is_empty() {
                return Err(ExecutionError::InvalidInput(
                    "must have at least 1 reward entry".into(),
                ));
            }
            for (_, amount) in rewards {
                if *amount == 0 {
                    return Err(ExecutionError::InvalidInput(
                        "reward amounts must be > 0".into(),
                    ));
                }
            }
        }

        TransactionAction::SubmitProposal {
            title,
            description,
            deposit,
            ..
        } => {
            if title.is_empty() || title.len() > 256 {
                return Err(ExecutionError::InvalidInput(
                    "proposal title must be 1-256 characters".into(),
                ));
            }
            if description.is_empty() || description.len() > 4096 {
                return Err(ExecutionError::InvalidInput(
                    "proposal description must be 1-4096 characters".into(),
                ));
            }
            if *deposit == 0 {
                return Err(ExecutionError::InvalidInput(
                    "proposal deposit must be > 0".into(),
                ));
            }
        }

        // CancelListing, VoteProposal, ExecuteProposal, session keys -- no extra validation needed
        TransactionAction::CancelListing { .. }
        | TransactionAction::VoteProposal { .. }
        | TransactionAction::ExecuteProposal { .. }
        | TransactionAction::CreateSession { .. }
        | TransactionAction::RevokeSession { .. } => {}

        // -- Rentals --------------------------------------------------------------
        TransactionAction::ListForRent {
            asset_class_id,
            asset_id,
            price_per_block,
            deposit: _,
            min_duration,
            max_duration,
        } => {
            if asset_class_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "asset_class_id must not be zero".into(),
                ));
            }
            if asset_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "asset_id must not be zero".into(),
                ));
            }
            if *price_per_block == 0 {
                return Err(ExecutionError::InvalidInput(
                    "price_per_block must be > 0".into(),
                ));
            }
            if *min_duration == 0 {
                return Err(ExecutionError::InvalidInput(
                    "min_duration must be > 0".into(),
                ));
            }
            if *max_duration < *min_duration {
                return Err(ExecutionError::InvalidInput(
                    "max_duration must be >= min_duration".into(),
                ));
            }
        }

        TransactionAction::RentAsset {
            rental_id,
            duration,
        } => {
            if rental_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "rental_id must not be zero".into(),
                ));
            }
            if *duration == 0 {
                return Err(ExecutionError::InvalidInput("duration must be > 0".into()));
            }
        }

        TransactionAction::ReturnRental { rental_id } => {
            if rental_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "rental_id must not be zero".into(),
                ));
            }
        }

        TransactionAction::ClaimExpiredRental { rental_id } => {
            if rental_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "rental_id must not be zero".into(),
                ));
            }
        }

        TransactionAction::CancelRentalListing { rental_id } => {
            if rental_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "rental_id must not be zero".into(),
                ));
            }
        }

        // -- Guilds ---------------------------------------------------------------
        TransactionAction::CreateGuild {
            name,
            description,
            max_members,
        } => {
            if name.is_empty() || name.len() > 64 {
                return Err(ExecutionError::InvalidInput(
                    "guild name must be 1-64 characters".into(),
                ));
            }
            if description.len() > 256 {
                return Err(ExecutionError::InvalidInput(
                    "guild description must be 0-256 characters".into(),
                ));
            }
            if *max_members == 0 || *max_members > 10_000 {
                return Err(ExecutionError::InvalidInput(
                    "max_members must be 1-10000".into(),
                ));
            }
        }

        TransactionAction::JoinGuild { guild_id } => {
            if guild_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "guild_id must not be zero".into(),
                ));
            }
        }

        TransactionAction::LeaveGuild { guild_id } => {
            if guild_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "guild_id must not be zero".into(),
                ));
            }
        }

        TransactionAction::GuildDeposit { guild_id, amount } => {
            if guild_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "guild_id must not be zero".into(),
                ));
            }
            if *amount == 0 {
                return Err(ExecutionError::InvalidInput(
                    "deposit amount must be > 0".into(),
                ));
            }
        }

        TransactionAction::GuildWithdraw { guild_id, amount } => {
            if guild_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "guild_id must not be zero".into(),
                ));
            }
            if *amount == 0 {
                return Err(ExecutionError::InvalidInput(
                    "withdraw amount must be > 0".into(),
                ));
            }
        }

        TransactionAction::GuildPromote {
            guild_id,
            member: _,
            role,
        } => {
            if guild_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "guild_id must not be zero".into(),
                ));
            }
            if role != "officer" && role != "member" {
                return Err(ExecutionError::InvalidInput(
                    "role must be \"officer\" or \"member\"".into(),
                ));
            }
        }

        TransactionAction::GuildKick {
            guild_id,
            member: _,
        } => {
            if guild_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "guild_id must not be zero".into(),
                ));
            }
        }

        // -- Tournaments ----------------------------------------------------------
        TransactionAction::CreateTournament {
            name,
            game_id,
            entry_fee: _,
            max_participants,
            min_participants,
            start_height: _,
            prize_distribution,
        } => {
            if name.is_empty() || name.len() > 128 {
                return Err(ExecutionError::InvalidInput(
                    "tournament name must be 1-128 characters".into(),
                ));
            }
            if game_id.is_empty() || game_id.len() > 64 {
                return Err(ExecutionError::InvalidInput(
                    "game_id must be 1-64 characters".into(),
                ));
            }
            if *min_participants < 2 {
                return Err(ExecutionError::InvalidInput(
                    "min_participants must be >= 2".into(),
                ));
            }
            if *max_participants < *min_participants {
                return Err(ExecutionError::InvalidInput(
                    "max_participants must be >= min_participants".into(),
                ));
            }
            if prize_distribution.is_empty() {
                return Err(ExecutionError::InvalidInput(
                    "prize_distribution must not be empty".into(),
                ));
            }
            if prize_distribution.len() > 100 {
                return Err(ExecutionError::InvalidInput(
                    "prize_distribution must have at most 100 entries".into(),
                ));
            }
            let sum: u32 = prize_distribution.iter().sum();
            if sum != 100 {
                return Err(ExecutionError::InvalidInput(format!(
                    "prize_distribution must sum to 100, got {sum}"
                )));
            }
            for pct in prize_distribution {
                if *pct == 0 || *pct > 100 {
                    return Err(ExecutionError::InvalidInput(
                        "each prize_distribution entry must be 1-100".into(),
                    ));
                }
            }
        }

        TransactionAction::JoinTournament { tournament_id } => {
            if tournament_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "tournament_id must not be zero".into(),
                ));
            }
        }

        TransactionAction::StartTournament { tournament_id } => {
            if tournament_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "tournament_id must not be zero".into(),
                ));
            }
        }

        TransactionAction::ReportTournamentResults {
            tournament_id,
            rankings,
        } => {
            if tournament_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "tournament_id must not be zero".into(),
                ));
            }
            if rankings.is_empty() {
                return Err(ExecutionError::InvalidInput(
                    "rankings must not be empty".into(),
                ));
            }
        }

        TransactionAction::ClaimTournamentPrize { tournament_id } => {
            if tournament_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "tournament_id must not be zero".into(),
                ));
            }
        }

        TransactionAction::CancelTournament { tournament_id } => {
            if tournament_id.is_zero() {
                return Err(ExecutionError::InvalidInput(
                    "tournament_id must not be zero".into(),
                ));
            }
        }
    }

    Ok(())
}

/// Validate a username: 3-32 chars, alphanumeric + underscore, no leading/trailing underscore.
fn validate_username(username: &str) -> Result<(), ExecutionError> {
    let len = username.len();
    if !(3..=32).contains(&len) {
        return Err(ExecutionError::InvalidInput(format!(
            "username must be 3-32 characters, got {len}"
        )));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(ExecutionError::InvalidInput(
            "username may only contain alphanumeric characters and underscores".into(),
        ));
    }
    if username.starts_with('_') || username.ends_with('_') {
        return Err(ExecutionError::InvalidInput(
            "username must not start or end with an underscore".into(),
        ));
    }
    Ok(())
}

/// Validate that a serialized transaction does not exceed the maximum size.
pub fn validate_tx_size(tx: &SignedTransaction, max_size: usize) -> Result<(), ExecutionError> {
    let serialized = borsh::to_vec(&tx.transaction).unwrap_or_default();
    if serialized.len() > max_size {
        return Err(ExecutionError::TransactionTooLarge {
            max: max_size,
            actual: serialized.len(),
        });
    }
    Ok(())
}

/// Validate that a transaction is not expired.
pub fn validate_tx_expiration(
    tx: &Transaction,
    current_timestamp: u64,
    max_age_seconds: u64,
) -> Result<(), ExecutionError> {
    if current_timestamp > tx.timestamp {
        let age = current_timestamp - tx.timestamp;
        if age > max_age_seconds {
            return Err(ExecutionError::TransactionExpired {
                max_age_secs: max_age_seconds,
                tx_age_secs: age,
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_config::ChainConfig;
    use polay_types::{
        attestation::MatchResult, governance::ProposalAction, Address, Hash, Signature,
        TransactionAction,
    };

    fn config() -> ChainConfig {
        ChainConfig::default()
    }

    fn addr(b: u8) -> Address {
        Address::new([b; 32])
    }

    fn base_tx(action: TransactionAction) -> Transaction {
        Transaction {
            chain_id: "polay-devnet-1".into(),
            nonce: 0,
            signer: addr(1),
            action,
            max_fee: 10_000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        }
    }

    // -- Transfer -----------------------------------------------------------

    #[test]
    fn transfer_zero_amount_rejected() {
        let tx = base_tx(TransactionAction::Transfer {
            to: addr(2),
            amount: 0,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn transfer_to_self_rejected() {
        let tx = base_tx(TransactionAction::Transfer {
            to: addr(1), // same as signer
            amount: 100,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn transfer_to_zero_rejected() {
        let tx = base_tx(TransactionAction::Transfer {
            to: Address::ZERO,
            amount: 100,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn transfer_valid() {
        let tx = base_tx(TransactionAction::Transfer {
            to: addr(2),
            amount: 100,
        });
        assert!(validate_transaction_input(&tx, &config()).is_ok());
    }

    // -- CreateAssetClass ---------------------------------------------------

    #[test]
    fn create_asset_empty_name_rejected() {
        let tx = base_tx(TransactionAction::CreateAssetClass {
            name: "".into(),
            symbol: "GLD".into(),
            asset_type: polay_types::asset::AssetType::Fungible,
            max_supply: None,
            metadata_uri: "".into(),
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn create_asset_symbol_too_long_rejected() {
        let tx = base_tx(TransactionAction::CreateAssetClass {
            name: "Gold".into(),
            symbol: "A".repeat(33),
            asset_type: polay_types::asset::AssetType::Fungible,
            max_supply: None,
            metadata_uri: "".into(),
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn create_asset_non_alphanumeric_symbol_rejected() {
        let tx = base_tx(TransactionAction::CreateAssetClass {
            name: "Gold".into(),
            symbol: "GL-D".into(),
            asset_type: polay_types::asset::AssetType::Fungible,
            max_supply: None,
            metadata_uri: "".into(),
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn create_asset_zero_max_supply_rejected() {
        let tx = base_tx(TransactionAction::CreateAssetClass {
            name: "Gold".into(),
            symbol: "GLD".into(),
            asset_type: polay_types::asset::AssetType::Fungible,
            max_supply: Some(0),
            metadata_uri: "".into(),
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn create_asset_valid() {
        let tx = base_tx(TransactionAction::CreateAssetClass {
            name: "Gold".into(),
            symbol: "GLD".into(),
            asset_type: polay_types::asset::AssetType::Fungible,
            max_supply: Some(1_000_000),
            metadata_uri: "https://example.com".into(),
        });
        assert!(validate_transaction_input(&tx, &config()).is_ok());
    }

    // -- MintAsset ----------------------------------------------------------

    #[test]
    fn mint_zero_amount_rejected() {
        let tx = base_tx(TransactionAction::MintAsset {
            asset_class_id: Hash::ZERO,
            to: addr(2),
            amount: 0,
            metadata: None,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- TransferAsset ------------------------------------------------------

    #[test]
    fn transfer_asset_to_self_rejected() {
        let tx = base_tx(TransactionAction::TransferAsset {
            asset_class_id: Hash::ZERO,
            to: addr(1),
            amount: 10,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn transfer_asset_zero_amount_rejected() {
        let tx = base_tx(TransactionAction::TransferAsset {
            asset_class_id: Hash::ZERO,
            to: addr(2),
            amount: 0,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- BurnAsset ----------------------------------------------------------

    #[test]
    fn burn_zero_amount_rejected() {
        let tx = base_tx(TransactionAction::BurnAsset {
            asset_class_id: Hash::ZERO,
            amount: 0,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- CreateListing ------------------------------------------------------

    #[test]
    fn listing_zero_amount_rejected() {
        let tx = base_tx(TransactionAction::CreateListing {
            asset_class_id: Hash::ZERO,
            amount: 0,
            price_per_unit: 100,
            currency: Hash::ZERO,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn listing_zero_price_rejected() {
        let tx = base_tx(TransactionAction::CreateListing {
            asset_class_id: Hash::ZERO,
            amount: 10,
            price_per_unit: 0,
            currency: Hash::ZERO,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- BuyListing ---------------------------------------------------------

    #[test]
    fn buy_listing_zero_id_rejected() {
        let tx = base_tx(TransactionAction::BuyListing {
            listing_id: Hash::ZERO,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- CreateProfile ------------------------------------------------------

    #[test]
    fn profile_short_username_rejected() {
        let tx = base_tx(TransactionAction::CreateProfile {
            username: "ab".into(),
            display_name: "Alice".into(),
            metadata: None,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn profile_empty_display_name_rejected() {
        let tx = base_tx(TransactionAction::CreateProfile {
            username: "alice".into(),
            display_name: "".into(),
            metadata: None,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- AddAchievement -----------------------------------------------------

    #[test]
    fn achievement_empty_id_rejected() {
        let tx = base_tx(TransactionAction::AddAchievement {
            player: addr(2),
            achievement_id: "".into(),
            name: "First Win".into(),
            metadata: "{}".into(),
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- UpdateReputation ---------------------------------------------------

    #[test]
    fn reputation_zero_delta_rejected() {
        let tx = base_tx(TransactionAction::UpdateReputation {
            player: addr(2),
            delta: 0,
            reason: "good behavior".into(),
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn reputation_empty_reason_rejected() {
        let tx = base_tx(TransactionAction::UpdateReputation {
            player: addr(2),
            delta: 5,
            reason: "".into(),
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- RegisterValidator --------------------------------------------------

    #[test]
    fn validator_excessive_commission_rejected() {
        let tx = base_tx(TransactionAction::RegisterValidator {
            commission_bps: 10_000, // > max (2000)
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- DelegateStake / UndelegateStake ------------------------------------

    #[test]
    fn delegate_zero_amount_rejected() {
        let tx = base_tx(TransactionAction::DelegateStake {
            validator: addr(2),
            amount: 0,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn undelegate_zero_amount_rejected() {
        let tx = base_tx(TransactionAction::UndelegateStake {
            validator: addr(2),
            amount: 0,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- RegisterAttestor ---------------------------------------------------

    #[test]
    fn attestor_empty_game_id_rejected() {
        let tx = base_tx(TransactionAction::RegisterAttestor {
            game_id: "".into(),
            endpoint: "https://example.com".into(),
            metadata: "{}".into(),
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- SubmitMatchResult --------------------------------------------------

    #[test]
    fn match_no_players_rejected() {
        let tx = base_tx(TransactionAction::SubmitMatchResult {
            match_result: MatchResult {
                match_id: Hash::ZERO,
                game_id: "chess".into(),
                timestamp: 0,
                players: vec![],
                scores: vec![],
                winners: vec![addr(1)],
                reward_pool: 0,
                server_signature: vec![],
                anti_cheat_score: None,
                replay_ref: None,
            },
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn match_winner_not_in_players_rejected() {
        let tx = base_tx(TransactionAction::SubmitMatchResult {
            match_result: MatchResult {
                match_id: Hash::ZERO,
                game_id: "chess".into(),
                timestamp: 0,
                players: vec![addr(1)],
                scores: vec![100],
                winners: vec![addr(99)],
                reward_pool: 0,
                server_signature: vec![],
                anti_cheat_score: None,
                replay_ref: None,
            },
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn match_scores_length_mismatch_rejected() {
        let tx = base_tx(TransactionAction::SubmitMatchResult {
            match_result: MatchResult {
                match_id: Hash::ZERO,
                game_id: "chess".into(),
                timestamp: 0,
                players: vec![addr(1), addr(2)],
                scores: vec![100],
                winners: vec![addr(1)],
                reward_pool: 0,
                server_signature: vec![],
                anti_cheat_score: None,
                replay_ref: None,
            },
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- DistributeReward ---------------------------------------------------

    #[test]
    fn reward_empty_list_rejected() {
        let tx = base_tx(TransactionAction::DistributeReward {
            match_id: Hash::ZERO,
            rewards: vec![],
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn reward_zero_amount_rejected() {
        let tx = base_tx(TransactionAction::DistributeReward {
            match_id: Hash::ZERO,
            rewards: vec![(addr(1), 0)],
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- SubmitProposal -----------------------------------------------------

    #[test]
    fn proposal_empty_title_rejected() {
        let tx = base_tx(TransactionAction::SubmitProposal {
            action: ProposalAction::TextProposal {
                title: "t".into(),
                description: "d".into(),
            },
            title: "".into(),
            description: "A proposal".into(),
            deposit: 100_000,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    #[test]
    fn proposal_zero_deposit_rejected() {
        let tx = base_tx(TransactionAction::SubmitProposal {
            action: ProposalAction::TextProposal {
                title: "t".into(),
                description: "d".into(),
            },
            title: "Title".into(),
            description: "Description".into(),
            deposit: 0,
        });
        assert!(validate_transaction_input(&tx, &config()).is_err());
    }

    // -- Transaction size limit ---------------------------------------------

    #[test]
    fn tx_size_limit_enforced() {
        let tx = Transaction {
            chain_id: "polay-devnet-1".into(),
            nonce: 0,
            signer: addr(1),
            action: TransactionAction::Transfer {
                to: addr(2),
                amount: 100,
            },
            max_fee: 10_000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };
        let stx = SignedTransaction::new(tx, Signature::ZERO, Hash::ZERO, vec![0u8; 32]);
        // A small limit should reject even a basic tx.
        let result = validate_tx_size(&stx, 10);
        assert!(result.is_err());
        match result.unwrap_err() {
            ExecutionError::TransactionTooLarge { max, actual } => {
                assert_eq!(max, 10);
                assert!(actual > 10);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn tx_size_ok_within_limit() {
        let tx = Transaction {
            chain_id: "polay-devnet-1".into(),
            nonce: 0,
            signer: addr(1),
            action: TransactionAction::Transfer {
                to: addr(2),
                amount: 100,
            },
            max_fee: 10_000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };
        let stx = SignedTransaction::new(tx, Signature::ZERO, Hash::ZERO, vec![0u8; 32]);
        assert!(validate_tx_size(&stx, 65_536).is_ok());
    }

    // -- Transaction expiration ---------------------------------------------

    #[test]
    fn tx_expiration_old_tx_rejected() {
        let tx = Transaction {
            chain_id: "polay-devnet-1".into(),
            nonce: 0,
            signer: addr(1),
            action: TransactionAction::Transfer {
                to: addr(2),
                amount: 100,
            },
            max_fee: 10_000,
            timestamp: 1_000,
            session: None,
            sponsor: None,
        };
        let result = validate_tx_expiration(&tx, 2_000, 300);
        assert!(result.is_err());
    }

    #[test]
    fn tx_expiration_fresh_tx_ok() {
        let tx = Transaction {
            chain_id: "polay-devnet-1".into(),
            nonce: 0,
            signer: addr(1),
            action: TransactionAction::Transfer {
                to: addr(2),
                amount: 100,
            },
            max_fee: 10_000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };
        assert!(validate_tx_expiration(&tx, 1_700_000_100, 300).is_ok());
    }
}
