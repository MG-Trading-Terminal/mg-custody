//! Prometheus metrics implementation.
//!
//! Registers real Prometheus counters, gauges, and histograms.
//! Exposes them via a `/metrics` endpoint in the gateway.

use prometheus::{
    register_counter_vec, register_gauge, register_gauge_vec, register_histogram_vec,
    CounterVec, Gauge, GaugeVec, HistogramVec, TextEncoder, Encoder,
};
use once_cell::sync::Lazy;

// ── Deposit metrics ──────────────────────────────────────────────

pub static DEPOSITS_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "custody_deposits_total",
        "Total deposits by chain and status",
        &["chain", "status"]
    ).unwrap()
});

pub static DEPOSIT_AMOUNT: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "custody_deposit_amount_total",
        "Total deposit amount by chain and asset",
        &["chain", "asset"]
    ).unwrap()
});

// ── Withdrawal metrics ───────────────────────────────────────────

pub static WITHDRAWALS_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "custody_withdrawals_total",
        "Total withdrawals by chain and status",
        &["chain", "status"]
    ).unwrap()
});

pub static WITHDRAWAL_LATENCY: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "custody_withdrawal_processing_seconds",
        "Withdrawal processing latency from received to finalized",
        &["chain"],
        vec![1.0, 5.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0]
    ).unwrap()
});

// ── Chain health metrics ─────────────────────────────────────────

pub static CHAIN_BLOCK_HEIGHT: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "custody_chain_block_height",
        "Current block height per chain",
        &["chain"]
    ).unwrap()
});

pub static CHAIN_SCAN_LAG: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "custody_chain_scan_lag_blocks",
        "Scanner lag in blocks (current - last_scanned)",
        &["chain"]
    ).unwrap()
});

pub static CHAIN_RPC_LATENCY: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "custody_chain_rpc_latency_seconds",
        "RPC call latency per chain",
        &["chain", "method"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    ).unwrap()
});

// ── Signing metrics ──────────────────────────────────────────────

pub static SIGNING_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "custody_signing_requests_total",
        "Total signing requests by chain and result",
        &["chain", "result"]
    ).unwrap()
});

pub static SIGNING_LATENCY: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "custody_signing_latency_seconds",
        "Transaction signing latency",
        &["chain"],
        vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0]
    ).unwrap()
});

// ── Policy metrics ───────────────────────────────────────────────

pub static POLICY_DECISIONS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "custody_policy_decisions_total",
        "Policy engine decisions by action",
        &["action"]
    ).unwrap()
});

// ── RBAC metrics ────────────────────────────────────────────────

pub static PERMISSION_DENIALS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "custody_permission_denials_total",
        "RBAC permission denials by required role and method",
        &["required_role", "method"]
    ).unwrap()
});

// ── Compliance metrics ───────────────────────────────────────────

pub static COMPLIANCE_SCREENINGS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "custody_compliance_screenings_total",
        "Compliance screenings by result",
        &["result"]
    ).unwrap()
});

// ── Wallet metrics ───────────────────────────────────────────────

pub static WALLET_BALANCE: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "custody_wallet_balance",
        "Wallet balance by chain, asset, and tier",
        &["chain", "asset", "tier"]
    ).unwrap()
});

// ── Circuit breaker metrics ─────────────────────────────────────

/// Gauge: 1 if circuit breaker is tripped, 0 if normal.
pub static CIRCUIT_BREAKER_TRIPPED: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "custody_circuit_breaker_tripped",
        "Circuit breaker state: 1=tripped, 0=normal",
        &["chain"]
    ).unwrap()
});

/// Counter of total outflow in USD equivalent (for monitoring approach to limits).
pub static CIRCUIT_BREAKER_OUTFLOW_USD: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "custody_circuit_breaker_outflow_usd",
        "Current outflow window total in USD",
        &["window"] // "hourly" or "daily"
    ).unwrap()
});

// ── Database pool metrics ────────────────────────────────────────

/// Gauge: number of active (in-use) DB connections.
pub static DB_POOL_ACTIVE: Lazy<prometheus::Gauge> = Lazy::new(|| {
    register_gauge!(
        "custody_db_pool_active_connections",
        "Number of active (in-use) database connections"
    ).unwrap()
});

/// Gauge: number of idle DB connections.
pub static DB_POOL_IDLE: Lazy<prometheus::Gauge> = Lazy::new(|| {
    register_gauge!(
        "custody_db_pool_idle_connections",
        "Number of idle database connections"
    ).unwrap()
});

/// Gauge: total pool size (max connections configured).
pub static DB_POOL_SIZE: Lazy<prometheus::Gauge> = Lazy::new(|| {
    register_gauge!(
        "custody_db_pool_max_connections",
        "Maximum database connections configured"
    ).unwrap()
});

// ── API request metrics ───────────────────────────────────────────

/// Histogram of HTTP API request durations, labelled by method and path.
pub static API_REQUEST_LATENCY: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "custody_api_request_duration_seconds",
        "API request latency by HTTP method and path",
        &["method", "path"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    ).unwrap()
});

/// Counter of total API requests, labelled by method and HTTP status code.
pub static API_REQUEST_COUNT: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "custody_api_requests_total",
        "Total API requests by HTTP method and status code",
        &["method", "status"]
    ).unwrap()
});

// ── Export ────────────────────────────────────────────────────────

/// Render all Prometheus metrics as text for the /metrics endpoint.
pub fn render_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_render() {
        // Increment a counter
        DEPOSITS_TOTAL.with_label_values(&["ethereum", "detected"]).inc();
        DEPOSITS_TOTAL.with_label_values(&["ethereum", "confirmed"]).inc();
        DEPOSITS_TOTAL.with_label_values(&["bitcoin", "detected"]).inc();

        let output = render_metrics();
        assert!(output.contains("custody_deposits_total"));
        assert!(output.contains("ethereum"));
        assert!(output.contains("bitcoin"));
    }

    #[test]
    fn gauge_metrics() {
        CHAIN_BLOCK_HEIGHT.with_label_values(&["ethereum"]).set(20_000_000.0);
        CHAIN_BLOCK_HEIGHT.with_label_values(&["bitcoin"]).set(850_000.0);

        let output = render_metrics();
        assert!(output.contains("custody_chain_block_height"));
    }

    #[test]
    fn histogram_metrics() {
        WITHDRAWAL_LATENCY.with_label_values(&["ethereum"]).observe(5.5);
        SIGNING_LATENCY.with_label_values(&["ethereum"]).observe(0.05);

        let output = render_metrics();
        assert!(output.contains("custody_withdrawal_processing_seconds"));
        assert!(output.contains("custody_signing_latency_seconds"));
    }
}
