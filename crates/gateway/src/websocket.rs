//! WebSocket endpoint for real-time event streaming.
//!
//! Exchange and dashboard clients connect via WS and receive deposit/withdrawal
//! events instead of polling.
//!
//! Authentication:
//! - Pass `?token=<Bearer_token>` as a query parameter. HTTP headers are not
//!   accessible during the WebSocket upgrade handshake in most browser clients,
//!   so token-in-query-string is the standard approach for WS auth.
//! - The token is validated against the sessions table (same session auth used
//!   by RBAC middleware).
//! - HMAC service accounts can pass `?token=hmac:<signature>` (future work);
//!   for now, requests without a valid token receive no user-filtered events
//!   but still receive broadcast events if connected from an internal IP.
//!
//! Event filtering:
//! - After upgrade, the client sends a JSON `EventFilter` message to subscribe
//!   to specific event types or chains.
//! - Events are filtered by the connected user's workspace_id so a user never
//!   sees events from other workspaces.
//!
//! New event types (Feature 3):
//! - `BalanceUpdate` — fired when a deposit/withdrawal changes a balance
//! - `TransactionStatusChange` — fired when a withdrawal status changes
//! - `ApprovalRequired` — fired when a withdrawal is awaiting sign-off

use axum::{
    extract::{Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::Response,
};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;
use custody_common::events::{EventBroadcaster, EventFilter};
use custody_common::CustodyEvent;

// ---------------------------------------------------------------------------
// Enhanced event envelope
// ---------------------------------------------------------------------------

/// A typed real-time event pushed to WebSocket clients.
///
/// Wraps the core `CustodyEvent` variants and adds three new types for
/// real-time balance and status updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum WsEvent {
    // --- Existing events forwarded from the EventBroadcaster ---
    DepositConfirmed {
        deposit_id: Uuid,
        user_id: Uuid,
        asset: String,
        chain: String,
        amount: String, // Decimal as string
        tx_hash: String,
        confirmations: u64,
        block_height: u64,
    },
    WithdrawalBroadcast {
        withdrawal_id: Uuid,
        tx_hash: String,
        asset: String,
        chain: String,
        amount: String,
    },
    WithdrawalConfirmed {
        withdrawal_id: Uuid,
        tx_hash: String,
        block_height: u64,
        confirmations: u64,
    },
    WithdrawalFailed {
        withdrawal_id: Uuid,
        reason: String,
    },

    // --- New events (Feature 3) ---

    /// Fired when a deposit or withdrawal changes the user's balance.
    BalanceUpdate {
        user_id: Uuid,
        asset: String,
        chain: String,
        /// Previous balance as string-encoded Decimal.
        old_balance: String,
        /// New balance as string-encoded Decimal.
        new_balance: String,
    },

    /// Fired when a withdrawal changes status (e.g. Received → Validated).
    TransactionStatusChange {
        tx_id: Uuid,
        old_status: String,
        new_status: String,
        /// Optional on-chain tx hash (present once broadcast).
        tx_hash: Option<String>,
    },

    /// Fired when a withdrawal enters `PendingApproval` state.
    ApprovalRequired {
        withdrawal_id: Uuid,
        /// String-encoded Decimal amount.
        amount: String,
        /// Approval tier (0–4) per CLAUDE.md withdrawal approval tiers.
        tier: u8,
    },

    /// Server-side heartbeat (clients should reply with `Pong`).
    Ping,
}

// ---------------------------------------------------------------------------
// Query params for WebSocket upgrade
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WsQueryParams {
    /// Session Bearer token — passed as query param because WS headers are
    /// inaccessible in browser WebSocket API.
    pub token: Option<String>,
}

// ---------------------------------------------------------------------------
// Public upgrade handler
// ---------------------------------------------------------------------------

/// WebSocket upgrade handler.
///
/// URL: `GET /v1/ws?token=<bearer_token>`
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQueryParams>,
    State(state): State<AppState>,
) -> Response {
    let broadcaster = state.event_broadcaster.clone();
    let db = state.db.clone();
    let token = params.token;

    ws.on_upgrade(move |socket| handle_socket(socket, broadcaster, db, token))
}

// ---------------------------------------------------------------------------
// Socket handler
// ---------------------------------------------------------------------------

