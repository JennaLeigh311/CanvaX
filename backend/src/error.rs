// Defines centralized application errors and converts them into HTTP responses.
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{fmt, net::AddrParseError};

#[derive(Debug)]
pub enum AppError {
    Config(String),
    Validation(String),
    NotFound(String),
    Database(sqlx::Error),
    Io(std::io::Error),
    Address(AddrParseError),
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) => write!(f, "configuration error: {message}"),
            Self::Validation(message) => write!(f, "validation error: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Address(err) => write!(f, "address parse error: {err}"),
            Self::Internal(message) => write!(f, "internal error: {message}"),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Config(message) | Self::Validation(message) => (StatusCode::BAD_REQUEST, message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database operation failed".to_string(),
            ),
            Self::Io(_) | Self::Address(_) | Self::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            ),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<AddrParseError> for AppError {
    fn from(value: AddrParseError) -> Self {
        Self::Address(value)
    }
}