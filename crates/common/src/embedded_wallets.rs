//! Embedded Wallets — non-custodial wallet management (Fireblocks Embedded).
//!
//! Users control their own keys, custody provides infrastructure.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

/// An embedded (non-custodial) wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedWallet {
    pub id: Uuid,
    pub user_id: String,
    pub wallet_type: EmbeddedWalletType,
    pub status: WalletStatus,
    pub accounts: Vec<EmbeddedAccount>,
    pub created_at: DateTime<Utc>,
    pub last_active_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddedWalletType {
    /// User holds full key (custody provides API).
    NonCustodial,
    /// User holds one MPC share, custody holds one, recovery agent holds one.
    MpcSplit,
    /// Social recovery (email/phone + device key).
    SocialRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalletStatus {
    Active,
    Frozen,
    PendingRecovery,
    Deactivated,
}

/// An account within an embedded wallet (one per chain).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedAccount {
    pub chain: Chain,
    pub address: String,
    pub derivation_path: String,
    pub is_deployed: bool, // for smart contract wallets (ERC-4337)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEmbeddedWalletRequest {
    pub user_id: String,
    pub wallet_type: EmbeddedWalletType,
    pub chains: Vec<Chain>,
}

/// Recovery configuration for embedded wallets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    pub method: RecoveryMethod,
    pub recovery_contacts: Vec<String>,
    pub timeout_hours: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMethod {
    SeedPhrase,
    SocialRecovery,
    MpcReshare,
    CustodyAssisted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_type_serialization() {
        assert_eq!(serde_json::to_string(&EmbeddedWalletType::MpcSplit).unwrap(), r#""mpc_split""#);
        assert_eq!(serde_json::to_string(&EmbeddedWalletType::SocialRecovery).unwrap(), r#""social_recovery""#);
    }

    #[test]
    fn embedded_wallet_structure() {
        let wallet = EmbeddedWallet {
            id: Uuid::nil(), user_id: "user@example.com".into(),
            wallet_type: EmbeddedWalletType::MpcSplit,
            status: WalletStatus::Active,
            accounts: vec![EmbeddedAccount {
                chain: Chain::Ethereum, address: "0x123".into(),
                derivation_path: "m/44'/60'/0'/0/0".into(), is_deployed: false,
            }],
            created_at: Utc::now(), last_active_at: None,
        };
        let json = serde_json::to_string(&wallet).unwrap();
        assert!(json.contains("mpc_split"));
        assert!(json.contains("user@example.com"));
    }

    #[test]
    fn recovery_method_serialization() {
        assert_eq!(serde_json::to_string(&RecoveryMethod::SocialRecovery).unwrap(), r#""social_recovery""#);
        assert_eq!(serde_json::to_string(&RecoveryMethod::MpcReshare).unwrap(), r#""mpc_reshare""#);
    }
}
