//! WalletConnect v2 protocol types — session management, request handling.
//!
//! Implements the WalletConnect v2 Sign API types for dApp connectivity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A WalletConnect v2 pairing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcPairing {
    pub topic: String,
    pub relay_protocol: String,
    pub relay_data: Option<String>,
    pub uri: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub expiry: DateTime<Utc>,
}

/// A WalletConnect v2 session (established after approval).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcSession {
    pub topic: String,
    pub pairing_topic: String,
    pub relay_protocol: String,
    pub controller_public_key: String,
    pub peer: WcPeerMetadata,
    pub namespaces: Vec<WcNamespace>,
    pub expiry: DateTime<Utc>,
    pub acknowledged: bool,
    pub created_at: DateTime<Utc>,
}

/// Metadata about the dApp peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcPeerMetadata {
    pub name: String,
    pub description: String,
    pub url: String,
    pub icons: Vec<String>,
}

/// A namespace defines what chains/methods/events a session supports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcNamespace {
    pub chain_namespace: String, // "eip155", "solana", etc.
    pub chains: Vec<String>,    // "eip155:1", "eip155:42161"
    pub methods: Vec<String>,   // "eth_sendTransaction", "personal_sign"
    pub events: Vec<String>,    // "chainChanged", "accountsChanged"
    pub accounts: Vec<String>,  // "eip155:1:0xabc..."
}

/// A JSON-RPC request from a dApp via WalletConnect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcRequest {
    pub id: u64,
    pub topic: String,
    pub method: String,
    pub params: serde_json::Value,
    pub chain_id: String,
    pub received_at: DateTime<Utc>,
}

/// Standard WalletConnect methods for EVM.
pub mod evm_methods {
    pub const ETH_SEND_TRANSACTION: &str = "eth_sendTransaction";
    pub const ETH_SIGN_TRANSACTION: &str = "eth_signTransaction";
    pub const PERSONAL_SIGN: &str = "personal_sign";
    pub const ETH_SIGN: &str = "eth_sign";
    pub const ETH_SIGN_TYPED_DATA: &str = "eth_signTypedData";
    pub const ETH_SIGN_TYPED_DATA_V4: &str = "eth_signTypedData_v4";
    pub const WALLET_ADD_ETHEREUM_CHAIN: &str = "wallet_addEthereumChain";
    pub const WALLET_SWITCH_ETHEREUM_CHAIN: &str = "wallet_switchEthereumChain";
}

/// Standard WalletConnect methods for Solana.
pub mod solana_methods {
    pub const SIGN_TRANSACTION: &str = "solana_signTransaction";
    pub const SIGN_MESSAGE: &str = "solana_signMessage";
    pub const SIGN_ALL_TRANSACTIONS: &str = "solana_signAllTransactions";
}

/// Build EIP-155 namespace for a set of chain IDs.
pub fn eip155_namespace(chain_ids: &[u64], accounts: &[String]) -> WcNamespace {
    WcNamespace {
        chain_namespace: "eip155".into(),
        chains: chain_ids.iter().map(|id| format!("eip155:{id}")).collect(),
        methods: vec![
            evm_methods::ETH_SEND_TRANSACTION.into(),
            evm_methods::PERSONAL_SIGN.into(),
            evm_methods::ETH_SIGN_TYPED_DATA_V4.into(),
        ],
        events: vec!["chainChanged".into(), "accountsChanged".into()],
        accounts: accounts.to_vec(),
    }
}

/// Build Solana namespace.
pub fn solana_namespace(accounts: &[String]) -> WcNamespace {
    WcNamespace {
        chain_namespace: "solana".into(),
        chains: vec!["solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp".into()],
        methods: vec![
            solana_methods::SIGN_TRANSACTION.into(),
            solana_methods::SIGN_MESSAGE.into(),
        ],
        events: vec![],
        accounts: accounts.to_vec(),
    }
}

/// Parse a WalletConnect URI (wc:topic@version?...)
pub fn parse_wc_uri(uri: &str) -> Option<(String, u8, String)> {
    if !uri.starts_with("wc:") { return None; }
    let rest = &uri[3..];
    let at_pos = rest.find('@')?;
    let topic = rest[..at_pos].to_string();
    let after_at = &rest[at_pos + 1..];
    let q_pos = after_at.find('?').unwrap_or(after_at.len());
    let version: u8 = after_at[..q_pos].parse().ok()?;
    let params = if q_pos < after_at.len() { after_at[q_pos + 1..].to_string() } else { String::new() };
    Some((topic, version, params))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wc_uri_v2() {
        let uri = "wc:7f6e504bfad60b485450578e05678ed3e8e8c4751d3c6160be17160d63ec90f9@2?relay-protocol=irn&symKey=abc";
        let (topic, version, params) = parse_wc_uri(uri).unwrap();
        assert_eq!(topic, "7f6e504bfad60b485450578e05678ed3e8e8c4751d3c6160be17160d63ec90f9");
        assert_eq!(version, 2);
        assert!(params.contains("relay-protocol"));
    }

    #[test]
    fn parse_wc_uri_invalid() {
        assert!(parse_wc_uri("http://example.com").is_none());
        assert!(parse_wc_uri("wc:").is_none());
    }

    #[test]
    fn eip155_namespace_building() {
        let ns = eip155_namespace(
            &[1, 42161],
            &["eip155:1:0xabc".into(), "eip155:42161:0xabc".into()],
        );
        assert_eq!(ns.chain_namespace, "eip155");
        assert_eq!(ns.chains.len(), 2);
        assert!(ns.methods.contains(&evm_methods::PERSONAL_SIGN.to_string()));
    }

    #[test]
    fn solana_namespace_building() {
        let ns = solana_namespace(&["solana:mainnet:addr1".into()]);
        assert_eq!(ns.chain_namespace, "solana");
        assert_eq!(ns.chains.len(), 1);
        assert!(ns.methods.contains(&solana_methods::SIGN_TRANSACTION.to_string()));
    }

    #[test]
    fn wc_session_serialization() {
        let session = WcSession {
            topic: "abc123".into(),
            pairing_topic: "def456".into(),
            relay_protocol: "irn".into(),
            controller_public_key: "0xpubkey".into(),
            peer: WcPeerMetadata {
                name: "My dApp".into(),
                description: "A cool dApp".into(),
                url: "https://myapp.xyz".into(),
                icons: vec!["https://myapp.xyz/icon.png".into()],
            },
            namespaces: vec![eip155_namespace(&[1], &["eip155:1:0xabc".into()])],
            expiry: Utc::now(),
            acknowledged: true,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("My dApp"));
        assert!(json.contains("eip155"));
    }

    #[test]
    fn wc_request_structure() {
        let req = WcRequest {
            id: 1234,
            topic: "topic123".into(),
            method: evm_methods::ETH_SEND_TRANSACTION.into(),
            params: serde_json::json!([{"from": "0x1", "to": "0x2", "value": "0x1"}]),
            chain_id: "eip155:1".into(),
            received_at: Utc::now(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("eth_sendTransaction"));
    }

    #[test]
    fn evm_method_constants() {
        assert_eq!(evm_methods::ETH_SEND_TRANSACTION, "eth_sendTransaction");
        assert_eq!(evm_methods::PERSONAL_SIGN, "personal_sign");
        assert_eq!(evm_methods::ETH_SIGN_TYPED_DATA_V4, "eth_signTypedData_v4");
    }
}
