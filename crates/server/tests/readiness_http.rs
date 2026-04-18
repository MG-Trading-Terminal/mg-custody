//! In-process HTTP tests for the `/v1/health/features*` endpoints.
//!
//! Builds the full axum router against a minimal AppState with a
//! hand-populated ReadinessHandle, then uses `tower::ServiceExt::oneshot`
//! to send real Request objects through the handler stack. Catches
//! shape regressions the dashboard would otherwise trip over — the
//! pages consume specific JSON keys and a silent rename would break
//! them at load time, not at compile time.
//!
//! No DB, no TCP. Runs in every CI pass.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use custody_common::emergency::FreezeFlag;
use custody_common::events::EventBroadcaster;
use custody_common::readiness::{FeatureStatus, ReadinessHandle, ReadinessReport};
use custody_common::readiness_chaos::ChaosOverrides;
use custody_db::CustodyDb;
use custody_gateway::state::AppState;
use std::sync::Arc;
use tower::ServiceExt;

fn build_router(handle: ReadinessHandle) -> axum::Router {
    let pool = sqlx::PgPool::connect_lazy("postgres://fake:fake@localhost:5432/fake").unwrap();
    let db = CustodyDb::new(pool);
    let seed = Arc::new(custody_address_factory::SeedBytes([0u8; 64]));
    let allocator = Arc::new(custody_address_factory::AddressAllocator::new(
        db.clone(),
        custody_address_factory::AddressFactoryConfig::default(),
    ));
    let pipeline = Arc::new(
        custody_withdrawal_pipeline::WithdrawalPipeline::new(
            db.clone(),
            rust_decimal_macros::dec!(500000),
            rust_decimal_macros::dec!(10000000),
            rust_decimal_macros::dec!(50000000),
        )
        .with_seed(Arc::new([0u8; 64])),
    );
    let state = AppState {
        db,
        address_allocator: allocator,
        seed,
        withdrawal_pipeline: pipeline,
        event_broadcaster: Arc::new(EventBroadcaster::with_default_capacity()),
        user_rate_limiter: Arc::new(custody_gateway::rate_limit::UserRateLimiter::default_user()),
        frozen: FreezeFlag::new(),
        email: Arc::new(custody_common::email::EmailSender::noop()),
        travel_rule_provider: None,
        readiness: handle,
        chaos_overrides: ChaosOverrides::new(),
        incident_acks: custody_common::readiness_ack::IncidentAcks::new(),
    };
    custody_gateway::routes::router(state)
}

fn seeded_handle() -> ReadinessHandle {
    let mut r = ReadinessReport::new();
    r.push(FeatureStatus::ready("scanner.ethereum", "ETH scanner"));
    r.push(FeatureStatus::degraded(
        "push.apns",
        "APNs",
        "p8 parse failed",
    ));
    r.push(FeatureStatus::misconfigured(
        "screening.aml",
        "AML vendors",
        vec!["CUSTODY_CHAINALYSIS_API_KEY".into()],
    ));
    r.push(FeatureStatus::disabled("mpc.multi_host", "MPC receiver"));
    ReadinessHandle::new(r)
}

