// Integration test that verifies migrations and basic canvas CRUD against PostgreSQL.
use std::env;

use sqlx::PgPool;
use uuid::Uuid;

fn resolve_database_url() -> String {
    env::var("TEST_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://user:password@127.0.0.1:5432/canvax".to_string())
}

#[tokio::test]
async fn db_roundtrip_canvas_insert_and_read() {
    let database_url = resolve_database_url();

    // 1) Connect to the test database.
    let pool = PgPool::connect(&database_url)
        .await
        .unwrap_or_else(|error| panic!("failed to connect to test database '{database_url}': {error}"));

    // 2) Run migrations before performing assertions.
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("failed to run migrations in test: {error}"));

    let canvas_id = Uuid::new_v4();
    let canvas_name = format!("test-canvas-{}", canvas_id);
    let width = 64_i32;
    let height = 64_i32;

    // 3) Insert a test canvas record.
    let inserted = sqlx::query_as::<_, (Uuid, String, i32, i32, chrono::DateTime<chrono::Utc>)>(
        "INSERT INTO canvases (id, name, width, height) VALUES ($1, $2, $3, $4) RETURNING id, name, width, height, created_at",
    )
    .bind(canvas_id)
    .bind(&canvas_name)
    .bind(width)
    .bind(height)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|error| panic!("failed to insert test canvas: {error}"));

    // 4) Read back and assert all fields match the inserted values.
    let fetched = sqlx::query_as::<_, (Uuid, String, i32, i32, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, name, width, height, created_at FROM canvases WHERE id = $1",
    )
    .bind(canvas_id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|error| panic!("failed to fetch inserted canvas: {error}"));

    assert_eq!(fetched.0, inserted.0);
    assert_eq!(fetched.1, inserted.1);
    assert_eq!(fetched.2, inserted.2);
    assert_eq!(fetched.3, inserted.3);
    assert_eq!(fetched.4, inserted.4);

    // 5) Clean up the inserted record so tests remain isolated.
    sqlx::query("DELETE FROM canvases WHERE id = $1")
        .bind(canvas_id)
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("failed to cleanup test canvas: {error}"));
}
