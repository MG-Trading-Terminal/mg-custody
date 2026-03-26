//! Cross-chain messaging — LayerZero, Wormhole, Axelar message types.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A cross-chain message/transfer tracking record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainTransfer {
    pub id: Uuid,
    pub protocol: CrossChainProtocol,
    pub source_chain_id: u64,
    pub dest_chain_id: u64,
    pub source_tx_hash: Option<String>,
    pub dest_tx_hash: Option<String>,
    pub token: String,
    pub amount: Decimal,
    pub sender: String,
    pub receiver: String,
    pub status: CrossChainStatus,
    pub message_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossChainProtocol {
    Wormhole,
    LayerZero,
    Axelar,
    Ccip, // Chainlink CCIP
    Hyperlane,
    NativeBridge, // L1 <-> L2 canonical
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossChainStatus {
    Initiated,
    SourceConfirmed,
    InTransit,
    DestPending,
    Completed,
    Failed,
    Refunded,
}

/// Wormhole VAA (Verified Action Approval).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WormholeVaa {
    pub emitter_chain: u16,
    pub emitter_address: String,
    pub sequence: u64,
    pub consistency_level: u8,
    pub payload: String,
    pub guardian_signatures: u8,
}

/// LayerZero message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerZeroMessage {
    pub src_chain_id: u16,
    pub dst_chain_id: u16,
    pub nonce: u64,
    pub src_address: String,
    pub dst_address: String,
    pub payload: String,
}

/// Chainlink CCIP message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CcipMessage {
    pub message_id: String,
    pub source_chain_selector: u64,
    pub dest_chain_selector: u64,
    pub sender: String,
    pub receiver: String,
    pub data: String,
    pub token_amounts: Vec<CcipTokenAmount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CcipTokenAmount {
    pub token: String,
    pub amount: String,
}

/// Chain IDs for cross-chain protocols.
pub mod chain_ids {
    // Wormhole chain IDs
    pub const WORMHOLE_ETHEREUM: u16 = 2;
    pub const WORMHOLE_SOLANA: u16 = 1;
    pub const WORMHOLE_BSC: u16 = 4;
    pub const WORMHOLE_POLYGON: u16 = 5;
    pub const WORMHOLE_AVALANCHE: u16 = 6;
    pub const WORMHOLE_ARBITRUM: u16 = 23;
    pub const WORMHOLE_OPTIMISM: u16 = 24;
    pub const WORMHOLE_BASE: u16 = 30;

    // LayerZero v2 endpoint IDs
    pub const LZ_ETHEREUM: u16 = 30101;
    pub const LZ_ARBITRUM: u16 = 30110;
    pub const LZ_OPTIMISM: u16 = 30111;
    pub const LZ_POLYGON: u16 = 30109;
    pub const LZ_BASE: u16 = 30184;
    pub const LZ_SOLANA: u16 = 30168;

    // CCIP chain selectors
    pub const CCIP_ETHEREUM: u64 = 5009297550715157269;
    pub const CCIP_ARBITRUM: u64 = 4949039107694359620;
    pub const CCIP_OPTIMISM: u64 = 3734403246176062136;
    pub const CCIP_POLYGON: u64 = 4051577828743386545;
    pub const CCIP_BASE: u64 = 15971525489660198786;
}

/// Estimate cross-chain transfer time.
pub fn estimate_transfer_time(protocol: CrossChainProtocol) -> u32 {
    match protocol {
        CrossChainProtocol::NativeBridge => 600,  // ~10 min for L2
        CrossChainProtocol::Wormhole => 900,       // ~15 min
        CrossChainProtocol::LayerZero => 120,      // ~2 min
        CrossChainProtocol::Axelar => 300,         // ~5 min
        CrossChainProtocol::Ccip => 180,           // ~3 min
        CrossChainProtocol::Hyperlane => 60,       // ~1 min
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn cross_chain_status_flow() {
        let statuses = vec![
            CrossChainStatus::Initiated, CrossChainStatus::SourceConfirmed,
            CrossChainStatus::InTransit, CrossChainStatus::DestPending,
            CrossChainStatus::Completed,
        ];
        for s in statuses {
            let json = serde_json::to_string(&s).unwrap();
            let parsed: CrossChainStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, parsed);
        }
    }

    #[test]
    fn protocol_serialization() {
        assert_eq!(serde_json::to_string(&CrossChainProtocol::Ccip).unwrap(), r#""ccip""#);
        assert_eq!(serde_json::to_string(&CrossChainProtocol::Hyperlane).unwrap(), r#""hyperlane""#);
    }

    #[test]
    fn transfer_time_estimates() {
        assert!(estimate_transfer_time(CrossChainProtocol::Hyperlane) <
                estimate_transfer_time(CrossChainProtocol::Wormhole));
    }

    #[test]
    fn wormhole_chain_ids() {
        assert_eq!(chain_ids::WORMHOLE_ETHEREUM, 2);
        assert_eq!(chain_ids::WORMHOLE_SOLANA, 1);
    }

    #[test]
    fn ccip_message_structure() {
        let msg = CcipMessage {
            message_id: "0xabc".into(),
            source_chain_selector: chain_ids::CCIP_ETHEREUM,
            dest_chain_selector: chain_ids::CCIP_ARBITRUM,
            sender: "0x1".into(), receiver: "0x2".into(), data: "0x".into(),
            token_amounts: vec![CcipTokenAmount { token: "USDC".into(), amount: "1000000".into() }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("source_chain_selector"));
    }

    #[test]
    fn cross_chain_transfer_tracking() {
        let transfer = CrossChainTransfer {
            id: Uuid::nil(), protocol: CrossChainProtocol::LayerZero,
            source_chain_id: 1, dest_chain_id: 42161,
            source_tx_hash: Some("0xabc".into()), dest_tx_hash: None,
            token: "USDC".into(), amount: dec!(10000),
            sender: "0x1".into(), receiver: "0x2".into(),
            status: CrossChainStatus::InTransit, message_hash: Some("0xhash".into()),
            created_at: Utc::now(), completed_at: None,
        };
        let json = serde_json::to_string(&transfer).unwrap();
        assert!(json.contains("in_transit"));
        assert!(json.contains("layer_zero"));
    }
}
