// Shared application state for database and configuration access across handlers.
use crate::config::Config;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Config,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(db: PgPool, config: Config) -> SharedState {
        Arc::new(Self { db, config })
    }
}