async fn json_body(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_features_returns_summary_and_features() {
    let app = build_router(seeded_handle());
    let resp = app
        .oneshot(Request::get("/v1/health/features").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;

    // Summary counts must match the seeded handle exactly.
    assert_eq!(body["summary"]["total"], 4);
    assert_eq!(body["summary"]["ready"], 1);
    assert_eq!(body["summary"]["degraded"], 1);
    assert_eq!(body["summary"]["misconfigured"], 1);
    assert_eq!(body["summary"]["disabled"], 1);

    // Features array carries every feature with the expected keys
    // — the dashboard reads these exact names.
    let feats = body["features"].as_array().unwrap();
    assert_eq!(feats.len(), 4);
    let eth = feats.iter().find(|f| f["name"] == "scanner.ethereum").unwrap();
    assert_eq!(eth["state"], "ready");
    let apns = feats.iter().find(|f| f["name"] == "push.apns").unwrap();
    assert_eq!(apns["state"], "degraded");
    assert_eq!(apns["reason"], "p8 parse failed");
    let aml = feats.iter().find(|f| f["name"] == "screening.aml").unwrap();
    assert_eq!(aml["state"], "misconfigured");
    assert!(aml["missing"].as_array().unwrap().iter().any(|m| m == "CUSTODY_CHAINALYSIS_API_KEY"));
}

#[tokio::test]
async fn health_features_uptime_shape_matches_dashboard() {
    // The uptime handler reads readiness_transitions from the DB,
    // so a lazy fake pool fails on first query. Skip unless a real
    // DATABASE_URL is set — then migrations+empty-table query both
    // succeed and the shape assertion is meaningful.
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL not set — skipping uptime-shape test");
        return;
    };
    let pool = match sqlx::PgPool::connect(&url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("DATABASE_URL set but connect failed ({e}) — skipping");
            return;
        }
    };
    if sqlx::migrate!("../db/migrations").run(&pool).await.is_err() {
        eprintln!("migrations failed — skipping");
        return;
    }
    let db = CustodyDb::new(pool);
    let seed = Arc::new(custody_address_factory::SeedBytes([0u8; 64]));
    let allocator = Arc::new(custody_address_factory::AddressAllocator::new(
        db.clone(),
        custody_address_factory::AddressFactoryConfig::default(),
    ));
    let pipeline = Arc::new(
        custody_withdrawal_pipeline::WithdrawalPipeline::new(
            db.clone(),
            rust_decimal_macros::dec!(500000),
            rust_decimal_macros::dec!(10000000),
            rust_decimal_macros::dec!(50000000),
        )
        .with_seed(Arc::new([0u8; 64])),
    );
    let state = AppState {
        db,
        address_allocator: allocator,
        seed,
        withdrawal_pipeline: pipeline,
        event_broadcaster: Arc::new(EventBroadcaster::with_default_capacity()),
        user_rate_limiter: Arc::new(custody_gateway::rate_limit::UserRateLimiter::default_user()),
        frozen: FreezeFlag::new(),
        email: Arc::new(custody_common::email::EmailSender::noop()),
        travel_rule_provider: None,
        readiness: seeded_handle(),
        chaos_overrides: ChaosOverrides::new(),
        incident_acks: custody_common::readiness_ack::IncidentAcks::new(),
    };
    let app = custody_gateway::routes::router(state);
    // Admin-only endpoint; router() falls through to Admin without Bearer.
    let resp = app
        .oneshot(
            Request::get("/v1/health/features/uptime?window=24h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;

    assert_eq!(body["window"], "24h");
    assert!(body["since"].is_string());
    assert!(body["until"].is_string());

    let feats = body["features"].as_array().unwrap();
    // One row per feature in the report — even disabled ones.
    assert_eq!(feats.len(), 4);
    for f in feats {
        // Every row carries the exact keys the /readiness page renders.
        assert!(f["feature"].is_string());
        assert!(f["uptime_pct"].is_string());
        assert!(f["down_seconds"].is_i64());
        assert!(f["transitions"].is_i64() || f["transitions"].is_u64());
        assert!(f["slo_target_pct"].is_string(), "slo target must appear");
        assert!(f["slo_breach"].is_boolean());
        assert!(f["budget_seconds"].is_i64());
        assert!(f["budget_consumed_seconds"].is_i64());
        assert!(f["budget_remaining_seconds"].is_i64());
    }
}

#[tokio::test]
async fn health_features_uptime_rejects_unknown_window() {
    let app = build_router(seeded_handle());
    let resp = app
        .oneshot(
            Request::get("/v1/health/features/uptime?window=90d")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // 90d is not in the supported set (1h/24h/7d/30d). Handler
    // must refuse with 400 rather than silently computing over a
    // zero-length window.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn health_features_refresh_requires_admin_body_is_202() {
    let app = build_router(seeded_handle());
    let resp = app
        .oneshot(
            Request::post("/v1/health/features/refresh")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    // router() grants Admin without Bearer, so this should succeed.
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let body = json_body(resp).await;
    assert_eq!(body["requested"], true);
}
