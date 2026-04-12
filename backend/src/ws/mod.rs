// WebSocket entrypoints and connection lifecycle hooks for real-time sync.
use axum::{
    extract::{Path, State, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{Canvas, CanvasStateSnapshot, Pixel, PixelUpdateEvent},
    state::{CanvasEvent, SharedState},
};

/// Upgrades an HTTP request to a canvas-scoped websocket connection.
pub async fn ws_handler(
    Path(canvas_id): Path<Uuid>,
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> Result<Response, AppError> {
    state.get_or_load_canvas(canvas_id, &state.db).await?;

    let session_id = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions (id, canvas_id, user_name) VALUES ($1, $2, $3)")
        .bind(session_id)
        .bind(canvas_id)
        .bind(Option::<String>::None)
        .execute(&state.db)
        .await
        .map_err(AppError::from)?;

    let canvas = sqlx::query_as::<_, Canvas>(
        "SELECT id, name, width, height, created_at FROM canvases WHERE id = $1",
    )
    .bind(canvas_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| AppError::NotFound(format!("canvas {canvas_id} was not found")))?;

    let (snapshot_pixels, broadcast_rx) = {
        let mut rooms = state.canvas_registry.write().await;
        let room = rooms
            .get_mut(&canvas_id)
            .ok_or_else(|| AppError::NotFound(format!("canvas {canvas_id} is not loaded")))?;

        room.active_sessions.insert(session_id);
        let pixels = flatten_grid(&room.grid);
        let rx = room.broadcaster.subscribe();
        (pixels, rx)
    };

    let snapshot = CanvasStateSnapshot {
        canvas,
        pixels: snapshot_pixels,
    };

    info!(canvas_id = %canvas_id, session_id = %session_id, "websocket session connected");

    Ok(ws.on_upgrade(move |socket| {
        handle_socket(socket, state, canvas_id, session_id, broadcast_rx, snapshot)
    }))
}

async fn handle_socket(
    socket: WebSocket,
    state: SharedState,
    canvas_id: Uuid,
    session_id: Uuid,
    mut broadcast_rx: broadcast::Receiver<CanvasEvent>,
    snapshot: CanvasStateSnapshot,
) {
    let (mut sender, mut receiver) = socket.split();

    if let Err(error) = send_snapshot(&mut sender, &snapshot).await {
        warn!(canvas_id = %canvas_id, session_id = %session_id, %error, "failed to send initial snapshot");
        finalize_disconnect(&state, canvas_id, session_id).await;
        return;
    }

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(raw))) => {
                        match serde_json::from_str::<PixelUpdateEvent>(&raw) {
                            Ok(mut event) => {
                                event.session_id = session_id.to_string();

                                if !is_within_bounds(&state, canvas_id, event.x, event.y).await {
                                    let message = serde_json::json!({"message": "pixel coordinates out of bounds"}).to_string();
                                    let _ = sender.send(Message::Text(message.into())).await;
                                    continue;
                                }

                                match state.apply_pixel_update(canvas_id, event).await {
                                    Ok(canvas_event) => {
                                        // Persist writes in a spawned task so websocket broadcasting stays non-blocking;
                                        // clients receive updates immediately while DB durability catches up asynchronously.
                                        persist_pixel_async(state.db.clone(), canvas_event.pixel.clone());

                                        // We intentionally broadcast back to the sender so every client, including
                                        // the originator, reconciles against authoritative server state.
                                        if let Err(error) = state.broadcast_event(canvas_id, canvas_event).await {
                                            warn!(canvas_id = %canvas_id, session_id = %session_id, %error, "failed to broadcast canvas event");
                                        }
                                    }
                                    Err(error) => {
                                        let message = serde_json::json!({"message": error.to_string()}).to_string();
                                        let _ = sender.send(Message::Text(message.into())).await;
                                    }
                                }
                            }
                            Err(error) => {
                                let message = serde_json::json!({"message": format!("invalid pixel update payload: {error}")}).to_string();
                                let _ = sender.send(Message::Text(message.into())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(_)) => {
                        // Ignore non-text websocket messages for this endpoint.
                    }
                    Some(Err(error)) => {
                        warn!(canvas_id = %canvas_id, session_id = %session_id, %error, "websocket receive error");
                        break;
                    }
                }
            }
            outbound = broadcast_rx.recv() => {
                match outbound {
                    Ok(event) => {
                        if let Ok(payload) = serde_json::to_string(&event) {
                            if sender.send(Message::Text(payload.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(canvas_id = %canvas_id, session_id = %session_id, skipped, "websocket receiver lagged behind broadcasts");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }

    // Concurrency model note: updates are applied in server arrival order under a short write lock,
    // producing last-write-wins semantics with server-assigned timestamp/version for deterministic ordering.
    finalize_disconnect(&state, canvas_id, session_id).await;
}

async fn send_snapshot(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    snapshot: &CanvasStateSnapshot,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(snapshot)
        .map_err(|error| AppError::InternalError(format!("failed to serialize canvas snapshot: {error}")))?;
    sender
        .send(Message::Text(payload.into()))
        .await
        .map_err(|error| AppError::InternalError(format!("failed to send canvas snapshot: {error}")))
}

fn persist_pixel_async(pool: sqlx::PgPool, pixel: Pixel) {
    tokio::spawn(async move {
        let result = sqlx::query(
            "INSERT INTO pixels (id, canvas_id, x, y, color, updated_at, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (canvas_id, x, y)
             DO UPDATE SET color = EXCLUDED.color, updated_at = EXCLUDED.updated_at, updated_by = EXCLUDED.updated_by",
        )
        .bind(pixel.id)
        .bind(pixel.canvas_id)
        .bind(pixel.x)
        .bind(pixel.y)
        .bind(pixel.color)
        .bind(pixel.updated_at)
        .bind(pixel.updated_by)
        .execute(&pool)
        .await;

        if let Err(error) = result {
            error!(%error, "failed to persist pixel update");
        }
    });
}

async fn is_within_bounds(state: &SharedState, canvas_id: Uuid, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 {
        return false;
    }

    let rooms = state.canvas_registry.read().await;
    let Some(room) = rooms.get(&canvas_id) else {
        return false;
    };

    let y_idx = y as usize;
    if y_idx >= room.grid.len() {
        return false;
    }

    let x_idx = x as usize;
    x_idx < room.grid[y_idx].len()
}

fn flatten_grid(grid: &[Vec<Pixel>]) -> Vec<Pixel> {
    grid.iter().flat_map(|row| row.iter().cloned()).collect()
}

async fn finalize_disconnect(state: &SharedState, canvas_id: Uuid, session_id: Uuid) {
    state.remove_session(canvas_id, session_id).await;

    if let Err(error) = sqlx::query("UPDATE sessions SET last_active = NOW() WHERE id = $1")
        .bind(session_id)
        .execute(&state.db)
        .await
    {
        warn!(canvas_id = %canvas_id, session_id = %session_id, %error, "failed to update session last_active");
    }

    info!(canvas_id = %canvas_id, session_id = %session_id, "websocket session disconnected");
}