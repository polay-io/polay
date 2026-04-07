//! High-level, read-only accessors over chain state.

use borsh::BorshDeserialize;
use polay_types::{
    AccountState, Achievement, Address, AssetClass, Attestor, Block, Delegation, EpochInfo,
    EquivocationEvidence, Event, Guild, GuildMembership, Hash, Listing, MatchResult,
    MatchSettlement, PlayerProfile, Proposal, Rental, SessionGrant, SupplyInfo, Tournament,
    TransactionReceipt, TxLocation, UnbondingEntry, ValidatorInfo, Vote,
};

use crate::error::StateResult;
use crate::keys;
use crate::store::{store_get, StateStore};

/// Read-only view into the chain state.
pub struct StateView<'a> {
    store: &'a dyn StateStore,
}

impl<'a> StateView<'a> {
    /// Create a new view backed by the given store.
    pub fn new(store: &'a dyn StateStore) -> Self {
        Self { store }
    }

    // -- Accounts ------------------------------------------------------------

    /// Retrieve the full [`AccountState`] for `addr`, or `None` if it does not
    /// exist.
    pub fn get_account(&self, addr: &Address) -> StateResult<Option<AccountState>> {
        store_get(self.store,&keys::account_key(addr))
    }

    /// Retrieve the native POL balance for `addr`. Returns `0` if the address
    /// has no balance entry.
    pub fn get_balance(&self, addr: &Address) -> StateResult<u64> {
        let val: Option<u64> = store_get(self.store,&keys::balance_key(addr))?;
        Ok(val.unwrap_or(0))
    }

    // -- Assets --------------------------------------------------------------

    /// Retrieve an [`AssetClass`] by its content-addressed id.
    pub fn get_asset_class(&self, id: &Hash) -> StateResult<Option<AssetClass>> {
        store_get(self.store,&keys::asset_class_key(id))
    }

    /// Retrieve the balance of a specific asset class held by `owner`.
    /// Returns `0` if there is no entry.
    pub fn get_asset_balance(&self, asset_class_id: &Hash, owner: &Address) -> StateResult<u64> {
        let val: Option<u64> = store_get(self.store, &keys::asset_balance_key(asset_class_id, owner))?;
        Ok(val.unwrap_or(0))
    }

    // -- Validators / Staking ------------------------------------------------

    /// Retrieve [`ValidatorInfo`] for the given address.
    pub fn get_validator(&self, addr: &Address) -> StateResult<Option<ValidatorInfo>> {
        store_get(self.store,&keys::validator_key(addr))
    }

    /// Retrieve a [`Delegation`] from `delegator` to `validator`.
    pub fn get_delegation(
        &self,
        delegator: &Address,
        validator: &Address,
    ) -> StateResult<Option<Delegation>> {
        store_get(self.store, &keys::delegation_key(delegator, validator))
    }

    // -- Unbonding queue ------------------------------------------------------

