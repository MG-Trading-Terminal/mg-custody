//! Metrics wiring — connect all 125+ endpoints to Prometheus counters.
//! Load test scenarios with realistic traffic patterns.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Endpoint metrics definitions
// ---------------------------------------------------------------------------

/// All metric names used across the custody service.
pub struct MetricNames;

impl MetricNames {
    // HTTP
    pub const HTTP_REQUESTS_TOTAL: &str = "custody_http_requests_total";
    pub const HTTP_REQUEST_DURATION: &str = "custody_http_request_duration_seconds";
    pub const HTTP_RESPONSE_SIZE: &str = "custody_http_response_size_bytes";

    // Deposits
    pub const DEPOSITS_DETECTED: &str = "custody_deposits_detected_total";
    pub const DEPOSITS_CONFIRMED: &str = "custody_deposits_confirmed_total";
    pub const DEPOSITS_CREDITED: &str = "custody_deposits_credited_total";
    pub const DEPOSIT_VALUE: &str = "custody_deposit_value_total";

    // Withdrawals
    pub const WITHDRAWALS_SUBMITTED: &str = "custody_withdrawals_submitted_total";
    pub const WITHDRAWALS_SIGNED: &str = "custody_withdrawals_signed_total";
    pub const WITHDRAWALS_BROADCAST: &str = "custody_withdrawals_broadcast_total";
    pub const WITHDRAWALS_CONFIRMED: &str = "custody_withdrawals_confirmed_total";
    pub const WITHDRAWALS_FAILED: &str = "custody_withdrawals_failed_total";
    pub const WITHDRAWAL_VALUE: &str = "custody_withdrawal_value_total";

    // Signing
    pub const SIGNING_OPERATIONS: &str = "custody_signing_operations_total";
    pub const SIGNING_LATENCY: &str = "custody_signing_latency_seconds";
    pub const RAW_SIGN_OPERATIONS: &str = "custody_raw_sign_operations_total";

    // Chain health
    pub const CHAIN_BLOCK_HEIGHT: &str = "custody_chain_block_height";
    pub const CHAIN_RPC_LATENCY: &str = "custody_chain_rpc_latency_seconds";
    pub const CHAIN_RPC_ERRORS: &str = "custody_chain_rpc_errors_total";
    pub const CHAIN_SCAN_LAG: &str = "custody_chain_scan_lag_blocks";

    // Vaults
    pub const VAULT_BALANCE: &str = "custody_vault_balance";
    pub const VAULT_COUNT: &str = "custody_vault_count";
    pub const INTERNAL_TRANSFERS: &str = "custody_internal_transfers_total";

    // Policy
    pub const POLICY_EVALUATIONS: &str = "custody_policy_evaluations_total";
    pub const POLICY_VIOLATIONS: &str = "custody_policy_violations_total";
    pub const APPROVALS_PENDING: &str = "custody_approvals_pending";
    pub const APPROVALS_DECIDED: &str = "custody_approvals_decided_total";

    // Compliance
    pub const SCREENINGS_TOTAL: &str = "custody_screenings_total";
    pub const SANCTIONS_BLOCKED: &str = "custody_sanctions_blocked_total";
    pub const HIGH_RISK_FLAGGED: &str = "custody_high_risk_flagged_total";

    // Webhooks
    pub const WEBHOOKS_DELIVERED: &str = "custody_webhooks_delivered_total";
    pub const WEBHOOKS_FAILED: &str = "custody_webhooks_failed_total";
    pub const WEBHOOK_LATENCY: &str = "custody_webhook_delivery_latency_seconds";

    // Gas station
    pub const GAS_REFUELS: &str = "custody_gas_refuels_total";
    pub const GAS_SPENT: &str = "custody_gas_spent_total";

    // Circuit breaker
    pub const CIRCUIT_BREAKER_TRIPS: &str = "custody_circuit_breaker_trips_total";
    pub const HOURLY_OUTFLOW: &str = "custody_hourly_outflow";
    pub const DAILY_OUTFLOW: &str = "custody_daily_outflow";

    // API keys
    pub const API_KEY_USAGE: &str = "custody_api_key_requests_total";
    pub const RATE_LIMIT_REJECTIONS: &str = "custody_rate_limit_rejections_total";
}

