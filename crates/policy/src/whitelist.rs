//! Address whitelist management.
//!
//! All withdrawal destinations must be whitelisted before sending.
//! Adding addresses requires admin approval (M-of-N quorum).
//! Supports one-time addresses that auto-expire after single use.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use custody_common::Chain;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A whitelisted address entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhitelistEntry {
    pub id: Uuid,
    pub address: String,
    pub chain: Chain,
    pub label: String,
    pub added_by: String,
    pub approved_by: Vec<String>,
    /// Optional expiry. None = permanent.
    pub expires_at: Option<DateTime<Utc>>,
    /// One-time use: auto-removed after first withdrawal.
    pub one_time: bool,
    /// Whether this entry has been used (for one-time entries).
    pub used: bool,
    pub created_at: DateTime<Utc>,
}

/// In-memory whitelist (production uses DB).
pub struct Whitelist {
    entries: HashMap<String, WhitelistEntry>,
}

impl Whitelist {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Add an address to the whitelist.
    pub fn add(&mut self, entry: WhitelistEntry) {
        let key = format!("{}:{}", entry.chain.as_str(), entry.address.to_lowercase());
        self.entries.insert(key, entry);
    }

    /// Check if an address is whitelisted and valid.
    pub fn is_whitelisted(&self, chain: Chain, address: &str) -> bool {
        let key = format!("{}:{}", chain.as_str(), address.to_lowercase());
        match self.entries.get(&key) {
            Some(entry) => {
                if entry.used && entry.one_time {
                    return false;
                }
                if let Some(expires) = entry.expires_at {
                    if Utc::now() > expires {
                        return false;
                    }
                }
                true
            }
            None => false,
        }
    }

    /// Mark a one-time address as used.
    pub fn mark_used(&mut self, chain: Chain, address: &str) -> bool {
        let key = format!("{}:{}", chain.as_str(), address.to_lowercase());
        if let Some(entry) = self.entries.get_mut(&key) {
            if entry.one_time {
                entry.used = true;
                return true;
            }
        }
        false
    }

    /// Remove an address from the whitelist.
    pub fn remove(&mut self, chain: Chain, address: &str) -> bool {
        let key = format!("{}:{}", chain.as_str(), address.to_lowercase());
        self.entries.remove(&key).is_some()
    }

    /// Get all entries for a chain.
    pub fn entries_for_chain(&self, chain: Chain) -> Vec<&WhitelistEntry> {
        self.entries
            .values()
            .filter(|e| e.chain == chain)
            .collect()
    }

    /// Total number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Whitelist {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(chain: Chain, address: &str, one_time: bool) -> WhitelistEntry {
        WhitelistEntry {
            id: Uuid::new_v4(),
            address: address.to_string(),
            chain,
            label: "test".to_string(),
            added_by: "admin@test.com".to_string(),
            approved_by: vec!["admin1@test.com".to_string()],
            expires_at: None,
            one_time,
            used: false,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn add_and_check_whitelist() {
        let mut wl = Whitelist::new();
        wl.add(make_entry(Chain::Ethereum, "0xABC123", false));

        assert!(wl.is_whitelisted(Chain::Ethereum, "0xabc123")); // case insensitive
        assert!(!wl.is_whitelisted(Chain::Ethereum, "0xDEF456")); // not in list
        assert!(!wl.is_whitelisted(Chain::Bitcoin, "0xabc123")); // wrong chain
    }

    #[test]
    fn one_time_address() {
        let mut wl = Whitelist::new();
        wl.add(make_entry(Chain::Ethereum, "0xONETIME", true));

        assert!(wl.is_whitelisted(Chain::Ethereum, "0xonetime"));
        wl.mark_used(Chain::Ethereum, "0xonetime");
        assert!(!wl.is_whitelisted(Chain::Ethereum, "0xonetime")); // expired after use
    }

    #[test]
    fn expired_address_rejected() {
        let mut wl = Whitelist::new();
        let mut entry = make_entry(Chain::Ethereum, "0xEXPIRED", false);
        entry.expires_at = Some(Utc::now() - chrono::Duration::hours(1)); // expired 1h ago
        wl.add(entry);

        assert!(!wl.is_whitelisted(Chain::Ethereum, "0xexpired"));
    }

    #[test]
    fn remove_address() {
        let mut wl = Whitelist::new();
        wl.add(make_entry(Chain::Ethereum, "0xREMOVE", false));
        assert!(wl.is_whitelisted(Chain::Ethereum, "0xremove"));

        assert!(wl.remove(Chain::Ethereum, "0xremove"));
        assert!(!wl.is_whitelisted(Chain::Ethereum, "0xremove"));
    }

    #[test]
    fn entries_for_chain() {
        let mut wl = Whitelist::new();
        wl.add(make_entry(Chain::Ethereum, "0xETH1", false));
        wl.add(make_entry(Chain::Ethereum, "0xETH2", false));
        wl.add(make_entry(Chain::Bitcoin, "bc1qbtc", false));

        assert_eq!(wl.entries_for_chain(Chain::Ethereum).len(), 2);
        assert_eq!(wl.entries_for_chain(Chain::Bitcoin).len(), 1);
        assert_eq!(wl.entries_for_chain(Chain::Solana).len(), 0);
    }
}