    /// Retrieve all unbonding entries for a delegator by scanning the
    /// per-delegator prefix.
    pub fn get_unbonding_entries(&self, delegator: &Address) -> StateResult<Vec<UnbondingEntry>> {
        let prefix = keys::unbonding_prefix(delegator);
        let pairs = self.store.prefix_scan(&prefix)?;
        let mut entries = Vec::with_capacity(pairs.len());
        for (_key, value) in pairs {
            let entry = UnbondingEntry::try_from_slice(&value).map_err(|e| {
                crate::error::StateError::SerializationError(format!(
                    "borsh decode unbonding entry: {}",
                    e,
                ))
            })?;
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Retrieve all mature unbonding entries (completion_height <= current_height)
    /// from the global index.
    pub fn get_mature_unbondings(&self, current_height: u64) -> StateResult<Vec<(UnbondingEntry, u8)>> {
        let prefix = keys::unbonding_index_prefix();
        let pairs = self.store.prefix_scan(&prefix)?;
        let mut entries = Vec::new();
        for (key, value) in pairs {
            // Key layout: [PREFIX_UNBONDING_INDEX(1) | completion_height(8 BE) | delegator(32) | index(1)]
            if key.len() < 1 + 8 + Address::LEN + 1 {
                continue;
            }
            let mut height_bytes = [0u8; 8];
            height_bytes.copy_from_slice(&key[1..9]);
            let completion_height = u64::from_be_bytes(height_bytes);
            if completion_height > current_height {
                break; // sorted by height, so no more mature entries
            }
            let index = key[1 + 8 + Address::LEN];
            let entry = UnbondingEntry::try_from_slice(&value).map_err(|e| {
                crate::error::StateError::SerializationError(format!(
                    "borsh decode unbonding entry: {}",
                    e,
                ))
            })?;
            entries.push((entry, index));
        }
        Ok(entries)
    }

    // -- Marketplace ---------------------------------------------------------

    /// Retrieve a [`Listing`] by its id.
    pub fn get_listing(&self, id: &Hash) -> StateResult<Option<Listing>> {
        store_get(self.store,&keys::listing_key(id))
    }

    // -- Identity / Gaming ---------------------------------------------------

    /// Retrieve a [`PlayerProfile`] for the given address.
    pub fn get_profile(&self, addr: &Address) -> StateResult<Option<PlayerProfile>> {
        store_get(self.store,&keys::profile_key(addr))
    }

    /// Retrieve a specific [`Achievement`] for a player.
    pub fn get_achievement(
        &self,
        addr: &Address,
        achievement_id: &str,
    ) -> StateResult<Option<Achievement>> {
        store_get(self.store, &keys::achievement_key(addr, achievement_id))
    }

    // -- Attestation ---------------------------------------------------------

    /// Retrieve an [`Attestor`] registration.
    pub fn get_attestor(&self, addr: &Address) -> StateResult<Option<Attestor>> {
        store_get(self.store,&keys::attestor_key(addr))
    }

    /// Retrieve a [`MatchResult`] by its id.
    pub fn get_match_result(&self, id: &Hash) -> StateResult<Option<MatchResult>> {
        store_get(self.store,&keys::match_result_key(id))
    }

    /// Retrieve a [`MatchSettlement`] by its match id.
    pub fn get_match_settlement(&self, id: &Hash) -> StateResult<Option<MatchSettlement>> {
        store_get(self.store,&keys::match_settlement_key(id))
    }

    // -- Governance -----------------------------------------------------------

    /// Retrieve a [`Proposal`] by its id.
    pub fn get_proposal(&self, id: &Hash) -> StateResult<Option<Proposal>> {
        store_get(self.store, &keys::proposal_key(id))
    }

    /// Retrieve a [`Vote`] for a specific proposal and voter.
    pub fn get_vote(&self, proposal_id: &Hash, voter: &Address) -> StateResult<Option<Vote>> {
        store_get(self.store, &keys::vote_key(proposal_id, voter))
    }

    /// Retrieve the list of all proposal IDs.
    pub fn get_proposal_list(&self) -> StateResult<Vec<Hash>> {
        let val: Option<Vec<Hash>> = store_get(self.store, &keys::proposal_list_key())?;
        Ok(val.unwrap_or_default())
    }

    // -- Rentals ---------------------------------------------------------------

    /// Retrieve a [`Rental`] by its id.
    pub fn get_rental(&self, rental_id: &Hash) -> StateResult<Option<Rental>> {
        store_get(self.store, &keys::rental_key(rental_id))
    }

    /// Retrieve all rental IDs for a given owner by scanning the owner index.
    pub fn get_rentals_by_owner(&self, owner: &Address) -> StateResult<Vec<Hash>> {
        let prefix = keys::rental_by_owner_prefix(owner);
        let pairs = self.store.prefix_scan(&prefix)?;
        let mut ids = Vec::with_capacity(pairs.len());
        for (key, _value) in pairs {
            // Key layout: [PREFIX(1) | owner(32) | rental_id(32)]
            if key.len() >= 1 + Address::LEN + Hash::LEN {
                let mut id_bytes = [0u8; 32];
                id_bytes.copy_from_slice(&key[1 + Address::LEN..]);
                ids.push(Hash::new(id_bytes));
            }
        }
        Ok(ids)
    }

    /// Retrieve all rental IDs for a given renter by scanning the renter index.
    pub fn get_rentals_by_renter(&self, renter: &Address) -> StateResult<Vec<Hash>> {
        let prefix = keys::rental_by_renter_prefix(renter);
        let pairs = self.store.prefix_scan(&prefix)?;
        let mut ids = Vec::with_capacity(pairs.len());
        for (key, _value) in pairs {
            // Key layout: [PREFIX(1) | renter(32) | rental_id(32)]
            if key.len() >= 1 + Address::LEN + Hash::LEN {
                let mut id_bytes = [0u8; 32];
                id_bytes.copy_from_slice(&key[1 + Address::LEN..]);
                ids.push(Hash::new(id_bytes));
            }
        }
        Ok(ids)
    }

    // -- Guilds ----------------------------------------------------------------

    /// Retrieve a [`Guild`] by its id.
    pub fn get_guild(&self, guild_id: &Hash) -> StateResult<Option<Guild>> {
        store_get(self.store, &keys::guild_key(guild_id))
    }

    /// Retrieve a [`GuildMembership`] for a specific guild and member.
    pub fn get_guild_membership(
        &self,
        guild_id: &Hash,
        member: &Address,
    ) -> StateResult<Option<GuildMembership>> {
        store_get(self.store, &keys::guild_member_key(guild_id, member))
    }

    /// Retrieve all guild memberships for a given guild by scanning the member prefix.
    pub fn get_guild_members(&self, guild_id: &Hash) -> StateResult<Vec<GuildMembership>> {
        let prefix = keys::guild_member_prefix(guild_id);
        let pairs = self.store.prefix_scan(&prefix)?;
        let mut members = Vec::with_capacity(pairs.len());
        for (_key, value) in pairs {
            let membership = GuildMembership::try_from_slice(&value).map_err(|e| {
                crate::error::StateError::SerializationError(format!(
                    "borsh decode guild membership: {}",
                    e,
                ))
            })?;
            members.push(membership);
        }
        Ok(members)
    }

    /// Retrieve all guild IDs that a member belongs to.
    pub fn get_member_guilds(&self, member: &Address) -> StateResult<Vec<Hash>> {
        let prefix = keys::member_guilds_prefix(member);
        let pairs = self.store.prefix_scan(&prefix)?;
        let mut ids = Vec::with_capacity(pairs.len());
        for (key, _value) in pairs {
            // Key layout: [PREFIX(1) | member(32) | guild_id(32)]
            if key.len() >= 1 + Address::LEN + Hash::LEN {
                let mut id_bytes = [0u8; 32];
                id_bytes.copy_from_slice(&key[1 + Address::LEN..]);
                ids.push(Hash::new(id_bytes));
            }
        }
        Ok(ids)
    }

    // -- Tournaments -----------------------------------------------------------

    /// Retrieve a [`Tournament`] by its id.
    pub fn get_tournament(&self, tournament_id: &Hash) -> StateResult<Option<Tournament>> {
        store_get(self.store, &keys::tournament_key(tournament_id))
    }

    /// Check whether a participant is registered for a tournament.
    pub fn is_tournament_participant(
        &self,
        tournament_id: &Hash,
        participant: &Address,
    ) -> StateResult<bool> {
        let val: Option<bool> = store_get(
            self.store,
            &keys::tournament_participant_key(tournament_id, participant),
        )?;
        Ok(val.unwrap_or(false))
    }

    // -- Session keys ---------------------------------------------------------

    /// Retrieve a [`SessionGrant`] for the given granter and session address.
    pub fn get_session(
        &self,
        granter: &Address,
        session_address: &Address,
    ) -> StateResult<Option<SessionGrant>> {
        store_get(self.store, &keys::session_key(granter, session_address))
    }

    /// Retrieve all sessions belonging to `granter` by scanning the session
    /// prefix.
    pub fn get_sessions_for_granter(&self, granter: &Address) -> StateResult<Vec<SessionGrant>> {
        let prefix = keys::session_prefix(granter);
        let pairs = self.store.prefix_scan(&prefix)?;
        let mut grants = Vec::with_capacity(pairs.len());
        for (_key, value) in pairs {
            let grant = SessionGrant::try_from_slice(&value).map_err(|e| {
                crate::error::StateError::SerializationError(format!(
                    "borsh decode session grant: {}",
                    e,
                ))
            })?;
            grants.push(grant);
        }
        Ok(grants)
    }

    // -- Receipts / Events ---------------------------------------------------

    /// Retrieve a [`TransactionReceipt`] by its transaction hash.
    pub fn get_receipt(&self, tx_hash: &Hash) -> StateResult<Option<TransactionReceipt>> {
        store_get(self.store, &keys::receipt_key(tx_hash))
    }

    /// Retrieve a [`TxLocation`] (block height and index) for a transaction.
    pub fn get_tx_location(&self, tx_hash: &Hash) -> StateResult<Option<TxLocation>> {
        store_get(self.store, &keys::tx_block_index_key(tx_hash))
    }

    /// Retrieve all events emitted in the block at `height`.
    pub fn get_block_events(&self, height: u64) -> StateResult<Option<Vec<Event>>> {
        store_get(self.store, &keys::block_events_key(height))
    }

    // -- Epoch ---------------------------------------------------------------

    /// Retrieve [`EpochInfo`] for a given epoch number.
    pub fn get_epoch_info(&self, epoch: u64) -> StateResult<Option<EpochInfo>> {
        store_get(self.store, &keys::epoch_key(epoch))
    }

    /// Compute the current epoch number from the chain height and epoch length.
    /// Returns `0` if height is 0.
    pub fn get_current_epoch(&self, epoch_length: u64) -> StateResult<u64> {
        let height = self.get_chain_height()?;
        Ok(height / epoch_length)
    }

    /// Retrieve the current active validator set addresses.
    pub fn get_active_validator_set(&self) -> StateResult<Option<Vec<Address>>> {
        store_get(self.store, &keys::active_validator_set_key())
    }

    // -- Economics / supply tracking -----------------------------------------

    /// Retrieve the global [`SupplyInfo`], or `None` if not yet initialized.
    pub fn get_supply_info(&self) -> StateResult<Option<SupplyInfo>> {
        store_get(self.store, &keys::supply_info_key())
    }

    // -- Equivocation evidence -------------------------------------------------

    /// Retrieve equivocation evidence for a validator at a specific height.
    pub fn get_equivocation_evidence(
        &self,
        validator: &Address,
        height: u64,
    ) -> StateResult<Option<EquivocationEvidence>> {
        store_get(self.store, &keys::equivocation_key(validator, height))
    }

    /// Retrieve all equivocation evidence for a validator.
    pub fn get_equivocation_evidence_for_validator(
        &self,
        validator: &Address,
    ) -> StateResult<Vec<EquivocationEvidence>> {
        let prefix = keys::equivocation_prefix(validator);
        let pairs = self.store.prefix_scan(&prefix)?;
        let mut evidence = Vec::with_capacity(pairs.len());
        for (_key, value) in pairs {
            let e = EquivocationEvidence::try_from_slice(&value).map_err(|e| {
                crate::error::StateError::SerializationError(format!(
                    "borsh decode equivocation evidence: {}",
                    e,
                ))
            })?;
            evidence.push(e);
        }
        Ok(evidence)
    }

    // -- Chain metadata ------------------------------------------------------

    /// Retrieve the current chain height. Returns `0` if not yet set (pre-genesis).
    pub fn get_chain_height(&self) -> StateResult<u64> {
        let val: Option<u64> = store_get(self.store,&keys::chain_height_key())?;
        Ok(val.unwrap_or(0))
    }

    /// Retrieve the hash of the latest committed block.
    /// Returns `Hash::ZERO` if not yet set.
    pub fn get_latest_hash(&self) -> StateResult<Hash> {
        let val: Option<Hash> = store_get(self.store,&keys::latest_hash_key())?;
        Ok(val.unwrap_or(Hash::ZERO))
    }

    /// Retrieve a [`Block`] by height.
    pub fn get_block(&self, height: u64) -> StateResult<Option<Block>> {
        store_get(self.store,&keys::block_key(height))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_writer::StateWriter;
    use crate::store::MemoryStore;
    use polay_types::block::BlockHeader;

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn test_hash(byte: u8) -> Hash {
        Hash::new([byte; 32])
    }

    #[test]
    fn balance_defaults_to_zero() {
        let store = MemoryStore::new();
        let view = StateView::new(&store);
        assert_eq!(view.get_balance(&test_addr(1)).unwrap(), 0);
    }

    #[test]
    fn chain_height_defaults_to_zero() {
        let store = MemoryStore::new();
        let view = StateView::new(&store);
        assert_eq!(view.get_chain_height().unwrap(), 0);
    }

    #[test]
    fn latest_hash_defaults_to_zero() {
        let store = MemoryStore::new();
        let view = StateView::new(&store);
        assert_eq!(view.get_latest_hash().unwrap(), Hash::ZERO);
    }

    #[test]
    fn round_trip_account() {
        let store = MemoryStore::new();
        let addr = test_addr(0xAA);
        let acct = AccountState::with_balance(addr, 1_000_000, 1);
        StateWriter::new(&store).set_account(&acct).unwrap();
        let got = StateView::new(&store).get_account(&addr).unwrap().unwrap();
        assert_eq!(got, acct);
    }

    #[test]
    fn round_trip_balance() {
        let store = MemoryStore::new();
        let addr = test_addr(0xBB);
        StateWriter::new(&store).set_balance(&addr, 42).unwrap();
        assert_eq!(StateView::new(&store).get_balance(&addr).unwrap(), 42);
    }

    #[test]
    fn round_trip_validator() {
        let store = MemoryStore::new();
        let addr = test_addr(0xCC);
        let mut info = ValidatorInfo::new(addr, 500);
        info.stake = 10_000_000;
        StateWriter::new(&store).set_validator(&info).unwrap();
        let got = StateView::new(&store)
            .get_validator(&addr)
            .unwrap()
            .unwrap();
        assert_eq!(got, info);
    }

    #[test]
    fn round_trip_delegation() {
        let store = MemoryStore::new();
        let delegator = test_addr(1);
        let validator = test_addr(2);
        let mut del = Delegation::new(delegator, validator);
        del.add_stake(5000);
        StateWriter::new(&store).set_delegation(&del).unwrap();
        let got = StateView::new(&store)
            .get_delegation(&delegator, &validator)
            .unwrap()
            .unwrap();
        assert_eq!(got, del);
    }

    #[test]
    fn round_trip_profile() {
        let store = MemoryStore::new();
        let addr = test_addr(0xDD);
        let profile = PlayerProfile::new(
            addr,
            "alice".into(),
            "Alice".into(),
            None,
            1,
        );
        StateWriter::new(&store).set_profile(&profile).unwrap();
        let got = StateView::new(&store).get_profile(&addr).unwrap().unwrap();
        assert_eq!(got, profile);
    }

    #[test]
    fn round_trip_achievement() {
        let store = MemoryStore::new();
        let addr = test_addr(0xEE);
        let ach = Achievement {
            id: "first_win".into(),
            player: addr,
            name: "First Win".into(),
            metadata: "{}".into(),
            awarded_at: 42,
            soulbound: true,
        };
        StateWriter::new(&store).set_achievement(&ach).unwrap();
        let got = StateView::new(&store)
            .get_achievement(&addr, "first_win")
            .unwrap()
            .unwrap();
        assert_eq!(got, ach);
    }

    #[test]
    fn round_trip_block_and_chain_meta() {
        let store = MemoryStore::new();
        let header = BlockHeader {
            height: 1,
            timestamp: 1700000000,
            parent_hash: Hash::ZERO,
            state_root: test_hash(0x44),
            transactions_root: test_hash(0x55),
            proposer: test_addr(0xAA),
            chain_id: "polay-devnet-1".into(),
            hash: test_hash(0x11),
        };
        let block = Block::new(header, vec![]);
        let writer = StateWriter::new(&store);
        writer.store_block(&block).unwrap();
        writer.set_chain_height(1).unwrap();
        writer.set_latest_hash(block.hash()).unwrap();

        let view = StateView::new(&store);
        assert_eq!(view.get_chain_height().unwrap(), 1);
        assert_eq!(view.get_latest_hash().unwrap(), *block.hash());
        assert_eq!(view.get_block(1).unwrap().unwrap(), block);
    }

    #[test]
    fn missing_entries_return_none() {
        let store = MemoryStore::new();
        let view = StateView::new(&store);
        let addr = test_addr(0xFF);
        assert!(view.get_account(&addr).unwrap().is_none());
        assert!(view.get_validator(&addr).unwrap().is_none());
        assert!(view.get_profile(&addr).unwrap().is_none());
        assert!(view.get_attestor(&addr).unwrap().is_none());
        assert!(view.get_block(999).unwrap().is_none());
    }

    #[test]
    fn round_trip_receipt() {
        let store = MemoryStore::new();
        let tx_hash = test_hash(0xAA);
        let receipt = TransactionReceipt::success(tx_hash, 10, 500, 21000, Address::ZERO, vec![
            Event::new("bank", "transfer", vec![
                ("from".into(), "alice".into()),
                ("to".into(), "bob".into()),
                ("amount".into(), "1000".into()),
            ]),
        ]);
        StateWriter::new(&store).set_receipt(&receipt).unwrap();
        let got = StateView::new(&store).get_receipt(&tx_hash).unwrap().unwrap();
        assert_eq!(got, receipt);
    }

    #[test]
    fn receipt_missing_returns_none() {
        let store = MemoryStore::new();
        let view = StateView::new(&store);
        assert!(view.get_receipt(&test_hash(0xFF)).unwrap().is_none());
    }

    #[test]
    fn round_trip_tx_location() {
        let store = MemoryStore::new();
        let tx_hash = test_hash(0xBB);
        let location = TxLocation {
            block_height: 42,
            tx_index: 3,
        };
        StateWriter::new(&store)
            .set_tx_location(&tx_hash, &location)
            .unwrap();
        let got = StateView::new(&store)
            .get_tx_location(&tx_hash)
            .unwrap()
            .unwrap();
        assert_eq!(got, location);
    }

    #[test]
    fn tx_location_missing_returns_none() {
        let store = MemoryStore::new();
        let view = StateView::new(&store);
        assert!(view.get_tx_location(&test_hash(0xFF)).unwrap().is_none());
    }

    #[test]
    fn round_trip_block_events() {
        let store = MemoryStore::new();
        let events = vec![
            Event::new("bank", "transfer", vec![
                ("amount".into(), "100".into()),
            ]),
            Event::new("asset", "mint", vec![
                ("amount".into(), "50".into()),
            ]),
        ];
        StateWriter::new(&store)
            .set_block_events(7, &events)
            .unwrap();
        let got = StateView::new(&store)
            .get_block_events(7)
            .unwrap()
            .unwrap();
        assert_eq!(got, events);
    }

    #[test]
    fn block_events_missing_returns_none() {
        let store = MemoryStore::new();
        let view = StateView::new(&store);
        assert!(view.get_block_events(999).unwrap().is_none());
    }

    #[test]
    fn multiple_receipts_stored_and_retrieved() {
        let store = MemoryStore::new();
        let writer = StateWriter::new(&store);
        let view = StateView::new(&store);

        let hashes: Vec<Hash> = (0u8..5).map(|b| test_hash(b + 1)).collect();
        for (idx, h) in hashes.iter().enumerate() {
            let receipt = TransactionReceipt::success(
                *h,
                10,
                (idx as u64 + 1) * 100,
                21000,
                Address::ZERO,
                vec![],
            );
            writer.set_receipt(&receipt).unwrap();
        }

        for (idx, h) in hashes.iter().enumerate() {
            let got = view.get_receipt(h).unwrap().unwrap();
            assert_eq!(got.tx_hash, *h);
            assert_eq!(got.fee_used, (idx as u64 + 1) * 100);
        }
    }

    #[test]
    fn events_correctly_aggregated_per_block() {
        let store = MemoryStore::new();
        let writer = StateWriter::new(&store);
        let view = StateView::new(&store);

        // Block 1: two events
        let events_1 = vec![
            Event::new("bank", "transfer", vec![("amount".into(), "100".into())]),
            Event::new("bank", "transfer", vec![("amount".into(), "200".into())]),
        ];
        writer.set_block_events(1, &events_1).unwrap();

        // Block 2: one event
        let events_2 = vec![
            Event::new("asset", "mint", vec![("amount".into(), "50".into())]),
        ];
        writer.set_block_events(2, &events_2).unwrap();

        assert_eq!(view.get_block_events(1).unwrap().unwrap().len(), 2);
        assert_eq!(view.get_block_events(2).unwrap().unwrap().len(), 1);
        assert!(view.get_block_events(3).unwrap().is_none());
    }
}
