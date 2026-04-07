use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::hash::Hash;

/// A structured event emitted during transaction execution.
///
/// Events are stored in transaction receipts and can be queried by module,
/// action, or individual attribute key/value pairs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Event {
    /// The module that emitted this event (e.g., "asset", "market", "staking").
    pub module: String,
    /// The action that triggered this event (e.g., "transfer", "mint", "slash").
    pub action: String,
    /// Free-form key-value attributes providing details.
    pub attributes: Vec<(String, String)>,
}

impl Event {
    /// Create a new event with the given module, action, and attributes.
    pub fn new(
        module: impl Into<String>,
        action: impl Into<String>,
        attributes: Vec<(String, String)>,
    ) -> Self {
        Self {
            module: module.into(),
            action: action.into(),
            attributes,
        }
    }

    /// Look up the first attribute value matching `key`.
    pub fn get_attribute(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    // -----------------------------------------------------------------------
    // Helper constructors for common event types
    // -----------------------------------------------------------------------

    /// Native-token transfer event.
    pub fn transfer(from: &Address, to: &Address, amount: u64) -> Self {
        Self::new(
            "bank",
            "transfer",
            vec![
                ("from".into(), from.to_hex()),
                ("to".into(), to.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Asset mint event.
    pub fn mint_asset(asset_class_id: &Hash, to: &Address, amount: u64) -> Self {
        Self::new(
            "asset",
            "mint",
            vec![
                ("asset_class_id".into(), asset_class_id.to_hex()),
                ("to".into(), to.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Asset transfer event.
    pub fn transfer_asset(
        asset_class_id: &Hash,
        from: &Address,
        to: &Address,
        amount: u64,
    ) -> Self {
        Self::new(
            "asset",
            "transfer",
            vec![
                ("asset_class_id".into(), asset_class_id.to_hex()),
                ("from".into(), from.to_hex()),
                ("to".into(), to.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Asset burn event.
    pub fn burn_asset(asset_class_id: &Hash, from: &Address, amount: u64) -> Self {
        Self::new(
            "asset",
            "burn",
            vec![
                ("asset_class_id".into(), asset_class_id.to_hex()),
                ("from".into(), from.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Asset class creation event.
    pub fn asset_class_created(asset_class_id: &Hash, creator: &Address, name: &str) -> Self {
        Self::new(
            "asset",
            "create_class",
            vec![
                ("asset_class_id".into(), asset_class_id.to_hex()),
                ("creator".into(), creator.to_hex()),
                ("name".into(), name.to_string()),
            ],
        )
    }

    /// Marketplace listing created.
    pub fn listing_created(
        listing_id: &Hash,
        seller: &Address,
        asset_class_id: &Hash,
        amount: u64,
        price_per_unit: u64,
    ) -> Self {
        Self::new(
            "market",
            "listing_created",
            vec![
                ("listing_id".into(), listing_id.to_hex()),
                ("seller".into(), seller.to_hex()),
                ("asset_class_id".into(), asset_class_id.to_hex()),
                ("amount".into(), amount.to_string()),
                ("price_per_unit".into(), price_per_unit.to_string()),
            ],
        )
    }

    /// Marketplace listing sold.
    pub fn listing_sold(listing_id: &Hash, buyer: &Address, seller: &Address) -> Self {
        Self::new(
            "market",
            "listing_sold",
            vec![
                ("listing_id".into(), listing_id.to_hex()),
                ("buyer".into(), buyer.to_hex()),
                ("seller".into(), seller.to_hex()),
            ],
        )
    }

    /// Marketplace listing cancelled.
    pub fn listing_cancelled(listing_id: &Hash, seller: &Address) -> Self {
        Self::new(
            "market",
            "listing_cancelled",
            vec![
                ("listing_id".into(), listing_id.to_hex()),
                ("seller".into(), seller.to_hex()),
            ],
        )
    }

    /// Player profile created.
    pub fn profile_created(address: &Address, username: &str) -> Self {
        Self::new(
            "identity",
            "profile_created",
            vec![
                ("address".into(), address.to_hex()),
                ("username".into(), username.to_string()),
            ],
        )
    }

    /// Achievement awarded.
    pub fn achievement_awarded(player: &Address, achievement_id: &str, name: &str) -> Self {
        Self::new(
            "identity",
            "achievement_awarded",
            vec![
                ("player".into(), player.to_hex()),
                ("achievement_id".into(), achievement_id.to_string()),
                ("name".into(), name.to_string()),
            ],
        )
    }

    /// Reputation changed.
    pub fn reputation_changed(player: &Address, delta: i64, new_value: i64) -> Self {
        Self::new(
            "identity",
            "reputation_changed",
            vec![
                ("player".into(), player.to_hex()),
                ("delta".into(), delta.to_string()),
                ("new_value".into(), new_value.to_string()),
            ],
        )
    }

    /// Validator registered.
    pub fn validator_registered(address: &Address, commission_bps: u16) -> Self {
        Self::new(
            "staking",
            "validator_registered",
            vec![
                ("address".into(), address.to_hex()),
                ("commission_bps".into(), commission_bps.to_string()),
            ],
        )
    }

    /// Stake delegated.
    pub fn stake_delegated(delegator: &Address, validator: &Address, amount: u64) -> Self {
        Self::new(
            "staking",
            "delegate",
            vec![
                ("delegator".into(), delegator.to_hex()),
                ("validator".into(), validator.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Stake undelegated.
    pub fn stake_undelegated(delegator: &Address, validator: &Address, amount: u64) -> Self {
        Self::new(
            "staking",
            "undelegate",
            vec![
                ("delegator".into(), delegator.to_hex()),
                ("validator".into(), validator.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Unbonding initiated.
    pub fn unbonding_initiated(
        delegator: &Address,
        validator: &Address,
        amount: u64,
        completion_height: u64,
    ) -> Self {
        Self::new(
            "staking",
            "unbonding_initiated",
            vec![
                ("delegator".into(), delegator.to_hex()),
                ("validator".into(), validator.to_hex()),
                ("amount".into(), amount.to_string()),
                ("completion_height".into(), completion_height.to_string()),
            ],
        )
    }

    /// Unbonding completed (funds released).
    pub fn unbonding_completed(delegator: &Address, validator: &Address, amount: u64) -> Self {
        Self::new(
            "staking",
            "unbonding_completed",
            vec![
                ("delegator".into(), delegator.to_hex()),
                ("validator".into(), validator.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Validator slashed.
    pub fn validator_slashed(validator: &Address, amount: u64, reason: &str) -> Self {
        Self::new(
            "staking",
            "slash",
            vec![
                ("validator".into(), validator.to_hex()),
                ("amount".into(), amount.to_string()),
                ("reason".into(), reason.to_string()),
            ],
        )
    }

    /// Match result submitted.
    pub fn match_result_submitted(match_id: &Hash, game_id: &str) -> Self {
        Self::new(
            "attestation",
            "match_result_submitted",
            vec![
                ("match_id".into(), match_id.to_hex()),
                ("game_id".into(), game_id.to_string()),
            ],
        )
    }

    /// Rewards distributed for a match.
    pub fn rewards_distributed(match_id: &Hash, total: u64) -> Self {
        Self::new(
            "attestation",
            "rewards_distributed",
            vec![
                ("match_id".into(), match_id.to_hex()),
                ("total".into(), total.to_string()),
            ],
        )
    }

    // -- Epoch events --------------------------------------------------------

    /// Epoch transition completed.
    pub fn epoch_transition(
        epoch: u64,
        validator_count: usize,
        total_staked: u64,
        rewards: u64,
    ) -> Self {
        Self::new(
            "epoch",
            "epoch_transition",
            vec![
                ("epoch".into(), epoch.to_string()),
                ("validator_count".into(), validator_count.to_string()),
                ("total_staked".into(), total_staked.to_string()),
                ("rewards_distributed".into(), rewards.to_string()),
            ],
        )
    }

    /// Validator unjailed at epoch boundary.
    pub fn validator_unjailed(address: &Address) -> Self {
        Self::new(
            "epoch",
            "validator_unjailed",
            vec![("address".into(), address.to_hex())],
        )
    }

    /// Validator set updated at epoch boundary.
    pub fn validator_set_updated(epoch: u64, count: usize) -> Self {
        Self::new(
            "epoch",
            "validator_set_updated",
            vec![
                ("epoch".into(), epoch.to_string()),
                ("validator_count".into(), count.to_string()),
            ],
        )
    }

    // -- Gas sponsorship events ------------------------------------------------

    /// Gas fee sponsored by a third party.
    pub fn gas_sponsored(sponsor: &Address, signer: &Address, amount: u64) -> Self {
        Self::new(
            "bank",
            "gas_sponsored",
            vec![
                ("sponsor".into(), sponsor.to_hex()),
                ("signer".into(), signer.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    // -- Session key events ---------------------------------------------------

    /// Session key created.
    pub fn session_created(granter: &Address, session_address: &Address, expires_at: u64) -> Self {
        Self::new(
            "session",
            "session_created",
            vec![
                ("granter".into(), granter.to_hex()),
                ("session_address".into(), session_address.to_hex()),
                ("expires_at".into(), expires_at.to_string()),
            ],
        )
    }

    /// Session key revoked.
    pub fn session_revoked(granter: &Address, session_address: &Address) -> Self {
        Self::new(
            "session",
            "session_revoked",
            vec![
                ("granter".into(), granter.to_hex()),
                ("session_address".into(), session_address.to_hex()),
            ],
        )
    }

    // -- Rental events -------------------------------------------------------

    /// Rental listing created.
    pub fn rental_listed(owner: &Address, rental_id: &Hash, asset_id: &Hash) -> Self {
        Self::new(
            "rental",
            "rental_listed",
            vec![
                ("owner".into(), owner.to_hex()),
                ("rental_id".into(), rental_id.to_hex()),
                ("asset_id".into(), asset_id.to_hex()),
            ],
        )
    }

    /// Asset rented.
    pub fn asset_rented(renter: &Address, rental_id: &Hash, duration: u64) -> Self {
        Self::new(
            "rental",
            "asset_rented",
            vec![
                ("renter".into(), renter.to_hex()),
                ("rental_id".into(), rental_id.to_hex()),
                ("duration".into(), duration.to_string()),
            ],
        )
    }

    /// Rental returned by the renter.
    pub fn rental_returned(renter: &Address, rental_id: &Hash) -> Self {
        Self::new(
            "rental",
            "rental_returned",
            vec![
                ("renter".into(), renter.to_hex()),
                ("rental_id".into(), rental_id.to_hex()),
            ],
        )
    }

    /// Rental expired.
    pub fn rental_expired(rental_id: &Hash) -> Self {
        Self::new(
            "rental",
            "rental_expired",
            vec![("rental_id".into(), rental_id.to_hex())],
        )
    }

    /// Rental listing cancelled.
    pub fn rental_cancelled(owner: &Address, rental_id: &Hash) -> Self {
        Self::new(
            "rental",
            "rental_cancelled",
            vec![
                ("owner".into(), owner.to_hex()),
                ("rental_id".into(), rental_id.to_hex()),
            ],
        )
    }

    // -- Guild events --------------------------------------------------------

    /// Guild created.
    pub fn guild_created(leader: &Address, guild_id: &Hash, name: &str) -> Self {
        Self::new(
            "guild",
            "guild_created",
            vec![
                ("leader".into(), leader.to_hex()),
                ("guild_id".into(), guild_id.to_hex()),
                ("name".into(), name.to_string()),
            ],
        )
    }

    /// Member joined a guild.
    pub fn guild_joined(member: &Address, guild_id: &Hash) -> Self {
        Self::new(
            "guild",
            "guild_joined",
            vec![
                ("member".into(), member.to_hex()),
                ("guild_id".into(), guild_id.to_hex()),
            ],
        )
    }

    /// Member left a guild.
    pub fn guild_left(member: &Address, guild_id: &Hash) -> Self {
        Self::new(
            "guild",
            "guild_left",
            vec![
                ("member".into(), member.to_hex()),
                ("guild_id".into(), guild_id.to_hex()),
            ],
        )
    }

    /// Deposit to guild treasury.
    pub fn guild_deposit(member: &Address, guild_id: &Hash, amount: u64) -> Self {
        Self::new(
            "guild",
            "guild_deposit",
            vec![
                ("member".into(), member.to_hex()),
                ("guild_id".into(), guild_id.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Withdrawal from guild treasury.
    pub fn guild_withdrawal(member: &Address, guild_id: &Hash, amount: u64) -> Self {
        Self::new(
            "guild",
            "guild_withdrawal",
            vec![
                ("member".into(), member.to_hex()),
                ("guild_id".into(), guild_id.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Guild member promoted.
    pub fn guild_member_promoted(member: &Address, guild_id: &Hash, role: &str) -> Self {
        Self::new(
            "guild",
            "guild_member_promoted",
            vec![
                ("member".into(), member.to_hex()),
                ("guild_id".into(), guild_id.to_hex()),
                ("role".into(), role.to_string()),
            ],
        )
    }

    /// Guild member kicked.
    pub fn guild_member_kicked(member: &Address, guild_id: &Hash) -> Self {
        Self::new(
            "guild",
            "guild_member_kicked",
            vec![
                ("member".into(), member.to_hex()),
                ("guild_id".into(), guild_id.to_hex()),
            ],
        )
    }

    // -- Tournament events ---------------------------------------------------

    /// Tournament created.
    pub fn tournament_created(organizer: &Address, tournament_id: &Hash, name: &str) -> Self {
        Self::new(
            "tournament",
            "tournament_created",
            vec![
                ("organizer".into(), organizer.to_hex()),
                ("tournament_id".into(), tournament_id.to_hex()),
                ("name".into(), name.to_string()),
            ],
        )
    }

    /// Participant joined a tournament.
    pub fn tournament_joined(participant: &Address, tournament_id: &Hash) -> Self {
        Self::new(
            "tournament",
            "tournament_joined",
            vec![
                ("participant".into(), participant.to_hex()),
                ("tournament_id".into(), tournament_id.to_hex()),
            ],
        )
    }

    /// Tournament started.
    pub fn tournament_started(tournament_id: &Hash, participants_count: u32) -> Self {
        Self::new(
            "tournament",
            "tournament_started",
            vec![
                ("tournament_id".into(), tournament_id.to_hex()),
                ("participants_count".into(), participants_count.to_string()),
            ],
        )
    }

    /// Tournament results reported.
    pub fn tournament_results_reported(tournament_id: &Hash, winner: &Address) -> Self {
        Self::new(
            "tournament",
            "tournament_results_reported",
            vec![
                ("tournament_id".into(), tournament_id.to_hex()),
                ("winner".into(), winner.to_hex()),
            ],
        )
    }

    /// Tournament prize claimed.
    pub fn tournament_prize_claimed(
        participant: &Address,
        tournament_id: &Hash,
        amount: u64,
    ) -> Self {
        Self::new(
            "tournament",
            "tournament_prize_claimed",
            vec![
                ("participant".into(), participant.to_hex()),
                ("tournament_id".into(), tournament_id.to_hex()),
                ("amount".into(), amount.to_string()),
            ],
        )
    }

    /// Tournament cancelled.
    pub fn tournament_cancelled(tournament_id: &Hash, refunded_count: u32) -> Self {
        Self::new(
            "tournament",
            "tournament_cancelled",
            vec![
                ("tournament_id".into(), tournament_id.to_hex()),
                ("refunded_count".into(), refunded_count.to_string()),
            ],
        )
    }

    // -- Governance events ---------------------------------------------------

    /// Governance proposal submitted.
    pub fn proposal_submitted(proposal_id: &Hash, proposer: &Address, title: &str) -> Self {
        Self::new(
            "governance",
            "proposal_submitted",
            vec![
                ("proposal_id".into(), proposal_id.to_hex()),
                ("proposer".into(), proposer.to_hex()),
                ("title".into(), title.to_string()),
            ],
        )
    }

    /// Vote cast on a governance proposal.
    pub fn vote_cast(
        proposal_id: &Hash,
        voter: &Address,
        option: &str,
        weight: u64,
    ) -> Self {
        Self::new(
            "governance",
            "vote_cast",
            vec![
                ("proposal_id".into(), proposal_id.to_hex()),
                ("voter".into(), voter.to_hex()),
                ("option".into(), option.to_string()),
                ("weight".into(), weight.to_string()),
            ],
        )
    }

    /// Governance proposal passed.
    pub fn proposal_passed(proposal_id: &Hash) -> Self {
        Self::new(
            "governance",
            "proposal_passed",
            vec![("proposal_id".into(), proposal_id.to_hex())],
        )
    }

    /// Governance proposal rejected.
    pub fn proposal_rejected(proposal_id: &Hash) -> Self {
        Self::new(
            "governance",
            "proposal_rejected",
            vec![("proposal_id".into(), proposal_id.to_hex())],
        )
    }

    /// Governance proposal executed.
    pub fn proposal_executed(proposal_id: &Hash) -> Self {
        Self::new(
            "governance",
            "proposal_executed",
            vec![("proposal_id".into(), proposal_id.to_hex())],
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_attribute_found() {
        let evt = Event::transfer(&Address::ZERO, &Address::ZERO, 1000);
        assert_eq!(evt.get_attribute("amount"), Some("1000"));
    }

    #[test]
    fn get_attribute_missing() {
        let evt = Event::transfer(&Address::ZERO, &Address::ZERO, 1000);
        assert_eq!(evt.get_attribute("nonexistent"), None);
    }

    #[test]
    fn serde_round_trip() {
        let evt = Event::listing_created(
            &Hash::ZERO,
            &Address::ZERO,
            &Hash::ZERO,
            5,
            100,
        );
        let json = serde_json::to_string(&evt).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(evt, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let evt = Event::transfer(&Address::ZERO, &Address::ZERO, 42);
        let encoded = borsh::to_vec(&evt).unwrap();
        let decoded = Event::try_from_slice(&encoded).unwrap();
        assert_eq!(evt, decoded);
    }
}
