//! High-level state mutation methods.

use polay_types::{
    AccountState, Achievement, Address, AssetClass, Attestor, Block, Delegation, EpochInfo,
    EquivocationEvidence, Event, Guild, GuildMembership, Hash, Listing, MatchResult,
    MatchSettlement, PlayerProfile, Proposal, Rental, SessionGrant, SupplyInfo, Tournament,
    TransactionReceipt, TxLocation, UnbondingEntry, ValidatorInfo, Vote,
};

use crate::error::StateResult;
use crate::keys;
use crate::store::{store_put, StateStore};

/// Wraps a [`StateStore`] and provides typed set/store methods that mirror
/// the getters on [`crate::StateView`].
pub struct StateWriter<'a> {
    store: &'a dyn StateStore,
}

impl<'a> StateWriter<'a> {
    /// Create a new writer backed by the given store.
    pub fn new(store: &'a dyn StateStore) -> Self {
        Self { store }
    }

    // -- Accounts ------------------------------------------------------------

    /// Persist the full [`AccountState`].  The key is derived from
    /// `account.address`.
    pub fn set_account(&self, account: &AccountState) -> StateResult<()> {
        store_put(self.store, &keys::account_key(&account.address), account)
    }

    /// Set the native POL balance for `addr`.
    pub fn set_balance(&self, addr: &Address, balance: u64) -> StateResult<()> {
        store_put(self.store, &keys::balance_key(addr), &balance)
    }

    // -- Assets --------------------------------------------------------------

    /// Persist an [`AssetClass`].  The key is derived from `asset_class.id`.
    pub fn set_asset_class(&self, asset_class: &AssetClass) -> StateResult<()> {
        store_put(self.store, &keys::asset_class_key(&asset_class.id), asset_class)
    }

    /// Set the balance of a specific asset class held by `owner`.
    pub fn set_asset_balance(
        &self,
        asset_class_id: &Hash,
        owner: &Address,
        balance: u64,
    ) -> StateResult<()> {
        store_put(self.store, &keys::asset_balance_key(asset_class_id, owner), &balance)
    }

    // -- Validators / Staking ------------------------------------------------

    /// Persist [`ValidatorInfo`].  The key is derived from `info.address`.
    pub fn set_validator(&self, info: &ValidatorInfo) -> StateResult<()> {
        store_put(self.store, &keys::validator_key(&info.address), info)
    }

    /// Persist a [`Delegation`].  The key is derived from
    /// `delegation.delegator` and `delegation.validator`.
    pub fn set_delegation(&self, delegation: &Delegation) -> StateResult<()> {
        store_put(
            self.store,
            &keys::delegation_key(&delegation.delegator, &delegation.validator),
            delegation,
        )
    }

    // -- Unbonding queue ------------------------------------------------------

    /// Persist an [`UnbondingEntry`]. Also writes a global index entry sorted
    /// by completion_height for efficient maturation scanning.
    pub fn set_unbonding_entry(&self, entry: &UnbondingEntry, index: u8) -> StateResult<()> {
        let key = keys::unbonding_key(&entry.delegator, entry.completion_height, index);
        store_put(self.store, &key, entry)?;
        // Also write to the global index for maturation scanning.
        let index_key = keys::unbonding_index_key(entry.completion_height, &entry.delegator, index);
        store_put(self.store, &index_key, entry)
    }

    /// Delete an unbonding entry and its global index entry.
    pub fn delete_unbonding_entry(
        &self,
        delegator: &Address,
        completion_height: u64,
        index: u8,
    ) -> StateResult<()> {
        let key = keys::unbonding_key(delegator, completion_height, index);
        self.store.delete(&key)?;
        let index_key = keys::unbonding_index_key(completion_height, delegator, index);
        self.store.delete(&index_key)
    }

    // -- Marketplace ---------------------------------------------------------

    /// Persist a [`Listing`].  The key is derived from `listing.id`.
    pub fn set_listing(&self, listing: &Listing) -> StateResult<()> {
        store_put(self.store, &keys::listing_key(&listing.id), listing)
    }

    // -- Identity / Gaming ---------------------------------------------------

    /// Persist a [`PlayerProfile`].  The key is derived from `profile.address`.
    pub fn set_profile(&self, profile: &PlayerProfile) -> StateResult<()> {
        store_put(self.store, &keys::profile_key(&profile.address), profile)
    }

