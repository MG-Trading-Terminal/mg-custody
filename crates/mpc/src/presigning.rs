//! Presigning cache for low-latency MPC signing.
//!
//! CGGMP21 supports presigning — generating partial signatures ahead of time.
//! When a signing request arrives, we use a cached presignature instead of
//! running the full protocol, reducing latency from seconds to milliseconds.

use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A cached presignature ready for use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPresignature {
    pub id: Uuid,
    pub key_id: String,
    /// Serialized presignature data (protocol-specific).
    pub data: Vec<u8>,
    pub created_at: DateTime<Utc>,
    /// Presignatures expire after this time (security measure).
    pub expires_at: DateTime<Utc>,
}

/// Presignature cache per key group.
pub struct PresigningCache {
    cache: Mutex<VecDeque<CachedPresignature>>,
    /// Target number of presignatures to maintain.
    target_count: usize,
    /// Presignature lifetime.
    ttl_secs: u64,
}

impl PresigningCache {
    pub fn new(target_count: usize, ttl_secs: u64) -> Self {
        Self {
            cache: Mutex::new(VecDeque::new()),
            target_count,
            ttl_secs,
        }
    }

    /// Default: keep 10 presignatures, 5-minute TTL.
    pub fn with_defaults() -> Self {
        Self::new(10, 300)
    }

    /// Add a presignature to the cache.
    pub fn push(&self, key_id: &str, data: Vec<u8>) {
        let now = Utc::now();
        let presig = CachedPresignature {
            id: Uuid::new_v4(),
            key_id: key_id.to_string(),
            data,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(self.ttl_secs as i64),
        };

        let mut cache = self.cache.lock().unwrap();
        cache.push_back(presig);

        // Trim if over capacity
        while cache.len() > self.target_count * 2 {
            cache.pop_front();
        }
    }

    /// Take a presignature from the cache (FIFO, removes expired).
    pub fn take(&self, key_id: &str) -> Option<CachedPresignature> {
        let mut cache = self.cache.lock().unwrap();
        let now = Utc::now();

        // Remove expired entries
        cache.retain(|p| p.expires_at > now);

        // Find and remove first matching
        cache.iter().position(|p| p.key_id == key_id).map(|pos| cache.remove(pos).unwrap())
    }

    /// Number of available presignatures for a key.
    pub fn available(&self, key_id: &str) -> usize {
        let cache = self.cache.lock().unwrap();
        let now = Utc::now();
        cache.iter().filter(|p| p.key_id == key_id && p.expires_at > now).count()
    }

    /// Whether the cache needs refilling.
    pub fn needs_refill(&self, key_id: &str) -> bool {
        self.available(key_id) < self.target_count / 2
    }

    /// Total cached presignatures.
    pub fn total_count(&self) -> usize {
        self.cache.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_take() {
        let cache = PresigningCache::with_defaults();
        cache.push("key1", vec![1, 2, 3]);
        cache.push("key1", vec![4, 5, 6]);

        let p = cache.take("key1").unwrap();
        assert_eq!(p.data, vec![1, 2, 3]); // FIFO
        assert_eq!(cache.available("key1"), 1);
    }

    #[test]
    fn take_returns_none_for_missing_key() {
        let cache = PresigningCache::with_defaults();
        assert!(cache.take("nonexistent").is_none());
    }

    #[test]
    fn expired_entries_removed() {
        let cache = PresigningCache::new(10, 0); // 0 second TTL = instant expire
        cache.push("key1", vec![1]);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(cache.take("key1").is_none()); // expired
    }

    #[test]
    fn needs_refill_detection() {
        let cache = PresigningCache::new(10, 300);
        assert!(cache.needs_refill("key1")); // 0 < 5

        for i in 0..6 {
            cache.push("key1", vec![i]);
        }
        assert!(!cache.needs_refill("key1")); // 6 >= 5
    }
}
