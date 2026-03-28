//! API rate limiting.
//!
//! Two rate limiters:
//! 1. `RateLimiter` — token-bucket per IP (existing, used by HMAC middleware).
//! 2. `UserRateLimiter` — sliding window counter per user_id with per-role limits.
//!    Falls back to IP-based limiting for unauthenticated requests.
//!
//! Per-role limits (requests per minute):
//! - Viewer:  30
//! - Editor:  60
//! - Signer: 100
//! - Admin:  200
//! - Owner:  500

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// IP-based token bucket (existing)
// ---------------------------------------------------------------------------

/// Token bucket rate limiter (IP-based).
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, TokenBucket>>,
    /// Maximum requests per window.
    max_requests: u32,
    /// Window duration.
    window: Duration,
}

struct TokenBucket {
    tokens: u32,
    last_refill: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// `max_requests`: maximum requests per window.
    /// `window`: time window for rate limiting.
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            max_requests,
            window,
        }
    }

    /// Default: 100 requests per minute.
    pub fn default_api() -> Self {
        Self::new(100, Duration::from_secs(60))
    }

    /// Default for signing: 10 requests per minute.
    pub fn default_signing() -> Self {
        Self::new(10, Duration::from_secs(60))
    }

    /// Check if a request is allowed for the given key.
    /// Returns Ok(remaining) if allowed, Err(retry_after) if rate limited.
    pub fn check(&self, key: &str) -> Result<u32, Duration> {
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();

        let bucket = buckets.entry(key.to_string()).or_insert(TokenBucket {
            tokens: self.max_requests,
            last_refill: now,
        });

        // Refill tokens if window has passed
        if now.duration_since(bucket.last_refill) >= self.window {
            bucket.tokens = self.max_requests;
            bucket.last_refill = now;
        }

        if bucket.tokens > 0 {
            bucket.tokens -= 1;
            Ok(bucket.tokens)
        } else {
            let retry_after = self.window - now.duration_since(bucket.last_refill);
            Err(retry_after)
        }
    }

    /// Clean up expired buckets.
    pub fn cleanup(&self) {
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();
        buckets.retain(|_, bucket| {
            now.duration_since(bucket.last_refill) < self.window * 2
        });
    }
}

// ---------------------------------------------------------------------------
// Per-user sliding window rate limiter
// ---------------------------------------------------------------------------

/// Per-role rate limit (requests per minute).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleLimit {
    pub max_per_minute: u32,
}

impl RoleLimit {
    /// Limits indexed by role ordinal: Viewer=0, Editor=1, Signer=2, Admin=3, Owner=4.
    pub fn for_role_ordinal(ordinal: u8) -> Self {
        let max = match ordinal {
            0 => 30,   // Viewer
            1 => 60,   // Editor
            2 => 100,  // Signer
            3 => 200,  // Admin
            _ => 500,  // Owner (4) or above
        };
        Self { max_per_minute: max }
    }

    pub fn viewer() -> Self { Self { max_per_minute: 30 } }
    pub fn editor() -> Self { Self { max_per_minute: 60 } }
    pub fn signer() -> Self { Self { max_per_minute: 100 } }
    pub fn admin() -> Self { Self { max_per_minute: 200 } }
    pub fn owner() -> Self { Self { max_per_minute: 500 } }
}

/// Sliding window counter entry.
struct SlidingWindow {
    /// Timestamps of each request within the current window.
    timestamps: Vec<Instant>,
}

impl SlidingWindow {
    fn new() -> Self {
        Self { timestamps: Vec::new() }
    }

