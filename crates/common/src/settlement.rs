//! Settlement network types (Phase 13).
//!
//! Internal settlement for instant transfers between custody clients.
//! Counterparty API for connecting with other custody providers.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A settlement transfer between two internal parties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementTransfer {
    pub id: Uuid,
    pub from_vault: String,
    pub to_vault: String,
    pub asset: String,
    pub amount: Decimal,
    pub status: SettlementStatus,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementStatus {
    Pending,
    Settled,
    Failed,
    Cancelled,
}

/// A counterparty connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counterparty {
    pub id: Uuid,
    pub name: String,
    pub api_url: String,
    pub status: CounterpartyStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterpartyStatus {
    Active,
    Suspended,
    Pending,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_status_serialization() {
        assert_eq!(serde_json::to_string(&SettlementStatus::Settled).unwrap(), "\"settled\"");
    }
}
