//! Probe logic vs vendor response shapes.
//!
//! Each probe in `custody_common::readiness` encodes specific
//! expectations about what a vendor returns: Bitcoin Core 401 means
//! "reachable + auth required" (positive signal), Java-Tron
//! `/wallet/getnowblock` must contain `block_header`, Solana
//! getHealth reports `"ok"` OR an error with `numSlotsBehind`, etc.
//! Unit tests in `readiness.rs` only cover error-on-unresolvable-host.
//! This file drives every probe against a real local HTTP server
//! that returns each representative vendor shape, so the
//! "interpretation of response" half of the probe is actually
//! exercised.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use custody_common::readiness::{
    probe_bitcoin_rpc, probe_ethereum_rpc, probe_http_reachable, probe_solana_rpc, probe_tron_api,
};
use std::sync::Arc;
use tokio::sync::Mutex;

type Shared = Arc<Mutex<serde_json::Value>>;

/// Spawn a test server returning the same body for any request.
async fn spawn_server(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://{addr}")
}

// ─── Ethereum eth_chainId ────────────────────────────────────────────

#[tokio::test]
async fn ethereum_rpc_ok_shape() {
    // Happy path: JSON-RPC returns a hex chainId.
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":"0x1"}))
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    assert!(probe_ethereum_rpc(&url).await.is_ok());
}

#[tokio::test]
async fn ethereum_rpc_missing_result_is_error() {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"jsonrpc":"2.0","id":1}))
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    let err = probe_ethereum_rpc(&url).await.unwrap_err();
    assert!(err.contains("result missing"), "got: {err}");
}

#[tokio::test]
async fn ethereum_rpc_error_field_is_error() {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "jsonrpc":"2.0","id":1,
            "error":{"code":-32601,"message":"method not found"},
        }))
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    let err = probe_ethereum_rpc(&url).await.unwrap_err();
    assert!(err.contains("RPC error"), "got: {err}");
}

#[tokio::test]
async fn ethereum_rpc_http_5xx_is_error() {
    async fn handler() -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    let err = probe_ethereum_rpc(&url).await.unwrap_err();
    assert!(err.contains("HTTP 500"), "got: {err}");
}

// ─── Bitcoin Core getblockchaininfo ──────────────────────────────────

#[tokio::test]
async fn bitcoin_rpc_ok_shape() {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "result": {"chain":"main","blocks":820000},
            "error": serde_json::Value::Null,
            "id": "probe",
        }))
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    assert!(probe_bitcoin_rpc(&url).await.is_ok());
}

#[tokio::test]
async fn bitcoin_rpc_401_is_reachable() {
    // Bitcoin Core without basic-auth returns 401. That's our
    // network-level positive signal — the daemon is there, auth is
    // just missing, which the operator knows about.
    async fn handler() -> StatusCode {
        StatusCode::UNAUTHORIZED
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    assert!(probe_bitcoin_rpc(&url).await.is_ok(), "401 must count as reachable");
}

#[tokio::test]
async fn bitcoin_rpc_error_field_is_error() {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "result": serde_json::Value::Null,
            "error": {"code":-32601,"message":"method not found"},
            "id": "probe",
        }))
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    let err = probe_bitcoin_rpc(&url).await.unwrap_err();
    assert!(err.contains("RPC error"), "got: {err}");
}

#[tokio::test]
async fn bitcoin_rpc_503_is_error() {
    // Mid-sync or dead daemon — not reachable for probe purposes.
    async fn handler() -> StatusCode {
        StatusCode::SERVICE_UNAVAILABLE
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    let err = probe_bitcoin_rpc(&url).await.unwrap_err();
    assert!(err.contains("HTTP 503"), "got: {err}");
}

// ─── Tron /wallet/getnowblock ────────────────────────────────────────

#[tokio::test]
async fn tron_api_ok_shape() {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "block_header": {
                "raw_data": {"number": 55_000_000, "txTrieRoot": "…"},
            },
        }))
    }
    let url = spawn_server(Router::new().route("/wallet/getnowblock", post(handler))).await;
    assert!(probe_tron_api(&url).await.is_ok());
}

#[tokio::test]
async fn tron_api_missing_block_header_is_error() {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"Error": "not ready"}))
    }
    let url = spawn_server(Router::new().route("/wallet/getnowblock", post(handler))).await;
    let err = probe_tron_api(&url).await.unwrap_err();
    assert!(err.contains("block_header"), "got: {err}");
}

#[tokio::test]
async fn tron_api_strips_trailing_slash() {
    // base_url ending with `/` shouldn't double-slash the path.
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"block_header": {"raw_data": {}}}))
    }
    let url = spawn_server(Router::new().route("/wallet/getnowblock", post(handler))).await;
    let url_with_slash = format!("{url}/");
    assert!(probe_tron_api(&url_with_slash).await.is_ok());
}

