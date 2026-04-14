//! Identity module — player profiles, achievements, and reputation.

use polay_state::{StateStore, StateView, StateWriter};
use polay_types::{Achievement, Address, Event, PlayerProfile};
use tracing::debug;

use crate::error::ExecutionError;

// ---------------------------------------------------------------------------
// Create profile
// ---------------------------------------------------------------------------

/// Create a new on-chain player profile for the signer.
pub fn execute_create_profile(
    signer: &Address,
    username: &str,
    display_name: &str,
    metadata: Option<&str>,
    store: &dyn StateStore,
    timestamp: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);

    // Check that no profile exists for this address yet.
    if view.get_profile(signer)?.is_some() {
        return Err(ExecutionError::ProfileAlreadyExists);
    }

    let profile = PlayerProfile::new(
        *signer,
        username.to_string(),
        display_name.to_string(),
        metadata.map(|s| s.to_string()),
        timestamp,
    );

    StateWriter::new(store).set_profile(&profile)?;

    debug!(
        address = %signer,
        username,
        "player profile created"
    );

    Ok(vec![Event::profile_created(signer, username)])
}

// ---------------------------------------------------------------------------
// Add achievement
// ---------------------------------------------------------------------------

/// Award a soulbound achievement to a player.
///
/// Achievements are always soulbound (non-transferable). Only a registered
/// attestor is allowed to award achievements (prevents arbitrary reputation
/// manipulation by untrusted accounts).
pub fn execute_add_achievement(
    signer: &Address,
    player: &Address,
    achievement_id: &str,
    name: &str,
    metadata: &str,
    store: &dyn StateStore,
    timestamp: u64,
) -> Result<Vec<Event>, ExecutionError> {
    // Authorization: signer must be a registered attestor.
    let view = StateView::new(store);
    if view.get_attestor(signer)?.is_none() {
        return Err(ExecutionError::Unauthorized);
    }

    let achievement = Achievement {
        id: achievement_id.to_string(),
        player: *player,
        name: name.to_string(),
        metadata: metadata.to_string(),
        awarded_at: timestamp,
        soulbound: true,
    };

    StateWriter::new(store).set_achievement(&achievement)?;

    debug!(
        player = %player,
        achievement_id,
        name,
        attestor = %signer,
        "achievement awarded"
    );

    Ok(vec![Event::achievement_awarded(
        player,
        achievement_id,
        name,
    )])
}

// ---------------------------------------------------------------------------
// Update reputation
// ---------------------------------------------------------------------------

