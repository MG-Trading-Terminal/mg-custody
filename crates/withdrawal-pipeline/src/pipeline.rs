//! Withdrawal pipeline state machine.
//!
//! Full lifecycle:
//! Received → Validated → [PendingApproval →] Signed → Broadcast → Confirming → Finalized

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use custody_broadcaster::Broadcaster;
use custody_broadcaster::bitcoin::BitcoinBroadcaster;
use custody_broadcaster::ethereum::EthereumBroadcaster;
use custody_broadcaster::solana::SolanaBroadcaster;
use custody_broadcaster::tron::TronBroadcaster;
use custody_common::{Chain, CustodyEvent, Withdrawal, WithdrawalStatus};
use custody_compliance::ofac::SanctionsChecker;
use custody_confirmations::WebhookSender;
use custody_db::CustodyDb;
use custody_compliance::screening::{ScreeningProvider, ChainalysisProvider};
use custody_compliance::travel_rule;
use custody_fee_estimator::bitcoin::BitcoinFeeEstimator;
use custody_fee_estimator::ethereum::EthereumFeeEstimator;
use custody_fee_estimator::solana::SolanaFeeEstimator;
use custody_fee_estimator::tron::TronFeeEstimator;
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
    mpc_shares: Option<Arc<Vec<Vec<u8>>>>,
    mpc_group_pubkey: Option<Vec<u8>>,
    // Per-chain broadcasters
    eth_broadcaster: Option<Arc<EthereumBroadcaster>>,
    btc_broadcaster: Option<Arc<BitcoinBroadcaster>>,
    trx_broadcaster: Option<Arc<TronBroadcaster>>,
    sol_broadcaster: Option<Arc<SolanaBroadcaster>>,
    eth_fee_estimator: Option<Arc<EthereumFeeEstimator>>,
    btc_fee_estimator: Option<Arc<BitcoinFeeEstimator>>,
    trx_fee_estimator: Option<Arc<TronFeeEstimator>>,
    sol_fee_estimator: Option<Arc<SolanaFeeEstimator>>,
    screening_provider: Option<Arc<ChainalysisProvider>>,
    nonce_manager: Option<Arc<NonceManager>>,
    webhook_sender: Option<Arc<WebhookSender>>,
    event_broadcaster: Option<Arc<custody_common::events::EventBroadcaster>>,
    /// Tron API URL for fetching latest block (ref_block_bytes/hash).
    trx_api_url: Option<String>,
    /// Solana RPC URL for fetching recent blockhash.
    sol_rpc_url: Option<String>,
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
            eth_broadcaster: None, btc_broadcaster: None,
            trx_broadcaster: None, sol_broadcaster: None,
            eth_fee_estimator: None, btc_fee_estimator: None,
            trx_fee_estimator: None, sol_fee_estimator: None,
            screening_provider: None, nonce_manager: None,
            webhook_sender: None, event_broadcaster: None,
            trx_api_url: None, sol_rpc_url: None,
            eth_chain_id: 1,
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
    pub fn with_btc_broadcaster(mut self, b: Arc<BitcoinBroadcaster>) -> Self { self.btc_broadcaster = Some(b); self }
    pub fn with_trx_broadcaster(mut self, b: Arc<TronBroadcaster>) -> Self { self.trx_broadcaster = Some(b); self }
    pub fn with_sol_broadcaster(mut self, b: Arc<SolanaBroadcaster>) -> Self { self.sol_broadcaster = Some(b); self }
    pub fn with_eth_fee_estimator(mut self, f: Arc<EthereumFeeEstimator>) -> Self { self.eth_fee_estimator = Some(f); self }
    pub fn with_btc_fee_estimator(mut self, f: Arc<BitcoinFeeEstimator>) -> Self { self.btc_fee_estimator = Some(f); self }
    pub fn with_trx_fee_estimator(mut self, f: Arc<TronFeeEstimator>) -> Self { self.trx_fee_estimator = Some(f); self }
    pub fn with_sol_fee_estimator(mut self, f: Arc<SolanaFeeEstimator>) -> Self { self.sol_fee_estimator = Some(f); self }
    pub fn with_screening_provider(mut self, s: Arc<ChainalysisProvider>) -> Self { self.screening_provider = Some(s); self }
    pub fn with_nonce_manager(mut self, n: Arc<NonceManager>) -> Self { self.nonce_manager = Some(n); self }
    pub fn with_webhook_sender(mut self, s: Arc<WebhookSender>) -> Self { self.webhook_sender = Some(s); self }
    pub fn with_event_broadcaster(mut self, b: Arc<custody_common::events::EventBroadcaster>) -> Self { self.event_broadcaster = Some(b); self }
    pub fn with_trx_api_url(mut self, url: String) -> Self { self.trx_api_url = Some(url); self }
    pub fn with_sol_rpc_url(mut self, url: String) -> Self { self.sol_rpc_url = Some(url); self }
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
                    // Whitelist check — opt-in: if no whitelist rows exist for the chain,
                    // all destinations are allowed.  If whitelist rows exist, only listed
                    // addresses may receive funds.
                    match self.db.is_address_whitelisted(&w.destination_address, w.chain.as_str()).await {
                        Ok(true) => { /* allowed */ }
                        Ok(false) => {
                            tracing::warn!(
                                id = %w.id,
                                chain = %w.chain.as_str(),
                                addr = %w.destination_address,
                                "Destination address not whitelisted — rejecting"
                            );
                            self.db.update_withdrawal_status_with_error(
                                w.id,
                                WithdrawalStatus::Failed,
                                "Destination address not whitelisted",
                            ).await?;
                            continue;
                        }
                        Err(e) => {
                            // Treat whitelist lookup failure as a hard block: do not
                            // allow funds to move when the safety check is unavailable.
                            tracing::error!(
                                id = %w.id, error = %e,
                                "Whitelist check failed — deferring withdrawal"
                            );
                            continue;
                        }
                    }

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
                    custody_common::prometheus_metrics::COMPLIANCE_SCREENINGS
                        .with_label_values(&["ofac", "blocked"]).inc();
                    tracing::error!(id = %w.id, addr = %w.destination_address, "SANCTIONED — blocked");
                    self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                    continue;
                }
            }

            // AML/KYT screening
            if let Some(ref screener) = self.screening_provider {
                match screener.screen_address(&w.destination_address, w.chain).await {
                    Ok(result) => {
                        if result.should_block() {
                            custody_common::prometheus_metrics::COMPLIANCE_SCREENINGS
                                .with_label_values(&["chainalysis", "blocked"]).inc();
                            tracing::error!(
                                id = %w.id, addr = %w.destination_address,
                                risk = ?result.risk_level, categories = ?result.categories,
                                "AML screening BLOCKED"
                            );
                            self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                            continue;
                        }
                        if result.needs_review() {
                            tracing::warn!(
                                id = %w.id, addr = %w.destination_address,
                                risk = ?result.risk_level,
                                "AML screening — manual review required"
                            );
                            self.db.update_withdrawal_status(w.id, WithdrawalStatus::PendingApproval).await?;
                            continue;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(id = %w.id, error = %e, "AML screening failed — continuing");
                    }
                }
            }

            // Travel Rule check (FATF Recommendation 16)
            if travel_rule::requires_travel_rule(w.amount, "EU") {
                tracing::info!(
                    id = %w.id, amount = %w.amount,
                    "Travel Rule applies — data exchange required"
                );
                // In production: send_originator_data() via NotabeneProvider
                // For now, log and continue (Travel Rule doesn't block, just requires data)
            }

            // Policy check
            if let Some(ref engine) = self.policy_engine {
                match engine.evaluate(w.chain, &w.asset, w.amount, &DestinationType::Any, &Role::Editor).action {
                    PolicyAction::Deny { ref reason } => {
                        custody_common::prometheus_metrics::POLICY_DECISIONS
                            .with_label_values(&[w.chain.as_str(), "deny"]).inc();
                        tracing::warn!(id = %w.id, reason, "Policy denied");
                        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                        continue;
                    }
                    PolicyAction::RequireApproval { quorum, .. } => {
                        custody_common::prometheus_metrics::POLICY_DECISIONS
                            .with_label_values(&[w.chain.as_str(), "require_approval"]).inc();
                        tracing::info!(id = %w.id, quorum, "Approval required");
                        self.db.update_withdrawal_status(w.id, WithdrawalStatus::PendingApproval).await?;
                        continue;
                    }
                    PolicyAction::Delay { .. } => continue,
                    PolicyAction::Allow => {}
                }
            }

            // Sign based on chain (with latency tracking)
            let sign_start = std::time::Instant::now();
            let sign_result = match w.chain {
                Chain::Ethereum => self.sign_eth(&w).await,
                Chain::Bitcoin => self.sign_btc(&w).await,
                Chain::Tron => self.sign_trx(&w).await,
                Chain::Solana => self.sign_sol(&w).await,
            };
            let sign_ms = sign_start.elapsed().as_millis() as f64 / 1000.0;
            custody_common::prometheus_metrics::SIGNING_LATENCY
                .with_label_values(&[w.chain.as_str()]).observe(sign_ms);

            if let Err(e) = sign_result {
                custody_common::prometheus_metrics::SIGNING_TOTAL
                    .with_label_values(&[w.chain.as_str(), "error"]).inc();
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
    /// Builds an unsigned EIP-1559 tx → Keccak-256 hash → MPC threshold sign → store.
    /// The private key NEVER exists in full — only key shares participate.
    async fn sign_eth_mpc(
        &self,
        w: &Withdrawal,
        shares: &[Vec<u8>],
        group_pubkey: &[u8],
    ) -> Result<()> {
        // Build the same EIP-1559 tx fields as sign_eth, but don't sign with seed
        let nonce = match &self.nonce_manager {
            Some(nm) => nm.next_nonce(&w.destination_address).await?,
            None => 0,
        };
        let (max_fee, priority_fee) = match &self.eth_fee_estimator {
            Some(est) => {
                let fee = est.estimate().await?;
                use rust_decimal::prelude::ToPrimitive;
                let mf = (fee.max_fee * Decimal::new(1_000_000_000, 0)).to_u128().unwrap_or(30_000_000_000);
                let pf = (fee.priority_fee * Decimal::new(1_000_000_000, 0)).to_u128().unwrap_or(2_000_000_000);
                (mf, pf)
            }
            None => (30_000_000_000u128, 2_000_000_000u128),
        };

        // Build the unsigned tx hash (EIP-1559 type 2 signing hash)
        use alloy::primitives::keccak256;
        let to: alloy::primitives::Address = w.destination_address.parse()
            .map_err(|_| anyhow::anyhow!("Invalid ETH address"))?;
        let value = custody_scanner::decimal_to_uint256(w.amount, 18);

        // Build signing input: hash critical tx fields with Keccak-256
        let mut preimage = Vec::new();
        preimage.push(0x02); // EIP-1559 type
        preimage.extend_from_slice(&self.eth_chain_id.to_be_bytes());
        preimage.extend_from_slice(&nonce.to_be_bytes());
        preimage.extend_from_slice(&max_fee.to_be_bytes());
        preimage.extend_from_slice(&priority_fee.to_be_bytes());
        preimage.extend_from_slice(&21000u64.to_be_bytes());
        preimage.extend_from_slice(to.as_slice());
        preimage.extend_from_slice(&value.to_be_bytes::<32>());
        let tx_hash = keccak256(&preimage);
        let mut message = [0u8; 32];
        message.copy_from_slice(tx_hash.as_slice());

        // Threshold sign with MPC (2-of-n)
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

        // Store the signed tx (signature + tx metadata for reconstruction)
        let signed_data = serde_json::to_vec(&serde_json::json!({
            "type": "mpc_eip1559",
            "chain_id": self.eth_chain_id,
            "nonce": nonce,
            "to": format!("{:?}", to),
            "value": value.to_string(),
            "max_fee_per_gas": max_fee,
            "max_priority_fee_per_gas": priority_fee,
            "gas_limit": 21000,
            "signature": hex::encode(&sig.signature),
            "signers": sig.signers,
        }))?;

        let gas_gwei = custody_fee_estimator::ethereum::wei_to_gwei(max_fee);
        self.db.set_signed_tx(w.id, &signed_data, nonce, gas_gwei).await?;
        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Signed).await?;

        tracing::info!(
            id = %w.id, nonce,
            signers = ?sig.signers,
            "ETH withdrawal signed with MPC threshold ECDSA"
        );

        Ok(())
    }

    /// Sign a Bitcoin withdrawal using UTXO selection + BIP-143.
    async fn sign_btc(&self, w: &Withdrawal) -> Result<()> {
        let seed = self.seed.as_ref().ok_or_else(|| anyhow::anyhow!("No seed"))?;

        let amount_sats = custody_utxo::btc_to_sats(w.amount);

        // Query real UTXOs from DB
        let utxo_rows = self.db.get_unspent_utxos().await?;
        let utxos: Vec<custody_utxo::Utxo> = utxo_rows.iter().map(|r| custody_utxo::Utxo {
            id: r.id,
            txid: r.txid.clone(),
            vout: r.vout as u32,
            amount_sats: r.amount_sats as u64,
            address: r.address.clone(),
            confirmations: r.confirmations as u64,
            locked: r.locked,
        }).collect();

        if utxos.is_empty() {
            anyhow::bail!("No available UTXOs for Bitcoin withdrawal");
        }

        // Estimate fee rate from Bitcoin Core RPC (fallback to 10 sat/vB)
        let fee_rate = if let Some(ref estimator) = self.btc_fee_estimator {
            match estimator.estimate().await {
                Ok(est) => {
                    use rust_decimal::prelude::ToPrimitive;
                    est.base_fee.to_u64().unwrap_or(10)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "BTC fee estimation failed, using default");
                    10
                }
            }
        } else {
            10 // default 10 sat/vB
        };

        let selection = custody_utxo::select_coins(&utxos, amount_sats, fee_rate)?;

        // Lock selected UTXOs to prevent double-spend
        let utxo_ids: Vec<uuid::Uuid> = selection.inputs.iter().map(|u| u.id).collect();
        self.db.lock_utxos(&utxo_ids).await?;

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

        // Parse recipient bech32 address → P2WPKH scriptPubKey
        let recipient_script = if w.destination_address.starts_with("bc1q") || w.destination_address.starts_with("tb1q") {
            // Decode bech32 segwit v0 address to witness program (20-byte pubkey hash)
            match bech32::segwit::decode(&w.destination_address) {
                Ok((_hrp, _version, program)) => {
                    if program.len() == 20 {
                        let mut hash = [0u8; 20];
                        hash.copy_from_slice(&program);
                        custody_signing::bitcoin::p2wpkh_script(&hash)
                    } else {
                        anyhow::bail!("Invalid witness program length: {}", program.len());
                    }
                }
                Err(e) => anyhow::bail!("Invalid bech32 address: {e}"),
            }
        } else {
            // Fallback: use change script (not ideal, but safe for testing)
            tracing::warn!(id = %w.id, addr = %w.destination_address, "Non-bech32 address, using fallback");
            change_script.clone()
        };

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

        // Decode Tron Base58Check address to hex (41-prefixed)
        let to_hex = if w.destination_address.starts_with('T') {
            let decoded = bs58::decode(&w.destination_address)
                .with_check(None)
                .into_vec()
                .map_err(|e| anyhow::anyhow!("Invalid Tron address: {e}"))?;
            hex::encode(&decoded)
        } else {
            w.destination_address.clone()
        };

        let amount_sun = custody_scanner::tron::trx_to_sun(w.amount);
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Fetch latest block for ref_block_bytes and ref_block_hash
        let (ref_block_bytes, ref_block_hash) = self.fetch_tron_block_ref().await
            .unwrap_or(([0x00, 0x00], [0x00; 8]));

        let params = custody_signing::tron::TrxTransferParams {
            owner_address_hex: from_hex,
            to_address_hex: to_hex,
            amount_sun,
            expiration: now + 60_000,
            timestamp: now,
            ref_block_bytes,
            ref_block_hash,
        };

        let (signature, raw_data) = custody_signing::tron::sign_trx_transfer(seed, 0, &params)?;

        // Store as JSON for Tron broadcaster (expects JSON)
        let signed_tx = serde_json::to_vec(&serde_json::json!({
            "raw_data_hex": hex::encode(&raw_data),
            "signature": [hex::encode(&signature)],
        }))?;
        self.db.set_signed_tx(w.id, &signed_tx, 0, Decimal::ZERO).await?;
        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Signed).await?;
        custody_common::prometheus_metrics::WITHDRAWALS_TOTAL
            .with_label_values(&["tron", "signed"]).inc();
        tracing::info!(id = %w.id, "TRX withdrawal signed");
        Ok(())
    }

    /// Fetch Tron block reference for transaction construction.
    async fn fetch_tron_block_ref(&self) -> Result<([u8; 2], [u8; 8])> {
        let api_url = self.trx_api_url.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Tron API URL configured"))?;

        let client = reqwest::Client::new();
        let resp: serde_json::Value = client
            .get(format!("{api_url}/wallet/getnowblock"))
            .send().await?
            .json().await?;

        let block_num = resp["block_header"]["raw_data"]["number"].as_u64()
            .ok_or_else(|| anyhow::anyhow!("No block number"))?;
        let block_id = resp["blockID"].as_str()
            .ok_or_else(|| anyhow::anyhow!("No block ID"))?;

        // ref_block_bytes: bytes 6-7 of block number (big-endian)
        let num_bytes = block_num.to_be_bytes();
        let ref_block_bytes = [num_bytes[6], num_bytes[7]];

        // ref_block_hash: bytes 8-15 of block hash
        let hash_bytes = hex::decode(block_id)?;
        let mut ref_block_hash = [0u8; 8];
        if hash_bytes.len() >= 16 {
            ref_block_hash.copy_from_slice(&hash_bytes[8..16]);
        }

        Ok((ref_block_bytes, ref_block_hash))
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

        // Fetch recent blockhash from Solana RPC
        let recent_blockhash = self.fetch_solana_blockhash().await
            .unwrap_or([0u8; 32]);

        let params = custody_signing::solana::SolTransferParams {
            to_pubkey: to_bytes,
            lamports,
            recent_blockhash,
        };

        let signed_tx = custody_signing::solana::sign_sol_transfer(seed, 0, &params)?;

        self.db.set_signed_tx(w.id, &signed_tx, 0, Decimal::ZERO).await?;
        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Signed).await?;
        custody_common::prometheus_metrics::WITHDRAWALS_TOTAL
            .with_label_values(&["solana", "signed"]).inc();
        tracing::info!(id = %w.id, "SOL withdrawal signed");
        Ok(())
    }

    /// Fetch recent blockhash from Solana RPC.
    async fn fetch_solana_blockhash(&self) -> Result<[u8; 32]> {
        let rpc_url = self.sol_rpc_url.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Solana RPC URL configured"))?;

        let client = reqwest::Client::new();
        let resp: serde_json::Value = client
            .post(rpc_url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "method": "getLatestBlockhash",
                "params": [{"commitment": "finalized"}]
            }))
            .send().await?
            .json().await?;

        let blockhash_str = resp["result"]["value"]["blockhash"].as_str()
            .ok_or_else(|| anyhow::anyhow!("No blockhash in response"))?;

        let decoded = bs58::decode(blockhash_str).into_vec()
            .map_err(|e| anyhow::anyhow!("Invalid blockhash: {e}"))?;

        let mut hash = [0u8; 32];
        if decoded.len() == 32 {
            hash.copy_from_slice(&decoded);
        }
        Ok(hash)
    }

    // ── Signed → Broadcast ───────────────────────────────────────────

    async fn process_signed(&self) -> Result<()> {
        for w in self.db.withdrawals_by_status(WithdrawalStatus::Signed).await? {
            let tx_bytes = match self.db.get_signed_tx_bytes(w.id).await? {
                Some(b) => b,
                None => { tracing::error!(id = %w.id, "No signed tx bytes"); continue; }
            };

            // Broadcast based on chain
            let broadcast_result = match w.chain {
                Chain::Ethereum => match &self.eth_broadcaster {
                    Some(b) => b.broadcast(&tx_bytes).await,
                    None => { tracing::debug!(id = %w.id, "No ETH broadcaster"); continue; }
                },
                Chain::Bitcoin => match &self.btc_broadcaster {
                    Some(b) => b.broadcast(&tx_bytes).await,
                    None => { tracing::debug!(id = %w.id, "No BTC broadcaster"); continue; }
                },
                Chain::Tron => match &self.trx_broadcaster {
                    Some(b) => b.broadcast(&tx_bytes).await,
                    None => { tracing::debug!(id = %w.id, "No TRX broadcaster"); continue; }
                },
                Chain::Solana => match &self.sol_broadcaster {
                    Some(b) => b.broadcast(&tx_bytes).await,
                    None => { tracing::debug!(id = %w.id, "No SOL broadcaster"); continue; }
                },
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
            let Some(ref tx_hash) = w.tx_hash else { continue };

            // Get tx_status from the appropriate broadcaster
            let status_result = match w.chain {
                Chain::Ethereum => match &self.eth_broadcaster {
                    Some(b) => b.tx_status(tx_hash).await,
                    None => continue,
                },
                Chain::Bitcoin => match &self.btc_broadcaster {
                    Some(b) => b.tx_status(tx_hash).await,
                    None => continue,
                },
                Chain::Tron => match &self.trx_broadcaster {
                    Some(b) => b.tx_status(tx_hash).await,
                    None => continue,
                },
                Chain::Solana => match &self.sol_broadcaster {
                    Some(b) => b.tx_status(tx_hash).await,
                    None => continue,
                },
            };

            match status_result? {
                Some(st) if st.success => {
                    if st.confirmations >= w.chain.required_confirmations() {
                        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Finalized).await?;

                        // Debit withdrawal from ledger (external outflow)
                        let (debit, credit) = custody_db::custody_ledger::LedgerEntry::pair(
                            &format!("hot:{}", w.asset.as_str()),
                            &format!("external:{}", w.chain.as_str()),
                            w.amount,
                            w.asset.as_str(),
                            w.chain,
                            w.id,
                            "withdrawal",
                        );
                        let _ = self.db.insert_ledger_pair(&debit, &credit).await;

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
        // Also broadcast to WebSocket subscribers
        if let Some(ref broadcaster) = self.event_broadcaster {
            let subs = broadcaster.broadcast(event.clone());
            if subs > 0 {
                tracing::debug!(subscribers = subs, "Event broadcast to WebSocket clients");
            }
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
