//! Observability metrics definitions.
//!
//! Defines metric names and labels used across the custody service.
//! Actual metric registration happens in the server crate via Prometheus.

/// Metric namespace prefix.
pub const METRIC_PREFIX: &str = "custody";

/// Metric names for deposits.
pub mod deposits {
    pub const DETECTED_TOTAL: &str = "custody_deposits_detected_total";
    pub const CONFIRMED_TOTAL: &str = "custody_deposits_confirmed_total";
    pub const FAILED_TOTAL: &str = "custody_deposits_failed_total";
    pub const CONFIRMATION_LATENCY: &str = "custody_deposit_confirmation_seconds";
}

/// Metric names for withdrawals.
pub mod withdrawals {
    pub const RECEIVED_TOTAL: &str = "custody_withdrawals_received_total";
    pub const SIGNED_TOTAL: &str = "custody_withdrawals_signed_total";
    pub const BROADCAST_TOTAL: &str = "custody_withdrawals_broadcast_total";
    pub const FINALIZED_TOTAL: &str = "custody_withdrawals_finalized_total";
    pub const FAILED_TOTAL: &str = "custody_withdrawals_failed_total";
    pub const PROCESSING_LATENCY: &str = "custody_withdrawal_processing_seconds";
}

/// Metric names for chain health.
pub mod chain_health {
    pub const BLOCK_HEIGHT: &str = "custody_chain_block_height";
    pub const LAST_SCAN_BLOCK: &str = "custody_chain_last_scan_block";
    pub const SCAN_LAG_BLOCKS: &str = "custody_chain_scan_lag_blocks";
    pub const RPC_LATENCY: &str = "custody_chain_rpc_latency_seconds";
    pub const RPC_ERRORS_TOTAL: &str = "custody_chain_rpc_errors_total";
}

/// Metric names for signing.
pub mod signing {
    pub const REQUESTS_TOTAL: &str = "custody_signing_requests_total";
    pub const LATENCY: &str = "custody_signing_latency_seconds";
    pub const ERRORS_TOTAL: &str = "custody_signing_errors_total";
}

/// Metric names for policy engine.
pub mod policy {
    pub const EVALUATIONS_TOTAL: &str = "custody_policy_evaluations_total";
    pub const BLOCKED_TOTAL: &str = "custody_policy_blocked_total";
    pub const APPROVALS_PENDING: &str = "custody_policy_approvals_pending";
}

/// Metric names for compliance.
pub mod compliance {
    pub const SCREENINGS_TOTAL: &str = "custody_compliance_screenings_total";
    pub const BLOCKED_TOTAL: &str = "custody_compliance_blocked_total";
    pub const SANCTIONS_HITS_TOTAL: &str = "custody_compliance_sanctions_hits_total";
}

/// Metric names for wallets.
pub mod wallets {
    pub const HOT_BALANCE: &str = "custody_wallet_hot_balance";
    pub const WARM_BALANCE: &str = "custody_wallet_warm_balance";
    pub const COLD_BALANCE: &str = "custody_wallet_cold_balance";
    pub const SWEEP_TOTAL: &str = "custody_wallet_sweeps_total";
}

/// Standard label names.
pub mod labels {
    pub const CHAIN: &str = "chain";
    pub const ASSET: &str = "asset";
    pub const STATUS: &str = "status";
    pub const TIER: &str = "tier";
    pub const PROVIDER: &str = "provider";
}

/// Health check status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
        }
    }
}

/// Per-chain health check result.
#[derive(Debug, Clone)]
pub struct ChainHealth {
    pub chain: String,
    pub status: HealthStatus,
    pub block_height: u64,
    pub last_scan_block: u64,
    pub scan_lag: u64,
    pub rpc_latency_ms: u64,
}

impl ChainHealth {
    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_names_have_prefix() {
        assert!(deposits::DETECTED_TOTAL.starts_with("custody_"));
        assert!(withdrawals::RECEIVED_TOTAL.starts_with("custody_"));
        assert!(chain_health::BLOCK_HEIGHT.starts_with("custody_"));
    }

    #[test]
    fn health_status() {
        assert_eq!(HealthStatus::Healthy.as_str(), "healthy");
        assert!(ChainHealth {
            chain: "ethereum".into(),
            status: HealthStatus::Healthy,
            block_height: 1000,
            last_scan_block: 999,
            scan_lag: 1,
            rpc_latency_ms: 50,
        }.is_healthy());
    }

    #[test]
    fn unhealthy_chain() {
        let health = ChainHealth {
            chain: "bitcoin".into(),
            status: HealthStatus::Unhealthy,
            block_height: 0,
            last_scan_block: 0,
            scan_lag: 0,
            rpc_latency_ms: 30000,
        };
        assert!(!health.is_healthy());
    }
}