// ─── Solana getHealth ────────────────────────────────────────────────

#[tokio::test]
async fn solana_rpc_ok_when_result_is_ok() {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":"ok"}))
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    assert!(probe_solana_rpc(&url).await.is_ok());
}

#[tokio::test]
async fn solana_rpc_error_with_num_slots_behind_is_degraded_reason() {
    // Mid-sync: JSON-RPC returns an error with slot-behind count.
    // Probe returns Err so readiness reports Degraded with the
    // operator-readable reason.
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "jsonrpc":"2.0","id":1,
            "error": {
                "code":-32005,
                "message":"Node is unhealthy",
                "data":{"numSlotsBehind":1234},
            },
        }))
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    let err = probe_solana_rpc(&url).await.unwrap_err();
    assert!(err.contains("Node is unhealthy"), "got: {err}");
    assert!(err.contains("numSlotsBehind"), "got: {err}");
}

#[tokio::test]
async fn solana_rpc_unexpected_shape_is_error() {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"unexpected":"thing"}))
    }
    let url = spawn_server(Router::new().route("/", post(handler))).await;
    let err = probe_solana_rpc(&url).await.unwrap_err();
    assert!(err.contains("unexpected response"), "got: {err}");
}

// ─── probe_http_reachable (Chainalysis / Elliptic / Notabene) ────────

#[tokio::test]
async fn http_reachable_200_is_ok() {
    async fn handler() -> StatusCode {
        StatusCode::OK
    }
    let url = spawn_server(Router::new().route("/", get(handler))).await;
    assert!(probe_http_reachable(&url).await.is_ok());
}

#[tokio::test]
async fn http_reachable_401_counts_as_reachable() {
    // Vendors that require auth for HEAD respond 401 without
    // credentials. That's still a positive signal — we reach them.
    async fn handler() -> StatusCode {
        StatusCode::UNAUTHORIZED
    }
    let url = spawn_server(Router::new().route("/", get(handler))).await;
    assert!(probe_http_reachable(&url).await.is_ok(), "401 = reachable");
}

#[tokio::test]
async fn http_reachable_403_counts_as_reachable() {
    async fn handler() -> StatusCode {
        StatusCode::FORBIDDEN
    }
    let url = spawn_server(Router::new().route("/", get(handler))).await;
    assert!(probe_http_reachable(&url).await.is_ok(), "403 = reachable");
}

#[tokio::test]
async fn http_reachable_404_counts_as_reachable() {
    // Ambiguous ("endpoint gone" vs. "wrong URL") but the host is
    // answering, which is what probe_http_reachable tests.
    async fn handler() -> StatusCode {
        StatusCode::NOT_FOUND
    }
    let url = spawn_server(Router::new().route("/", get(handler))).await;
    assert!(probe_http_reachable(&url).await.is_ok());
}

#[tokio::test]
async fn http_reachable_5xx_is_error() {
    async fn handler() -> StatusCode {
        StatusCode::BAD_GATEWAY
    }
    let url = spawn_server(Router::new().route("/", get(handler))).await;
    let err = probe_http_reachable(&url).await.unwrap_err();
    assert!(err.contains("502"), "got: {err}");
}

#[tokio::test]
async fn http_reachable_300_redirects_are_reachable() {
    async fn handler() -> (StatusCode, [(&'static str, &'static str); 1]) {
        (StatusCode::FOUND, [("location", "/next")])
    }
    let url = spawn_server(Router::new().route("/", get(handler))).await;
    // reqwest follows redirects by default, so hitting "/" → 302 →
    // "/next" (404). 404 is in our "reachable" allowlist, so OK.
    assert!(probe_http_reachable(&url).await.is_ok());
}

// ─── Probes are bounded by PROBE_TIMEOUT ─────────────────────────────

#[tokio::test]
async fn probe_honours_timeout_on_slow_server() {
    // Server that hangs forever on probe path.
    async fn slow(State(_): State<Shared>) -> Json<serde_json::Value> {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        Json(serde_json::json!({}))
    }
    let state: Shared = Arc::new(Mutex::new(serde_json::json!({})));
    let url = spawn_server(
        Router::new()
            .route("/", post(slow))
            .with_state(state),
    )
    .await;

    let started = std::time::Instant::now();
    let res = probe_ethereum_rpc(&url).await;
    let elapsed = started.elapsed();
    assert!(res.is_err(), "timeout must be an error");
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "probe must return within ~5s (PROBE_TIMEOUT); took {elapsed:?}"
    );
}