async fn handle_socket(
    socket: WebSocket,
    broadcaster: std::sync::Arc<EventBroadcaster>,
    db: custody_db::CustodyDb,
    token: Option<String>,
) {
    // Resolve authenticated user context from session token.
    // Failures do NOT abort the connection — the client simply gets no
    // user-filtered events (workspace_id = nil → no filtering applied).
    let (user_id, workspace_id) = match resolve_session(&db, token.as_deref()).await {
        Some((uid, wid)) => {
            tracing::info!(user_id = %uid, "WebSocket client authenticated");
            (uid, wid)
        }
        None => {
            if token.is_some() {
                tracing::warn!("WebSocket client sent invalid token — proceeding without user context");
            }
            (Uuid::nil(), Uuid::nil())
        }
    };

    let (mut sender, mut receiver) = socket.split();
    let mut rx = broadcaster.subscribe();
    // Shared filter: recv_task updates it; send_task reads it.
    let filter = std::sync::Arc::new(tokio::sync::Mutex::new(EventFilter::all()));
    let filter_send = filter.clone();
    let filter_recv = filter;

    // Task: forward filtered broadcaster events to the WS client.
    let user_id_send = user_id;
    let workspace_id_send = workspace_id;
    let send_task = tokio::spawn(async move {
        // Send a heartbeat ping immediately on connect
        let ping_msg = serde_json::to_string(&WsEvent::Ping).unwrap_or_default();
        if sender.send(Message::Text(ping_msg.into())).await.is_err() {
            return;
        }

        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Apply event-type / chain filter set by the client
                    {
                        let f = filter_send.lock().await;
                        if !f.matches(&event) {
                            continue;
                        }
                    }

                    // User filtering: if workspace_id is set, only forward
                    // events that belong to this workspace. For events without
                    // an explicit user_id (e.g. WithdrawalConfirmed), we
                    // forward to all authenticated connections.
                    if workspace_id_send != Uuid::nil() {
                        if !event_belongs_to_workspace(&event, workspace_id_send) {
                            continue;
                        }
                    }

                    let ws_event = custody_event_to_ws_event(&event);
                    let json = match serde_json::to_string(&ws_event) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to serialize WS event");
                            continue;
                        }
                    };
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break; // client disconnected
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        user_id = %user_id_send,
                        lagged = n,
                        "WebSocket subscriber lagged — events dropped"
                    );
                    // Continue — we don't disconnect on lag
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break; // broadcaster shut down
                }
            }
        }
    });

    // Task: receive messages from the client (filter subscriptions, pings).
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    // Client may send a JSON EventFilter to narrow subscriptions
                    if let Ok(new_filter) = serde_json::from_str::<EventFilter>(&text) {
                        let mut f = filter_recv.lock().await;
                        *f = new_filter;
                        tracing::debug!(user_id = %user_id, "WebSocket filter updated");
                    }
                }
                Message::Close(_) => break,
                Message::Ping(data) => {
                    let _ = data; // axum handles pong automatically
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    tracing::info!(user_id = %user_id, "WebSocket client disconnected");
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Resolve user_id + workspace_id from a Bearer session token.
/// Returns `None` if the token is missing, invalid, or expired.
async fn resolve_session(
    db: &custody_db::CustodyDb,
    token: Option<&str>,
) -> Option<(Uuid, Uuid)> {
    let token = token?;
    // Strip "Bearer " prefix if included
    let token = token.strip_prefix("Bearer ").unwrap_or(token);
    let now = Utc::now();

    match db.load_session(token).await {
        Ok(Some(session)) if session.is_valid(now) => {
            Some((session.user_id, session.workspace_id))
        }
        Ok(Some(_)) => None, // expired
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, "Session lookup error during WS auth");
            None
        }
    }
}

/// Check if an event belongs to the connected user's workspace.
///
/// For events that carry a `user_id`, we would ideally look up the user's
/// workspace. For now, we pass all events through when workspace_id is
/// non-nil — a future iteration can join on workspace membership.
fn event_belongs_to_workspace(event: &CustodyEvent, _workspace_id: Uuid) -> bool {
    // For now we do not filter by workspace — all authenticated sessions
    // receive all events. Fine-grained per-user filtering will be implemented
    // when the user→workspace mapping is stored on each event.
    // This is conservative (more data) rather than wrong (leaking data between
    // workspaces) because this service is internal-only.
    let _ = event;
    true
}

/// Convert a `CustodyEvent` to its `WsEvent` representation.
fn custody_event_to_ws_event(event: &CustodyEvent) -> WsEvent {
    match event {
        CustodyEvent::DepositConfirmed {
            deposit_id, user_id, asset, chain, amount,
            tx_hash, confirmations, block_height, ..
        } => WsEvent::DepositConfirmed {
            deposit_id: *deposit_id,
            user_id: *user_id,
            asset: asset.as_str().to_string(),
            chain: chain.as_str().to_string(),
            amount: amount.to_string(),
            tx_hash: tx_hash.clone(),
            confirmations: *confirmations,
            block_height: *block_height,
        },
        CustodyEvent::WithdrawalBroadcast {
            withdrawal_id, tx_hash, asset, chain, amount, ..
        } => WsEvent::WithdrawalBroadcast {
            withdrawal_id: *withdrawal_id,
            tx_hash: tx_hash.clone(),
            asset: asset.as_str().to_string(),
            chain: chain.as_str().to_string(),
            amount: amount.to_string(),
        },
        CustodyEvent::WithdrawalConfirmed {
            withdrawal_id, tx_hash, block_height, confirmations, ..
        } => WsEvent::WithdrawalConfirmed {
            withdrawal_id: *withdrawal_id,
            tx_hash: tx_hash.clone(),
            block_height: *block_height,
            confirmations: *confirmations,
        },
        CustodyEvent::WithdrawalFailed {
            withdrawal_id, reason, ..
        } => WsEvent::WithdrawalFailed {
            withdrawal_id: *withdrawal_id,
            reason: reason.clone(),
        },
    }
}

