//! OFAC/Sanctions address screening.
//!
//! Checks addresses against the OFAC SDN (Specially Designated Nationals) list
//! and other sanctions lists (EU, UN).
//!
//! The SDN list is refreshed daily from the US Treasury.
//! Reference: <https://www.treasury.gov/ofac/downloads/sdnlist.txt>

use std::collections::HashSet;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A sanctions list with known addresses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionsList {
    pub name: String,
    pub source_url: String,
    pub addresses: HashSet<String>,
    pub last_updated: DateTime<Utc>,
}

impl SanctionsList {
    /// Create an empty sanctions list.
    pub fn new(name: &str, source_url: &str) -> Self {
        Self {
            name: name.to_string(),
            source_url: source_url.to_string(),
            addresses: HashSet::new(),
            last_updated: Utc::now(),
        }
    }

    /// Add an address to the list (lowercase normalized).
    pub fn add_address(&mut self, address: &str) {
        self.addresses.insert(address.to_lowercase());
    }

    /// Check if an address is on this sanctions list.
    pub fn contains(&self, address: &str) -> bool {
        self.addresses.contains(&address.to_lowercase())
    }

    /// Number of addresses in the list.
    pub fn len(&self) -> usize {
        self.addresses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }
}

/// Multi-list sanctions checker.
pub struct SanctionsChecker {
    lists: Vec<SanctionsList>,
}

/// Result of a sanctions check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionsCheckResult {
    pub address: String,
    pub is_sanctioned: bool,
    pub matched_lists: Vec<String>,
    pub checked_at: DateTime<Utc>,
}

impl SanctionsChecker {
    pub fn new() -> Self {
        Self { lists: Vec::new() }
    }

    /// Add a sanctions list.
    pub fn add_list(&mut self, list: SanctionsList) {
        self.lists.push(list);
    }

    /// Check an address against all loaded sanctions lists.
    pub fn check(&self, address: &str) -> SanctionsCheckResult {
        let matched: Vec<String> = self
            .lists
            .iter()
            .filter(|l| l.contains(address))
            .map(|l| l.name.clone())
            .collect();

        SanctionsCheckResult {
            address: address.to_lowercase(),
            is_sanctioned: !matched.is_empty(),
            matched_lists: matched,
            checked_at: Utc::now(),
        }
    }

    /// Total addresses across all lists.
    pub fn total_addresses(&self) -> usize {
        self.lists.iter().map(|l| l.len()).sum()
    }

    /// Refresh a list by downloading from its source URL.
    /// Parses the SDN list for cryptocurrency addresses (lines containing "Digital Currency Address").
    pub async fn refresh_list(&mut self, list_name: &str) -> Result<()> {
        let list = match self.lists.iter_mut().find(|l| l.name == list_name) {
            Some(l) => l,
            None => anyhow::bail!("Unknown list: {list_name}"),
        };

        tracing::info!(list_name, url = %list.source_url, "Downloading sanctions list");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let resp = client.get(&list.source_url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("OFAC download failed: HTTP {}", resp.status());
        }

        let body = resp.text().await?;
        let mut count = 0;

        // Parse SDN text format: lines with "Digital Currency Address" contain crypto addresses
        for line in body.lines() {
            // SDN format: addresses appear after "Digital Currency Address -" tags
            if line.contains("Digital Currency Address") {
                // Extract addresses (hex for ETH, base58 for BTC, etc.)
                for word in line.split_whitespace() {
                    let trimmed = word.trim_end_matches([';', '.', ',']);
                    if is_crypto_address(trimmed) {
                        list.add_address(trimmed);
                        count += 1;
                    }
                }
            }
        }

        list.last_updated = Utc::now();
        tracing::info!(list_name, addresses = count, "Sanctions list updated");
        Ok(())
    }

    /// Refresh all lists.
    pub async fn refresh_all(&mut self) -> Result<()> {
        let names: Vec<String> = self.lists.iter().map(|l| l.name.clone()).collect();
        for name in names {
            if let Err(e) = self.refresh_list(&name).await {
                tracing::warn!(list = %name, error = %e, "Failed to refresh sanctions list");
            }
        }
        Ok(())
    }
}

