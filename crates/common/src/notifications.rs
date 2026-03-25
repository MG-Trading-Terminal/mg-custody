//! Push notification types for mobile approval workflows.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Push notification for pending withdrawal approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalNotification {
    pub id: Uuid,
    pub withdrawal_id: Uuid,
    pub title: String,
    pub body: String,
    pub amount: String,
    pub asset: String,
    pub destination: String,
    pub tier: u8,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Push notification provider (Firebase, APNs, or generic webhook).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushProvider {
    Firebase { server_key: String },
    Apns { key_id: String, team_id: String },
    Webhook { url: String },
}

/// Multi-region database configuration for DR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiRegionDbConfig {
    pub primary: DbRegion,
    pub replicas: Vec<DbRegion>,
    pub failover_strategy: FailoverStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbRegion {
    pub name: String,
    pub url: String,
    pub region: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailoverStrategy {
    /// Automatic failover to nearest replica.
    Automatic,
    /// Manual failover requires admin action.
    Manual,
    /// Read from replicas, write to primary.
    ReadReplica,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_serialization() {
        let n = ApprovalNotification {
            id: Uuid::new_v4(),
            withdrawal_id: Uuid::new_v4(),
            title: "Withdrawal Approval Required".into(),
            body: "10 ETH to 0x742d...".into(),
            amount: "10.0".into(),
            asset: "ETH".into(),
            destination: "0x742d35...".into(),
            tier: 2,
            expires_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&n).unwrap();
        assert!(json.contains("Withdrawal"));
    }

    #[test]
    fn failover_strategies() {
        assert_eq!(
            serde_json::to_string(&FailoverStrategy::Automatic).unwrap(),
            "\"automatic\""
        );
    }
}
