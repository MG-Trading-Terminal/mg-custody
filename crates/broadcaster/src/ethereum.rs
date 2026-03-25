//! Ethereum transaction broadcaster.
//!
//! Submits signed transactions via `eth_sendRawTransaction` and monitors
//! for inclusion via `eth_getTransactionReceipt`.

use alloy::primitives::B256;
use alloy::providers::{Provider, ProviderBuilder};
use anyhow::{Context, Result};
use custody_common::Chain;

use crate::{Broadcaster, TxStatus};

/// Ethereum broadcaster using alloy.
pub struct EthereumBroadcaster {
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
}

impl EthereumBroadcaster {
    pub fn new(rpc_url: &str) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .connect_http(rpc_url.parse().context("Invalid Ethereum RPC URL")?);
        Ok(Self { provider })
    }
}

impl Broadcaster for EthereumBroadcaster {
    fn chain(&self) -> Chain {
        Chain::Ethereum
    }

    async fn broadcast(&self, signed_tx: &[u8]) -> Result<String> {
        let pending = self
            .provider
            .send_raw_transaction(signed_tx)
            .await
            .context("eth_sendRawTransaction failed")?;

        let tx_hash = pending.tx_hash();
        tracing::info!(tx_hash = %tx_hash, "Transaction broadcast");
        Ok(format!("{tx_hash:#x}"))
    }

    async fn tx_status(&self, tx_hash: &str) -> Result<Option<TxStatus>> {
        let hash: B256 = tx_hash
            .parse()
            .context("Invalid transaction hash")?;

        let receipt = self.provider.get_transaction_receipt(hash).await?;

        match receipt {
            Some(receipt) => {
                let block_number = receipt.block_number.unwrap_or(0);
                let current = self.provider.get_block_number().await?;
                let confirmations = current.saturating_sub(block_number);
                let success = receipt.status();

                Ok(Some(TxStatus {
                    block_number,
                    confirmations,
                    success,
                }))
            }
            None => Ok(None), // Not yet mined
        }
    }
}
