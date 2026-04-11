// Loads required backend configuration from environment variables using dotenv.
use std::{env, net::SocketAddr};

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