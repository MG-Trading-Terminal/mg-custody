//! Withdrawal pipeline state machine.
//!
//! Full lifecycle:
//! Received → Validated → [PendingApproval →] Signed → Broadcast → Confirming → Finalized

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use custody_broadcaster::Broadcaster;
use custody_broadcaster::ethereum::EthereumBroadcaster;
use custody_common::{Chain, CustodyEvent, Withdrawal, WithdrawalStatus};
use custody_compliance::ofac::SanctionsChecker;
use custody_confirmations::WebhookSender;
use custody_db::CustodyDb;
use custody_fee_estimator::ethereum::EthereumFeeEstimator;
use custody_policy::{DestinationType, PolicyAction, PolicyEngine};
use custody_policy::rbac::Role;
use custody_signing::ethereum::{
    Erc20TransferParams, EthTransferParams, sign_erc20_transfer, sign_eth_transfer,
};
use rust_decimal::Decimal;

use crate::nonce_manager::NonceManager;
use crate::validation;

/// The main withdrawal pipeline orchestrator.
pub struct WithdrawalPipeline {
    db: CustodyDb,
    max_single_usd: Decimal,
    max_hourly_usd: Decimal,
    max_daily_usd: Decimal,
    policy_engine: Option<Arc<PolicyEngine>>,
    sanctions_checker: Option<Arc<SanctionsChecker>>,
    seed: Option<Arc<[u8; 64]>>,
    eth_broadcaster: Option<Arc<EthereumBroadcaster>>,
    eth_fee_estimator: Option<Arc<EthereumFeeEstimator>>,
    nonce_manager: Option<Arc<NonceManager>>,
    webhook_sender: Option<Arc<WebhookSender>>,
    eth_chain_id: u64,
}

impl WithdrawalPipeline {
    pub fn new(
        db: CustodyDb,
        max_single_usd: Decimal,
        max_hourly_usd: Decimal,
        max_daily_usd: Decimal,
    ) -> Self {
        Self {
            db, max_single_usd, max_hourly_usd, max_daily_usd,
            policy_engine: None, sanctions_checker: None, seed: None,
            eth_broadcaster: None, eth_fee_estimator: None, nonce_manager: None,
            webhook_sender: None, eth_chain_id: 1,
        }
    }

    pub fn with_policy_engine(mut self, e: Arc<PolicyEngine>) -> Self { self.policy_engine = Some(e); self }
    pub fn with_sanctions_checker(mut self, c: Arc<SanctionsChecker>) -> Self { self.sanctions_checker = Some(c); self }
    pub fn with_seed(mut self, s: Arc<[u8; 64]>) -> Self { self.seed = Some(s); self }
    pub fn with_eth_broadcaster(mut self, b: Arc<EthereumBroadcaster>) -> Self { self.eth_broadcaster = Some(b); self }
    pub fn with_eth_fee_estimator(mut self, f: Arc<EthereumFeeEstimator>) -> Self { self.eth_fee_estimator = Some(f); self }
    pub fn with_nonce_manager(mut self, n: Arc<NonceManager>) -> Self { self.nonce_manager = Some(n); self }
    pub fn with_webhook_sender(mut self, s: Arc<WebhookSender>) -> Self { self.webhook_sender = Some(s); self }
    pub fn with_eth_chain_id(mut self, id: u64) -> Self { self.eth_chain_id = id; self }

    pub async fn tick(&self) -> Result<()> {
        self.process_received().await?;
        self.process_validated().await?;
        self.process_signed().await?;
        self.process_broadcast().await?;
        Ok(())
    }

    pub async fn run(&self, interval: Duration) -> Result<()> {
        loop {
            if let Err(e) = self.tick().await { tracing::error!(error = %e, "Pipeline tick failed"); }
            tokio::time::sleep(interval).await;
        }
    }

    // ── Received → Validated ─────────────────────────────────────────

