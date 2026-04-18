//! End-to-end integration test for the readiness alert pipeline.
//!
//! Spins up a tiny axum server on a random port that captures every
//! incoming POST into a shared Vec. Points
//! CUSTODY_READINESS__ALERT_WEBHOOK at it, then drives
//! `fire_alerts` with synthetic transitions and asserts the
//! captured payloads match expectations.
//!
//! The point is to prove the full chain actually sends what we
//! think it sends — batching, silence filtering, cooldown
//! suppression, HMAC header emission — end-to-end, without mocking
//! reqwest. When this test passes, we know that fire_alerts
//! genuinely produces HTTP traffic on the wire.

use axum::{extract::State, routing::post, Json, Router};
use custody_server::readiness_build::{fire_alerts, AlertState, Transition};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
struct Captured {
    bodies: Arc<Mutex<Vec<serde_json::Value>>>,
    headers: Arc<Mutex<Vec<Vec<(String, String)>>>>,
}

async fn capture_handler(
    State(captured): State<Captured>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> axum::http::StatusCode {
    let header_pairs = headers
        .iter()
        .map(|(n, v)| (n.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    captured.bodies.lock().await.push(body);
    captured.headers.lock().await.push(header_pairs);
    axum::http::StatusCode::OK
}

/// Start a capture server on an ephemeral port, return its URL and
/// the shared state the test drains assertions from.
async fn spawn_capture() -> (String, Captured) {
    let captured = Captured::default();
    let app = Router::new()
        .route("/capture", post(capture_handler))
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    // A tiny sleep to let the listener accept.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (format!("http://{addr}/capture"), captured)
}

/// Snapshot env var state for one feature so concurrent tests
/// don't race on the shared process env. Each test in this file
/// sets a unique CUSTODY_READINESS__ALERT_WEBHOOK value; they
/// serialize via this mutex.
static ENV_LOCK: Mutex<()> = Mutex::const_new(());

#[tokio::test]
async fn degradation_fires_one_webhook_with_expected_shape() {
    let _guard = ENV_LOCK.lock().await;
    let (url, captured) = spawn_capture().await;
    // SAFETY: `set_var` is unsafe in Rust 2024 because mutating the
    // process env is inherently racy; we serialise every test in
    // this file behind ENV_LOCK, and no other test in the workspace
    // touches these vars.
    unsafe {
        std::env::set_var("CUSTODY_READINESS__ALERT_WEBHOOK", &url);
        std::env::remove_var("CUSTODY_READINESS__ALERT_WEBHOOK_SECRET");
        std::env::remove_var("CUSTODY_READINESS__ALERT_SLACK_WEBHOOK");
        std::env::remove_var("CUSTODY_READINESS__ALERT_PAGERDUTY_ROUTING_KEY");
    }

    let mut alerts = AlertState::default();
    let silenced: Vec<String> = vec![];
    let transitions = vec![Transition::Degradation {
        feature: "scanner.ethereum",
        reason: "eth_chainId timeout".into(),
    }];
    fire_alerts(&mut alerts, &silenced, &transitions).await;

    // Give the POST a moment to arrive.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let bodies = captured.bodies.lock().await.clone();
    assert_eq!(bodies.len(), 1, "exactly one batched POST");
    let body = &bodies[0];
    assert_eq!(body["service"], "mg-custody");
    let trs = body["transitions"].as_array().expect("transitions array");
    assert_eq!(trs.len(), 1);
    assert_eq!(trs[0]["kind"], "degradation");
    assert_eq!(trs[0]["feature"], "scanner.ethereum");
    assert_eq!(trs[0]["reason"], "eth_chainId timeout");
}

#[tokio::test]
async fn batches_multiple_transitions_into_single_post() {
    let _guard = ENV_LOCK.lock().await;
    let (url, captured) = spawn_capture().await;
    unsafe {
        std::env::set_var("CUSTODY_READINESS__ALERT_WEBHOOK", &url);
        std::env::remove_var("CUSTODY_READINESS__ALERT_WEBHOOK_SECRET");
        std::env::remove_var("CUSTODY_READINESS__ALERT_SLACK_WEBHOOK");
        std::env::remove_var("CUSTODY_READINESS__ALERT_PAGERDUTY_ROUTING_KEY");
    }

    let mut alerts = AlertState::default();
    let silenced: Vec<String> = vec![];
    let transitions = vec![
        Transition::Degradation {
            feature: "scanner.ethereum",
            reason: "timeout".into(),
        },
        Transition::Degradation {
            feature: "scanner.bitcoin",
            reason: "unreachable".into(),
        },
        Transition::Recovery {
            feature: "push.fcm",
            summary: "FCM back",
        },
    ];
    fire_alerts(&mut alerts, &silenced, &transitions).await;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let bodies = captured.bodies.lock().await.clone();
    assert_eq!(bodies.len(), 1, "one POST, not three");
    let trs = bodies[0]["transitions"].as_array().unwrap();
    assert_eq!(trs.len(), 3, "all three transitions in the batch");
}

#[tokio::test]
async fn silence_fully_suppresses_degradation() {
    let _guard = ENV_LOCK.lock().await;
    let (url, captured) = spawn_capture().await;
    unsafe {
        std::env::set_var("CUSTODY_READINESS__ALERT_WEBHOOK", &url);
        std::env::remove_var("CUSTODY_READINESS__ALERT_WEBHOOK_SECRET");
        std::env::remove_var("CUSTODY_READINESS__ALERT_SLACK_WEBHOOK");
        std::env::remove_var("CUSTODY_READINESS__ALERT_PAGERDUTY_ROUTING_KEY");
    }

    let mut alerts = AlertState::default();
    let silenced = vec!["scanner.ethereum".to_string()];
    let transitions = vec![Transition::Degradation {
        feature: "scanner.ethereum",
        reason: "doesn't matter".into(),
    }];
    fire_alerts(&mut alerts, &silenced, &transitions).await;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let bodies = captured.bodies.lock().await.clone();
    assert!(
        bodies.is_empty(),
        "silenced feature → no POST at all (not just empty batch)"
    );
}

#[tokio::test]
async fn cooldown_suppresses_second_fire_inside_window() {
    let _guard = ENV_LOCK.lock().await;
    let (url, captured) = spawn_capture().await;
    unsafe {
        std::env::set_var("CUSTODY_READINESS__ALERT_WEBHOOK", &url);
        std::env::remove_var("CUSTODY_READINESS__ALERT_WEBHOOK_SECRET");
        std::env::remove_var("CUSTODY_READINESS__ALERT_SLACK_WEBHOOK");
        std::env::remove_var("CUSTODY_READINESS__ALERT_PAGERDUTY_ROUTING_KEY");
    }

    let mut alerts = AlertState::default();
    let silenced: Vec<String> = vec![];
    let transitions = vec![Transition::Degradation {
        feature: "scanner.ethereum",
        reason: "first".into(),
    }];
    // First call: fires.
    fire_alerts(&mut alerts, &silenced, &transitions).await;
    // Second call within cooldown: should be suppressed.
    fire_alerts(&mut alerts, &silenced, &transitions).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let bodies = captured.bodies.lock().await.clone();
    assert_eq!(
        bodies.len(),
        1,
        "cooldown kept the second batch from being sent"
    );
}

#[tokio::test]
async fn hmac_signature_header_emitted_when_secret_set() {
    let _guard = ENV_LOCK.lock().await;
    let (url, captured) = spawn_capture().await;
    unsafe {
        std::env::set_var("CUSTODY_READINESS__ALERT_WEBHOOK", &url);
        std::env::set_var(
            "CUSTODY_READINESS__ALERT_WEBHOOK_SECRET",
            "test-secret-abc123",
        );
        std::env::remove_var("CUSTODY_READINESS__ALERT_SLACK_WEBHOOK");
        std::env::remove_var("CUSTODY_READINESS__ALERT_PAGERDUTY_ROUTING_KEY");
    }

    let mut alerts = AlertState::default();
    let silenced: Vec<String> = vec![];
    let transitions = vec![Transition::Degradation {
        feature: "scanner.ethereum",
        reason: "r".into(),
    }];
    fire_alerts(&mut alerts, &silenced, &transitions).await;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let headers = captured.headers.lock().await.clone();
    assert_eq!(headers.len(), 1);
    let sig = headers[0]
        .iter()
        .find(|(n, _)| n == "x-custody-signature")
        .expect("X-Custody-Signature must be present when secret configured");
    assert!(sig.1.starts_with("t="), "signature must include timestamp");
    assert!(sig.1.contains(",v1="), "signature must carry v1 hex");

    unsafe {
        std::env::remove_var("CUSTODY_READINESS__ALERT_WEBHOOK_SECRET");
    }
}
