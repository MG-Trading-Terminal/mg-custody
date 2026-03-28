//! Detailed health types for the `/v1/health/detailed` endpoint.
//!
//! `DetailedHealth` aggregates per-chain scanner state with RPC latency metrics
//! and system-level runtime information (DB pool, memory, circuit breaker).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level response for `GET /v1/health/detailed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedHealth {
    /// Per-chain health keyed by lowercase chain name ("ethereum", "bitcoin", etc.).
    pub chains: HashMap<String, ChainHealth>,
    /// Service-level runtime metrics.
    pub system: SystemHealth,
    /// ISO-8601 timestamp of when this snapshot was generated.
    pub generated_at: DateTime<Utc>,
}

/// Health status for a single chain's scanner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainHealth {
    /// Coarse status: "healthy" | "lagging" | "stalled" | "unknown".
    pub status: String,
    /// RPC endpoint URL (no credentials — safe to log).
    pub rpc_url: String,
    /// Latest block/slot reported by the RPC node (None if unreachable).
    pub latest_block: Option<u64>,
    /// Block/slot most recently scanned by our scanner.
    pub scanner_block: Option<u64>,
    /// `latest_block - scanner_block`; None when either is absent.
    pub scanner_lag: Option<i64>,
    /// When the scanner last successfully completed a scan iteration.
    pub last_scan_at: Option<DateTime<Utc>>,
    /// Average RPC round-trip latency in milliseconds over the last N calls.
    pub avg_rpc_latency_ms: Option<f64>,
    /// Fraction of RPC calls that returned errors in the current window, as a percentage.
    pub error_rate_pct: f64,
    /// Deposit events detected in the current calendar day (UTC).
    pub deposits_today: u64,
    /// Withdrawals confirmed in the current calendar day (UTC).
    pub withdrawals_today: u64,
}

impl ChainHealth {
    /// Classify `status` based on scanner lag thresholds.
    ///
    /// | lag (blocks) | status    |
    /// |-------------|-----------|
    /// | < 50        | healthy   |
    /// | 50 – 200    | lagging   |
    /// | ≥ 200       | stalled   |
    /// | no data     | unknown   |
    pub fn classify_status(lag: Option<i64>) -> &'static str {
        match lag {
            None => "unknown",
            Some(l) if l < 50 => "healthy",
            Some(l) if l < 200 => "lagging",
            _ => "stalled",
        }
    }
}

/// Service-level runtime information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    /// Seconds since the process started.
    pub uptime_seconds: u64,
    /// Configured DB pool maximum size.
    pub db_pool_size: u32,
    /// Number of DB connections currently in use.
    pub db_pool_active: u32,
    /// Resident set size in megabytes (best-effort; 0 if unavailable).
    pub memory_mb: u64,
    /// Webhook deliveries currently in `pending` state.
    pub pending_webhooks: u64,
    /// Circuit breaker state: "normal" | "tripped".
    pub circuit_breaker: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_healthy_when_lag_zero() {
        assert_eq!(ChainHealth::classify_status(Some(0)), "healthy");
    }

    #[test]
    fn classify_healthy_when_lag_below_50() {
        assert_eq!(ChainHealth::classify_status(Some(49)), "healthy");
    }

    #[test]
    fn classify_lagging_at_50() {
        assert_eq!(ChainHealth::classify_status(Some(50)), "lagging");
    }

    #[test]
    fn classify_lagging_below_200() {
        assert_eq!(ChainHealth::classify_status(Some(199)), "lagging");
    }

    #[test]
    fn classify_stalled_at_200() {
        assert_eq!(ChainHealth::classify_status(Some(200)), "stalled");
    }

    #[test]
    fn classify_stalled_large_lag() {
        assert_eq!(ChainHealth::classify_status(Some(10_000)), "stalled");
    }

    #[test]
    fn classify_unknown_when_no_data() {
        assert_eq!(ChainHealth::classify_status(None), "unknown");
    }

    #[test]
    fn detailed_health_serialization_roundtrip() {
        let h = DetailedHealth {
            chains: {
                let mut m = HashMap::new();
                m.insert("ethereum".into(), ChainHealth {
                    status: "healthy".into(),
                    rpc_url: "https://rpc.example.com".into(),
                    latest_block: Some(19_500_000),
                    scanner_block: Some(19_499_950),
                    scanner_lag: Some(50),
                    last_scan_at: None,
                    avg_rpc_latency_ms: Some(120.0),
                    error_rate_pct: 0.5,
                    deposits_today: 15,
                    withdrawals_today: 8,
                });
                m
            },
            system: SystemHealth {
                uptime_seconds: 86400,
                db_pool_size: 10,
                db_pool_active: 3,
                memory_mb: 256,
                pending_webhooks: 2,
                circuit_breaker: "normal".into(),
            },
            generated_at: Utc::now(),
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("ethereum"));
        assert!(json.contains("healthy"));
        assert!(json.contains("uptime_seconds"));
        assert!(json.contains("circuit_breaker"));
        // Deserialize back
        let h2: DetailedHealth = serde_json::from_str(&json).unwrap();
        assert!(h2.chains.contains_key("ethereum"));
        assert_eq!(h2.system.db_pool_size, 10);
    }
}
