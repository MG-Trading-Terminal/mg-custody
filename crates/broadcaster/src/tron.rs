//! Tron transaction broadcaster.
//!
//! Submits signed transactions via Tron HTTP API
//! (`/wallet/broadcasttransaction`).

use anyhow::Result;
use custody_common::Chain;

use crate::{Broadcaster, TxStatus};

/// Tron broadcaster using HTTP API.
pub struct TronBroadcaster {
    api_url: String,
    client: reqwest::Client,
}

impl TronBroadcaster {
    pub fn new(api_url: &str) -> Self {
        Self {
            api_url: api_url.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

impl Broadcaster for TronBroadcaster {
    fn chain(&self) -> Chain {
        Chain::Tron
    }

    async fn broadcast(&self, signed_tx: &[u8]) -> Result<String> {
        // TODO: POST /wallet/broadcasttransaction
        let _ = signed_tx;
        anyhow::bail!("Tron broadcast not yet implemented")
    }

    async fn tx_status(&self, tx_hash: &str) -> Result<Option<TxStatus>> {
        // TODO: POST /wallet/gettransactioninfobyid
        let _ = tx_hash;
        Ok(None)
    }
}
