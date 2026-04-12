//! Environment-backed runtime configuration for the CanvaX backend.
//!
//! This module is responsible for loading required process configuration from
//! environment variables and exposing typed values to the rest of the backend.
use std::{env, net::SocketAddr};

/// Runtime configuration required to start the backend service.
///
/// The values are loaded from environment variables through [`Config::from_env`]
/// and then reused by database, websocket, and HTTP bootstrap code.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string used by sqlx to create the application pool.
    pub database_url: String,
    /// IP address or hostname the Axum server binds to.
    pub host: String,
    /// TCP port the Axum server listens on.
    pub port: u16,
    /// Width of the collaborative pixel grid in cells.
    pub canvas_width: u32,
    /// Height of the collaborative pixel grid in cells.
    pub canvas_height: u32,
    /// Maximum number of concurrent sessions the server should allow.
    pub max_sessions: usize,
}

impl Config {
    /// Loads required configuration values from environment variables.
    ///
    /// # Parameters
    ///
    /// This function does not accept parameters; it reads from process
    /// environment variables (`DATABASE_URL`, `HOST`, `PORT`,
    /// `CANVAS_WIDTH`, `CANVAS_HEIGHT`, and `MAX_SESSIONS`).
    ///
    /// # Returns
    ///
    /// Returns a fully-populated [`Config`] instance when all required
    /// variables are present and parseable.
    ///
    /// # Errors
    ///
    /// This function panics if a required variable is missing, empty, or has
    /// an invalid type.
    pub fn from_env() -> Self {
        dotenv::dotenv().ok();

        Self {
            database_url: required_var("DATABASE_URL"),
            host: required_var("HOST"),
            port: parse_required("PORT"),
            canvas_width: parse_required("CANVAS_WIDTH"),
            canvas_height: parse_required("CANVAS_HEIGHT"),
            max_sessions: parse_required("MAX_SESSIONS"),
        }
    }

    /// Builds the socket address used by Axum to bind the HTTP server.
    ///
    /// # Parameters
    ///
    /// - `self`: Configuration containing `host` and `port`.
    ///
    /// # Returns
    ///
    /// Returns a [`SocketAddr`] parsed from `<host>:<port>`.
    ///
    /// # Errors
    ///
    /// This function panics if the host/port combination cannot be parsed
    /// into a valid socket address.
    pub fn socket_addr(&self) -> SocketAddr {
        let combined = format!("{}:{}", self.host, self.port);
        combined
            .parse::<SocketAddr>()
            .unwrap_or_else(|error| panic!("invalid HOST/PORT combination '{combined}': {error}"))
    }
}

fn required_var(key: &str) -> String {
    let value = env::var(key)
        .unwrap_or_else(|_| panic!("missing required environment variable: {key}"));

    if value.trim().is_empty() {
        panic!("environment variable {key} is required and cannot be empty");
    }

    value
}

fn parse_required<T>(key: &str) -> T
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let raw = required_var(key);
    raw.parse::<T>()
        .unwrap_or_else(|error| panic!("environment variable {key} has invalid value '{raw}': {error}"))
}