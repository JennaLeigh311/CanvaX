// Defines centralized application errors and converts them into HTTP responses.
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::fmt;

/// Application-level error type used by HTTP handlers.
#[derive(Debug)]
pub enum AppError {
    /// Returned when a requested resource does not exist.
    NotFound(String),
    /// Returned when input validation fails for request payloads/params.
    ValidationError(String),
    /// Wraps database errors from SQLx operations.
    DatabaseError(sqlx::Error),
    /// Returned for unexpected non-database runtime errors.
    InternalError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::ValidationError(message) => write!(f, "validation error: {message}"),
            Self::DatabaseError(err) => write!(f, "database error: {err}"),
            Self::InternalError(message) => write!(f, "internal error: {message}"),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::ValidationError(message) => (StatusCode::BAD_REQUEST, message),
            Self::DatabaseError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database operation failed".to_string(),
            ),
            Self::InternalError(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                message,
            ),
        };

        (status, Json(json!({ "message": message }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        Self::DatabaseError(value)
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::InternalError(format!("I/O error: {value}"))
    }
}