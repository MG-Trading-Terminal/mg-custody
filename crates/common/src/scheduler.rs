//! Background task scheduler.
//!
//! Manages periodic tasks: OFAC refresh, reconciliation, sweep checks,
//! chain health monitoring.

use std::time::Duration;
use serde::{Deserialize, Serialize};

/// A scheduled background task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub name: String,
    pub interval_secs: u64,
    pub enabled: bool,
    pub description: String,
}

/// Default scheduled tasks for the custody service.
pub fn default_tasks() -> Vec<ScheduledTask> {
    vec![
        ScheduledTask {
            name: "ethereum_scanner".into(),
            interval_secs: 12,
            enabled: true,
            description: "Scan Ethereum blocks for deposits".into(),
        },
        ScheduledTask {
            name: "bitcoin_scanner".into(),
            interval_secs: 60, // ~10 min blocks, check every minute
            enabled: true,
            description: "Scan Bitcoin blocks for deposits".into(),
        },
        ScheduledTask {
            name: "tron_scanner".into(),
            interval_secs: 3,
            enabled: true,
            description: "Scan Tron blocks for deposits".into(),
        },
        ScheduledTask {
            name: "solana_scanner".into(),
            interval_secs: 2,
            enabled: true,
            description: "Scan Solana slots for deposits".into(),
        },
        ScheduledTask {
            name: "confirmation_tracker".into(),
            interval_secs: 12,
            enabled: true,
            description: "Check deposit/withdrawal confirmations".into(),
        },
        ScheduledTask {
            name: "withdrawal_pipeline".into(),
            interval_secs: 5,
            enabled: true,
            description: "Process withdrawal state machine".into(),
        },
        ScheduledTask {
            name: "webhook_retry".into(),
            interval_secs: 30,
            enabled: true,
            description: "Retry failed webhook deliveries".into(),
        },
        ScheduledTask {
            name: "ofac_refresh".into(),
            interval_secs: 86400, // daily
            enabled: true,
            description: "Refresh OFAC SDN sanctions list".into(),
        },
        ScheduledTask {
            name: "hot_reconciliation".into(),
            interval_secs: 600, // 10 minutes
            enabled: true,
            description: "Reconcile hot wallet on-chain vs ledger".into(),
        },
        ScheduledTask {
            name: "cold_reconciliation".into(),
            interval_secs: 21600, // 6 hours
            enabled: true,
            description: "Reconcile warm/cold wallets".into(),
        },
        ScheduledTask {
            name: "sweep_check".into(),
            interval_secs: 60,
            enabled: true,
            description: "Check tier thresholds for sweep needs".into(),
        },
        ScheduledTask {
            name: "price_refresh".into(),
            interval_secs: 60,
            enabled: true,
            description: "Refresh asset USD prices from CoinGecko".into(),
        },
        ScheduledTask {
            name: "chain_health".into(),
            interval_secs: 30,
            enabled: true,
            description: "Check RPC node health per chain".into(),
        },
        ScheduledTask {
            name: "presigning_refill".into(),
            interval_secs: 60,
            enabled: true,
            description: "Refill MPC presigning cache".into(),
        },
        ScheduledTask {
            name: "approval_expiry".into(),
            interval_secs: 60,
            enabled: true,
            description: "Expire timed-out approval requests".into(),
        },
    ]
}

/// Dynamic token import — add a token by contract address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenImportRequest {
    pub chain: String,
    pub contract_address: String,
    /// If not provided, will be fetched on-chain (name, symbol, decimals).
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
}

/// Chain configuration that can be hot-reloaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainHotConfig {
    pub chain: String,
    pub enabled: bool,
    pub rpc_url: String,
    pub fallback_rpc_urls: Vec<String>,
    pub required_confirmations: u64,
    pub max_gas_price: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tasks_count() {
        let tasks = default_tasks();
        assert!(tasks.len() >= 14);
        // All should have names
        for task in &tasks {
            assert!(!task.name.is_empty());
            assert!(task.interval_secs > 0);
        }
    }

    #[test]
    fn ofac_refresh_daily() {
        let tasks = default_tasks();
        let ofac = tasks.iter().find(|t| t.name == "ofac_refresh").unwrap();
        assert_eq!(ofac.interval_secs, 86400);
    }

    #[test]
    fn hot_reconciliation_10min() {
        let tasks = default_tasks();
        let recon = tasks.iter().find(|t| t.name == "hot_reconciliation").unwrap();
        assert_eq!(recon.interval_secs, 600);
    }

    #[test]
    fn token_import_request() {
        let req = TokenImportRequest {
            chain: "ethereum".into(),
            contract_address: "0x1234...".into(),
            symbol: Some("WBTC".into()),
            decimals: Some(8),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("WBTC"));
    }
}
