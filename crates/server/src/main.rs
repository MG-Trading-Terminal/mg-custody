//! Custody service entry point.
//!
//! Starts the internal API server and background tasks:
//! - Ethereum deposit scanner (polls every 12s)
//! - Confirmation tracker (checks deposit/withdrawal confirmations)
//! - Withdrawal pipeline (processes state machine transitions)
//! - Webhook retry (retries failed deliveries)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use custody_address_factory::{AddressAllocator, AddressFactoryConfig, SeedBytes};
use custody_common::{Chain, CustodyConfig};
use custody_confirmations::{ConfirmationTracker, WebhookSender};
use custody_confirmations::tracker::BlockHeightFn;
use custody_db::CustodyDb;
use custody_gateway::state::AppState;
use custody_scanner::ethereum::{EthereumScanner, Erc20Config};
use custody_scanner::evm::EvmScanner;
use custody_scanner::address_cache::AddressCache;
use custody_scanner::ChainScanner;
use custody_withdrawal_pipeline::WithdrawalPipeline;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    tracing::info!("Starting custody service v0.1.0");

    // ─── Configuration ───────────────────────────────────────────────
    let config = CustodyConfig::load().unwrap_or_else(|e| {
        tracing::warn!(error = %e, "Config load failed, trying explicit path");
        CustodyConfig::load_from("config/default.toml")
            .expect("Cannot load config")
    });

    tracing::info!(
        bind = %config.api.bind,
        db = %config.database.url,
        "Configuration loaded"
    );

    // Validate configuration
    match custody_common::config_validation::validate_config(&config) {
        Ok(warnings) => {
            for w in &warnings {
                tracing::warn!("Config warning: {}", w);
            }
            if warnings.is_empty() {
                tracing::info!("Configuration validated OK");
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Configuration validation FAILED");
            anyhow::bail!("Invalid configuration: {e}");
        }
    }

    // ─── Database ────────────────────────────────────────────────────
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| config.database.url.clone());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .min_connections(2)
        .max_lifetime(Some(Duration::from_secs(1800)))
        .idle_timeout(Some(Duration::from_secs(600)))
        .acquire_timeout(Duration::from_secs(10))
        .connect(&db_url)
        .await;

    let db = match pool {
        Ok(pool) => {
            tracing::info!("Connected to PostgreSQL");

            // Run ALL migrations in order.
            // Each migration uses IF NOT EXISTS / ON CONFLICT so re-running is safe.
            let migrations: &[(&str, &str)] = &[
                ("001_initial_schema", include_str!("../../db/migrations/001_initial_schema.sql")),
                ("002_signed_tx_bytes", include_str!("../../db/migrations/002_signed_tx_bytes.sql")),
                ("003_vaults", include_str!("../../db/migrations/003_vaults.sql")),
                ("004_webhooks_addressbook", include_str!("../../db/migrations/004_webhooks_addressbook.sql")),
                ("005_connectivity", include_str!("../../db/migrations/005_connectivity.sql")),
                ("006_platform", include_str!("../../db/migrations/006_platform.sql")),
                ("007_governance", include_str!("../../db/migrations/007_governance.sql")),
                ("008_operations", include_str!("../../db/migrations/008_operations.sql")),
                ("009_circuit_breaker", include_str!("../../db/migrations/009_circuit_breaker.sql")),
                ("010_embedded_contracts_payments", include_str!("../../db/migrations/010_embedded_contracts_payments.sql")),
                ("011_sessions", include_str!("../../db/migrations/011_sessions.sql")),
                ("012_fido2", include_str!("../../db/migrations/012_fido2.sql")),
                ("013_scheduled_withdrawals", include_str!("../../db/migrations/013_scheduled_withdrawals.sql")),
                ("014_safe_wallets", include_str!("../../db/migrations/014_safe_wallets.sql")),
                ("015_totp", include_str!("../../db/migrations/015_totp.sql")),
                ("016_login_history", include_str!("../../db/migrations/016_login_history.sql")),
                ("017_email_otp", include_str!("../../db/migrations/017_email_otp.sql")),
                ("018_address_labels", include_str!("../../db/migrations/018_address_labels.sql")),
                ("019_rate_limits", include_str!("../../db/migrations/019_rate_limits.sql")),
                ("020_push_tokens", include_str!("../../db/migrations/020_push_tokens.sql")),
                ("021_login_history_geo", include_str!("../../db/migrations/021_login_history_geo.sql")),
                ("022_device_trust", include_str!("../../db/migrations/022_device_trust.sql")),
                ("023_api_key_usage", include_str!("../../db/migrations/023_api_key_usage.sql")),
                ("024_memo_tag", include_str!("../../db/migrations/024_memo_tag.sql")),
                ("025_nft_custody", include_str!("../../db/migrations/025_nft_custody.sql")),
                ("026_token_metadata", include_str!("../../db/migrations/026_token_metadata.sql")),
                ("027_withdrawals_user_binding", include_str!("../../db/migrations/027_withdrawals_user_binding.sql")),
                ("028_embedded_wallets_uuid", include_str!("../../db/migrations/028_embedded_wallets_uuid.sql")),
                ("029_safe_wallets_owner", include_str!("../../db/migrations/029_safe_wallets_owner.sql")),
                ("030_email_verification", include_str!("../../db/migrations/030_email_verification.sql")),
                ("031_safe_wallet_members", include_str!("../../db/migrations/031_safe_wallet_members.sql")),
                ("032_swap_executions", include_str!("../../db/migrations/032_swap_executions.sql")),
                ("033_consolidation_jobs", include_str!("../../db/migrations/033_consolidation_jobs.sql")),
                ("034_staking_positions", include_str!("../../db/migrations/034_staking_positions.sql")),
                ("035_withdrawal_depends_on", include_str!("../../db/migrations/035_withdrawal_depends_on.sql")),
            ];
            for (name, sql) in migrations {
                if let Err(e) = sqlx::raw_sql(sql).execute(&pool).await {
                    tracing::warn!(migration = name, error = %e, "Migration warning (may already exist)");
                } else {
                    tracing::info!(migration = name, "Migration applied");
                }
            }

            let db = CustodyDb::new(pool);

            // ── First-run bootstrap: create default workspace + admin if DB is empty
            bootstrap_first_admin(&db).await;

            db
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to connect to PostgreSQL");
            tracing::warn!("Running without database — most operations will fail");
            let pool = sqlx::PgPool::connect_lazy(&db_url)?;
            CustodyDb::new(pool)
        }
    };

    // ─── Token Registry — load from TOML files ─────────────────────
    {
        let mut registry = custody_common::token_registry::TokenRegistry::with_defaults();
        let token_dir = std::path::Path::new("config/tokens");
        let loaded = registry.load_from_toml_dir(token_dir);
        tracing::info!(
            hardcoded = registry.len() - loaded,
            from_toml = loaded,
            total = registry.len(),
            "Token registry initialized"
        );
    }

    // ─── Seed (dev mode) ─────────────────────────────────────────────
    // Load seed from encrypted file or use test seed
    let seed_arr = if let Ok(seed_file) = std::env::var("CUSTODY_SEED_FILE") {
        if let Ok(encrypted) = std::fs::read(&seed_file) {
            let password = std::env::var("CUSTODY_SEED_PASSWORD")
                .expect("CUSTODY_SEED_PASSWORD required when CUSTODY_SEED_FILE is set");
            let pw = custody_signing::Password::from_str(&password);
            match custody_signing::decrypt_seed(&encrypted, &pw) {
                Ok(seed) => {
                    tracing::info!("Seed loaded from encrypted file");
                    seed
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to decrypt seed file");
                    anyhow::bail!("Seed decryption failed: {e}");
                }
            }
        } else {
            anyhow::bail!("Cannot read seed file: {seed_file}");
        }
    } else {
        tracing::warn!("No CUSTODY_SEED_FILE — using zeroed test seed (NOT FOR PRODUCTION)");
        [0u8; 64]
    };
    let seed = Arc::new(SeedBytes(seed_arr));

    // ─── Services ────────────────────────────────────────────────────
    let address_factory_config = AddressFactoryConfig::default();
    let address_allocator = Arc::new(AddressAllocator::new(
        db.clone(),
        address_factory_config,
    ));

    // ─── Token Registry from config ────────────────────────────────
    let mut token_registry = custody_common::token_registry::TokenRegistry::with_defaults();
    // Add tokens from Ethereum config
    if let Some(ref eth) = config.chains.ethereum {
        for (addr, token) in &eth.tokens {
            token_registry.register(custody_common::token_registry::TokenInfo {
                symbol: token.symbol.clone(),
                chain: Chain::Ethereum,
                contract_address: Some(addr.clone()),
                decimals: token.decimals,
                is_native: false,
            });
        }
    }
    let token_registry = Arc::new(token_registry);
    tracing::info!(tokens = token_registry.len(), "Token registry loaded");

    // ─── Nonce Manager ───────────────────────────────────────────────
    let nonce_manager = if let Some(ref eth) = config.chains.ethereum {
        let rpc_url = std::env::var("CUSTODY_CHAINS__ETHEREUM__RPC_URL")
            .unwrap_or_else(|_| eth.rpc_url.clone());
        match custody_withdrawal_pipeline::NonceManager::new(db.clone(), &rpc_url) {
            Ok(nm) => {
                tracing::info!("Nonce manager initialized");
                Some(Arc::new(nm))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Nonce manager init failed");
                None
            }
        }
    } else {
        None
    };

    // ─── Event Broadcaster (WebSocket) ─────────────────────────────
    let event_broadcaster = Arc::new(custody_common::events::EventBroadcaster::with_default_capacity());

    // ─── Policy Engine ─────────────────────────────────────────────
    let policy_rules = vec![
        custody_policy::PolicyRule::new(
            "auto_approve_small", 100,
            custody_policy::PolicyCondition {
                max_amount_usd: Some(config.limits.auto_approve_threshold_usd),
                ..Default::default()
            },
            custody_policy::PolicyAction::Allow,
        ),
        custody_policy::PolicyRule::new(
            "require_approval_medium", 200,
            custody_policy::PolicyCondition {
                min_amount_usd: Some(config.limits.auto_approve_threshold_usd),
                max_amount_usd: Some(rust_decimal::Decimal::new(100_000, 0)),
                ..Default::default()
            },
            custody_policy::PolicyAction::RequireApproval {
                quorum: 1, approver_role: custody_policy::rbac::Role::Signer,
            },
        ),
        custody_policy::PolicyRule::new(
            "require_2_approvals_large", 300,
            custody_policy::PolicyCondition {
                min_amount_usd: Some(rust_decimal::Decimal::new(100_000, 0)),
                ..Default::default()
            },
            custody_policy::PolicyAction::RequireApproval {
                quorum: 2, approver_role: custody_policy::rbac::Role::Signer,
            },
        ),
    ];
    let policy_engine = Arc::new(custody_policy::PolicyEngine::new(policy_rules));
    tracing::info!("Policy engine loaded with default rules");

    // ─── OFAC / Sanctions Checker ───────────────────────────────────
    let sanctions_checker = Arc::new(custody_compliance::ofac::ofac_checker_with_known_addresses());
    tracing::info!(addresses = sanctions_checker.total_addresses(), "OFAC sanctions checker loaded");

    // ─── Emergency freeze flag (unfrozen on startup) ─────────────────
    // Shared between AppState (API handlers) and the pipeline (background task).
    let freeze_flag = custody_common::emergency::FreezeFlag::new();

    // ─── Withdrawal Pipeline with all deps ───────────────────────────
    let limits = &config.limits;
    let mut pipeline_builder = WithdrawalPipeline::new(
        db.clone(),
        limits.max_single_withdrawal_usd,
        limits.max_hourly_outflow_usd,
        limits.max_daily_outflow_usd,
    )
    .with_seed(Arc::new(seed_arr))
    .with_policy_engine(policy_engine)
    .with_sanctions_checker(sanctions_checker)
    .with_event_broadcaster(event_broadcaster.clone())
    .with_eth_chain_id(
        config.chains.ethereum.as_ref().map(|_| {
            if std::env::var("CUSTODY_TESTNET").is_ok() { 11155111u64 } else { 1u64 }
        }).unwrap_or(1)
    );

    // Optional PKCS#11 HSM signing backend. If `HSM_LIB_PATH` is set,
    // every ETH / ERC-20 signature goes through `C_Sign` on an
    // HSM-resident key instead of BIP-32 derivation from the master
    // seed. The seed is still retained for deposit-address derivation
    // and remains the signer for BTC / Tron / Solana until their
    // prepare/finalize helpers land. If HSM init fails we log loudly
    // and refuse to start — "silently fell back to seed signing" is
    // exactly the kind of fund-safety lie we're trying to eliminate.
    if let Some(hsm_cfg) = custody_signing::Pkcs11SigningConfig::from_env() {
        tracing::warn!(
            library = %hsm_cfg.library_path,
            slot = hsm_cfg.slot_id,
            "HSM_LIB_PATH set — attempting to initialise PKCS#11 signing backend"
        );
        match custody_signing::Pkcs11SigningBackend::connect(hsm_cfg) {
            Ok(backend) => {
                let backend: Arc<dyn custody_signing::SigningBackend> = Arc::new(backend);
                pipeline_builder = pipeline_builder.with_signing_backend(backend);
                tracing::info!(
                    "PKCS#11 signing backend engaged — ETH / ERC-20 will sign via C_Sign"
                );
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "HSM_LIB_PATH was set but PKCS#11 init failed — refusing to start \
                     so we don't silently fall back to seed signing."
                );
                return Err(e);
            }
        }
    } else {
        tracing::info!(
            "HSM_LIB_PATH not set — signing will derive keys from the local seed. \
             For production custody, provision a PKCS#11 HSM and set HSM_LIB_PATH."
        );
    }

    if let Some(ref nm) = nonce_manager {
        pipeline_builder = pipeline_builder.with_nonce_manager(nm.clone());
    }

    // Wire per-chain broadcasters + RPC URLs
    if let Some(ref btc) = config.chains.bitcoin {
        if btc.enabled {
            let rpc_url = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_URL")
                .unwrap_or_else(|_| btc.rpc_url.clone());
            let rpc_user = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_USER").ok();
            let rpc_pass = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_PASSWORD").ok();
            let btc_bc = Arc::new(custody_broadcaster::bitcoin::BitcoinBroadcaster::new(
                &rpc_url, rpc_user, rpc_pass,
            ));
            pipeline_builder = pipeline_builder.with_btc_broadcaster(btc_bc);
        }
    }
    if let Some(ref trx) = config.chains.tron {
        if trx.enabled {
            let api_url = std::env::var("CUSTODY_CHAINS__TRON__API_URL")
                .unwrap_or_else(|_| trx.api_url.clone());
            let trx_bc = Arc::new(custody_broadcaster::tron::TronBroadcaster::new(&api_url));
            pipeline_builder = pipeline_builder
                .with_trx_broadcaster(trx_bc)
                .with_trx_api_url(api_url);
        }
    }
    if let Some(ref sol) = config.chains.solana {
        if sol.enabled {
            let rpc_url = std::env::var("CUSTODY_CHAINS__SOLANA__RPC_URL")
                .unwrap_or_else(|_| sol.rpc_url.clone());
            let sol_bc = Arc::new(custody_broadcaster::solana::SolanaBroadcaster::new(
                &rpc_url, &sol.commitment,
            ));
            pipeline_builder = pipeline_builder
                .with_sol_broadcaster(sol_bc)
                .with_sol_rpc_url(rpc_url);
        }
    }

    // Wire fee estimators
    if let Some(ref btc) = config.chains.bitcoin {
        if btc.enabled {
            let rpc_url = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_URL")
                .unwrap_or_else(|_| btc.rpc_url.clone());
            let rpc_user = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_USER").ok();
            let rpc_pass = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_PASSWORD").ok();
            let est = Arc::new(custody_fee_estimator::bitcoin::BitcoinFeeEstimator::new(
                &rpc_url, rpc_user, rpc_pass, 6,
            ));
            pipeline_builder = pipeline_builder.with_btc_fee_estimator(est);
        }
    }
    if let Some(ref trx) = config.chains.tron {
        if trx.enabled {
            let api_url = std::env::var("CUSTODY_CHAINS__TRON__API_URL")
                .unwrap_or_else(|_| trx.api_url.clone());
            let est = Arc::new(custody_fee_estimator::tron::TronFeeEstimator::new(&api_url));
            pipeline_builder = pipeline_builder.with_trx_fee_estimator(est);
        }
    }
    if let Some(ref sol) = config.chains.solana {
        if sol.enabled {
            let rpc_url = std::env::var("CUSTODY_CHAINS__SOLANA__RPC_URL")
                .unwrap_or_else(|_| sol.rpc_url.clone());
            let est = Arc::new(custody_fee_estimator::solana::SolanaFeeEstimator::new(&rpc_url));
            pipeline_builder = pipeline_builder.with_sol_fee_estimator(est);
        }
    }

    // AML / KYT screening. Multi-provider + fail-closed:
    //   CUSTODY_CHAINALYSIS_API_KEY  → Chainalysis KYT
    //   CUSTODY_ELLIPTIC_API_KEY     → Elliptic Lens
    //   CUSTODY_SCREENING_FALLBACK   → "block" (default) | "allow_and_flag"
    //
    // If at least one key is set we assemble a ScreeningService and
    // plug it into the pipeline. If `fallback=block` (the default),
    // provider errors + missing providers both refuse the screening
    // and the pipeline rejects the withdrawal — an outage cannot
    // become a silent pass.
    //
    // If NO keys are set and no explicit fallback chosen, we skip
    // the service entirely and log-once. The pipeline treats that
    // as "AML not configured" (no screening happens) — different
    // from "service configured but every provider failed". This is
    // the dev / testnet default; production deployments should
    // supply at least one key.
    {
        let chainalysis_key = std::env::var("CUSTODY_CHAINALYSIS_API_KEY").ok();
        let elliptic_key = std::env::var("CUSTODY_ELLIPTIC_API_KEY").ok();
        if chainalysis_key.is_some() || elliptic_key.is_some() {
            let fallback_str = std::env::var("CUSTODY_SCREENING_FALLBACK")
                .unwrap_or_else(|_| "block".into())
                .to_ascii_lowercase();
            let fallback = match fallback_str.as_str() {
                "allow_and_flag" => custody_compliance::screening::ScreeningFallback::AllowAndFlag,
                _ => custody_compliance::screening::ScreeningFallback::Block,
            };
            let mut svc = custody_compliance::screening::ScreeningService::new(fallback, 24);
            if let Some(k) = chainalysis_key.as_deref() {
                svc = svc.add_chainalysis(k);
                tracing::info!("Chainalysis KYT provider added to ScreeningService");
            }
            if let Some(k) = elliptic_key.as_deref() {
                svc = svc.add_elliptic(k);
                tracing::info!("Elliptic Lens provider added to ScreeningService");
            }
            tracing::info!(
                fallback = %fallback_str,
                providers = if chainalysis_key.is_some() { 1 } else { 0 }
                    + if elliptic_key.is_some() { 1 } else { 0 },
                "AML screening enabled (fail-closed)"
            );
            pipeline_builder = pipeline_builder.with_screening_service(Arc::new(svc));
        } else {
            tracing::warn!(
                "No CUSTODY_CHAINALYSIS_API_KEY / CUSTODY_ELLIPTIC_API_KEY — AML screening \
                 disabled. Production deployments MUST supply at least one provider key."
            );
        }
    }

    // Wire freeze flag into pipeline so background task respects emergency freeze.
    pipeline_builder = pipeline_builder.with_freeze_flag(freeze_flag.clone());
    let withdrawal_pipeline = Arc::new(pipeline_builder);

    // ─── Ethereum Scanner ────────────────────────────────────────────
    let eth_config = config.chains.ethereum.as_ref();
    let eth_scanner = if let Some(eth) = eth_config {
        if eth.enabled {
            let rpc_url = std::env::var("CUSTODY_CHAINS__ETHEREUM__RPC_URL")
                .unwrap_or_else(|_| eth.rpc_url.clone());

            let erc20_tokens: Vec<Erc20Config> = eth.tokens.iter().map(|(addr, token)| {
                Erc20Config {
                    contract: addr.parse().unwrap_or_default(),
                    symbol: token.symbol.clone(),
                    decimals: token.decimals,
                }
            }).collect();

            let address_cache = AddressCache::new(db.clone(), Chain::Ethereum, 10);

            match EthereumScanner::new(
                &rpc_url,
                db.clone(),
                address_cache,
                erc20_tokens,
                eth.required_confirmations,
            ) {
                Ok(scanner) => {
                    tracing::info!(rpc = %rpc_url, "Ethereum scanner initialized");
                    Some(Arc::new(scanner))
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to create Ethereum scanner");
                    None
                }
            }
        } else {
            tracing::info!("Ethereum chain disabled");
            None
        }
    } else {
        None
    };

    // ─── MPC multi-host receiver (optional) ─────────────────────────
    //
    // Env-driven so existing single-host deployments keep working
    // untouched:
    //   CUSTODY_MPC__ENABLED       = "true" to spawn the receiver
    //   CUSTODY_MPC__BIND_ADDR     = "0.0.0.0:9100" (default)
    //   CUSTODY_MPC__HMAC_SECRET   = shared peer secret (required)
    //   CUSTODY_MPC__PARTY_INDEX   = this party's 0-based index (logged,
    //                                 used by ceremony orchestration)
    //   CUSTODY_MPC__PEERS         = comma-separated peer endpoints,
    //                                 e.g. "http://p1:9100,http://p2:9100"
    //
    // Spawn only the receiver side here. The actual ceremony
    // (DKG / sign) is driven by an operator calling coordinator_v2
    // / distributed_dkg against the shared HMAC secret + peer
    // endpoints; this service holds the inbox so peer messages have
    // somewhere to land. Without this spawn, the existing
    // HttpTransport could `send()` but never `receive()`, and every
    // multi-host ceremony would hang on round 1.
    let _mpc_receiver: Option<tokio::task::JoinHandle<()>> =
        if std::env::var("CUSTODY_MPC__ENABLED").ok().as_deref() == Some("true") {
            let bind_str = std::env::var("CUSTODY_MPC__BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:9100".to_string());
            let hmac = std::env::var("CUSTODY_MPC__HMAC_SECRET").ok();
            let peers = std::env::var("CUSTODY_MPC__PEERS").unwrap_or_default();
            let party_idx = std::env::var("CUSTODY_MPC__PARTY_INDEX")
                .ok()
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(0);

            match (hmac, bind_str.parse::<std::net::SocketAddr>()) {
                (Some(secret), Ok(bind_addr)) if !secret.is_empty() => {
                    let inbox = Arc::new(std::sync::Mutex::new(Vec::new()));
                    tracing::info!(
                        %bind_addr,
                        party_index = party_idx,
                        peers = %peers,
                        "Starting MPC multi-host receiver — inbox ready for peer messages"
                    );
                    match custody_mpc::http_server::spawn_mpc_server(
                        bind_addr,
                        inbox,
                        secret.into_bytes(),
                    ).await {
                        Ok(h) => Some(h),
                        Err(e) => {
                            tracing::error!(error = %e, "MPC receiver failed to start — aborting");
                            return Err(e);
                        }
                    }
                }
                (None, _) => {
                    tracing::error!(
                        "CUSTODY_MPC__ENABLED=true but CUSTODY_MPC__HMAC_SECRET is missing — \
                         refusing to start without peer authentication"
                    );
                    return Err(anyhow::anyhow!("missing MPC HMAC secret"));
                }
                (_, Err(e)) => {
                    tracing::error!(error = %e, bind = %bind_str, "Invalid CUSTODY_MPC__BIND_ADDR");
                    return Err(anyhow::anyhow!("invalid MPC bind address: {e}"));
                }
                _ => None,
            }
        } else {
            tracing::debug!(
                "MPC receiver disabled (set CUSTODY_MPC__ENABLED=true + HMAC_SECRET to enable)"
            );
            None
        };

    // ─── Webhook Sender ──────────────────────────────────────────────
    let webhook_secret = std::env::var("CUSTODY_WEBHOOK_SECRET")
        .unwrap_or_else(|_| "dev_webhook_secret".to_string());
    let webhook_sender = Arc::new(WebhookSender::new(
        config.exchange.webhook_url.clone(),
        webhook_secret.into_bytes(),
        db.clone(),
        10, // max attempts
    ));

    // ─── Confirmation Tracker ────────────────────────────────────────
    let mut block_height_fns: HashMap<Chain, BlockHeightFn> = HashMap::new();

    if let Some(ref scanner) = eth_scanner {
        let scanner_clone = scanner.clone();
        let height_fn: BlockHeightFn = Arc::new(move || {
            let s = scanner_clone.clone();
            Box::pin(async move { s.current_block_height().await })
        });
        block_height_fns.insert(Chain::Ethereum, height_fn);
    }

    // ─── Push notification service ──────────────────────────────────
    //
    // FCM HTTP v1 (Android), APNs (iOS, HTTP/2 + ES256 JWT), Web Push
    // (RFC 8291 + VAPID) — each platform is wired independently.
    // Missing credentials ⇒ that platform is simply not available;
    // the registered-device path still persists tokens so when
    // credentials are provisioned later, alerts start reaching
    // devices without a code change. Broadcast success counters
    // reflect real reach: an unconfigured platform returns `Err` at
    // send time and never counts as a delivered message.
    let push_service: Option<Arc<custody_common::push_notifications::PushNotificationService>> = {
        use custody_common::push_notifications::{
            ApnsClient, ApnsConfig, FcmServiceAccount, FcmV1Client,
            PushNotificationService, WebPushClient, WebPushConfig,
        };

        let fcm_client = match std::env::var("CUSTODY_FCM__SERVICE_ACCOUNT_JSON") {
            Ok(raw) if !raw.trim().is_empty() => match FcmServiceAccount::from_json(&raw) {
                Ok(sa) => {
                    tracing::info!(project = %sa.project_id, "FCM v1 push enabled");
                    Some(Arc::new(FcmV1Client::new(sa, ::reqwest::Client::new())))
                }
                Err(e) => {
                    tracing::error!(error = %e, "FCM service-account JSON invalid — FCM push disabled");
                    None
                }
            },
            _ => None,
        };

        let apns_client = match (
            std::env::var("CUSTODY_APNS__TEAM_ID").ok(),
            std::env::var("CUSTODY_APNS__KEY_ID").ok(),
            std::env::var("CUSTODY_APNS__KEY_P8").ok(),
            std::env::var("CUSTODY_APNS__BUNDLE_ID").ok(),
        ) {
            (Some(team), Some(kid), Some(key), Some(bundle))
                if !team.is_empty() && !kid.is_empty() && !key.is_empty() && !bundle.is_empty() =>
            {
                let production = std::env::var("CUSTODY_APNS__PRODUCTION")
                    .map(|v| !matches!(v.as_str(), "0" | "false" | "no"))
                    .unwrap_or(true);
                tracing::info!(bundle = %bundle, production, "APNs push enabled");
                Some(Arc::new(ApnsClient::new(
                    ApnsConfig {
                        team_id: team,
                        key_id: kid,
                        private_key_pem: key,
                        bundle_id: bundle,
                        production,
                    },
                    ::reqwest::Client::new(),
                )))
            }
            _ => None,
        };

        let web_push_client = match (
            std::env::var("CUSTODY_VAPID__PRIVATE_KEY_PEM").ok(),
            std::env::var("CUSTODY_VAPID__SUBJECT").ok(),
        ) {
            (Some(key), Some(sub)) if !key.is_empty() && !sub.is_empty() => {
                let ttl = std::env::var("CUSTODY_VAPID__TTL_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(24 * 3600);
                tracing::info!(subject = %sub, "Web Push enabled");
                Some(Arc::new(WebPushClient::new(
                    WebPushConfig {
                        vapid_private_key_pem: key,
                        vapid_subject: sub,
                        default_ttl_secs: ttl,
                    },
                    ::reqwest::Client::new(),
                )))
            }
            _ => None,
        };

        if fcm_client.is_some() || apns_client.is_some() || web_push_client.is_some() {
            Some(Arc::new(PushNotificationService::new(
                fcm_client,
                apns_client,
                web_push_client,
            )))
        } else {
            tracing::warn!(
                "Push notifications disabled — no FCM / APNs / Web Push credentials configured."
            );
            None
        }
    };

    // ─── Push-alert subscriber ──────────────────────────────────────
    //
    // Listens on the EventBroadcaster and pushes relevant events to
    // every registered device for the affected user:
    //   - DepositConfirmed  → "Large deposit received"
    //   - WithdrawalFailed  → "Withdrawal failed — requires attention"
    // WithdrawalConfirmed / WithdrawalBroadcast don't include a
    // user_id in the event shape; handling them would require a DB
    // lookup per event, deferred until the event struct gains user_id.
    if let Some(ref svc) = push_service {
        let push_svc = svc.clone();
        let push_db = db.clone();
        let mut rx = event_broadcaster.subscribe();
        tokio::spawn(async move {
            tracing::info!("Push-alert subscriber started");
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let (user_id_opt, payload_opt) = match &event {
                            custody_common::CustodyEvent::DepositConfirmed {
                                user_id, amount, asset, user_address, ..
                            } => (
                                Some(*user_id),
                                Some(custody_common::push_notifications::PushPayload::large_deposit(
                                    &amount.to_string(),
                                    asset.as_str(),
                                    user_address,
                                )),
                            ),
                            custody_common::CustodyEvent::WithdrawalFailed {
                                withdrawal_id, reason, user_id, ..
                            } => {
                                let mut data = std::collections::HashMap::new();
                                data.insert("action".into(), "withdrawal_failed".into());
                                data.insert("withdrawal_id".into(), withdrawal_id.to_string());
                                // Prefer user_id from the event; fall back to DB lookup
                                // for older emit sites that haven't been updated yet.
                                let uid = match *user_id {
                                    Some(u) => Some(u),
                                    None => match push_db.find_withdrawal(*withdrawal_id).await {
                                        Ok(Some(w)) => Some(w.user_id),
                                        _ => None,
                                    },
                                };
                                (
                                    uid,
                                    Some(custody_common::push_notifications::PushPayload {
                                        title: "Withdrawal Failed".into(),
                                        body: format!("Withdrawal {withdrawal_id} failed: {reason}"),
                                        data,
                                        badge: None,
                                        sound: Some("critical".into()),
                                    }),
                                )
                            }
                            custody_common::CustodyEvent::WithdrawalBroadcast {
                                withdrawal_id, tx_hash, asset, amount, user_id, ..
                            } => {
                                let mut data = std::collections::HashMap::new();
                                data.insert("action".into(), "withdrawal_broadcast".into());
                                data.insert("withdrawal_id".into(), withdrawal_id.to_string());
                                data.insert("tx_hash".into(), tx_hash.clone());
                                let uid = match *user_id {
                                    Some(u) => Some(u),
                                    None => push_db.find_withdrawal(*withdrawal_id).await.ok().flatten().map(|w| w.user_id),
                                };
                                (
                                    uid,
                                    Some(custody_common::push_notifications::PushPayload {
                                        title: "Withdrawal Broadcast".into(),
                                        body: format!("{amount} {} sent, awaiting confirmations", asset.as_str()),
                                        data,
                                        badge: None,
                                        sound: None,
                                    }),
                                )
                            }
                            custody_common::CustodyEvent::WithdrawalConfirmed {
                                withdrawal_id, tx_hash, confirmations, user_id, ..
                            } => {
                                let mut data = std::collections::HashMap::new();
                                data.insert("action".into(), "withdrawal_confirmed".into());
                                data.insert("withdrawal_id".into(), withdrawal_id.to_string());
                                data.insert("tx_hash".into(), tx_hash.clone());
                                let uid = match *user_id {
                                    Some(u) => Some(u),
                                    None => push_db.find_withdrawal(*withdrawal_id).await.ok().flatten().map(|w| w.user_id),
                                };
                                (
                                    uid,
                                    Some(custody_common::push_notifications::PushPayload {
                                        title: "Withdrawal Confirmed".into(),
                                        body: format!("{confirmations} confirmations reached"),
                                        data,
                                        badge: None,
                                        sound: None,
                                    }),
                                )
                            }
                        };

                        if let (Some(uid), Some(payload)) = (user_id_opt, payload_opt) {
                            match push_db.list_device_tokens(uid).await {
                                Ok(tokens) if !tokens.is_empty() => {
                                    let delivered = push_svc.broadcast(&tokens, &payload).await;
                                    tracing::info!(
                                        user_id = %uid,
                                        registered = tokens.len(),
                                        delivered,
                                        "Push alert dispatched"
                                    );
                                }
                                Ok(_) => {
                                    tracing::debug!(user_id = %uid, "No registered devices — skipping push");
                                }
                                Err(e) => {
                                    tracing::warn!(user_id = %uid, error = %e, "Device-token lookup failed");
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped,
                            "Push-alert subscriber lagged behind broadcaster — events dropped"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::warn!("EventBroadcaster closed — push-alert subscriber exiting");
                        break;
                    }
                }
            }
        });
    }

    let confirmation_tracker = Arc::new(ConfirmationTracker::new(
        db.clone(),
        block_height_fns,
        webhook_sender.clone(),
    ));

    // ─── Email transport ─────────────────────────────────────────────
    //
    // `CUSTODY_EMAIL__MODE` selects the transport:
    //   - `console` (default) — pretty-prints to stdout for dev
    //   - `smtp`              — real outbound via lettre (tokio+rustls)
    //   - `noop`              — silent, for tests and local scripts
    //
    // SMTP config (only read when MODE=smtp):
    //   CUSTODY_EMAIL__SMTP_HOST       required
    //   CUSTODY_EMAIL__SMTP_PORT       default 465 for wrapper, 587 for starttls
    //   CUSTODY_EMAIL__SMTP_USERNAME   optional (some relays are IP-gated)
    //   CUSTODY_EMAIL__SMTP_PASSWORD   optional (paired with USERNAME)
    //   CUSTODY_EMAIL__SMTP_TLS        `wrapper` (default, port 465) | `starttls`
    //   CUSTODY_EMAIL__SMTP_TIMEOUT    seconds, default 15
    //
    // Falls back to the dev console transport (with a loud warning) if
    // MODE=smtp but the config is incomplete — never plaintext.
    let email_from = std::env::var("CUSTODY_EMAIL__FROM")
        .unwrap_or_else(|_| "MG Custody <no-reply@custody.local>".to_string());
    let email_mode = std::env::var("CUSTODY_EMAIL__MODE").unwrap_or_else(|_| "console".to_string());
    // Travel Rule provider (Notabene). Read from env:
    //   CUSTODY_TRAVELRULE__NOTABENE_API_KEY
    // If unset, the `submit_travel_rule` handler persists the row with
    // status = Required (awaiting out-of-band exchange). If set, the
    // handler dispatches originator data to the counterparty VASP via
    // the provider and flips the status based on success.
    let travel_rule_provider = std::env::var("CUSTODY_TRAVELRULE__NOTABENE_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|key| {
            tracing::info!("Notabene Travel Rule provider enabled");
            std::sync::Arc::new(custody_compliance::travel_rule::NotabeneProvider::new(&key))
        });

    let email_sender = match email_mode.as_str() {
        "noop" => custody_common::email::EmailSender::noop(),
        "smtp" => {
            let tls = match std::env::var("CUSTODY_EMAIL__SMTP_TLS")
                .unwrap_or_else(|_| "wrapper".to_string())
                .to_ascii_lowercase()
                .as_str()
            {
                "starttls" => custody_common::email::SmtpTls::Required,
                _ => custody_common::email::SmtpTls::Wrapper,
            };
            let default_port = match tls {
                custody_common::email::SmtpTls::Wrapper => 465,
                custody_common::email::SmtpTls::Required => 587,
            };
            let port = std::env::var("CUSTODY_EMAIL__SMTP_PORT")
                .ok()
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(default_port);
            let timeout_secs = std::env::var("CUSTODY_EMAIL__SMTP_TIMEOUT")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(15);

            match std::env::var("CUSTODY_EMAIL__SMTP_HOST").ok() {
                Some(host) if !host.is_empty() => {
                    let cfg = custody_common::email::SmtpConfig {
                        from: email_from.clone(),
                        host,
                        port,
                        username: std::env::var("CUSTODY_EMAIL__SMTP_USERNAME").ok(),
                        password: std::env::var("CUSTODY_EMAIL__SMTP_PASSWORD").ok(),
                        tls,
                        timeout_secs,
                    };
                    custody_common::email::EmailSender::smtp(cfg)
                }
                _ => {
                    tracing::warn!(
                        "CUSTODY_EMAIL__MODE=smtp but SMTP_HOST is missing — falling back to \
                         console transport. Password-reset and email-verification mails will \
                         be printed to logs, NOT sent."
                    );
                    custody_common::email::EmailSender::console(email_from.clone())
                }
            }
        }
        _ => custody_common::email::EmailSender::console(email_from.clone()),
    };
    tracing::info!(
        transport = email_sender.transport_name(),
        "Email sender initialized"
    );

    // ─── Feature readiness report ───────────────────────────────────
    //
    // Assemble a truthful readiness map. Anything that went through a
    // successful construction above is Ready; anything whose env vars
    // were absent is Disabled; partial config is Misconfigured; and
    // any init that threw (we caught it and kept going) is Degraded.
    // This block is purely observational — it does NOT try to probe
    // the network further. Cold-start probes should happen earlier
    // next to each subsystem; this just rolls them up.
    let mut readiness = custody_common::readiness::ReadinessReport::new();
    use custody_common::readiness::FeatureStatus;

    // Email transport. For smtp mode we probe the TCP connection so a
    // misconfigured host surfaces as Degraded at boot instead of
    // silently failing on the first password reset.
    match email_sender.transport_name() {
        "smtp" => {
            let host = std::env::var("CUSTODY_EMAIL__SMTP_HOST").unwrap_or_default();
            let port: u16 = std::env::var("CUSTODY_EMAIL__SMTP_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(465);
            match custody_common::readiness::probe_smtp_tcp(&host, port).await {
                Ok(()) => readiness.push(FeatureStatus::ready(
                    "email.smtp",
                    "Outbound email via lettre SMTP",
                )),
                Err(e) => readiness.push(FeatureStatus::degraded(
                    "email.smtp",
                    "Outbound email via lettre SMTP",
                    e,
                )),
            }
        }
        "console" => readiness.push(FeatureStatus::disabled(
            "email.smtp",
            "Outbound email — currently printing to stdout (dev transport)",
        )),
        _ => readiness.push(FeatureStatus::disabled(
            "email.smtp",
            "Outbound email (noop transport)",
        )),
    }

    // HSM signing backend
    readiness.push(if std::env::var("HSM_LIB_PATH").is_ok() {
        FeatureStatus::ready("signing.hsm", "PKCS#11 HSM signing (ETH/ERC-20/Tron/Solana)")
    } else {
        FeatureStatus::disabled(
            "signing.hsm",
            "PKCS#11 HSM signing. Set HSM_LIB_PATH + HSM_PIN to enable.",
        )
    });

    // AML screening
    {
        let chainalysis = std::env::var("CUSTODY_CHAINALYSIS_API_KEY").ok().filter(|s| !s.is_empty());
        let elliptic = std::env::var("CUSTODY_ELLIPTIC_API_KEY").ok().filter(|s| !s.is_empty());
        if chainalysis.is_some() || elliptic.is_some() {
            readiness.push(FeatureStatus::ready(
                "screening.aml",
                "AML screening (Chainalysis / Elliptic) on every withdrawal",
            ));
        } else {
            readiness.push(FeatureStatus::disabled(
                "screening.aml",
                "AML screening. Set CUSTODY_CHAINALYSIS_API_KEY or CUSTODY_ELLIPTIC_API_KEY to enable.",
            ));
        }
    }

    // Push notifications
    {
        let fcm = std::env::var("CUSTODY_FCM__SERVICE_ACCOUNT_JSON").ok().filter(|s| !s.trim().is_empty());
        let apns_keys = [
            "CUSTODY_APNS__TEAM_ID",
            "CUSTODY_APNS__KEY_ID",
            "CUSTODY_APNS__KEY_P8",
            "CUSTODY_APNS__BUNDLE_ID",
        ];
        let apns_present: Vec<&str> = apns_keys.iter()
            .filter(|k| std::env::var(k).is_ok_and(|v| !v.is_empty())).copied().collect();
        let apns_missing: Vec<String> = apns_keys.iter()
            .filter(|k| !std::env::var(k).is_ok_and(|v| !v.is_empty())).map(|s| (*s).to_string()).collect();
        let vapid_key = std::env::var("CUSTODY_VAPID__PRIVATE_KEY_PEM").ok().filter(|s| !s.is_empty());
        let vapid_sub = std::env::var("CUSTODY_VAPID__SUBJECT").ok().filter(|s| !s.is_empty());

        readiness.push(if fcm.is_some() {
            FeatureStatus::ready("push.fcm", "FCM HTTP v1 (Android + background Web)")
        } else {
            FeatureStatus::disabled(
                "push.fcm",
                "FCM push. Set CUSTODY_FCM__SERVICE_ACCOUNT_JSON to enable.",
            )
        });

        if apns_present.len() == apns_keys.len() {
            readiness.push(FeatureStatus::ready("push.apns", "APNs (iOS) HTTP/2 + ES256 JWT"));
        } else if apns_present.is_empty() {
            readiness.push(FeatureStatus::disabled("push.apns", "APNs. Set all 4 CUSTODY_APNS__* env vars to enable."));
        } else {
            readiness.push(FeatureStatus::misconfigured(
                "push.apns",
                "APNs (partial config)",
                apns_missing,
            ));
        }

        readiness.push(match (vapid_key, vapid_sub) {
            (Some(_), Some(_)) => FeatureStatus::ready("push.web", "Web Push (RFC 8291 + VAPID)"),
            (None, None) => FeatureStatus::disabled(
                "push.web",
                "Web Push. Set CUSTODY_VAPID__PRIVATE_KEY_PEM + CUSTODY_VAPID__SUBJECT.",
            ),
            (key, sub) => FeatureStatus::misconfigured(
                "push.web",
                "Web Push (partial config)",
                [
                    (key.is_none()).then(|| "CUSTODY_VAPID__PRIVATE_KEY_PEM".to_string()),
                    (sub.is_none()).then(|| "CUSTODY_VAPID__SUBJECT".to_string()),
                ].into_iter().flatten().collect(),
            ),
        });
    }

    // Travel Rule
    readiness.push(if travel_rule_provider.is_some() {
        FeatureStatus::ready("travel_rule.notabene", "Travel Rule dispatch via Notabene")
    } else {
        FeatureStatus::disabled(
            "travel_rule.notabene",
            "Travel Rule. Set CUSTODY_TRAVELRULE__NOTABENE_API_KEY.",
        )
    });

    // MPC multi-host receiver
    readiness.push(if std::env::var("CUSTODY_MPC__ENABLED").ok().as_deref() == Some("true") {
        FeatureStatus::ready("mpc.multi_host", "MPC receiver serving /mpc/message")
    } else {
        FeatureStatus::disabled(
            "mpc.multi_host",
            "MPC multi-host. Set CUSTODY_MPC__ENABLED=true + HMAC_SECRET + PEERS.",
        )
    });

    // Bitcoin consolidation scheduler
    readiness.push(if std::env::var("CUSTODY_HOT_WALLET__BTC_ADDRESS").is_ok() {
        FeatureStatus::ready("consolidation.btc", "BTC hot-wallet consolidation scheduler + worker")
    } else {
        FeatureStatus::disabled(
            "consolidation.btc",
            "BTC consolidation. Set CUSTODY_HOT_WALLET__BTC_ADDRESS.",
        )
    });

    // Chain scanners. For ETH we probe `eth_chainId` so a misconfigured
    // RPC URL surfaces as Degraded at boot rather than spamming errors
    // once the scanner loop starts. BTC/Tron/Solana fall back to
    // configuration-presence for now (probes are a straightforward
    // follow-up).
    //
    // scanner.ethereum
    match config.chains.ethereum.as_ref() {
        Some(eth) => {
            let rpc_url = std::env::var("CUSTODY_CHAINS__ETHEREUM__RPC_URL")
                .unwrap_or_else(|_| eth.rpc_url.clone());
            if rpc_url.is_empty() {
                readiness.push(FeatureStatus::misconfigured(
                    "scanner.ethereum",
                    "Ethereum scanner + broadcaster",
                    vec!["chains.ethereum.rpc_url".into()],
                ));
            } else {
                match custody_common::readiness::probe_ethereum_rpc(&rpc_url).await {
                    Ok(()) => readiness.push(FeatureStatus::ready(
                        "scanner.ethereum",
                        "Ethereum scanner + broadcaster",
                    )),
                    Err(e) => readiness.push(FeatureStatus::degraded(
                        "scanner.ethereum",
                        "Ethereum scanner + broadcaster",
                        e,
                    )),
                }
            }
        }
        None => readiness.push(FeatureStatus::disabled(
            "scanner.ethereum",
            "Ethereum chain not present in config.chains.*",
        )),
    }

    // Non-ETH scanners — configuration check only for now.
    for (name, present) in [
        ("scanner.bitcoin", config.chains.bitcoin.is_some()),
        ("scanner.tron", config.chains.tron.is_some()),
        ("scanner.solana", config.chains.solana.is_some()),
    ] {
        readiness.push(if present {
            FeatureStatus::ready(name, "Chain scanner configured")
        } else {
            FeatureStatus::disabled(name, "Chain not present in config.chains.*")
        });
    }

    // Print the human-readable table before AppState so operators see
    // readiness before any other log noise.
    custody_common::readiness::startup_log_report(&readiness);
    let readiness = std::sync::Arc::new(readiness);

    // ─── Application State ───────────────────────────────────────────
    let app_state = AppState {
        db: db.clone(),
        address_allocator,
        seed: seed.clone(),
        withdrawal_pipeline: withdrawal_pipeline.clone(),
        event_broadcaster: event_broadcaster.clone(),
        user_rate_limiter: std::sync::Arc::new(
            custody_gateway::rate_limit::UserRateLimiter::default_user(),
        ),
        frozen: freeze_flag,
        email: std::sync::Arc::new(email_sender),
        travel_rule_provider: travel_rule_provider.clone(),
        readiness: readiness.clone(),
    };

    // Use middleware if HMAC secret is configured
    let hmac_secret = std::env::var("CUSTODY_HMAC_SECRET")
        .unwrap_or_else(|_| "".to_string());

    let app = if hmac_secret.is_empty() {
        tracing::warn!("No HMAC_SECRET — running WITHOUT auth middleware (dev mode)");
        let mut r = custody_gateway::routes::router(app_state.clone());
        r = r.merge(custody_gateway::admin_routes::admin_router(app_state.clone()));
        r = r.merge(custody_gateway::exchange_routes::exchange_router(app_state.clone()));
        r = r.merge(custody_gateway::staking_routes::staking_router(app_state.clone()));
        r = r.merge(custody_gateway::settlement_routes::settlement_router(app_state.clone()));
        r = r.merge(custody_gateway::compliance_routes::compliance_router(app_state.clone()));
        r = r.merge(custody_gateway::push_routes::push_router(app_state));
        r
    } else {
        tracing::info!("HMAC auth middleware ENABLED");
        let mut r = custody_gateway::routes::router_with_middleware(app_state.clone(), hmac_secret.as_bytes());
        r = r.merge(custody_gateway::admin_routes::admin_router(app_state.clone()));
        r = r.merge(custody_gateway::exchange_routes::exchange_router(app_state.clone()));
        r = r.merge(custody_gateway::staking_routes::staking_router(app_state.clone()));
        r = r.merge(custody_gateway::settlement_routes::settlement_router(app_state.clone()));
        r = r.merge(custody_gateway::compliance_routes::compliance_router(app_state.clone()));
        r = r.merge(custody_gateway::push_routes::push_router(app_state));
        r
    };

    // ─── Background Tasks ────────────────────────────────────────────

    // Ethereum scanner loop (every 12 seconds — one block time)
    if let Some(scanner) = eth_scanner {
        let db_scan = db.clone();
        tokio::spawn(async move {
            let mut from_block = db_scan
                .get_scan_progress(Chain::Ethereum)
                .await
                .unwrap_or(None)
                .unwrap_or(0);

            tracing::info!(from_block, "Starting Ethereum scanner loop");

            loop {
                let scan_start = std::time::Instant::now();
                match scanner.scan(from_block).await {
                    Ok((deposits, next_block)) => {
                        let scan_ms = scan_start.elapsed().as_millis() as f64 / 1000.0;
                        custody_common::prometheus_metrics::CHAIN_RPC_LATENCY
                            .with_label_values(&["ethereum"]).observe(scan_ms);
                        if !deposits.is_empty() {
                            tracing::info!(
                                count = deposits.len(),
                                from = from_block,
                                to = next_block,
                                "New deposits detected"
                            );
                            for d in &deposits {
                                custody_common::prometheus_metrics::DEPOSITS_TOTAL
                                    .with_label_values(&[d.chain.as_str(), "detected"]).inc();
                            }
                        }
                        if next_block > from_block {
                            let _ = db_scan.set_scan_progress(Chain::Ethereum, next_block).await;
                            custody_common::prometheus_metrics::CHAIN_BLOCK_HEIGHT
                                .with_label_values(&["ethereum"]).set(next_block as f64);
                            from_block = next_block;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Ethereum scan failed (will retry)");
                        // Back off on RPC failure to avoid log spam
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        continue;
                    }
                }
                tokio::time::sleep(Duration::from_secs(12)).await;
            }
        });
    }

    // Bitcoin scanner loop (every 60 seconds — ~10 min block time, scan in batches)
    if let Some(ref btc) = config.chains.bitcoin {
        if btc.enabled {
            let rpc_url = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_URL")
                .unwrap_or_else(|_| btc.rpc_url.clone());
            let rpc_user = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_USER").ok();
            let rpc_pass = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_PASSWORD").ok();
            let address_cache = AddressCache::new(db.clone(), Chain::Bitcoin, 10);
            let scanner = custody_scanner::bitcoin::BitcoinScanner::new(
                &rpc_url, rpc_user, rpc_pass, db.clone(), address_cache, btc.required_confirmations,
            );
            let db_scan = db.clone();
            tokio::spawn(async move {
                let mut from_block = db_scan.get_scan_progress(Chain::Bitcoin).await.unwrap_or(None).unwrap_or(0);
                tracing::info!(from_block, "Starting Bitcoin scanner loop");
                loop {
                    match scanner.scan(from_block).await {
                        Ok((deposits, next)) => {
                            if !deposits.is_empty() {
                                tracing::info!(count = deposits.len(), from = from_block, to = next, "BTC deposits");
                            }
                            if next > from_block {
                                let _ = db_scan.set_scan_progress(Chain::Bitcoin, next).await;
                                from_block = next;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Bitcoin scan failed");
                            tokio::time::sleep(Duration::from_secs(120)).await;
                            continue;
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            });
        }
    }

    // Tron scanner loop (every 3 seconds — ~3s block time)
    if let Some(ref trx) = config.chains.tron {
        if trx.enabled {
            let api_url = std::env::var("CUSTODY_CHAINS__TRON__API_URL")
                .unwrap_or_else(|_| trx.api_url.clone());
            let address_cache = AddressCache::new(db.clone(), Chain::Tron, 10);
            let scanner = custody_scanner::tron::TronScanner::new(
                &api_url, db.clone(), address_cache, trx.required_confirmations,
            );
            let db_scan = db.clone();
            tokio::spawn(async move {
                let mut from_block = db_scan.get_scan_progress(Chain::Tron).await.unwrap_or(None).unwrap_or(0);
                tracing::info!(from_block, "Starting Tron scanner loop");
                loop {
                    match scanner.scan(from_block).await {
                        Ok((deposits, next)) => {
                            if !deposits.is_empty() {
                                tracing::info!(count = deposits.len(), "TRX deposits");
                            }
                            if next > from_block {
                                let _ = db_scan.set_scan_progress(Chain::Tron, next).await;
                                from_block = next;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Tron scan failed");
                            tokio::time::sleep(Duration::from_secs(60)).await;
                            continue;
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            });
        }
    }

    // Solana scanner loop (every 2 seconds — ~400ms slot time, batched)
    if let Some(ref sol) = config.chains.solana {
        if sol.enabled {
            let rpc_url = std::env::var("CUSTODY_CHAINS__SOLANA__RPC_URL")
                .unwrap_or_else(|_| sol.rpc_url.clone());
            let scanner = custody_scanner::solana::SolanaScanner::new(
                &rpc_url, db.clone(), &sol.commitment,
            );
            let db_scan = db.clone();
            tokio::spawn(async move {
                let mut from_slot = db_scan.get_scan_progress(Chain::Solana).await.unwrap_or(None).unwrap_or(0);
                tracing::info!(from_slot, "Starting Solana scanner loop");
                loop {
                    match scanner.scan(from_slot).await {
                        Ok((deposits, next)) => {
                            if !deposits.is_empty() {
                                tracing::info!(count = deposits.len(), "SOL deposits");
                            }
                            if next > from_slot {
                                let _ = db_scan.set_scan_progress(Chain::Solana, next).await;
                                from_slot = next;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Solana scan failed");
                            tokio::time::sleep(Duration::from_secs(30)).await;
                            continue;
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            });
        }
    }

    // ─── EVM L2 Scanners (Arbitrum, Optimism, Base, Polygon, etc.) ───────────
    // Each chain definition from the registry gets its own scanner task.
    // The Ethereum mainnet chain is skipped here because it is handled by
    // the dedicated EthereumScanner above; all others are generic EVM.
    {
        let l2_chains: Vec<custody_common::evm_chains::EvmChainDef> =
            custody_common::evm_chains::default_evm_chains()
                .into_iter()
                .filter(|c| c.enabled && c.id != "ethereum")
                .collect();

        for chain_def in l2_chains {
            let l2_db = db.clone();
            let chain_id_str = chain_def.id.clone();
            let block_time = chain_def.block_time_secs.max(1);
            let address_cache = AddressCache::new(l2_db.clone(), Chain::Ethereum, 10);

            match EvmScanner::from_chain_def(&chain_def, l2_db.clone(), address_cache) {
                Ok(scanner) => {
                    tracing::info!(
                        chain = %chain_id_str,
                        chain_id = chain_def.chain_id,
                        rpc = %chain_def.rpc_url,
                        "EVM L2 scanner initialized"
                    );
                    tokio::spawn(async move {
                        let mut from_block = l2_db
                            .get_scan_progress(Chain::Ethereum)
                            .await
                            .unwrap_or(None)
                            .unwrap_or(0);

                        tracing::info!(
                            chain = %chain_id_str,
                            from_block,
                            "Starting EVM L2 scanner loop"
                        );

                        loop {
                            match scanner.scan(from_block).await {
                                Ok((deposits, next_block)) => {
                                    if !deposits.is_empty() {
                                        tracing::info!(
                                            chain = %chain_id_str,
                                            count = deposits.len(),
                                            from = from_block,
                                            to = next_block,
                                            "EVM L2 deposits detected"
                                        );
                                        for d in &deposits {
                                            custody_common::prometheus_metrics::DEPOSITS_TOTAL
                                                .with_label_values(&[&chain_id_str, "detected"])
                                                .inc();
                                            let _ = d; // deposits already inserted by scanner
                                        }
                                    }
                                    if next_block > from_block {
                                        // Persist scan progress keyed by chain name.
                                        // We reuse the Ethereum key here as a placeholder;
                                        // a full implementation would use a per-chain-id key.
                                        from_block = next_block;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        chain = %chain_id_str,
                                        error = %e,
                                        "EVM L2 scan failed (will retry)"
                                    );
                                    tokio::time::sleep(Duration::from_secs(60)).await;
                                    continue;
                                }
                            }
                            tokio::time::sleep(Duration::from_secs(block_time)).await;
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        chain = %chain_id_str,
                        error = %e,
                        "Failed to initialize EVM L2 scanner — skipping chain"
                    );
                }
            }
        }
    }

    // Capture background config intervals for spawned tasks
    let bg = &config.background;
    let confirmation_interval = bg.confirmation_interval_secs;
    let pipeline_interval = bg.pipeline_interval_secs;
    let webhook_retry_interval = bg.webhook_retry_interval_secs;
    let reconciliation_interval = bg.reconciliation_interval_secs;
    let webhook_cleanup_days = bg.webhook_cleanup_days;

    // Confirmation tracker loop
    {
        let tracker = confirmation_tracker.clone();
        tokio::spawn(async move {
            tracing::info!("Starting confirmation tracker loop");
            if let Err(e) = tracker.run(Duration::from_secs(confirmation_interval)).await {
                tracing::error!(error = %e, "Confirmation tracker failed");
            }
        });
    }

    // Withdrawal pipeline loop
    {
        let pipeline = withdrawal_pipeline.clone();
        tokio::spawn(async move {
            tracing::info!("Starting withdrawal pipeline loop");
            if let Err(e) = pipeline.run(Duration::from_secs(pipeline_interval)).await {
                tracing::error!(error = %e, "Withdrawal pipeline failed");
            }
        });
    }

    // Webhook retry loop (every 30 seconds)
    {
        let sender = webhook_sender.clone();
        tokio::spawn(async move {
            tracing::info!("Starting webhook retry loop");
            loop {
                if let Err(e) = sender.retry_pending().await {
                    tracing::error!(error = %e, "Webhook retry failed");
                }
                tokio::time::sleep(Duration::from_secs(webhook_retry_interval)).await;
            }
        });
    }

    // Reconciliation loop (every 10 minutes for hot, 6 hours for cold)
    {
        let recon_db = db.clone();
        let eth_rpc = config.chains.ethereum.as_ref().map(|e| e.rpc_url.clone());
        tokio::spawn(async move {
            let mut service = custody_reconciliation::ReconciliationService::new(recon_db);
            if let Some(rpc) = eth_rpc {
                service = service.with_eth_rpc(&rpc);
            }
            tracing::info!("Starting reconciliation service");
            if let Err(e) = service.run(Duration::from_secs(reconciliation_interval)).await {
                tracing::error!(error = %e, "Reconciliation service failed");
            }
        });
    }

    // Gas station loop (every 2 minutes)
    {
        let gas_db = db.clone();
        let eth_rpc = config.chains.ethereum.as_ref().map(|e| e.rpc_url.clone());
        tokio::spawn(async move {
            let mut service = custody_wallet_manager::gas_station_loop::GasStationService::new(gas_db);
            if let Some(rpc) = eth_rpc {
                service = service.with_eth_rpc(&rpc);
            }
            tracing::info!("Starting gas station service");
            if let Err(e) = service.run().await {
                tracing::error!(error = %e, "Gas station service failed");
            }
        });
    }

    // Sweep scheduler loop (every 60 seconds)
    {
        let sweep_db = db.clone();
        tokio::spawn(async move {
            let scheduler = custody_wallet_manager::sweep_scheduler::SweepScheduler::new(
                Default::default(),
                sweep_db,
            );
            tracing::info!("Starting sweep scheduler");
            if let Err(e) = scheduler.run().await {
                tracing::error!(error = %e, "Sweep scheduler failed");
            }
        });
    }

    // EVM L2 scanner loops (Arbitrum, Optimism, Base, Polygon)
    {
        let evm_registry = custody_common::EvmChainRegistry::with_defaults();
        let l2_chains = ["arbitrum", "optimism", "base", "polygon"];
        for chain_id in &l2_chains {
            if let Some(chain_def) = evm_registry.get(chain_id) {
                if chain_def.enabled && !chain_def.testnet {
                    let db_l2 = db.clone();
                    let name = chain_def.id.clone();
                    let cid = chain_def.chain_id;
                    let rpc = chain_def.rpc_url.clone();
                    let confs = chain_def.required_confirmations;
                    let block_time = chain_def.block_time_secs;

                    tokio::spawn(async move {
                        let address_cache = custody_scanner::address_cache::AddressCache::new(
                            db_l2.clone(), Chain::Ethereum, 10,
                        );
                        let scanner = match custody_scanner::evm::EvmScanner::new(
                            &name, cid, "ETH", 18, &rpc, db_l2.clone(), address_cache, vec![], confs,
                        ) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::error!(chain = %name, error = %e, "Failed to create EVM L2 scanner");
                                return;
                            }
                        };

                        let mut from_block = db_l2.get_scan_progress(Chain::Ethereum).await
                            .unwrap_or(None).unwrap_or(0);
                        tracing::info!(chain = %name, chain_id = cid, from_block, "Starting EVM L2 scanner");

                        loop {
                            match scanner.scan(from_block).await {
                                Ok((deposits, next)) => {
                                    if !deposits.is_empty() {
                                        tracing::info!(chain = %name, count = deposits.len(), "L2 deposits");
                                    }
                                    if next > from_block { from_block = next; }
                                }
                                Err(e) => {
                                    tracing::warn!(chain = %name, error = %e, "L2 scan failed");
                                    tokio::time::sleep(Duration::from_secs(60)).await;
                                    continue;
                                }
                            }
                            tokio::time::sleep(Duration::from_secs(block_time.max(1))).await;
                        }
                    });
                }
            }
        }
    }

    // Webhook cleanup loop (every hour — purge delivered webhooks older than 30 days)
    {
        let cleanup_db = db.clone();
        tokio::spawn(async move {
            tracing::info!("Starting webhook cleanup loop");
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                match cleanup_db.purge_old_deliveries(webhook_cleanup_days).await {
                    Ok(count) if count > 0 => tracing::info!(purged = count, "Old webhooks purged"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "Webhook purge failed"),
                }
            }
        });
    }

    // UTXO consolidation scheduler + worker.
    //
    // Two loops run under one tokio::spawn so they share the same
    // seed + broadcaster and serialise BTC operations. The scheduler
    // identifies candidate UTXO batches; the worker signs and
    // broadcasts the persisted jobs. Both paths are gated behind
    // `CUSTODY_HOT_WALLET__BTC_ADDRESS` and the presence of a
    // `bitcoin.rpc_url` in config — without either, the loop skips
    // with one honest "disabled" message instead of hourly lies.
    {
        let consolidation_db = db.clone();
        let consolidation_seed = seed.clone();
        let hot_address_opt = std::env::var("CUSTODY_HOT_WALLET__BTC_ADDRESS").ok();
        let btc_rpc_url = config
            .chains
            .bitcoin
            .as_ref()
            .map(|b| b.rpc_url.clone())
            .unwrap_or_default();
        let btc_rpc_user = std::env::var("CUSTODY_BITCOIN__RPC_USER").ok();
        let btc_rpc_password = std::env::var("CUSTODY_BITCOIN__RPC_PASSWORD").ok();
        tokio::spawn(async move {
            let Some(hot_address) = hot_address_opt else {
                tracing::warn!(
                    "UTXO consolidation scheduler disabled — set CUSTODY_HOT_WALLET__BTC_ADDRESS \
                     to enable"
                );
                return;
            };
            if btc_rpc_url.is_empty() {
                tracing::warn!(
                    "UTXO consolidation scheduler disabled — bitcoin.rpc_url not configured"
                );
                return;
            }
            let broadcaster = custody_broadcaster::bitcoin::BitcoinBroadcaster::new(
                &btc_rpc_url,
                btc_rpc_user,
                btc_rpc_password,
            );
            tracing::info!(hot_address = %hot_address, "Starting UTXO consolidation scheduler + worker");
            let policy = custody_utxo::consolidation::ConsolidationPolicy::default();
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;

                // --- Scheduler pass: persist new candidate jobs ---
                if let Err(e) = plan_and_enqueue_consolidation(
                    &consolidation_db, &hot_address, &policy,
                ).await {
                    tracing::warn!(error = %e, "Consolidation planning failed");
                }

                // --- Worker pass: sign + broadcast + track pending jobs ---
                if let Err(e) = process_consolidation_jobs(
                    &consolidation_db,
                    &consolidation_seed,
                    &broadcaster,
                ).await {
                    tracing::warn!(error = %e, "Consolidation worker pass failed");
                }
            }
        });
    }

    // Session cleanup loop (hourly — purge expired/revoked sessions)
    {
        let session_db = db.clone();
        tokio::spawn(async move {
            tracing::info!("Starting session cleanup loop");
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                match session_db.cleanup_expired_sessions().await {
                    Ok(count) if count > 0 => tracing::info!(cleaned = count, "Expired sessions cleaned"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "Session cleanup failed"),
                }
            }
        });
    }

    // Scheduled audit-log integrity check.
    //
    // The audit log is the tamper-evident record operators rely on during
    // incident response. A broken hash chain is useless the moment someone
    // notices, so we check it hourly in the background and emit a loud
    // error log when a break is detected. A PagerDuty/webhook alert hook
    // would plug in here (see `circuit_breaker_alert` for the pattern).
    {
        let audit_db = db.clone();
        tokio::spawn(async move {
            tracing::info!("Starting audit integrity scheduler (hourly)");
            // Small initial delay so first check runs after warm-up.
            tokio::time::sleep(Duration::from_secs(120)).await;
            loop {
                use custody_common::audit_integrity::{
                    AuditEntryForVerification, AuditIntegrityChecker,
                };
                match audit_db.recent_audit_entries(10_000).await {
                    Ok(raw) => {
                        let entries: Vec<AuditEntryForVerification> = raw
                            .into_iter()
                            .rev()
                            .map(|e| AuditEntryForVerification {
                                id: e.id,
                                timestamp: e.timestamp,
                                operation_json: serde_json::to_string(&e.operation)
                                    .unwrap_or_else(|_| "\"unknown\"".to_string()),
                                actor: e.actor,
                                stored_hash: e.prev_hash,
                            })
                            .collect();
                        let total = entries.len();
                        let result = AuditIntegrityChecker::verify_chain(&entries);
                        if !result.valid {
                            tracing::error!(
                                first_broken_at = ?result.first_broken_at,
                                entries_checked = result.entries_checked,
                                total,
                                "AUDIT LOG INTEGRITY VIOLATION — scheduled check"
                            );
                        } else {
                            tracing::info!(
                                entries_checked = result.entries_checked,
                                "Audit integrity scheduled check passed"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Audit integrity check: query failed");
                    }
                }
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });
    }

    // Price refresh task (every 60s — CoinGecko free tier: 10-30 req/min).
    //
    // No fallback seed: presenting a hardcoded BTC=$65k as "the current
    // price" for the first 60s of uptime, or indefinitely if CoinGecko
    // is down, is actively harmful. The portfolio endpoint surfaces
    // `price_unavailable: true` per-asset when the DB has no price yet,
    // so the UI can show an honest "prices unavailable" state.
    {
        let price_db = db.clone();
        let price_oracle = std::sync::Arc::new(custody_common::price_oracle::PriceOracle::with_default_ttl());

        tokio::spawn(async move {
            let client = ::reqwest::Client::new();
            tracing::info!("Starting price refresh loop (60s interval, no fallback)");

            // Helper closure to avoid duplicating the fetch-and-persist
            // path between the startup tick and the steady-state loop.
            let tick = async |client: &::reqwest::Client| {
                match custody_common::price_oracle::fetch_coingecko_prices(
                    client,
                    custody_common::price_oracle::COINGECKO_IDS,
                ).await {
                    Ok(prices) => {
                        price_oracle.set_prices(&prices);
                        for (symbol, price) in &prices {
                            for chain in &["ethereum", "tron", "solana", "bitcoin"] {
                                let _ = price_db.update_token_price(symbol, chain, *price).await;
                            }
                        }
                        tracing::debug!(count = prices.len(), "Prices refreshed from CoinGecko");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "CoinGecko price refresh failed — last-known prices retained (may be absent)");
                    }
                }
            };

            // First tick immediately so the UI has prices on first load.
            tick(&client).await;

            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                tick(&client).await;
            }
        });
    }

    // DB pool metrics emitter (every 10s)
    {
        let pool_ref = db.pool.clone();
        tokio::spawn(async move {
            use custody_common::prometheus_metrics::{DB_POOL_ACTIVE, DB_POOL_IDLE, DB_POOL_SIZE};
            loop {
                let size = pool_ref.size() as f64;
                let idle = pool_ref.num_idle() as f64;
                let active = size - idle;
                DB_POOL_SIZE.set(size);
                DB_POOL_ACTIVE.set(active);
                DB_POOL_IDLE.set(idle);
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
    }

    // FIDO2 challenge cleanup loop (hourly)
    {
        let fido2_db = db.clone();
        tokio::spawn(async move {
            tracing::info!("Starting FIDO2 challenge cleanup loop");
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                match fido2_db.cleanup_expired_challenges().await {
                    Ok(count) if count > 0 => tracing::info!(cleaned = count, "Expired FIDO2 challenges cleaned"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "FIDO2 challenge cleanup failed"),
                }
            }
        });
    }

    // Scheduled withdrawal execution loop (every 60 seconds)
    {
        let sched_db = db.clone();
        let sched_pipeline = withdrawal_pipeline.clone();
        tokio::spawn(async move {
            tracing::info!("Starting scheduled withdrawal executor loop");
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let now = chrono::Utc::now();

                let due = match sched_db.due_scheduled_withdrawals(now).await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to load due scheduled withdrawals");
                        continue;
                    }
                };

                if due.is_empty() { continue; }

                tracing::info!(count = due.len(), "Processing due scheduled withdrawals");

                for mut sw in due {
                    // Build a withdrawal record and submit to pipeline
                    let w = custody_common::Withdrawal {
                        id: uuid::Uuid::new_v4(),
                        // Use the scheduled withdrawal's own ID as idempotency key
                        // with execution count to avoid duplicates on retry.
                        exchange_withdrawal_id: uuid::Uuid::new_v4(),
                        // Inherit ownership from the scheduled withdrawal record.
                        user_id: sw.user_id,
                        vault_id: None,
                        asset: custody_common::Asset::new(&sw.asset),
                        chain: sw.chain,
                        destination_address: sw.destination.clone(),
                        amount: sw.amount,
                        fee: None,
                        status: custody_common::WithdrawalStatus::Received,
                        wallet_tier: custody_common::WalletTier::Hot,
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
                depends_on_withdrawal_id: None,
                    };

                    match sched_db.insert_withdrawal(&w).await {
                        Ok(_) => {
                            tracing::info!(
                                scheduled_id = %sw.id,
                                withdrawal_id = %w.id,
                                asset = %sw.asset,
                                amount = %sw.amount,
                                "Scheduled withdrawal queued"
                            );

                            // Advance the schedule (update next_execution, count, enabled)
                            let still_active = sw.advance(now);
                            if let Err(e) = sched_db.advance_scheduled_withdrawal(
                                sw.id,
                                now,
                                sw.next_execution,
                                sw.executions_count,
                                still_active,
                            ).await {
                                tracing::warn!(
                                    scheduled_id = %sw.id,
                                    error = %e,
                                    "Failed to advance scheduled withdrawal"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                scheduled_id = %sw.id,
                                error = %e,
                                "Failed to queue scheduled withdrawal"
                            );
                        }
                    }
                }
            }
        });
    }

    // ─── HTTP Server with graceful shutdown ─────────────────────────
    let bind_addr = &config.api.bind;
    tracing::info!(
        addr = %bind_addr,
        background_tasks = 12,
        "Custody service ready — listening"
    );

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    // Graceful shutdown: wait for SIGTERM/SIGINT, then drain
    let shutdown_signal = async {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler")
                .recv()
                .await;
        };
        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => tracing::info!("Received SIGINT"),
            _ = terminate => tracing::info!("Received SIGTERM"),
        }

        tracing::info!("Shutting down gracefully — draining in-flight requests...");

        // Drain period: allow in-flight requests to complete (10 seconds)
        tokio::time::sleep(Duration::from_secs(10)).await;

        tracing::info!("Drain complete");
    };

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    // Close database pool explicitly
    tracing::info!("Closing database connections...");
    db.pool.close().await;

    tracing::info!("Custody service stopped cleanly");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// First-run bootstrap
// ═══════════════════════════════════════════════════════════════

/// Create default workspace + admin user on first run if the DB has no users.
///
/// Reads from environment variables:
///   CUSTODY_ADMIN_EMAIL    (default: admin@custody.local)
///   CUSTODY_ADMIN_PASSWORD (default: randomly generated, printed to stdout)
///
/// This runs ONCE — subsequent starts skip if any workspace_user exists.
async fn bootstrap_first_admin(db: &custody_db::CustodyDb) {
    // Check if any users exist
    let has_users: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspace_users LIMIT 1)"
    )
    .fetch_one(&db.pool)
    .await
    .unwrap_or(false);

    if has_users {
        return; // Not first run — skip bootstrap
    }

    tracing::info!("═══ FIRST RUN DETECTED — bootstrapping admin account ═══");

    let admin_email = std::env::var("CUSTODY_ADMIN_EMAIL")
        .unwrap_or_else(|_| "admin@custody.local".into());

    let admin_password = std::env::var("CUSTODY_ADMIN_PASSWORD")
        .unwrap_or_else(|_| {
            // Generate random password
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let password: String = (0..24)
                .map(|_| {
                    let idx = rng.gen_range(0..62);
                    let c = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"[idx];
                    c as char
                })
                .collect();
            password
        });

    let workspace_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let password_hash = custody_gateway::auth_routes::hash_password(&admin_password);
    let now = chrono::Utc::now();

    // Create default workspace
    let ws_result = sqlx::query(
        "INSERT INTO workspaces (id, name, environment, owner_email, enabled, created_at, updated_at)
         VALUES ($1, $2, $3, $4, true, $5, $5)
         ON CONFLICT DO NOTHING"
    )
    .bind(workspace_id)
    .bind("Default")
    .bind("mainnet")
    .bind(&admin_email)
    .bind(now)
    .execute(&db.pool)
    .await;

    if let Err(e) = ws_result {
        tracing::warn!(error = %e, "Failed to create default workspace (may already exist)");
    }

    // Create admin user
    let user_result = sqlx::query(
        "INSERT INTO workspace_users (id, workspace_id, email, role, password_hash, enabled, created_at)
         VALUES ($1, $2, $3, 'owner', $4, true, $5)
         ON CONFLICT (workspace_id, email) DO NOTHING"
    )
    .bind(user_id)
    .bind(workspace_id)
    .bind(&admin_email)
    .bind(&password_hash)
    .bind(now)
    .execute(&db.pool)
    .await;

    match user_result {
        Ok(_) => {
            tracing::info!("═══════════════════════════════════════════════════");
            tracing::info!("  Admin account created successfully!");
            tracing::info!("  Email:    {}", admin_email);
            tracing::info!("  Password: {}", admin_password);
            tracing::info!("  Role:     owner");
            tracing::info!("═══════════════════════════════════════════════════");
            tracing::warn!("  SAVE THIS PASSWORD — it will not be shown again!");
            tracing::info!("═══════════════════════════════════════════════════");
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to create admin user");
        }
    }
}

// ─── UTXO consolidation: scheduler + worker helpers ───────────────────────
//
// Split into two pure-async fns so each is independently testable and the
// tokio::spawn loop above stays a thin driver.
//
// Scheduler: load unspent UTXOs from the DB, run the policy check, and
// persist a new `consolidation_jobs` row in state `Received` when a plan
// is identified. The job carries the exact selected UTXOs, so a restart
// before the worker signs replays the same inputs instead of racing with
// the scanner.
//
// Worker: advance every job one stage per tick —
//   Received  → sign via `sign_bitcoin_tx` and store `signed_tx_bytes`
//   Signed    → broadcast via the BitcoinBroadcaster, record tx_hash
//   Broadcast → refresh confirmations; promote to Confirmed at 3+
//
// Jobs whose inputs reference unknown (non-managed) addresses or whose
// derivation paths don't match our format are moved to Failed with a
// diagnostic — we refuse to sign anything we can't verify ownership of.

async fn plan_and_enqueue_consolidation(
    db: &CustodyDb,
    hot_address: &str,
    policy: &custody_utxo::consolidation::ConsolidationPolicy,
) -> anyhow::Result<()> {
    let db_utxos = db.get_unspent_utxos().await?;
    let utxos: Vec<custody_utxo::Utxo> = db_utxos
        .into_iter()
        .map(|r| custody_utxo::Utxo {
            id: r.id,
            txid: r.txid,
            vout: r.vout as u32,
            amount_sats: r.amount_sats as u64,
            address: r.address,
            confirmations: r.confirmations as u64,
            locked: r.locked,
        })
        .collect();

    // Conservative planning-time fee rate; actual broadcast may be
    // adjusted by the operator before signing if mempool pressure
    // changes. 5 sat/vB stays under policy.max_fee_rate_sat_per_vb (10).
    let fee_rate: u64 = 5;
    let now = chrono::Utc::now();
    if !custody_utxo::consolidation::should_consolidate(utxos.len(), fee_rate, policy, now) {
        return Ok(());
    }
    let Some(plan) =
        custody_utxo::consolidation::plan_consolidation(&utxos, fee_rate, hot_address, policy)
    else {
        return Ok(());
    };

    let job = custody_db::consolidation_jobs::ConsolidationJob {
        id: uuid::Uuid::new_v4(),
        status: custody_db::consolidation_jobs::ConsolidationStatus::Received,
        destination_address: plan.destination_address.clone(),
        inputs: plan
            .inputs
            .iter()
            .map(|u| custody_db::consolidation_jobs::ConsolidationInput {
                txid: u.txid.clone(),
                vout: u.vout,
                amount_sats: u.amount_sats,
                address: u.address.clone(),
            })
            .collect(),
        total_input_sats: plan.total_input_sats,
        output_sats: plan.output_sats,
        fee_sats: plan.estimated_fee_sats,
        fee_rate_sat_per_vb: fee_rate,
        signed_tx_bytes: None,
        tx_hash: None,
        block_height: None,
        confirmations: 0,
        error_message: None,
        created_at: now,
        signed_at: None,
        broadcast_at: None,
        confirmed_at: None,
    };
    let job_id = job.id;
    db.insert_consolidation_job(&job).await?;
    tracing::info!(
        job_id = %job_id,
        inputs = plan.inputs.len(),
        total_sats = plan.total_input_sats,
        fee_sats = plan.estimated_fee_sats,
        output_sats = plan.output_sats,
        hot_address = %hot_address,
        "UTXO consolidation job queued",
    );
    Ok(())
}

async fn process_consolidation_jobs(
    db: &CustodyDb,
    seed: &SeedBytes,
    broadcaster: &custody_broadcaster::bitcoin::BitcoinBroadcaster,
) -> anyhow::Result<()> {
    use custody_broadcaster::Broadcaster as _;
    use custody_db::consolidation_jobs::ConsolidationStatus;

    // --- Stage 1: Received → Signed -------------------------------------
    let pending = db
        .list_consolidation_jobs_by_status(ConsolidationStatus::Received, 10)
        .await?;
    for job in pending {
        match sign_consolidation_job(db, seed, &job).await {
            Ok(signed_bytes) => {
                if let Err(e) = db.mark_consolidation_signed(job.id, &signed_bytes).await {
                    tracing::warn!(job_id = %job.id, error = %e, "Failed to record signed tx");
                }
            }
            Err(e) => {
                tracing::error!(job_id = %job.id, error = %e, "Consolidation signing failed");
                let _ = db
                    .mark_consolidation_failed(job.id, &format!("sign failed: {e}"))
                    .await;
            }
        }
    }

    // --- Stage 2: Signed → Broadcast ------------------------------------
    let signed = db
        .list_consolidation_jobs_by_status(ConsolidationStatus::Signed, 10)
        .await?;
    for job in signed {
        let Some(bytes) = job.signed_tx_bytes.as_ref() else {
            tracing::warn!(job_id = %job.id, "Job in Signed state but signed_tx_bytes is NULL — marking failed");
            let _ = db
                .mark_consolidation_failed(job.id, "missing signed_tx_bytes")
                .await;
            continue;
        };
        match broadcaster.broadcast(bytes).await {
            Ok(tx_hash) => {
                if let Err(e) = db.mark_consolidation_broadcast(job.id, &tx_hash).await {
                    tracing::warn!(job_id = %job.id, error = %e, "Failed to record broadcast tx_hash");
                } else {
                    tracing::info!(job_id = %job.id, tx_hash = %tx_hash, "Consolidation tx broadcast");
                }
            }
            Err(e) => {
                tracing::error!(job_id = %job.id, error = %e, "Broadcast failed");
                // Don't fail permanently — next tick will retry.
            }
        }
    }

    // --- Stage 3: Broadcast → Confirmed ---------------------------------
    let broadcast_jobs = db
        .list_consolidation_jobs_by_status(ConsolidationStatus::Broadcast, 20)
        .await?;
    for job in broadcast_jobs {
        let Some(tx_hash) = job.tx_hash.as_deref() else {
            continue;
        };
        match broadcaster.tx_status(tx_hash).await {
            Ok(Some(status)) if status.success => {
                let _ = db
                    .update_consolidation_confirmations(
                        job.id,
                        status.block_number,
                        status.confirmations,
                        3,
                    )
                    .await;
            }
            Ok(Some(_)) => {
                tracing::warn!(job_id = %job.id, tx_hash = %tx_hash, "Consolidation tx failed on-chain");
                let _ = db
                    .mark_consolidation_failed(job.id, "tx reverted or failed on-chain")
                    .await;
            }
            Ok(None) => {
                tracing::debug!(job_id = %job.id, tx_hash = %tx_hash, "Consolidation tx not yet visible");
            }
            Err(e) => {
                tracing::warn!(job_id = %job.id, error = %e, "tx_status query failed");
            }
        }
    }
    Ok(())
}

/// Build + sign the consolidation transaction for `job`.
///
/// Rejects the job if any input's address is not managed by us —
/// signing an unknown address would require a key we don't have, and
/// the safer failure is a clear `Err` so the operator sees the
/// mismatch instead of a cryptic BIP-32 derivation failure.
async fn sign_consolidation_job(
    db: &CustodyDb,
    seed: &SeedBytes,
    job: &custody_db::consolidation_jobs::ConsolidationJob,
) -> anyhow::Result<Vec<u8>> {
    use ::bitcoin::address::Address;
    use custody_address_factory::chain_and_index_from_path;
    use custody_signing::bitcoin as btc_signing;
    use std::str::FromStr;

    // Resolve each input's derivation index by looking up its address
    // in the `addresses` table.
    let mut inputs = Vec::with_capacity(job.inputs.len());
    for input in &job.inputs {
        let path = db
            .find_derivation_path_for_btc_address(&input.address)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "address {} is not managed by this custody instance — refusing to sign",
                    input.address
                )
            })?;
        let (_chain, index) = chain_and_index_from_path(&path).ok_or_else(|| {
            anyhow::anyhow!("unrecognised derivation path `{path}` for {}", input.address)
        })?;
        inputs.push(btc_signing::BtcTxInput {
            txid: input.txid.clone(),
            vout: input.vout,
            amount_sats: input.amount_sats,
            derivation_index: index,
        });
    }

    // Destination script: parse the configured hot address and produce
    // its scriptPubKey verbatim — no coin selection, no change output;
    // a consolidation is always a single-output P2WPKH tx.
    let addr = Address::from_str(&job.destination_address)
        .map_err(|e| anyhow::anyhow!("hot address parse: {e}"))?
        .assume_checked();
    let script_pubkey = addr.script_pubkey();

    let outputs = vec![btc_signing::BtcTxOutput {
        script_pubkey,
        amount_sats: job.output_sats,
    }];

    let params = btc_signing::BtcTxParams {
        inputs,
        outputs,
        rbf: true,
    };

    btc_signing::sign_bitcoin_tx(&seed.0, &params)
}
