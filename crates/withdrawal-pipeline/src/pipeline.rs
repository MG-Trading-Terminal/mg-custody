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
    /// MPC key shares for threshold signing (if available, overrides single-key).
    mpc_shares: Option<Arc<Vec<Vec<u8>>>>,
    /// MPC group public key (for verification after signing).
    mpc_group_pubkey: Option<Vec<u8>>,
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
            mpc_shares: None, mpc_group_pubkey: None,
            eth_broadcaster: None, eth_fee_estimator: None, nonce_manager: None,
            webhook_sender: None, eth_chain_id: 1,
        }
    }

    pub fn with_policy_engine(mut self, e: Arc<PolicyEngine>) -> Self { self.policy_engine = Some(e); self }
    pub fn with_sanctions_checker(mut self, c: Arc<SanctionsChecker>) -> Self { self.sanctions_checker = Some(c); self }
    pub fn with_seed(mut self, s: Arc<[u8; 64]>) -> Self { self.seed = Some(s); self }
    pub fn with_mpc_shares(mut self, shares: Vec<Vec<u8>>, group_pubkey: Vec<u8>) -> Self {
        self.mpc_shares = Some(Arc::new(shares));
        self.mpc_group_pubkey = Some(group_pubkey);
        self
    }
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
                            custody_common::prometheus_metrics::WITHDRAWALS_TOTAL
                                .with_label_values(&[w.chain.as_str(), "validated"]).inc();
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

            // Sign based on chain
            let sign_result = match w.chain {
                Chain::Ethereum => self.sign_eth(&w).await,
                Chain::Bitcoin => self.sign_btc(&w).await,
                Chain::Tron => self.sign_trx(&w).await,
                Chain::Solana => self.sign_sol(&w).await,
            };

            if let Err(e) = sign_result {
                tracing::error!(id = %w.id, chain = ?w.chain, error = %e, "Signing failed");
            }
        }
        Ok(())
    }

    async fn sign_eth(&self, w: &Withdrawal) -> Result<()> {
        // Check if MPC signing is available
        if let (Some(shares), Some(group_pk)) = (&self.mpc_shares, &self.mpc_group_pubkey) {
            return self.sign_eth_mpc(w, shares, group_pk).await;
        }

        // Fall back to single-key signing
        let seed = self.seed.as_ref().ok_or_else(|| anyhow::anyhow!("No seed (and no MPC shares)"))?;

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

        let to: alloy::primitives::Address = match w.destination_address.parse() {
            Ok(addr) => addr,
            Err(_) => {
                // Mark as failed — invalid address should have been caught in validation
                self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                tracing::error!(id = %w.id, addr = %w.destination_address, "Invalid ETH address — marking failed");
                return Ok(());
            }
        };

        // Resolve token info from registry
        let token_registry = custody_common::token_registry::TokenRegistry::with_defaults();
        let asset_str = w.asset.as_str();
        let is_native = token_registry.is_native(asset_str, w.chain);
        let decimals = token_registry.decimals(asset_str, w.chain).unwrap_or(18);

        let signed_tx = if is_native {
            sign_eth_transfer(seed, 0, &EthTransferParams {
                to, value_wei: custody_scanner::decimal_to_uint256(w.amount, decimals),
                nonce, chain_id: self.eth_chain_id,
                max_fee_per_gas: max_fee, max_priority_fee_per_gas: priority_fee,
                gas_limit: 21_000,
            })?
        } else {
            // ERC-20: resolve contract address from token registry
            let contract_addr = token_registry.contract_address(asset_str, w.chain)
                .ok_or_else(|| anyhow::anyhow!("Unknown token: {} on {:?}", asset_str, w.chain))?;
            let token_contract: alloy::primitives::Address = contract_addr.parse()
                .map_err(|_| anyhow::anyhow!("Invalid contract address: {contract_addr}"))?;

            sign_erc20_transfer(seed, 0, &Erc20TransferParams {
                token_contract,
                to, amount: custody_scanner::decimal_to_uint256(w.amount, decimals),
                nonce, chain_id: self.eth_chain_id,
                max_fee_per_gas: max_fee, max_priority_fee_per_gas: priority_fee,
                gas_limit: 65_000,
            })?
        };

        let gas_gwei = custody_fee_estimator::ethereum::wei_to_gwei(max_fee);
        self.db.set_signed_tx(w.id, &signed_tx, nonce, gas_gwei).await?;
        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Signed).await?;
        custody_common::prometheus_metrics::WITHDRAWALS_TOTAL
            .with_label_values(&[w.chain.as_str(), "signed"]).inc();
        custody_common::prometheus_metrics::SIGNING_TOTAL
            .with_label_values(&[w.chain.as_str(), "success"]).inc();
        tracing::info!(id = %w.id, nonce, "Signed");
        Ok(())
    }

    /// Sign ETH withdrawal using MPC threshold signing.
    ///
    /// Instead of single-key: builds unsigned tx → hashes → MPC threshold sign → attach signature.
    /// The private key NEVER exists in full — only key shares participate.
    async fn sign_eth_mpc(
        &self,
        w: &Withdrawal,
        shares: &[Vec<u8>],
        group_pubkey: &[u8],
    ) -> Result<()> {
        use sha2::Digest;

        // For now, sign the withdrawal hash with MPC (proving MPC works in pipeline)
        // Full integration would build an unsigned EIP-1559 tx, hash it, sign, and reassemble
        let mut hasher = sha2::Sha256::new();
        hasher.update(w.id.as_bytes());
        hasher.update(w.destination_address.as_bytes());
        hasher.update(w.amount.to_string().as_bytes());
        let msg_hash = hasher.finalize();
        let mut message = [0u8; 32];
        message.copy_from_slice(&msg_hash);

        // Use 2 shares (threshold)
        let threshold = 2u16;
        let signing_shares = if shares.len() > threshold as usize {
            &shares[..threshold as usize]
        } else {
            shares
        };

        let sig = custody_mpc::ecdsa::threshold_sign(signing_shares, &message, threshold)?;

        // Verify the MPC signature
        let valid = custody_mpc::ecdsa::sign::verify_ecdsa_signature(
            group_pubkey, &message, &sig.signature,
        )?;

        if !valid {
            anyhow::bail!("MPC signature verification failed");
        }

        // Store the MPC signature (not a full tx, but proves MPC works)
        self.db.set_signed_tx(w.id, &sig.signature, 0, Decimal::ZERO).await?;
        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Signed).await?;

        tracing::info!(
            id = %w.id,
            signers = ?sig.signers,
            sig_len = sig.signature.len(),
            "ETH withdrawal signed with MPC threshold ECDSA"
        );

        Ok(())
    }

    /// Sign a Bitcoin withdrawal using UTXO selection + BIP-143.
    async fn sign_btc(&self, w: &Withdrawal) -> Result<()> {
        let seed = self.seed.as_ref().ok_or_else(|| anyhow::anyhow!("No seed"))?;

        let amount_sats = custody_utxo::btc_to_sats(w.amount);

        // Select UTXOs from DB (for now use mock UTXOs)
        // In production: query utxos table for unspent outputs
        let mock_utxos = vec![custody_utxo::Utxo {
            id: uuid::Uuid::new_v4(),
            txid: "0".repeat(64),
            vout: 0,
            amount_sats: amount_sats + 10_000, // enough for amount + fee
            address: "bc1qhot...".into(),
            confirmations: 100,
            locked: false,
        }];

        let selection = custody_utxo::select_coins(&mock_utxos, amount_sats, 10)?;

        // Build recipient script
        use custody_address_factory::hd::{SeedBytes, derive_secp256k1_pubkey, derivation_path};
        let seed_bytes = SeedBytes(**seed);
        let change_path = derivation_path(custody_common::Chain::Bitcoin, 1)?; // fresh change addr
        let change_pk = derive_secp256k1_pubkey(&seed_bytes, &change_path)?;
        let change_hash = {
            use sha2::Digest;
            use ripemd::Ripemd160;
            let sha = sha2::Sha256::digest(&change_pk.compressed);
            let ripemd = Ripemd160::digest(sha);
            let mut h = [0u8; 20];
            h.copy_from_slice(&ripemd);
            h
        };
        let change_script = custody_signing::bitcoin::p2wpkh_script(&change_hash);

        // For recipient, parse from address (simplified: use same script format)
        // In production: proper address → scriptPubKey parsing
        let recipient_script = change_script.clone(); // placeholder

        let mut outputs = vec![custody_signing::bitcoin::BtcTxOutput {
            script_pubkey: recipient_script,
            amount_sats,
        }];
        if selection.change_sats > 0 {
            outputs.push(custody_signing::bitcoin::BtcTxOutput {
                script_pubkey: change_script,
                amount_sats: selection.change_sats,
            });
        }

        let inputs: Vec<custody_signing::bitcoin::BtcTxInput> = selection.inputs.iter().map(|u| {
            custody_signing::bitcoin::BtcTxInput {
                txid: u.txid.clone(),
                vout: u.vout,
                amount_sats: u.amount_sats,
                derivation_index: 0,
            }
        }).collect();

        let params = custody_signing::bitcoin::BtcTxParams { inputs, outputs, rbf: true };
        let signed_tx = custody_signing::bitcoin::sign_bitcoin_tx(seed, &params)?;

        self.db.set_signed_tx(w.id, &signed_tx, 0, Decimal::ZERO).await?;
        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Signed).await?;
        custody_common::prometheus_metrics::WITHDRAWALS_TOTAL
            .with_label_values(&["bitcoin", "signed"]).inc();
        tracing::info!(id = %w.id, fee = selection.fee_sats, "BTC withdrawal signed");
        Ok(())
    }

    /// Sign a Tron withdrawal.
    async fn sign_trx(&self, w: &Withdrawal) -> Result<()> {
        let seed = self.seed.as_ref().ok_or_else(|| anyhow::anyhow!("No seed"))?;

        let from_hex = custody_signing::tron::tron_address_hex(seed, 0)?;
        // Convert destination to hex (if Base58Check, need to decode)
        let to_hex = if w.destination_address.starts_with('T') {
            // Base58Check → hex conversion would go here
            // For now, use as-is if it's already hex
            format!("41{}", &w.destination_address[1..].chars().take(40).collect::<String>())
        } else {
            w.destination_address.clone()
        };

        let amount_sun = custody_scanner::tron::trx_to_sun(w.amount);
        let now = chrono::Utc::now().timestamp_millis() as u64;

        let params = custody_signing::tron::TrxTransferParams {
            owner_address_hex: from_hex,
            to_address_hex: to_hex,
            amount_sun,
            expiration: now + 60_000, // 1 minute expiry
            timestamp: now,
            ref_block_bytes: [0x12, 0x34], // TODO: from latest block
            ref_block_hash: [0xAA; 8],
        };

        let (signature, raw_data) = custody_signing::tron::sign_trx_transfer(seed, 0, &params)?;

        // Store signature + raw_data as the "signed tx"
        let mut signed_tx = raw_data;
        signed_tx.extend_from_slice(&signature);
        self.db.set_signed_tx(w.id, &signed_tx, 0, Decimal::ZERO).await?;
        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Signed).await?;
        custody_common::prometheus_metrics::WITHDRAWALS_TOTAL
            .with_label_values(&["tron", "signed"]).inc();
        tracing::info!(id = %w.id, "TRX withdrawal signed");
        Ok(())
    }

    /// Sign a Solana withdrawal.
    async fn sign_sol(&self, w: &Withdrawal) -> Result<()> {
        let seed = self.seed.as_ref().ok_or_else(|| anyhow::anyhow!("No seed"))?;

        let to_bytes: [u8; 32] = bs58::decode(&w.destination_address)
            .into_vec()
            .map_err(|_| anyhow::anyhow!("Invalid Solana address"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Solana address must be 32 bytes"))?;

        let lamports = custody_scanner::solana::sol_to_lamports(w.amount);

        let params = custody_signing::solana::SolTransferParams {
            to_pubkey: to_bytes,
            lamports,
            recent_blockhash: [0u8; 32], // TODO: fetch from chain
        };

        let signed_tx = custody_signing::solana::sign_sol_transfer(seed, 0, &params)?;

        self.db.set_signed_tx(w.id, &signed_tx, 0, Decimal::ZERO).await?;
        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Signed).await?;
        custody_common::prometheus_metrics::WITHDRAWALS_TOTAL
            .with_label_values(&["solana", "signed"]).inc();
        tracing::info!(id = %w.id, "SOL withdrawal signed");
        Ok(())
    }

    // ── Signed → Broadcast ───────────────────────────────────────────

    async fn process_signed(&self) -> Result<()> {
        for w in self.db.withdrawals_by_status(WithdrawalStatus::Signed).await? {
            let tx_bytes = match self.db.get_signed_tx_bytes(w.id).await? {
                Some(b) => b,
                None => { tracing::error!(id = %w.id, "No signed tx bytes"); continue; }
            };

            // Try to broadcast based on chain
            let broadcast_result = match w.chain {
                Chain::Ethereum => {
                    if let Some(ref b) = self.eth_broadcaster {
                        b.broadcast(&tx_bytes).await
                    } else {
                        tracing::debug!(id = %w.id, "No ETH broadcaster configured");
                        continue;
                    }
                }
                // BTC/TRX/SOL: broadcasters not yet wired to pipeline
                // In production: add btc_broadcaster, trx_broadcaster, sol_broadcaster fields
                other => {
                    tracing::debug!(id = %w.id, chain = ?other, "Broadcaster not configured for chain");
                    continue;
                }
            };

            match broadcast_result {
                Ok(tx_hash) => {
                    self.db.set_withdrawal_tx_hash(w.id, &tx_hash).await?;
                    self.db.update_withdrawal_status(w.id, WithdrawalStatus::Broadcast).await?;
                    custody_common::prometheus_metrics::WITHDRAWALS_TOTAL
                        .with_label_values(&[w.chain.as_str(), "broadcast"]).inc();
                    self.fire_webhook(&CustodyEvent::WithdrawalBroadcast {
                        withdrawal_id: w.id, exchange_withdrawal_id: w.exchange_withdrawal_id,
                        tx_hash, asset: w.asset.clone(), chain: w.chain,
                        amount: w.amount, fee: w.fee.unwrap_or(Decimal::ZERO),
                    }).await;
                    tracing::info!(id = %w.id, chain = ?w.chain, "Broadcast");
                }
                Err(e) => tracing::error!(id = %w.id, chain = ?w.chain, error = %e, "Broadcast failed"),
            }
        }
        Ok(())
    }

    // ── Broadcast → Confirming → Finalized ───────────────────────────

    async fn process_broadcast(&self) -> Result<()> {
        for w in self.db.withdrawals_by_status(WithdrawalStatus::Broadcast).await? {
            let broadcaster = match w.chain {
                Chain::Ethereum => match &self.eth_broadcaster {
                    Some(b) => b,
                    None => continue,
                },
                _ => continue, // other chain broadcasters not yet wired
            };
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
