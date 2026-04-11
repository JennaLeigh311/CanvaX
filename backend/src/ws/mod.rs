// WebSocket entrypoints and connection lifecycle hooks for real-time sync.
use axum::{
    extract::{State, ws::{WebSocket, WebSocketUpgrade}},
    response::Response,
};
use futures_util::StreamExt;
use tracing::info;

use crate::state::SharedState;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<SharedState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: SharedState) {
    info!(
        max_sessions = state.config.max_sessions,
        "WebSocket client connected"
    );

    while let Some(result) = socket.next().await {
        if result.is_err() {
            break;
        }
    }

    info!("WebSocket client disconnected");
}