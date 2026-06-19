//! Database bootstrap helpers for PostgreSQL pool creation and startup checks.
use crate::config::Config;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

/// Creates and validates the shared PostgreSQL connection pool.
///
/// # Parameters
///
/// - `config`: Loaded runtime configuration containing the database URL.
///
/// # Returns
///
/// Returns a ready-to-use [`PgPool`] with startup migrations applied.
///
/// # Errors
///
/// This function panics if connecting to PostgreSQL fails, startup migrations
/// fail, or the post-connect health probe query fails.
pub async fn create_pool(config: &Config) -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        // Bound how long a request waits for a free connection so a saturated
        // pool fails fast instead of stalling websocket setup indefinitely.
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "failed to connect to PostgreSQL using DATABASE_URL='{}': {}",
                config.database_url, error
            )
        });

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("failed to run database migrations on startup: {error}"));

    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("database health check failed after pool creation: {error}"));

    pool
}