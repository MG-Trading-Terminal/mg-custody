//! Multi-RPC endpoint failover.
//!
//! Manages multiple RPC endpoints per chain with automatic failover
//! when the primary endpoint fails. Tracks latency and error rates.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// An RPC endpoint with health tracking.
#[derive(Debug)]
pub struct RpcEndpoint {
    pub url: String,
    pub is_primary: bool,
    consecutive_errors: AtomicU64,
    total_requests: AtomicU64,
    total_errors: AtomicU64,
    last_success: std::sync::Mutex<Option<Instant>>,
}

impl RpcEndpoint {
    pub fn new(url: &str, is_primary: bool) -> Self {
        Self {
            url: url.to_string(),
            is_primary,
            consecutive_errors: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            last_success: std::sync::Mutex::new(None),
        }
    }

    /// Record a successful request.
    pub fn record_success(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.consecutive_errors.store(0, Ordering::Relaxed);
        *self.last_success.lock().unwrap() = Some(Instant::now());
    }

    /// Record a failed request.
    pub fn record_error(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_errors.fetch_add(1, Ordering::Relaxed);
        self.consecutive_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Check if this endpoint is healthy (< 3 consecutive errors).
    pub fn is_healthy(&self) -> bool {
        self.consecutive_errors.load(Ordering::Relaxed) < 3
    }

    /// Error rate (0.0 - 1.0).
    pub fn error_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        let errors = self.total_errors.load(Ordering::Relaxed);
        if total == 0 { 0.0 } else { errors as f64 / total as f64 }
    }
}

/// RPC failover manager for a chain.
pub struct RpcFailover {
    endpoints: Vec<RpcEndpoint>,
    current: AtomicUsize,
}

impl RpcFailover {
    /// Create a failover manager with primary + fallback endpoints.
    pub fn new(primary: &str, fallbacks: &[String]) -> Self {
        let mut endpoints = vec![RpcEndpoint::new(primary, true)];
        for fb in fallbacks {
            endpoints.push(RpcEndpoint::new(fb, false));
        }
        Self {
            endpoints,
            current: AtomicUsize::new(0),
        }
    }

    /// Get the current best endpoint URL.
    pub fn current_url(&self) -> &str {
        let idx = self.current.load(Ordering::Relaxed);
        &self.endpoints[idx % self.endpoints.len()].url
    }

    /// Record success for current endpoint.
    pub fn record_success(&self) {
        let idx = self.current.load(Ordering::Relaxed);
        if let Some(ep) = self.endpoints.get(idx) {
            ep.record_success();
        }
    }

    /// Record error and potentially failover.
    pub fn record_error(&self) {
        let idx = self.current.load(Ordering::Relaxed);
        if let Some(ep) = self.endpoints.get(idx) {
            ep.record_error();
            if !ep.is_healthy() {
                self.failover();
            }
        }
    }

    /// Switch to the next healthy endpoint.
    fn failover(&self) {
        let current = self.current.load(Ordering::Relaxed);
        for i in 1..self.endpoints.len() {
            let next = (current + i) % self.endpoints.len();
            if self.endpoints[next].is_healthy() {
                self.current.store(next, Ordering::Relaxed);
                tracing::warn!(
                    from = %self.endpoints[current].url,
                    to = %self.endpoints[next].url,
                    "RPC failover triggered"
                );
                return;
            }
        }
        tracing::error!("All RPC endpoints unhealthy — no failover target");
    }

    /// Number of endpoints.
    pub fn endpoint_count(&self) -> usize {
        self.endpoints.len()
    }

    /// Number of healthy endpoints.
    pub fn healthy_count(&self) -> usize {
        self.endpoints.iter().filter(|e| e.is_healthy()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_endpoint() {
        let fm = RpcFailover::new("http://primary:8545", &[]);
        assert_eq!(fm.current_url(), "http://primary:8545");
        assert_eq!(fm.endpoint_count(), 1);
    }

    #[test]
    fn failover_on_errors() {
        let fm = RpcFailover::new("http://primary:8545", &["http://fallback:8545".into()]);
        assert_eq!(fm.current_url(), "http://primary:8545");

        // 3 consecutive errors trigger failover
        fm.record_error();
        fm.record_error();
        fm.record_error();
        assert_eq!(fm.current_url(), "http://fallback:8545");
    }

    #[test]
    fn success_resets_errors() {
        let fm = RpcFailover::new("http://primary:8545", &["http://fallback:8545".into()]);
        fm.record_error();
        fm.record_error();
        fm.record_success(); // resets consecutive errors
        fm.record_error(); // only 1 consecutive error now
        assert_eq!(fm.current_url(), "http://primary:8545"); // no failover
    }

    #[test]
    fn healthy_count() {
        let fm = RpcFailover::new("http://a:8545", &["http://b:8545".into(), "http://c:8545".into()]);
        assert_eq!(fm.healthy_count(), 3);
    }
}
