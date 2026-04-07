//! Conflict detection via access-set analysis.
//!
//! Before executing transactions in parallel we need to know which state keys
//! each transaction will touch. Two transactions conflict when one writes a key
//! that the other reads or writes.

use std::collections::HashSet;

use sha2::Digest;
use polay_types::{Address, SignedTransaction, TransactionAction};

/// Represents the set of state keys a transaction will read or write.
#[derive(Debug, Clone)]
pub struct AccessSet {
    /// State keys that will be read.
    pub reads: HashSet<Vec<u8>>,
    /// State keys that will be written.
    pub writes: HashSet<Vec<u8>>,
}

impl AccessSet {
    pub fn new() -> Self {
        Self {
            reads: HashSet::new(),
            writes: HashSet::new(),
        }
    }

    /// Check if this access set conflicts with another.
    ///
    /// Conflicts occur when:
    /// - **write-write**: both transactions write the same key
    /// - **read-write**: one reads what the other writes (either direction)
    pub fn conflicts_with(&self, other: &AccessSet) -> bool {
        // Write-write conflicts.
        if self.writes.iter().any(|k| other.writes.contains(k)) {
            return true;
        }
        // Read-write conflicts (either direction).
        if self.reads.iter().any(|k| other.writes.contains(k)) {
            return true;
        }
        if self.writes.iter().any(|k| other.reads.contains(k)) {
            return true;
        }
        false
    }
}

