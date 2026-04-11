// Sets up the shared PostgreSQL connection pool used across request handlers.
use crate::error::AppError;
use sqlx::{PgPool, postgres::PgPoolOptions};

pub fn create_pool(database_url: &str) -> Result<PgPool, AppError> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect_lazy(database_url)
        .map_err(AppError::from)?;

    Ok(pool)
}