    async fn process_received(&self) -> Result<()> {
        // Query real outflow from DB for circuit breaker
        let hourly_used = self.db.hourly_outflow().await.unwrap_or(Decimal::ZERO);
        let daily_used = self.db.daily_outflow().await.unwrap_or(Decimal::ZERO);

        for w in self.db.withdrawals_by_status(WithdrawalStatus::Received).await? {
            match validation::validate_withdrawal(&w) {
                Ok(()) => {
                    match validation::check_circuit_breaker(
                        w.amount, self.max_single_usd, hourly_used,
                        self.max_hourly_usd, daily_used, self.max_daily_usd,
                    ) {
                        Ok(()) => {
                            self.db.update_withdrawal_status(w.id, WithdrawalStatus::Validated).await?;
                            tracing::info!(id = %w.id, "Validated");
                        }
                        Err(e) => tracing::warn!(id = %w.id, error = %e, "Circuit breaker"),
                    }
                }
                Err(e) => {
                    tracing::error!(id = %w.id, error = %e, "Validation failed");
                    self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                }
            }
        }
        Ok(())
    }

    // ── Validated → Signed ───────────────────────────────────────────

    async fn process_validated(&self) -> Result<()> {
        for w in self.db.withdrawals_by_status(WithdrawalStatus::Validated).await? {
            // Sanctions check
            if let Some(ref checker) = self.sanctions_checker {
                let result = checker.check(&w.destination_address);
                if result.is_sanctioned {
                    tracing::error!(id = %w.id, addr = %w.destination_address, "SANCTIONED — blocked");
                    self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                    continue;
                }
            }

            // Policy check
            if let Some(ref engine) = self.policy_engine {
                match engine.evaluate(w.chain, &w.asset, w.amount, &DestinationType::Any, &Role::Editor).action {
                    PolicyAction::Deny { ref reason } => {
                        tracing::warn!(id = %w.id, reason, "Policy denied");
                        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                        continue;
                    }
                    PolicyAction::RequireApproval { quorum, .. } => {
                        tracing::info!(id = %w.id, quorum, "Approval required");
                        self.db.update_withdrawal_status(w.id, WithdrawalStatus::PendingApproval).await?;
                        continue;
                    }
                    PolicyAction::Delay { .. } => continue,
                    PolicyAction::Allow => {}
                }
            }

            // Sign (ETH only for now)
            if w.chain == Chain::Ethereum {
                if let Err(e) = self.sign_eth(&w).await {
                    tracing::error!(id = %w.id, error = %e, "ETH sign failed");
                }
            }
        }
        Ok(())
    }

