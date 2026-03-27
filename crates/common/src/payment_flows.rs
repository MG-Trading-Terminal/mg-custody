//! Payment flows — automated payment workflows, payouts, DVP settlements.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Asset, Chain};

/// A payment flow (automated multi-step payment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentFlow {
    pub id: Uuid,
    pub name: String,
    pub status: PaymentFlowStatus,
    pub steps: Vec<PaymentStep>,
    pub created_at: DateTime<Utc>,
    pub executed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentFlowStatus {
    Draft,
    Pending,
    Executing,
    Completed,
    PartiallyCompleted,
    Failed,
    Cancelled,
}

/// A single step in a payment flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentStep {
    pub index: u32,
    pub step_type: PaymentStepType,
    pub status: PaymentFlowStatus,
    pub tx_hash: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentStepType {
    Transfer { source_vault_id: Uuid, dest_address: String, asset: Asset, chain: Chain, amount: Decimal },
    InternalTransfer { source_vault_id: Uuid, dest_vault_id: Uuid, asset: Asset, chain: Chain, amount: Decimal },
    Approval { asset: Asset, chain: Chain, spender: String, amount: Decimal },
    ContractCall { vault_id: Uuid, chain: Chain, contract: String, calldata: String },
    Wait { seconds: u32 },
}

/// A payout instruction set (batch payroll/vendor payments).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayoutInstruction {
    pub id: Uuid,
    pub payee: String,
    pub address: String,
    pub chain: Chain,
    pub asset: Asset,
    pub amount: Decimal,
    pub reference: Option<String>,
    pub status: PayoutStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayoutStatus { Pending, Approved, Executing, Completed, Failed }

/// Smart Transfer — DVP (Delivery vs Payment) settlement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartTransfer {
    pub id: Uuid,
    pub ticket_id: String,
    pub legs: Vec<SmartTransferLeg>,
    pub status: SmartTransferStatus,
    pub counterparty: String,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartTransferLeg {
    pub asset: Asset,
    pub chain: Chain,
    pub amount: Decimal,
    pub direction: LegDirection,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegDirection { Send, Receive }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartTransferStatus { Created, Pending, Settling, Settled, Cancelled, Expired }

/// Sweep to Omnibus — consolidate all vault funds to a single address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepToOmnibus {
    pub id: Uuid,
    pub chain: Chain,
    pub asset: Asset,
    pub omnibus_address: String,
    pub source_vaults: Vec<Uuid>,
    pub total_amount: Decimal,
    pub tx_hashes: Vec<String>,
    pub status: PayoutStatus,
    pub created_at: DateTime<Utc>,
}

/// Fiat account link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiatAccount {
    pub id: Uuid,
    pub provider: FiatProvider,
    pub account_name: String,
    pub currency: String,
    pub balance: Option<Decimal>,
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FiatProvider { Silvergate, Signature, BankFrick, Bcb, Custom }

/// User group for permission management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserGroup {
    pub id: Uuid,
    pub name: String,
    pub members: Vec<Uuid>,
    pub permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Job for async bulk operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkJob {
    pub id: Uuid,
    pub job_type: BulkJobType,
    pub total_items: u32,
    pub completed_items: u32,
    pub failed_items: u32,
    pub status: PayoutStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkJobType { CreateVaults, CreateAddresses, BatchWithdrawals, SweepAll }

/// Trading quote/order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingQuote {
    pub id: Uuid,
    pub from_asset: String,
    pub to_asset: String,
    pub from_amount: Decimal,
    pub to_amount: Decimal,
    pub rate: Decimal,
    pub provider: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingOrder {
    pub id: Uuid,
    pub quote_id: Uuid,
    pub status: TradingOrderStatus,
    pub executed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradingOrderStatus { Pending, Executed, Failed, Expired }

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn payment_flow_status() {
        assert_eq!(serde_json::to_string(&PaymentFlowStatus::Executing).unwrap(), r#""executing""#);
        assert_eq!(serde_json::to_string(&PaymentFlowStatus::PartiallyCompleted).unwrap(), r#""partially_completed""#);
    }

    #[test]
    fn payment_step_transfer() {
        let step = PaymentStepType::Transfer {
            source_vault_id: Uuid::nil(), dest_address: "0x1".into(),
            asset: Asset::new("ETH"), chain: Chain::Ethereum, amount: dec!(1.5),
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("transfer"));
    }

    #[test]
    fn payment_step_wait() {
        let step = PaymentStepType::Wait { seconds: 60 };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("wait"));
        assert!(json.contains("60"));
    }

    #[test]
    fn smart_transfer_dvp() {
        let st = SmartTransfer {
            id: Uuid::nil(), ticket_id: "DVP-001".into(),
            legs: vec![
                SmartTransferLeg { asset: Asset::new("USDC"), chain: Chain::Ethereum, amount: dec!(100000), direction: LegDirection::Send, tx_hash: None },
                SmartTransferLeg { asset: Asset::new("BTC"), chain: Chain::Bitcoin, amount: dec!(1.5), direction: LegDirection::Receive, tx_hash: None },
            ],
            status: SmartTransferStatus::Created, counterparty: "OTC Desk".into(),
            created_at: Utc::now(), settled_at: None,
        };
        assert_eq!(st.legs.len(), 2);
    }

    #[test]
    fn fiat_provider_serialization() {
        assert_eq!(serde_json::to_string(&FiatProvider::BankFrick).unwrap(), r#""bank_frick""#);
    }

    #[test]
    fn bulk_job_type() {
        assert_eq!(serde_json::to_string(&BulkJobType::CreateVaults).unwrap(), r#""create_vaults""#);
        assert_eq!(serde_json::to_string(&BulkJobType::SweepAll).unwrap(), r#""sweep_all""#);
    }

    #[test]
    fn trading_quote() {
        let quote = TradingQuote {
            id: Uuid::nil(), from_asset: "USDC".into(), to_asset: "ETH".into(),
            from_amount: dec!(3000), to_amount: dec!(1), rate: dec!(3000),
            provider: "B2C2".into(), expires_at: Utc::now(),
        };
        let json = serde_json::to_string(&quote).unwrap();
        assert!(json.contains("B2C2"));
    }

    #[test]
    fn sweep_to_omnibus() {
        let sweep = SweepToOmnibus {
            id: Uuid::nil(), chain: Chain::Ethereum, asset: Asset::new("USDC"),
            omnibus_address: "0xomnibus".into(), source_vaults: vec![Uuid::nil()],
            total_amount: dec!(1000000), tx_hashes: vec![],
            status: PayoutStatus::Pending, created_at: Utc::now(),
        };
        let json = serde_json::to_string(&sweep).unwrap();
        assert!(json.contains("omnibus"));
    }

    #[test]
    fn user_group() {
        let group = UserGroup {
            id: Uuid::nil(), name: "Traders".into(),
            members: vec![Uuid::nil()], permissions: vec!["transaction_write".into()],
            created_at: Utc::now(),
        };
        assert_eq!(group.name, "Traders");
    }

    #[test]
    fn trading_order_status() {
        assert_eq!(serde_json::to_string(&TradingOrderStatus::Executed).unwrap(), r#""executed""#);
    }
}
