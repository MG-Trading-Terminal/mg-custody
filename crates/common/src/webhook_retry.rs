//! Webhook retry strategies — exponential backoff with jitter, dead letter queue.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Retry strategy for webhook delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStrategy {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f32,
    pub jitter_percent: u32,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self { max_attempts: 10, initial_delay_ms: 1000, max_delay_ms: 300_000, backoff_multiplier: 2.0, jitter_percent: 20 }
    }
}

impl RetryStrategy {
    /// Calculate delay for attempt N (0-indexed).
    pub fn delay_ms(&self, attempt: u32) -> u64 {
        let base = (self.initial_delay_ms as f64) * self.backoff_multiplier.powi(attempt as i32) as f64;
        let clamped = (base as u64).min(self.max_delay_ms);
        // Add jitter
        let jitter_range = clamped * self.jitter_percent as u64 / 100;
        let jitter = if jitter_range > 0 { rand_simple(jitter_range) } else { 0 };
        clamped + jitter
    }

    /// Next retry time from now.
    pub fn next_retry_at(&self, attempt: u32) -> DateTime<Utc> {
        Utc::now() + Duration::milliseconds(self.delay_ms(attempt) as i64)
    }

    /// Should we retry at this attempt count?
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }
}

/// Simple pseudo-random for jitter (no crypto needed).
fn rand_simple(max: u64) -> u64 {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64;
    seed % max.max(1)
}

/// Delivery attempt tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    pub attempt: u32,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub latency_ms: u32,
    pub attempted_at: DateTime<Utc>,
}

/// Webhook health score based on recent deliveries.
pub fn webhook_health_score(total: u64, successful: u64) -> f32 {
    if total == 0 { return 1.0; }
    successful as f32 / total as f32
}

/// Should we disable a webhook endpoint based on failure rate?
pub fn should_disable_endpoint(consecutive_failures: u32, threshold: u32) -> bool {
    consecutive_failures >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_retry_strategy() {
        let s = RetryStrategy::default();
        assert_eq!(s.max_attempts, 10);
        assert_eq!(s.initial_delay_ms, 1000);
    }

    #[test]
    fn delay_increases_exponentially() {
        let s = RetryStrategy { jitter_percent: 0, ..Default::default() };
        let d0 = s.delay_ms(0);
        let d1 = s.delay_ms(1);
        let d2 = s.delay_ms(2);
        assert!(d1 > d0);
        assert!(d2 > d1);
    }

    #[test]
    fn delay_capped() {
        let s = RetryStrategy { jitter_percent: 0, ..Default::default() };
        let d20 = s.delay_ms(20);
        assert!(d20 <= s.max_delay_ms);
    }

    #[test]
    fn should_retry_within_max() {
        let s = RetryStrategy::default();
        assert!(s.should_retry(0));
        assert!(s.should_retry(9));
        assert!(!s.should_retry(10));
    }

    #[test]
    fn health_score_perfect() {
        assert_eq!(webhook_health_score(100, 100), 1.0);
    }

    #[test]
    fn health_score_partial() {
        let score = webhook_health_score(100, 90);
        assert!(score > 0.89 && score < 0.91);
    }

    #[test]
    fn health_score_empty() {
        assert_eq!(webhook_health_score(0, 0), 1.0);
    }

    #[test]
    fn disable_endpoint_threshold() {
        assert!(!should_disable_endpoint(4, 5));
        assert!(should_disable_endpoint(5, 5));
        assert!(should_disable_endpoint(10, 5));
    }
}
