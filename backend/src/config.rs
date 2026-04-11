// Loads and validates backend configuration from environment variables.
use crate::error::AppError;
use std::{env, net::SocketAddr};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, AppError> {
        let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse::<u16>()
            .map_err(|_| AppError::Config("PORT must be a valid u16 number".to_string()))?;

        let database_url = env::var("DATABASE_URL")
            .map_err(|_| AppError::Config("DATABASE_URL must be set".to_string()))?;

        if database_url.trim().is_empty() {
            return Err(AppError::Config(
                "DATABASE_URL cannot be empty".to_string(),
            ));
        }

        Ok(Self {
            host,
            port,
            database_url,
        })
    }

    pub fn socket_addr(&self) -> Result<SocketAddr, AppError> {
        let combined = format!("{}:{}", self.host, self.port);
        combined.parse::<SocketAddr>().map_err(AppError::from)
    }
}