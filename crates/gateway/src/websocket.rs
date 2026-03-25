//! WebSocket endpoint for real-time event streaming.
//!
//! Exchange connects via WS and receives deposit/withdrawal events
//! instead of polling webhooks.

use axum::{
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::Response,
};
use futures::{SinkExt, StreamExt};
use custody_common::events::EventBroadcaster;
use std::sync::Arc;

/// WebSocket handler — upgrades HTTP to WS and streams events.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(broadcaster): State<Arc<EventBroadcaster>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, broadcaster))
}

async fn handle_socket(socket: WebSocket, broadcaster: Arc<EventBroadcaster>) {
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
                    // axum handles pong automatically
                    let _ = data;
                }
                _ => {} // ignore other messages
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    tracing::info!("WebSocket client disconnected");
}
