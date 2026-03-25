//! WebSocket endpoint for real-time event streaming.
//!
//! Exchange connects via WS and receives deposit/withdrawal events
//! instead of polling webhooks.

use axum::{
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::Response,
};
use futures::{SinkExt, StreamExt};

use crate::state::AppState;

/// WebSocket handler — upgrades HTTP to WS and streams events.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    let broadcaster = state.event_broadcaster.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, broadcaster))
}

async fn handle_socket(
    socket: WebSocket,
    broadcaster: std::sync::Arc<custody_common::events::EventBroadcaster>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = broadcaster.subscribe();

    // Spawn task to forward events to WS client
    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let json = serde_json::to_string(&event).unwrap_or_default();
            if sender.send(Message::Text(json.into())).await.is_err() {
                break; // client disconnected
            }
        }
    });

    // Read from client (for ping/pong, close frames)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                Message::Ping(data) => {
                    let _ = data;
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    tracing::info!("WebSocket client disconnected");
}
