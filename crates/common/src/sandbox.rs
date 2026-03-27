//! Sandbox/Testnet environment — separate configs, test data, rate limits.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::Chain;

/// Environment mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Sandbox,   // fake data, no real chains
    Testnet,   // real testnet chains (Sepolia, testnet3, Shasta, devnet)
    Mainnet,   // production
}

/// Testnet chain mapping.
pub fn testnet_chain(chain: Chain) -> TestnetInfo {
    match chain {
        Chain::Ethereum => TestnetInfo { chain, testnet_name: "Sepolia".into(), chain_id: 11155111, faucet_url: Some("https://sepoliafaucet.com".into()), explorer: "https://sepolia.etherscan.io".into() },
        Chain::Bitcoin => TestnetInfo { chain, testnet_name: "Testnet3".into(), chain_id: 0, faucet_url: Some("https://bitcoinfaucet.uo1.net".into()), explorer: "https://mempool.space/testnet".into() },
        Chain::Tron => TestnetInfo { chain, testnet_name: "Shasta".into(), chain_id: 0, faucet_url: Some("https://www.trongrid.io/shasta".into()), explorer: "https://shasta.tronscan.org".into() },
        Chain::Solana => TestnetInfo { chain, testnet_name: "Devnet".into(), chain_id: 0, faucet_url: Some("https://faucet.solana.com".into()), explorer: "https://explorer.solana.com/?cluster=devnet".into() },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestnetInfo {
    pub chain: Chain,
    pub testnet_name: String,
    pub chain_id: u64,
    pub faucet_url: Option<String>,
    pub explorer: String,
}

/// Sandbox rate limits (more permissive than production).
pub fn sandbox_rate_limits() -> RateLimits {
    RateLimits { requests_per_minute: 1000, requests_per_hour: 50000, max_batch_size: 500 }
}

pub fn production_rate_limits() -> RateLimits {
    RateLimits { requests_per_minute: 100, requests_per_hour: 5000, max_batch_size: 100 }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub max_batch_size: u32,
}

/// IP geofencing — block requests from restricted countries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoFenceConfig {
    pub enabled: bool,
    pub mode: GeoFenceMode,
    pub countries: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoFenceMode {
    AllowList, // only allow listed countries
    BlockList, // block listed countries
}

/// OFAC-restricted countries (default block list).
pub fn ofac_blocked_countries() -> Vec<&'static str> {
    vec!["KP", "IR", "CU", "SY", "RU", "BY", "MM", "VE", "ZW", "SD", "SS"]
}

/// Session management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub ip_address: String,
    pub user_agent: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub is_active: bool,
}

/// Deposit address rotation — generate new addresses periodically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressRotationConfig {
    pub chain: Chain,
    pub rotate_after_deposits: u32,
    pub rotate_after_hours: u32,
    pub max_addresses_per_user: u32,
    pub enabled: bool,
}

pub fn default_rotation_config(chain: Chain) -> AddressRotationConfig {
    match chain {
        Chain::Bitcoin => AddressRotationConfig { chain, rotate_after_deposits: 1, rotate_after_hours: 0, max_addresses_per_user: 100, enabled: true },
        _ => AddressRotationConfig { chain, rotate_after_deposits: 0, rotate_after_hours: 720, max_addresses_per_user: 10, enabled: false },
    }
}

/// 2FA enforcement per operation type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwoFactorPolicy {
    pub operation: String,
    pub required: bool,
    pub allowed_methods: Vec<TwoFactorMethod>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TwoFactorMethod { Totp, Fido2, Email, Sms, Push }

pub fn default_2fa_policies() -> Vec<TwoFactorPolicy> {
    vec![
        TwoFactorPolicy { operation: "withdrawal".into(), required: true, allowed_methods: vec![TwoFactorMethod::Fido2, TwoFactorMethod::Totp] },
        TwoFactorPolicy { operation: "policy_change".into(), required: true, allowed_methods: vec![TwoFactorMethod::Fido2] },
        TwoFactorPolicy { operation: "api_key_create".into(), required: true, allowed_methods: vec![TwoFactorMethod::Fido2, TwoFactorMethod::Totp] },
        TwoFactorPolicy { operation: "address_whitelist".into(), required: true, allowed_methods: vec![TwoFactorMethod::Fido2, TwoFactorMethod::Totp] },
        TwoFactorPolicy { operation: "vault_create".into(), required: false, allowed_methods: vec![TwoFactorMethod::Totp] },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testnet_chains() {
        let eth = testnet_chain(Chain::Ethereum);
        assert_eq!(eth.testnet_name, "Sepolia");
        assert_eq!(eth.chain_id, 11155111);
        let sol = testnet_chain(Chain::Solana);
        assert_eq!(sol.testnet_name, "Devnet");
    }

    #[test]
    fn sandbox_more_permissive() {
        let sandbox = sandbox_rate_limits();
        let prod = production_rate_limits();
        assert!(sandbox.requests_per_minute > prod.requests_per_minute);
    }

    #[test]
    fn ofac_countries() {
        let blocked = ofac_blocked_countries();
        assert!(blocked.contains(&"KP")); // North Korea
        assert!(blocked.contains(&"IR")); // Iran
        assert!(blocked.len() >= 10);
    }

    #[test]
    fn geofence_modes() {
        assert_eq!(serde_json::to_string(&GeoFenceMode::BlockList).unwrap(), r#""block_list""#);
    }

    #[test]
    fn bitcoin_address_rotation() {
        let config = default_rotation_config(Chain::Bitcoin);
        assert!(config.enabled);
        assert_eq!(config.rotate_after_deposits, 1); // new address per deposit (privacy)
    }

    #[test]
    fn eth_no_rotation_by_default() {
        let config = default_rotation_config(Chain::Ethereum);
        assert!(!config.enabled); // ETH addresses reusable
    }

    #[test]
    fn default_2fa_policies_count() {
        let policies = default_2fa_policies();
        assert!(policies.len() >= 5);
        let withdrawal = policies.iter().find(|p| p.operation == "withdrawal").unwrap();
        assert!(withdrawal.required);
        assert!(withdrawal.allowed_methods.contains(&TwoFactorMethod::Fido2));
    }

    #[test]
    fn two_factor_method_serialization() {
        assert_eq!(serde_json::to_string(&TwoFactorMethod::Fido2).unwrap(), r#""fido2""#);
        assert_eq!(serde_json::to_string(&TwoFactorMethod::Totp).unwrap(), r#""totp""#);
    }

    #[test]
    fn environment_serialization() {
        assert_eq!(serde_json::to_string(&Environment::Sandbox).unwrap(), r#""sandbox""#);
        assert_eq!(serde_json::to_string(&Environment::Mainnet).unwrap(), r#""mainnet""#);
    }
}