    /// Prune timestamps older than `window` and check whether a new request is allowed.
    ///
    /// Returns `Ok(remaining)` if allowed (the timestamp is recorded), or
    /// `Err(retry_after_secs)` if the limit has been reached.
    fn check(&mut self, limit: u32, window: Duration) -> Result<u32, u64> {
        let now = Instant::now();
        let cutoff = now.checked_sub(window).unwrap_or(now);

        // Prune old entries
        self.timestamps.retain(|t| *t >= cutoff);

        let count = self.timestamps.len() as u32;
        if count < limit {
            self.timestamps.push(now);
            Ok(limit - count - 1)
        } else {
            // Retry after the oldest request falls outside the window
            let oldest = self.timestamps.first().copied().unwrap_or(now);
            let window_end = oldest + window;
            let retry = window_end.checked_duration_since(now).unwrap_or(Duration::ZERO);
            Err(retry.as_secs().max(1))
        }
    }
}

/// Per-user sliding window rate limiter.
///
/// Keys are either `"user:{uuid}"` for authenticated requests or
/// `"ip:{ip}"` for unauthenticated ones.
pub struct UserRateLimiter {
    windows: Mutex<HashMap<String, SlidingWindow>>,
    window: Duration,
}

impl UserRateLimiter {
    pub fn new(window: Duration) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            window,
        }
    }

    /// Default: 1-minute sliding window.
    pub fn default_user() -> Self {
        Self::new(Duration::from_secs(60))
    }

    /// Check whether `key` is within `limit`.
    ///
    /// Returns `Ok((remaining, limit))` if allowed, or `Err(retry_after_secs)` if blocked.
    pub fn check(&self, key: &str, limit: u32) -> Result<(u32, u32), u64> {
        let window = self.window;
        let mut map = self.windows.lock().unwrap();
        let entry = map.entry(key.to_string()).or_insert_with(SlidingWindow::new);
        match entry.check(limit, window) {
            Ok(remaining) => Ok((remaining, limit)),
            Err(retry) => Err(retry),
        }
    }

    /// Evict windows that have been idle longer than 2× window.
    pub fn cleanup(&self) {
        let mut map = self.windows.lock().unwrap();
        let now = Instant::now();
        let two_windows = self.window * 2;
        map.retain(|_, sw| {
            sw.timestamps
                .last()
                .map(|t| now.duration_since(*t) < two_windows)
                .unwrap_or(false)
        });
    }
}

// ---------------------------------------------------------------------------
// Re-export UserRateLimiter helper used in middleware
// ---------------------------------------------------------------------------

/// Build the bucket key for a user_id-keyed request.
pub fn user_key(user_id: &str) -> String {
    format!("user:{user_id}")
}

