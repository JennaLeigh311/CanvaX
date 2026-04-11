// HTTP route handlers for the backend API surface.
use axum::Json;
use serde_json::{Value, json};

pub async fn health_check() -> Json<Value> {
    Json(json!({
        "service": "canvax-backend",
        "status": "ok"
    }))
}