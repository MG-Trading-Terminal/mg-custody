//! Prometheus metrics implementation.
//!
//! Registers real Prometheus counters, gauges, and histograms.
//! Exposes them via a `/metrics` endpoint in the gateway.

use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec,
    CounterVec, GaugeVec, HistogramVec, TextEncoder, Encoder,
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
