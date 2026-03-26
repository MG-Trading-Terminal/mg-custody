//! Settlement protocol — inter-custody messaging, bilateral netting execution.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Settlement session between two custodians.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementSession {
    pub id: Uuid,
    pub initiator: String,
    pub counterparty: String,
    pub status: SessionStatus,
    pub orders: Vec<SettlementLeg>,
    pub net_result: Option<NettingResult>,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Proposed,
    Accepted,
    NettingComplete,
    Settling,
    Settled,
    Rejected,
    Expired,
}

/// A single leg of a settlement (one direction).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementLeg {
    pub id: Uuid,
    pub from: String,
    pub to: String,
    pub asset: String,
    pub amount: Decimal,
}

/// Result of bilateral netting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NettingResult {
    pub gross_legs: u32,
    pub net_legs: u32,
    pub savings_percent: Decimal,
    pub net_transfers: Vec<NetTransfer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetTransfer {
    pub from: String,
    pub to: String,
    pub asset: String,
    pub net_amount: Decimal,
}

/// Execute bilateral netting on a set of legs.
pub fn bilateral_net(legs: &[SettlementLeg]) -> NettingResult {
    use std::collections::HashMap;
    // Group by (asset, pair) — net A→B against B→A
    let mut flows: HashMap<(String, String, String), Decimal> = HashMap::new();

    for leg in legs {
        let key = if leg.from < leg.to {
            (leg.asset.clone(), leg.from.clone(), leg.to.clone())
        } else {
            (leg.asset.clone(), leg.to.clone(), leg.from.clone())
        };
        let direction = if leg.from < leg.to { Decimal::ONE } else { -Decimal::ONE };
        *flows.entry(key).or_insert(Decimal::ZERO) += leg.amount * direction;
    }

    let net_transfers: Vec<NetTransfer> = flows.into_iter()
        .filter(|(_, amount)| *amount != Decimal::ZERO)
        .map(|((asset, party_a, party_b), amount)| {
            if amount > Decimal::ZERO {
                NetTransfer { from: party_a, to: party_b, asset, net_amount: amount }
            } else {
                NetTransfer { from: party_b, to: party_a, asset, net_amount: -amount }
            }
        })
        .collect();

    let gross = legs.len() as u32;
    let net = net_transfers.len() as u32;
    let savings = if gross > 0 {
        Decimal::new(100, 0) * Decimal::new((gross - net) as i64, 0) / Decimal::new(gross as i64, 0)
    } else {
        Decimal::ZERO
    };

    NettingResult {
        gross_legs: gross,
        net_legs: net,
        savings_percent: savings,
        net_transfers,
    }
}

/// Inter-custody protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolMessage {
    Hello { custody_id: String, version: String, supported_assets: Vec<String> },
    ProposeSettlement { session_id: Uuid, legs: Vec<SettlementLeg> },
    AcceptSettlement { session_id: Uuid },
    RejectSettlement { session_id: Uuid, reason: String },
    ConfirmExecution { session_id: Uuid, tx_hashes: Vec<String> },
    Heartbeat { custody_id: String, timestamp: DateTime<Utc> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn bilateral_netting_basic() {
        let legs = vec![
            SettlementLeg { id: Uuid::nil(), from: "A".into(), to: "B".into(), asset: "USDT".into(), amount: dec!(1000) },
            SettlementLeg { id: Uuid::nil(), from: "B".into(), to: "A".into(), asset: "USDT".into(), amount: dec!(600) },
        ];
        let result = bilateral_net(&legs);
        assert_eq!(result.gross_legs, 2);
        assert_eq!(result.net_legs, 1);
        assert_eq!(result.net_transfers[0].net_amount, dec!(400));
        assert_eq!(result.net_transfers[0].from, "A");
    }

    #[test]
    fn bilateral_netting_perfect_offset() {
        let legs = vec![
            SettlementLeg { id: Uuid::nil(), from: "A".into(), to: "B".into(), asset: "ETH".into(), amount: dec!(5) },
            SettlementLeg { id: Uuid::nil(), from: "B".into(), to: "A".into(), asset: "ETH".into(), amount: dec!(5) },
        ];
        let result = bilateral_net(&legs);
        assert_eq!(result.net_legs, 0); // perfectly offset
        assert_eq!(result.savings_percent, dec!(100));
    }

    #[test]
    fn bilateral_netting_multi_asset() {
        let legs = vec![
            SettlementLeg { id: Uuid::nil(), from: "A".into(), to: "B".into(), asset: "USDT".into(), amount: dec!(1000) },
            SettlementLeg { id: Uuid::nil(), from: "A".into(), to: "B".into(), asset: "ETH".into(), amount: dec!(2) },
            SettlementLeg { id: Uuid::nil(), from: "B".into(), to: "A".into(), asset: "USDT".into(), amount: dec!(300) },
        ];
        let result = bilateral_net(&legs);
        assert_eq!(result.gross_legs, 3);
        assert_eq!(result.net_legs, 2); // USDT netted, ETH stays
    }

    #[test]
    fn protocol_message_hello() {
        let msg = ProtocolMessage::Hello {
            custody_id: "mg-custody-01".into(),
            version: "1.0".into(),
            supported_assets: vec!["BTC".into(), "ETH".into(), "USDT".into()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("mg-custody-01"));
    }

    #[test]
    fn session_status_serialization() {
        let json = serde_json::to_string(&SessionStatus::NettingComplete).unwrap();
        assert_eq!(json, r#""netting_complete""#);
    }
}
