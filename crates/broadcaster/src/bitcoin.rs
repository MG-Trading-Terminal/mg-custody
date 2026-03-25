//! Bitcoin transaction broadcaster.
//!
//! Submits signed transactions via Bitcoin Core RPC (`sendrawtransaction`)
//! and checks status via `getrawtransaction`.

use anyhow::Result;
use custody_common::Chain;

use crate::{Broadcaster, TxStatus};

/// Bitcoin broadcaster using Bitcoin Core RPC.
pub struct BitcoinBroadcaster {
    rpc_url: String,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
}

impl BitcoinBroadcaster {
    pub fn new(rpc_url: &str, rpc_user: Option<String>, rpc_password: Option<String>) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            rpc_user,
            rpc_password,
        }
    }
}

impl Broadcaster for BitcoinBroadcaster {
    fn chain(&self) -> Chain {
        Chain::Bitcoin
    }

    async fn broadcast(&self, signed_tx: &[u8]) -> Result<String> {
        // TODO: sendrawtransaction RPC
        let tx_hex = hex::encode(signed_tx);
        tracing::info!(tx_len = tx_hex.len(), "Bitcoin broadcast (stub)");
        anyhow::bail!("Bitcoin broadcast not yet implemented")
    }

    async fn tx_status(&self, tx_hash: &str) -> Result<Option<TxStatus>> {
        // TODO: getrawtransaction + getblockcount to compute confirmations
        let _ = tx_hash;
        Ok(None)
    }
}
