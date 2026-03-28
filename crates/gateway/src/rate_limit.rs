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
//!
//! ## DB persistence
//!
//! `UserRateLimiter` optionally persists counters to PostgreSQL (migration 019).
//! The in-memory `SlidingWindow` is the fast path; the DB is written in batches
//! by a background task spawned via `start_sync_task`. On startup, call
//! `load_from_db` to restore counters that survived a restart.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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
///
/// The limiter optionally persists counter state to PostgreSQL so that a
/// service restart does not reset all windows to zero (avoiding a brief
/// rate-limit bypass window on restart).  Set a `db_pool` via
/// `with_db_pool` and call `load_from_db` on startup, then spawn the
/// background sync task with `start_sync_task`.
pub struct UserRateLimiter {
    windows: Mutex<HashMap<String, SlidingWindow>>,
    window: Duration,
    /// Set of keys that have been modified since the last DB flush.
    dirty: Mutex<std::collections::HashSet<String>>,
    /// Optional DB pool for persistence.
    db_pool: Option<sqlx::PgPool>,
}

impl UserRateLimiter {
    pub fn new(window: Duration) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            window,
            dirty: Mutex::new(std::collections::HashSet::new()),
            db_pool: None,
        }
    }

    /// Default: 1-minute sliding window.
    pub fn default_user() -> Self {
        Self::new(Duration::from_secs(60))
    }

    /// Attach a DB pool for persistent counter storage.
    pub fn with_db_pool(mut self, pool: sqlx::PgPool) -> Self {
        self.db_pool = Some(pool);
        self
    }

    /// Check whether `key` is within `limit`.
    ///
    /// Returns `Ok((remaining, limit))` if allowed, or `Err(retry_after_secs)` if blocked.
    /// Marks the key dirty for the next DB sync cycle.
    pub fn check(&self, key: &str, limit: u32) -> Result<(u32, u32), u64> {
        let window = self.window;
        let mut map = self.windows.lock().unwrap();
        let entry = map.entry(key.to_string()).or_insert_with(SlidingWindow::new);
        let result = match entry.check(limit, window) {
            Ok(remaining) => Ok((remaining, limit)),
            Err(retry) => Err(retry),
        };
        // Mark dirty regardless of allow/deny — the count changed either way.
        if let Ok(mut dirty) = self.dirty.lock() {
            dirty.insert(key.to_string());
        }
        result
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

    /// Load persisted counters from the DB on startup.
    ///
    /// Only restores counters whose window is still active (not expired).
    /// Returns the number of keys restored.
    pub async fn load_from_db(&self) -> anyhow::Result<usize> {
        let Some(ref pool) = self.db_pool else {
            return Ok(0);
        };
        let window_secs = self.window.as_secs() as i64;

        // Use the non-macro query API so this compiles without DATABASE_URL.
        let rows = sqlx::query(
            r#"
            SELECT key, count, window_start
            FROM rate_limit_counters
            WHERE window_seconds = $1
              AND window_start > NOW() - ($1 || ' seconds')::INTERVAL
            "#,
        )
        .bind(window_secs as i32)
        .fetch_all(pool)
        .await?;

        let count = rows.len();
        let mut map = self.windows.lock().unwrap();
        let now = Instant::now();
        for row in rows {
            use sqlx::Row;
            let key: String = row.try_get("key")?;
            let hit_count: i32 = row.try_get("count")?;
            let window_start: chrono::DateTime<chrono::Utc> = row.try_get("window_start")?;

            // Compute how long ago the window started.
            let age_secs = chrono::Utc::now()
                .signed_duration_since(window_start)
                .num_seconds()
                .max(0) as u64;

            let entry = map.entry(key).or_insert_with(SlidingWindow::new);
            let hits = hit_count.max(0) as usize;
            // Place synthetic timestamps at window_start offset so they
            // expire naturally at the correct time.
            let fake_ts = now.checked_sub(Duration::from_secs(age_secs)).unwrap_or(now);
            for _ in 0..hits {
                entry.timestamps.push(fake_ts);
            }
        }
        Ok(count)
    }

    /// Flush dirty counters to the DB.
    ///
    /// Writes counts as of this instant; the window_start is approximated
    /// as `NOW() - elapsed_since_first_entry`.
    pub async fn flush_to_db(&self) -> anyhow::Result<usize> {
        let Some(ref pool) = self.db_pool else {
            return Ok(0);
        };

        // Snapshot which keys are dirty then clear the set atomically.
        let dirty_keys: Vec<String> = {
            let mut d = self.dirty.lock().unwrap();
            d.drain().collect()
        };

        if dirty_keys.is_empty() {
            return Ok(0);
        }

        let window_secs = self.window.as_secs() as i32;
        let now = Instant::now();
        let mut flushed = 0usize;

        // Snapshot counter values before any awaits (lock must not be held across await).
        // Each element is either Some((count, offset)) for an upsert or None for a delete.
        let snapshots: Vec<(String, Option<(i32, u64)>)> = {
            let map = self.windows.lock().unwrap();
            dirty_keys.iter().map(|key| {
                let payload = map.get(key.as_str()).map(|sw| {
                    let count = sw.timestamps.len() as i32;
                    let offset = sw.timestamps.first()
                        .map(|t| now.duration_since(*t).as_secs())
                        .unwrap_or(0);
                    (count, offset)
                });
                (key.clone(), payload)
            }).collect()
        };
        // Lock is released here — safe to await below.

        for (key, payload) in snapshots {
            match payload {
                None => {
                    // Key was evicted — remove from DB.
                    let _ = sqlx::query(
                        "DELETE FROM rate_limit_counters WHERE key = $1",
                    )
                    .bind(&key)
                    .execute(pool)
                    .await;
                }
                Some((count, offset_secs)) => {
                    let window_start = chrono::Utc::now()
                        - chrono::Duration::seconds(offset_secs as i64);

                    sqlx::query(
                        r#"
                        INSERT INTO rate_limit_counters (key, count, window_start, window_seconds)
                        VALUES ($1, $2, $3, $4)
                        ON CONFLICT (key) DO UPDATE
                            SET count = EXCLUDED.count,
                                window_start = EXCLUDED.window_start,
                                window_seconds = EXCLUDED.window_seconds
                        "#,
                    )
                    .bind(&key)
                    .bind(count)
                    .bind(window_start)
                    .bind(window_secs)
                    .execute(pool)
                    .await?;

                    flushed += 1;
                }
            }
        }

        Ok(flushed)
    }

    /// Spawn a background task that syncs dirty counters to the DB every
    /// `interval` seconds.  Call this once after constructing the limiter.
    pub fn start_sync_task(self: Arc<Self>, interval: Duration) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                match self.flush_to_db().await {
                    Ok(n) if n > 0 => {
                        tracing::debug!(keys = n, "Rate limit counters flushed to DB");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Rate limit DB flush failed");
                    }
                }
                self.cleanup();
            }
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
