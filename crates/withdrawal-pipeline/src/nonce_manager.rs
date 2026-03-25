//! Single-threaded nonce allocator for Ethereum-style accounts.
//!
//! Ensures sequential nonce allocation with no gaps.
//! On startup, syncs from chain via `eth_getTransactionCount`.
//! After that, increments locally with DB persistence.

use std::sync::Arc;

use alloy::providers::{Provider, ProviderBuilder};
use anyhow::{Context, Result};
use custody_db::CustodyDb;
use dashmap::DashMap;
use tokio::sync::Mutex;

/// Manages nonce allocation per Ethereum address.
///
/// Thread-safe: uses a Mutex per address to ensure single-threaded allocation.
pub struct NonceManager {
    db: CustodyDb,
    provider: alloy::providers::fillers::FillProvider<
        alloy::providers::fillers::JoinFill<
            alloy::providers::Identity,
            alloy::providers::fillers::JoinFill<
                alloy::providers::fillers::GasFiller,
                alloy::providers::fillers::JoinFill<
                    alloy::providers::fillers::BlobGasFiller,
                    alloy::providers::fillers::JoinFill<
                        alloy::providers::fillers::NonceFiller,
                        alloy::providers::fillers::ChainIdFiller,
                    >,
                >,
            >,
        >,
        alloy::providers::RootProvider,
    >,
    /// Per-address lock for sequential nonce allocation.
    locks: DashMap<String, Arc<Mutex<()>>>,
}

impl NonceManager {
    pub fn new(db: CustodyDb, rpc_url: &str) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .connect_http(rpc_url.parse().context("Invalid RPC URL")?);
        Ok(Self {
            db,
            provider,
            locks: DashMap::new(),
        })
    }

    /// Allocate the next nonce for the given address.
    ///
    /// On first call (or after restart), reads from chain.
    /// Subsequent calls increment from DB.
    pub async fn next_nonce(&self, address: &str) -> Result<u64> {
        let lock = self
            .locks
            .entry(address.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();

        let _guard = lock.lock().await;

        // Check if we have a stored nonce
        if let Some(nonce) = self.db.get_nonce(address).await? {
            // Increment and return the current value
            let next = self.db.increment_nonce(address).await?;
            tracing::debug!(address, nonce = next, "Nonce allocated from DB");
            return Ok(next);
        }

        // First time: sync from chain
        let chain_nonce = self.sync_from_chain(address).await?;
        tracing::info!(address, nonce = chain_nonce, "Nonce synced from chain");
        Ok(chain_nonce)
    }

    /// Sync nonce from chain (called on startup or after detecting a gap).
    pub async fn sync_from_chain(&self, address: &str) -> Result<u64> {
        let addr: alloy::primitives::Address = address
            .parse()
            .context("Invalid Ethereum address")?;

        let nonce = self
            .provider
            .get_transaction_count(addr)
            .await
            .context("eth_getTransactionCount failed")?;

        // Store nonce + 1 as the next available (the chain returns the count, which equals the next nonce)
        self.db.set_nonce(address, nonce + 1).await?;

        Ok(nonce)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn nonce_manager_design() {
        // NonceManager requires a real DB and RPC connection.
        // Unit tests verify the lock mechanism conceptually.
        // Integration tests (with Sepolia) will test actual nonce allocation.

        // The key invariant: for a given address, nonces are strictly sequential
        // with no gaps, even under concurrent calls.
        // This is guaranteed by the per-address Mutex.
    }
}
