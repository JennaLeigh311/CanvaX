// Shared application state for database/config access and real-time in-memory canvas rooms.
use crate::{
    config::Config,
    error::AppError,
    models::{Canvas, Pixel, PixelUpdateEvent},
};
use chrono::Utc;
use sqlx::PgPool;
use std::{collections::{HashMap, HashSet}, sync::Arc};
use tokio::sync::{RwLock, broadcast};
use tracing::info;
use uuid::Uuid;

/// In-memory registry containing all active canvas rooms keyed by canvas id.
pub type CanvasRegistry = Arc<RwLock<HashMap<Uuid, CanvasRoom>>>;

/// Process-wide shared application state cloned into Axum handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    /// SQLx PostgreSQL connection pool.
    pub db: PgPool,
    /// Loaded environment-backed backend configuration.
    pub config: Config,
    /// Active in-memory canvas cache and real-time room metadata.
    /// RwLock strategy: reads (state lookup/snapshot) use read locks, while writes
    /// (pixel updates/session cleanup) take write locks only for short critical sections.
    pub canvas_registry: CanvasRegistry,
}

/// Arc-wrapped state shared safely across async tasks and handlers.
pub type SharedState = Arc<AppState>;

/// In-memory room representation for one active canvas.
#[derive(Debug, Clone)]
pub struct CanvasRoom {
    /// 2D pixel grid indexed by `[y][x]`.
    pub grid: Vec<Vec<Pixel>>,
    /// Broadcast channel used to fan out real-time events to active WebSocket sessions.
    pub broadcaster: broadcast::Sender<CanvasEvent>,
    /// Connected WebSocket session ids currently participating in the room.
    pub active_sessions: HashSet<Uuid>,
    /// Monotonic version counter incremented on every accepted pixel update.
    pub version: u64,
}

/// Server-generated event emitted to subscribed sessions after a pixel mutation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanvasEvent {
    /// Canvas id that this event belongs to.
    pub canvas_id: Uuid,
    /// Pixel state after the update has been applied by the server.
    pub pixel: Pixel,
    /// Monotonic server-assigned version for optimistic ordering.
    pub version: u64,
    /// Server timestamp when the update was accepted.
    pub server_timestamp: chrono::DateTime<chrono::Utc>,
}

impl AppState {
    /// Creates a new Arc-wrapped shared state object with an empty canvas registry.
    pub fn new(db: PgPool, config: Config) -> SharedState {
        Arc::new(Self {
            db,
            config,
            canvas_registry: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Loads a canvas room from PostgreSQL into memory if it is not already cached.
    pub async fn get_or_load_canvas(&self, id: Uuid, db: &PgPool) -> Result<(), AppError> {
        {
            let rooms = self.canvas_registry.read().await;
            if rooms.contains_key(&id) {
                return Ok(());
            }
        }

        let canvas = sqlx::query_as::<_, Canvas>(
            "SELECT id, name, width, height, created_at FROM canvases WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound(format!("canvas {id} was not found")))?;

        if canvas.width <= 0 || canvas.height <= 0 {
            return Err(AppError::InternalError(format!(
                "canvas {id} has invalid dimensions {}x{}",
                canvas.width, canvas.height
            )));
        }

        let width = canvas.width as usize;
        let height = canvas.height as usize;
        let now = Utc::now();

        let mut grid = Vec::with_capacity(height);
        for y in 0..height {
            let mut row = Vec::with_capacity(width);
            for x in 0..width {
                row.push(Pixel {
                    id: Uuid::new_v4(),
                    canvas_id: id,
                    x: x as i32,
                    y: y as i32,
                    color: "#000000".to_string(),
                    updated_at: now,
                    updated_by: None,
                });
            }
            grid.push(row);
        }

        let persisted_pixels = sqlx::query_as::<_, Pixel>(
            "SELECT id, canvas_id, x, y, color, updated_at, updated_by FROM pixels WHERE canvas_id = $1",
        )
        .bind(id)
        .fetch_all(db)
        .await
        .map_err(AppError::from)?;

        for pixel in persisted_pixels {
            if pixel.x >= 0 && pixel.y >= 0 {
                let x = pixel.x as usize;
                let y = pixel.y as usize;
                if y < grid.len() && x < grid[y].len() {
                    grid[y][x] = pixel;
                }
            }
        }

        let (broadcaster, _) = broadcast::channel(1024);
        let room = CanvasRoom {
            grid,
            broadcaster,
            active_sessions: HashSet::new(),
            version: 0,
        };

        let mut rooms = self.canvas_registry.write().await;
        rooms.entry(id).or_insert(room);
        info!(canvas_id = %id, "canvas loaded into in-memory registry");

        Ok(())
    }

    /// Applies a pixel update to in-memory state and returns the server-stamped event.
    pub async fn apply_pixel_update(
        &self,
        canvas_id: Uuid,
        event: PixelUpdateEvent,
    ) -> Result<CanvasEvent, AppError> {
        let mut rooms = self.canvas_registry.write().await;
        let room = rooms
            .get_mut(&canvas_id)
            .ok_or_else(|| AppError::NotFound(format!("canvas {canvas_id} is not loaded")))?;

        if event.x < 0 || event.y < 0 {
            return Err(AppError::ValidationError(
                "x and y must be non-negative".to_string(),
            ));
        }

        let x = event.x as usize;
        let y = event.y as usize;
        if y >= room.grid.len() || x >= room.grid[y].len() {
            return Err(AppError::ValidationError(format!(
                "pixel coordinate ({}, {}) is out of bounds",
                event.x, event.y
            )));
        }

        let now = Utc::now();
        let pixel = &mut room.grid[y][x];
        pixel.color = event.color;
        pixel.updated_at = now;
        pixel.updated_by = Some(event.session_id.clone());

        if let Ok(session_uuid) = Uuid::parse_str(&event.session_id) {
            room.active_sessions.insert(session_uuid);
        }

        room.version = room.version.saturating_add(1);

        Ok(CanvasEvent {
            canvas_id,
            pixel: pixel.clone(),
            version: room.version,
            server_timestamp: now,
        })
    }

    /// Broadcasts an event to all currently subscribed sessions for a canvas room.
    pub async fn broadcast_event(&self, canvas_id: Uuid, event: CanvasEvent) -> Result<(), AppError> {
        let rooms = self.canvas_registry.read().await;
        let room = rooms
            .get(&canvas_id)
            .ok_or_else(|| AppError::NotFound(format!("canvas {canvas_id} is not loaded")))?;

        let _ = room.broadcaster.send(event);
        Ok(())
    }

    /// Removes a disconnected session from a room and clears empty room metadata.
    pub async fn remove_session(&self, canvas_id: Uuid, session_id: Uuid) {
        let mut rooms = self.canvas_registry.write().await;

        let should_remove_room = if let Some(room) = rooms.get_mut(&canvas_id) {
            room.active_sessions.remove(&session_id);
            room.active_sessions.is_empty() && room.broadcaster.receiver_count() == 0
        } else {
            false
        };

        if should_remove_room {
            rooms.remove(&canvas_id);
            info!(canvas_id = %canvas_id, "canvas room removed after last session disconnect");
        }
    }
}