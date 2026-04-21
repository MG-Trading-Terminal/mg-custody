//! Per-user cooldown for operator-driven state mutations.
//!
//! Chaos injections, silence creations, and similar admin ops
//! should not be spammable — an operator running a shell loop by
//! accident would paint the dashboard with noise, exhaust audit
//! budget, and potentially page the oncall each tick. This
//! provides a dead-simple one-op-per-N-seconds cooldown keyed by
//! `(user_id, op_bucket)`.
//!
//! Not a replacement for the workspace-wide RateLimiter — that
//! handles request-per-minute sliding windows. This is a tight
//! per-button debounce; 10s is the typical cooldown we use here.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct AdminCooldown {
    last_fired: Mutex<HashMap<String, Instant>>,
}

impl AdminCooldown {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return `Ok(())` when the caller may proceed. Returns
    /// `Err(remaining_secs)` when the cooldown is still active —
    /// the caller renders 429 with a Retry-After of that many
    /// seconds.
    ///
    /// `key` should uniquely identify the (user, operation) pair,
    /// e.g. `format!("chaos-inject:{user_id}")`. Different buckets
    /// are tracked separately so silencing doesn't rate-limit
    /// chaos injection and vice versa.
    pub fn check(&self, key: &str, cooldown: Duration) -> Result<(), u64> {
        let mut map = self
            .last_fired
            .lock()
            .expect("admin cooldown lock poisoned");
        let now = Instant::now();
        if let Some(prev) = map.get(key)
            && now.duration_since(*prev) < cooldown
        {
            let remaining = cooldown - now.duration_since(*prev);
            return Err(remaining.as_secs().max(1));
        }
        map.insert(key.to_string(), now);
        Ok(())
    }

    /// Opportunistically drop stale entries so the HashMap doesn't
    /// grow unbounded over a long uptime. Cheap: called under the
    /// same lock as `check` would take.
    pub fn prune_older_than(&self, window: Duration) {
        let mut map = self
            .last_fired
            .lock()
            .expect("admin cooldown lock poisoned");
        let now = Instant::now();
        map.retain(|_, t| now.duration_since(*t) < window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_call_passes_second_within_window_is_limited() {
        let cd = AdminCooldown::new();
        assert!(cd.check("chaos:alice", Duration::from_secs(10)).is_ok());
        let err = cd
            .check("chaos:alice", Duration::from_secs(10))
            .unwrap_err();
        assert!(err <= 10 && err >= 1, "retry-after within bounds: {err}");
    }

    #[test]
    fn different_users_tracked_independently() {
        let cd = AdminCooldown::new();
        assert!(cd.check("chaos:alice", Duration::from_secs(10)).is_ok());
        // Different user — not rate-limited by alice's call.
        assert!(cd.check("chaos:bob", Duration::from_secs(10)).is_ok());
    }

    #[test]
    fn different_buckets_tracked_independently() {
        let cd = AdminCooldown::new();
        assert!(cd.check("chaos:alice", Duration::from_secs(10)).is_ok());
        // Same user, different operation bucket — not rate-limited.
        assert!(cd.check("silence:alice", Duration::from_secs(10)).is_ok());
    }

    #[test]
    fn prune_drops_stale() {
        let cd = AdminCooldown::new();
        cd.check("chaos:alice", Duration::from_secs(10)).unwrap();
        // Pruning keeps everything newer than the window …
        cd.prune_older_than(Duration::from_secs(60));
        assert!(cd.check("chaos:alice", Duration::from_secs(10)).is_err());
        // … and drops older than the window. Using a 0ns window
        // removes everything: next call to check() writes fresh.
        cd.prune_older_than(Duration::from_nanos(0));
        assert!(cd.check("chaos:alice", Duration::from_secs(10)).is_ok());
    }
}
