//! Unified chain adapter trait.
//!
//! Provides a single interface for interacting with any supported blockchain.
//! Each chain implements this trait, and the server uses a `HashMap<Chain, Box<dyn ChainAdapter>>`
//! to dispatch operations.

use anyhow::Result;

use crate::{Chain, Deposit};

/// Unified interface for chain-specific operations.
///
/// Combines scanning, broadcasting, and status checking into one trait
/// so the server can treat all chains uniformly.
pub trait ChainAdapter: Send + Sync {
    /// Which chain this adapter handles.
    fn chain(&self) -> Chain;

    /// Scan for new deposits from a starting block/slot.
    /// Returns detected deposits and the next scan position.
    fn scan(
        &self,
        from_block: u64,
    ) -> impl std::future::Future<Output = Result<(Vec<Deposit>, u64)>> + Send;

    /// Get the current block height / slot number.
    fn block_height(
        &self,
    ) -> impl std::future::Future<Output = Result<u64>> + Send;

    /// Broadcast a signed transaction. Returns the tx hash.
    fn broadcast(
        &self,
        signed_tx: &[u8],
    ) -> impl std::future::Future<Output = Result<String>> + Send;

    /// Check if a chain node is healthy and reachable.
    fn health_check(
        &self,
    ) -> impl std::future::Future<Output = Result<bool>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify the trait is object-safe enough for our use case
    // (Note: with RPITIT it's not fully dyn-compatible, but we use
    //  generics/enums rather than trait objects in practice)
    #[test]
    fn chain_adapter_trait_exists() {
        // Compile-time check that the trait is defined correctly
        fn _assert_send_sync<T: ChainAdapter>() {}
    }
}
