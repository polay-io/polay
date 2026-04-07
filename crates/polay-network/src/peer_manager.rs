use libp2p::PeerId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const DEFAULT_SCORE: i32 = 100;
pub const BAN_THRESHOLD: i32 = -100;
pub const MAX_PEERS: usize = 50;
pub const MIN_PEERS: usize = 4;

/// Per-message score adjustment for a valid message.
const GOOD_MESSAGE_SCORE: i32 = 1;
/// Per-message score penalty for an invalid / malformed message.
const BAD_MESSAGE_PENALTY: i32 = -20;

/// Metadata tracked per connected peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub score: i32,
    pub connected_at: Instant,
    pub last_message: Instant,
    pub messages_received: u64,
    pub invalid_messages: u64,
    pub bytes_received: u64,
}

impl PeerInfo {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            score: DEFAULT_SCORE,
            connected_at: now,
            last_message: now,
            messages_received: 0,
            invalid_messages: 0,
            bytes_received: 0,
        }
    }
}

/// Record of a banned peer.
#[derive(Debug, Clone)]
pub struct BanRecord {
    pub peer_id: PeerId,
    pub reason: String,
    pub banned_at: Instant,
    /// `None` means the ban is permanent.
    pub expires_at: Option<Instant>,
}

/// Manages connected peer metadata, scoring, and banning.
pub struct PeerManager {
    peers: HashMap<PeerId, PeerInfo>,
    banned: HashMap<PeerId, BanRecord>,
    max_peers: usize,
    min_peers: usize,
}

impl PeerManager {
    pub fn new(max_peers: usize, min_peers: usize) -> Self {
        Self {
            peers: HashMap::new(),
            banned: HashMap::new(),
            max_peers,
            min_peers,
        }
    }

    /// Record a new peer connection. Returns `false` if the peer is banned or
    /// we are at capacity.
    pub fn on_peer_connected(&mut self, peer_id: PeerId) -> bool {
        if self.is_banned(&peer_id) {
            return false;
        }
        if self.peers.len() >= self.max_peers {
            return false;
        }
        self.peers.entry(peer_id).or_insert_with(PeerInfo::new);
        true
    }

    /// Record peer disconnection.
    pub fn on_peer_disconnected(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
    }

    /// Record a valid message from a peer (increases score).
    pub fn record_good_message(&mut self, peer_id: &PeerId) {
        if let Some(info) = self.peers.get_mut(peer_id) {
            info.score = info.score.saturating_add(GOOD_MESSAGE_SCORE);
            info.messages_received += 1;
            info.last_message = Instant::now();
        }
    }

    /// Record an invalid / malformed message (decreases score, may trigger a
    /// ban). Returns a `BanRecord` if the peer was banned as a result.
    pub fn record_bad_message(&mut self, peer_id: &PeerId) -> Option<BanRecord> {
        if let Some(info) = self.peers.get_mut(peer_id) {
            info.score = info.score.saturating_add(BAD_MESSAGE_PENALTY);
            info.invalid_messages += 1;
            info.last_message = Instant::now();

            if info.score <= BAN_THRESHOLD {
                let record = BanRecord {
                    peer_id: *peer_id,
                    reason: format!(
                        "score {} <= ban threshold {} ({} invalid messages)",
                        info.score, BAN_THRESHOLD, info.invalid_messages
                    ),
                    banned_at: Instant::now(),
                    expires_at: Some(Instant::now() + Duration::from_secs(3600)),
                };
                self.peers.remove(peer_id);
                self.banned.insert(*peer_id, record.clone());
                return Some(record);
            }
        }
        None
    }

    /// Check if a peer is banned.
    pub fn is_banned(&self, peer_id: &PeerId) -> bool {
        if let Some(record) = self.banned.get(peer_id) {
            match record.expires_at {
                Some(expires) => Instant::now() < expires,
                None => true, // permanent ban
            }
        } else {
            false
        }
    }

    /// Ban a peer with a reason and optional duration.
    pub fn ban_peer(
        &mut self,
        peer_id: PeerId,
        reason: String,
        duration: Option<Duration>,
    ) -> BanRecord {
        self.peers.remove(&peer_id);
        let record = BanRecord {
            peer_id,
            reason,
            banned_at: Instant::now(),
            expires_at: duration.map(|d| Instant::now() + d),
        };
        self.banned.insert(peer_id, record.clone());
        record
    }

    /// Remove bans that have expired. Returns the list of unbanned peer IDs.
    pub fn cleanup_expired_bans(&mut self) -> Vec<PeerId> {
        let now = Instant::now();
        let expired: Vec<PeerId> = self
            .banned
            .iter()
            .filter_map(|(peer_id, record)| match record.expires_at {
                Some(expires) if expires <= now => Some(*peer_id),
                _ => None,
            })
            .collect();
        for peer_id in &expired {
            self.banned.remove(peer_id);
        }
        expired
    }

    /// Get connected peer count.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Returns `true` if we have reached the maximum number of connected peers.
    pub fn is_at_capacity(&self) -> bool {
        self.peers.len() >= self.max_peers
    }

    /// Returns `true` if we are below the minimum desired peer count.
    pub fn needs_more_peers(&self) -> bool {
        self.peers.len() < self.min_peers
    }

    /// Get the lowest-scoring peer (candidate for eviction when at capacity).
    pub fn lowest_scoring_peer(&self) -> Option<PeerId> {
        self.peers
            .iter()
            .min_by_key(|(_, info)| info.score)
            .map(|(peer_id, _)| *peer_id)
    }

