use serde::{Deserialize, Serialize};

pub const API_ERROR_SCHEMA_VERSION: u32 = 1;

pub const ERROR_AUTHENTICATION_REQUIRED: &str = "authentication_required";
pub const ERROR_INVALID_CREDENTIAL: &str = "invalid_credential";
pub const ERROR_FORBIDDEN: &str = "forbidden";
pub const ERROR_INVALID_PARAMETER: &str = "invalid_parameter";
pub const ERROR_NOT_FOUND: &str = "not_found";
pub const ERROR_METHOD_NOT_ALLOWED: &str = "method_not_allowed";
pub const ERROR_CONFLICT: &str = "conflict";
pub const ERROR_PAYLOAD_TOO_LARGE: &str = "payload_too_large";
pub const ERROR_UNAVAILABLE_FEATURE: &str = "unavailable_feature";
pub const ERROR_INTERNAL_FAILURE: &str = "internal_failure";

/// Versioned `/v1` error envelope. `code` is a stable machine vocabulary;
/// unknown future codes must be treated as forward-compatible by clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ApiErrorEnvelope {
    pub schema_version: u32,
    pub code: String,
    pub message: String,
    pub request_id: String,
    #[serde(default)]
    pub details: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

impl ApiErrorEnvelope {
    pub fn new(code: impl Into<String>, message: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            schema_version: API_ERROR_SCHEMA_VERSION,
            code: code.into(),
            message: message.into(),
            request_id: request_id.into(),
            details: serde_json::Value::Object(serde_json::Map::new()),
            retry_after_ms: None,
        }
    }

    pub fn is_known_core_code(code: &str) -> bool {
        matches!(
            code,
            ERROR_AUTHENTICATION_REQUIRED
                | ERROR_INVALID_CREDENTIAL
                | ERROR_FORBIDDEN
                | ERROR_INVALID_PARAMETER
                | ERROR_NOT_FOUND
                | ERROR_METHOD_NOT_ALLOWED
                | ERROR_CONFLICT
                | ERROR_PAYLOAD_TOO_LARGE
                | ERROR_UNAVAILABLE_FEATURE
                | ERROR_INTERNAL_FAILURE
        )
    }
}
