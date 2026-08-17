//! Shared HTTP error helpers for daemon handlers.

use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use medousa_types::{ApiErrorEnvelope, ERROR_INTERNAL_FAILURE};

pub const UNASSIGNED_REQUEST_ID: &str = "unassigned";
const REQUEST_ID_HEADER: &str = "x-request-id";

pub fn request_id_from(headers: &HeaderMap) -> String {
    headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(UNASSIGNED_REQUEST_ID)
        .to_string()
}

pub fn code_for_status(status: StatusCode) -> &'static str {
    match status.as_u16() {
        400 => medousa_types::ERROR_INVALID_PARAMETER,
        401 => medousa_types::ERROR_AUTHENTICATION_REQUIRED,
        403 => medousa_types::ERROR_FORBIDDEN,
        404 => medousa_types::ERROR_NOT_FOUND,
        405 => medousa_types::ERROR_METHOD_NOT_ALLOWED,
        409 | 412 => medousa_types::ERROR_CONFLICT,
        413 => medousa_types::ERROR_PAYLOAD_TOO_LARGE,
        503 => medousa_types::ERROR_UNAVAILABLE_FEATURE,
        _ => ERROR_INTERNAL_FAILURE,
    }
}

#[derive(Debug, Clone)]
pub struct DeclaredApiError {
    pub status: StatusCode,
    pub envelope: ApiErrorEnvelope,
}

impl DeclaredApiError {
    pub fn from_status(
        status: StatusCode,
        message: impl Into<String>,
        request_id: impl Into<String>,
    ) -> Self {
        Self {
            status,
            envelope: ApiErrorEnvelope::new(code_for_status(status), message, request_id),
        }
    }
}

impl IntoResponse for DeclaredApiError {
    fn into_response(self) -> Response {
        envelope_response(self.status, self.envelope)
    }
}

pub fn envelope_response(status: StatusCode, envelope: ApiErrorEnvelope) -> Response {
    let body = serde_json::to_vec(&envelope).expect("error envelope");
    let mut response = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("api error response");
    if let Ok(value) = HeaderValue::from_str(&envelope.request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}

pub fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

pub async fn normalize_declared_error_envelope(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let request_id = request_id_from(request.headers());
    let response = next.run(request).await;
    wrap_plaintext_error(response, &request_id).await
}

async fn wrap_plaintext_error(response: Response, request_id: &str) -> Response {
    let status = response.status();
    if status.is_success() || status.is_informational() {
        return response;
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    if content_type.starts_with("application/json")
        || content_type.starts_with("text/event-stream")
        || content_type.starts_with("application/octet-stream")
    {
        return response;
    }
    let bytes = to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap_or_default();
    let message = String::from_utf8_lossy(&bytes);
    let message = message.trim();
    let message = if message.is_empty() {
        status
            .canonical_reason()
            .unwrap_or("request failed")
            .to_string()
    } else {
        message.to_string()
    };
    DeclaredApiError::from_status(status, message, request_id).into_response()
}
