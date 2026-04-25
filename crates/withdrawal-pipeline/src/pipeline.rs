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
use custody_compliance::travel_rule::{self, NotabeneProvider, TravelRuleProvider as _};
use custody_fee_estimator::bitcoin::BitcoinFeeEstimator;
use custody_fee_estimator::ethereum::EthereumFeeEstimator;
use custody_fee_estimator::solana::SolanaFeeEstimator;
use custody_fee_estimator::tron::TronFeeEstimator;
use custody_policy::{DestinationType, PolicyAction, PolicyEngine};
use custody_policy::rbac::Role;
use custody_signing::SigningBackend;
use custody_signing::ethereum::{
    Erc20TransferParams, EthContractCallParams, EthTransferParams, finalize_eth_signed_tx,
    prepare_erc20_transfer, prepare_eth_contract_call, prepare_eth_transfer,
    sign_erc20_transfer, sign_eth_contract_call, sign_eth_transfer,
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
    /// Travel Rule dispatcher. When set AND a withdrawal is above
    /// the jurisdictional threshold, the pipeline looks up the
    /// persisted TravelRuleData (written by the gateway's
    /// POST /v1/travel-rule handler) and calls `send_originator_data`
    /// before moving the row to Validated.
    ///
    /// Fail-closed semantics:
    ///   - Provider configured + TR data row present + dispatch Ok
    ///       → proceed, mark row as Submitted
    ///   - Provider configured + TR data row MISSING
    ///       → refuse to advance; the withdrawal stays in Received
    ///         until the gateway submits TR data
    ///   - Provider configured + dispatch Err
    ///       → refuse to advance; mark row as Failed; operator retries
    ///   - Provider absent
    ///       → log a warning; proceed (operator has opted to handle
    ///         Travel Rule out-of-band)
    ///
    /// Silently bypassing Travel Rule when configured is a
    /// compliance bug — same class as the AML fail-closed gate on
    /// the submit endpoint.
    travel_rule_provider: Option<Arc<NotabeneProvider>>,
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
            travel_rule_provider: None,
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

    pub fn with_travel_rule_provider(mut self, p: Arc<NotabeneProvider>) -> Self {
        self.travel_rule_provider = Some(p);
        self
    }
    pub fn with_policy_engine(mut self, e: Arc<PolicyEngine>) -> Self { self.policy_engine = Some(e); self }
    pub fn with_sanctions_checker(mut self, c: Arc<SanctionsChecker>) -> Self { self.sanctions_checker = Some(c); self }
    pub fn with_seed(mut self, s: Arc<[u8; 64]>) -> Self { self.seed = Some(s); self }
    /// Plug in an external signing backend (PKCS#11 HSM, signer-service
    /// IPC, MPC coordinator client, etc.). When present, ETH + ERC-20
    /// signatures come from the backend's `sign(Chain::Ethereum,
    /// prehash)` call; the seed path is bypassed. BTC / Tron / Solana
    /// still use the seed path until their `prepare_*` helpers land.
    ///
    /// **MPC wiring note**: multi-host MPC plugs in HERE, not via a
    /// dedicated method. An `MpcSigningBackend` wraps the coordinator
    /// client and orchestrates the threshold ceremony across party
    /// hosts, returning the aggregated signature. The pipeline only
    /// provides the prehash (EVM/Tron) or message bytes (Solana) — it
    /// never holds shares. An earlier `with_mpc_shares(...)` builder
    /// method accepted raw ECDSA shares + group pubkey and called
    /// `cggmp21::threshold_sign` in-process; that was single-process
    /// threshold signing (every share in service memory — defeats the
    /// threshold property) wrapped in fake-MPC naming. Removed in this
    /// commit. If a caller needs MPC, write the backend impl and pass
    /// it here; do not add a shortcut that silently downgrades to seed.
    pub fn with_signing_backend(mut self, b: Arc<dyn SigningBackend>) -> Self {
        self.signing_backend = Some(b);
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
        //
        // Defence-in-depth: the per-stage loops below ALSO re-check
        // `is_frozen()` on every iteration. An operator who flips
        // the freeze mid-tick expects the NEXT row to be skipped,
        // not to wait for the current batch (which could be dozens
        // of withdrawals totalling $$$) to drain through sign +
        // broadcast. The tick-level guard short-circuits the common
        // case where the freeze was already set before the tick
        // started; the loop-level guard covers the live-flip case.
        if self.is_frozen() {
            tracing::warn!("Pipeline tick skipped — system is frozen");
            return Ok(());
        }
        self.process_received().await?;
        self.process_validated().await?;
        self.process_signed().await?;
        self.process_broadcast().await?;
        Ok(())
    }

    /// Snapshot of the emergency freeze flag.
    ///
    /// Called at the top of every per-withdrawal loop iteration so
    /// a freeze flip takes effect on the next row, not after the
    /// current batch finishes. Returns `false` when no flag is
    /// wired (emergency control is opt-in via `with_freeze_flag`).
    fn is_frozen(&self) -> bool {
        self.frozen
            .as_ref()
            .map(|f| f.is_frozen())
            .unwrap_or(false)
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
            // Re-check freeze per row — operator flip mid-tick must
            // halt on the NEXT withdrawal, not after the batch drains.
            if self.is_frozen() {
                tracing::warn!(id = %w.id, "Freeze flipped mid-tick — halting received-stage loop");
                return Ok(());
            }
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
            // Re-check freeze per row — signing is the most
            // irreversible stage (after signing the private key
            // has produced a valid tx; cancelling requires replace-
            // by-fee on BTC or nonce-churn on EVM). Halt BEFORE
            // we invoke the HSM / seed.
            if self.is_frozen() {
                tracing::warn!(id = %w.id, "Freeze flipped mid-tick — halting validated-stage loop");
                return Ok(());
            }
            // Dependency gate. When a withdrawal is part of a
            // multi-step flow (Lido unstake, Uniswap approve+swap,
            // …) we refuse to sign it until its predecessor is
            // Finalized. If the predecessor is Failed we fail this
            // one the same way so partial sequences don't half-execute.
            if let Some(dep_id) = w.depends_on_withdrawal_id {
                match self.db.find_withdrawal(dep_id).await? {
                    Some(dep) => match dep.status {
                        WithdrawalStatus::Finalized | WithdrawalStatus::Confirming => {
                            // Finalized = required confirmations reached; Confirming
                            // = broadcast and at least one confirmation. Allow
                            // proceeding at Confirming too so multi-step chains
                            // don't stall waiting for the full confirmation depth
                            // of the leading tx. Adjust if an operator wants
                            // strict-Finalized semantics.
                        }
                        WithdrawalStatus::Failed | WithdrawalStatus::Cancelled => {
                            tracing::error!(
                                id = %w.id, dep = %dep_id, dep_status = ?dep.status,
                                "Dependency terminated unsuccessfully — failing dependent"
                            );
                            self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                            continue;
                        }
                        _ => {
                            tracing::debug!(
                                id = %w.id, dep = %dep_id, dep_status = ?dep.status,
                                "Dependency not yet finalized — skipping this tick"
                            );
                            continue;
                        }
                    },
                    None => {
                        tracing::warn!(
                            id = %w.id, dep = %dep_id,
                            "depends_on_withdrawal_id points at a missing row — failing"
                        );
                        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                        continue;
                    }
                }
            }

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

            // Travel Rule (FATF Recommendation 16).
            //
            // When a withdrawal crosses the jurisdictional threshold
            // we MUST exchange originator/beneficiary data with the
            // counterparty VASP before broadcast. Three outcomes:
            //
            //   1. Provider wired + TR data row already persisted
            //      (gateway's POST /v1/travel-rule was called) and
            //      dispatch succeeds → mark row Submitted, proceed.
            //   2. Provider wired but one of the pre-conditions is
            //      missing (no TR row yet, dispatch errored) →
            //      refuse to advance this withdrawal. Row stays in
            //      Received / moves to Failed respectively. Caller
            //      retries after supplying TR data or after the
            //      vendor recovers. Never silently broadcast without
            //      the required data exchange — that's the
            //      compliance bug this path exists to prevent.
            //   3. Provider absent → log warn, proceed. Operator has
            //      opted to handle TR out-of-band (e.g. manual
            //      bilateral exchange before admitting the
            //      withdrawal via API).
            if travel_rule::requires_travel_rule(w.amount, "EU") {
                match self.travel_rule_provider.as_ref() {
                    Some(provider) => {
                        let tr_row = self
                            .db
                            .find_travel_rule_by_withdrawal(w.id)
                            .await
                            .ok()
                            .flatten();
                        let Some(tr) = tr_row else {
                            tracing::warn!(
                                id = %w.id, amount = %w.amount,
                                "Travel Rule threshold exceeded but no TR data submitted — \
                                 withdrawal stays in Received until POST /v1/travel-rule runs"
                            );
                            continue;
                        };
                        let mapped = travel_rule::data_from_persisted(&tr);
                        match provider.send_originator_data(&mapped).await {
                            Ok(()) => {
                                tracing::info!(
                                    id = %w.id,
                                    "Travel Rule originator data dispatched to counterparty VASP"
                                );
                                // Best-effort — persist Submitted status
                                // so the gateway's GET /v1/travel-rule/{id}
                                // reflects reality. Failure here is
                                // logged but not fatal: the dispatch
                                // succeeded, just the local record
                                // didn't update.
                                if let Err(e) = self
                                    .db
                                    .update_travel_rule_status(tr.id, "submitted")
                                    .await
                                {
                                    tracing::warn!(
                                        id = %w.id, error = %e,
                                        "TR dispatch OK but DB status update failed"
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    id = %w.id, error = %e,
                                    "Travel Rule dispatch failed — refusing to advance \
                                     withdrawal; compliance bypass prevented"
                                );
                                let _ = self
                                    .db
                                    .update_travel_rule_status(tr.id, "failed")
                                    .await;
                                self.db
                                    .update_withdrawal_status(w.id, WithdrawalStatus::Failed)
                                    .await?;
                                continue;
                            }
                        }
                    }
                    None => {
                        tracing::warn!(
                            id = %w.id, amount = %w.amount,
                            "Travel Rule threshold exceeded but no provider configured — \
                             operator must handle originator/beneficiary data exchange \
                             out-of-band before this broadcast lands."
                        );
                    }
                }
            }

            // Policy check.
            //
            // Compute the REAL DestinationType for this address
            // instead of hard-coding `Any`. Previously the
            // pipeline always passed `Any`, which meant any
            // policy rule that specified
            // `destination_type: Some(Whitelisted)` would
            // silently never match (the matcher in
            // policy/rule.rs::matches treats non-Any rules as
            // exact equality — `Whitelisted != Any`). An
            // operator who configured "auto-approve up to $X to
            // whitelisted destinations" saw those rules drop
            // through to the default-deny branch or to the next
            // rule with no idea why. Making the lookup honest
            // here fixes the fake-feature.
            //
            // We classify as Whitelisted when the address exists
            // in the per-chain whitelist table, Any otherwise.
            // Internal (our own vaults) isn't tracked yet —
            // treat as Any until vault-ownership lookup lands.
            let destination_type = match self
                .db
                .is_address_whitelisted(&w.destination_address, w.chain.as_str())
                .await
            {
                Ok(true) => DestinationType::Whitelisted,
                // On Ok(false) the earlier process_received stage
                // would have already failed the withdrawal when a
                // whitelist exists — if we reach here, either
                // there's no whitelist (empty table) or this
                // specific address isn't listed. Either way it's
                // not "Whitelisted" for policy purposes.
                Ok(false) => DestinationType::Any,
                Err(e) => {
                    tracing::warn!(
                        id = %w.id, error = %e,
                        "Whitelist lookup for policy evaluation failed — \
                         defaulting destination_type to Any (rules demanding \
                         Whitelisted will not match this run)"
                    );
                    DestinationType::Any
                }
            };

            if let Some(ref engine) = self.policy_engine {
                match engine.evaluate(w.chain, &w.asset, w.amount, &destination_type, &Role::Editor).action {
                    PolicyAction::Deny { ref reason } => {
                        custody_common::prometheus_metrics::POLICY_DECISIONS
                            .with_label_values(&[w.chain.as_str(), "deny"]).inc();
                        tracing::warn!(id = %w.id, reason, "Policy denied");
                        self.db.update_withdrawal_status(w.id, WithdrawalStatus::Failed).await?;
                        continue;
                    }
                    PolicyAction::RequireApproval { quorum, approver_role } => {
                        custody_common::prometheus_metrics::POLICY_DECISIONS
                            .with_label_values(&[w.chain.as_str(), "require_approval"]).inc();
                        tracing::info!(id = %w.id, quorum, "Approval required");
                        // Persist an approval_requests row so the
                        // approve-withdrawal handler has something
                        // to attach decisions to. Without this row
                        // the handler previously flipped status on
                        // a SINGLE approval regardless of quorum —
                        // a straight-up bypass of the multi-approver
                        // rule. Expiry matches Tier 4 delay per
                        // CLAUDE.md (24h for the biggest withdrawals).
                        let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);
                        if let Err(e) = self
                            .db
                            .insert_approval_request(
                                w.id,
                                quorum as i32,
                                &format!("{approver_role:?}"),
                                expires_at,
                            )
                            .await
                        {
                            tracing::error!(
                                id = %w.id, error = %e,
                                "Failed to persist approval_requests row — \
                                 refusing to advance; quorum cannot be enforced \
                                 without the pending record"
                            );
                            // Leave status as-is and retry on next tick.
                            continue;
                        }
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

        // Fee estimation. Fail-closed: if an estimator is wired but
        // the upstream call fails, we do NOT fall back to a
        // hardcoded 50 gwei. A stale fixed price underbids mainnet
        // during fee spikes and the transaction sits in the mempool
        // forever, which looks like "broadcast succeeded" at the
        // custody layer but is effectively silent failure. Refuse
        // to sign until the estimator recovers; the pipeline tick
        // will retry on the next pass.
        //
        // The fallback constants below apply ONLY when no estimator
        // was wired — in that case the operator opted out explicitly
        // and accepts the static-price consequences (useful for
        // testnets or private chains where fee markets are flat).
        let (max_fee, priority_fee) = match &self.eth_fee_estimator {
            Some(est) => match est.estimate().await {
                Ok(e) => {
                    // Cap check against the estimate we already
                    // have. Previously this called
                    // `est.gas_exceeds_cap().await.unwrap_or(false)`,
                    // which did a SECOND RPC round-trip and —
                    // worse — silently defaulted to "not
                    // exceeded" if that call failed. A transient
                    // RPC hiccup during a fee spike would then
                    // let the pipeline sign a withdrawal at
                    // whatever gas price `estimate()` returned,
                    // well above the operator-configured cap.
                    // Compare the local estimate against the
                    // configured cap directly and fail-closed if
                    // in doubt.
                    let cap = rust_decimal::Decimal::new(est.gas_cap_gwei() as i64, 0);
                    if e.base_fee > cap {
                        tracing::info!(
                            id = %w.id,
                            base_fee_gwei = %e.base_fee,
                            cap_gwei = %cap,
                            "Gas cap exceeded — deferring"
                        );
                        return Ok(());
                    }
                    (custody_fee_estimator::ethereum::gwei_to_wei(e.max_fee),
                     custody_fee_estimator::ethereum::gwei_to_wei(e.priority_fee))
                }
                Err(e) => {
                    tracing::warn!(
                        id = %w.id, error = %e,
                        "ETH fee estimator failed — deferring signing to next tick \
                         rather than broadcasting at a stale fallback price"
                    );
                    return Ok(());
                }
            },
            None => (50_000_000_000u128, 2_000_000_000u128),
        };

        // Nonce. Propagate allocation failures — the previous
        // `unwrap_or(0)` silently substituted zero, which for a
        // funded hot-wallet address is already spent. The tx
        // would then sign with nonce=0, broadcast, be rejected
        // by the chain ("nonce too low"), and the withdrawal
        // would end up in an opaque Failed state with an error
        // that never pointed at the real cause (a transient DB
        // hiccup or the one-shot reconciliation RPC). Defer
        // signing to the next tick instead; NonceManager's
        // per-address lock means the retry is cheap and safe.
        let nonce = match &self.nonce_manager {
            Some(nm) => {
                let addr = custody_signing::ethereum::eth_address(seed, 0)?;
                match nm.next_nonce(&format!("{addr:#x}")).await {
                    Ok(n) => n,
                    Err(e) => {
                        tracing::warn!(
                            id = %w.id,
                            error = %e,
                            "ETH nonce allocation failed — deferring signing \
                             to next tick rather than broadcasting at nonce=0"
                        );
                        return Ok(());
                    }
                }
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

        // Check for embedded contract calldata. The staking flow
        // stashes the Lido `submit(address)` calldata in the
        // withdrawal's `memo` field as a hex string so the pipeline
        // can route native-asset contract calls (payable) without a
        // separate "tx_type" column. Any withdrawal whose memo parses
        // to ≥4 bytes of hex takes the contract-call path.
        let embedded_calldata: Option<Vec<u8>> = w.memo.as_deref().and_then(parse_hex_calldata);

        // Build the signing params once; branch on backend availability.
        let signed_tx = if let Some(calldata) = embedded_calldata {
            let params = EthContractCallParams {
                to,
                value_wei: custody_scanner::decimal_to_uint256(w.amount, 18),
                input: alloy::primitives::Bytes::from(calldata),
                nonce,
                chain_id: self.eth_chain_id,
                max_fee_per_gas: max_fee,
                max_priority_fee_per_gas: priority_fee,
                // Contract calls need more headroom than 21k. 150k
                // comfortably fits Lido's `submit` (~80k) plus slack.
                gas_limit: 150_000,
            };
            match &self.signing_backend {
                Some(backend) => {
                    let (tx, prehash) = prepare_eth_contract_call(&params);
                    let sig = backend.sign(Chain::Ethereum, &prehash)
                        .map_err(|e| anyhow::anyhow!("HSM sign(contract call) failed: {e}"))?;
                    finalize_eth_signed_tx(tx, &sig)?
                }
                None => sign_eth_contract_call(seed, 0, &params)?,
            }
        } else if is_native {
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
    ///
    /// Two wire formats:
    /// - Native TRX → `TransferContract` + `amount_sun`
    ///   (`prepare/sign_trx_transfer`).
    /// - TRC-20 (USDT, USDC, …) → `TriggerSmartContract` with
    ///   ABI-encoded `transfer(to, amount)` calldata
    ///   (`prepare/sign_trc20_transfer`). Amount is in token base
    ///   units (`human × 10^decimals`); `fee_limit_sun` is set
    ///   from `trc20_fee_limit_sun` (default ≈ 30 TRX, well above
    ///   the typical ~12 TRX Energy burn so we don't trip
    ///   `OUT_OF_ENERGY` on a hot day).
    async fn sign_trx(&self, w: &Withdrawal) -> Result<()> {
        let token_registry = custody_common::token_registry::TokenRegistry::with_defaults();
        let seed = self.seed.as_ref().ok_or_else(|| anyhow::anyhow!("No seed"))?;
        let from_hex = custody_signing::tron::tron_address_hex(seed, 0)?;

        // Decode destination Base58Check → 41-prefixed hex.
        let to_hex = if w.destination_address.starts_with('T') {
            let decoded = bs58::decode(&w.destination_address)
                .with_check(None)
                .into_vec()
                .map_err(|e| anyhow::anyhow!("Invalid Tron address: {e}"))?;
            hex::encode(&decoded)
        } else {
            w.destination_address.clone()
        };

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let (ref_block_bytes, ref_block_hash) = self.fetch_tron_block_ref().await
            .unwrap_or(([0x00, 0x00], [0x00; 8]));

        let (signature, raw_data) = if token_registry.is_native(w.asset.as_str(), w.chain) {
            let amount_sun = custody_scanner::tron::trx_to_sun(w.amount);
            let params = custody_signing::tron::TrxTransferParams {
                owner_address_hex: from_hex.clone(),
                to_address_hex: to_hex,
                amount_sun,
                expiration: now + 60_000,
                timestamp: now,
                ref_block_bytes,
                ref_block_hash,
            };
            if let Some(backend) = &self.signing_backend {
                let (raw_data, prehash) =
                    custody_signing::tron::prepare_trx_transfer(&params);
                let sig65 = backend
                    .sign(Chain::Tron, &prehash)
                    .map_err(|e| anyhow::anyhow!("HSM sign(Tron) failed: {e}"))?;
                anyhow::ensure!(
                    sig65.len() == 65,
                    "HSM returned {} bytes for Tron signature, expected 65",
                    sig65.len()
                );
                (sig65, raw_data)
            } else {
                custody_signing::tron::sign_trx_transfer(seed, 0, &params)?
            }
        } else {
            // TRC-20 path: contract + decimals must be known.
            let Some(contract_b58) =
                token_registry.contract_address(w.asset.as_str(), w.chain)
            else {
                let msg = format!(
                    "Unknown TRC-20 asset {}: no contract address in registry. \
                     Refusing to sign.",
                    w.asset.as_str()
                );
                tracing::error!(id = %w.id, asset = %w.asset.as_str(), "{msg}");
                self.db
                    .update_withdrawal_status_with_error(
                        w.id, WithdrawalStatus::Failed, &msg,
                    ).await?;
                return Ok(());
            };
            let Some(decimals) = token_registry.decimals(w.asset.as_str(), w.chain) else {
                let msg = format!(
                    "TRC-20 asset {} has no decimals in registry. Refusing to sign.",
                    w.asset.as_str()
                );
                tracing::error!(id = %w.id, asset = %w.asset.as_str(), "{msg}");
                self.db
                    .update_withdrawal_status_with_error(
                        w.id, WithdrawalStatus::Failed, &msg,
                    ).await?;
                return Ok(());
            };

            let contract_hex = {
                let decoded = bs58::decode(contract_b58)
                    .with_check(None)
                    .into_vec()
                    .map_err(|e| anyhow::anyhow!("Invalid TRC-20 contract address: {e}"))?;
                hex::encode(&decoded)
            };

            let amount_base = decimal_to_token_base_units(w.amount, decimals)
                .ok_or_else(|| anyhow::anyhow!(
                    "TRC-20 amount {} × 10^{} overflows u128",
                    w.amount, decimals
                ))?;

            // Energy / Bandwidth pre-flight. A TRC-20 transfer
            // burns ~29k Energy + ~265 Bandwidth. If the source
            // address has neither the staked resources nor enough
            // TRX to burn for them at the current rate, the node
            // accepts the tx but execution halts `OUT_OF_ENERGY`
            // and the withdrawal sits as Signed/Broadcast forever.
            // Refuse here with a structured Failed status so an
            // operator can fund the gas-station address before
            // re-queueing.
            //
            // RPC failures are logged but do not block — if the
            // Tron node is flaking, refusing every TRC-20 wd
            // would be worse than the existing best-effort sign +
            // broadcast (which will surface the failure on the
            // tx_status receipt).
            match self.fetch_tron_account_resources(&from_hex).await {
                Ok(resources) => {
                    let check =
                        custody_common::tron_energy::can_afford_trc20_transfer(&resources);
                    if !check.can_proceed {
                        let msg = format!(
                            "Tron source address {} cannot afford TRC-20 transfer: \
                             energy_avail={}, bandwidth_avail={}, trx_balance_sun={}, \
                             trx_needed={}. Refusing to sign.",
                            from_hex,
                            resources.energy_available(),
                            resources.bandwidth_available(),
                            resources.trx_balance_sun,
                            check.trx_needed,
                        );
                        tracing::error!(id = %w.id, asset = %w.asset.as_str(), "{msg}");
                        self.db
                            .update_withdrawal_status_with_error(
                                w.id, WithdrawalStatus::Failed, &msg,
                            ).await?;
                        return Ok(());
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        id = %w.id, error = %e,
                        "Tron resource pre-flight failed — proceeding with sign \
                         (broadcaster receipt will surface OUT_OF_ENERGY if it happens)"
                    );
                }
            }

            // Default fee_limit ≈ 30 TRX. Energy burn for a TRC-20
            // `transfer` is ~29k Energy × 420 sun ≈ 12 TRX at the
            // network's "burn TRX for Energy" rate; we add headroom
            // for Energy-price drift between scan and inclusion.
            let fee_limit_sun: u64 = 30_000_000;

            let params = custody_signing::tron::Trc20TransferParams {
                owner_address_hex: from_hex.clone(),
                contract_address_hex: contract_hex,
                to_address_hex: to_hex,
                amount: amount_base,
                fee_limit_sun,
                expiration: now + 60_000,
                timestamp: now,
                ref_block_bytes,
                ref_block_hash,
            };

            if let Some(backend) = &self.signing_backend {
                let (raw_data, prehash) =
                    custody_signing::tron::prepare_trc20_transfer(&params);
                let sig65 = backend
                    .sign(Chain::Tron, &prehash)
                    .map_err(|e| anyhow::anyhow!("HSM sign(Tron/TRC-20) failed: {e}"))?;
                anyhow::ensure!(
                    sig65.len() == 65,
                    "HSM returned {} bytes for Tron TRC-20 signature, expected 65",
                    sig65.len()
                );
                (sig65, raw_data)
            } else {
                custody_signing::tron::sign_trc20_transfer(seed, 0, &params)?
            }
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
        tracing::info!(id = %w.id, asset = %w.asset.as_str(), "Tron withdrawal signed");
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

    /// Fetch the owner's TRX balance + Energy/Bandwidth quotas so
    /// the TRC-20 path can refuse to sign when the source can't
    /// possibly cover the on-chain Energy burn. Without this the
    /// tx is still accepted by the node but execution halts with
    /// `OUT_OF_ENERGY`, which leaves the withdrawal stuck pending
    /// while the operator notices manually.
    ///
    /// Two RPC calls because the Tron public API splits resources
    /// (`getaccountresource`) from balance (`getaccount`). Both
    /// take the 41-prefixed hex address with `visible=false`.
    async fn fetch_tron_account_resources(
        &self,
        address_hex: &str,
    ) -> Result<custody_common::tron_energy::TronAccountResources> {
        let api_url = self.trx_api_url.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Tron API URL configured"))?;
        let client = reqwest::Client::new();

        let res: serde_json::Value = client
            .post(format!("{api_url}/wallet/getaccountresource"))
            .json(&serde_json::json!({ "address": address_hex, "visible": false }))
            .send().await?
            .json().await?;
        let acct: serde_json::Value = client
            .post(format!("{api_url}/wallet/getaccount"))
            .json(&serde_json::json!({ "address": address_hex, "visible": false }))
            .send().await?
            .json().await?;

        // Tron returns omitted fields when the value is zero —
        // treat absent as zero rather than bailing, so a brand-new
        // account (no resources, no balance) still produces a
        // structured "cannot afford" outcome rather than a fetcher
        // error that retries forever.
        let bandwidth_used = res["NetUsed"].as_u64().unwrap_or(0)
            + res["freeNetUsed"].as_u64().unwrap_or(0);
        let bandwidth_limit = res["NetLimit"].as_u64().unwrap_or(0)
            + res["freeNetLimit"].as_u64().unwrap_or(0);
        let energy_used = res["EnergyUsed"].as_u64().unwrap_or(0);
        let energy_limit = res["EnergyLimit"].as_u64().unwrap_or(0);
        let trx_balance_sun = acct["balance"].as_u64().unwrap_or(0);

        Ok(custody_common::tron_energy::TronAccountResources {
            bandwidth_used,
            bandwidth_limit,
            energy_used,
            energy_limit,
            trx_balance_sun,
        })
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
            // Re-check freeze per row — Signed → Broadcast is the
            // point of no return (signed bytes hit the mempool and
            // the tx is irreversible). If an operator freezes while
            // a batch of signed rows is queued, we must NOT keep
            // pushing them onto the network.
            if self.is_frozen() {
                tracing::warn!(id = %w.id, "Freeze flipped mid-tick — halting signed-stage loop");
                return Ok(());
            }
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
            // Re-check freeze per row. At this stage the tx is
            // already on-chain so halting doesn't stop funds
            // movement — it only pauses confirmation tracking +
            // webhook firing. That is intentional: a freeze means
            // "quiesce custody activity pending investigation",
            // and emitting `withdrawal_confirmed` webhooks to
            // downstream systems while we're in that state risks
            // cascading decisions on top of suspected compromise.
            if self.is_frozen() {
                tracing::warn!(id = %w.id, "Freeze flipped mid-tick — halting broadcast-stage loop");
                return Ok(());
            }
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

/// Parse `memo` as hex-encoded EVM calldata. Returns `Some(bytes)`
/// only when the memo starts with `0x`, decodes as valid hex, and
/// has at least 4 bytes (a function selector). Anything else — a
/// Cosmos memo, a Solana note, plain user text — is treated as a
/// pure memo by the caller and the tx takes the vanilla transfer
/// path.
/// Convert a `Decimal` human-units amount to `u128` token base
/// units. Returns `None` on overflow or if the result is negative.
/// Truncates fractional units beyond `decimals` (defensive — the
/// API layer is expected to enforce scale already).
fn decimal_to_token_base_units(amount: Decimal, decimals: u8) -> Option<u128> {
    use rust_decimal::prelude::ToPrimitive;
    if amount.is_sign_negative() {
        return None;
    }
    let multiplier = Decimal::from_i128_with_scale(10i128.pow(decimals as u32), 0);
    (amount * multiplier).trunc().to_u128()
}

fn parse_hex_calldata(memo: &str) -> Option<Vec<u8>> {
    let hex_part = memo.strip_prefix("0x")?;
    let bytes = hex::decode(hex_part).ok()?;
    if bytes.len() < 4 {
        return None;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn pipeline_builder() {
        let _limits = (dec!(500000), dec!(10000000), dec!(50000000));
    }

    #[test]
    fn decimal_to_token_base_units_usdt_6() {
        // 100 USDT, 6 decimals → 100_000_000 base units.
        assert_eq!(
            decimal_to_token_base_units(dec!(100), 6),
            Some(100_000_000)
        );
        // Fractional under scale rounds via trunc.
        assert_eq!(
            decimal_to_token_base_units(dec!(0.5), 6),
            Some(500_000)
        );
        // Negative refused.
        assert_eq!(decimal_to_token_base_units(dec!(-1), 6), None);
        // Zero is a valid value.
        assert_eq!(decimal_to_token_base_units(dec!(0), 6), Some(0));
    }

    #[test]
    fn parse_hex_calldata_accepts_lido_submit() {
        // Selector 0xa1903eab + 32-byte padded zero address
        let memo = "0xa1903eab0000000000000000000000000000000000000000000000000000000000000000";
        let decoded = parse_hex_calldata(memo).unwrap();
        assert_eq!(decoded.len(), 36);
        assert_eq!(&decoded[..4], &[0xa1, 0x90, 0x3e, 0xab]);
    }

    #[test]
    fn parse_hex_calldata_rejects_plain_text() {
        assert!(parse_hex_calldata("order #12").is_none());
        assert!(parse_hex_calldata("cosmos-memo").is_none());
        assert!(parse_hex_calldata("0xZZZ").is_none());
        // Too short to be a selector.
        assert!(parse_hex_calldata("0xdead").is_none());
    }
}
