//! Web3 Provider — ethers.js/web3.js/web3.py compatible JSON-RPC proxy.
//!
//! Allows dApps and scripts to use custody as a signing backend
//! via standard Web3 provider interface.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported Web3 provider methods that custody handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Web3Method {
    EthAccounts,
    EthSign,
    PersonalSign,
    EthSignTransaction,
    EthSendTransaction,
    EthSignTypedDataV4,
    EthChainId,
    EthBlockNumber,
    EthGetBalance,
    EthGetTransactionCount,
    EthEstimateGas,
    EthGasPrice,
    EthCall,
    NetVersion,
}

impl Web3Method {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "eth_accounts" => Some(Self::EthAccounts),
            "eth_sign" => Some(Self::EthSign),
            "personal_sign" => Some(Self::PersonalSign),
            "eth_signTransaction" => Some(Self::EthSignTransaction),
            "eth_sendTransaction" => Some(Self::EthSendTransaction),
            "eth_signTypedData_v4" => Some(Self::EthSignTypedDataV4),
            "eth_chainId" => Some(Self::EthChainId),
            "eth_blockNumber" => Some(Self::EthBlockNumber),
            "eth_getBalance" => Some(Self::EthGetBalance),
            "eth_getTransactionCount" => Some(Self::EthGetTransactionCount),
            "eth_estimateGas" => Some(Self::EthEstimateGas),
            "eth_gasPrice" => Some(Self::EthGasPrice),
            "eth_call" => Some(Self::EthCall),
            "net_version" => Some(Self::NetVersion),
            _ => None,
        }
    }

    /// Does this method require signing (vs read-only)?
    pub fn requires_signing(&self) -> bool {
        matches!(self,
            Self::EthSign | Self::PersonalSign | Self::EthSignTransaction |
            Self::EthSendTransaction | Self::EthSignTypedDataV4
        )
    }
}

/// JSON-RPC request from Web3 provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: serde_json::Value,
}

/// JSON-RPC response to Web3 provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }

    pub fn error(id: serde_json::Value, code: i64, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".into(), id, result: None,
            error: Some(JsonRpcError { code, message: message.into(), data: None }),
        }
    }
}

/// Web3 provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Web3ProviderConfig {
    pub vault_id: Uuid,
    pub chain_id: u64,
    pub rpc_url: String,
    pub allowed_methods: Vec<String>,
}

/// API Co-signer configuration (SGX/AWS Nitro/GCP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoSignerConfig {
    pub id: Uuid,
    pub cosigner_type: CoSignerType,
    pub endpoint: String,
    pub public_key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoSignerType {
    SgxEnclave,
    AwsNitro,
    GcpConfidential,
    Software,
}

/// Network routing policy for asset groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPolicy {
    pub id: Uuid,
    pub name: String,
    pub asset_groups: Vec<AssetGroup>,
    pub default_route: RouteTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetGroup {
    pub name: String,
    pub assets: Vec<String>,
    pub chains: Vec<String>,
    pub route: RouteTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RouteTarget {
    Direct,
    ViaNetwork { network_id: String },
    ViaExchange { exchange_id: String },
    Blocked { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_web3_methods() {
        assert_eq!(Web3Method::from_str("eth_accounts"), Some(Web3Method::EthAccounts));
        assert_eq!(Web3Method::from_str("personal_sign"), Some(Web3Method::PersonalSign));
        assert_eq!(Web3Method::from_str("unknown"), None);
    }

    #[test]
    fn signing_methods_identified() {
        assert!(Web3Method::EthSendTransaction.requires_signing());
        assert!(Web3Method::PersonalSign.requires_signing());
        assert!(!Web3Method::EthBlockNumber.requires_signing());
        assert!(!Web3Method::EthCall.requires_signing());
    }

    #[test]
    fn jsonrpc_success_response() {
        let resp = JsonRpcResponse::success(serde_json::json!(1), serde_json::json!("0x1234"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("0x1234"));
        assert!(!json.contains("error"));
    }

    #[test]
    fn jsonrpc_error_response() {
        let resp = JsonRpcResponse::error(serde_json::json!(1), -32601, "Method not found");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("-32601"));
        assert!(!json.contains("result"));
    }

    #[test]
    fn cosigner_type_serialization() {
        assert_eq!(serde_json::to_string(&CoSignerType::AwsNitro).unwrap(), r#""aws_nitro""#);
        assert_eq!(serde_json::to_string(&CoSignerType::SgxEnclave).unwrap(), r#""sgx_enclave""#);
    }

    #[test]
    fn route_target_variants() {
        let direct = RouteTarget::Direct;
        let json = serde_json::to_string(&direct).unwrap();
        assert!(json.contains("direct"));

        let blocked = RouteTarget::Blocked { reason: "sanctioned".into() };
        let json = serde_json::to_string(&blocked).unwrap();
        assert!(json.contains("sanctioned"));
    }

    #[test]
    fn web3_request_structure() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(), id: serde_json::json!(1),
            method: "eth_sendTransaction".into(),
            params: serde_json::json!([{"from": "0x1", "to": "0x2"}]),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("eth_sendTransaction"));
    }
}