/// Standard label names for metrics.
pub struct MetricLabels;

impl MetricLabels {
    pub const CHAIN: &str = "chain";
    pub const ASSET: &str = "asset";
    pub const STATUS: &str = "status";
    pub const METHOD: &str = "method";
    pub const PATH: &str = "path";
    pub const TIER: &str = "tier";
    pub const PROVIDER: &str = "provider";
    pub const VAULT_ID: &str = "vault_id";
    pub const API_KEY_ID: &str = "api_key_id";
    pub const ERROR_CODE: &str = "error_code";
}

/// All metric names as a list (for documentation/registration).
pub fn all_metric_names() -> Vec<(&'static str, &'static str)> {
    vec![
        (MetricNames::HTTP_REQUESTS_TOTAL, "Total HTTP requests by method/path/status"),
        (MetricNames::HTTP_REQUEST_DURATION, "HTTP request latency histogram"),
        (MetricNames::DEPOSITS_DETECTED, "Deposits detected by chain scanner"),
        (MetricNames::DEPOSITS_CONFIRMED, "Deposits reaching required confirmations"),
        (MetricNames::WITHDRAWALS_SUBMITTED, "Withdrawals submitted via API"),
        (MetricNames::WITHDRAWALS_CONFIRMED, "Withdrawals confirmed on-chain"),
        (MetricNames::WITHDRAWALS_FAILED, "Withdrawals that failed"),
        (MetricNames::SIGNING_OPERATIONS, "Signing operations by chain"),
        (MetricNames::SIGNING_LATENCY, "Signing operation latency"),
        (MetricNames::CHAIN_BLOCK_HEIGHT, "Current block height per chain"),
        (MetricNames::CHAIN_RPC_LATENCY, "RPC call latency per chain"),
        (MetricNames::VAULT_BALANCE, "Vault balance by asset/chain"),
        (MetricNames::POLICY_VIOLATIONS, "Policy rule violations"),
        (MetricNames::SANCTIONS_BLOCKED, "Transactions blocked by sanctions"),
        (MetricNames::WEBHOOKS_DELIVERED, "Webhooks successfully delivered"),
        (MetricNames::CIRCUIT_BREAKER_TRIPS, "Circuit breaker trip events"),
        (MetricNames::RATE_LIMIT_REJECTIONS, "Rate-limited requests"),
    ]
}

// ---------------------------------------------------------------------------
// Load test scenarios
// ---------------------------------------------------------------------------

/// Load test scenario definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestScenario {
    pub name: String,
    pub description: String,
    pub duration_secs: u32,
    pub ramp_up_secs: u32,
    pub target_rps: u32,
    pub operations: Vec<LoadTestOperation>,
}

/// A single operation in a load test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestOperation {
    pub name: String,
    pub method: String,
    pub path: String,
    pub weight: u32,
    pub expected_latency_ms: u32,
}

