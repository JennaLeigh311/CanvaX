// Shared application state for database access and in-memory collaboration registries.
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    canvas_registry: Arc<RwLock<HashMap<Uuid, CanvasState>>>,
    session_registry: Arc<RwLock<HashMap<Uuid, SessionState>>>,
}

#[derive(Debug, Clone)]
pub struct CanvasState {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub canvas_id: Uuid,
    pub user_id: Uuid,
}

impl AppState {
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            db_pool,
            canvas_registry: Arc::new(RwLock::new(HashMap::new())),
            session_registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn active_session_count(&self) -> usize {
        self.session_registry
            .try_read()
            .map(|sessions| sessions.len())
            .unwrap_or(0)
    }

    pub fn active_canvas_count(&self) -> usize {
        self.canvas_registry
            .try_read()
            .map(|canvases| canvases.len())
            .unwrap_or(0)
    }
}