//! Deterministic state-key construction.
//!
//! Every key is prefixed with a unique byte that identifies the namespace,
//! followed by the relevant identifiers encoded as raw bytes.  This makes
//! prefix iteration trivial and avoids key collisions between namespaces.

use polay_types::{Address, Hash};

// ---------------------------------------------------------------------------
// Prefix bytes — each namespace gets exactly one.
// ---------------------------------------------------------------------------

pub const PREFIX_ACCOUNT: u8 = 0x01;
pub const PREFIX_BALANCE: u8 = 0x02;
pub const PREFIX_ASSET_CLASS: u8 = 0x03;
pub const PREFIX_ASSET_BALANCE: u8 = 0x04;
pub const PREFIX_VALIDATOR: u8 = 0x05;
pub const PREFIX_DELEGATION: u8 = 0x06;
pub const PREFIX_LISTING: u8 = 0x07;
pub const PREFIX_PROFILE: u8 = 0x08;
pub const PREFIX_ACHIEVEMENT: u8 = 0x09;
pub const PREFIX_ATTESTOR: u8 = 0x0A;
pub const PREFIX_MATCH_RESULT: u8 = 0x0B;
pub const PREFIX_MATCH_SETTLEMENT: u8 = 0x0C;
pub const PREFIX_UNBONDING: u8 = 0x0D;
/// Global unbonding index sorted by completion_height for efficient maturation scanning.
pub const PREFIX_UNBONDING_INDEX: u8 = 0x0E;
pub const PREFIX_PROPOSAL: u8 = 0x0F;
pub const PREFIX_VOTE: u8 = 0x10;
pub const PREFIX_PROPOSAL_LIST: u8 = 0x11;
pub const PREFIX_RECEIPT: u8 = 0x12;
pub const PREFIX_EVENT: u8 = 0x13;
pub const PREFIX_TX_BLOCK_INDEX: u8 = 0x14;
pub const PREFIX_BLOCK_EVENTS: u8 = 0x15;
pub const PREFIX_EPOCH: u8 = 0x16;
pub const PREFIX_ACTIVE_VALIDATOR_SET: u8 = 0x17;
pub const PREFIX_SESSION: u8 = 0x18;
// Rentals
pub const PREFIX_RENTAL: u8 = 0x19;
pub const PREFIX_RENTAL_BY_OWNER: u8 = 0x1A;
pub const PREFIX_RENTAL_BY_RENTER: u8 = 0x1B;

// Guilds
pub const PREFIX_GUILD: u8 = 0x1C;
pub const PREFIX_GUILD_MEMBER: u8 = 0x1D;
pub const PREFIX_MEMBER_GUILDS: u8 = 0x1E;

// Tournaments
pub const PREFIX_TOURNAMENT: u8 = 0x1F;
pub const PREFIX_TOURNAMENT_PARTICIPANT: u8 = 0x20;

// Economics / supply tracking
pub const PREFIX_SUPPLY: u8 = 0x21;

// Equivocation evidence
pub const PREFIX_EQUIVOCATION: u8 = 0x22;

pub const PREFIX_CHAIN_META: u8 = 0xF0;
pub const PREFIX_BLOCK: u8 = 0xF1;

// Well-known sub-keys under PREFIX_CHAIN_META.
const META_HEIGHT: &[u8] = b"height";
const META_LATEST_HASH: &[u8] = b"latest_hash";

// ---------------------------------------------------------------------------
// Key builders
// ---------------------------------------------------------------------------

/// Key: `[PREFIX_ACCOUNT | address(32)]`
pub fn account_key(addr: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_ACCOUNT);
    key.extend_from_slice(addr.as_bytes());
    key
}

/// Key: `[PREFIX_BALANCE | address(32)]`
pub fn balance_key(addr: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_BALANCE);
    key.extend_from_slice(addr.as_bytes());
    key
}

/// Key: `[PREFIX_ASSET_CLASS | id(32)]`
pub fn asset_class_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_ASSET_CLASS);
    key.extend_from_slice(id.as_bytes());
    key
}

/// Key: `[PREFIX_ASSET_BALANCE | asset_class_id(32) | owner(32)]`
pub fn asset_balance_key(asset_class_id: &Hash, owner: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN + Address::LEN);
    key.push(PREFIX_ASSET_BALANCE);
    key.extend_from_slice(asset_class_id.as_bytes());
    key.extend_from_slice(owner.as_bytes());
    key
}

/// Key: `[PREFIX_VALIDATOR | address(32)]`
pub fn validator_key(addr: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_VALIDATOR);
    key.extend_from_slice(addr.as_bytes());
    key
}