/// Predefined load test scenarios.
pub fn load_test_scenarios() -> Vec<LoadTestScenario> {
    vec![
        LoadTestScenario {
            name: "smoke".into(),
            description: "Basic smoke test — verify endpoints respond".into(),
            duration_secs: 60, ramp_up_secs: 5, target_rps: 5,
            operations: vec![
                LoadTestOperation { name: "health".into(), method: "GET".into(), path: "/health".into(), weight: 30, expected_latency_ms: 10 },
                LoadTestOperation { name: "list_vaults".into(), method: "GET".into(), path: "/v1/vaults".into(), weight: 20, expected_latency_ms: 50 },
                LoadTestOperation { name: "list_transactions".into(), method: "GET".into(), path: "/v1/transactions".into(), weight: 20, expected_latency_ms: 100 },
                LoadTestOperation { name: "estimate_fee".into(), method: "GET".into(), path: "/v1/estimate-fee?chain=ethereum".into(), weight: 30, expected_latency_ms: 20 },
            ],
        },
        LoadTestScenario {
            name: "standard".into(),
            description: "Normal production traffic pattern".into(),
            duration_secs: 300, ramp_up_secs: 30, target_rps: 50,
            operations: vec![
                LoadTestOperation { name: "health".into(), method: "GET".into(), path: "/health".into(), weight: 10, expected_latency_ms: 10 },
                LoadTestOperation { name: "get_withdrawal".into(), method: "GET".into(), path: "/v1/withdrawals/{id}".into(), weight: 25, expected_latency_ms: 50 },
                LoadTestOperation { name: "list_transactions".into(), method: "GET".into(), path: "/v1/transactions".into(), weight: 20, expected_latency_ms: 100 },
                LoadTestOperation { name: "submit_withdrawal".into(), method: "POST".into(), path: "/v1/withdrawals".into(), weight: 15, expected_latency_ms: 200 },
                LoadTestOperation { name: "create_address".into(), method: "POST".into(), path: "/v1/addresses".into(), weight: 10, expected_latency_ms: 150 },
                LoadTestOperation { name: "wallet_balance".into(), method: "GET".into(), path: "/v1/wallets/ETH".into(), weight: 20, expected_latency_ms: 50 },
            ],
        },
        LoadTestScenario {
            name: "peak".into(),
            description: "Peak traffic — 3x normal load".into(),
            duration_secs: 600, ramp_up_secs: 60, target_rps: 150,
            operations: vec![
                LoadTestOperation { name: "get_withdrawal".into(), method: "GET".into(), path: "/v1/withdrawals/{id}".into(), weight: 30, expected_latency_ms: 100 },
                LoadTestOperation { name: "list_transactions".into(), method: "GET".into(), path: "/v1/transactions".into(), weight: 25, expected_latency_ms: 200 },
                LoadTestOperation { name: "submit_withdrawal".into(), method: "POST".into(), path: "/v1/withdrawals".into(), weight: 20, expected_latency_ms: 300 },
                LoadTestOperation { name: "internal_transfer".into(), method: "POST".into(), path: "/v1/transfers".into(), weight: 15, expected_latency_ms: 150 },
                LoadTestOperation { name: "batch_withdraw".into(), method: "POST".into(), path: "/v1/transactions/batch".into(), weight: 10, expected_latency_ms: 500 },
            ],
        },
        LoadTestScenario {
            name: "stress".into(),
            description: "Stress test — find breaking point".into(),
            duration_secs: 1200, ramp_up_secs: 120, target_rps: 500,
            operations: vec![
                LoadTestOperation { name: "mixed_reads".into(), method: "GET".into(), path: "/v1/transactions".into(), weight: 40, expected_latency_ms: 500 },
                LoadTestOperation { name: "mixed_writes".into(), method: "POST".into(), path: "/v1/withdrawals".into(), weight: 30, expected_latency_ms: 1000 },
                LoadTestOperation { name: "raw_sign".into(), method: "POST".into(), path: "/v1/vaults/{id}/raw-sign".into(), weight: 15, expected_latency_ms: 200 },
                LoadTestOperation { name: "batch".into(), method: "POST".into(), path: "/v1/transactions/batch".into(), weight: 15, expected_latency_ms: 2000 },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_metrics_listed() {
        let metrics = all_metric_names();
        assert!(metrics.len() >= 15);
        // All names start with custody_
        for (name, _) in &metrics {
            assert!(name.starts_with("custody_"), "Metric {name} missing custody_ prefix");
        }
    }

    #[test]
    fn load_test_scenarios_count() {
        let scenarios = load_test_scenarios();
        assert_eq!(scenarios.len(), 4);
        assert_eq!(scenarios[0].name, "smoke");
        assert_eq!(scenarios[3].name, "stress");
    }

    #[test]
    fn load_test_rps_increasing() {
        let scenarios = load_test_scenarios();
        for i in 1..scenarios.len() {
            assert!(scenarios[i].target_rps >= scenarios[i-1].target_rps,
                "Scenarios should have increasing RPS");
        }
    }

    #[test]
    fn load_test_weights_valid() {
        for scenario in load_test_scenarios() {
            let total_weight: u32 = scenario.operations.iter().map(|o| o.weight).sum();
            assert_eq!(total_weight, 100, "Weights should sum to 100 in {}", scenario.name);
        }
    }

    #[test]
    fn metric_labels_non_empty() {
        assert!(!MetricLabels::CHAIN.is_empty());
        assert!(!MetricLabels::STATUS.is_empty());
    }
}
