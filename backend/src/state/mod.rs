// Shared application state for database/config access and real-time in-memory canvas rooms.
use crate::{
    config::Config,
    error::AppError,
    models::{Canvas, Pixel, PixelUpdateEvent},
};
use chrono::Utc;
use sqlx::PgPool;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{RwLock, broadcast};
use tracing::info;
use uuid::Uuid;

/// In-memory registry containing active canvas rooms keyed by canvas id.
pub type CanvasRegistry = Arc<RwLock<HashMap<Uuid, CanvasRoom>>>;

/// Process-wide shared application state cloned into Axum handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    /// SQLx PostgreSQL connection pool.
    pub db: PgPool,
    /// Loaded environment-backed backend configuration.
    pub config: Config,
    /// Active in-memory canvas cache and real-time room metadata.
    /// RwLock strategy: reads (getting pixel state) use read locks, while writes
    /// (pixel updates/session membership changes) use write locks only briefly.
    pub canvas_registry: CanvasRegistry,
}

/// Arc-wrapped state shared safely across async tasks and handlers.
pub type SharedState = Arc<AppState>;

/// In-memory room representation for one active canvas.
#[derive(Debug, Clone)]
pub struct CanvasRoom {
    /// 2D pixel grid indexed by `[y][x]`.
    pub grid: Vec<Vec<Pixel>>,
    /// Per-pixel server versions used for optimistic concurrency ordering.
    pub server_versions: Vec<Vec<u64>>,
    /// Per-pixel last writer session id to reconcile overwritten optimistic updates.
    pub last_writer_session: Vec<Vec<Option<String>>>,
    /// Broadcast channel used to fan out real-time events to active WebSocket sessions.
    pub broadcaster: broadcast::Sender<CanvasEvent>,
    /// Connected WebSocket session ids currently participating in the room.
    pub active_sessions: HashSet<Uuid>,
}

/// Broadcast event type used inside the websocket room channel.
#[derive(Debug, Clone)]
pub enum CanvasEvent {
    PixelAccepted {
        x: i32,
        y: i32,
        color: String,
        server_version: u64,
        session_id: String,
    },
    PixelRejected {
        target_session_id: String,
        x: i32,
        y: i32,
        winning_color: String,
        server_version: u64,
    },
    SessionJoined {
        session_id: String,
        active_session_count: usize,
    },
    SessionLeft {
        session_id: String,
        active_session_count: usize,
    },
}

/// Result of applying one optimistic pixel update in memory.
#[derive(Debug, Clone)]
pub struct ApplyPixelUpdateResult {
    /// Accepted update broadcast payload with server-assigned version.
    pub accepted: CanvasEvent,
    /// Reconciliation event for the overwritten lower-version writer (if any).
    pub rejected: Option<CanvasEvent>,
    /// Updated pixel persisted asynchronously in PostgreSQL.
    pub updated_pixel: Pixel,
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

        let server_versions = vec![vec![0_u64; width]; height];
        let last_writer_session = vec![vec![None; width]; height];
        let (broadcaster, _) = broadcast::channel(1024);

        let room = CanvasRoom {
            grid,
            server_versions,
            last_writer_session,
            broadcaster,
            active_sessions: HashSet::new(),
        };

        let mut rooms = self.canvas_registry.write().await;
        rooms.entry(id).or_insert(room);
        info!(canvas_id = %id, "canvas loaded into in-memory registry");

        Ok(())
    }

    /// Applies a pixel update to in-memory state and returns accepted/rejected events.
    pub async fn apply_pixel_update(
        &self,
        canvas_id: Uuid,
        event: PixelUpdateEvent,
    ) -> Result<ApplyPixelUpdateResult, AppError> {
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

        let previous_writer = room.last_writer_session[y][x].clone();
        let previous_version = room.server_versions[y][x];
        let next_version = previous_version.saturating_add(1);

        let now = Utc::now();
        let pixel = &mut room.grid[y][x];
        pixel.color = event.color.clone();
        pixel.updated_at = now;
        pixel.updated_by = Some(event.session_id.clone());

        room.server_versions[y][x] = next_version;
        room.last_writer_session[y][x] = Some(event.session_id.clone());
        if let Ok(session_uuid) = Uuid::parse_str(&event.session_id) {
            room.active_sessions.insert(session_uuid);
        }

        let accepted = CanvasEvent::PixelAccepted {
            x: event.x,
            y: event.y,
            color: event.color,
            server_version: next_version,
            session_id: event.session_id.clone(),
        };

        let rejected = if let Some(previous_session_id) = previous_writer {
            if previous_session_id != event.session_id {
                Some(CanvasEvent::PixelRejected {
                    target_session_id: previous_session_id,
                    x: event.x,
                    y: event.y,
                    winning_color: pixel.color.clone(),
                    server_version: next_version,
                })
            } else {
                None
            }
        } else {
            None
        };

        Ok(ApplyPixelUpdateResult {
            accepted,
            rejected,
            updated_pixel: pixel.clone(),
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

    /// Registers a session in an active room and returns the active session count.
    pub async fn add_session(&self, canvas_id: Uuid, session_id: Uuid) -> Result<usize, AppError> {
        let mut rooms = self.canvas_registry.write().await;
        let room = rooms
            .get_mut(&canvas_id)
            .ok_or_else(|| AppError::NotFound(format!("canvas {canvas_id} is not loaded")))?;
        room.active_sessions.insert(session_id);
        Ok(room.active_sessions.len())
    }

    /// Returns a room broadcast receiver for the target canvas.
    pub async fn subscribe_canvas_events(
        &self,
        canvas_id: Uuid,
    ) -> Result<broadcast::Receiver<CanvasEvent>, AppError> {
        let rooms = self.canvas_registry.read().await;
        let room = rooms
            .get(&canvas_id)
            .ok_or_else(|| AppError::NotFound(format!("canvas {canvas_id} is not loaded")))?;
        Ok(room.broadcaster.subscribe())
    }

    /// Returns a flattened copy of current in-memory pixels for snapshot delivery.
    pub async fn snapshot_pixels(&self, canvas_id: Uuid) -> Result<Vec<Pixel>, AppError> {
        let rooms = self.canvas_registry.read().await;
        let room = rooms
            .get(&canvas_id)
            .ok_or_else(|| AppError::NotFound(format!("canvas {canvas_id} is not loaded")))?;
        Ok(room
            .grid
            .iter()
            .flat_map(|row| row.iter().cloned())
            .collect())
    }

    /// Removes a disconnected session from a room and clears empty room metadata.
    pub async fn remove_session(&self, canvas_id: Uuid, session_id: Uuid) -> usize {
        let mut rooms = self.canvas_registry.write().await;

        let mut active_count = 0usize;
        let should_remove_room = if let Some(room) = rooms.get_mut(&canvas_id) {
            room.active_sessions.remove(&session_id);
            active_count = room.active_sessions.len();
            room.active_sessions.is_empty() && room.broadcaster.receiver_count() == 0
        } else {
            false
        };

        if should_remove_room {
            rooms.remove(&canvas_id);
            info!(canvas_id = %canvas_id, "canvas room removed after last session disconnect");
        }

        active_count
    }
}