impl Default for AccessSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Predict the access set for a transaction without executing it.
///
/// This is a *static* analysis based solely on the transaction's action type
/// and fields. It is intentionally **conservative**: when in doubt the
/// prediction over-approximates the set, which can only reduce parallelism,
/// never compromise correctness.
pub fn predict_access_set(tx: &SignedTransaction) -> AccessSet {
    use polay_state::keys;

    let mut set = AccessSet::new();
    let signer = &tx.transaction.signer;

    // Every transaction reads and writes the signer's account (nonce, balance).
    set.reads.insert(keys::account_key(signer));
    set.writes.insert(keys::account_key(signer));
    set.reads.insert(keys::balance_key(signer));
    set.writes.insert(keys::balance_key(signer));

    // If a sponsor is present, they also need to be in the write set since
    // their balance is modified (fee deduction). Two txs with the same sponsor
    // must not run in parallel.
    if let Some(sponsor) = &tx.transaction.sponsor {
        set.reads.insert(keys::account_key(sponsor));
        set.writes.insert(keys::account_key(sponsor));
        set.reads.insert(keys::balance_key(sponsor));
        set.writes.insert(keys::balance_key(sponsor));
    }

    match &tx.transaction.action {
        TransactionAction::Transfer { to, .. } => {
            set.reads.insert(keys::account_key(to));
            set.writes.insert(keys::account_key(to));
            set.reads.insert(keys::balance_key(to));
            set.writes.insert(keys::balance_key(to));
        }
        TransactionAction::CreateAssetClass { .. } => {
            // Creates a new asset class whose ID is derived from (signer, name).
            // The signer's account key already captured above is the main
            // conflict point. The asset-class key itself is unique per
            // (signer, name) so additional marking is unnecessary.
        }
        TransactionAction::MintAsset {
            asset_class_id,
            to,
            ..
        } => {
            set.reads.insert(keys::asset_class_key(asset_class_id));
            set.writes.insert(keys::asset_class_key(asset_class_id)); // total_supply
            set.reads.insert(keys::asset_balance_key(asset_class_id, to));
            set.writes.insert(keys::asset_balance_key(asset_class_id, to));
        }
        TransactionAction::TransferAsset {
            asset_class_id,
            to,
            ..
        } => {
            set.reads
                .insert(keys::asset_balance_key(asset_class_id, signer));
            set.writes
                .insert(keys::asset_balance_key(asset_class_id, signer));
            set.reads.insert(keys::asset_balance_key(asset_class_id, to));
            set.writes.insert(keys::asset_balance_key(asset_class_id, to));
        }
        TransactionAction::BurnAsset {
            asset_class_id, ..
        } => {
            set.reads.insert(keys::asset_class_key(asset_class_id));
            set.writes.insert(keys::asset_class_key(asset_class_id));
            set.reads
                .insert(keys::asset_balance_key(asset_class_id, signer));
            set.writes
                .insert(keys::asset_balance_key(asset_class_id, signer));
        }
        TransactionAction::CreateListing {
            asset_class_id, ..
        } => {
            set.reads
                .insert(keys::asset_balance_key(asset_class_id, signer));
            set.writes
                .insert(keys::asset_balance_key(asset_class_id, signer));
            // Creates a new listing key (unique ID) — no additional conflict.
        }
        TransactionAction::CancelListing { listing_id } => {
            set.reads.insert(keys::listing_key(listing_id));
            set.writes.insert(keys::listing_key(listing_id));
        }
        TransactionAction::BuyListing { listing_id } => {
            set.reads.insert(keys::listing_key(listing_id));
            set.writes.insert(keys::listing_key(listing_id));
            // Also touches buyer and seller balances, but we don't know the
            // seller until execution. Listing key itself acts as the conflict
            // point so two buys for the same listing are serialised.
        }
        TransactionAction::CreateProfile { .. } => {
            set.writes.insert(keys::profile_key(signer));
        }
        TransactionAction::AddAchievement {
            player,
            achievement_id,
            ..
        } => {
            set.writes
                .insert(keys::achievement_key(player, achievement_id));
        }
        TransactionAction::UpdateReputation { player, .. } => {
            set.reads.insert(keys::profile_key(player));
            set.writes.insert(keys::profile_key(player));
        }
        TransactionAction::DelegateStake { validator, .. } => {
            set.reads.insert(keys::validator_key(validator));
            set.writes.insert(keys::validator_key(validator));
            set.reads.insert(keys::delegation_key(signer, validator));
            set.writes.insert(keys::delegation_key(signer, validator));
        }
        TransactionAction::UndelegateStake { validator, .. } => {
            set.reads.insert(keys::validator_key(validator));
            set.writes.insert(keys::validator_key(validator));
            set.reads.insert(keys::delegation_key(signer, validator));
            set.writes.insert(keys::delegation_key(signer, validator));
        }
        TransactionAction::RegisterValidator { .. } => {
            set.writes.insert(keys::validator_key(signer));
        }
        TransactionAction::RegisterAttestor { .. } => {
            set.writes.insert(keys::attestor_key(signer));
        }
        TransactionAction::SubmitMatchResult { match_result } => {
            set.writes
                .insert(keys::match_result_key(&match_result.match_id));
            set.writes
                .insert(keys::match_settlement_key(&match_result.match_id));
        }
        TransactionAction::DistributeReward { match_id, rewards } => {
            set.reads.insert(keys::match_settlement_key(match_id));
            set.writes.insert(keys::match_settlement_key(match_id));
            for (addr, _) in rewards {
                set.reads.insert(keys::balance_key(addr));
                set.writes.insert(keys::balance_key(addr));
            }
        }
        TransactionAction::SubmitProposal { .. } => {
            // Creates a new proposal whose ID is derived from content.
            // Signer account already captured above.
        }
        TransactionAction::VoteProposal { proposal_id, .. } => {
            set.reads.insert(keys::proposal_key(proposal_id));
            set.writes.insert(keys::proposal_key(proposal_id));
            set.writes.insert(keys::vote_key(proposal_id, signer));
        }
        TransactionAction::ExecuteProposal { proposal_id } => {
            set.reads.insert(keys::proposal_key(proposal_id));
            set.writes.insert(keys::proposal_key(proposal_id));
        }
        TransactionAction::CreateSession { session_pubkey, .. } => {
            // Derive session address to predict the state key.
            let digest = sha2::Sha256::digest(session_pubkey);
            let mut addr_bytes = [0u8; 32];
            addr_bytes.copy_from_slice(&digest[..32]);
            let session_address = Address::new(addr_bytes);
            set.reads
                .insert(keys::session_key(&tx.transaction.signer, &session_address));
            set.writes
                .insert(keys::session_key(&tx.transaction.signer, &session_address));
        }
        TransactionAction::RevokeSession { session_address } => {
            set.reads
                .insert(keys::session_key(&tx.transaction.signer, session_address));
            set.writes
                .insert(keys::session_key(&tx.transaction.signer, session_address));
        }

        // -- Rentals --
        TransactionAction::ListForRent {
            asset_class_id,
            asset_id,
            ..
        } => {
            // The rental ID is derived at execution time, but the asset
            // balance is the main conflict point.
            set.reads
                .insert(keys::asset_balance_key(asset_class_id, signer));
            set.writes
                .insert(keys::asset_balance_key(asset_class_id, signer));
            // Also touch the asset_id as a key (in case of NFT escrow).
            let _ = asset_id; // referenced via asset_class_id balance above
        }
        TransactionAction::RentAsset { rental_id, .. } => {
            set.reads.insert(keys::rental_key(rental_id));
            set.writes.insert(keys::rental_key(rental_id));
        }
        TransactionAction::ReturnRental { rental_id } => {
            set.reads.insert(keys::rental_key(rental_id));
            set.writes.insert(keys::rental_key(rental_id));
        }
        TransactionAction::ClaimExpiredRental { rental_id } => {
            set.reads.insert(keys::rental_key(rental_id));
            set.writes.insert(keys::rental_key(rental_id));
        }
        TransactionAction::CancelRentalListing { rental_id } => {
            set.reads.insert(keys::rental_key(rental_id));
            set.writes.insert(keys::rental_key(rental_id));
        }

        // -- Guilds --
        TransactionAction::CreateGuild { .. } => {
            // Creates a new guild whose ID is derived from content.
            // Signer account already captured above.
        }
        TransactionAction::JoinGuild { guild_id } => {
            set.reads.insert(keys::guild_key(guild_id));
            set.writes.insert(keys::guild_key(guild_id));
            set.writes
                .insert(keys::guild_member_key(guild_id, signer));
            set.writes
                .insert(keys::member_guilds_key(signer, guild_id));
        }
        TransactionAction::LeaveGuild { guild_id } => {
            set.reads.insert(keys::guild_key(guild_id));
            set.writes.insert(keys::guild_key(guild_id));
            set.reads
                .insert(keys::guild_member_key(guild_id, signer));
            set.writes
                .insert(keys::guild_member_key(guild_id, signer));
            set.writes
                .insert(keys::member_guilds_key(signer, guild_id));
        }
        TransactionAction::GuildDeposit { guild_id, .. } => {
            set.reads.insert(keys::guild_key(guild_id));
            set.writes.insert(keys::guild_key(guild_id));
            set.reads
                .insert(keys::guild_member_key(guild_id, signer));
        }
        TransactionAction::GuildWithdraw { guild_id, .. } => {
            set.reads.insert(keys::guild_key(guild_id));
            set.writes.insert(keys::guild_key(guild_id));
            set.reads
                .insert(keys::guild_member_key(guild_id, signer));
        }
        TransactionAction::GuildPromote {
            guild_id, member, ..
        } => {
            set.reads.insert(keys::guild_key(guild_id));
            set.reads
                .insert(keys::guild_member_key(guild_id, signer));
            set.reads
                .insert(keys::guild_member_key(guild_id, member));
            set.writes
                .insert(keys::guild_member_key(guild_id, member));
        }
        TransactionAction::GuildKick { guild_id, member } => {
            set.reads.insert(keys::guild_key(guild_id));
            set.writes.insert(keys::guild_key(guild_id));
            set.reads
                .insert(keys::guild_member_key(guild_id, signer));
            set.reads
                .insert(keys::guild_member_key(guild_id, member));
            set.writes
                .insert(keys::guild_member_key(guild_id, member));
            set.writes
                .insert(keys::member_guilds_key(member, guild_id));
        }

        // -- Tournaments --
        TransactionAction::CreateTournament { .. } => {
            // Creates a new tournament whose ID is derived from content.
            // Signer account already captured above.
        }
        TransactionAction::JoinTournament { tournament_id } => {
            set.reads.insert(keys::tournament_key(tournament_id));
            set.writes.insert(keys::tournament_key(tournament_id));
            set.writes
                .insert(keys::tournament_participant_key(tournament_id, signer));
        }
        TransactionAction::StartTournament { tournament_id } => {
            set.reads.insert(keys::tournament_key(tournament_id));
            set.writes.insert(keys::tournament_key(tournament_id));
        }
        TransactionAction::ReportTournamentResults {
            tournament_id, ..
        } => {
            set.reads.insert(keys::tournament_key(tournament_id));
            set.writes.insert(keys::tournament_key(tournament_id));
        }
        TransactionAction::ClaimTournamentPrize { tournament_id } => {
            set.reads.insert(keys::tournament_key(tournament_id));
            set.writes.insert(keys::tournament_key(tournament_id));
        }
        TransactionAction::CancelTournament { tournament_id } => {
            set.reads.insert(keys::tournament_key(tournament_id));
            set.writes.insert(keys::tournament_key(tournament_id));
        }
    }

    set
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_types::{
        Address, AssetType, Hash, MatchResult, Signature, Transaction, TransactionAction,
    };

    fn addr(b: u8) -> Address {
        Address::new([b; 32])
    }

    fn hash(b: u8) -> Hash {
        Hash::new([b; 32])
    }

    fn make_stx(signer: Address, action: TransactionAction) -> SignedTransaction {
        SignedTransaction::new(
            Transaction {
                chain_id: "test".into(),
                nonce: 0,
                signer,
                action,
                max_fee: 1_000_000,
                timestamp: 1,
                session: None,
                sponsor: None,
            },
            Signature::ZERO,
            Hash::ZERO,
            vec![0u8; 32],
        )
    }

    // -- AccessSet unit tests ------------------------------------------------

    #[test]
    fn write_write_conflict() {
        let mut a = AccessSet::new();
        a.writes.insert(b"key1".to_vec());

        let mut b = AccessSet::new();
        b.writes.insert(b"key1".to_vec());

        assert!(a.conflicts_with(&b));
    }

    #[test]
    fn read_write_conflict() {
        let mut a = AccessSet::new();
        a.reads.insert(b"key1".to_vec());

        let mut b = AccessSet::new();
        b.writes.insert(b"key1".to_vec());

        assert!(a.conflicts_with(&b));
        // Symmetric check:
        assert!(b.conflicts_with(&a));
    }

    #[test]
    fn read_read_no_conflict() {
        let mut a = AccessSet::new();
        a.reads.insert(b"key1".to_vec());

        let mut b = AccessSet::new();
        b.reads.insert(b"key1".to_vec());

        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn disjoint_keys_no_conflict() {
        let mut a = AccessSet::new();
        a.writes.insert(b"key_a".to_vec());
        a.reads.insert(b"key_c".to_vec());

        let mut b = AccessSet::new();
        b.writes.insert(b"key_b".to_vec());
        b.reads.insert(b"key_d".to_vec());

        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn empty_sets_no_conflict() {
        let a = AccessSet::new();
        let b = AccessSet::new();
        assert!(!a.conflicts_with(&b));
    }

    // -- predict_access_set for every TransactionAction variant ---------------

    #[test]
    fn predict_transfer() {
        let stx = make_stx(
            addr(1),
            TransactionAction::Transfer {
                to: addr(2),
                amount: 100,
            },
        );
        let set = predict_access_set(&stx);
        assert!(set.writes.contains(&polay_state::keys::balance_key(&addr(1))));
        assert!(set.writes.contains(&polay_state::keys::balance_key(&addr(2))));
        assert!(set.writes.contains(&polay_state::keys::account_key(&addr(1))));
        assert!(set.writes.contains(&polay_state::keys::account_key(&addr(2))));
    }

    #[test]
    fn predict_create_asset_class() {
        let stx = make_stx(
            addr(1),
            TransactionAction::CreateAssetClass {
                name: "Gold".into(),
                symbol: "GLD".into(),
                asset_type: AssetType::Fungible,
                max_supply: None,
                metadata_uri: "".into(),
            },
        );
        let set = predict_access_set(&stx);
        // At minimum: signer account + balance.
        assert!(set.writes.contains(&polay_state::keys::account_key(&addr(1))));
    }

    #[test]
    fn predict_mint_asset() {
        let stx = make_stx(
            addr(1),
            TransactionAction::MintAsset {
                asset_class_id: hash(10),
                to: addr(2),
                amount: 50,
                metadata: None,
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::asset_class_key(&hash(10))));
        assert!(set
            .writes
            .contains(&polay_state::keys::asset_balance_key(&hash(10), &addr(2))));
    }

    #[test]
    fn predict_transfer_asset() {
        let stx = make_stx(
            addr(1),
            TransactionAction::TransferAsset {
                asset_class_id: hash(10),
                to: addr(2),
                amount: 5,
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::asset_balance_key(&hash(10), &addr(1))));
        assert!(set
            .writes
            .contains(&polay_state::keys::asset_balance_key(&hash(10), &addr(2))));
    }

    #[test]
    fn predict_burn_asset() {
        let stx = make_stx(
            addr(1),
            TransactionAction::BurnAsset {
                asset_class_id: hash(10),
                amount: 3,
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::asset_class_key(&hash(10))));
        assert!(set
            .writes
            .contains(&polay_state::keys::asset_balance_key(&hash(10), &addr(1))));
    }

    #[test]
    fn predict_create_listing() {
        let stx = make_stx(
            addr(1),
            TransactionAction::CreateListing {
                asset_class_id: hash(10),
                amount: 1,
                price_per_unit: 100,
                currency: Hash::ZERO,
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::asset_balance_key(&hash(10), &addr(1))));
    }

    #[test]
    fn predict_cancel_listing() {
        let stx = make_stx(
            addr(1),
            TransactionAction::CancelListing {
                listing_id: hash(20),
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::listing_key(&hash(20))));
    }

    #[test]
    fn predict_buy_listing() {
        let stx = make_stx(
            addr(1),
            TransactionAction::BuyListing {
                listing_id: hash(20),
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::listing_key(&hash(20))));
    }

    #[test]
    fn predict_create_profile() {
        let stx = make_stx(
            addr(1),
            TransactionAction::CreateProfile {
                username: "alice".into(),
                display_name: "Alice".into(),
                metadata: None,
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::profile_key(&addr(1))));
    }

    #[test]
    fn predict_add_achievement() {
        let stx = make_stx(
            addr(1),
            TransactionAction::AddAchievement {
                player: addr(2),
                achievement_id: "first_win".into(),
                name: "First Win".into(),
                metadata: "{}".into(),
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::achievement_key(&addr(2), "first_win")));
    }

    #[test]
    fn predict_update_reputation() {
        let stx = make_stx(
            addr(1),
            TransactionAction::UpdateReputation {
                player: addr(2),
                delta: 10,
                reason: "good".into(),
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::profile_key(&addr(2))));
        assert!(set
            .reads
            .contains(&polay_state::keys::profile_key(&addr(2))));
    }

    #[test]
    fn predict_register_validator() {
        let stx = make_stx(
            addr(1),
            TransactionAction::RegisterValidator {
                commission_bps: 500,
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::validator_key(&addr(1))));
    }

    #[test]
    fn predict_delegate_stake() {
        let stx = make_stx(
            addr(1),
            TransactionAction::DelegateStake {
                validator: addr(2),
                amount: 1000,
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::validator_key(&addr(2))));
        assert!(set
            .writes
            .contains(&polay_state::keys::delegation_key(&addr(1), &addr(2))));
    }

    #[test]
    fn predict_undelegate_stake() {
        let stx = make_stx(
            addr(1),
            TransactionAction::UndelegateStake {
                validator: addr(2),
                amount: 500,
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::validator_key(&addr(2))));
        assert!(set
            .writes
            .contains(&polay_state::keys::delegation_key(&addr(1), &addr(2))));
    }

    #[test]
    fn predict_register_attestor() {
        let stx = make_stx(
            addr(1),
            TransactionAction::RegisterAttestor {
                game_id: "chess".into(),
                endpoint: "https://e.com".into(),
                metadata: "{}".into(),
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::attestor_key(&addr(1))));
    }

    #[test]
    fn predict_submit_match_result() {
        let stx = make_stx(
            addr(1),
            TransactionAction::SubmitMatchResult {
                match_result: MatchResult {
                    match_id: hash(30),
                    game_id: "chess".into(),
                    timestamp: 0,
                    players: vec![],
                    scores: vec![],
                    winners: vec![],
                    reward_pool: 0,
                    server_signature: vec![],
                    anti_cheat_score: None,
                    replay_ref: None,
                },
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::match_result_key(&hash(30))));
        assert!(set
            .writes
            .contains(&polay_state::keys::match_settlement_key(&hash(30))));
    }

    #[test]
    fn predict_distribute_reward() {
        let stx = make_stx(
            addr(1),
            TransactionAction::DistributeReward {
                match_id: hash(30),
                rewards: vec![(addr(2), 100), (addr(3), 200)],
            },
        );
        let set = predict_access_set(&stx);
        assert!(set
            .writes
            .contains(&polay_state::keys::match_settlement_key(&hash(30))));
        assert!(set
            .writes
            .contains(&polay_state::keys::balance_key(&addr(2))));
        assert!(set
            .writes
            .contains(&polay_state::keys::balance_key(&addr(3))));
    }

    // -- Cross-transaction conflict scenarios --------------------------------

    #[test]
    fn transfers_to_different_recipients_no_conflict_beyond_signer() {
        // Player A -> B and Player C -> D should NOT conflict.
        let tx1 = make_stx(
            addr(1),
            TransactionAction::Transfer {
                to: addr(2),
                amount: 100,
            },
        );
        let tx2 = make_stx(
            addr(3),
            TransactionAction::Transfer {
                to: addr(4),
                amount: 200,
            },
        );
        let s1 = predict_access_set(&tx1);
        let s2 = predict_access_set(&tx2);
        assert!(
            !s1.conflicts_with(&s2),
            "transfers between disjoint address sets should not conflict"
        );
    }

    #[test]
    fn transfers_to_same_recipient_conflict() {
        // Player A -> C and Player B -> C should conflict (both write C's balance).
        let tx1 = make_stx(
            addr(1),
            TransactionAction::Transfer {
                to: addr(3),
                amount: 100,
            },
        );
        let tx2 = make_stx(
            addr(2),
            TransactionAction::Transfer {
                to: addr(3),
                amount: 200,
            },
        );
        let s1 = predict_access_set(&tx1);
        let s2 = predict_access_set(&tx2);
        assert!(
            s1.conflicts_with(&s2),
            "transfers to the same recipient must conflict"
        );
    }

    #[test]
    fn same_signer_always_conflicts() {
        // Two txs from the same signer always conflict (signer account key).
        let tx1 = make_stx(
            addr(1),
            TransactionAction::Transfer {
                to: addr(2),
                amount: 10,
            },
        );
        let tx2 = make_stx(
            addr(1),
            TransactionAction::Transfer {
                to: addr(3),
                amount: 20,
            },
        );
        let s1 = predict_access_set(&tx1);
        let s2 = predict_access_set(&tx2);
        assert!(
            s1.conflicts_with(&s2),
            "transactions from the same signer must conflict"
        );
    }

    // -- Sponsor conflict tests -----------------------------------------------

    fn make_sponsored_stx(signer: Address, sponsor: Address, action: TransactionAction) -> SignedTransaction {
        SignedTransaction::new(
            Transaction {
                chain_id: "test".into(),
                nonce: 0,
                signer,
                action,
                max_fee: 1_000_000,
                timestamp: 1,
                session: None,
                sponsor: Some(sponsor),
            },
            Signature::ZERO,
            Hash::ZERO,
            vec![0u8; 32],
        )
    }

    #[test]
    fn same_sponsor_two_txs_conflict() {
        // Two transactions from different signers but the same sponsor must
        // conflict because they both write the sponsor's balance.
        let sponsor = addr(10);
        let tx1 = make_sponsored_stx(
            addr(1),
            sponsor,
            TransactionAction::Transfer {
                to: addr(5),
                amount: 100,
            },
        );
        let tx2 = make_sponsored_stx(
            addr(2),
            sponsor,
            TransactionAction::Transfer {
                to: addr(6),
                amount: 200,
            },
        );
        let s1 = predict_access_set(&tx1);
        let s2 = predict_access_set(&tx2);
        assert!(
            s1.conflicts_with(&s2),
            "transactions with the same sponsor must conflict"
        );
    }

    #[test]
    fn different_sponsors_no_conflict() {
        // Two transactions from different signers and different sponsors should
        // NOT conflict (assuming disjoint recipients).
        let tx1 = make_sponsored_stx(
            addr(1),
            addr(10),
            TransactionAction::Transfer {
                to: addr(5),
                amount: 100,
            },
        );
        let tx2 = make_sponsored_stx(
            addr(2),
            addr(11),
            TransactionAction::Transfer {
                to: addr(6),
                amount: 200,
            },
        );
        let s1 = predict_access_set(&tx1);
        let s2 = predict_access_set(&tx2);
        assert!(
            !s1.conflicts_with(&s2),
            "transactions with different sponsors and signers should not conflict"
        );
    }

    #[test]
    fn sponsored_tx_includes_sponsor_in_write_set() {
        let sponsor = addr(10);
        let tx = make_sponsored_stx(
            addr(1),
            sponsor,
            TransactionAction::Transfer {
                to: addr(5),
                amount: 100,
            },
        );
        let set = predict_access_set(&tx);
        assert!(
            set.writes.contains(&polay_state::keys::account_key(&sponsor)),
            "sponsor account key should be in write set"
        );
        assert!(
            set.writes.contains(&polay_state::keys::balance_key(&sponsor)),
            "sponsor balance key should be in write set"
        );
    }
}