/// Key: `[PREFIX_DELEGATION | delegator(32) | validator(32)]`
pub fn delegation_key(delegator: &Address, validator: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN + Address::LEN);
    key.push(PREFIX_DELEGATION);
    key.extend_from_slice(delegator.as_bytes());
    key.extend_from_slice(validator.as_bytes());
    key
}

/// Key: `[PREFIX_LISTING | id(32)]`
pub fn listing_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_LISTING);
    key.extend_from_slice(id.as_bytes());
    key
}

/// Key: `[PREFIX_PROFILE | address(32)]`
pub fn profile_key(addr: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_PROFILE);
    key.extend_from_slice(addr.as_bytes());
    key
}

/// Key: `[PREFIX_ACHIEVEMENT | address(32) | achievement_id bytes]`
pub fn achievement_key(addr: &Address, achievement_id: &str) -> Vec<u8> {
    let id_bytes = achievement_id.as_bytes();
    let mut key = Vec::with_capacity(1 + Address::LEN + id_bytes.len());
    key.push(PREFIX_ACHIEVEMENT);
    key.extend_from_slice(addr.as_bytes());
    key.extend_from_slice(id_bytes);
    key
}

/// Key: `[PREFIX_ATTESTOR | address(32)]`
pub fn attestor_key(addr: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_ATTESTOR);
    key.extend_from_slice(addr.as_bytes());
    key
}

/// Key: `[PREFIX_MATCH_RESULT | match_id(32)]`
pub fn match_result_key(match_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_MATCH_RESULT);
    key.extend_from_slice(match_id.as_bytes());
    key
}

/// Key: `[PREFIX_MATCH_SETTLEMENT | match_id(32)]`
pub fn match_settlement_key(match_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_MATCH_SETTLEMENT);
    key.extend_from_slice(match_id.as_bytes());
    key
}

/// Key: `[PREFIX_UNBONDING | delegator(32) | completion_height(8 BE) | index(1)]`
///
/// Big-endian height ensures entries are lexicographically ordered by completion time.
pub fn unbonding_key(delegator: &Address, completion_height: u64, index: u8) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN + 8 + 1);
    key.push(PREFIX_UNBONDING);
    key.extend_from_slice(delegator.as_bytes());
    key.extend_from_slice(&completion_height.to_be_bytes());
    key.push(index);
    key
}

/// Prefix: `[PREFIX_UNBONDING | delegator(32)]` — for scanning all entries for a delegator.
pub fn unbonding_prefix(delegator: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_UNBONDING);
    key.extend_from_slice(delegator.as_bytes());
    key
}

/// Key: `[PREFIX_UNBONDING_INDEX | completion_height(8 BE) | delegator(32) | index(1)]`
///
/// Global index sorted by completion_height for efficient maturation scanning.
pub fn unbonding_index_key(completion_height: u64, delegator: &Address, index: u8) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + 8 + Address::LEN + 1);
    key.push(PREFIX_UNBONDING_INDEX);
    key.extend_from_slice(&completion_height.to_be_bytes());
    key.extend_from_slice(delegator.as_bytes());
    key.push(index);
    key
}

/// Prefix: `[PREFIX_UNBONDING_INDEX]` — for scanning all global unbonding entries.
pub fn unbonding_index_prefix() -> Vec<u8> {
    vec![PREFIX_UNBONDING_INDEX]
}

/// Key: `[PREFIX_PROPOSAL | id(32)]`
pub fn proposal_key(id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_PROPOSAL);
    key.extend_from_slice(id.as_bytes());
    key
}

/// Key: `[PREFIX_VOTE | proposal_id(32) | voter(32)]`
pub fn vote_key(proposal_id: &Hash, voter: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN + Address::LEN);
    key.push(PREFIX_VOTE);
    key.extend_from_slice(proposal_id.as_bytes());
    key.extend_from_slice(voter.as_bytes());
    key
}

/// Key: `[PREFIX_PROPOSAL_LIST]` -- stores a borsh-encoded `Vec<Hash>` of all proposal IDs.
pub fn proposal_list_key() -> Vec<u8> {
    vec![PREFIX_PROPOSAL_LIST]
}

/// Key: `[PREFIX_CHAIN_META | "height"]`
pub fn chain_height_key() -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + META_HEIGHT.len());
    key.push(PREFIX_CHAIN_META);
    key.extend_from_slice(META_HEIGHT);
    key
}

/// Key: `[PREFIX_CHAIN_META | "latest_hash"]`
pub fn latest_hash_key() -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + META_LATEST_HASH.len());
    key.push(PREFIX_CHAIN_META);
    key.extend_from_slice(META_LATEST_HASH);
    key
}