/// Adjust a player's reputation score.
///
/// Creates a default profile if one does not exist for the player, to allow
/// reputation tracking even before the player explicitly creates a profile.
/// Only a registered attestor is allowed to modify reputation.
pub fn execute_update_reputation(
    signer: &Address,
    player: &Address,
    delta: i64,
    _reason: &str,
    store: &dyn StateStore,
    timestamp: u64,
) -> Result<Vec<Event>, ExecutionError> {
    let view = StateView::new(store);
    let writer = StateWriter::new(store);

    // Authorization: signer must be a registered attestor.
    if view.get_attestor(signer)?.is_none() {
        return Err(ExecutionError::Unauthorized);
    }

    let mut profile = match view.get_profile(player)? {
        Some(p) => p,
        None => {
            // Create a minimal profile so reputation can be tracked.
            PlayerProfile::new(
                *player,
                player.to_hex(),
                "Anonymous".to_string(),
                None,
                timestamp,
            )
        }
    };

    profile.adjust_reputation(delta);
    writer.set_profile(&profile)?;

    debug!(
        player = %player,
        delta,
        new_reputation = profile.reputation,
        attestor = %signer,
        "reputation updated"
    );

    Ok(vec![Event::reputation_changed(
        player,
        delta,
        profile.reputation,
    )])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_state::MemoryStore;
    use polay_types::{Attestor, AttestorStatus};

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    /// Register an attestor so identity operations are authorized.
    fn register_attestor(store: &MemoryStore, addr: &Address) {
        let attestor = Attestor {
            address: *addr,
            game_id: "test-game".to_string(),
            endpoint: "localhost:8080".to_string(),
            metadata: "{}".to_string(),
            status: AttestorStatus::Active,
            registered_at: 0,
        };
        StateWriter::new(store).set_attestor(&attestor).unwrap();
    }

    #[test]
    fn create_profile_happy_path() {
        let store = MemoryStore::new();
        let addr = test_addr(1);

        let events = execute_create_profile(
            &addr,
            "alice",
            "Alice",
            Some("{\"bio\":\"gamer\"}"),
            &store,
            100,
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].module, "identity");
        assert_eq!(events[0].action, "profile_created");

        let profile = StateView::new(&store).get_profile(&addr).unwrap().unwrap();
        assert_eq!(profile.username, "alice");
        assert_eq!(profile.display_name, "Alice");
        assert_eq!(profile.reputation, 0);
    }

    #[test]
    fn create_profile_duplicate() {
        let store = MemoryStore::new();
        let addr = test_addr(1);

        execute_create_profile(&addr, "alice", "Alice", None, &store, 100).unwrap();
        let err = execute_create_profile(&addr, "alice2", "Alice2", None, &store, 200).unwrap_err();
        assert!(matches!(err, ExecutionError::ProfileAlreadyExists));
    }

    #[test]
    fn add_achievement_requires_attestor() {
        let store = MemoryStore::new();
        let non_attestor = test_addr(99);
        let player = test_addr(2);

        let err = execute_add_achievement(&non_attestor, &player, "x", "X", "{}", &store, 500)
            .unwrap_err();
        assert!(matches!(err, ExecutionError::Unauthorized));
    }

    #[test]
    fn add_achievement_happy_path() {
        let store = MemoryStore::new();
        let admin = test_addr(1);
        let player = test_addr(2);
        register_attestor(&store, &admin);

        let events = execute_add_achievement(
            &admin,
            &player,
            "first_win",
            "First Win",
            "{\"rarity\":\"common\"}",
            &store,
            500,
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "achievement_awarded");

        let ach = StateView::new(&store)
            .get_achievement(&player, "first_win")
            .unwrap()
            .unwrap();
        assert_eq!(ach.name, "First Win");
        assert!(ach.soulbound);
        assert_eq!(ach.awarded_at, 500);
    }

    #[test]
    fn update_reputation_requires_attestor() {
        let store = MemoryStore::new();
        let non_attestor = test_addr(99);
        let player = test_addr(2);

        let err = execute_update_reputation(&non_attestor, &player, 10, "reason", &store, 100)
            .unwrap_err();
        assert!(matches!(err, ExecutionError::Unauthorized));
    }

    #[test]
    fn update_reputation_with_existing_profile() {
        let store = MemoryStore::new();
        let admin = test_addr(1);
        let player = test_addr(2);
        register_attestor(&store, &admin);

        execute_create_profile(&player, "bob", "Bob", None, &store, 100).unwrap();

        let events =
            execute_update_reputation(&admin, &player, 10, "good behavior", &store, 200).unwrap();
        assert_eq!(events.len(), 1);

        let profile = StateView::new(&store)
            .get_profile(&player)
            .unwrap()
            .unwrap();
        assert_eq!(profile.reputation, 10);

        // Apply negative delta.
        execute_update_reputation(&admin, &player, -25, "toxic", &store, 300).unwrap();
        let profile = StateView::new(&store)
            .get_profile(&player)
            .unwrap()
            .unwrap();
        assert_eq!(profile.reputation, -15);
    }

    #[test]
    fn update_reputation_auto_creates_profile() {
        let store = MemoryStore::new();
        let admin = test_addr(1);
        let player = test_addr(2);
        register_attestor(&store, &admin);

        // No profile exists yet.
        assert!(StateView::new(&store)
            .get_profile(&player)
            .unwrap()
            .is_none());

        execute_update_reputation(&admin, &player, 5, "reward", &store, 100).unwrap();

        let profile = StateView::new(&store)
            .get_profile(&player)
            .unwrap()
            .unwrap();
        assert_eq!(profile.reputation, 5);
        assert_eq!(profile.display_name, "Anonymous");
    }
}
