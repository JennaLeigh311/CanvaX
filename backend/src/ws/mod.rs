// WebSocket entrypoints and connection lifecycle hooks for real-time sync.
use axum::{
    extract::{State, ws::{WebSocket, WebSocketUpgrade}},
    response::Response,
};
use futures_util::StreamExt;
use tracing::info;

use crate::state::AppState;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    info!(
        active_sessions = state.active_session_count(),
        "WebSocket client connected"
    );

    while let Some(result) = socket.next().await {
        if result.is_err() {
            break;
        }
    }

    info!("WebSocket client disconnected");
}