/// Key: `[PREFIX_BLOCK | height(8 bytes big-endian)]`
///
/// Big-endian encoding ensures that block keys are sorted by height when
/// iterated in lexicographic order.
pub fn block_key(height: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + 8);
    key.push(PREFIX_BLOCK);
    key.extend_from_slice(&height.to_be_bytes());
    key
}

/// Key: `[PREFIX_RECEIPT | tx_hash(32)]`
///
/// Stores a transaction receipt keyed by the transaction hash.
pub fn receipt_key(tx_hash: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_RECEIPT);
    key.extend_from_slice(tx_hash.as_bytes());
    key
}

/// Key: `[PREFIX_TX_BLOCK_INDEX | tx_hash(32)]`
///
/// Maps a transaction hash to its location (block_height, tx_index) in the chain.
pub fn tx_block_index_key(tx_hash: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_TX_BLOCK_INDEX);
    key.extend_from_slice(tx_hash.as_bytes());
    key
}

/// Key: `[PREFIX_BLOCK_EVENTS | height(8 bytes big-endian)]`
///
/// Stores all events emitted in a block, keyed by block height.
/// Big-endian encoding ensures block events are sorted by height.
pub fn block_events_key(height: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + 8);
    key.push(PREFIX_BLOCK_EVENTS);
    key.extend_from_slice(&height.to_be_bytes());
    key
}

/// Key: `[PREFIX_EPOCH | epoch_number(8 bytes big-endian)]`
///
/// Stores epoch info keyed by epoch number.
/// Big-endian encoding ensures epoch keys are sorted by epoch number.
pub fn epoch_key(epoch: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + 8);
    key.push(PREFIX_EPOCH);
    key.extend_from_slice(&epoch.to_be_bytes());
    key
}

/// Key: `[PREFIX_ACTIVE_VALIDATOR_SET]`
///
/// Stores the current active validator set (the authoritative set used by
/// consensus). This is a single key that is overwritten at each epoch
/// transition.
pub fn active_validator_set_key() -> Vec<u8> {
    vec![PREFIX_ACTIVE_VALIDATOR_SET]
}

/// Key: `[PREFIX_SESSION | granter_address(32) | session_address(32)]`
///
/// Stores a session grant keyed by both the granting account and the session
/// address so the granter can enumerate and manage their sessions.
pub fn session_key(granter: &Address, session_address: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN + Address::LEN);
    key.push(PREFIX_SESSION);
    key.extend_from_slice(granter.as_bytes());
    key.extend_from_slice(session_address.as_bytes());
    key
}

/// Prefix: `[PREFIX_SESSION | granter_address(32)]` -- for scanning all
/// sessions belonging to a granter.
pub fn session_prefix(granter: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_SESSION);
    key.extend_from_slice(granter.as_bytes());
    key
}

// ---------------------------------------------------------------------------
// Rental keys
// ---------------------------------------------------------------------------

/// Key: `[PREFIX_RENTAL | rental_id(32)]`
pub fn rental_key(rental_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_RENTAL);
    key.extend_from_slice(rental_id.as_bytes());
    key
}

/// Key: `[PREFIX_RENTAL_BY_OWNER | owner(32) | rental_id(32)]`
pub fn rental_by_owner_key(owner: &Address, rental_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN + Hash::LEN);
    key.push(PREFIX_RENTAL_BY_OWNER);
    key.extend_from_slice(owner.as_bytes());
    key.extend_from_slice(rental_id.as_bytes());
    key
}

/// Prefix: `[PREFIX_RENTAL_BY_OWNER | owner(32)]` -- for scanning all rentals by owner.
pub fn rental_by_owner_prefix(owner: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_RENTAL_BY_OWNER);
    key.extend_from_slice(owner.as_bytes());
    key
}

/// Key: `[PREFIX_RENTAL_BY_RENTER | renter(32) | rental_id(32)]`
pub fn rental_by_renter_key(renter: &Address, rental_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN + Hash::LEN);
    key.push(PREFIX_RENTAL_BY_RENTER);
    key.extend_from_slice(renter.as_bytes());
    key.extend_from_slice(rental_id.as_bytes());
    key
}

/// Prefix: `[PREFIX_RENTAL_BY_RENTER | renter(32)]` -- for scanning all rentals by renter.
pub fn rental_by_renter_prefix(renter: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_RENTAL_BY_RENTER);
    key.extend_from_slice(renter.as_bytes());
    key
}

// ---------------------------------------------------------------------------
// Guild keys
// ---------------------------------------------------------------------------

