//! Database models and transport DTOs shared across REST and websocket layers.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Represents a collaborative pixel canvas persisted in PostgreSQL.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Canvas {
    /// Primary key for the canvas record.
    pub id: Uuid,
    /// Display name shown in UI lists and canvas headers.
    pub name: String,
    /// Number of horizontal cells in the canvas grid.
    pub width: i32,
    /// Number of vertical cells in the canvas grid.
    pub height: i32,
    /// Optional classroom ownership for this canvas.
    pub classroom_id: Option<Uuid>,
    /// Timestamp when the canvas was created.
    pub created_at: DateTime<Utc>,
}

/// Represents a classroom workspace that can contain multiple canvases.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Classroom {
    /// Primary key for the classroom record.
    pub id: Uuid,
    /// Display name shown in classroom listings and headers.
    pub name: String,
    /// Timestamp when the classroom was created.
    pub created_at: DateTime<Utc>,
}

/// Represents a single pixel state entry tied to a canvas coordinate.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Pixel {
    /// Primary key for the pixel row.
    pub id: Uuid,
    /// Parent canvas identifier for this pixel.
    pub canvas_id: Uuid,
    /// X coordinate of the pixel in the grid.
    pub x: i32,
    /// Y coordinate of the pixel in the grid.
    pub y: i32,
    /// Pixel color in #RRGGBB format.
    pub color: String,
    /// Timestamp of the most recent update.
    pub updated_at: DateTime<Utc>,
    /// Session identifier associated with the latest update.
    pub updated_by: Option<String>,
}

/// Represents a connected collaboration session for a specific canvas.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    /// Primary key for the session record.
    pub id: Uuid,
    /// Canvas identifier that this session is connected to.
    pub canvas_id: Uuid,
    /// Optional display name for the connected user.
    pub user_name: Option<String>,
    /// Timestamp when the session first connected.
    pub connected_at: DateTime<Utc>,
    /// Timestamp of the most recent observed client activity.
    pub last_active: DateTime<Utc>,
}

/// Request payload for creating a new collaborative canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCanvasRequest {
    /// Human-readable name for the new canvas.
    pub name: String,
    /// Requested number of horizontal cells.
    pub width: i32,
    /// Requested number of vertical cells.
    pub height: i32,
    /// Optional classroom id when creating classroom-scoped canvases.
    #[serde(default)]
    pub classroom_id: Option<Uuid>,
}

/// Request payload for creating a new classroom board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateClassroomRequest {
    /// Human-readable classroom name.
    pub name: String,
}

/// WebSocket event payload for updating a single pixel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelUpdateEvent {
    /// X coordinate of the updated pixel.
    pub x: i32,
    /// Y coordinate of the updated pixel.
    pub y: i32,
    /// New color for the pixel in #RRGGBB format.
    pub color: String,
    /// Client-side timestamp in Unix milliseconds when the edit was produced.
    pub client_timestamp: u64,
    /// Session identifier that produced this update.
    pub session_id: String,
}

/// Snapshot payload sent to clients when joining a canvas session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasStateSnapshot {
    /// Canvas metadata and dimensions.
    pub canvas: Canvas,
    /// Current persisted pixels for the canvas.
    pub pixels: Vec<Pixel>,
}