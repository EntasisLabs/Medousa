//! Provider-neutral device sign-in. The package owns OAuth and the workshop
//! stores credentials; this bridge pins each flow to its originating workshop.
use super::window_request::WorkshopRequest;
use crate::{pairing_client::WorkshopTransportConfig, workshop_transport::workshop_post_json};
use medousa_types::review_provider::{
    ReviewProviderAuthRequest as Request, ReviewProviderAuthResponse as Response,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::LazyLock,
    time::{Duration, Instant},
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri::Manager;
use tauri::AppHandle;
use tokio::sync::Mutex;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewAuthAction {
    Begin { provider_id: String, host: String },
    Poll { flow_id: String },
    Open { flow_id: String },
    Cancel { flow_id: String },
}
#[derive(Clone)]
struct Session {
    transport: WorkshopTransportConfig,
    challenge: Response,
    expires: Instant,
}
static SESSIONS: LazyLock<Mutex<BTreeMap<String, Session>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
async fn session(id: &str) -> Result<Session, String> {
    let mut sessions = SESSIONS.lock().await;
    sessions.retain(|_, session| session.expires > Instant::now());
    sessions
        .get(id)
        .cloned()
        .ok_or_else(|| "Sign-in expired. Start again.".into())
}
async fn cancel(session: Session, flow_id: String) {
    let request = Request::Cancel { flow_id };
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        workshop_post_json::<Response, _>(&session.transport, "/v1/reviews/auth", &request),
    )
    .await;
}
#[tauri::command]
pub async fn review_provider_auth(
    app: AppHandle,
    state: WorkshopRequest,
    request: ReviewAuthAction,
) -> Result<Response, String> {
    match request {
        ReviewAuthAction::Begin { provider_id, host } => {
            let transport = state.config().await?;
            let challenge: Response = workshop_post_json(
                &transport,
                "/v1/reviews/auth",
                &Request::Begin {
                    provider_id,
                    host,
                    client_id: None,
                },
            )
            .await
            .map_err(friendly_error)?;
            let Response::Pending {
                flow_id,
                expires_in,
                ..
            } = &challenge
            else {
                return Err("The review package did not return a sign-in code.".into());
            };
            let id = flow_id.clone();
            let session = Session {
                transport,
                expires: Instant::now() + Duration::from_secs((*expires_in).min(900)),
                challenge: challenge.clone(),
            };
            let mut sessions = SESSIONS.lock().await;
            sessions.retain(|_, session| session.expires > Instant::now());
            if sessions.len() >= 16 {
                drop(sessions);
                cancel(session, id).await;
                return Err("Finish or cancel an existing sign-in first.".into());
            }
            sessions.insert(id, session);
            Ok(challenge)
        }
        ReviewAuthAction::Open { flow_id } => {
            let session = session(&flow_id).await?;
            let Response::Pending {
                verification_uri, ..
            } = &session.challenge
            else {
                unreachable!()
            };
            crate::oauth_browser::open(&app, verification_uri)?;
            Ok(session.challenge)
        }
        ReviewAuthAction::Poll { flow_id } => {
            let session = session(&flow_id).await?;
            let response: Response = workshop_post_json(
                &session.transport,
                "/v1/reviews/auth",
                &Request::Poll { flow_id },
            )
            .await
            .map_err(friendly_error)?;
            if matches!(response, Response::Connected { .. }) {
                crate::oauth_browser::dismiss(&app);
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            // Keep the pinned transport until the UI acknowledges completion by
            // cancelling its flow. A lost IPC reply can safely poll again.
            Ok(response)
        }
        ReviewAuthAction::Cancel { flow_id } => {
            let session = SESSIONS.lock().await.remove(&flow_id);
            crate::oauth_browser::dismiss(&app);
            if let Some(session) = session {
                cancel(session, flow_id).await;
            }
            Ok(Response::Cancelled)
        }
    }
}
fn friendly_error(error: String) -> String {
    if let Some(start) = error.find('{') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&error[start..]) {
            if let Some(message) = value["message"].as_str() {
                return message.to_string();
            }
        }
    }
    error
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_accepts_only_device_actions_not_callbacks_or_client_configuration() {
        for action in [
            "begin_browser",
            "complete_browser",
            "refresh",
            "arbitrary_endpoint",
        ] {
            assert!(
                serde_json::from_value::<ReviewAuthAction>(serde_json::json!({"action":action}))
                    .is_err()
            );
        }
        assert!(
            serde_json::from_value::<ReviewAuthAction>(
                serde_json::json!({"action":"begin","provider_id":"example","host":"example.test"})
            )
            .is_ok()
        );
        assert_eq!(
            friendly_error(r#"HTTP 400: {"message":"Sign-in expired."}"#.into()),
            "Sign-in expired."
        );
    }
}