    /// Persist an [`Achievement`].  The key is derived from `achievement.player`
    /// and `achievement.id`.
    pub fn set_achievement(&self, achievement: &Achievement) -> StateResult<()> {
        store_put(
            self.store,
            &keys::achievement_key(&achievement.player, &achievement.id),
            achievement,
        )
    }

    // -- Attestation ---------------------------------------------------------

    /// Persist an [`Attestor`] registration.  The key is derived from
    /// `attestor.address`.
    pub fn set_attestor(&self, attestor: &Attestor) -> StateResult<()> {
        store_put(self.store, &keys::attestor_key(&attestor.address), attestor)
    }

    /// Persist a [`MatchResult`].  The key is derived from `result.match_id`.
    pub fn set_match_result(&self, result: &MatchResult) -> StateResult<()> {
        store_put(self.store, &keys::match_result_key(&result.match_id), result)
    }

    /// Persist a [`MatchSettlement`].  The key is derived from
    /// `settlement.match_id`.
    pub fn set_match_settlement(&self, settlement: &MatchSettlement) -> StateResult<()> {
        store_put(
            self.store,
            &keys::match_settlement_key(&settlement.match_id),
            settlement,
        )
    }

    // -- Governance -----------------------------------------------------------

    /// Persist a [`Proposal`]. The key is derived from `proposal.id`.
    pub fn set_proposal(&self, proposal: &Proposal) -> StateResult<()> {
        store_put(self.store, &keys::proposal_key(&proposal.id), proposal)
    }

    /// Persist a [`Vote`]. The key is derived from `vote.proposal_id` and
    /// `vote.voter`.
    pub fn set_vote(&self, vote: &Vote) -> StateResult<()> {
        store_put(
            self.store,
            &keys::vote_key(&vote.proposal_id, &vote.voter),
            vote,
        )
    }

    /// Persist the proposal list (Vec<Hash> of all proposal IDs).
    pub fn set_proposal_list(&self, list: &[Hash]) -> StateResult<()> {
        store_put(self.store, &keys::proposal_list_key(), &list.to_vec())
    }

    // -- Rentals ---------------------------------------------------------------

    /// Persist a [`Rental`] and update the owner index. If the rental has a
    /// renter, the renter index is also updated.
    pub fn set_rental(&self, rental: &Rental) -> StateResult<()> {
        store_put(self.store, &keys::rental_key(&rental.rental_id), rental)?;
        // Owner index entry (value is just a marker).
        store_put(
            self.store,
            &keys::rental_by_owner_key(&rental.owner, &rental.rental_id),
            &true,
        )?;
        // Renter index entry, if present.
        if let Some(renter) = &rental.renter {
            store_put(
                self.store,
                &keys::rental_by_renter_key(renter, &rental.rental_id),
                &true,
            )?;
        }
        Ok(())
    }

    /// Delete a [`Rental`] and remove it from all indexes.
    pub fn delete_rental(
        &self,
        rental_id: &Hash,
        owner: &Address,
        renter: Option<&Address>,
    ) -> StateResult<()> {
        self.store.delete(&keys::rental_key(rental_id))?;
        self.store
            .delete(&keys::rental_by_owner_key(owner, rental_id))?;
        if let Some(renter) = renter {
            self.store
                .delete(&keys::rental_by_renter_key(renter, rental_id))?;
        }
        Ok(())
    }

    // -- Guilds ----------------------------------------------------------------

    /// Persist a [`Guild`]. The key is derived from `guild.guild_id`.
    pub fn set_guild(&self, guild: &Guild) -> StateResult<()> {
        store_put(self.store, &keys::guild_key(&guild.guild_id), guild)
    }

    /// Delete a [`Guild`] by its id.
    pub fn delete_guild(&self, guild_id: &Hash) -> StateResult<()> {
        self.store.delete(&keys::guild_key(guild_id))
    }

    /// Persist a [`GuildMembership`] and update the member-guilds reverse index.
    pub fn set_guild_membership(&self, membership: &GuildMembership) -> StateResult<()> {
        store_put(
            self.store,
            &keys::guild_member_key(&membership.guild_id, &membership.member),
            membership,
        )?;
        // Reverse index: member -> guild (marker value).
        store_put(
            self.store,
            &keys::member_guilds_key(&membership.member, &membership.guild_id),
            &true,
        )
    }

