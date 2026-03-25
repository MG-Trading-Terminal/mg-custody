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
        .connect(&db_url)
        .await;

    let db = match pool {
        Ok(pool) => {
            tracing::info!("Connected to PostgreSQL");

            // Run migrations
            let migration_sql = include_str!("../../db/migrations/001_initial_schema.sql");
            if let Err(e) = sqlx::raw_sql(migration_sql).execute(&pool).await {
                tracing::warn!(error = %e, "Migration warning (tables may already exist)");
            } else {
                tracing::info!("Database migrations applied");
            }

            CustodyDb::new(pool)
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to connect to PostgreSQL");
            tracing::warn!("Running without database — most operations will fail");
            let pool = sqlx::PgPool::connect_lazy(&db_url)?;
            CustodyDb::new(pool)
        }
    };

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

    // ─── Application State ───────────────────────────────────────────
    let app_state = AppState {
        db: db.clone(),
        address_allocator,
        seed,
        withdrawal_pipeline: withdrawal_pipeline.clone(),
        event_broadcaster: event_broadcaster.clone(),
    };

    // Use middleware if HMAC secret is configured
    let hmac_secret = std::env::var("CUSTODY_HMAC_SECRET")
        .unwrap_or_else(|_| "".to_string());

    let app = if hmac_secret.is_empty() {
        tracing::warn!("No HMAC_SECRET — running WITHOUT auth middleware (dev mode)");
        let mut r = custody_gateway::routes::router(app_state.clone());
        r = r.merge(custody_gateway::admin_routes::admin_router(app_state.clone()));
        r = r.merge(custody_gateway::exchange_routes::exchange_router(app_state));
        r
    } else {
        tracing::info!("HMAC auth middleware ENABLED");
        let mut r = custody_gateway::routes::router_with_middleware(app_state.clone(), hmac_secret.as_bytes());
        r = r.merge(custody_gateway::admin_routes::admin_router(app_state.clone()));
        r = r.merge(custody_gateway::exchange_routes::exchange_router(app_state));
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
                match scanner.scan(from_block).await {
                    Ok((deposits, next_block)) => {
                        if !deposits.is_empty() {
                            tracing::info!(
                                count = deposits.len(),
                                from = from_block,
                                to = next_block,
                                "New deposits detected"
                            );
                        }
                        if next_block > from_block {
                            let _ = db_scan.set_scan_progress(Chain::Ethereum, next_block).await;
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

    // Confirmation tracker loop (every 12 seconds)
    {
        let tracker = confirmation_tracker.clone();
        tokio::spawn(async move {
            tracing::info!("Starting confirmation tracker loop");
            if let Err(e) = tracker.run(Duration::from_secs(12)).await {
                tracing::error!(error = %e, "Confirmation tracker failed");
            }
        });
    }

    // Withdrawal pipeline loop (every 5 seconds)
    {
        let pipeline = withdrawal_pipeline.clone();
        tokio::spawn(async move {
            tracing::info!("Starting withdrawal pipeline loop");
            if let Err(e) = pipeline.run(Duration::from_secs(5)).await {
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
                tokio::time::sleep(Duration::from_secs(30)).await;
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
            if let Err(e) = service.run(Duration::from_secs(600)).await {
                tracing::error!(error = %e, "Reconciliation service failed");
            }
        });
    }

    // ─── HTTP Server with graceful shutdown ─────────────────────────
    let bind_addr = &config.api.bind;
    tracing::info!(
        addr = %bind_addr,
        background_tasks = 8,
        "Custody service ready — listening"
    );

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    // Graceful shutdown: wait for SIGTERM/SIGINT
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
        tracing::info!("Shutting down gracefully...");
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    tracing::info!("Custody service stopped");
    Ok(())
}
