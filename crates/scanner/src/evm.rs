//! Generic EVM chain scanner.
//!
//! Reuses the Ethereum scanning logic for any EVM-compatible L2.
//! Differences: chain_id, RPC URL, confirmation requirements, block time.
//! Shares address derivation, signing, and transaction format with Ethereum.

use alloy::consensus::Transaction;
use alloy::network::ReceiptResponse;
use alloy::providers::{Provider, ProviderBuilder};
use anyhow::{Context, Result};
use chrono::Utc;
use custody_common::{Asset, Chain, Deposit, DepositStatus};
use custody_db::CustodyDb;
use uuid::Uuid;

use crate::address_cache::AddressCache;
use crate::ethereum::Erc20Config;

/// Generic EVM scanner that works for any EVM-compatible chain.
/// Wraps an alloy provider pointed at any EVM RPC endpoint.
pub struct EvmScanner {
    chain_name: String,
    chain_id: u64,
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
    db: CustodyDb,
    address_cache: AddressCache,
    #[allow(dead_code)]
    erc20_tokens: Vec<Erc20Config>,
    required_confirmations: u64,
}

impl EvmScanner {
    pub fn new(
        chain_name: &str,
        chain_id: u64,
        rpc_url: &str,
        db: CustodyDb,
        address_cache: AddressCache,
        erc20_tokens: Vec<Erc20Config>,
        required_confirmations: u64,
    ) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .connect_http(rpc_url.parse().context("Invalid EVM RPC URL")?);

        Ok(Self {
            chain_name: chain_name.to_string(),
            chain_id,
            provider,
            db,
            address_cache,
            erc20_tokens,
            required_confirmations,
        })
    }

    pub fn chain_name(&self) -> &str { &self.chain_name }
    pub fn chain_id(&self) -> u64 { self.chain_id }

    /// Get the current block height.
    pub async fn current_block_height(&self) -> Result<u64> {
        Ok(self.provider.get_block_number().await?)
    }

    /// Scan a range of blocks for native ETH deposits to our addresses.
    pub async fn scan(&self, from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        let current = self.current_block_height().await?;
        let safe_block = current.saturating_sub(self.required_confirmations);

        if from_block > safe_block {
            return Ok((vec![], from_block));
        }

        let batch_size = 50u64;
        let to_block = std::cmp::min(from_block + batch_size - 1, safe_block);

        self.address_cache.refresh_if_stale(current).await?;

        let mut deposits = Vec::new();

        // Check blocks for transactions to our addresses
        for height in from_block..=to_block {
            let block = self.provider
                .get_block_by_number(height.into())
                .await?;

            if let Some(block) = block {
                for tx_hash in block.transactions.hashes() {
                    let hash = alloy::primitives::B256::from(*tx_hash);
                    // Get receipt to check destination
                    let receipt = match self.provider.get_transaction_receipt(hash).await {
                        Ok(Some(r)) => r,
                        _ => continue,
                    };

                    let to_addr = match receipt.to() {
                        Some(addr) => format!("{addr:#x}"),
                        None => continue, // contract creation
                    };

                    if !self.address_cache.contains(&to_addr) {
                        continue;
                    }

                    // Get tx for value
                    let tx = match self.provider.get_transaction_by_hash(hash).await {
                        Ok(Some(t)) => t,
                        _ => continue,
                    };

                    let value = tx.inner.value();
                    if value == alloy::primitives::U256::ZERO {
                        continue;
                    }

                    let amount = crate::uint256_to_decimal(value, 18);
                    let user_id = self.address_cache.lookup(&to_addr).unwrap_or(Uuid::nil());

                    let deposit = Deposit {
                        id: Uuid::new_v4(),
                        user_id,
                        asset: Asset::new(&self.chain_name.to_uppercase()),
                        chain: Chain::Ethereum, // EVM chains reuse Ethereum infrastructure
                        address: to_addr,
                        amount,
                        tx_hash: format!("{:#x}", tx_hash),
                        log_index: None,
                        block_height: height,
                        confirmations: 0,
                        required_confirmations: self.required_confirmations,
                        status: DepositStatus::Detected,
                        detected_at: Utc::now(),
                        confirmed_at: None,
                        credited_at: None,
                    };

                    if self.db.find_deposit_by_tx_hash(&deposit.tx_hash).await?.is_none() {
                        self.db.insert_deposit(&deposit).await?;
                        deposits.push(deposit);
                    }
                }
            }
        }

        if !deposits.is_empty() {
            tracing::info!(
                chain = %self.chain_name, chain_id = self.chain_id,
                from = from_block, to = to_block, count = deposits.len(),
                "EVM deposits detected"
            );
        }

        Ok((deposits, to_block + 1))
    }
}
