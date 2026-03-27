//! Network topology — RPC node management, health monitoring, failover.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::Chain;

/// An RPC node endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcNode {
    pub url: String,
    pub chain: Chain,
    pub provider: String,
    pub priority: u32,
    pub is_healthy: bool,
    pub latency_ms: Option<u32>,
    pub block_height: Option<u64>,
    pub last_check_at: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
    pub requests_total: u64,
    pub errors_total: u64,
}

impl RpcNode {
    pub fn error_rate(&self) -> f32 {
        if self.requests_total == 0 { return 0.0; }
        self.errors_total as f32 / self.requests_total as f32
    }

    pub fn is_usable(&self) -> bool {
        self.is_healthy && self.consecutive_failures < 3
    }
}

/// RPC provider types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcProvider {
    SelfHosted,
    Alchemy,
    Infura,
    QuickNode,
    Chainstack,
    Ankr,
    PublicNode,
}

/// Select best RPC node from a list (lowest latency + healthy).
pub fn select_best_node(nodes: &[RpcNode]) -> Option<&RpcNode> {
    nodes.iter()
        .filter(|n| n.is_usable())
        .min_by_key(|n| (n.priority, n.latency_ms.unwrap_or(u32::MAX)))
}

/// Multi-region deployment topology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentTopology {
    pub regions: Vec<RegionConfig>,
    pub active_region: String,
    pub failover_strategy: FailoverStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionConfig {
    pub name: String,
    pub is_primary: bool,
    pub db_endpoint: String,
    pub rpc_nodes: Vec<String>,
    pub signer_endpoint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailoverStrategy { Manual, Automatic, DnsBasedec }

/// Node health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealthCheck {
    pub url: String,
    pub reachable: bool,
    pub latency_ms: u32,
    pub block_height: Option<u64>,
    pub version: Option<String>,
    pub checked_at: DateTime<Utc>,
}

pub fn check_node_health_request(chain: Chain) -> serde_json::Value {
    match chain {
        Chain::Ethereum => serde_json::json!({"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}),
        Chain::Bitcoin => serde_json::json!({"jsonrpc":"2.0","id":1,"method":"getblockcount","params":[]}),
        Chain::Solana => serde_json::json!({"jsonrpc":"2.0","id":1,"method":"getSlot","params":[{"commitment":"finalized"}]}),
        Chain::Tron => serde_json::json!({}), // Tron uses HTTP, not JSON-RPC
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_node(url: &str, healthy: bool, latency: u32, priority: u32) -> RpcNode {
        RpcNode {
            url: url.into(), chain: Chain::Ethereum, provider: "test".into(),
            priority, is_healthy: healthy, latency_ms: Some(latency),
            block_height: Some(19500000), last_check_at: Some(Utc::now()),
            consecutive_failures: if healthy { 0 } else { 5 },
            requests_total: 1000, errors_total: if healthy { 10 } else { 500 },
        }
    }

    #[test]
    fn select_lowest_latency() {
        let nodes = vec![
            test_node("fast", true, 20, 1),
            test_node("slow", true, 200, 1),
        ];
        let best = select_best_node(&nodes).unwrap();
        assert_eq!(best.url, "fast");
    }

    #[test]
    fn skip_unhealthy() {
        let nodes = vec![
            test_node("fast_broken", false, 10, 1),
            test_node("slow_ok", true, 200, 1),
        ];
        let best = select_best_node(&nodes).unwrap();
        assert_eq!(best.url, "slow_ok");
    }

    #[test]
    fn priority_first() {
        let nodes = vec![
            test_node("low_pri", true, 10, 2),
            test_node("high_pri", true, 50, 1),
        ];
        let best = select_best_node(&nodes).unwrap();
        assert_eq!(best.url, "high_pri"); // priority 1 > latency 10
    }

    #[test]
    fn error_rate() {
        let n = test_node("test", true, 50, 1);
        assert!(n.error_rate() < 0.02); // 10/1000 = 1%
    }

    #[test]
    fn no_usable_nodes() {
        let nodes = vec![test_node("broken", false, 10, 1)];
        assert!(select_best_node(&nodes).is_none());
    }

    #[test]
    fn health_check_request_eth() {
        let req = check_node_health_request(Chain::Ethereum);
        assert_eq!(req["method"], "eth_blockNumber");
    }

    #[test]
    fn health_check_request_sol() {
        let req = check_node_health_request(Chain::Solana);
        assert_eq!(req["method"], "getSlot");
    }

    #[test]
    fn rpc_provider_serialization() {
        assert_eq!(serde_json::to_string(&RpcProvider::QuickNode).unwrap(), r#""quick_node""#);
    }

    #[test]
    fn failover_strategy() {
        assert_eq!(serde_json::to_string(&FailoverStrategy::Automatic).unwrap(), r#""automatic""#);
    }
}
