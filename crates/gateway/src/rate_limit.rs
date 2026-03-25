//! API rate limiting.
//!
//! Token bucket rate limiter per API key / IP address.
//! Prevents abuse and protects the signing service from overload.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token bucket rate limiter.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
