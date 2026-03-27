//! Address derivation cache — avoid re-deriving known addresses.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::Chain;

/// Cached derived address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAddress {
    pub chain: Chain,
    pub index: u32,
    pub address: String,
    pub derivation_path: String,
}

/// In-memory address cache (per-chain).
pub struct AddressDerivationCache {
    cache: HashMap<(Chain, u32), CachedAddress>,
    max_size: usize,
}

impl AddressDerivationCache {
    pub fn new(max_size: usize) -> Self {
        Self { cache: HashMap::new(), max_size }
    }

    pub fn get(&self, chain: Chain, index: u32) -> Option<&CachedAddress> {
        self.cache.get(&(chain, index))
    }

    pub fn insert(&mut self, addr: CachedAddress) {
        if self.cache.len() >= self.max_size {
            // Evict oldest (simple: just clear half)
            let keys: Vec<_> = self.cache.keys().take(self.max_size / 2).cloned().collect();
            for k in keys { self.cache.remove(&k); }
        }
        self.cache.insert((addr.chain, addr.index), addr);
    }

    pub fn contains(&self, chain: Chain, index: u32) -> bool {
        self.cache.contains_key(&(chain, index))
    }

    pub fn len(&self) -> usize { self.cache.len() }
    pub fn is_empty(&self) -> bool { self.cache.is_empty() }

    /// Get all cached addresses for a chain.
    pub fn addresses_for_chain(&self, chain: Chain) -> Vec<&CachedAddress> {
        self.cache.values().filter(|a| a.chain == chain).collect()
    }
}

/// On-chain data indexing status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingStatus {
    pub chain: Chain,
    pub current_block: u64,
    pub target_block: u64,
    pub blocks_behind: u64,
    pub indexing_rate_bps: f32, // blocks per second
    pub estimated_catch_up_secs: u32,
    pub is_synced: bool,
}

impl IndexingStatus {
    pub fn new(chain: Chain, current: u64, target: u64, rate: f32) -> Self {
        let behind = target.saturating_sub(current);
        let catch_up = if rate > 0.0 { (behind as f32 / rate) as u32 } else { u32::MAX };
        Self {
            chain, current_block: current, target_block: target,
            blocks_behind: behind, indexing_rate_bps: rate,
            estimated_catch_up_secs: catch_up,
            is_synced: behind <= 3,
        }
    }
}

/// Deposit forwarding engine — auto-forward deposits to hot wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardingEngine {
    pub chain: Chain,
    pub enabled: bool,
    pub min_amount: rust_decimal::Decimal,
    pub destination: String,
    pub auto_consolidate: bool,
    pub pending_forwards: u32,
    pub total_forwarded: rust_decimal::Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_insert_and_get() {
        let mut cache = AddressDerivationCache::new(100);
        cache.insert(CachedAddress {
            chain: Chain::Ethereum, index: 0,
            address: "0x123".into(), derivation_path: "m/44'/60'/0'/0/0".into(),
        });
        assert!(cache.contains(Chain::Ethereum, 0));
        assert!(!cache.contains(Chain::Ethereum, 1));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn cache_eviction() {
        let mut cache = AddressDerivationCache::new(2);
        for i in 0..5 {
            cache.insert(CachedAddress {
                chain: Chain::Ethereum, index: i,
                address: format!("0x{i}"), derivation_path: format!("m/{i}"),
            });
        }
        assert!(cache.len() <= 3); // eviction happened
    }

    #[test]
    fn cache_per_chain() {
        let mut cache = AddressDerivationCache::new(100);
        cache.insert(CachedAddress { chain: Chain::Ethereum, index: 0, address: "0x1".into(), derivation_path: "m/0".into() });
        cache.insert(CachedAddress { chain: Chain::Bitcoin, index: 0, address: "bc1q".into(), derivation_path: "m/0".into() });
        assert_eq!(cache.addresses_for_chain(Chain::Ethereum).len(), 1);
        assert_eq!(cache.addresses_for_chain(Chain::Bitcoin).len(), 1);
    }

    #[test]
    fn indexing_synced() {
        let status = IndexingStatus::new(Chain::Ethereum, 19500000, 19500002, 10.0);
        assert!(status.is_synced); // only 2 behind
    }

    #[test]
    fn indexing_behind() {
        let status = IndexingStatus::new(Chain::Ethereum, 19490000, 19500000, 100.0);
        assert!(!status.is_synced);
        assert_eq!(status.blocks_behind, 10000);
        assert_eq!(status.estimated_catch_up_secs, 100);
    }

    #[test]
    fn indexing_zero_rate() {
        let status = IndexingStatus::new(Chain::Bitcoin, 0, 840000, 0.0);
        assert_eq!(status.estimated_catch_up_secs, u32::MAX);
    }

    #[test]
    fn forwarding_engine() {
        let engine = ForwardingEngine {
            chain: Chain::Ethereum, enabled: true,
            min_amount: rust_decimal::Decimal::new(1, 1), // 0.1
            destination: "0xhot_wallet".into(),
            auto_consolidate: true, pending_forwards: 3,
            total_forwarded: rust_decimal::Decimal::new(150, 0),
        };
        assert!(engine.enabled);
        assert_eq!(engine.pending_forwards, 3);
    }
}
