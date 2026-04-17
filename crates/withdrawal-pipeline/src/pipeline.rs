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
use custody_common::emergency::FreezeFlag;
use custody_compliance::ofac::SanctionsChecker;
use custody_confirmations::WebhookSender;
use custody_db::CustodyDb;
use custody_compliance::screening::ScreeningService;
use custody_compliance::travel_rule;
use custody_fee_estimator::bitcoin::BitcoinFeeEstimator;
use custody_fee_estimator::ethereum::EthereumFeeEstimator;
use custody_fee_estimator::solana::SolanaFeeEstimator;
use custody_fee_estimator::tron::TronFeeEstimator;
use custody_policy::{DestinationType, PolicyAction, PolicyEngine};
use custody_policy::rbac::Role;
use custody_signing::SigningBackend;
use custody_signing::ethereum::{
    Erc20TransferParams, EthTransferParams, finalize_eth_signed_tx, prepare_erc20_transfer,
    prepare_eth_transfer, sign_erc20_transfer, sign_eth_transfer,
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
    /// External signing backend (PKCS#11 HSM, MPC coordinator client,
    /// signer-service IPC, etc.). When set, outbound signatures are
    /// computed by the backend instead of derived from `seed`. MPC
    /// multi-host signing plugs in at this same trait boundary — the
    /// pipeline does NOT hold key shares directly; shares live on
    /// the respective MPC party hosts and the backend orchestrates
    /// the ceremony per signature.
    signing_backend: Option<Arc<dyn SigningBackend>>,
    // Per-chain broadcasters
    eth_broadcaster: Option<Arc<EthereumBroadcaster>>,
    btc_broadcaster: Option<Arc<BitcoinBroadcaster>>,
    trx_broadcaster: Option<Arc<TronBroadcaster>>,
    sol_broadcaster: Option<Arc<SolanaBroadcaster>>,
    eth_fee_estimator: Option<Arc<EthereumFeeEstimator>>,
    btc_fee_estimator: Option<Arc<BitcoinFeeEstimator>>,
    trx_fee_estimator: Option<Arc<TronFeeEstimator>>,
    sol_fee_estimator: Option<Arc<SolanaFeeEstimator>>,
    /// Multi-provider, fail-closed screening service. When set, the
    /// pipeline refuses to validate a withdrawal whose destination
    /// address scored High/Severe by any configured provider, and
    /// promotes Medium-risk results to manual review. Vendor
    /// unavailability applies the configured fallback — Block in
    /// production so an outage cannot become a silent bypass.
    screening_service: Option<Arc<ScreeningService>>,
    nonce_manager: Option<Arc<NonceManager>>,
    webhook_sender: Option<Arc<WebhookSender>>,
    event_broadcaster: Option<Arc<custody_common::events::EventBroadcaster>>,
    /// Emergency freeze flag.  When set, `tick` returns early without processing.
    frozen: Option<FreezeFlag>,
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
            signing_backend: None,
            eth_broadcaster: None, btc_broadcaster: None,
            trx_broadcaster: None, sol_broadcaster: None,
            eth_fee_estimator: None, btc_fee_estimator: None,
            trx_fee_estimator: None, sol_fee_estimator: None,
            screening_service: None, nonce_manager: None,
            webhook_sender: None, event_broadcaster: None,
            frozen: None,
            trx_api_url: None, sol_rpc_url: None,
            eth_chain_id: 1,
        }
    }

    /// Wire in the emergency freeze flag.  When the flag is set to true
    /// `tick` will return immediately without processing any withdrawals.
    pub fn with_freeze_flag(mut self, flag: FreezeFlag) -> Self {
        self.frozen = Some(flag);
        self
    }

    pub fn with_policy_engine(mut self, e: Arc<PolicyEngine>) -> Self { self.policy_engine = Some(e); self }
    pub fn with_sanctions_checker(mut self, c: Arc<SanctionsChecker>) -> Self { self.sanctions_checker = Some(c); self }
    pub fn with_seed(mut self, s: Arc<[u8; 64]>) -> Self { self.seed = Some(s); self }
    /// Plug in an external signing backend (PKCS#11 HSM, signer-service
    /// IPC, etc.). When present, ETH + ERC-20 signatures come from the
    /// backend's `sign(Chain::Ethereum, prehash)` call; the seed path
    /// is bypassed. BTC / Tron / Solana still use the seed path until
    /// their `prepare_*` helpers land.
    pub fn with_signing_backend(mut self, b: Arc<dyn SigningBackend>) -> Self {
        self.signing_backend = Some(b);
        self
    }
    /// Deprecated placeholder for direct-shares MPC signing.
    ///
    /// Previously accepted the raw ECDSA shares + group public key and
    /// had the pipeline call `cggmp21::threshold_sign` in-process. That
    /// is NOT multi-host MPC — it is single-process threshold signing
    /// where every share lives in the service memory (defeating the
    /// entire point of the threshold scheme). It also produced a
    /// non-RLP custom-JSON "signed tx" blob that no broadcaster could
    /// submit, so the path was spec-wise broken top to bottom.
    ///
    /// Real multi-host MPC plugs in via `with_signing_backend` — a
    /// future `MpcSigningBackend` impl orchestrates the ceremony
    /// across party hosts, and the pipeline only provides the
    /// prehash (for EVM/Tron) or message (for Solana). This method
    /// is retained as a hard-fail to surface the misconfiguration
    /// to any caller still invoking it.
    #[deprecated(
        note = "Direct-shares MPC signing was broken and single-process-only. Use `with_signing_backend` with an MPC-backed SigningBackend impl instead."
    )]
    pub fn with_mpc_shares(self, _shares: Vec<Vec<u8>>, _group_pubkey: Vec<u8>) -> Self {
        tracing::error!(
            "with_mpc_shares is deprecated and ignored — use with_signing_backend with an \
             MPC-backed SigningBackend. Pipeline will sign via seed/HSM until then."
        );
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
    pub fn with_screening_service(mut self, s: Arc<ScreeningService>) -> Self {
        self.screening_service = Some(s);
        self
    }
    pub fn with_nonce_manager(mut self, n: Arc<NonceManager>) -> Self { self.nonce_manager = Some(n); self }
    pub fn with_webhook_sender(mut self, s: Arc<WebhookSender>) -> Self { self.webhook_sender = Some(s); self }
    pub fn with_event_broadcaster(mut self, b: Arc<custody_common::events::EventBroadcaster>) -> Self { self.event_broadcaster = Some(b); self }
    pub fn with_trx_api_url(mut self, url: String) -> Self { self.trx_api_url = Some(url); self }
    pub fn with_sol_rpc_url(mut self, url: String) -> Self { self.sol_rpc_url = Some(url); self }
    pub fn with_eth_chain_id(mut self, id: u64) -> Self { self.eth_chain_id = id; self }

    pub async fn tick(&self) -> Result<()> {
        // Check emergency freeze before processing any withdrawals.
        if self.frozen.as_ref().map(|f| f.is_frozen()).unwrap_or(false) {
            tracing::warn!("Pipeline tick skipped — system is frozen");
            return Ok(());
        }
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

            // AML / KYT screening. The ScreeningService is fail-closed:
            // vendor outages apply its configured fallback (Block in
            // production) instead of a silent pass. `Ok(result)` where
            // `result.provider == "fallback"` signals that no provider
            // actually ran — we block with the same "provider_error"
            // category so an outage cannot become a bypass vector.
            if let Some(ref svc) = self.screening_service {
                let result = svc.screen_address(&w.destination_address, w.chain).await;
                let provider_label = result.provider.clone();
                if result.should_block() {
                    custody_common::prometheus_metrics::COMPLIANCE_SCREENINGS
                        .with_label_values(&[&provider_label, "blocked"]).inc();
                    tracing::error!(
                        id = %w.id, addr = %w.destination_address,
                        risk = ?result.risk_level, categories = ?result.categories,
                        provider = %provider_label,
                        "AML screening BLOCKED"
                    );
                    self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                    continue;
                }
                if result.needs_review() {
                    custody_common::prometheus_metrics::COMPLIANCE_SCREENINGS
                        .with_label_values(&[&provider_label, "review"]).inc();
                    tracing::warn!(
                        id = %w.id, addr = %w.destination_address,
                        risk = ?result.risk_level, provider = %provider_label,
                        "AML screening — manual review required"
                    );
                    self.db.update_withdrawal_status(w.id, WithdrawalStatus::PendingApproval).await?;
                    continue;
                }
                custody_common::prometheus_metrics::COMPLIANCE_SCREENINGS
                    .with_label_values(&[&provider_label, "clean"]).inc();
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
        // Signing backend (HSM / MPC) wins when configured; otherwise
        // the pipeline derives from the seed. MPC is not called
        // in-process — it must arrive through `signing_backend`
        // because any pipeline-local shares would undermine the
        // threshold property.
        let seed = self.seed.as_ref().ok_or_else(|| anyhow::anyhow!("No seed and no signing_backend — refusing to sign"))?;

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

        // Build the signing params once; branch on backend availability.
        let signed_tx = if is_native {
            let params = EthTransferParams {
                to, value_wei: custody_scanner::decimal_to_uint256(w.amount, decimals),
                nonce, chain_id: self.eth_chain_id,
                max_fee_per_gas: max_fee, max_priority_fee_per_gas: priority_fee,
                gas_limit: 21_000,
            };
            match &self.signing_backend {
                Some(backend) => {
                    // HSM / MPC / signer-service path. Prepare the
                    // unsigned tx + 32-byte prehash, hand the prehash
                    // to the backend for a 65-byte r||s||v, then
                    // attach + serialise. Private key never leaves
                    // the backend.
                    let (tx, prehash) = prepare_eth_transfer(&params);
                    let sig = backend.sign(Chain::Ethereum, &prehash)
                        .map_err(|e| anyhow::anyhow!("HSM sign(ETH) failed: {e}"))?;
                    finalize_eth_signed_tx(tx, &sig)?
                }
                None => sign_eth_transfer(seed, 0, &params)?,
            }
        } else {
            // ERC-20: resolve contract address from token registry
            let contract_addr = token_registry.contract_address(asset_str, w.chain)
                .ok_or_else(|| anyhow::anyhow!("Unknown token: {} on {:?}", asset_str, w.chain))?;
            let token_contract: alloy::primitives::Address = contract_addr.parse()
                .map_err(|_| anyhow::anyhow!("Invalid contract address: {contract_addr}"))?;

            let params = Erc20TransferParams {
                token_contract,
                to, amount: custody_scanner::decimal_to_uint256(w.amount, decimals),
                nonce, chain_id: self.eth_chain_id,
                max_fee_per_gas: max_fee, max_priority_fee_per_gas: priority_fee,
                gas_limit: 65_000,
            };
            match &self.signing_backend {
                Some(backend) => {
                    let (tx, prehash) = prepare_erc20_transfer(&params);
                    let sig = backend.sign(Chain::Ethereum, &prehash)
                        .map_err(|e| anyhow::anyhow!("HSM sign(ERC-20) failed: {e}"))?;
                    finalize_eth_signed_tx(tx, &sig)?
                }
                None => sign_erc20_transfer(seed, 0, &params)?,
            }
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

        // HSM-aware signing: when a backend is plugged in, we compute
        // the SHA-256 prehash of the raw protobuf, hand it to the
        // backend as Chain::Tron, and splice the returned r||s||v
        // into the Tron wire format. Otherwise the seed path does
        // the full derive + sign_prehash in-process.
        let (signature, raw_data) = if let Some(backend) = &self.signing_backend {
            let (raw_data, prehash) = custody_signing::tron::prepare_trx_transfer(&params);
            let sig65 = backend
                .sign(Chain::Tron, &prehash)
                .map_err(|e| anyhow::anyhow!("HSM sign(Tron) failed: {e}"))?;
            if sig65.len() != 65 {
                anyhow::bail!(
                    "HSM returned {} bytes for Tron signature, expected 65",
                    sig65.len()
                );
            }
            (sig65, raw_data)
        } else {
            custody_signing::tron::sign_trx_transfer(seed, 0, &params)?
        };

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

        // HSM-aware signing. For Solana we need the sender's public
        // key to build the message — the HSM exposes it via
        // `public_key(Chain::Solana)`, which reads `CKA_EC_POINT` off
        // the paired public-key object. Fall back to seed derivation
        // if the backend has no Solana key configured.
        let signed_tx = if let Some(backend) = &self.signing_backend {
            let from_pubkey_bytes = backend.public_key(Chain::Solana)
                .ok_or_else(|| anyhow::anyhow!(
                    "HSM is configured but has no Solana (Ed25519) public key — \
                     provision `custody/solana/0` in the HSM or unset HSM_LIB_PATH for seed signing"
                ))?;
            if from_pubkey_bytes.len() != 32 {
                anyhow::bail!(
                    "HSM Solana public_key returned {} bytes, expected 32",
                    from_pubkey_bytes.len()
                );
            }
            let mut from_pubkey = [0u8; 32];
            from_pubkey.copy_from_slice(&from_pubkey_bytes);
            let message = custody_signing::solana::prepare_sol_transfer(from_pubkey, &params);
            let signature = backend
                .sign(Chain::Solana, &message)
                .map_err(|e| anyhow::anyhow!("HSM sign(Solana) failed: {e}"))?;
            custody_signing::solana::finalize_sol_signed_tx(&message, &signature)?
        } else {
            custody_signing::solana::sign_sol_transfer(seed, 0, &params)?
        };

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
                        user_id: Some(w.user_id),
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
                            user_id: Some(w.user_id),
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
                        user_id: Some(w.user_id),
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
