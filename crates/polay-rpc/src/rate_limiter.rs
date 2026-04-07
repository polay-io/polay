//! Rate limiting for the RPC server.
//!
//! Provides both a global submission throttle and per-IP rate limiting to
//! prevent abuse and DoS attacks on public-facing RPC endpoints.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use parking_lot::RwLock;

// ---------------------------------------------------------------------------
// SubmissionThrottle (global)
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
// IpRateLimiter (per-IP)
// ---------------------------------------------------------------------------

/// Per-IP rate limiter state.
struct IpBucket {
    /// Number of requests in the current window.
    count: u32,
    /// Start of the current window.
    window_start: Instant,
}

/// Per-IP rate limiter for RPC endpoints.
///
/// Tracks request counts per IP address in sliding 1-second windows.
/// Periodically cleans up stale entries to bound memory usage.
pub struct IpRateLimiter {
    /// Maximum requests per second per IP.
    max_per_second: u32,
    /// Maximum concurrent connections per IP.
    max_connections_per_ip: u32,
    /// Per-IP buckets.
    buckets: RwLock<HashMap<String, IpBucket>>,
    /// Last time we cleaned up stale entries.
    last_cleanup: RwLock<Instant>,
}

impl IpRateLimiter {
    /// Create a new per-IP rate limiter.
    pub fn new(max_per_second: u32, max_connections_per_ip: u32) -> Self {
        Self {
            max_per_second,
            max_connections_per_ip,
            buckets: RwLock::new(HashMap::new()),
            last_cleanup: RwLock::new(Instant::now()),
        }
    }

    /// Check whether a request from the given IP is allowed.
    pub fn check_and_increment(&self, ip: &str) -> bool {
        let now = Instant::now();
        self.maybe_cleanup(now);

        let mut buckets = self.buckets.write();
        let bucket = buckets.entry(ip.to_string()).or_insert(IpBucket {
            count: 0,
            window_start: now,
        });

        // Reset window if expired.
        if now.duration_since(bucket.window_start).as_secs() >= 1 {
            bucket.count = 0;
            bucket.window_start = now;
        }

        if bucket.count >= self.max_per_second {
            return false;
        }

        bucket.count += 1;
        true
    }

    /// Return the current request count for an IP.
    pub fn current_count(&self, ip: &str) -> u32 {
        let buckets = self.buckets.read();
        buckets.get(ip).map(|b| b.count).unwrap_or(0)
    }

    /// Return the configured max connections per IP.
    pub fn max_connections_per_ip(&self) -> u32 {
        self.max_connections_per_ip
    }

    /// Clean up stale entries older than 60 seconds (at most once per 30s).
    fn maybe_cleanup(&self, now: Instant) {
        let should_clean = {
            let last = *self.last_cleanup.read();
            now.duration_since(last).as_secs() >= 30
        };

        if should_clean {
            let mut last = self.last_cleanup.write();
            // Double-check after lock.
            if now.duration_since(*last).as_secs() >= 30 {
                *last = now;
                let mut buckets = self.buckets.write();
                buckets.retain(|_, b| now.duration_since(b.window_start).as_secs() < 60);
            }
        }
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
