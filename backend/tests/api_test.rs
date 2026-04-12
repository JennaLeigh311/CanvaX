// Integration tests for Phase 2 REST canvas API endpoints using Axum request helpers.
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::get,
};
use canvax_backend::{
    config::Config,
    handlers,
    models::{Canvas, CanvasStateSnapshot},
    state::{AppState, SharedState},
    ws,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn resolve_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://user:password@127.0.0.1:5432/canvax".to_string())
}

async fn build_test_router() -> (Router, PgPool) {
    let database_url = resolve_database_url();
    let pool = PgPool::connect(&database_url)
        .await
        .unwrap_or_else(|error| panic!("failed to connect to test database '{database_url}': {error}"));

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("failed to run migrations in API test: {error}"));

    // Keep tests isolated by clearing canvases before each scenario.
    sqlx::query("DELETE FROM canvases")
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("failed to clean canvases table: {error}"));

    let config = Config {
        database_url,
        host: "127.0.0.1".to_string(),
        port: 8080,
        canvas_width: 64,
        canvas_height: 64,
        max_sessions: 500,
    };
    let state: SharedState = AppState::new(pool.clone(), config);

    let app = Router::new()
        .route("/", get(handlers::health_check))
        .nest("/api", handlers::routes())
        .route("/ws", get(ws::ws_handler))
        .with_state(state);

    (app, pool)
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: String,
}

#[tokio::test]
async fn api_canvas_crud_flow() {
    let (app, pool) = build_test_router().await;

    // 1) POST /api/canvases with valid body -> 201.
    let create_body = json!({
        "name": "Integration Test Canvas",
        "width": 32,
        "height": 16
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/canvases")
                .header("content-type", "application/json")
                .body(Body::from(create_body.to_string()))
                .expect("failed to build create request"),
        )
        .await
        .expect("create request failed");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("failed to read create response body");
    let created_canvas: Canvas =
        serde_json::from_slice(&body).expect("failed to deserialize created canvas");

    // 2) POST /api/canvases with empty name -> 400.
    let invalid_body = json!({ "name": "   ", "width": 32, "height": 16 });
    let invalid_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/canvases")
                .header("content-type", "application/json")
                .body(Body::from(invalid_body.to_string()))
                .expect("failed to build invalid request"),
        )
        .await
        .expect("invalid create request failed");
    assert_eq!(invalid_response.status(), StatusCode::BAD_REQUEST);
    let invalid_body_bytes = to_bytes(invalid_response.into_body(), 1024 * 1024)
        .await
        .expect("failed to read invalid response body");
    let invalid_message: ErrorBody =
        serde_json::from_slice(&invalid_body_bytes).expect("failed to parse error body");
    assert!(invalid_message.message.contains("name cannot be empty"));

    // 3) GET /api/canvases/:id with created id -> 200 snapshot.
    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/canvases/{}", created_canvas.id))
                .body(Body::empty())
                .expect("failed to build get request"),
        )
        .await
        .expect("get request failed");
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_bytes = to_bytes(get_response.into_body(), 1024 * 1024)
        .await
        .expect("failed to read get response body");
    let snapshot: CanvasStateSnapshot =
        serde_json::from_slice(&get_bytes).expect("failed to parse snapshot");
    assert_eq!(snapshot.canvas.id, created_canvas.id);
    assert_eq!(snapshot.canvas.name, "Integration Test Canvas");
    assert!(snapshot.pixels.is_empty());

    // 4) GET /api/canvases/:fake-uuid -> 404.
    let fake_id = Uuid::new_v4();
    let fake_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/canvases/{fake_id}"))
                .body(Body::empty())
                .expect("failed to build fake get request"),
        )
        .await
        .expect("fake get request failed");
    assert_eq!(fake_response.status(), StatusCode::NOT_FOUND);
    let fake_body = to_bytes(fake_response.into_body(), 1024 * 1024)
        .await
        .expect("failed to read fake response body");
    let fake_message: ErrorBody =
        serde_json::from_slice(&fake_body).expect("failed to parse fake error body");
    assert!(fake_message.message.contains("was not found"));

    // 5) DELETE /api/canvases/:id -> 204.
    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/canvases/{}", created_canvas.id))
                .body(Body::empty())
                .expect("failed to build delete request"),
        )
        .await
        .expect("delete request failed");
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    // Cleanup verification to keep tests deterministic.
    let count_row: (i64,) = sqlx::query_as("SELECT COUNT(*)::bigint FROM canvases WHERE id = $1")
        .bind(created_canvas.id)
        .fetch_one(&pool)
        .await
        .expect("failed to validate canvas cleanup");
    assert_eq!(count_row.0, 0);
}