/// Build the bucket key for an IP-keyed (unauthenticated) request.
pub fn ip_key(ip: &str) -> String {
    format!("ip:{ip}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- IP-based token bucket (existing) ---

    #[test]
    fn allows_within_limit() {
        let limiter = RateLimiter::new(5, Duration::from_secs(60));

        for i in (0..5).rev() {
            assert_eq!(limiter.check("client1"), Ok(i));
        }
    }

    #[test]
    fn blocks_over_limit() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));

        assert!(limiter.check("client1").is_ok());
        assert!(limiter.check("client1").is_ok());
        assert!(limiter.check("client1").is_err()); // 3rd request blocked
    }

    #[test]
    fn different_keys_independent() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));

        assert!(limiter.check("client1").is_ok());
        assert!(limiter.check("client1").is_err());
        assert!(limiter.check("client2").is_ok()); // different key, separate bucket
    }

    #[test]
    fn refills_after_window() {
        let limiter = RateLimiter::new(1, Duration::from_millis(10));

        assert!(limiter.check("client1").is_ok());
        assert!(limiter.check("client1").is_err());

        std::thread::sleep(Duration::from_millis(15));
        assert!(limiter.check("client1").is_ok()); // refilled
    }

    #[test]
    fn returns_retry_after() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));

        limiter.check("client1").unwrap();
        let err = limiter.check("client1").unwrap_err();
        assert!(err.as_secs() > 0); // retry after some seconds
    }

    // --- Per-role limits ---

    #[test]
    fn role_limits_correct_values() {
        assert_eq!(RoleLimit::viewer().max_per_minute, 30);
        assert_eq!(RoleLimit::editor().max_per_minute, 60);
        assert_eq!(RoleLimit::signer().max_per_minute, 100);
        assert_eq!(RoleLimit::admin().max_per_minute, 200);
        assert_eq!(RoleLimit::owner().max_per_minute, 500);
    }

    #[test]
    fn role_limit_by_ordinal() {
        assert_eq!(RoleLimit::for_role_ordinal(0).max_per_minute, 30);
        assert_eq!(RoleLimit::for_role_ordinal(1).max_per_minute, 60);
        assert_eq!(RoleLimit::for_role_ordinal(2).max_per_minute, 100);
        assert_eq!(RoleLimit::for_role_ordinal(3).max_per_minute, 200);
        assert_eq!(RoleLimit::for_role_ordinal(4).max_per_minute, 500);
    }

    // --- UserRateLimiter sliding window ---

    #[test]
    fn user_rate_limiter_allows_within_limit() {
        let limiter = UserRateLimiter::new(Duration::from_secs(60));
        let limit = 3u32;
        let key = user_key("user-abc");

        let (rem, lim) = limiter.check(&key, limit).expect("first allowed");
        assert_eq!(lim, 3);
        assert_eq!(rem, 2);

        limiter.check(&key, limit).expect("second allowed");
        limiter.check(&key, limit).expect("third allowed");
        let err = limiter.check(&key, limit).expect_err("fourth blocked");
        assert!(err >= 1);
    }

    #[test]
    fn user_rate_limiter_different_users_independent() {
        let limiter = UserRateLimiter::new(Duration::from_secs(60));
        let key_a = user_key("user-a");
        let key_b = user_key("user-b");

        limiter.check(&key_a, 1).expect("a allowed");
        limiter.check(&key_a, 1).expect_err("a blocked");
        limiter.check(&key_b, 1).expect("b independent — allowed");
    }

    #[test]
    fn user_rate_limiter_sliding_window_expires() {
        let limiter = UserRateLimiter::new(Duration::from_millis(20));
        let key = user_key("user-slide");

        limiter.check(&key, 1).expect("first allowed");
        limiter.check(&key, 1).expect_err("second blocked");

        std::thread::sleep(Duration::from_millis(25));
        // Old timestamp has slid out of the window
        limiter.check(&key, 1).expect("allowed after window expired");
    }

    #[test]
    fn ip_fallback_key_format() {
        assert_eq!(ip_key("1.2.3.4"), "ip:1.2.3.4");
        assert_eq!(user_key("some-uuid"), "user:some-uuid");
    }

    #[test]
    fn viewer_limit_lower_than_owner() {
        let limiter = UserRateLimiter::new(Duration::from_secs(60));
        let viewer_limit = RoleLimit::viewer().max_per_minute;
        let owner_limit = RoleLimit::owner().max_per_minute;

        // Exhaust viewer limit
        let key = user_key("viewer-test");
        for _ in 0..viewer_limit {
            limiter.check(&key, viewer_limit).expect("within viewer limit");
        }
        limiter.check(&key, viewer_limit).expect_err("viewer exceeded");

        // Owner with same request count is still allowed (different key, different limit)
        let key_owner = user_key("owner-test");
        for _ in 0..viewer_limit {
            limiter.check(&key_owner, owner_limit).expect("owner within limit");
        }
        limiter.check(&key_owner, owner_limit).expect("owner still has headroom");
    }

    #[test]
    fn limit_exceeded_returns_retry_after_at_least_one_second() {
        let limiter = UserRateLimiter::new(Duration::from_secs(60));
        let key = user_key("retry-test");
        limiter.check(&key, 1).unwrap();
        let retry_secs = limiter.check(&key, 1).unwrap_err();
        assert!(retry_secs >= 1, "retry_after must be at least 1 second, got {retry_secs}");
    }
}
