//! Grafana dashboard configuration generator.
//!
//! Generates JSON dashboard definitions for self-hosted Grafana.

use serde_json::{json, Value};

/// Generate the main custody dashboard JSON.
pub fn custody_dashboard() -> Value {
    json!({
        "dashboard": {
            "title": "MG Custody Overview",
            "uid": "custody-main",
            "timezone": "utc",
            "panels": [
                panel("Deposits (24h)", "custody_deposits_total", "counter", 0, 0),
                panel("Withdrawals (24h)", "custody_withdrawals_total", "counter", 8, 0),
                panel("ETH Block Height", "custody_chain_block_height{chain=\"ethereum\"}", "gauge", 16, 0),
                panel("Scanner Lag", "custody_chain_scan_lag_blocks", "graph", 0, 8),
                panel("Signing Latency", "custody_signing_latency_seconds", "heatmap", 8, 8),
                panel("Hot Wallet Balance", "custody_wallet_balance{tier=\"hot\"}", "graph", 16, 8),
                panel("RPC Latency", "custody_chain_rpc_latency_seconds", "graph", 0, 16),
                panel("Policy Decisions", "custody_policy_decisions_total", "piechart", 8, 16),
                panel("Compliance Blocks", "custody_compliance_blocked_total", "counter", 16, 16),
            ],
            "refresh": "30s",
            "time": { "from": "now-24h", "to": "now" },
        }
    })
}

fn panel(title: &str, query: &str, panel_type: &str, x: u32, y: u32) -> Value {
    json!({
        "title": title,
        "type": panel_type,
        "gridPos": { "x": x, "y": y, "w": 8, "h": 8 },
        "targets": [{ "expr": query, "refId": "A" }],
        "datasource": "Prometheus",
    })
}

/// Generate alerts configuration.
pub fn alert_rules() -> Value {
    json!({
        "groups": [{
            "name": "custody-alerts",
            "rules": [
                { "alert": "HighScannerLag", "expr": "custody_chain_scan_lag_blocks > 100", "for": "5m", "severity": "warning" },
                { "alert": "SigningErrors", "expr": "rate(custody_signing_errors_total[5m]) > 0", "for": "1m", "severity": "critical" },
                { "alert": "HotWalletLow", "expr": "custody_wallet_balance{tier='hot'} < 0.01", "for": "5m", "severity": "warning" },
                { "alert": "RPCLatencyHigh", "expr": "histogram_quantile(0.95, custody_chain_rpc_latency_seconds) > 5", "for": "5m", "severity": "warning" },
                { "alert": "CircuitBreakerTripped", "expr": "custody_policy_blocked_total > 0", "for": "0s", "severity": "critical" },
                { "alert": "SanctionsHit", "expr": "custody_compliance_sanctions_hits_total > 0", "for": "0s", "severity": "emergency" },
            ]
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_has_panels() {
        let dash = custody_dashboard();
        let panels = dash["dashboard"]["panels"].as_array().unwrap();
        assert!(panels.len() >= 9);
    }

    #[test]
    fn alert_rules_defined() {
        let alerts = alert_rules();
        let rules = alerts["groups"][0]["rules"].as_array().unwrap();
        assert!(rules.len() >= 6);
        assert!(rules.iter().any(|r| r["alert"] == "SanctionsHit"));
    }
}