    /// Get a peer's score.
    pub fn get_score(&self, peer_id: &PeerId) -> Option<i32> {
        self.peers.get(peer_id).map(|info| info.score)
    }

    /// Get the maximum peer capacity.
    pub fn max_peers(&self) -> usize {
        self.max_peers
    }

    /// Get the minimum desired peer count.
    pub fn min_peers(&self) -> usize {
        self.min_peers
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_peer() -> PeerId {
        PeerId::random()
    }

    #[test]
    fn connect_and_disconnect() {
        let mut pm = PeerManager::new(50, 4);
        let peer = make_peer();

        assert!(pm.on_peer_connected(peer));
        assert_eq!(pm.peer_count(), 1);

        pm.on_peer_disconnected(&peer);
        assert_eq!(pm.peer_count(), 0);
    }

    #[test]
    fn reject_when_at_capacity() {
        let mut pm = PeerManager::new(2, 1);
        let p1 = make_peer();
        let p2 = make_peer();
        let p3 = make_peer();

        assert!(pm.on_peer_connected(p1));
        assert!(pm.on_peer_connected(p2));
        assert!(!pm.on_peer_connected(p3));
        assert!(pm.is_at_capacity());
    }

    #[test]
    fn good_message_increases_score() {
        let mut pm = PeerManager::new(50, 4);
        let peer = make_peer();
        pm.on_peer_connected(peer);

        let initial = pm.get_score(&peer).unwrap();
        pm.record_good_message(&peer);
        assert!(pm.get_score(&peer).unwrap() > initial);
    }

    #[test]
    fn bad_messages_decrease_score_and_eventually_ban() {
        let mut pm = PeerManager::new(50, 4);
        let peer = make_peer();
        pm.on_peer_connected(peer);

        // Each bad message subtracts BAD_MESSAGE_PENALTY (20).
        // Starting score is 100, ban at -100, so we need
        // ceil((100 + 100) / 20) = 10 bad messages.
        for _ in 0..10 {
            let ban = pm.record_bad_message(&peer);
            if ban.is_some() {
                assert!(pm.is_banned(&peer));
                assert_eq!(pm.peer_count(), 0);
                return;
            }
        }
        // After 10 bad messages the score should be 100 - 200 = -100 which
        // equals the threshold. The ban fires when score <= threshold, so it
        // should have been triggered.
        panic!("peer should have been banned");
    }

    #[test]
    fn ban_peer_directly() {
        let mut pm = PeerManager::new(50, 4);
        let peer = make_peer();
        pm.on_peer_connected(peer);

        let record = pm.ban_peer(peer, "test ban".into(), Some(Duration::from_secs(60)));
        assert_eq!(record.peer_id, peer);
        assert!(pm.is_banned(&peer));
        assert_eq!(pm.peer_count(), 0);
    }

    #[test]
    fn permanent_ban() {
        let mut pm = PeerManager::new(50, 4);
        let peer = make_peer();
        pm.on_peer_connected(peer);

        pm.ban_peer(peer, "permanent".into(), None);
        assert!(pm.is_banned(&peer));

        // Cleanup should not remove permanent bans.
        let unbanned = pm.cleanup_expired_bans();
        assert!(unbanned.is_empty());
        assert!(pm.is_banned(&peer));
    }

    #[test]
    fn banned_peer_cannot_reconnect() {
        let mut pm = PeerManager::new(50, 4);
        let peer = make_peer();
        pm.on_peer_connected(peer);
        pm.ban_peer(peer, "nope".into(), Some(Duration::from_secs(3600)));

        assert!(!pm.on_peer_connected(peer));
    }

    #[test]
    fn cleanup_expired_bans() {
        let mut pm = PeerManager::new(50, 4);
        let peer = make_peer();

        // Ban with zero duration so it expires immediately.
        let record = BanRecord {
            peer_id: peer,
            reason: "expired test".into(),
            banned_at: Instant::now(),
            expires_at: Some(Instant::now() - Duration::from_secs(1)),
        };
        pm.banned.insert(peer, record);

        assert!(!pm.is_banned(&peer)); // already expired
        let unbanned = pm.cleanup_expired_bans();
        assert_eq!(unbanned.len(), 1);
        assert_eq!(unbanned[0], peer);
    }

    #[test]
    fn lowest_scoring_peer_returns_weakest() {
        let mut pm = PeerManager::new(50, 4);
        let p1 = make_peer();
        let p2 = make_peer();
        let p3 = make_peer();

        pm.on_peer_connected(p1);
        pm.on_peer_connected(p2);
        pm.on_peer_connected(p3);

        // Lower p2's score.
        pm.record_bad_message(&p2);
        pm.record_bad_message(&p2);

        assert_eq!(pm.lowest_scoring_peer(), Some(p2));
    }

    #[test]
    fn needs_more_peers_logic() {
        let mut pm = PeerManager::new(50, 4);
        assert!(pm.needs_more_peers());

        for _ in 0..4 {
            pm.on_peer_connected(make_peer());
        }
        assert!(!pm.needs_more_peers());
    }

    #[test]
    fn duplicate_connect_is_idempotent() {
        let mut pm = PeerManager::new(50, 4);
        let peer = make_peer();

        assert!(pm.on_peer_connected(peer));
        assert!(pm.on_peer_connected(peer)); // same peer again
        assert_eq!(pm.peer_count(), 1);
    }
}
