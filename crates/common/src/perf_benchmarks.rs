//! Performance measurement utilities for custody operations.
//!
//! Provides timers, percentile computation, and pool health metrics.
//! All timing values are in milliseconds.  No f64 is used for monetary
//! values — these are latency measurements only.

use std::time::Instant;

// ---------------------------------------------------------------------------
// PerfTimer
// ---------------------------------------------------------------------------

/// Lightweight wall-clock timer for instrumenting custody operations.
pub struct PerfTimer {
    label: String,
    start: Instant,
}

impl PerfTimer {
    /// Start a new timer with the given label.
    pub fn start(label: &str) -> Self {
        Self {
            label: label.to_string(),
            start: Instant::now(),
        }
    }

    /// Return elapsed milliseconds without stopping the timer.
    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1_000.0
    }

    /// Log the elapsed time at INFO level and return milliseconds.
    ///
    /// Consumes the timer.
    pub fn finish(self) -> f64 {
        let ms = self.elapsed_ms();
        tracing::info!(
            label = %self.label,
            elapsed_ms = ms,
            event = "perf_timer",
            "Operation completed"
        );
        ms
    }
}

// ---------------------------------------------------------------------------
// BenchmarkResult
// ---------------------------------------------------------------------------

/// Aggregated benchmark results for a single custody operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkResult {
    /// Name of the measured operation (e.g., "signing_latency", "db_query").
    pub operation: String,
    /// Number of samples in this result.
    pub samples: u32,
    /// 50th-percentile latency in milliseconds.
    pub p50_ms: f64,
    /// 95th-percentile latency in milliseconds.
    pub p95_ms: f64,
    /// 99th-percentile latency in milliseconds.
    pub p99_ms: f64,
    /// Minimum observed latency in milliseconds.
    pub min_ms: f64,
    /// Maximum observed latency in milliseconds.
    pub max_ms: f64,
    /// Arithmetic mean latency in milliseconds.
    pub avg_ms: f64,
}

impl BenchmarkResult {
    /// Compute percentiles from a slice of latency samples.
    ///
    /// Sorts `latencies` in place for efficient percentile selection.
    /// Returns a zeroed result if `latencies` is empty.
    pub fn from_latencies(operation: &str, latencies: &mut [f64]) -> Self {
        if latencies.is_empty() {
            return Self {
                operation: operation.to_string(),
                samples: 0,
                p50_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                avg_ms: 0.0,
            };
        }

        // Sort ascending for percentile indexing.
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let n = latencies.len();
        let percentile = |p: f64| -> f64 {
            // Nearest-rank method: index = ceil(p/100 * n) - 1, clamped.
            let rank = (p / 100.0 * n as f64).ceil() as usize;
            latencies[rank.saturating_sub(1).min(n - 1)]
        };

        let sum: f64 = latencies.iter().sum();

        Self {
            operation: operation.to_string(),
            samples: n as u32,
            p50_ms: percentile(50.0),
            p95_ms: percentile(95.0),
            p99_ms: percentile(99.0),
            min_ms: latencies[0],
            max_ms: latencies[n - 1],
            avg_ms: sum / n as f64,
        }
    }
}

// ---------------------------------------------------------------------------
// PoolMetrics
// ---------------------------------------------------------------------------

/// Snapshot of connection pool health at a point in time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PoolMetrics {
    /// Configured maximum pool size.
    pub max_connections: u32,
    /// Connections currently in use by queries.
    pub active_connections: u32,
    /// Connections sitting idle in the pool.
    pub idle_connections: u32,
    /// How long (ms) `acquire()` will wait before returning an error.
    pub acquire_timeout_ms: u64,
    /// Average time (ms) spent waiting for a connection from the pool.
    pub avg_acquire_ms: f64,
}

// ---------------------------------------------------------------------------
// PerfReport — response payload for GET /v1/health/perf
// ---------------------------------------------------------------------------

