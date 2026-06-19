//! Shared backend state and in-memory room coordination for realtime canvases.
use crate::{
    config::Config,
    error::AppError,
    models::{Canvas, Pixel, PixelUpdateEvent},
};
use chrono::Utc;
use sqlx::PgPool;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex as StdMutex, RwLock as StdRwLock},
    time::Duration,
};
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{debug, error, info};
use uuid::Uuid;

/// In-memory registry containing active canvas rooms keyed by canvas id.
///
/// Rooms are stored behind `Arc` so that hot-path operations (pixel updates,
/// broadcasts, snapshots) only take a brief *read* lock on the registry to
/// clone the room handle, then synchronize on the room's own interior locks.
/// This keeps edits to different canvases fully parallel and removes the
/// process-wide write lock that previously serialized every pixel edit.
pub type CanvasRegistry = Arc<RwLock<HashMap<Uuid, Arc<CanvasRoom>>>>;

/// Process-wide shared application state cloned into Axum handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    /// SQLx PostgreSQL connection pool.
    pub db: PgPool,
    /// Loaded environment-backed backend configuration.
    pub config: Config,
    /// Active in-memory canvas cache and real-time room metadata.
    pub canvas_registry: CanvasRegistry,
    /// Bounded channel into the write-behind persistence worker. Pixel updates
    /// are enqueued here and flushed to PostgreSQL in batched upserts, which
    /// provides backpressure instead of spawning an unbounded task per edit.
    pub pixel_writer: mpsc::Sender<Pixel>,
}

/// Arc-wrapped state shared safely across async tasks and handlers.
pub type SharedState = Arc<AppState>;

/// Mutable per-pixel state for a single canvas room, guarded by one lock so
/// grid color, version, and last-writer metadata always mutate atomically.
#[derive(Debug)]
pub struct RoomState {
    /// 2D pixel grid indexed by `[y][x]`.
    pub grid: Vec<Vec<Pixel>>,
    /// Per-pixel server versions used for optimistic concurrency ordering.
    pub server_versions: Vec<Vec<u64>>,
    /// Per-pixel last writer session id to reconcile overwritten optimistic updates.
    pub last_writer_session: Vec<Vec<Option<String>>>,
}

/// In-memory room representation for one active canvas.
///
/// The room is shared via `Arc` and uses interior locks so multiple sessions
/// can coordinate without ever holding the global registry lock.
#[derive(Debug)]
pub struct CanvasRoom {
    /// Mutable grid/version/writer state guarded by its own lock.
    pub state: StdRwLock<RoomState>,
    /// Broadcast channel used to fan out real-time events to active WebSocket sessions.
    pub broadcaster: broadcast::Sender<CanvasEvent>,
    /// Connected WebSocket session ids currently participating in the room.
    pub active_sessions: StdMutex<HashSet<Uuid>>,
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
    ///
    /// Also spawns the background write-behind persistence worker and stores the
    /// sender used by the websocket layer to enqueue pixel writes.
    ///
    /// # Parameters
    ///
    /// - `db`: Shared SQLx PostgreSQL pool.
    /// - `config`: Loaded runtime configuration.
    ///
    /// # Returns
    ///
    /// Returns [`SharedState`] as an `Arc<AppState>` suitable for Axum cloning.
    ///
    /// # Errors
    ///
    /// This constructor does not return errors.
    pub fn new(db: PgPool, config: Config) -> SharedState {
        let pixel_writer = spawn_pixel_writer(db.clone(), &config);
        Arc::new(Self {
            db,
            config,
            canvas_registry: Arc::new(RwLock::new(HashMap::new())),
            pixel_writer,
        })
    }

