//! HTTP server for MPC multi-host ceremonies.
//!
//! Each party in a multi-host MPC session runs this server on a known
//! port. Peer parties POST `MpcMessage` JSON bodies to `/mpc/message`;
//! we verify the HMAC and deposit the message into the party's
//! `HttpTransport` inbox so the ceremony loop can pull it on the next
//! `receive()` call.
//!
//! Before this module, `HttpTransport::send()` could reach peers but
//! nothing on the receiving end accepted the delivery — the "multi-host
//! ceremony" was a one-way street. With this server + the existing
//! transport, a real multi-host CGGMP21 / FROST round can complete.
//!
//! # Security
//! - HMAC verification is done by `deliver_to_inbox`; bad signatures
//!   produce 401 and never reach the inbox.
//! - Oversized payloads → 413.
//! - Health probe at `/mpc/health` is open (no auth) so an operator
//!   can confirm the party process is alive without knowing the HMAC.
//! - The server binds ONLY the MPC endpoints; do not expose this to
//!   the public internet. Deployments should run it on the Zone 2
//!   internal network, mutually reachable by peer parties.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};

use crate::http_transport::deliver_to_inbox;
use crate::transport::{MpcMessage, TransportError};

/// Shared state for the receive handler.
#[derive(Clone)]
struct ReceiverState {
    inbox: Arc<Mutex<Vec<MpcMessage>>>,
    hmac_secret: Arc<Vec<u8>>,
}

/// Build the MPC server router. Mounts:
///
///   POST /mpc/message   — peer-to-peer message delivery (HMAC-authed)
///   GET  /mpc/health    — liveness probe (unauthed)
pub fn mpc_router(
    inbox: Arc<Mutex<Vec<MpcMessage>>>,
    hmac_secret: Vec<u8>,
) -> Router {
    let state = ReceiverState {
        inbox,
        hmac_secret: Arc::new(hmac_secret),
    };
    Router::new()
        .route("/mpc/message", post(receive_message))
        .route("/mpc/health", get(health))
        .with_state(state)
}

/// Spawn an axum server serving the MPC router on `bind_addr`.
/// Returns a join handle so the caller can coordinate shutdown.
///
/// This is the counterpart to `HttpTransport::send()` on the peer
/// side — each party in a multi-host ceremony runs one.
pub async fn spawn_mpc_server(
    bind_addr: SocketAddr,
    inbox: Arc<Mutex<Vec<MpcMessage>>>,
    hmac_secret: Vec<u8>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let router = mpc_router(inbox, hmac_secret);
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("MPC server bind {bind_addr}: {e}"))?;
    tracing::info!(%bind_addr, "MPC HTTP receiver listening");

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(error = %e, "MPC HTTP server exited");
        }
    });
    Ok(handle)
}

async fn receive_message(
    State(state): State<ReceiverState>,
    Json(msg): Json<MpcMessage>,
) -> StatusCode {
    match deliver_to_inbox(&state.inbox, &state.hmac_secret, msg) {
        Ok(()) => StatusCode::OK,
        Err(TransportError::AuthenticationFailed) => {
            tracing::warn!("Rejected MPC message: HMAC verification failed");
            StatusCode::UNAUTHORIZED
        }
        Err(TransportError::MessageTooLarge) => {
            tracing::warn!("Rejected MPC message: payload too large");
            StatusCode::PAYLOAD_TOO_LARGE
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Unexpected error delivering MPC message");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn health() -> &'static str {
    "mpc-ok"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::sign_message;
    use axum::body::Body;
    use axum::http::Request;
    use chrono::Utc;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn sample_inbox() -> Arc<Mutex<Vec<MpcMessage>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    #[tokio::test]
    async fn health_returns_ok_body() {
        let router = mpc_router(sample_inbox(), b"k".to_vec());
        let resp = router
            .oneshot(Request::get("/mpc/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn message_with_valid_hmac_lands_in_inbox() {
        let secret = b"shared-secret".to_vec();
        let inbox = sample_inbox();
        let router = mpc_router(inbox.clone(), secret.clone());

        let mut msg = MpcMessage {
            session_id: Uuid::new_v4(),
            round: 1,
            from_party: 2,
            to_party: Some(0),
            payload: vec![0xBE, 0xEF],
            hmac: String::new(),
            timestamp: Utc::now(),
        };
        msg.hmac = sign_message(&secret, &msg);

        let body = serde_json::to_vec(&msg).unwrap();
        let req = Request::post("/mpc/message")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(inbox.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn message_with_bad_hmac_is_unauthorized() {
        let inbox = sample_inbox();
        let router = mpc_router(inbox.clone(), b"real".to_vec());

        let msg = MpcMessage {
            session_id: Uuid::new_v4(),
            round: 1,
            from_party: 2,
            to_party: Some(0),
            payload: vec![0xAA],
            // HMAC signed with a *different* key than the server has
            hmac: sign_message(b"wrong-key", &MpcMessage {
                session_id: Uuid::new_v4(),
                round: 1,
                from_party: 2,
                to_party: Some(0),
                payload: vec![0xAA],
                hmac: String::new(),
                timestamp: Utc::now(),
            }),
            timestamp: Utc::now(),
        };

        let body = serde_json::to_vec(&msg).unwrap();
        let req = Request::post("/mpc/message")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(inbox.lock().unwrap().len(), 0);
    }
}
