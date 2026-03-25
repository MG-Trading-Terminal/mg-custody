//! API key management with scoped permissions.
//!
//! Each API key is associated with a role and optionally scoped to
//! specific chains, assets, or operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rbac::Role;

/// An API key for authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    /// The secret key (hashed in DB, shown once on creation).
    pub key_hash: String,
    pub role: Role,
    /// IP addresses allowed to use this key (empty = any IP).
    pub ip_allowlist: Vec<String>,
    /// Specific chains this key can access (empty = all).
    pub allowed_chains: Vec<String>,
    /// Specific operations this key can perform (empty = all for role).
    pub allowed_operations: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub enabled: bool,
}

impl ApiKey {
    /// Check if a request from the given IP is allowed.
    pub fn ip_allowed(&self, ip: &str) -> bool {
        if self.ip_allowlist.is_empty() {
            return true; // no restrictions
        }
        self.ip_allowlist.iter().any(|allowed| allowed == ip)
    }

    /// Check if this key is expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Utc::now() > expires
        } else {
            false
        }
    }

    /// Check if this key is usable (enabled + not expired + IP allowed).
    pub fn is_valid(&self, ip: &str) -> bool {
        self.enabled && !self.is_expired() && self.ip_allowed(ip)
    }
}

/// FIDO2/WebAuthn credential for hardware key authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAuthnCredential {
    pub id: Uuid,
    pub user_id: String,
    pub credential_id: Vec<u8>,
    pub public_key: Vec<u8>,
    pub sign_count: u32,
    pub name: String, // e.g., "YubiKey 5C"
    pub registered_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Time-delayed release configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayConfig {
    /// Tier 0 (< $1K + whitelisted): 1-5 min delay.
    pub tier_0_minutes: u32,
    /// Tier 1 ($1K - $10K): 10 min delay.
    pub tier_1_minutes: u32,
    /// Tier 2 ($10K - $100K): 30 min delay.
    pub tier_2_minutes: u32,
    /// Tier 3 ($100K - $500K): 2 hour delay.
    pub tier_3_minutes: u32,
    /// Tier 4 (> $500K): 24 hour delay.
    pub tier_4_minutes: u32,
}

impl Default for DelayConfig {
    fn default() -> Self {
        Self {
            tier_0_minutes: 1,
            tier_1_minutes: 10,
            tier_2_minutes: 30,
            tier_3_minutes: 120,
            tier_4_minutes: 1440,
        }
    }
}

impl DelayConfig {
    /// Get the delay in minutes for a given USD amount.
    pub fn delay_minutes(&self, amount_usd: rust_decimal::Decimal) -> u32 {
        use rust_decimal_macros::dec;
        if amount_usd >= dec!(500000) { self.tier_4_minutes }
        else if amount_usd >= dec!(100000) { self.tier_3_minutes }
        else if amount_usd >= dec!(10000) { self.tier_2_minutes }
        else if amount_usd >= dec!(1000) { self.tier_1_minutes }
        else { self.tier_0_minutes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn test_key() -> ApiKey {
        ApiKey {
            id: Uuid::new_v4(),
            name: "test-key".into(),
            key_hash: "hashed".into(),
            role: Role::Editor,
            ip_allowlist: vec!["10.0.0.1".into(), "10.0.0.2".into()],
            allowed_chains: vec![],
            allowed_operations: vec![],
            created_at: Utc::now(),
            last_used_at: None,
            expires_at: None,
            enabled: true,
        }
    }

    #[test]
    fn ip_allowlist() {
        let key = test_key();
        assert!(key.ip_allowed("10.0.0.1"));
        assert!(key.ip_allowed("10.0.0.2"));
        assert!(!key.ip_allowed("192.168.1.1"));
    }

    #[test]
    fn empty_ip_allowlist_allows_all() {
        let mut key = test_key();
        key.ip_allowlist = vec![];
        assert!(key.ip_allowed("any.ip.here.com"));
    }

    #[test]
    fn expired_key() {
        let mut key = test_key();
        key.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        assert!(key.is_expired());
        assert!(!key.is_valid("10.0.0.1"));
    }

    #[test]
    fn disabled_key() {
        let mut key = test_key();
        key.enabled = false;
        assert!(!key.is_valid("10.0.0.1"));
    }

    #[test]
    fn valid_key() {
        let key = test_key();
        assert!(key.is_valid("10.0.0.1"));
    }

    #[test]
    fn delay_config_tiers() {
        let config = DelayConfig::default();
        assert_eq!(config.delay_minutes(dec!(500)), 1);      // tier 0
        assert_eq!(config.delay_minutes(dec!(5000)), 10);     // tier 1
        assert_eq!(config.delay_minutes(dec!(50000)), 30);    // tier 2
        assert_eq!(config.delay_minutes(dec!(200000)), 120);  // tier 3
        assert_eq!(config.delay_minutes(dec!(1000000)), 1440); // tier 4
    }

    #[test]
    fn webauthn_credential_structure() {
        let cred = WebAuthnCredential {
            id: Uuid::new_v4(),
            user_id: "admin@company.com".into(),
            credential_id: vec![1, 2, 3],
            public_key: vec![4, 5, 6],
            sign_count: 0,
            name: "YubiKey 5C".into(),
            registered_at: Utc::now(),
            last_used_at: None,
        };
        assert_eq!(cred.sign_count, 0);
    }
}