    /// Returns the shared room handle for a canvas, holding the registry read
    /// lock only long enough to clone the `Arc`.
    async fn room(&self, canvas_id: Uuid) -> Result<Arc<CanvasRoom>, AppError> {
        let rooms = self.canvas_registry.read().await;
        rooms
            .get(&canvas_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("canvas {canvas_id} is not loaded")))
    }

    /// Loads a canvas room from PostgreSQL into memory if it is not already cached.
    ///
    /// # Parameters
    ///
    /// - `id`: Target canvas id to load.
    /// - `db`: Database pool used for canvas and pixel reads.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the canvas is already cached or loaded successfully.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] if the canvas does not exist and
    /// [`AppError::DatabaseError`] / [`AppError::InternalError`] when loading fails.
    pub async fn get_or_load_canvas(&self, id: Uuid, db: &PgPool) -> Result<(), AppError> {
        {
            let rooms = self.canvas_registry.read().await;
            if rooms.contains_key(&id) {
                return Ok(());
            }
        }

        let canvas = sqlx::query_as::<_, Canvas>(
            "SELECT id, name, width, height, classroom_id, created_at FROM canvases WHERE id = $1",
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
        let (broadcaster, _) = broadcast::channel(self.config.broadcast_capacity);

        let room = Arc::new(CanvasRoom {
            state: StdRwLock::new(RoomState {
                grid,
                server_versions,
                last_writer_session,
            }),
            broadcaster,
            active_sessions: StdMutex::new(HashSet::new()),
        });

        let mut rooms = self.canvas_registry.write().await;
        rooms.entry(id).or_insert(room);
        info!(canvas_id = %id, "canvas loaded into in-memory registry");

        Ok(())
    }

    /// Applies a pixel update to in-memory state and returns accepted/rejected events.
    ///
    /// Only a brief registry read lock plus the target room's write lock are held,
    /// so edits to different canvases proceed fully in parallel.
    ///
    /// # Parameters
    ///
    /// - `canvas_id`: Canvas room identifier.
    /// - `event`: Incoming pixel update payload from a websocket client.
    ///
    /// # Returns
    ///
    /// Returns [`ApplyPixelUpdateResult`] with accepted event, optional rejected
    /// reconciliation event, and persisted pixel payload.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] when room is missing and
    /// [`AppError::ValidationError`] for out-of-bounds coordinates.
    pub async fn apply_pixel_update(
        &self,
        canvas_id: Uuid,
        event: PixelUpdateEvent,
    ) -> Result<ApplyPixelUpdateResult, AppError> {
        let room = self.room(canvas_id).await?;

        // Validate against the room's actual dimensions using signed comparisons so
        // large coordinates can never wrap to a valid index when cast to `usize`.
        let result = {
            let mut state = room
                .state
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            let height = state.grid.len() as i64;
            let width = state.grid.first().map_or(0, |row| row.len()) as i64;
            if event.x < 0 || event.y < 0 || (event.x as i64) >= width || (event.y as i64) >= height {
                return Err(AppError::ValidationError(format!(
                    "pixel coordinate ({}, {}) is out of bounds",
                    event.x, event.y
                )));
            }

            let x = event.x as usize;
            let y = event.y as usize;

            let previous_writer = state.last_writer_session[y][x].clone();
            let previous_version = state.server_versions[y][x];
            let next_version = previous_version.saturating_add(1);

            let now = Utc::now();
            let pixel = &mut state.grid[y][x];
            pixel.color = event.color.clone();
            pixel.updated_at = now;
            pixel.updated_by = Some(event.session_id.clone());
            let updated_pixel = pixel.clone();
            let winning_color = updated_pixel.color.clone();

            state.server_versions[y][x] = next_version;
            state.last_writer_session[y][x] = Some(event.session_id.clone());

            let accepted = CanvasEvent::PixelAccepted {
                x: event.x,
                y: event.y,
                color: event.color.clone(),
                server_version: next_version,
                session_id: event.session_id.clone(),
            };

            let rejected = match previous_writer {
                Some(previous_session_id) if previous_session_id != event.session_id => {
                    Some(CanvasEvent::PixelRejected {
                        target_session_id: previous_session_id,
                        x: event.x,
                        y: event.y,
                        winning_color,
                        server_version: next_version,
                    })
                }
                _ => None,
            };

            ApplyPixelUpdateResult {
                accepted,
                rejected,
                updated_pixel,
            }
        };

        if let Ok(session_uuid) = Uuid::parse_str(&event.session_id) {
            room.active_sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(session_uuid);
        }

        Ok(result)
    }

    /// Broadcasts an event to all currently subscribed sessions for a canvas room.
    ///
    /// # Parameters
    ///
    /// - `canvas_id`: Canvas room identifier.
    /// - `event`: Event payload to fan out to subscribers.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the event was submitted to the room broadcaster.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] if the target room is not loaded.
    pub async fn broadcast_event(&self, canvas_id: Uuid, event: CanvasEvent) -> Result<(), AppError> {
        let room = self.room(canvas_id).await?;
        let _ = room.broadcaster.send(event);
        Ok(())
    }

    /// Registers a session in an active room and returns the active session count.
    ///
    /// # Parameters
    ///
    /// - `canvas_id`: Canvas room identifier.
    /// - `session_id`: Session identifier to insert.
    ///
    /// # Returns
    ///
    /// Returns the number of active sessions after insertion.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] if the target room is not loaded.
    pub async fn add_session(&self, canvas_id: Uuid, session_id: Uuid) -> Result<usize, AppError> {
        let room = self.room(canvas_id).await?;
        let mut sessions = room
            .active_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions.insert(session_id);
        Ok(sessions.len())
    }

    /// Returns a room broadcast receiver for the target canvas.
    ///
    /// # Parameters
    ///
    /// - `canvas_id`: Canvas room identifier.
    ///
    /// # Returns
    ///
    /// Returns a [`broadcast::Receiver`] subscribed to room events.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] if the target room is not loaded.
    pub async fn subscribe_canvas_events(
        &self,
        canvas_id: Uuid,
    ) -> Result<broadcast::Receiver<CanvasEvent>, AppError> {
        let room = self.room(canvas_id).await?;
        Ok(room.broadcaster.subscribe())
    }

    /// Returns a flattened copy of current in-memory pixels for snapshot delivery.
    ///
    /// # Parameters
    ///
    /// - `canvas_id`: Canvas room identifier.
    ///
    /// # Returns
    ///
    /// Returns a row-major flattened vector of room pixels.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] if the target room is not loaded.
    pub async fn snapshot_pixels(&self, canvas_id: Uuid) -> Result<Vec<Pixel>, AppError> {
        let room = self.room(canvas_id).await?;
        let state = room
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(state.grid.iter().flat_map(|row| row.iter().cloned()).collect())
    }

    /// Removes a disconnected session from a room and clears empty room metadata.
    ///
    /// # Parameters
    ///
    /// - `canvas_id`: Canvas room identifier.
    /// - `session_id`: Session identifier to remove.
    ///
    /// # Returns
    ///
    /// Returns the number of active sessions remaining after removal.
    ///
    /// # Errors
    ///
    /// This function does not return errors; missing rooms/sessions are treated
    /// as no-op conditions.
    pub async fn remove_session(&self, canvas_id: Uuid, session_id: Uuid) -> usize {
        let room = {
            let rooms = self.canvas_registry.read().await;
            rooms.get(&canvas_id).cloned()
        };

        let Some(room) = room else {
            return 0;
        };

        let active_count = {
            let mut sessions = room
                .active_sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            sessions.remove(&session_id);
            sessions.len()
        };

        // Only evict the room when no sessions remain. We re-check emptiness under
        // the registry write lock to avoid removing a room another client just
        // joined in the gap between the read above and the write below.
        if active_count == 0 {
            let mut rooms = self.canvas_registry.write().await;
            let still_empty = rooms.get(&canvas_id).is_some_and(|room| {
                room.active_sessions
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .is_empty()
            });
            if still_empty {
                rooms.remove(&canvas_id);
                info!(canvas_id = %canvas_id, "canvas room removed after last session disconnect");
            }
        }

        active_count
    }
}

