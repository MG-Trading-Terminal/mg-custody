//! Blockchain deposit scanners.
//!
//! One scanner per chain. Each polls new blocks and checks transactions
//! against our known deposit address set.

use custody_common::{Chain, Deposit};

/// Trait for chain-specific block scanners.
pub trait ChainScanner: Send + Sync {
    /// Which chain this scanner handles.
    fn chain(&self) -> Chain;

    /// Scan blocks from `from_block` to the current chain tip.
    /// Returns newly detected deposits and the last scanned block height.
    fn scan(
        &self,
        from_block: u64,
    ) -> impl std::future::Future<Output = anyhow::Result<(Vec<Deposit>, u64)>> + Send;

    /// Get the current block height from the chain node.
    fn current_block_height(
        &self,
    ) -> impl std::future::Future<Output = anyhow::Result<u64>> + Send;
}
