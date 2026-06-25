//! Shared HTTP error helpers for daemon handlers.

use axum::http::StatusCode;

pub fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        err.to_string(),
    )
}
