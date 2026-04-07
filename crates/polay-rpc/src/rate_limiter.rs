//! Rate limiting for the RPC server.
//!
//! Provides a global submission throttle that tracks the number of transaction
//! submissions per second to prevent abuse and DoS attacks.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use parking_lot::RwLock;

// ---------------------------------------------------------------------------
// SubmissionThrottle
// ---------------------------------------------------------------------------

/// A simple global sliding-window submission throttle.
///
/// Tracks the number of transaction submissions within the current one-second
/// window. If the limit is exceeded, further submissions are rejected until the
/// window resets.
pub struct SubmissionThrottle {
    /// Maximum submissions allowed per second.
    max_per_second: u32,
    /// Counter for the current window.
    counter: AtomicU32,
    /// Start of the current window.
    window_start: RwLock<Instant>,
}

impl SubmissionThrottle {
    /// Create a new throttle with the given per-second limit.
    pub fn new(max_per_second: u32) -> Self {
        Self {
            max_per_second,
            counter: AtomicU32::new(0),
            window_start: RwLock::new(Instant::now()),
        }
    }

    /// Check whether a submission is allowed.
    ///
    /// Returns `true` if the submission is permitted, `false` if it should be
    /// rejected due to rate limiting.
    pub fn check_and_increment(&self) -> bool {
        let now = Instant::now();

        // Check if we need to reset the window.
        {
            let window_start = *self.window_start.read();
            if now.duration_since(window_start).as_secs() >= 1 {
                // Reset the window. We use a write lock briefly.
                let mut ws = self.window_start.write();
                // Double-check after acquiring write lock to avoid races.
                if now.duration_since(*ws).as_secs() >= 1 {
                    *ws = now;
                    self.counter.store(0, Ordering::Relaxed);
                }
            }
        }

        // Try to increment the counter.
        let prev = self.counter.fetch_add(1, Ordering::Relaxed);
        if prev >= self.max_per_second {
            // We went over the limit; undo the increment.
            self.counter.fetch_sub(1, Ordering::Relaxed);
            return false;
        }

        true
    }

    /// Return the current count in the active window.
    pub fn current_count(&self) -> u32 {
        self.counter.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_allows_within_limit() {
        let throttle = SubmissionThrottle::new(5);
        for _ in 0..5 {
            assert!(throttle.check_and_increment());
        }
    }

    #[test]
    fn throttle_rejects_over_limit() {
        let throttle = SubmissionThrottle::new(3);
        assert!(throttle.check_and_increment());
        assert!(throttle.check_and_increment());
        assert!(throttle.check_and_increment());
        // 4th should be rejected
        assert!(!throttle.check_and_increment());
        assert!(!throttle.check_and_increment());
    }

    #[test]
    fn throttle_counter_tracks_submissions() {
        let throttle = SubmissionThrottle::new(10);
        assert_eq!(throttle.current_count(), 0);
        throttle.check_and_increment();
        assert_eq!(throttle.current_count(), 1);
        throttle.check_and_increment();
        assert_eq!(throttle.current_count(), 2);
    }

    #[test]
    fn throttle_rejects_at_boundary() {
        let throttle = SubmissionThrottle::new(1);
        assert!(throttle.check_and_increment());
        assert!(!throttle.check_and_increment());
    }
}
