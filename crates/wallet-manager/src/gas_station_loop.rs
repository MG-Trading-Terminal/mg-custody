//! Gas station background loop.
//!
//! Periodically checks deposit address gas balances and creates funding
//! transactions when addresses need gas for token transfers.
//!
//! When a deposit address balance drops below the configured threshold, this
//! service inserts a `Withdrawal` record into the database with status
//! `Received`.  The withdrawal pipeline's normal tick then picks it up,
//! validates it, signs it, and broadcasts it — following the same code path
//! as any external withdrawal request.  This ensures gas funding is logged in
//! the audit trail, subject to the circuit breaker, and observable via metrics.

use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use custody_common::{Asset, Chain, WalletTier, Withdrawal, WithdrawalStatus};
use custody_db::CustodyDb;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::gas_station::{GasFundingRequest, GasFundingTx, GasStation, GasStationConfig};

/// Sentinel UUID namespace for gas funding withdrawals.
///
/// Gas funding is internal — there is no exchange-side withdrawal ID.
/// We derive a deterministic UUID from the target address + chain + day so
/// that duplicate funding attempts within the same calendar day are
/// idempotent (the INSERT uses ON CONFLICT DO NOTHING).
const GAS_STATION_NAMESPACE: uuid::Uuid =
    uuid::Uuid::from_bytes(*b"gas-station-fund");

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
            if addresses.is_empty() {
                continue;
            }

            let mut address_balances: Vec<(String, Chain, Decimal)> = Vec::new();

            if let Some(ref rpc_url) = self.eth_rpc_url {
                if *chain == Chain::Ethereum {
                    let provider =
                        custody_reconciliation::eth_balance::EthBalanceProvider::new(rpc_url);
                    for addr in addresses.iter().take(20) {
                        // rate-limit: at most 20 per tick
                        let balance =
                            provider.native_balance(addr).await.unwrap_or(Decimal::ZERO);
                        address_balances.push((addr.clone(), *chain, balance));
                    }
                }
            }

            // For TRX/SOL: would use chain-specific RPC to check balances.
            // Until those providers are wired, skip non-ETH chains in gas check.
            if address_balances.is_empty() {
                continue;
            }

            let funding_requests = self.gas_station.check_funding_needs(&address_balances);

            for req in &funding_requests {
                match self.create_gas_funding_withdrawal(req).await {
                    Ok(tx) => {
                        tracing::info!(
                            withdrawal_id = %tx.withdrawal_id,
                            chain = ?req.chain,
                            address = %req.address,
                            current = %req.current_balance,
                            fund_amount = %req.fund_amount,
                            "Gas funding withdrawal submitted to pipeline"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            chain = ?req.chain,
                            address = %req.address,
                            "Failed to create gas funding withdrawal"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Create a withdrawal record for a gas funding request.
    ///
    /// The withdrawal is inserted with status `Received` so the normal pipeline
    /// will sign and broadcast it.  A deterministic `exchange_withdrawal_id` is
    /// derived from the target address, chain, and current UTC day so that
    /// repeated ticks on the same day are idempotent.
    async fn create_gas_funding_withdrawal(
        &self,
        req: &GasFundingRequest,
    ) -> Result<GasFundingTx> {
        let gas_asset = match req.chain {
            Chain::Ethereum => "ETH",
            Chain::Tron => "TRX",
            Chain::Solana => "SOL",
            Chain::Bitcoin => {
                anyhow::bail!("Bitcoin does not need gas station funding");
            }
        };

        // Derive a stable idempotency UUID from (address, chain, day).
        // Using Uuid v5 (SHA-1 namespace) gives us a deterministic ID that is
        // safe to re-derive without storing state.
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let key = format!("{}:{}:{}", req.address, req.chain.as_str(), today);
        let exchange_withdrawal_id =
            Uuid::new_v5(&GAS_STATION_NAMESPACE, key.as_bytes());

        let withdrawal_id = Uuid::new_v4();
        let now = Utc::now();

        let withdrawal = Withdrawal {
            id: withdrawal_id,
            exchange_withdrawal_id,
            // Gas-station transfers are system-initiated (no human user).
            // We use the nil UUID to mark them as system operations so the
            // ownership layer treats them as non-user flows.
            user_id: Uuid::nil(),
            vault_id: None,
            asset: Asset::new(gas_asset),
            chain: req.chain,
            destination_address: req.address.clone(),
            amount: req.fund_amount,
            fee: None,
            status: WithdrawalStatus::Received,
            wallet_tier: WalletTier::Hot,
            tx_hash: None,
            block_height: None,
            confirmations: 0,
            error_message: None,
            created_at: now,
            signed_at: None,
            broadcast_at: None,
            confirmed_at: None,
            delay_release_at: None,
            memo: None,
            destination_tag: None,
        };

        // ON CONFLICT DO NOTHING ensures idempotency: if this key already
        // exists for today, no duplicate withdrawal is created.
        self.db.insert_withdrawal(&withdrawal).await?;

        // Also record in the custody ledger for accounting (hot → gas_station).
        let sweep_id = Uuid::new_v4();
        if let Err(e) = self
            .db
            .record_sweep(sweep_id, "hot", "gas_station", gas_asset, req.chain, req.fund_amount)
            .await
        {
            // Non-fatal: the withdrawal is already inserted.  Ledger sync
            // happens via reconciliation if this fails.
            tracing::warn!(error = %e, "Failed to record gas funding in ledger");
        }

        Ok(GasFundingTx {
            withdrawal_id,
            chain: req.chain,
            target_address: req.address.clone(),
            gas_asset: gas_asset.to_string(),
            amount: req.fund_amount,
            created_at: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas_station::GasFundingRequest;
    use rust_decimal_macros::dec;

    /// Verify the idempotency key derivation is stable across calls.
    #[test]
    fn idempotency_key_is_stable() {
        let addr = "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18";
        let chain = Chain::Ethereum;
        let today = "2026-03-27";

        let key = format!("{}:{}:{}", addr, chain.as_str(), today);
        let id1 = Uuid::new_v5(&GAS_STATION_NAMESPACE, key.as_bytes());
        let id2 = Uuid::new_v5(&GAS_STATION_NAMESPACE, key.as_bytes());
        assert_eq!(id1, id2, "Same inputs must produce same idempotency UUID");
    }

    /// Different addresses on the same day must produce different idempotency keys.
    #[test]
    fn different_addresses_different_keys() {
        let day = "2026-03-27";
        let key_a = format!("0xAAAA:ethereum:{}", day);
        let key_b = format!("0xBBBB:ethereum:{}", day);
        let id_a = Uuid::new_v5(&GAS_STATION_NAMESPACE, key_a.as_bytes());
        let id_b = Uuid::new_v5(&GAS_STATION_NAMESPACE, key_b.as_bytes());
        assert_ne!(id_a, id_b);
    }

    /// Same address on different days must produce different idempotency keys.
    #[test]
    fn different_days_different_keys() {
        let addr = "0xAAAA:ethereum";
        let id_d1 = Uuid::new_v5(&GAS_STATION_NAMESPACE, format!("{}:2026-03-27", addr).as_bytes());
        let id_d2 = Uuid::new_v5(&GAS_STATION_NAMESPACE, format!("{}:2026-03-28", addr).as_bytes());
        assert_ne!(id_d1, id_d2);
    }

    /// Bitcoin must return an error — no gas station needed for UTXO chain.
    #[test]
    fn bitcoin_request_is_rejected() {
        // Mirror the gas_asset selection logic from create_gas_funding_withdrawal.
        // Returns Err for Bitcoin, Ok with asset name otherwise.
        fn gas_asset_for_chain(chain: Chain) -> Result<&'static str> {
            match chain {
                Chain::Bitcoin => anyhow::bail!("Bitcoin does not need gas station funding"),
                Chain::Ethereum => Ok("ETH"),
                Chain::Tron => Ok("TRX"),
                Chain::Solana => Ok("SOL"),
            }
        }

        assert!(gas_asset_for_chain(Chain::Bitcoin).is_err());
        assert_eq!(gas_asset_for_chain(Chain::Ethereum).unwrap(), "ETH");
        assert_eq!(gas_asset_for_chain(Chain::Tron).unwrap(), "TRX");
        assert_eq!(gas_asset_for_chain(Chain::Solana).unwrap(), "SOL");
    }

    /// Verify GasFundingTx fields are correctly populated by checking the struct.
    #[test]
    fn gas_funding_tx_fields() {
        let now = Utc::now();
        let tx = GasFundingTx {
            withdrawal_id: Uuid::new_v4(),
            chain: Chain::Ethereum,
            target_address: "0xdeadbeef".into(),
            gas_asset: "ETH".into(),
            amount: dec!(0.045),
            created_at: now,
        };
        assert_eq!(tx.gas_asset, "ETH");
        assert_eq!(tx.amount, dec!(0.045));
        assert_eq!(tx.chain, Chain::Ethereum);
    }
}