    async fn sign_eth(&self, w: &Withdrawal) -> Result<()> {
        let seed = self.seed.as_ref().ok_or_else(|| anyhow::anyhow!("No seed"))?;

        // Fee estimation
        let (max_fee, priority_fee) = match &self.eth_fee_estimator {
            Some(est) => match est.estimate().await {
                Ok(e) => {
                    if est.gas_exceeds_cap().await.unwrap_or(false) {
                        tracing::info!(id = %w.id, "Gas cap exceeded — deferring");
                        return Ok(());
                    }
                    (custody_fee_estimator::ethereum::gwei_to_wei(e.max_fee),
                     custody_fee_estimator::ethereum::gwei_to_wei(e.priority_fee))
                }
                Err(_) => (50_000_000_000u128, 2_000_000_000u128),
            },
            None => (50_000_000_000u128, 2_000_000_000u128),
        };

        // Nonce
        let nonce = match &self.nonce_manager {
            Some(nm) => {
                let addr = custody_signing::ethereum::eth_address(seed, 0)?;
                nm.next_nonce(&format!("{addr:#x}")).await.unwrap_or(0)
            }
            None => 0,
        };

        let to: alloy::primitives::Address = w.destination_address.parse()
            .map_err(|_| anyhow::anyhow!("Bad ETH address"))?;

        let signed_tx = if w.asset.as_str() == "ETH" {
            sign_eth_transfer(seed, 0, &EthTransferParams {
                to, value_wei: custody_scanner::decimal_to_uint256(w.amount, 18),
                nonce, chain_id: self.eth_chain_id,
                max_fee_per_gas: max_fee, max_priority_fee_per_gas: priority_fee,
                gas_limit: 21_000,
            })?
        } else {
            sign_erc20_transfer(seed, 0, &Erc20TransferParams {
                token_contract: alloy::primitives::Address::ZERO,
                to, amount: custody_scanner::decimal_to_uint256(w.amount, 6),
                nonce, chain_id: self.eth_chain_id,
                max_fee_per_gas: max_fee, max_priority_fee_per_gas: priority_fee,
                gas_limit: 65_000,
            })?
        };

        let gas_gwei = custody_fee_estimator::ethereum::wei_to_gwei(max_fee);
        self.db.set_signed_tx(w.id, &signed_tx, nonce, gas_gwei).await?;
        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Signed).await?;
        tracing::info!(id = %w.id, nonce, "Signed");
        Ok(())
    }

    // ── Signed → Broadcast ───────────────────────────────────────────

    async fn process_signed(&self) -> Result<()> {
        let Some(ref broadcaster) = self.eth_broadcaster else { return Ok(()) };

        for w in self.db.withdrawals_by_status(WithdrawalStatus::Signed).await? {
            if w.chain != Chain::Ethereum { continue; }

            let tx_bytes = match self.db.get_signed_tx_bytes(w.id).await? {
                Some(b) => b,
                None => { tracing::error!(id = %w.id, "No signed tx bytes"); continue; }
            };

            match broadcaster.broadcast(&tx_bytes).await {
                Ok(tx_hash) => {
                    self.db.set_withdrawal_tx_hash(w.id, &tx_hash).await?;
                    self.db.update_withdrawal_status(w.id, WithdrawalStatus::Broadcast).await?;
                    self.fire_webhook(&CustodyEvent::WithdrawalBroadcast {
                        withdrawal_id: w.id, exchange_withdrawal_id: w.exchange_withdrawal_id,
                        tx_hash, asset: w.asset.clone(), chain: w.chain,
                        amount: w.amount, fee: w.fee.unwrap_or(Decimal::ZERO),
                    }).await;
                    tracing::info!(id = %w.id, "Broadcast");
                }
                Err(e) => tracing::error!(id = %w.id, error = %e, "Broadcast failed"),
            }
        }
        Ok(())
    }

    // ── Broadcast → Confirming → Finalized ───────────────────────────

    async fn process_broadcast(&self) -> Result<()> {
        let Some(ref broadcaster) = self.eth_broadcaster else { return Ok(()) };

        for w in self.db.withdrawals_by_status(WithdrawalStatus::Broadcast).await? {
            let Some(ref tx_hash) = w.tx_hash else { continue };

            match broadcaster.tx_status(tx_hash).await? {
                Some(st) if st.success => {
                    if st.confirmations >= w.chain.required_confirmations() {
                        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Finalized).await?;
                        self.fire_webhook(&CustodyEvent::WithdrawalConfirmed {
                            withdrawal_id: w.id, exchange_withdrawal_id: w.exchange_withdrawal_id,
                            tx_hash: tx_hash.clone(), block_height: st.block_number,
                            confirmations: st.confirmations,
                        }).await;
                        tracing::info!(id = %w.id, "Finalized");
                    } else {
                        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Confirming).await?;
                    }
                }
                Some(_) => {
                    self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                    self.fire_webhook(&CustodyEvent::WithdrawalFailed {
                        withdrawal_id: w.id, exchange_withdrawal_id: w.exchange_withdrawal_id,
                        reason: "Transaction reverted".into(),
                    }).await;
                }
                None => tracing::debug!(id = %w.id, "Not yet mined"),
            }
        }
        Ok(())
    }

    async fn fire_webhook(&self, event: &CustodyEvent) {
        if let Some(ref sender) = self.webhook_sender {
            let _ = sender.send(event).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    #[test]
    fn pipeline_builder() {
        let _limits = (dec!(500000), dec!(10000000), dec!(50000000));
    }
}
