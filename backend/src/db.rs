// Sets up the shared PostgreSQL connection pool used across request handlers.
use crate::error::AppError;
use sqlx::{PgPool, postgres::PgPoolOptions};

pub async fn create_pool(database_url: &str) -> Result<PgPool, AppError> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await
        .map_err(AppError::from)?;

    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await
        .map_err(AppError::from)?;

    Ok(pool)
}