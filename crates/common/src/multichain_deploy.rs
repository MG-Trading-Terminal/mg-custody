//! Multi-chain contract deployment + LayerZero OFT bridge management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

/// Multi-chain deployment plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentPlan {
    pub id: Uuid,
    pub name: String,
    pub template_id: Uuid,
    pub chains: Vec<ChainDeployment>,
    pub status: DeploymentStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainDeployment {
    pub chain: Chain,
    pub chain_id: u64,
    pub contract_address: Option<String>,
    pub tx_hash: Option<String>,
    pub status: DeploymentStatus,
    pub gas_used: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus { Planned, Deploying, Deployed, Failed, Verified }

/// LayerZero OFT (Omnichain Fungible Token) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerZeroOft {
    pub token_name: String,
    pub token_symbol: String,
    pub home_chain: Chain,
    pub home_address: String,
    pub peers: Vec<OftPeer>,
    pub dvn_config: DvnConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OftPeer {
    pub chain: Chain,
    pub chain_id: u64,
    pub lz_eid: u32,
    pub address: String,
    pub is_connected: bool,
}

/// DVN (Decentralized Verifier Network) configuration for LayerZero.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DvnConfig {
    pub required_dvns: Vec<String>,
    pub optional_dvns: Vec<String>,
    pub threshold: u32,
}

/// SDK generation config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkGenerationConfig {
    pub language: SdkLang,
    pub api_base_url: String,
    pub include_auth: bool,
    pub generate_types: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SdkLang { Rust, Python, TypeScript, Go, Java, CSharp, Ruby }

/// Generate TypeScript SDK method stub.
pub fn generate_ts_method(method: &str, http_method: &str, path: &str) -> String {
    format!(
        "  async {}(params?: Record<string, unknown>): Promise<unknown> {{\n    return this.request('{}', '{}', params);\n  }}\n",
        method, http_method, path
    )
}

/// Generate Python SDK method stub.
pub fn generate_py_method(method: &str, http_method: &str, path: &str) -> String {
    format!(
        "    def {}(self, **kwargs) -> dict:\n        return self._request('{}', '{}', **kwargs)\n",
        method, http_method, path
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_plan() {
        let plan = DeploymentPlan {
            id: Uuid::nil(), name: "USDC Multichain".into(), template_id: Uuid::nil(),
            chains: vec![
                ChainDeployment { chain: Chain::Ethereum, chain_id: 1, contract_address: Some("0x1".into()), tx_hash: None, status: DeploymentStatus::Deployed, gas_used: Some(2000000) },
                ChainDeployment { chain: Chain::Solana, chain_id: 0, contract_address: None, tx_hash: None, status: DeploymentStatus::Planned, gas_used: None },
            ],
            status: DeploymentStatus::Deploying, created_at: Utc::now(),
        };
        assert_eq!(plan.chains.len(), 2);
    }

    #[test]
    fn lz_oft_config() {
        let oft = LayerZeroOft {
            token_name: "My Token".into(), token_symbol: "MTK".into(),
            home_chain: Chain::Ethereum, home_address: "0xhome".into(),
            peers: vec![OftPeer { chain: Chain::Solana, chain_id: 0, lz_eid: 30168, address: "sol_addr".into(), is_connected: true }],
            dvn_config: DvnConfig { required_dvns: vec!["LZ Labs".into()], optional_dvns: vec![], threshold: 1 },
        };
        assert_eq!(oft.peers.len(), 1);
        assert!(oft.peers[0].is_connected);
    }

    #[test]
    fn sdk_lang_all() {
        let langs = [SdkLang::Rust, SdkLang::Python, SdkLang::TypeScript, SdkLang::Go, SdkLang::Java, SdkLang::CSharp, SdkLang::Ruby];
        assert_eq!(langs.len(), 7);
    }

    #[test]
    fn ts_method_generation() {
        let ts = generate_ts_method("listVaults", "GET", "/v1/vaults");
        assert!(ts.contains("async listVaults"));
        assert!(ts.contains("GET"));
    }

    #[test]
    fn py_method_generation() {
        let py = generate_py_method("list_vaults", "GET", "/v1/vaults");
        assert!(py.contains("def list_vaults"));
        assert!(py.contains("GET"));
    }

    #[test]
    fn deployment_status() {
        assert_eq!(serde_json::to_string(&DeploymentStatus::Verified).unwrap(), r#""verified""#);
    }
}
