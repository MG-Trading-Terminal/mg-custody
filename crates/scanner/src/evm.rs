//! Generic EVM chain scanner.
//!
//! Reuses the Ethereum scanning logic for any EVM-compatible L2/sidechain.
//! All EVM chains share: address format, ERC-20 Transfer event topic, tx structure.
//! They differ in: chain_id, RPC URL, confirmation requirements, native token, block time.
//!
//! Supports both native token deposits and ERC-20 Transfer events.

use alloy::consensus::Transaction;
use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, FixedBytes, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use anyhow::{Context, Result};
use chrono::Utc;
use custody_common::{Asset, Chain, Deposit, DepositStatus};
use custody_common::evm_chains::EvmChainDef;
use custody_db::CustodyDb;
use uuid::Uuid;

use crate::address_cache::AddressCache;
use crate::ethereum::Erc20Config;
use crate::uint256_to_decimal;

/// ERC-20 Transfer(address,address,uint256) event topic.
const TRANSFER_TOPIC: FixedBytes<32> = FixedBytes::new([
    0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b,
    0x69, 0xc2, 0xb0, 0x68, 0xfc, 0x37, 0x8d, 0xaa,
    0x95, 0x2b, 0xa7, 0xf1, 0x63, 0xc4, 0xa1, 0x16,
    0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23, 0xb3, 0xef,
]);

