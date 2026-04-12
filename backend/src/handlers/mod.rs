//! HTTP handler implementations and route assembly for backend REST endpoints.
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
///
/// # Parameters
///
/// This function does not accept parameters.
///
/// # Returns
///
/// Returns an Axum [`Router`] scoped to canvas CRUD endpoints.
///
/// # Errors
///
/// This function does not return errors directly; handler-level errors are
/// produced at request time by individual route handlers.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/canvases", post(create_canvas).get(list_canvases))
        .route("/canvases/{id}", get(get_canvas).delete(delete_canvas))
}

/// Lightweight service health endpoint used by setup checks.
///
/// # Parameters
///
/// This handler does not accept parameters.
///
/// # Returns
///
/// Returns a JSON payload indicating the service is reachable.
///
/// # Errors
///
/// This handler does not return operational errors.
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "service": "canvax-backend",
        "status": "ok"
    }))
}

/// Deployment health endpoint that validates DB reachability and runtime readiness.
///
/// # Parameters
///
/// - `state`: Shared application state containing database pool and active
///   in-memory canvas registry.
///
/// # Returns
///
/// Returns `(200, { status: "ok", db: "connected", active_canvases: N })`
/// when the database probe succeeds, otherwise
/// `(503, { status: "degraded", db: "error", error: "..." })`.
///
/// # Errors
///
/// This handler does not bubble errors; it maps database probe failures to
/// a degraded 503 health response.
pub async fn deployment_health(
    State(state): State<SharedState>,
) -> (StatusCode, Json<Value>) {
    let db_probe = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await;

    match db_probe {
        Ok(_) => {
            let active_canvases = state.canvas_registry.read().await.len();
            (
                StatusCode::OK,
                Json(json!({
                    "status": "ok",
                    "db": "connected",
                    "active_canvases": active_canvases,
                })),
            )
        }
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "degraded",
                "db": "error",
                "error": error.to_string(),
            })),
        ),
    }
}

/// Creates a new canvas after validating input dimensions and name.
///
/// # Parameters
///
/// - `state`: Shared application state with database pool access.
/// - `payload`: JSON request body with canvas name and dimensions.
///
/// # Returns
///
/// Returns `(201, Json<Canvas>)` containing the created canvas record.
///
/// # Errors
///
/// Returns [`AppError::ValidationError`] for invalid names/dimensions, and
/// [`AppError::DatabaseError`] for persistence failures.
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
///
/// # Parameters
///
/// - `state`: Shared application state with database pool access.
/// - `id`: Canvas identifier extracted from route params.
///
/// # Returns
///
/// Returns `Json<CanvasStateSnapshot>` with metadata and ordered pixel data.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the canvas does not exist and
/// [`AppError::DatabaseError`] for query failures.
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
///
/// # Parameters
///
/// - `state`: Shared application state with database pool access.
///
/// # Returns
///
/// Returns `Json<Vec<CanvasListItem>>` sorted by creation time descending.
///
/// # Errors
///
/// Returns [`AppError::DatabaseError`] if loading the canvas list fails.
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
///
/// # Parameters
///
/// - `state`: Shared application state with database pool access.
/// - `id`: Canvas identifier extracted from route params.
///
/// # Returns
///
/// Returns [`StatusCode::NO_CONTENT`] when deletion succeeds.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] if the canvas does not exist and
/// [`AppError::DatabaseError`] if deletion fails.
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