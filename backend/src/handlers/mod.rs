// HTTP handlers and API route assembly for canvas CRUD operations.
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::FromRow;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{Canvas, CanvasStateSnapshot, CreateCanvasRequest, Pixel},
    state::SharedState,
};

/// Builds `/api` routes for canvas management endpoints.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/canvases", post(create_canvas).get(list_canvases))
        .route("/canvases/{id}", get(get_canvas).delete(delete_canvas))
}

/// Lightweight service health endpoint used by setup checks.
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "service": "canvax-backend",
        "status": "ok"
    }))
}

/// Creates a new canvas after validating input dimensions and name.
pub async fn create_canvas(
    State(state): State<SharedState>,
    Json(payload): Json<CreateCanvasRequest>,
) -> Result<(StatusCode, Json<Canvas>), AppError> {
    let trimmed_name = payload.name.trim();
    if trimmed_name.is_empty() {
        return Err(AppError::ValidationError(
            "name cannot be empty or whitespace".to_string(),
        ));
    }

    if !(8..=128).contains(&payload.width) || !(8..=128).contains(&payload.height) {
        return Err(AppError::ValidationError(
            "width and height must be between 8 and 128".to_string(),
        ));
    }

    let canvas_id = Uuid::new_v4();
    let canvas = sqlx::query_as::<_, Canvas>(
        "INSERT INTO canvases (id, name, width, height) VALUES ($1, $2, $3, $4) RETURNING id, name, width, height, created_at",
    )
    .bind(canvas_id)
    .bind(trimmed_name)
    .bind(payload.width)
    .bind(payload.height)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::from)?;

    info!(canvas_id = %canvas.id, name = %canvas.name, "canvas created");

    Ok((StatusCode::CREATED, Json(canvas)))
}

/// Fetches a canvas and all persisted pixels for initial client state hydration.
pub async fn get_canvas(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CanvasStateSnapshot>, AppError> {
    let canvas = sqlx::query_as::<_, Canvas>(
        "SELECT id, name, width, height, created_at FROM canvases WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| AppError::NotFound(format!("canvas {id} was not found")))?;

    let pixels = sqlx::query_as::<_, Pixel>(
        "SELECT id, canvas_id, x, y, color, updated_at, updated_by FROM pixels WHERE canvas_id = $1 ORDER BY y ASC, x ASC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::from)?;

    info!(canvas_id = %id, pixels = pixels.len(), "canvas snapshot fetched");

    Ok(Json(CanvasStateSnapshot { canvas, pixels }))
}

/// Returns all canvases with summary fields required by the canvas selection screen.
pub async fn list_canvases(
    State(state): State<SharedState>,
) -> Result<Json<Vec<CanvasListItem>>, AppError> {
    let canvases = sqlx::query_as::<_, CanvasListItem>(
        "SELECT id, name, width, height FROM canvases ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::from)?;

    info!(count = canvases.len(), "canvas list fetched");
    Ok(Json(canvases))
}

/// Deletes a canvas by id and relies on cascade rules to remove associated pixels.
pub async fn delete_canvas(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM canvases WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(AppError::from)?;

    if result.rows_affected() == 0 {
        warn!(canvas_id = %id, "delete requested for missing canvas");
        return Err(AppError::NotFound(format!("canvas {id} was not found")));
    }

    info!(canvas_id = %id, "canvas deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// Summary projection used by the list-canvases endpoint.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CanvasListItem {
    /// Unique canvas identifier.
    pub id: Uuid,
    /// Display name of the canvas.
    pub name: String,
    /// Width of the canvas grid.
    pub width: i32,
    /// Height of the canvas grid.
    pub height: i32,
}