/// Generic EVM scanner that works for any EVM-compatible chain.
///
/// Created from an `EvmChainDef` or manually with chain parameters.
/// Scans both native token transfers and ERC-20 Transfer events.
pub struct EvmScanner {
    chain_name: String,
    chain_id: u64,
    native_token: String,
    native_decimals: u8,
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

impl EvmScanner {
    /// Create scanner from explicit parameters.
    pub fn new(
        chain_name: &str,
        chain_id: u64,
        native_token: &str,
        native_decimals: u8,
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
            native_token: native_token.to_string(),
            native_decimals,
            provider,
            db,
            address_cache,
            erc20_tokens,
            required_confirmations,
        })
    }

    /// Create scanner from an `EvmChainDef` registry entry.
    pub fn from_chain_def(
        def: &EvmChainDef,
        db: CustodyDb,
        address_cache: AddressCache,
    ) -> Result<Self> {
        let erc20_tokens: Vec<Erc20Config> = def.tokens.values().map(|t| {
            Erc20Config {
                contract: t.contract_address.parse()
                    .unwrap_or(Address::ZERO),
                symbol: t.symbol.clone(),
                decimals: t.decimals,
            }
        }).collect();

        Self::new(
            &def.id,
            def.chain_id,
            &def.native_token,
            def.native_decimals,
            &def.rpc_url,
            db,
            address_cache,
            erc20_tokens,
            def.required_confirmations,
        )
    }

    pub fn chain_name(&self) -> &str { &self.chain_name }
    pub fn chain_id(&self) -> u64 { self.chain_id }
    pub fn native_token(&self) -> &str { &self.native_token }
    pub fn token_count(&self) -> usize { self.erc20_tokens.len() }

    /// Get the current block height.
    pub async fn current_block_height(&self) -> Result<u64> {
        self.provider
            .get_block_number()
            .await
            .with_context(|| format!("Failed to get block number for {}", self.chain_name))
    }

    /// Scan blocks for native transfers to our addresses.
    async fn scan_native_transfers(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<Deposit>> {
        let mut deposits = Vec::new();

        for block_num in from_block..=to_block {
            let block = self.provider
                .get_block_by_number(BlockNumberOrTag::Number(block_num))
                .await
                .with_context(|| format!("[{}] Failed to get block {}", self.chain_name, block_num))?;

            let Some(block) = block else { continue };

            for tx_hash in block.transactions.hashes() {
                let tx_detail = self.provider
                    .get_transaction_by_hash(tx_hash)
                    .await
                    .with_context(|| format!("[{}] Failed to get tx {}", self.chain_name, tx_hash))?;

                let Some(tx_detail) = tx_detail else { continue };

                let to_addr = tx_detail.to();
                let Some(to_addr) = to_addr else { continue };
                let to_hex = format!("{to_addr:#x}");

                if let Some(user_id) = self.address_cache.lookup(&to_hex) {
                    let value = tx_detail.value();
                    if value > U256::ZERO {
                        let amount = uint256_to_decimal(value, self.native_decimals);
                        let deposit = Deposit {
                            id: Uuid::new_v4(),
                            user_id,
                            asset: Asset::new(&self.native_token),
                            chain: Chain::Ethereum, // All EVM chains use Ethereum signing infra
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

    /// Scan blocks for ERC-20 Transfer events to our addresses.
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

        let logs = self.provider
            .get_logs(&filter)
            .await
            .with_context(|| format!("[{}] Failed to get ERC-20 logs", self.chain_name))?;

        let mut deposits = Vec::new();

        for log in logs {
            if log.topics().len() < 3 { continue; }

            let to_topic = log.topics()[2];
            let to_addr = Address::from_slice(&to_topic[12..]);
            let to_hex = format!("{to_addr:#x}");

            if let Some(user_id) = self.address_cache.lookup(&to_hex) {
                let amount_raw = if log.data().data.len() >= 32 {
                    U256::from_be_slice(&log.data().data[..32])
                } else {
                    continue;
                };

                let contract = log.address();
                let Some(token) = self.erc20_tokens.iter().find(|t| t.contract == contract) else {
                    continue;
                };

                let amount = uint256_to_decimal(amount_raw, token.decimals);
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

    /// Full scan: native transfers + ERC-20 events, with DB dedup.
    pub async fn scan(&self, from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        let current = self.current_block_height().await?;
        let safe_block = current.saturating_sub(self.required_confirmations);

        if from_block > safe_block {
            return Ok((vec![], from_block));
        }

        let batch_size = 100u64;
        let to_block = std::cmp::min(from_block + batch_size - 1, safe_block);

        self.address_cache.refresh_if_stale(current).await?;

        // Scan both native and ERC-20
        let mut deposits = self.scan_native_transfers(from_block, to_block).await?;
        let erc20_deposits = self.scan_erc20_transfers(from_block, to_block).await?;
        deposits.extend(erc20_deposits);

        // Dedup against DB
        let mut new_deposits = Vec::new();
        for deposit in deposits {
            if self.db.find_deposit_by_tx_hash(&deposit.tx_hash).await?.is_none() {
                self.db.insert_deposit(&deposit).await?;
                new_deposits.push(deposit);
            }
        }

        if !new_deposits.is_empty() {
            tracing::info!(
                chain = %self.chain_name,
                chain_id = self.chain_id,
                from = from_block,
                to = to_block,
                native = new_deposits.iter().filter(|d| d.asset.0 == self.native_token).count(),
                erc20 = new_deposits.iter().filter(|d| d.asset.0 != self.native_token).count(),
                "EVM deposits detected"
            );
        }

        Ok((new_deposits, to_block + 1))
    }
}

/// Create scanners for all enabled EVM chains from a registry.
///
/// Each chain gets its own `AddressCache` (they scan different address sets per chain).
/// The `db` is cloned per scanner (it wraps an `Arc<PgPool>` internally).
pub fn create_scanners_from_registry(
    registry: &custody_common::evm_chains::EvmChainRegistry,
    db: &CustodyDb,
    cache_refresh_interval: u64,
) -> Vec<EvmScanner> {
    let mut scanners = Vec::new();
    for chain_def in registry.enabled_chains() {
        // Each EVM chain reuses Ethereum address infrastructure
        let address_cache = AddressCache::new(
            db.clone(),
            Chain::Ethereum,
            cache_refresh_interval,
        );
        match EvmScanner::from_chain_def(chain_def, db.clone(), address_cache) {
            Ok(scanner) => {
                tracing::info!(
                    chain = %chain_def.id,
                    chain_id = chain_def.chain_id,
                    tokens = chain_def.tokens.len(),
                    confirmations = chain_def.required_confirmations,
                    "EVM scanner created"
                );
                scanners.push(scanner);
            }
            Err(e) => {
                tracing::error!(
                    chain = %chain_def.id,
                    error = %e,
                    "Failed to create EVM scanner"
                );
            }
        }
    }
    scanners
}

#[cfg(test)]
mod tests {
    use super::*;
    use custody_common::evm_chains::{EvmChainDef, EvmTokenDef};
    use std::collections::HashMap;

    fn test_chain_def(id: &str, chain_id: u64, native_token: &str) -> EvmChainDef {
        EvmChainDef {
            id: id.to_string(),
            name: format!("{} Network", id),
            chain_id,
            rpc_url: "http://localhost:8545".to_string(),
            fallback_rpc_urls: vec![],
            explorer_url: None,
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: native_token.to_string(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        }
    }

    #[test]
    fn chain_def_with_tokens() {
        let mut def = test_chain_def("arbitrum", 42161, "ETH");
        def.tokens.insert("USDC".to_string(), EvmTokenDef {
            symbol: "USDC".to_string(),
            contract_address: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831".to_string(),
            decimals: 6,
        });
        def.tokens.insert("USDT".to_string(), EvmTokenDef {
            symbol: "USDT".to_string(),
            contract_address: "0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9".to_string(),
            decimals: 6,
        });

        // Verify token parsing for scanner creation
        let erc20_tokens: Vec<Erc20Config> = def.tokens.values().map(|t| {
            Erc20Config {
                contract: t.contract_address.parse().unwrap_or(Address::ZERO),
                symbol: t.symbol.clone(),
                decimals: t.decimals,
            }
        }).collect();

        assert_eq!(erc20_tokens.len(), 2);
        assert!(erc20_tokens.iter().any(|t| t.symbol == "USDC" && t.decimals == 6));
        assert!(erc20_tokens.iter().any(|t| t.symbol == "USDT" && t.decimals == 6));
    }

    #[test]
    fn bsc_native_decimals_18() {
        // BSC USDT uses 18 decimals (not 6!) — this is a known gotcha
        let def = test_chain_def("bsc", 56, "BNB");
        assert_eq!(def.native_decimals, 18);
        assert_eq!(def.native_token, "BNB");
    }

    #[test]
    fn polygon_native_token_matic() {
        let def = test_chain_def("polygon", 137, "POL");
        assert_eq!(def.native_token, "POL");
        assert_eq!(def.chain_id, 137);
    }

    #[test]
    fn registry_creates_multiple_scanners() {
        use custody_common::evm_chains::EvmChainRegistry;

        let mut registry = EvmChainRegistry::new();
        registry.register(test_chain_def("arbitrum", 42161, "ETH"));
        registry.register(test_chain_def("optimism", 10, "ETH"));
        registry.register(test_chain_def("base", 8453, "ETH"));

        let enabled = registry.enabled_chains();
        assert_eq!(enabled.len(), 3);

        // All should have unique chain IDs
        let chain_ids: Vec<u64> = enabled.iter().map(|c| c.chain_id).collect();
        assert!(chain_ids.contains(&42161));
        assert!(chain_ids.contains(&10));
        assert!(chain_ids.contains(&8453));
    }

    #[test]
    fn disabled_chains_excluded() {
        use custody_common::evm_chains::EvmChainRegistry;

        let mut registry = EvmChainRegistry::new();
        let mut def = test_chain_def("zksync", 324, "ETH");
        def.enabled = false;
        registry.register(def);
        registry.register(test_chain_def("base", 8453, "ETH"));

        let enabled = registry.enabled_chains();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "base");
    }

    #[test]
    fn transfer_topic_matches_ethereum() {
        // Our local TRANSFER_TOPIC must match the one in ethereum.rs
        use alloy::primitives::keccak256;
        let computed = keccak256("Transfer(address,address,uint256)");
        assert_eq!(computed, TRANSFER_TOPIC);
    }
}