    /// Delete a [`GuildMembership`] and remove the reverse index entry.
    pub fn delete_guild_membership(
        &self,
        guild_id: &Hash,
        member: &Address,
    ) -> StateResult<()> {
        self.store
            .delete(&keys::guild_member_key(guild_id, member))?;
        self.store
            .delete(&keys::member_guilds_key(member, guild_id))
    }

    // -- Tournaments -----------------------------------------------------------

    /// Persist a [`Tournament`]. The key is derived from `tournament.tournament_id`.
    pub fn set_tournament(&self, tournament: &Tournament) -> StateResult<()> {
        store_put(
            self.store,
            &keys::tournament_key(&tournament.tournament_id),
            tournament,
        )
    }

    /// Record a participant in a tournament (marker value).
    pub fn set_tournament_participant(
        &self,
        tournament_id: &Hash,
        participant: &Address,
    ) -> StateResult<()> {
        store_put(
            self.store,
            &keys::tournament_participant_key(tournament_id, participant),
            &true,
        )
    }

    /// Remove a participant record from a tournament.
    pub fn delete_tournament_participant(
        &self,
        tournament_id: &Hash,
        participant: &Address,
    ) -> StateResult<()> {
        self.store
            .delete(&keys::tournament_participant_key(tournament_id, participant))
    }

    // -- Session keys ---------------------------------------------------------

    /// Persist a [`SessionGrant`]. The key is derived from `grant.granter` and
    /// `grant.session_address`.
    pub fn set_session(&self, grant: &SessionGrant) -> StateResult<()> {
        store_put(
            self.store,
            &keys::session_key(&grant.granter, &grant.session_address),
            grant,
        )
    }

    // -- Receipts / Events ---------------------------------------------------

    /// Persist a [`TransactionReceipt`] keyed by its `tx_hash`.
    pub fn set_receipt(&self, receipt: &TransactionReceipt) -> StateResult<()> {
        store_put(self.store, &keys::receipt_key(&receipt.tx_hash), receipt)
    }

    /// Persist a [`TxLocation`] mapping a transaction hash to its block
    /// location.
    pub fn set_tx_location(&self, tx_hash: &Hash, location: &TxLocation) -> StateResult<()> {
        store_put(self.store, &keys::tx_block_index_key(tx_hash), location)
    }

    /// Persist block-level events for `height`.
    pub fn set_block_events(&self, height: u64, events: &[Event]) -> StateResult<()> {
        store_put(self.store, &keys::block_events_key(height), &events.to_vec())
    }

    // -- Epoch ---------------------------------------------------------------

    /// Persist an [`EpochInfo`] record. The key is derived from `info.epoch`.
    pub fn set_epoch_info(&self, info: &EpochInfo) -> StateResult<()> {
        store_put(self.store, &keys::epoch_key(info.epoch), info)
    }

    /// Set the current active validator set (list of addresses).
    pub fn set_active_validator_set(&self, validators: &[Address]) -> StateResult<()> {
        store_put(self.store, &keys::active_validator_set_key(), &validators.to_vec())
    }

    // -- Economics / supply tracking -----------------------------------------

    /// Persist the global [`SupplyInfo`].
    pub fn set_supply_info(&self, info: &SupplyInfo) -> StateResult<()> {
        store_put(self.store, &keys::supply_info_key(), info)
    }

    // -- Equivocation evidence -------------------------------------------------

    /// Persist equivocation evidence. The key is derived from the validator
    /// address and block height.
    pub fn set_equivocation_evidence(&self, evidence: &EquivocationEvidence) -> StateResult<()> {
        store_put(
            self.store,
            &keys::equivocation_key(&evidence.validator, evidence.height),
            evidence,
        )
    }

    // -- Chain metadata ------------------------------------------------------

    /// Set the current chain height.
    pub fn set_chain_height(&self, height: u64) -> StateResult<()> {
        store_put(self.store, &keys::chain_height_key(), &height)
    }

    /// Set the hash of the latest committed block.
    pub fn set_latest_hash(&self, hash: &Hash) -> StateResult<()> {
        store_put(self.store, &keys::latest_hash_key(), hash)
    }

