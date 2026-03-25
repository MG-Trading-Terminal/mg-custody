//! Gas station background loop.
//!
//! Periodically checks deposit address gas balances and creates funding
//! transactions when addresses need gas for token transfers.

use std::time::Duration;

use anyhow::Result;
use custody_common::Chain;
use custody_db::CustodyDb;
use rust_decimal::Decimal;

use crate::gas_station::{GasStation, GasStationConfig};

/// Gas station background service.
pub struct GasStationService {
    gas_station: GasStation,
    db: CustodyDb,
    /// ETH RPC for checking balances (optional).
    eth_rpc_url: Option<String>,
    interval: Duration,
}

impl GasStationService {
    pub fn new(db: CustodyDb) -> Self {
        let configs = vec![
            GasStationConfig::ethereum_default(),
            GasStationConfig::tron_default(),
            GasStationConfig::solana_default(),
        ];

        Self {
            gas_station: GasStation::new(configs),
            db,
            eth_rpc_url: None,
            interval: Duration::from_secs(120), // check every 2 minutes
        }
    }

    pub fn with_eth_rpc(mut self, url: &str) -> Self {
        self.eth_rpc_url = Some(url.to_string());
        self
    }

    /// Run the gas station loop.
    pub async fn run(&self) -> Result<()> {
        tracing::info!("Starting gas station service");
        loop {
            if let Err(e) = self.tick().await {
                tracing::error!(error = %e, "Gas station tick failed");
            }
            tokio::time::sleep(self.interval).await;
        }
    }

    async fn tick(&self) -> Result<()> {
        // For each chain, get deposit addresses and check gas balances
        let chains = [Chain::Ethereum, Chain::Tron, Chain::Solana];

        for chain in &chains {
            let addresses = self.db.all_addresses_for_chain(*chain).await?;
            if addresses.is_empty() { continue; }

            // Build (address, chain, balance) tuples
            // For now, assume zero balance and let gas station decide
            // In production: query on-chain balance per address
            let mut address_balances = Vec::new();

            if let Some(ref rpc_url) = self.eth_rpc_url {
                if *chain == Chain::Ethereum {
                    let provider = custody_reconciliation::eth_balance::EthBalanceProvider::new(rpc_url);
                    for addr in addresses.iter().take(20) { // rate limit
                        let balance = provider.native_balance(addr).await.unwrap_or(Decimal::ZERO);
                        address_balances.push((addr.clone(), *chain, balance));
                    }
                }
            }

            // For TRX/SOL: would use chain-specific RPC to check balances
            // For now, skip non-ETH chains in gas balance check
            if address_balances.is_empty() { continue; }

            let funding_requests = self.gas_station.check_funding_needs(&address_balances);

            for req in &funding_requests {
                tracing::info!(
                    chain = ?req.chain,
                    address = %req.address,
                    current = %req.current_balance,
                    fund_amount = %req.fund_amount,
                    "Gas funding needed — creating internal transfer"
                );

                // Record the gas funding in the ledger
                let sweep_id = uuid::Uuid::new_v4();
                let asset = match req.chain {
                    Chain::Ethereum => "ETH",
                    Chain::Tron => "TRX",
                    Chain::Solana => "SOL",
                    Chain::Bitcoin => continue, // BTC doesn't need gas funding
                };

                if let Err(e) = self.db.record_sweep(
                    sweep_id, "hot", "gas_station", asset, req.chain, req.fund_amount,
                ).await {
                    tracing::error!(error = %e, "Failed to record gas funding");
                }
            }
        }

        Ok(())
    }
}
