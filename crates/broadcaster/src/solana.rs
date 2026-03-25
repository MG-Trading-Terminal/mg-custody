//! Solana transaction broadcaster.
//!
//! Submits transactions via `sendTransaction` RPC and handles
//! blockhash expiry with automatic retry.
//!
//! Solana-specific: transactions expire after ~90 seconds (blockhash TTL).
//! If expired, the transaction must be re-signed with a fresh blockhash.

use anyhow::Result;
use custody_common::Chain;

use crate::{Broadcaster, TxStatus};

/// Maximum retries with fresh blockhash.
pub const MAX_BLOCKHASH_RETRIES: u32 = 3;

/// Solana broadcaster.
pub struct SolanaBroadcaster {
    rpc_url: String,
    commitment: String,
}

impl SolanaBroadcaster {
    pub fn new(rpc_url: &str, commitment: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            commitment: commitment.to_string(),
        }
    }
}

impl Broadcaster for SolanaBroadcaster {
    fn chain(&self) -> Chain {
        Chain::Solana
    }

    async fn broadcast(&self, signed_tx: &[u8]) -> Result<String> {
        // TODO: sendTransaction RPC with skipPreflight=false
        // If "BlockhashNotFound" error, caller should re-sign with fresh blockhash
        let _ = signed_tx;
        anyhow::bail!("Solana broadcast not yet implemented")
    }

    async fn tx_status(&self, tx_hash: &str) -> Result<Option<TxStatus>> {
        // TODO: getTransaction RPC with commitment=finalized
        // If transaction not found, it may have expired (blockhash TTL)
        let _ = tx_hash;
        Ok(None)
    }
}

/// Check if an error indicates a blockhash expiry.
pub fn is_blockhash_expired(error_msg: &str) -> bool {
    error_msg.contains("BlockhashNotFound")
        || error_msg.contains("blockhash not found")
        || error_msg.contains("Transaction simulation failed: Blockhash not found")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockhash_expiry_detection() {
        assert!(is_blockhash_expired("BlockhashNotFound"));
        assert!(is_blockhash_expired("Transaction simulation failed: Blockhash not found"));
        assert!(!is_blockhash_expired("InsufficientFunds"));
        assert!(!is_blockhash_expired("success"));
    }
}
