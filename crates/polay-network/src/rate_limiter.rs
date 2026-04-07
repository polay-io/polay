use libp2p::PeerId;
use std::collections::HashMap;
use std::time::Instant;

/// Per-peer rate limiter using a sliding-window counter.
pub struct PeerRateLimiter {
    limits: HashMap<PeerId, PeerLimit>,
    max_messages_per_second: u32,
    max_bytes_per_second: u64,
}

#[derive(Debug)]
struct PeerLimit {
    message_count: u32,
    byte_count: u64,
    window_start: Instant,
}

impl PeerLimit {
    fn new() -> Self {
        Self {
            message_count: 0,
            byte_count: 0,
            window_start: Instant::now(),
        }
    }

    /// Reset counters if the 1-second window has elapsed.
    fn maybe_reset(&mut self) {
        if self.window_start.elapsed().as_secs() >= 1 {
            self.message_count = 0;
            self.byte_count = 0;
            self.window_start = Instant::now();
        }
    }
}

impl PeerRateLimiter {
    pub fn new(max_msg_per_sec: u32, max_bytes_per_sec: u64) -> Self {
        Self {
            limits: HashMap::new(),
            max_messages_per_second: max_msg_per_sec,
            max_bytes_per_second: max_bytes_per_sec,
        }
    }

    /// Check whether a message of `message_size` bytes from `peer_id` should
    /// be allowed. Returns `true` if allowed, `false` if rate-limited.
    pub fn check_rate(&mut self, peer_id: &PeerId, message_size: usize) -> bool {
        let limit = self
            .limits
            .entry(*peer_id)
            .or_insert_with(PeerLimit::new);

        limit.maybe_reset();

        if limit.message_count >= self.max_messages_per_second {
            return false;
        }
        if limit.byte_count.saturating_add(message_size as u64) > self.max_bytes_per_second {
            return false;
        }

        limit.message_count += 1;
        limit.byte_count += message_size as u64;
        true
    }

    /// Remove tracking for a disconnected peer.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.limits.remove(peer_id);
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
    fn allows_under_message_limit() {
        let mut rl = PeerRateLimiter::new(10, 1_000_000);
        let peer = make_peer();

        for _ in 0..10 {
            assert!(rl.check_rate(&peer, 100));
        }
    }

    #[test]
    fn blocks_over_message_limit() {
        let mut rl = PeerRateLimiter::new(5, 1_000_000);
        let peer = make_peer();

        for _ in 0..5 {
            assert!(rl.check_rate(&peer, 100));
        }
        // 6th message should be blocked.
        assert!(!rl.check_rate(&peer, 100));
    }

    #[test]
    fn blocks_over_byte_limit() {
        let mut rl = PeerRateLimiter::new(1000, 500);
        let peer = make_peer();

        assert!(rl.check_rate(&peer, 400));
        // Next message would push total to 800 > 500 limit.
        assert!(!rl.check_rate(&peer, 400));
    }

    #[test]
    fn different_peers_tracked_independently() {
        let mut rl = PeerRateLimiter::new(2, 1_000_000);
        let p1 = make_peer();
        let p2 = make_peer();

        assert!(rl.check_rate(&p1, 100));
        assert!(rl.check_rate(&p1, 100));
        assert!(!rl.check_rate(&p1, 100)); // p1 at limit

        assert!(rl.check_rate(&p2, 100)); // p2 still has budget
    }

    #[test]
    fn remove_peer_clears_state() {
        let mut rl = PeerRateLimiter::new(2, 1_000_000);
        let peer = make_peer();

        assert!(rl.check_rate(&peer, 100));
        assert!(rl.check_rate(&peer, 100));
        assert!(!rl.check_rate(&peer, 100));

        rl.remove_peer(&peer);

        // After removal, the peer gets a fresh window.
        assert!(rl.check_rate(&peer, 100));
    }

    #[test]
    fn window_resets_after_one_second() {
        let mut rl = PeerRateLimiter::new(2, 1_000_000);
        let peer = make_peer();

        assert!(rl.check_rate(&peer, 100));
        assert!(rl.check_rate(&peer, 100));
        assert!(!rl.check_rate(&peer, 100));

        // Manually expire the window by back-dating window_start.
        if let Some(limit) = rl.limits.get_mut(&peer) {
            limit.window_start = Instant::now() - std::time::Duration::from_secs(2);
        }

        // Should be allowed again after window reset.
        assert!(rl.check_rate(&peer, 100));
    }
}