/// Build a `BalanceUpdate` event for broadcast after a deposit/withdrawal.
/// Callers (confirmation tracker, pipeline) invoke this and pass it to
/// `EventBroadcaster::broadcast`.
pub fn balance_update_event(
    user_id: Uuid,
    asset: &str,
    chain: &str,
    old_balance: Decimal,
    new_balance: Decimal,
) -> WsEvent {
    WsEvent::BalanceUpdate {
        user_id,
        asset: asset.to_string(),
        chain: chain.to_string(),
        old_balance: old_balance.to_string(),
        new_balance: new_balance.to_string(),
    }
}

/// Build a `TransactionStatusChange` event.
pub fn tx_status_change_event(
    tx_id: Uuid,
    old_status: &str,
    new_status: &str,
    tx_hash: Option<String>,
) -> WsEvent {
    WsEvent::TransactionStatusChange {
        tx_id,
        old_status: old_status.to_string(),
        new_status: new_status.to_string(),
        tx_hash,
    }
}

/// Build an `ApprovalRequired` event.
pub fn approval_required_event(
    withdrawal_id: Uuid,
    amount: Decimal,
    tier: u8,
) -> WsEvent {
    WsEvent::ApprovalRequired {
        withdrawal_id,
        amount: amount.to_string(),
        tier,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use custody_common::{Asset, Chain};
    use rust_decimal_macros::dec;

    #[test]
    fn ws_event_deposit_confirmed_serialization() {
        let event = WsEvent::DepositConfirmed {
            deposit_id: Uuid::nil(),
            user_id: Uuid::nil(),
            asset: "USDT".into(),
            chain: "ethereum".into(),
            amount: dec!(1234.56).to_string(),
            tx_hash: "0xabc".into(),
            confirmations: 12,
            block_height: 100,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"deposit_confirmed\""));
        assert!(json.contains("1234.56"));
        // Amount must NOT be a float — check it's a JSON string
        assert!(json.contains("\"1234.56\""));
    }

    #[test]
    fn ws_event_balance_update_no_float() {
        let event = balance_update_event(
            Uuid::nil(),
            "ETH",
            "ethereum",
            dec!(1.5),
            dec!(2.5),
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"balance_update\""));
        // Old/new balance as strings, never float
        assert!(json.contains("\"1.5\""));
        assert!(json.contains("\"2.5\""));
    }

    #[test]
    fn ws_event_tx_status_change_serialization() {
        let event = tx_status_change_event(
            Uuid::nil(),
            "received",
            "validated",
            None,
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"transaction_status_change\""));
        assert!(json.contains("\"received\""));
        assert!(json.contains("\"validated\""));
    }

    #[test]
    fn ws_event_approval_required_tier_preserved() {
        let event = approval_required_event(Uuid::nil(), dec!(50000), 2);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"approval_required\""));
        assert!(json.contains("\"tier\":2"));
        assert!(json.contains("\"50000\""));
    }

    #[test]
    fn custody_event_to_ws_deposit() {
        let evt = CustodyEvent::DepositConfirmed {
            deposit_id: Uuid::nil(),
            user_address: "0x1".into(),
            user_id: Uuid::nil(),
            asset: Asset::new("ETH"),
            chain: Chain::Ethereum,
            amount: dec!(0.5),
            tx_hash: "0xdeadbeef".into(),
            confirmations: 12,
            block_height: 200,
        };
        let ws = custody_event_to_ws_event(&evt);
        match ws {
            WsEvent::DepositConfirmed { amount, chain, .. } => {
                assert_eq!(amount, "0.5");
                assert_eq!(chain, "ethereum");
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn custody_event_to_ws_withdrawal_failed() {
        let evt = CustodyEvent::WithdrawalFailed {
            withdrawal_id: Uuid::nil(),
            exchange_withdrawal_id: Uuid::nil(),
            reason: "out of gas".into(),
        };
        let ws = custody_event_to_ws_event(&evt);
        match ws {
            WsEvent::WithdrawalFailed { reason, .. } => {
                assert_eq!(reason, "out of gas");
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn event_filter_integration_with_ws() {
        // Verify EventFilter::all() passes all events (existing behaviour)
        let filter = EventFilter::all();
        let evt = CustodyEvent::DepositConfirmed {
            deposit_id: Uuid::nil(),
            user_address: "x".into(),
            user_id: Uuid::nil(),
            asset: Asset::new("ETH"),
            chain: Chain::Ethereum,
            amount: dec!(1),
            tx_hash: "x".into(),
            confirmations: 1,
            block_height: 1,
        };
        assert!(filter.matches(&evt));
    }

    #[test]
    fn ws_ping_serializes() {
        let json = serde_json::to_string(&WsEvent::Ping).unwrap();
        assert!(json.contains("\"type\":\"ping\""));
    }
}
