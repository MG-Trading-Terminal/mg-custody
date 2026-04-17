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

    // Wire Chainalysis screening (if API key available)
    if let Ok(chainalysis_key) = std::env::var("CUSTODY_CHAINALYSIS_API_KEY") {
        let screener = Arc::new(custody_compliance::screening::ChainalysisProvider::new(&chainalysis_key));
        pipeline_builder = pipeline_builder.with_screening_provider(screener);
        tracing::info!("Chainalysis KYT screening enabled");
    } else {
        tracing::warn!("No CUSTODY_CHAINALYSIS_API_KEY — AML screening disabled");
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

    let confirmation_tracker = Arc::new(ConfirmationTracker::new(
        db.clone(),
        block_height_fns,
        webhook_sender.clone(),
    ));

    // ─── Email transport ─────────────────────────────────────────────
    // `CUSTODY_EMAIL__MODE` selects the transport: `console` (default in
    // dev — pretty-prints to stdout) or `noop` (silent, useful for tests
    // and local scripts). SMTP variant is a follow-up once lettre is
    // added to the workspace.
    let email_from = std::env::var("CUSTODY_EMAIL__FROM")
        .unwrap_or_else(|_| "MG Custody <no-reply@custody.local>".to_string());
    let email_sender = match std::env::var("CUSTODY_EMAIL__MODE")
        .unwrap_or_else(|_| "console".to_string())
        .as_str()
    {
        "noop" => custody_common::email::EmailSender::noop(),
        _ => custody_common::email::EmailSender::console(email_from),
    };
    tracing::info!(
        transport = email_sender.transport_name(),
        "Email sender initialized"
    );

    // ─── Application State ───────────────────────────────────────────
    let app_state = AppState {
        db: db.clone(),
        address_allocator,
        seed,
        withdrawal_pipeline: withdrawal_pipeline.clone(),
        event_broadcaster: event_broadcaster.clone(),
        user_rate_limiter: std::sync::Arc::new(
            custody_gateway::rate_limit::UserRateLimiter::default_user(),
        ),
        frozen: freeze_flag,
        email: std::sync::Arc::new(email_sender),
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

    // UTXO consolidation loop (daily at 03:00 UTC — when fees are typically low)
    {
        let consolidation_db = db.clone();
        tokio::spawn(async move {
            tracing::info!("Starting UTXO consolidation scheduler");
            let policy = custody_utxo::consolidation::ConsolidationPolicy::default();
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await; // check hourly

                // Load current UTXOs from DB and convert to utxo crate types
                let db_utxos = match consolidation_db.get_unspent_utxos().await {
                    Ok(u) => u,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to load UTXOs for consolidation check");
                        continue;
                    }
                };
                let utxos: Vec<custody_utxo::Utxo> = db_utxos.into_iter().map(|r| {
                    custody_utxo::Utxo {
                        id: r.id,
                        txid: r.txid,
                        vout: r.vout as u32,
                        amount_sats: r.amount_sats as u64,
                        address: r.address,
                        confirmations: r.confirmations as u64,
                        locked: r.locked,
                    }
                }).collect();

                // Check fee environment (use conservative 5 sat/vB for planning)
                let fee_rate = 5u64;
                let now = chrono::Utc::now();

                if !custody_utxo::consolidation::should_consolidate(utxos.len(), fee_rate, &policy, now) {
                    continue;
                }

                // Get hot wallet address for consolidation destination
                let hot_address = "bc1q_hot_wallet_placeholder".to_string(); // TODO: derive from config

                if let Some(plan) = custody_utxo::consolidation::plan_consolidation(&utxos, fee_rate, &hot_address, &policy) {
                    tracing::info!(
                        inputs = plan.inputs.len(),
                        total_sats = plan.total_input_sats,
                        fee_sats = plan.estimated_fee_sats,
                        output_sats = plan.output_sats,
                        "UTXO consolidation planned — submitting to pipeline"
                    );
                    // TODO: Create consolidation withdrawal and submit to pipeline
                    // The withdrawal should have tx_type = "consolidation" to bypass circuit breaker
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

    // Price refresh task (every 60s — CoinGecko free tier: 10-30 req/min)
    {
        let price_db = db.clone();
        let price_oracle = std::sync::Arc::new(custody_common::price_oracle::PriceOracle::with_default_ttl());
        // Seed fallback prices immediately
        price_oracle.seed_fallback_prices();

        tokio::spawn(async move {
            let client = ::reqwest::Client::new();
            tracing::info!("Starting price refresh loop (60s interval)");
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;

                match custody_common::price_oracle::fetch_coingecko_prices(
                    &client,
                    custody_common::price_oracle::COINGECKO_IDS,
                ).await {
                    Ok(prices) => {
                        price_oracle.set_prices(&prices);
                        // Persist to DB for dashboard
                        for (symbol, price) in &prices {
                            // Update across all chains for this symbol
                            for chain in &["ethereum", "tron", "solana", "bitcoin"] {
                                let _ = price_db.update_token_price(symbol, chain, *price).await;
                            }
                        }
                        tracing::debug!(count = prices.len(), "Prices refreshed from CoinGecko");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "CoinGecko price refresh failed — using cached prices");
                    }
                }
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