    /// Store a full [`Block`] keyed by its height.
    pub fn store_block(&self, block: &Block) -> StateResult<()> {
        store_put(self.store, &keys::block_key(block.height()), block)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_view::StateView;
    use crate::store::MemoryStore;
    use polay_types::{
        attestation::AttestorStatus, market::ListingStatus,
    };

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn test_hash(byte: u8) -> Hash {
        Hash::new([byte; 32])
    }

    #[test]
    fn set_and_get_listing() {
        let store = MemoryStore::new();
        let listing = Listing {
            id: test_hash(0x01),
            seller: test_addr(0xAA),
            asset_class_id: test_hash(0x02),
            amount: 10,
            price_per_unit: 500,
            currency: Hash::ZERO,
            status: ListingStatus::Active,
            royalty_bps: 250,
            created_at: 5,
        };
        StateWriter::new(&store).set_listing(&listing).unwrap();
        let got = StateView::new(&store)
            .get_listing(&listing.id)
            .unwrap()
            .unwrap();
        assert_eq!(got, listing);
    }

    #[test]
    fn set_and_get_attestor() {
        let store = MemoryStore::new();
        let attestor = Attestor {
            address: test_addr(0xBB),
            game_id: "arena".into(),
            endpoint: "http://localhost:9090".into(),
            metadata: "{}".into(),
            status: AttestorStatus::Active,
            registered_at: 1,
        };
        StateWriter::new(&store).set_attestor(&attestor).unwrap();
        let got = StateView::new(&store)
            .get_attestor(&attestor.address)
            .unwrap()
            .unwrap();
        assert_eq!(got, attestor);
    }

    #[test]
    fn set_and_get_match_result() {
        let store = MemoryStore::new();
        let result = MatchResult {
            match_id: test_hash(0x10),
            game_id: "card-clash".into(),
            timestamp: 1700000000,
            players: vec![test_addr(1), test_addr(2)],
            scores: vec![1500, 1200],
            winners: vec![test_addr(1)],
            reward_pool: 10_000,
            server_signature: vec![0xAB; 64],
            anti_cheat_score: Some(95),
            replay_ref: None,
        };
        StateWriter::new(&store).set_match_result(&result).unwrap();
        let got = StateView::new(&store)
            .get_match_result(&result.match_id)
            .unwrap()
            .unwrap();
        assert_eq!(got, result);
    }

    #[test]
    fn set_and_get_match_settlement() {
        let store = MemoryStore::new();
        let settlement = MatchSettlement {
            match_id: test_hash(0x10),
            settled: true,
            rewards_distributed: vec![(test_addr(1), 1000), (test_addr(2), 200)],
            quarantined: false,
            settled_at: 101,
        };
        StateWriter::new(&store)
            .set_match_settlement(&settlement)
            .unwrap();
        let got = StateView::new(&store)
            .get_match_settlement(&settlement.match_id)
            .unwrap()
            .unwrap();
        assert_eq!(got, settlement);
    }

    #[test]
    fn set_and_get_asset_class_and_balance() {
        let store = MemoryStore::new();
        let ac = AssetClass {
            id: test_hash(0x55),
            name: "Gold Sword".into(),
            symbol: "GSWD".into(),
            asset_type: polay_types::asset::AssetType::Fungible,
            total_supply: 1000,
            max_supply: Some(10_000),
            creator: test_addr(0xAA),
            metadata_uri: "https://example.com/meta.json".into(),
            created_at: 1700000000,
        };
        let owner = test_addr(0xBB);
        let writer = StateWriter::new(&store);
        writer.set_asset_class(&ac).unwrap();
        writer.set_asset_balance(&ac.id, &owner, 50).unwrap();

        let view = StateView::new(&store);
        assert_eq!(view.get_asset_class(&ac.id).unwrap().unwrap(), ac);
        assert_eq!(view.get_asset_balance(&ac.id, &owner).unwrap(), 50);
        // Unknown owner has zero balance.
        assert_eq!(
            view.get_asset_balance(&ac.id, &test_addr(0xFF)).unwrap(),
            0
        );
    }

    #[test]
    fn overwrite_balance() {
        let store = MemoryStore::new();
        let addr = test_addr(1);
        let writer = StateWriter::new(&store);
        let view = StateView::new(&store);
        writer.set_balance(&addr, 100).unwrap();
        assert_eq!(view.get_balance(&addr).unwrap(), 100);
        writer.set_balance(&addr, 200).unwrap();
        assert_eq!(view.get_balance(&addr).unwrap(), 200);
    }
}
