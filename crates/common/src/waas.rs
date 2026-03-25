//! Wallet-as-a-Service platform types (Phase 15).
//!
//! Multi-tenant workspace management, SDK support, white-label API.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A tenant workspace (isolated environment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub environment: WorkspaceEnvironment,
    pub owner_id: String,
    pub created_at: DateTime<Utc>,
    pub api_key_count: u32,
    pub vault_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceEnvironment {
    Sandbox,  // no real assets, for development
    Testnet,  // testnet blockchains
    Mainnet,  // production, real assets
}

/// API key for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub role: String, // maps to RBAC role
    pub ip_whitelist: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// SDK language support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SdkLanguage {
    Rust,
    Python,
    TypeScript,
    Go,
    Java,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_environment_serialization() {
        assert_eq!(serde_json::to_string(&WorkspaceEnvironment::Mainnet).unwrap(), "\"mainnet\"");
        assert_eq!(serde_json::to_string(&WorkspaceEnvironment::Sandbox).unwrap(), "\"sandbox\"");
    }

    #[test]
    fn sdk_languages() {
        let langs = vec![SdkLanguage::Rust, SdkLanguage::Python, SdkLanguage::TypeScript, SdkLanguage::Go, SdkLanguage::Java];
        assert_eq!(langs.len(), 5);
    }
}
