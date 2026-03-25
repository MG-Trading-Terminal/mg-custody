//! Load testing framework types.
//!
//! Defines scenarios for stress testing the custody service.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Load test scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestScenario {
    pub name: String,
    pub description: String,
    pub concurrent_deposits: u32,
    pub concurrent_withdrawals: u32,
    pub duration_secs: u64,
    pub ramp_up_secs: u64,
}

/// Pre-defined load test scenarios.
pub fn standard_scenarios() -> Vec<LoadTestScenario> {
    vec![
        LoadTestScenario {
            name: "smoke".into(),
            description: "Basic functionality verification".into(),
            concurrent_deposits: 10,
            concurrent_withdrawals: 5,
            duration_secs: 30,
            ramp_up_secs: 5,
        },
        LoadTestScenario {
            name: "standard".into(),
            description: "Normal production load".into(),
            concurrent_deposits: 100,
            concurrent_withdrawals: 20,
            duration_secs: 300,
            ramp_up_secs: 30,
        },
        LoadTestScenario {
            name: "peak".into(),
            description: "Peak load (1000 deposits + 100 withdrawals)".into(),
            concurrent_deposits: 1000,
            concurrent_withdrawals: 100,
            duration_secs: 600,
            ramp_up_secs: 60,
        },
        LoadTestScenario {
            name: "stress".into(),
            description: "Beyond capacity — find breaking point".into(),
            concurrent_deposits: 5000,
            concurrent_withdrawals: 500,
            duration_secs: 300,
            ramp_up_secs: 30,
        },
    ]
}

/// Load test result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestResult {
    pub scenario: String,
    pub total_requests: u64,
    pub successful: u64,
    pub failed: u64,
    pub p50_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub p99_latency_ms: u64,
    pub max_latency_ms: u64,
    pub requests_per_second: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_scenarios_exist() {
        let scenarios = standard_scenarios();
        assert!(scenarios.len() >= 4);
        assert!(scenarios.iter().any(|s| s.name == "peak"));
    }

    #[test]
    fn peak_scenario_targets() {
        let scenarios = standard_scenarios();
        let peak = scenarios.iter().find(|s| s.name == "peak").unwrap();
        assert_eq!(peak.concurrent_deposits, 1000);
        assert_eq!(peak.concurrent_withdrawals, 100);
    }
}
