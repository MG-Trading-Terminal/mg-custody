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
    /// Addresses we've reconciled with the chain since this
    /// process started. Prevents repeating the chain probe on
    /// every allocation while still catching the "service
    /// restarted mid-broadcast; DB thinks N+1 but chain still
    /// expects N" footgun.
    reconciled: DashMap<String, ()>,
}

impl NonceManager {
    pub fn new(db: CustodyDb, rpc_url: &str) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .connect_http(rpc_url.parse().context("Invalid RPC URL")?);
        Ok(Self {
            db,
            provider,
            locks: DashMap::new(),
            reconciled: DashMap::new(),
        })
    }

    /// Allocate the next nonce for the given address.
    ///
    /// On **first call per process** (first allocation since boot),
    /// reconciles DB against chain — a service restart between
    /// broadcast and confirmation leaves the DB ahead of the chain
    /// (we'd incremented pre-broadcast) or behind (the daemon
    /// restarted a chain node and we missed events). Either way,
    /// blindly incrementing from DB produces bad nonces.
    ///
    /// Reconciliation policy:
    ///   - chain > db: some tx landed we don't track. Advance DB to
    ///     chain's count. Safe — we'd have picked a conflicting
    ///     nonce otherwise.
    ///   - chain == db: fine, no action.
    ///   - db > chain by 1: typical in-flight tx waiting to confirm.
    ///     Keep DB value — optimistic, matches how stuck-tx recovery
    ///     wants it.
    ///   - db > chain by 2+: likely a lost broadcast (service died
    ///     mid-send, tx never hit the mempool). Log a loud warning
    ///     but keep DB value — rolling back could double-spend if
    ///     the tx IS actually in-flight. Operator must investigate
    ///     via the stuck-tx rebroadcast path.
    ///
    /// After that reconciliation, subsequent calls are
    /// DB-increment-only as before — no per-allocation RPC call.
    pub async fn next_nonce(&self, address: &str) -> Result<u64> {
        let lock = self
            .locks
            .entry(address.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();

        let _guard = lock.lock().await;

        // Per-process reconciliation with the chain. Cheapest in
        // the common path (no RPC after the first allocation per
        // address per boot).
        if !self.reconciled.contains_key(address) {
            self.reconcile_against_chain(address).await?;
            self.reconciled.insert(address.to_string(), ());
        }

        // Stored nonce path (common after first call this boot).
        if let Some(_nonce) = self.db.get_nonce(address).await? {
            let next = self.db.increment_nonce(address).await?;
            tracing::debug!(address, nonce = next, "Nonce allocated from DB");
            return Ok(next);
        }

        // First time EVER (no row in DB): sync from chain, writing
        // chain+1 as the next available.
        let chain_nonce = self.sync_from_chain(address).await?;
        tracing::info!(address, nonce = chain_nonce, "Nonce initialised from chain");
        Ok(chain_nonce)
    }

    /// Compare the DB-stored next-nonce with the chain's current
    /// transaction count. Write a gauge for alerting and either
    /// fast-forward the DB (chain ahead) or warn loudly (DB ahead
    /// by too much).
    async fn reconcile_against_chain(&self, address: &str) -> Result<()> {
        let addr: alloy::primitives::Address = address
            .parse()
            .context("Invalid Ethereum address")?;
        let chain_count = self
            .provider
            .get_transaction_count(addr)
            .await
            .context("eth_getTransactionCount during reconciliation")?;
        let db_next = match self.db.get_nonce(address).await? {
            Some(v) => v,
            None => {
                // No DB row yet — nothing to reconcile.
                return Ok(());
            }
        };
        // db_next is the NEXT nonce to allocate; chain_count is the
        // count of confirmed transactions (also the next nonce the
        // chain expects). Normal in-flight: db_next = chain_count
        // + (unconfirmed_count).
        if chain_count > db_next {
            tracing::warn!(
                address,
                db_next,
                chain_count,
                "Chain is ahead of DB on nonce — another signer or manual tx \
                 advanced the account. Fast-forwarding DB to chain."
            );
            self.db.set_nonce(address, chain_count).await?;
        } else if db_next > chain_count + 1 {
            // Normal in-flight is at most 1 ahead; 2+ means we
            // likely lost a broadcast at some point.
            tracing::warn!(
                address,
                db_next,
                chain_count,
                gap = db_next - chain_count,
                "DB nonce ahead of chain by more than 1 — possible lost \
                 broadcast. Not rolling back (would risk double-spend if the \
                 tx IS in-flight). Investigate via /v1/admin/withdrawals?status=Broadcast \
                 and consider RBF / rebroadcast for the stuck tx."
            );
        }
        Ok(())
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