/// Full performance report returned by `GET /v1/health/perf`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PerfReport {
    /// Signing latency percentiles (populated from Prometheus histogram snapshot).
    pub signing_latency: Option<BenchmarkResult>,
    /// DB query latency percentiles.
    pub db_query_latency: Option<BenchmarkResult>,
    /// Scanner throughput in blocks-per-second per chain.
    pub scanner_throughput: std::collections::HashMap<String, f64>,
    /// Connection pool health.
    pub pool: PoolMetrics,
    /// ISO-8601 timestamp of the snapshot.
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

impl PerfReport {
    /// Build a minimal report suitable for the health endpoint when no
    /// sampled data is available (e.g., immediately after startup).
    pub fn placeholder(pool: PoolMetrics) -> Self {
        Self {
            signing_latency: None,
            db_query_latency: None,
            scanner_throughput: std::collections::HashMap::new(),
            pool,
            generated_at: chrono::Utc::now(),
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
    fn percentiles_single_element() {
        let mut lats = vec![42.0_f64];
        let r = BenchmarkResult::from_latencies("op", &mut lats);
        assert_eq!(r.samples, 1);
        assert_eq!(r.p50_ms, 42.0);
        assert_eq!(r.p95_ms, 42.0);
        assert_eq!(r.p99_ms, 42.0);
        assert_eq!(r.min_ms, 42.0);
        assert_eq!(r.max_ms, 42.0);
        assert_eq!(r.avg_ms, 42.0);
    }

    #[test]
    fn percentiles_known_values() {
        // 100 samples from 1ms to 100ms.
        let mut lats: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let r = BenchmarkResult::from_latencies("db_query", &mut lats);

        assert_eq!(r.samples, 100);
        assert_eq!(r.min_ms, 1.0);
        assert_eq!(r.max_ms, 100.0);
        // p50: rank = ceil(0.5 * 100) = 50 → index 49 → value 50.
        assert_eq!(r.p50_ms, 50.0);
        // p95: rank = ceil(0.95 * 100) = 95 → index 94 → value 95.
        assert_eq!(r.p95_ms, 95.0);
        // p99: rank = ceil(0.99 * 100) = 99 → index 98 → value 99.
        assert_eq!(r.p99_ms, 99.0);
        // avg = (1+2+...+100) / 100 = 5050 / 100 = 50.5
        assert!((r.avg_ms - 50.5).abs() < 1e-9);
    }

    #[test]
    fn percentiles_empty() {
        let mut lats: Vec<f64> = vec![];
        let r = BenchmarkResult::from_latencies("empty", &mut lats);
        assert_eq!(r.samples, 0);
        assert_eq!(r.p50_ms, 0.0);
    }

    #[test]
    fn timer_elapsed_is_nonnegative() {
        let t = PerfTimer::start("test");
        let ms = t.elapsed_ms();
        assert!(ms >= 0.0);
    }

    #[test]
    fn timer_finish_returns_ms() {
        let t = PerfTimer::start("test");
        let ms = t.finish();
        assert!(ms >= 0.0);
    }

    #[test]
    fn pool_metrics_struct_fields() {
        let m = PoolMetrics {
            max_connections: 20,
            active_connections: 5,
            idle_connections: 15,
            acquire_timeout_ms: 10_000,
            avg_acquire_ms: 1.2,
        };
        assert_eq!(m.max_connections, 20);
        assert_eq!(m.active_connections + m.idle_connections, 20);
    }

    #[test]
    fn perf_report_placeholder() {
        let pool = PoolMetrics {
            max_connections: 10,
            active_connections: 0,
            idle_connections: 10,
            acquire_timeout_ms: 5_000,
            avg_acquire_ms: 0.0,
        };
        let r = PerfReport::placeholder(pool);
        assert!(r.signing_latency.is_none());
        assert!(r.scanner_throughput.is_empty());
    }
}
