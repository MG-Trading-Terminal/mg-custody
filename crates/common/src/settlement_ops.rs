//! Settlement execution layer + counterparty API protocol.
//!
//! Internal off-chain settlement between custody clients.
//! Counterparty discovery and authenticated transfer protocol.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Internal settlement: instant transfer between vaults without on-chain tx.
pub struct SettlementEngine {
    /// Pending settlements awaiting confirmation.
    pending: Vec<SettlementOrder>,
}

/// A settlement order between two parties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementOrder {
    pub id: Uuid,
    pub from_party: String,
    pub to_party: String,
    pub asset: String,
    pub amount: Decimal,
    pub status: SettlementOrderStatus,
    pub netting_group: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementOrderStatus {
    Pending,
    Matched,
    Netted,
    Settled,
    Failed,
}

impl SettlementEngine {
    pub fn new() -> Self { Self { pending: Vec::new() } }

    /// Submit a settlement order.
    pub fn submit(&mut self, order: SettlementOrder) -> Uuid {
        let id = order.id;
        self.pending.push(order);
        id
    }

    /// Net opposing orders (A→B + B→A = reduced amounts).
    pub fn net_orders(&mut self) -> Vec<NettingResult> {
        let mut results = Vec::new();
        // Simple bilateral netting
        let mut i = 0;
        while i < self.pending.len() {
            let mut j = i + 1;
            while j < self.pending.len() {
                if self.pending[i].from_party == self.pending[j].to_party
                    && self.pending[i].to_party == self.pending[j].from_party
                    && self.pending[i].asset == self.pending[j].asset
                {
                    let net = self.pending[i].amount - self.pending[j].amount;
                    results.push(NettingResult {
                        order_a: self.pending[i].id,
                        order_b: self.pending[j].id,
                        original_total: self.pending[i].amount + self.pending[j].amount,
                        netted_amount: net.abs(),
                        savings: self.pending[i].amount.min(self.pending[j].amount) * Decimal::new(2, 0),
                    });
                }
                j += 1;
            }
            i += 1;
        }
        results
    }

    pub fn pending_count(&self) -> usize { self.pending.len() }
}

impl Default for SettlementEngine {
    fn default() -> Self { Self::new() }
}

/// Result of netting two opposing orders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NettingResult {
    pub order_a: Uuid,
    pub order_b: Uuid,
    pub original_total: Decimal,
    pub netted_amount: Decimal,
    pub savings: Decimal,
}

/// Counterparty API protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CounterpartyMessage {
    /// Discovery: "I'm a custody provider, here are my capabilities"
    Hello { provider_name: String, supported_assets: Vec<String>, api_version: String },
    /// Settlement request
    SettlementRequest { order: SettlementOrder },
    /// Settlement confirmation
    SettlementConfirm { order_id: Uuid, tx_hash: Option<String> },
    /// Settlement rejection
    SettlementReject { order_id: Uuid, reason: String },
}

/// Workspace billing entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingEntry {
    pub workspace_id: Uuid,
    pub period: String, // "2026-03"
    pub api_calls: u64,
    pub deposits_processed: u64,
    pub withdrawals_processed: u64,
    pub storage_gb: f64,
    pub total_usd: Decimal,
}

/// Multi-tenant workspace isolation check.
pub fn validate_workspace_access(workspace_id: Uuid, resource_workspace: Uuid) -> bool {
    workspace_id == resource_workspace
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_order(from: &str, to: &str, amount: Decimal) -> SettlementOrder {
        SettlementOrder {
            id: Uuid::new_v4(), from_party: from.into(), to_party: to.into(),
            asset: "USDT".into(), amount, status: SettlementOrderStatus::Pending,
            netting_group: None, created_at: Utc::now(), settled_at: None,
        }
    }

    #[test]
    fn settlement_submit() {
        let mut engine = SettlementEngine::new();
        engine.submit(make_order("A", "B", dec!(1000)));
        assert_eq!(engine.pending_count(), 1);
    }

    #[test]
    fn settlement_netting() {
        let mut engine = SettlementEngine::new();
        engine.submit(make_order("A", "B", dec!(1000)));
        engine.submit(make_order("B", "A", dec!(700)));
        let results = engine.net_orders();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].netted_amount, dec!(300)); // 1000-700
    }

    #[test]
    fn counterparty_protocol() {
        let msg = CounterpartyMessage::Hello {
            provider_name: "MG Custody".into(),
            supported_assets: vec!["BTC".into(), "ETH".into()],
            api_version: "1.0".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("MG Custody"));
    }

    #[test]
    fn workspace_isolation() {
        let ws1 = Uuid::from_u128(1);
        let ws2 = Uuid::from_u128(2);
        assert!(validate_workspace_access(ws1, ws1));
        assert!(!validate_workspace_access(ws1, ws2));
    }

    #[test]
    fn billing_entry() {
        let entry = BillingEntry {
            workspace_id: Uuid::new_v4(), period: "2026-03".into(),
            api_calls: 10000, deposits_processed: 500, withdrawals_processed: 200,
            storage_gb: 5.5, total_usd: dec!(299),
        };
        assert_eq!(entry.api_calls, 10000);
    }
}
