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
use state::AppState;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<(), AppError> {
    init_tracing();

    let config = Config::from_env();
    let pool = create_pool(&config.database_url).await?;
    let app_state = AppState::new(pool);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(handlers::health_check))
        .route("/ws", get(ws::ws_handler))
        .with_state(app_state)
        .layer(cors);

    let addr = config.socket_addr();
    info!(
        %addr,
        canvas_width = config.canvas_width,
        canvas_height = config.canvas_height,
        max_sessions = config.max_sessions,
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
