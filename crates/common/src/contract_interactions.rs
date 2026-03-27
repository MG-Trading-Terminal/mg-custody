//! Smart contract interactions — read/write calls, ABI management, deployment tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

/// A deployed smart contract tracked by the custody.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedContract {
    pub id: Uuid,
    pub address: String,
    pub chain: Chain,
    pub name: Option<String>,
    pub abi_hash: Option<String>,
    pub deployer_vault_id: Option<Uuid>,
    pub deploy_tx_hash: Option<String>,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
}

/// ABI template for contract interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractTemplate {
    pub id: Uuid,
    pub name: String,
    pub abi: serde_json::Value,
    pub bytecode: Option<String>,
    pub compiler_version: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A contract function call request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCallRequest {
    pub vault_id: Uuid,
    pub chain: Chain,
    pub contract_address: String,
    pub function_name: String,
    pub parameters: Vec<ContractParam>,
    pub value_wei: Option<String>,
    pub is_read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractParam {
    pub name: String,
    pub param_type: String,
    pub value: serde_json::Value,
}

/// Result of a contract call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCallResult {
    pub success: bool,
    pub return_data: Option<serde_json::Value>,
    pub tx_hash: Option<String>,
    pub gas_used: Option<u64>,
    pub error: Option<String>,
}

/// Build eth_call for read-only contract interaction.
pub fn build_read_call(contract: &str, calldata: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "eth_call",
        "params": [{"to": contract, "data": calldata}, "latest"]
    })
}

/// Build eth_sendTransaction for write contract interaction.
pub fn build_write_call(
    from: &str, contract: &str, calldata: &str, value: &str, gas_limit: u64,
) -> serde_json::Value {
    serde_json::json!({
        "from": from, "to": contract,
        "data": calldata, "value": value,
        "gas": format!("0x{:x}", gas_limit),
    })
}

/// Common contract function selectors.
pub mod selectors {
    pub const TRANSFER: &str = "0xa9059cbb";
    pub const APPROVE: &str = "0x095ea7b3";
    pub const BALANCE_OF: &str = "0x70a08231";
    pub const TOTAL_SUPPLY: &str = "0x18160ddd";
    pub const DECIMALS: &str = "0x313ce567";
    pub const NAME: &str = "0x06fdde03";
    pub const SYMBOL: &str = "0x95d89b41";
    pub const ALLOWANCE: &str = "0xdd62ed3e";
    pub const TRANSFER_FROM: &str = "0x23b872dd";
    pub const OWNER_OF: &str = "0x6352211e"; // ERC-721
    pub const SAFE_TRANSFER_FROM: &str = "0x42842e0e"; // ERC-721
    pub const SET_APPROVAL_FOR_ALL: &str = "0xa22cb465";
    pub const IS_APPROVED_FOR_ALL: &str = "0xe985e9c5";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_call_request() {
        let req = build_read_call("0xcontract", "0x70a08231");
        assert_eq!(req["method"], "eth_call");
    }

    #[test]
    fn write_call_request() {
        let tx = build_write_call("0xfrom", "0xto", "0xdata", "0x0", 65000);
        assert!(tx["gas"].as_str().unwrap().starts_with("0x"));
    }

    #[test]
    fn contract_param_structure() {
        let param = ContractParam {
            name: "to".into(), param_type: "address".into(),
            value: serde_json::json!("0x123"),
        };
        let json = serde_json::to_string(&param).unwrap();
        assert!(json.contains("address"));
    }

    #[test]
    fn deployed_contract() {
        let c = DeployedContract {
            id: Uuid::nil(), address: "0xabc".into(), chain: Chain::Ethereum,
            name: Some("USDC Proxy".into()), abi_hash: None,
            deployer_vault_id: None, deploy_tx_hash: None,
            verified: true, created_at: Utc::now(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("USDC Proxy"));
    }

    #[test]
    fn selector_constants() {
        assert_eq!(selectors::TRANSFER, "0xa9059cbb");
        assert_eq!(selectors::BALANCE_OF, "0x70a08231");
        assert_eq!(selectors::OWNER_OF, "0x6352211e");
    }
}
