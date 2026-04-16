//! Ethereum deposit scanner.
//!
//! Detects ETH native transfers and ERC-20 Transfer events to our deposit addresses.
//! Uses alloy for JSON-RPC communication.

use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, FixedBytes, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::consensus::Transaction as _;
use anyhow::{Context, Result};
use chrono::Utc;
use custody_common::{Asset, Chain, Deposit, DepositStatus};
use custody_db::CustodyDb;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::address_cache::AddressCache;
use crate::{ChainScanner, uint256_to_decimal};

/// ERC-20 Transfer(address,address,uint256) event topic.
/// keccak256("Transfer(address,address,uint256)")
const TRANSFER_TOPIC: FixedBytes<32> = FixedBytes::new([
    0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b,
    0x69, 0xc2, 0xb0, 0x68, 0xfc, 0x37, 0x8d, 0xaa,
    0x95, 0x2b, 0xa7, 0xf1, 0x63, 0xc4, 0xa1, 0x16,
    0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23, 0xb3, 0xef,
]);

/// Configuration for a tracked ERC-20 token.
#[derive(Debug, Clone)]
pub struct Erc20Config {
    pub contract: Address,
    pub symbol: String,
    pub decimals: u8,
}

/// Ethereum block scanner implementing ChainScanner.
pub struct EthereumScanner {
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
    erc20_tokens: Vec<Erc20Config>,
    required_confirmations: u64,
}

impl EthereumScanner {
    /// Create a new Ethereum scanner.
    pub fn new(
        rpc_url: &str,
        db: CustodyDb,
        address_cache: AddressCache,
        erc20_tokens: Vec<Erc20Config>,
        required_confirmations: u64,
    ) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .connect_http(rpc_url.parse().context("Invalid Ethereum RPC URL")?);

        Ok(Self {
            provider,
            db,
            address_cache,
            erc20_tokens,
            required_confirmations,
        })
    }

    /// Scan a range of blocks for ETH native transfers to our addresses.
    async fn scan_native_transfers(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<Deposit>> {
        let mut deposits = Vec::new();

        for block_num in from_block..=to_block {
            let block = self
                .provider
                .get_block_by_number(BlockNumberOrTag::Number(block_num))
                .await
                .with_context(|| format!("Failed to get block {block_num}"))?;

            let Some(block) = block else { continue };

            // Iterate transaction hashes from the block
            for tx_hash in block.transactions.hashes() {
                let tx_detail = self
                    .provider
                    .get_transaction_by_hash(tx_hash)
                    .await
                    .with_context(|| format!("Failed to get tx {tx_hash}"))?;

                let Some(tx_detail) = tx_detail else { continue };

                // In alloy 1.x, tx fields are accessed via the Transaction trait or inner
                let to_addr = tx_detail.to();
                let Some(to_addr) = to_addr else { continue };
                let to_hex = format!("{to_addr:#x}");

                if let Some(user_id) = self.address_cache.lookup(&to_hex) {
                    let value = tx_detail.value();
                    if value > U256::ZERO {
                        let amount = match uint256_to_decimal(value, 18) {
                            Ok(a) => a,
                            Err(e) => {
                                tracing::warn!(tx_hash = %tx_hash, "Skipping deposit: {e}");
                                continue;
                            }
                        };
                        let deposit = Deposit {
                            id: Uuid::new_v4(),
                            user_id,
                            asset: Asset::new("ETH"),
                            chain: Chain::Ethereum,
                            address: to_hex,
                            amount,
                            tx_hash: format!("{tx_hash:#x}"),
                            log_index: None,
                            block_height: block_num,
                            confirmations: 0,
                            required_confirmations: self.required_confirmations,
                            status: DepositStatus::Detected,
                            detected_at: Utc::now(),
                            confirmed_at: None,
                            credited_at: None,
                            memo: None,
                            source_memo: None,
                        };
                        deposits.push(deposit);
                    }
                }
            }
        }

        Ok(deposits)
    }

    /// Scan a range of blocks for ERC-20 Transfer events to our addresses.
    async fn scan_erc20_transfers(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<Deposit>> {
        if self.erc20_tokens.is_empty() {
            return Ok(vec![]);
        }

        let contract_addresses: Vec<Address> = self.erc20_tokens.iter().map(|t| t.contract).collect();

        let filter = Filter::new()
            .address(contract_addresses)
            .event_signature(TRANSFER_TOPIC)
            .from_block(from_block)
            .to_block(to_block);

        let logs = self
            .provider
            .get_logs(&filter)
            .await
            .context("Failed to get ERC-20 Transfer logs")?;

        let mut deposits = Vec::new();

        for log in logs {
            // Transfer(from, to, amount): topic[1]=from, topic[2]=to, data=amount
            if log.topics().len() < 3 {
                continue;
            }

            let to_topic = log.topics()[2];
            let to_addr = Address::from_slice(&to_topic[12..]);
            let to_hex = format!("{to_addr:#x}");

            if let Some(user_id) = self.address_cache.lookup(&to_hex) {
                // Decode amount from data
                let amount_raw = if log.data().data.len() >= 32 {
                    U256::from_be_slice(&log.data().data[..32])
                } else {
                    continue;
                };

                // Find token config
                let contract = log.address();
                let Some(token) = self.erc20_tokens.iter().find(|t| t.contract == contract) else {
                    continue;
                };

                let amount = match uint256_to_decimal(amount_raw, token.decimals) {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::warn!(contract = %contract, "Skipping ERC-20 deposit: {e}");
                        continue;
                    }
                };
                let block_number = log.block_number.unwrap_or(0);
                let tx_hash = log.transaction_hash.unwrap_or_default();
                let log_index = log.log_index.map(|i| i as u32);

                let deposit = Deposit {
                    id: Uuid::new_v4(),
                    user_id,
                    asset: Asset::new(&token.symbol),
                    chain: Chain::Ethereum,
                    address: to_hex,
                    amount,
                    tx_hash: format!("{tx_hash:#x}"),
                    log_index,
                    block_height: block_number,
                    confirmations: 0,
                    required_confirmations: self.required_confirmations,
                    status: DepositStatus::Detected,
                    detected_at: Utc::now(),
                    confirmed_at: None,
                    credited_at: None,
                            memo: None,
                            source_memo: None,
                };
                deposits.push(deposit);
            }
        }

        Ok(deposits)
    }
}

