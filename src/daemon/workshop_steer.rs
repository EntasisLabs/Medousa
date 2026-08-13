use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use crate::agent_runtime::turn_worker_tools::steer_bound_workshop_for_session;
use crate::daemon::state::AppState;
use crate::request_principal::{PrincipalKind, RequestPrincipal};

#[derive(Debug, Deserialize)]
pub struct WorkshopSteerRequest {
    pub message: String,
}

pub async fn steer_bound_workshop_handler(
    State(_state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(session_id): Path<String>,
    Json(body): Json<WorkshopSteerRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let speaker = resolve_steer_speaker(&principal);
    match steer_bound_workshop_for_session(session_id.trim(), body.message.trim(), speaker) {
        Ok(value) => {
            let status = if value.is_ok() {
                StatusCode::OK
            } else {
                StatusCode::CONFLICT
            };
            let value = serde_json::to_value(value)
                .unwrap_or_else(|error| json!({ "ok": false, "error": error.to_string() }));
            (status, Json(value))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "error": err.to_string() })),
        ),
    }
}

fn resolve_steer_speaker(principal: &RequestPrincipal) -> Option<String> {
    if let Some(bound) = principal.profile_id() {
        return Some(bound.to_string());
    }
    if principal.kind() == PrincipalKind::LegacyLocal {
        return Some(crate::user_profiles::resolve_workshop_identity_user_id());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_principal::TransportClass;

    #[test]
    fn anonymous_principal_cannot_select_a_speaker() {
        assert_eq!(
            resolve_steer_speaker(&RequestPrincipal::anonymous(TransportClass::Direct)),
            None
        );
    }

    #[test]
    fn legacy_local_uses_the_workshop_identity() {
        assert_eq!(
            resolve_steer_speaker(&RequestPrincipal::legacy_local()),
            Some(crate::user_profiles::resolve_workshop_identity_user_id())
        );
    }
}
