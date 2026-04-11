// Entry point for the CanvaX backend: loads config, initializes shared state,
// builds the router, and starts the Axum HTTP/WebSocket server.
mod config;
mod db;
mod error;
mod handlers;
mod models;
mod state;
mod ws;

use axum::{Router, routing::get};
use config::Config;
use db::create_pool;
use error::AppError;
use state::{AppState, SharedState};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<(), AppError> {
    init_tracing();

    let config = Config::from_env();
    let addr = config.socket_addr();
    let canvas_width = config.canvas_width;
    let canvas_height = config.canvas_height;
    let max_sessions = config.max_sessions;
    let pool = create_pool(&config).await;
    // Arc is required because Axum clones state across async handlers and tasks,
    // and all clones must point to the same shared pool/config instance.
    let app_state: SharedState = AppState::new(pool, config);

    // Allow all origins during local development so frontend and backend can iterate quickly.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(handlers::health_check))
        .nest("/api", handlers::routes())
        .route("/ws", get(ws::ws_handler))
        .with_state(app_state)
        .layer(cors);

    info!(
        %addr,
        canvas_width,
        canvas_height,
        max_sessions,
        "CanvaX backend starting"
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(env_filter).with_target(false).init();
}