impl ChainScanner for EthereumScanner {
    fn chain(&self) -> Chain {
        Chain::Ethereum
    }

    async fn scan(&self, from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        let current = self.current_block_height().await?;

        // Only scan up to (current - required_confirmations) to avoid reorg risk
        let safe_block = current.saturating_sub(self.required_confirmations);
        if from_block > safe_block {
            return Ok((vec![], from_block));
        }

        // Limit batch size to avoid overwhelming the node
        let batch_size = 100;
        let to_block = std::cmp::min(from_block + batch_size - 1, safe_block);

        // Refresh address cache if stale
        self.address_cache.refresh_if_stale(current).await?;

        // Scan both native and ERC-20 transfers
        let mut deposits = self.scan_native_transfers(from_block, to_block).await?;
        let erc20_deposits = self.scan_erc20_transfers(from_block, to_block).await?;
        deposits.extend(erc20_deposits);

        // Dedup: check each deposit against DB
        let mut new_deposits = Vec::new();
        for deposit in deposits {
            let existing = self.db.find_deposit_by_tx_hash(&deposit.tx_hash).await?;
            if existing.is_none() {
                self.db.insert_deposit(&deposit).await?;
                new_deposits.push(deposit);
            }
        }

        tracing::info!(
            from = from_block,
            to = to_block,
            deposits = new_deposits.len(),
            "Ethereum scan complete"
        );

        Ok((new_deposits, to_block + 1))
    }

    async fn current_block_height(&self) -> Result<u64> {
        self.provider
            .get_block_number()
            .await
            .context("Failed to get Ethereum block number")
    }
}

/// Parse an ERC-20 Transfer event's `to` address from a log topic.
pub fn parse_transfer_to(topic: &FixedBytes<32>) -> Address {
    Address::from_slice(&topic[12..])
}

/// Parse an ERC-20 Transfer event's amount from log data.
pub fn parse_transfer_amount(data: &[u8], decimals: u8) -> anyhow::Result<Decimal> {
    let raw = if data.len() >= 32 {
        U256::from_be_slice(&data[..32])
    } else {
        U256::ZERO
    };
    uint256_to_decimal(raw, decimals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn transfer_topic_is_correct() {
        use alloy::primitives::keccak256;
        let computed = keccak256("Transfer(address,address,uint256)");
        assert_eq!(computed, TRANSFER_TOPIC);
    }

    #[test]
    fn parse_transfer_to_from_topic() {
        let mut topic = [0u8; 32];
        topic[12..32].copy_from_slice(&hex::decode("dAC17F958D2ee523a2206206994597C13D831ec7").unwrap());
        let addr = parse_transfer_to(&FixedBytes::new(topic));
        assert_eq!(
            format!("{addr:#x}"),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    #[test]
    fn parse_transfer_amount_usdt() {
        // 1,000,000 base units = 1 USDT (6 decimals)
        let mut data = [0u8; 32];
        data[29] = 0x0F;
        data[30] = 0x42;
        data[31] = 0x40; // 0x0F4240 = 1_000_000
        let amount = parse_transfer_amount(&data, 6).unwrap();
        assert_eq!(amount, dec!(1));
    }

    #[test]
    fn parse_transfer_amount_eth() {
        let raw = U256::from(1_000_000_000_000_000_000u128);
        let amount = uint256_to_decimal(raw, 18).unwrap();
        assert_eq!(amount, dec!(1));
    }
}
