use serde::{Deserialize, Serialize};

use crate::{
    COMPUTER_DRIVER_PROTOCOL_VERSION, ComputerActionReceipt, ComputerActionRequest,
    ComputerDriverPreflight, ComputerObservation, ComputerObservationRequest,
};

/// Hard framing ceiling for one sidecar request or response.
///
/// Semantic observations are already node-bounded. The byte ceiling prevents
/// a broken or compromised local driver from growing the daemon indefinitely
/// before serde gets a chance to validate the payload.
pub const MAX_COMPUTER_DRIVER_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerDriverRequestEnvelope {
    pub protocol_version: u16,
    pub request_id: String,
    #[serde(flatten)]
    pub request: ComputerDriverRequest,
}

impl ComputerDriverRequestEnvelope {
    pub fn new(request_id: impl Into<String>, request: ComputerDriverRequest) -> Self {
        Self {
            protocol_version: COMPUTER_DRIVER_PROTOCOL_VERSION,
            request_id: request_id.into(),
            request,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ComputerDriverRequest {
    Preflight,
    Observe { request: ComputerObservationRequest },
    Act { request: ComputerActionRequest },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerDriverResponseEnvelope {
    pub protocol_version: u16,
    pub request_id: String,
    #[serde(flatten)]
    pub response: ComputerDriverResponse,
}

impl ComputerDriverResponseEnvelope {
    pub fn success(request_id: impl Into<String>, result: ComputerDriverResponseResult) -> Self {
        Self {
            protocol_version: COMPUTER_DRIVER_PROTOCOL_VERSION,
            request_id: request_id.into(),
            response: ComputerDriverResponse::Success {
                result: Box::new(result),
            },
        }
    }

    pub fn error(
        request_id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            protocol_version: COMPUTER_DRIVER_PROTOCOL_VERSION,
            request_id: request_id.into(),
            response: ComputerDriverResponse::Error {
                error: ComputerDriverError {
                    code: code.into(),
                    message: message.into(),
                    retryable,
                },
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ComputerDriverResponse {
    Success {
        result: Box<ComputerDriverResponseResult>,
    },
    Error {
        error: ComputerDriverError,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ComputerDriverResponseResult {
    Preflight { report: ComputerDriverPreflight },
    Observation { observation: ComputerObservation },
    Action { receipt: ComputerActionReceipt },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerDriverError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[cfg(test)]
mod tests {
    use medousa_world::WorldResourceId;

    use super::*;

    #[test]
    fn request_round_trips_without_an_ambiguous_payload_shape() {
        let request = ComputerDriverRequestEnvelope::new(
            "request:one",
            ComputerDriverRequest::Observe {
                request: ComputerObservationRequest::new("desktop:one", "session:one"),
            },
        );
        let value = serde_json::to_value(&request).expect("encode request");
        assert_eq!(value["method"], "observe");
        assert_eq!(value["request"]["resource_id"], "desktop:one");
        let decoded: ComputerDriverRequestEnvelope =
            serde_json::from_value(value).expect("decode request");
        assert_eq!(decoded, request);
    }

    #[test]
    fn error_response_cannot_be_mistaken_for_an_observation() {
        let response = ComputerDriverResponseEnvelope::error(
            "request:one",
            "permission_denied",
            "Accessibility permission is required",
            false,
        );
        let value = serde_json::to_value(&response).expect("encode response");
        assert_eq!(value["status"], "error");
        assert!(value.get("result").is_none());
        let decoded: ComputerDriverResponseEnvelope =
            serde_json::from_value(value).expect("decode response");
        assert_eq!(decoded, response);
    }

    #[test]
    fn observation_request_keeps_its_exact_resource_type() {
        let request = ComputerObservationRequest::new("desktop:one", "session:one");
        assert_eq!(request.resource_id, WorldResourceId::new("desktop:one"));
    }

    #[test]
    fn action_request_carries_an_exact_observation_fence() {
        let request = ComputerDriverRequestEnvelope::new(
            "request:press",
            ComputerDriverRequest::Act {
                request: ComputerActionRequest {
                    resource_id: WorldResourceId::new("desktop:one"),
                    session_id: "session:one".to_string(),
                    observation_generation: "generation:one".to_string(),
                    observation_revision: 7,
                    element_ref: "ax:button:one".to_string(),
                    action: crate::ComputerAction::Press,
                },
            },
        );
        let value = serde_json::to_value(request).expect("encode action request");
        assert_eq!(value["method"], "act");
        assert_eq!(value["request"]["observation_revision"], 7);
        assert_eq!(value["request"]["action"], "press");
    }
}
