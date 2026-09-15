//! Daemon-owned OpenAI Live session bootstrap.
//!
//! The mobile client supplies only its WebRTC SDP offer. Provider credentials
//! remain in the selected workshop's secret authority.

use axum::{Json, http::StatusCode, routing::post};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;

const OPENAI_LIVE_SESSIONS_URL: &str = "https://api.openai.com/v1/live/sessions";
const MAX_SDP_BYTES: usize = 256 * 1024;
const MAX_LIVE_BINDINGS: usize = 64;

static LIVE_SESSION_BINDINGS: LazyLock<Mutex<BTreeMap<String, String>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSessionCreateRequest {
    pub sdp: String,
    pub session_id: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LiveSessionCreateResponse {
    pub live_session_id: String,
    pub sdp: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiLiveResponse {
    session: OpenAiLiveSession,
    transport: OpenAiLiveTransport,
}

#[derive(Debug, Deserialize)]
struct OpenAiLiveSession {
    id: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiLiveTransport {
    sdp: String,
}

pub fn surface() -> DeclaredRouter<AppState> {
    DeclaredRouter::default().route(
        RoutePolicy {
            method: axum::http::Method::POST,
            path: "/v1/live/sessions",
            group: RouteGroup::Portal,
            required_capability: Some(crate::request_principal::Capability::WorkshopInteract),
            bootstrap_public: false,
            browser_policy: BrowserPolicy::NativeOnly,
            body_limit: MAX_SDP_BYTES + 4096,
            rate_limit_class: RateLimitClass::Mutation,
        },
        post(create_live_session),
    )
}

async fn create_live_session(
    Json(request): Json<LiveSessionCreateRequest>,
) -> Result<Json<LiveSessionCreateResponse>, (StatusCode, String)> {
    let sdp = request.sdp.trim();
    let medousa_session_id = request.session_id.trim();
    if sdp.is_empty() || medousa_session_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "sdp and sessionId are required".into(),
        ));
    }
    if sdp.len() > MAX_SDP_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "SDP offer is too large".into(),
        ));
    }
    if !sdp.starts_with("v=0") {
        return Err((StatusCode::BAD_REQUEST, "invalid SDP offer".into()));
    }

    let api_key = crate::session::load_provider_api_key("openai").ok_or((
        StatusCode::PRECONDITION_FAILED,
        "Configure an OpenAI API key on this workshop to use Medousa Live".into(),
    ))?;

    let body = json!({
        "session": {
            "model": "gpt-live-1",
            "instructions": "You are Medousa's live voice. Be concise and conversational. Delegate requests that need tools or durable work to the Medousa application.",
        },
        "transport": {
            "type": "webrtc",
            "sdp": sdp,
        },
    });

    let response = reqwest::Client::new()
        .post(OPENAI_LIVE_SESSIONS_URL)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Could not reach OpenAI Live: {error}"),
            )
        })?;
    let status = response.status();
    if !status.is_success() {
        let operator_message = match status.as_u16() {
            401 | 403 => "OpenAI rejected the workshop credential",
            429 => "OpenAI Live is temporarily rate limited",
            _ => "OpenAI could not start the Live session",
        };
        return Err((StatusCode::BAD_GATEWAY, operator_message.into()));
    }

    let decoded: OpenAiLiveResponse = response.json().await.map_err(|_| {
        (
            StatusCode::BAD_GATEWAY,
            "OpenAI Live returned an invalid session response".into(),
        )
    })?;
    if decoded.session.id.trim().is_empty() || decoded.transport.sdp.trim().is_empty() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "OpenAI Live returned an incomplete session response".into(),
        ));
    }

    remember_binding(&decoded.session.id, medousa_session_id);
    Ok(Json(LiveSessionCreateResponse {
        live_session_id: decoded.session.id,
        sdp: decoded.transport.sdp,
    }))
}

fn remember_binding(live_session_id: &str, medousa_session_id: &str) {
    let Ok(mut bindings) = LIVE_SESSION_BINDINGS.lock() else {
        return;
    };
    while bindings.len() >= MAX_LIVE_BINDINGS {
        let Some(oldest) = bindings.keys().next().cloned() else {
            break;
        };
        bindings.remove(&oldest);
    }
    bindings.insert(live_session_id.to_string(), medousa_session_id.to_string());
}

pub fn medousa_session_for_live(live_session_id: &str) -> Option<String> {
    LIVE_SESSION_BINDINGS
        .lock()
        .ok()
        .and_then(|bindings| bindings.get(live_session_id).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_live_session_answer() {
        let decoded: OpenAiLiveResponse = serde_json::from_value(json!({
            "session": { "id": "live_123" },
            "transport": { "type": "webrtc", "sdp": "v=0\\r\\nanswer" }
        }))
        .expect("response");
        assert_eq!(decoded.session.id, "live_123");
        assert!(decoded.transport.sdp.starts_with("v=0"));
    }

    #[test]
    fn remembers_medousa_session_binding() {
        remember_binding("live_test", "session_test");
        assert_eq!(
            medousa_session_for_live("live_test").as_deref(),
            Some("session_test")
        );
    }
}