/// Spawns the background write-behind worker that batches pixel upserts.
///
/// Incoming pixels are coalesced by `(canvas_id, x, y)` (keeping only the most
/// recent color) and flushed either when the pending batch reaches a size cap or
/// on a fixed interval. This turns the previous one-task-per-pixel firehose into
/// a small number of bounded, batched transactions, which is the main lever for
/// sustaining many concurrent editors without exhausting the connection pool.
fn spawn_pixel_writer(db: PgPool, config: &Config) -> mpsc::Sender<Pixel> {
    let (tx, mut rx) = mpsc::channel::<Pixel>(config.pixel_write_buffer);
    let flush_interval = Duration::from_millis(config.pixel_flush_interval_ms);
    let max_batch = config.pixel_flush_max_batch;

    tokio::spawn(async move {
        let mut pending: HashMap<(Uuid, i32, i32), Pixel> = HashMap::new();
        let mut ticker = tokio::time::interval(flush_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                received = rx.recv() => {
                    match received {
                        Some(pixel) => {
                            pending.insert((pixel.canvas_id, pixel.x, pixel.y), pixel);
                            if pending.len() >= max_batch {
                                flush_pixels(&db, &mut pending).await;
                            }
                        }
                        None => {
                            // Senders dropped; flush remaining work and exit.
                            flush_pixels(&db, &mut pending).await;
                            break;
                        }
                    }
                }
                _ = ticker.tick() => {
                    flush_pixels(&db, &mut pending).await;
                }
            }
        }
    });

    tx
}

/// Flushes the pending pixel batch as a single multi-row upsert.
async fn flush_pixels(db: &PgPool, pending: &mut HashMap<(Uuid, i32, i32), Pixel>) {
    if pending.is_empty() {
        return;
    }

    let batch: Vec<Pixel> = pending.drain().map(|(_, pixel)| pixel).collect();
    let batch_len = batch.len();

    let mut builder = sqlx::QueryBuilder::new(
        "INSERT INTO pixels (id, canvas_id, x, y, color, updated_at, updated_by) ",
    );
    builder.push_values(batch.iter(), |mut row, pixel| {
        row.push_bind(pixel.id)
            .push_bind(pixel.canvas_id)
            .push_bind(pixel.x)
            .push_bind(pixel.y)
            .push_bind(pixel.color.clone())
            .push_bind(pixel.updated_at)
            .push_bind(pixel.updated_by.clone());
    });
    builder.push(
        " ON CONFLICT (canvas_id, x, y) DO UPDATE SET \
         color = EXCLUDED.color, updated_at = EXCLUDED.updated_at, updated_by = EXCLUDED.updated_by",
    );

    if let Err(error) = builder.build().execute(db).await {
        error!(%error, batch_len, "failed to flush pixel batch to PostgreSQL");
        // Re-queue the batch so a transient DB error doesn't drop durable writes;
        // newer edits to the same coordinate will still win via the HashMap key.
        for pixel in batch {
            pending
                .entry((pixel.canvas_id, pixel.x, pixel.y))
                .or_insert(pixel);
        }
    } else {
        debug!(batch_len, "flushed pixel batch to PostgreSQL");
    }
}