/// Key: `[PREFIX_GUILD | guild_id(32)]`
pub fn guild_key(guild_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_GUILD);
    key.extend_from_slice(guild_id.as_bytes());
    key
}

/// Key: `[PREFIX_GUILD_MEMBER | guild_id(32) | member(32)]`
pub fn guild_member_key(guild_id: &Hash, member: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN + Address::LEN);
    key.push(PREFIX_GUILD_MEMBER);
    key.extend_from_slice(guild_id.as_bytes());
    key.extend_from_slice(member.as_bytes());
    key
}

/// Prefix: `[PREFIX_GUILD_MEMBER | guild_id(32)]` -- for scanning all members of a guild.
pub fn guild_member_prefix(guild_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_GUILD_MEMBER);
    key.extend_from_slice(guild_id.as_bytes());
    key
}

/// Key: `[PREFIX_MEMBER_GUILDS | member(32) | guild_id(32)]`
pub fn member_guilds_key(member: &Address, guild_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN + Hash::LEN);
    key.push(PREFIX_MEMBER_GUILDS);
    key.extend_from_slice(member.as_bytes());
    key.extend_from_slice(guild_id.as_bytes());
    key
}

/// Prefix: `[PREFIX_MEMBER_GUILDS | member(32)]` -- for scanning all guilds a member belongs to.
pub fn member_guilds_prefix(member: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_MEMBER_GUILDS);
    key.extend_from_slice(member.as_bytes());
    key
}

// ---------------------------------------------------------------------------
// Tournament keys
// ---------------------------------------------------------------------------

/// Key: `[PREFIX_TOURNAMENT | tournament_id(32)]`
pub fn tournament_key(tournament_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_TOURNAMENT);
    key.extend_from_slice(tournament_id.as_bytes());
    key
}

/// Key: `[PREFIX_TOURNAMENT_PARTICIPANT | tournament_id(32) | participant(32)]`
pub fn tournament_participant_key(tournament_id: &Hash, participant: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN + Address::LEN);
    key.push(PREFIX_TOURNAMENT_PARTICIPANT);
    key.extend_from_slice(tournament_id.as_bytes());
    key.extend_from_slice(participant.as_bytes());
    key
}

/// Prefix: `[PREFIX_TOURNAMENT_PARTICIPANT | tournament_id(32)]` -- for scanning all participants.
pub fn tournament_participant_prefix(tournament_id: &Hash) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Hash::LEN);
    key.push(PREFIX_TOURNAMENT_PARTICIPANT);
    key.extend_from_slice(tournament_id.as_bytes());
    key
}

// ---------------------------------------------------------------------------
// Supply info key
// ---------------------------------------------------------------------------

/// Key: `[PREFIX_SUPPLY]` -- single key storing the global SupplyInfo.
pub fn supply_info_key() -> Vec<u8> {
    vec![PREFIX_SUPPLY]
}

// ---------------------------------------------------------------------------
// Equivocation evidence keys
// ---------------------------------------------------------------------------

/// Key: `[PREFIX_EQUIVOCATION | validator(32) | height(8 BE)]`
///
/// Stores equivocation evidence for a validator at a given block height.
pub fn equivocation_key(validator: &Address, height: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN + 8);
    key.push(PREFIX_EQUIVOCATION);
    key.extend_from_slice(validator.as_bytes());
    key.extend_from_slice(&height.to_be_bytes());
    key
}

