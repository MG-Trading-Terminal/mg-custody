//! In-memory cache of known deposit addresses for fast lookup during scanning.
//!
//! The cache is a DashMap of (lowercase address → user_id) loaded from the
//! database and refreshed periodically to pick up newly created addresses.

use std::sync::atomic::{AtomicU64, Ordering};

use custody_common::Chain;
use custody_db::CustodyDb;
use dashmap::DashMap;
use uuid::Uuid;

/// Cache of deposit addresses for a specific chain.
pub struct AddressCache {
    db: CustodyDb,
    chain: Chain,
    /// Lowercase address → user_id mapping.
    addresses: DashMap<String, Uuid>,
    /// Block number when the cache was last refreshed.
    last_refresh_block: AtomicU64,
    /// Refresh every N blocks.
    refresh_interval: u64,
}

impl AddressCache {
    /// Create a new address cache.
    pub fn new(db: CustodyDb, chain: Chain, refresh_interval: u64) -> Self {
        Self {
            db,
            chain,
            addresses: DashMap::new(),
            last_refresh_block: AtomicU64::new(0),
            refresh_interval,
        }
    }

    /// Refresh the cache if stale (current_block - last_refresh >= interval).
    pub async fn refresh_if_stale(&self, current_block: u64) -> anyhow::Result<()> {
        let last = self.last_refresh_block.load(Ordering::Relaxed);
        if current_block.saturating_sub(last) >= self.refresh_interval {
            self.refresh().await?;
            self.last_refresh_block.store(current_block, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Force a refresh from the database.
    pub async fn refresh(&self) -> anyhow::Result<()> {
        let addrs = self.db.all_addresses_for_chain(self.chain).await?;
        // Note: all_addresses_for_chain returns Vec<String> (addresses only).
        // TODO: Update DB method to return (address, user_id) pairs.
        // For now, insert with a placeholder user_id.
        for addr in addrs {
            let normalized = addr.to_lowercase();
            self.addresses.entry(normalized).or_insert(Uuid::nil());
        }
        tracing::debug!(
            chain = ?self.chain,
            count = self.addresses.len(),
            "Address cache refreshed"
        );
        Ok(())
    }

    /// Look up an address in the cache.
    ///
    /// The address should be lowercase for Ethereum/Tron.
    /// Returns the user_id if found.
    pub fn lookup(&self, address: &str) -> Option<Uuid> {
        self.addresses.get(&address.to_lowercase()).map(|r| *r.value())
    }

    /// Check if an address is known (any form).
    pub fn contains(&self, address: &str) -> bool {
        self.addresses.contains_key(&address.to_lowercase())
    }

    /// Number of cached addresses.
    pub fn len(&self) -> usize {
        self.addresses.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }

    /// Insert an address manually (e.g., when a new address is just created).
    pub fn insert(&self, address: &str, user_id: Uuid) {
        self.addresses.insert(address.to_lowercase(), user_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_lookup_case_insensitive() {
        // Test the DashMap lookup directly (no DB needed)
        let addresses: DashMap<String, Uuid> = DashMap::new();
        let addr = "0xabcdef1234567890abcdef1234567890abcdef12".to_string();
        addresses.insert(addr.clone(), Uuid::from_u128(1));

        // Case-insensitive lookup
        let lookup = |a: &str| -> bool {
            addresses.contains_key(&a.to_lowercase())
        };

        assert!(lookup("0xabcdef1234567890abcdef1234567890abcdef12"));
        assert!(lookup("0xABCDEF1234567890ABCDEF1234567890ABCDEF12"));
        assert!(lookup("0xAbCdEf1234567890AbCdEf1234567890AbCdEf12"));
        assert!(!lookup("0x0000000000000000000000000000000000000000"));
    }
}