impl Default for SanctionsChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Heuristic check if a string looks like a crypto address.
fn is_crypto_address(s: &str) -> bool {
    // Ethereum-style (0x + 40 hex)
    if s.starts_with("0x") && s.len() == 42 && s[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    // Bitcoin (starts with 1, 3, or bc1, length 26-62)
    if (s.starts_with('1') || s.starts_with('3') || s.starts_with("bc1"))
        && s.len() >= 26 && s.len() <= 62
        && s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return true;
    }
    // Tron (starts with T, length 34)
    if s.starts_with('T') && s.len() == 34 && s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return true;
    }
    false
}

/// OFAC SDN list URL.
pub const OFAC_SDN_URL: &str = "https://www.treasury.gov/ofac/downloads/sdnlist.txt";

/// OFAC specially designated nationals crypto addresses list.
pub const OFAC_SDN_ADVANCED_URL: &str = "https://www.treasury.gov/ofac/downloads/sanctions/1.0/sdn_advanced.xml";

/// Create a pre-populated OFAC checker with known sanctioned crypto addresses.
pub fn ofac_checker_with_known_addresses() -> SanctionsChecker {
    let mut checker = SanctionsChecker::new();

    let mut ofac = SanctionsList::new("OFAC SDN", OFAC_SDN_URL);

    // Well-known OFAC-sanctioned crypto addresses (examples from public OFAC actions)
    // Tornado Cash contracts (sanctioned August 2022)
    ofac.add_address("0xd90e2f925da726b50c4ed8d0fb90ad053324f31b");
    ofac.add_address("0xd4b88df4d29f5cedd6857912842cff3b20c8cfa3");
    ofac.add_address("0x910cbd523d972eb0a6f4cae4618ad62622b39dbf");
    // Add more from OFAC updates...

    let eu = SanctionsList::new("EU Sanctions", "https://webgate.ec.europa.eu/europeaid/fsd/fsf");
    // EU sanctions list addresses would be loaded from the EU registry

    checker.add_list(ofac);
    checker.add_list(eu);
    checker
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanctions_list_basic() {
        let mut list = SanctionsList::new("test", "http://test.com");
        list.add_address("0xBAD123");
        list.add_address("0xBAD456");

        assert!(list.contains("0xbad123")); // case insensitive
        assert!(list.contains("0xBAD456"));
        assert!(!list.contains("0xGOOD789"));
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn sanctions_checker_multi_list() {
        let mut checker = SanctionsChecker::new();

        let mut ofac = SanctionsList::new("OFAC", "http://ofac.gov");
        ofac.add_address("0xSanctioned1");
        checker.add_list(ofac);

        let mut eu = SanctionsList::new("EU", "http://eu.europa.eu");
        eu.add_address("0xSanctioned1"); // on both lists
        eu.add_address("0xSanctioned2"); // EU only
        checker.add_list(eu);

        let result = checker.check("0xsanctioned1");
        assert!(result.is_sanctioned);
        assert_eq!(result.matched_lists.len(), 2); // on both lists

        let result = checker.check("0xsanctioned2");
        assert!(result.is_sanctioned);
        assert_eq!(result.matched_lists.len(), 1); // EU only

        let result = checker.check("0xclean");
        assert!(!result.is_sanctioned);
        assert!(result.matched_lists.is_empty());
    }

    #[test]
    fn known_tornado_cash_addresses_sanctioned() {
        let checker = ofac_checker_with_known_addresses();
        let result = checker.check("0xd90e2f925da726b50c4ed8d0fb90ad053324f31b");
        assert!(result.is_sanctioned);
        assert!(result.matched_lists.contains(&"OFAC SDN".to_string()));
    }

    #[test]
    fn clean_address_passes() {
        let checker = ofac_checker_with_known_addresses();
        let result = checker.check("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18");
        assert!(!result.is_sanctioned);
    }
}