/// Prefix: `[PREFIX_EQUIVOCATION | validator(32)]` -- for scanning all evidence for a validator.
pub fn equivocation_prefix(validator: &Address) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + Address::LEN);
    key.push(PREFIX_EQUIVOCATION);
    key.extend_from_slice(validator.as_bytes());
    key
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_types::{Address, Hash};

    fn addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn hash(byte: u8) -> Hash {
        Hash::new([byte; 32])
    }

    #[test]
    fn keys_have_correct_prefix() {
        assert_eq!(account_key(&addr(1))[0], PREFIX_ACCOUNT);
        assert_eq!(balance_key(&addr(1))[0], PREFIX_BALANCE);
        assert_eq!(asset_class_key(&hash(1))[0], PREFIX_ASSET_CLASS);
        assert_eq!(
            asset_balance_key(&hash(1), &addr(1))[0],
            PREFIX_ASSET_BALANCE
        );
        assert_eq!(validator_key(&addr(1))[0], PREFIX_VALIDATOR);
        assert_eq!(delegation_key(&addr(1), &addr(2))[0], PREFIX_DELEGATION);
        assert_eq!(listing_key(&hash(1))[0], PREFIX_LISTING);
        assert_eq!(profile_key(&addr(1))[0], PREFIX_PROFILE);
        assert_eq!(
            achievement_key(&addr(1), "first_win")[0],
            PREFIX_ACHIEVEMENT
        );
        assert_eq!(attestor_key(&addr(1))[0], PREFIX_ATTESTOR);
        assert_eq!(match_result_key(&hash(1))[0], PREFIX_MATCH_RESULT);
        assert_eq!(match_settlement_key(&hash(1))[0], PREFIX_MATCH_SETTLEMENT);
        assert_eq!(receipt_key(&hash(1))[0], PREFIX_RECEIPT);
        assert_eq!(tx_block_index_key(&hash(1))[0], PREFIX_TX_BLOCK_INDEX);
        assert_eq!(block_events_key(42)[0], PREFIX_BLOCK_EVENTS);
        assert_eq!(chain_height_key()[0], PREFIX_CHAIN_META);
        assert_eq!(latest_hash_key()[0], PREFIX_CHAIN_META);
        assert_eq!(block_key(42)[0], PREFIX_BLOCK);
    }

    #[test]
    fn key_lengths_are_deterministic() {
        assert_eq!(account_key(&addr(1)).len(), 1 + 32);
        assert_eq!(balance_key(&addr(1)).len(), 1 + 32);
        assert_eq!(asset_class_key(&hash(1)).len(), 1 + 32);
        assert_eq!(asset_balance_key(&hash(1), &addr(1)).len(), 1 + 32 + 32);
        assert_eq!(validator_key(&addr(1)).len(), 1 + 32);
        assert_eq!(delegation_key(&addr(1), &addr(2)).len(), 1 + 32 + 32);
        assert_eq!(listing_key(&hash(1)).len(), 1 + 32);
        assert_eq!(profile_key(&addr(1)).len(), 1 + 32);
        assert_eq!(attestor_key(&addr(1)).len(), 1 + 32);
        assert_eq!(match_result_key(&hash(1)).len(), 1 + 32);
        assert_eq!(match_settlement_key(&hash(1)).len(), 1 + 32);
        assert_eq!(receipt_key(&hash(1)).len(), 1 + 32);
        assert_eq!(tx_block_index_key(&hash(1)).len(), 1 + 32);
        assert_eq!(block_events_key(0).len(), 1 + 8);
        assert_eq!(block_key(0).len(), 1 + 8);
    }

    #[test]
    fn different_addresses_produce_different_keys() {
        assert_ne!(account_key(&addr(1)), account_key(&addr(2)));
    }

    #[test]
    fn different_namespaces_never_collide() {
        let a = account_key(&addr(1));
        let b = balance_key(&addr(1));
        assert_ne!(a, b);
    }

    #[test]
    fn block_keys_sort_by_height() {
        let k0 = block_key(0);
        let k1 = block_key(1);
        let k_max = block_key(u64::MAX);
        assert!(k0 < k1);
        assert!(k1 < k_max);
    }

    #[test]
    fn block_events_keys_sort_by_height() {
        let k0 = block_events_key(0);
        let k1 = block_events_key(1);
        let k_max = block_events_key(u64::MAX);
        assert!(k0 < k1);
        assert!(k1 < k_max);
    }

    #[test]
    fn epoch_keys_have_correct_prefix() {
        assert_eq!(epoch_key(0)[0], PREFIX_EPOCH);
        assert_eq!(active_validator_set_key()[0], PREFIX_ACTIVE_VALIDATOR_SET);
    }

    #[test]
    fn epoch_keys_sort_by_epoch() {
        let k0 = epoch_key(0);
        let k1 = epoch_key(1);
        let k_max = epoch_key(u64::MAX);
        assert!(k0 < k1);
        assert!(k1 < k_max);
    }

    #[test]
    fn epoch_key_length() {
        assert_eq!(epoch_key(0).len(), 1 + 8);
        assert_eq!(active_validator_set_key().len(), 1);
    }

    #[test]
    fn session_keys_have_correct_prefix() {
        assert_eq!(session_key(&addr(1), &addr(2))[0], PREFIX_SESSION);
    }

    #[test]
    fn session_key_length() {
        assert_eq!(session_key(&addr(1), &addr(2)).len(), 1 + 32 + 32);
    }

    #[test]
    fn session_prefix_length() {
        assert_eq!(session_prefix(&addr(1)).len(), 1 + 32);
    }

    #[test]
    fn different_session_addresses_produce_different_keys() {
        assert_ne!(
            session_key(&addr(1), &addr(2)),
            session_key(&addr(1), &addr(3)),
        );
    }
}
