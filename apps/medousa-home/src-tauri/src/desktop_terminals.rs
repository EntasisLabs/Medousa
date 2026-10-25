//! Resolve terminal launch intent into an existing daemon-owned PTY identity.
//! Closing a renderer never kills a session; reopening a destination can attach it again.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::desktop_windows::DesktopTarget;
use crate::pairing_client::WorkshopTransportConfig;
use crate::terminal::TerminalSessionSummary;
use crate::workshop_transport::{workshop_get_json, workshop_post_json};
type LaunchSlot = Arc<tokio::sync::Mutex<Launch>>;

#[derive(Default)]
pub struct TerminalWindows {
    // Serialize each destination across renderers. An offline workshop must not
    // hold up terminal launches on other workshops or destinations.
    launches: Mutex<HashMap<(String, Option<String>), LaunchSlot>>,
}

struct Launch {
    request_key: String,
    session_id: Option<String>,
    title: String,
}

#[derive(Deserialize)]
struct Sessions {
    sessions: Vec<TerminalSessionSummary>,
}

impl TerminalWindows {
    pub async fn resolve(
        &self,
        config: &WorkshopTransportConfig,
        target: &mut DesktopTarget,
    ) -> Result<(), String> {
        if let Some(id) = &target.resource_id {
            let sessions: Sessions = workshop_get_json(config, "/v1/sessions/shell").await?;
            if !sessions
                .sessions
                .iter()
                .any(|session| &session.session_id == id && !session.exited)
            {
                return Err("This terminal session has ended. Open a new terminal.".into());
            }
            return Ok(());
        }
        let key = (target.workshop_id.clone(), target.ssh_target_id.clone());
        let slot = self
            .launches
            .lock()
            .map_err(|error| error.to_string())?
            .entry(key)
            .or_insert_with(|| {
                Arc::new(tokio::sync::Mutex::new(Launch {
                    request_key: uuid::Uuid::new_v4().to_string(),
                    session_id: None,
                    title: "Terminal".into(),
                }))
            })
            .clone();
        let mut launch = slot.lock().await;
        let sessions: Sessions = workshop_get_json(config, "/v1/sessions/shell").await?;
        if let Some(id) = &launch.session_id {
            if sessions
                .sessions
                .iter()
                .any(|session| &session.session_id == id && !session.exited)
            {
                target.resource_id = Some(id.clone());
                target.ssh_target_id = None;
                target.title = Some(launch.title.clone());
                return Ok(());
            }
            // A known session ended. This explicit launch may create a new one.
            launch.session_id = None;
            launch.request_key = uuid::Uuid::new_v4().to_string();
        }
        let response: Value = if let Some(id) = &target.ssh_target_id {
            let targets: Value = workshop_get_json(config, "/v1/ssh/targets").await?;
            let saved = targets
                .get("targets")
                .and_then(Value::as_array)
                .and_then(|targets| {
                    targets
                        .iter()
                        .find(|entry| entry["target_id"].as_str() == Some(id))
                })
                .ok_or("This SSH destination is no longer available. Refresh the picker.")?;
            launch.title = format!("SSH · {}", saved["name"].as_str().unwrap_or("Server"));
            // Preserve the key even if transport fails after the daemon accepted it.
            // Retrying can observe the same receipt, never replay a remote operation.
            workshop_post_json(
                config,
                "/v1/ssh/terminal",
                &json!({
                    "target_id": id, "request_key": launch.request_key,
                }),
            )
            .await?
        } else {
            workshop_post_json(
                config,
                "/v1/sessions/shell",
                &json!({
                    "work_id": null, "cwd": null, "lease_id": null, "cols": 80, "rows": 24,
                }),
            )
            .await?
        };
        let id = response["session_id"]
            .as_str()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| {
                response["error"]
                    .as_str()
                    .unwrap_or("The workshop did not return a terminal session.")
                    .to_string()
            })?;
        launch.session_id = Some(id.into());
        target.resource_id = Some(id.into());
        target.ssh_target_id = None;
        target.title = Some(launch.title.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Bytes,
        extract::State,
        http::{Method, StatusCode, Uri},
        Json, Router,
    };
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };

    #[derive(Default)]
    struct Daemon {
        sessions: Mutex<Vec<Value>>,
        requests: Mutex<Vec<(String, Value)>>,
        receipts: Mutex<HashMap<String, String>>,
        lose_reply: AtomicBool,
        hold_sessions: AtomicBool,
        entered: tokio::sync::Notify,
        resume: tokio::sync::Notify,
    }
    fn session(id: &str, exited: bool) -> Value {
        json!({"session_id":id,"cwd":"/repo","root_kind":"scripts","work_id":null,"argv":["sh"],"exited":exited})
    }
    async fn serve(
        State(state): State<Arc<Daemon>>,
        method: Method,
        uri: Uri,
        body: Bytes,
    ) -> Result<Json<Value>, (StatusCode, &'static str)> {
        match (method, uri.path()) {
            (Method::GET, "/v1/sessions/shell") => {
                if state.hold_sessions.load(Ordering::SeqCst) {
                    state.entered.notify_one();
                    state.resume.notified().await;
                }
                Ok(Json(json!({"sessions": *state.sessions.lock().unwrap()})))
            }
            (Method::GET, "/v1/ssh/targets") => {
                Ok(Json(json!({"targets":[{"target_id":"nas","name":"NAS"}]})))
            }
            (Method::POST, path @ ("/v1/sessions/shell" | "/v1/ssh/terminal")) => {
                let input: Value = serde_json::from_slice(&body).unwrap();
                let mut requests = state.requests.lock().unwrap();
                requests.push((path.into(), input.clone()));
                let mut receipts = state.receipts.lock().unwrap();
                let key = input["request_key"].as_str().unwrap_or("").to_string();
                let id = if !key.is_empty() && receipts.contains_key(&key) {
                    receipts[&key].clone()
                } else {
                    let id = format!("pty-{}", requests.len());
                    state.sessions.lock().unwrap().push(session(&id, false));
                    if !key.is_empty() {
                        receipts.insert(key, id.clone());
                    }
                    id
                };
                if state.lose_reply.swap(false, Ordering::SeqCst) {
                    return Err((StatusCode::BAD_GATEWAY, "reply lost after admission"));
                }
                Ok(Json(json!({"session_id":id})))
            }
            _ => Err((StatusCode::NOT_FOUND, "unknown fixture route")),
        }
    }
    struct Server {
        daemon: Arc<Daemon>,
        config: WorkshopTransportConfig,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn server() -> Server {
        let daemon = Arc::new(Daemon::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = WorkshopTransportConfig {
            lan_base: format!("http://{}", listener.local_addr().unwrap()),
            iroh_ticket: None,
            session_token: None,
            pairing_id: String::new(),
            session_expires_at: None,
            phone_id: String::new(),
            workshop_device_id: String::new(),
            daemon_public_key: None,
        };
        let router = Router::new().fallback(serve).with_state(daemon.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Server {
            daemon,
            config,
            task,
        }
    }
    fn target(ssh: bool) -> DesktopTarget {
        DesktopTarget {
            kind: crate::desktop_windows::DesktopKind::Terminal,
            workshop_id: "a".into(),
            resource_id: None,
            ssh_target_id: ssh.then(|| "nas".into()),
            path: None,
            title: None,
        }
    }

    #[tokio::test]
    async fn an_unreachable_workshop_does_not_block_other_destinations() {
        let slow = server().await;
        let fast = server().await;
        slow.daemon.hold_sessions.store(true, Ordering::SeqCst);
        let windows = Arc::new(TerminalWindows::default());
        let owner = windows.clone();
        let config = slow.config.clone();
        let pending = tokio::spawn(async move { owner.resolve(&config, &mut target(false)).await });
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            slow.daemon.entered.notified(),
        )
        .await
        .unwrap();
        let mut other = target(false);
        other.workshop_id = "b".into();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            windows.resolve(&fast.config, &mut other),
        )
        .await
        .unwrap()
        .unwrap();
        slow.daemon.resume.notify_one();
        pending.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn concurrent_launches_reuse_a_live_pty_and_scope_caches_do_not_alias() {
        let server = server().await;
        let windows = TerminalWindows::default();
        let (mut first, mut second) = (target(true), target(true));
        let (a, b) = tokio::join!(
            windows.resolve(&server.config, &mut first),
            windows.resolve(&server.config, &mut second)
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(first.resource_id, second.resource_id);
        assert_eq!(server.daemon.requests.lock().unwrap().len(), 1);
        // Reopening after its renderer closes resolves the same resource too.
        let mut reopened = target(true);
        windows
            .resolve(&server.config, &mut reopened)
            .await
            .unwrap();
        assert_eq!(reopened.resource_id, first.resource_id);
        let mut other = target(true);
        other.workshop_id = "b".into();
        windows.resolve(&server.config, &mut other).await.unwrap();
        assert_ne!(other.resource_id, first.resource_id);
    }
    #[tokio::test]
    async fn lost_ssh_replies_reuse_the_admitted_request_key() {
        let server = server().await;
        let windows = TerminalWindows::default();
        server.daemon.lose_reply.store(true, Ordering::SeqCst);
        assert!(windows
            .resolve(&server.config, &mut target(true))
            .await
            .is_err());
        let mut retry = target(true);
        windows.resolve(&server.config, &mut retry).await.unwrap();
        let requests = server.daemon.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].1["request_key"], requests[1].1["request_key"]);
        assert_eq!(server.daemon.sessions.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn shells_reuse_live_sessions_but_explicitly_relaunch_after_exit() {
        let server = server().await;
        let windows = TerminalWindows::default();
        let mut first = target(false);
        windows.resolve(&server.config, &mut first).await.unwrap();
        let mut second = target(false);
        windows.resolve(&server.config, &mut second).await.unwrap();
        assert_eq!(first.resource_id, second.resource_id);
        server.daemon.sessions.lock().unwrap()[0]["exited"] = json!(true);
        let mut third = target(false);
        windows.resolve(&server.config, &mut third).await.unwrap();
        assert_ne!(first.resource_id, third.resource_id);
        let mut ended = target(false);
        ended.resource_id = first.resource_id;
        assert!(windows.resolve(&server.config, &mut ended).await.is_err());
        assert_eq!(server.daemon.requests.lock().unwrap().len(), 2);
    }
    #[tokio::test]
    async fn existing_session_opens_never_create_another_pty() {
        let server = server().await;
        server
            .daemon
            .sessions
            .lock()
            .unwrap()
            .push(session("saved", false));
        let windows = TerminalWindows::default();
        let mut saved = target(false);
        saved.resource_id = Some("saved".into());
        windows.resolve(&server.config, &mut saved).await.unwrap();
        saved.resource_id = Some("missing".into());
        assert!(windows.resolve(&server.config, &mut saved).await.is_err());
        let mut missing_target = target(true);
        missing_target.ssh_target_id = Some("removed".into());
        assert!(windows
            .resolve(&server.config, &mut missing_target)
            .await
            .is_err());
        assert!(server.daemon.requests.lock().unwrap().is_empty());
    }
}
