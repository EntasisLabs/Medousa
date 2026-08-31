use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use medousa_sdk::{SdkError, Transport};

type Handler = Box<dyn Fn() -> serde_json::Value + Send + Sync>;
type RequestHeaders = Arc<Mutex<Vec<Vec<(String, String)>>>>;

struct MockTransport {
    handlers: HashMap<(String, String), Handler>,
    calls: Arc<Mutex<Vec<(String, String)>>>,
    sse_batches: Arc<Mutex<VecDeque<Vec<bytes::Bytes>>>>,
    request_headers: RequestHeaders,
}

impl MockTransport {
    fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            calls: Arc::new(Mutex::new(Vec::new())),
            sse_batches: Arc::new(Mutex::new(VecDeque::new())),
            request_headers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn on_get(mut self, path: &str, value: serde_json::Value) -> Self {
        self.handlers.insert(
            ("GET".to_string(), path.to_string()),
            Box::new(move || value.clone()),
        );
        self
    }

    fn on_post(mut self, path: &str, value: serde_json::Value) -> Self {
        self.handlers.insert(
            ("POST".to_string(), path.to_string()),
            Box::new(move || value.clone()),
        );
        self
    }

    fn on_delete(mut self, path: &str, value: serde_json::Value) -> Self {
        self.handlers.insert(
            ("DELETE".to_string(), path.to_string()),
            Box::new(move || value.clone()),
        );
        self
    }

    fn call_count(&self) -> usize {
        self.calls.lock().expect("calls lock").len()
    }

    fn with_sse_batches(self, batches: Vec<Vec<bytes::Bytes>>) -> Self {
        self.sse_batches
            .lock()
            .expect("SSE batches lock")
            .extend(batches);
        self
    }

    fn dispatch(&self, method: &str, path: &str) -> Result<serde_json::Value, SdkError> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((method.to_string(), path.to_string()));
        self.handlers
            .get(&(method.to_string(), path.to_string()))
            .map(|handler| handler())
            .ok_or_else(|| SdkError::Transport(format!("no handler for {method} {path}")))
    }
}

impl Transport for MockTransport {
    fn get_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.dispatch("GET", &path) })
    }

    fn post_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        _body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.dispatch("POST", &path) })
    }

    fn post_json_with_headers<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        _body: serde_json::Value,
        headers: Vec<(&'static str, String)>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move {
            self.request_headers
                .lock()
                .expect("request headers lock")
                .push(
                    headers
                        .into_iter()
                        .map(|(name, value)| (name.to_string(), value))
                        .collect(),
                );
            self.dispatch("POST", &path)
        })
    }

    fn delete_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.dispatch("DELETE", &path) })
    }

    fn put_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        _body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.dispatch("PUT", &path) })
    }

    fn patch_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        _body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.dispatch("PATCH", &path) })
    }

    fn post_empty_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.dispatch("POST", &path) })
    }

    fn put_text<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        _body: String,
        _extra_headers: Vec<(&'static str, String)>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.dispatch("PUT", &path) })
    }

    #[cfg(feature = "sse")]
    fn stream_sse<'a>(
        &'a self,
        _base_url: &'a str,
        path: String,
    ) -> Pin<Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, SdkError>> + Send + 'a>> {
        let _path = path;
        Box::pin(futures_util::stream::once(async move {
            Err(SdkError::Transport("mock SSE not configured".to_string()))
        }))
    }

    #[cfg(feature = "sse")]
    fn stream_sse_with_accept<'a>(
        &'a self,
        _base_url: &'a str,
        path: String,
        accept: &'static str,
    ) -> Pin<Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, SdkError>> + Send + 'a>> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((accept.to_string(), path));
        let batch = self
            .sse_batches
            .lock()
            .expect("SSE batches lock")
            .pop_front()
            .unwrap_or_default();
        Box::pin(futures_util::stream::iter(batch.into_iter().map(Ok)))
    }
}

fn health_json(contract_revision: u32) -> serde_json::Value {
    serde_json::json!({
        "runtime": {
            "authority_id": format!("auth_{}", "a".repeat(64)),
            "product_version": "0.9.1",
            "build_revision": "test-build-42",
            "contract_revision": contract_revision,
            "base_schema_revision": 1,
            "deployment_profile": "full",
            "deployment_target": "full:macos:aarch64",
            "advertised_capabilities": ["transport.http"]
        },
        "status": "ok",
        "backend": "test",
        "worker_id": "worker-1",
        "now_utc": "2026-01-01T00:00:00Z"
    })
}

#[cfg(feature = "sse")]
#[tokio::test]
async fn typed_v2_stream_reconnects_with_cursor_and_dedupes_replay() {
    use futures_util::StreamExt;
    use medousa_sdk::{BackoffPolicy, ReconnectPolicy};
    use std::time::Duration;

    let first_payload = r#"{"schema_version":2,"turn_id":"turn-1","seq":1,"emitted_at_utc":"2026-08-14T00:00:00Z","event":{"type":"content_append","text":"Hel"}}"#;
    let final_payload = r#"{"schema_version":2,"turn_id":"turn-1","seq":2,"emitted_at_utc":"2026-08-14T00:00:01Z","event":{"type":"final","text":"Hello"}}"#;
    let first = bytes::Bytes::from(format!("data: {first_payload}\n\n"));
    let replay_and_final = bytes::Bytes::from(format!(
        "data: {first_payload}\n\ndata: {final_payload}\n\n"
    ));
    let transport =
        Arc::new(MockTransport::new().with_sse_batches(vec![vec![first], vec![replay_and_final]]));
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");
    let policy = ReconnectPolicy {
        backoff: BackoffPolicy {
            base: Duration::ZERO,
            max: Duration::ZERO,
            ..BackoffPolicy::default()
        },
        ..ReconnectPolicy::default()
    };

    let events: Vec<_> = client
        .interactive()
        .stream_reconnecting_v2_with_policy(
            "https://workshop.example/v1/interactive/turn/turn-1/stream",
            policy,
        )
        .map(|event| event.expect("v2 event"))
        .collect()
        .await;

    assert_eq!(
        events.iter().map(|event| event.seq).collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        transport.calls.lock().expect("calls lock").as_slice(),
        [
            (
                medousa_types::turn_stream::TURN_STREAM_V2_MEDIA_TYPE.to_string(),
                "https://workshop.example/v1/interactive/turn/turn-1/stream".to_string(),
            ),
            (
                medousa_types::turn_stream::TURN_STREAM_V2_MEDIA_TYPE.to_string(),
                "https://workshop.example/v1/interactive/turn/turn-1/stream?since=1".to_string(),
            ),
        ]
    );
}

#[cfg(feature = "sse")]
#[tokio::test]
async fn typed_v3_stream_reconnects_with_raw_fact_cursor_and_dedupes_replay() {
    use futures_util::StreamExt;
    use medousa_sdk::{BackoffPolicy, ReconnectPolicy};
    use std::time::Duration;

    let started = r#"{"schema_version":3,"turn_id":"turn-1","seq":1,"emitted_at_utc":"2026-08-14T00:00:00Z","event":{"type":"assistant_text_started","segment_id":"segment-1","model_round":1}}"#;
    let append = r#"{"schema_version":3,"turn_id":"turn-1","seq":2,"emitted_at_utc":"2026-08-14T00:00:01Z","event":{"type":"content_append","segment_id":"segment-1","text":"Hello"}}"#;
    let completed = r#"{"schema_version":3,"turn_id":"turn-1","seq":3,"emitted_at_utc":"2026-08-14T00:00:02Z","event":{"type":"turn_completed","outcome":"completed","aggregate_text":"Hello"}}"#;
    let first = bytes::Bytes::from(format!("data: {started}\n\n"));
    let replay_and_terminal = bytes::Bytes::from(format!(
        "data: {started}\n\ndata: {append}\n\ndata: {completed}\n\n"
    ));
    let transport = Arc::new(
        MockTransport::new().with_sse_batches(vec![vec![first], vec![replay_and_terminal]]),
    );
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");
    let policy = ReconnectPolicy {
        backoff: BackoffPolicy {
            base: Duration::ZERO,
            max: Duration::ZERO,
            ..BackoffPolicy::default()
        },
        ..ReconnectPolicy::default()
    };

    let events: Vec<_> = client
        .interactive()
        .stream_reconnecting_v3_with_policy(
            "https://workshop.example/v1/interactive/turn/turn-1/stream",
            policy,
        )
        .map(|event| event.expect("v3 event"))
        .collect()
        .await;

    assert_eq!(
        events.iter().map(|event| event.seq).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(
        transport.calls.lock().expect("calls lock").as_slice(),
        [
            (
                medousa_types::turn_stream::TURN_STREAM_V3_MEDIA_TYPE.to_string(),
                "https://workshop.example/v1/interactive/turn/turn-1/stream".to_string(),
            ),
            (
                medousa_types::turn_stream::TURN_STREAM_V3_MEDIA_TYPE.to_string(),
                "https://workshop.example/v1/interactive/turn/turn-1/stream?since=1".to_string(),
            ),
        ]
    );
}

#[cfg(feature = "sse")]
#[tokio::test]
async fn typed_v2_stream_exhausts_retries_without_sequence_progress() {
    use futures_util::StreamExt;
    use medousa_sdk::{BackoffPolicy, ReconnectPolicy};
    use std::time::Duration;

    let transport = Arc::new(MockTransport::new().with_sse_batches(vec![vec![], vec![]]));
    let client = medousa_sdk::MedousaClient::with_transport(transport, "http://127.0.0.1:8080");
    let policy = ReconnectPolicy {
        backoff: BackoffPolicy {
            base: Duration::ZERO,
            max: Duration::ZERO,
            max_attempts: Some(1),
            ..BackoffPolicy::default()
        },
        ..ReconnectPolicy::default()
    };
    let error = client
        .interactive()
        .stream_reconnecting_v2_with_policy("/stream", policy)
        .next()
        .await
        .expect("reconnect error")
        .expect_err("stream should exhaust retries");

    assert!(error.to_string().contains("attempts exhausted"));
}

#[tokio::test]
async fn mock_transport_routes_health_get() {
    let transport = Arc::new(MockTransport::new().on_get(
        "/v1/health",
        health_json(medousa_sdk::DAEMON_API_CONTRACT_REVISION),
    ));
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");
    let health = client.health().get().await.expect("health get");
    assert_eq!(health.status, "ok");
    assert_eq!(health.runtime.build_revision, "test-build-42");
    assert_eq!(transport.call_count(), 1);
}

#[tokio::test]
async fn health_rejects_missing_or_incompatible_contract_identity() {
    let missing = Arc::new(MockTransport::new().on_get(
        "/v1/health",
        serde_json::json!({
            "status": "ok",
            "backend": "test",
            "worker_id": "old-worker",
            "now_utc": "2026-01-01T00:00:00Z"
        }),
    ));
    let client = medousa_sdk::MedousaClient::with_transport(missing, "http://127.0.0.1:8080");
    let error = client
        .health()
        .get()
        .await
        .expect_err("descriptor required");
    assert!(matches!(error, medousa_sdk::SdkError::Compatibility(_)));
    assert!(
        error
            .to_string()
            .contains("omitted the required runtime descriptor")
    );

    let mut invalid_descriptor = health_json(medousa_sdk::DAEMON_API_CONTRACT_REVISION);
    invalid_descriptor["runtime"]
        .as_object_mut()
        .expect("runtime object")
        .remove("authority_id");
    let invalid = Arc::new(MockTransport::new().on_get("/v1/health", invalid_descriptor));
    let client = medousa_sdk::MedousaClient::with_transport(invalid, "http://127.0.0.1:8080");
    let error = client
        .health()
        .get()
        .await
        .expect_err("descriptor fields are required");
    assert!(matches!(error, medousa_sdk::SdkError::Compatibility(_)));
    assert!(error.to_string().contains("invalid health contract"));

    let incompatible = Arc::new(MockTransport::new().on_get(
        "/v1/health",
        health_json(medousa_sdk::DAEMON_API_CONTRACT_REVISION + 1),
    ));
    let client = medousa_sdk::MedousaClient::with_transport(incompatible, "http://127.0.0.1:8080");
    let error = client
        .health()
        .get()
        .await
        .expect_err("revision must match");
    assert!(matches!(error, medousa_sdk::SdkError::Compatibility(_)));
    assert!(error.to_string().contains("test-build-42"));
    assert!(error.to_string().contains("contract revision"));
}

#[tokio::test]
async fn session_create_and_history_keep_required_workshop_authority() {
    use medousa_types::CreateSessionRequest;

    let authority = format!("auth_{}", "b".repeat(64));
    let transport = Arc::new(
        MockTransport::new()
            .on_post(
                "/v1/sessions",
                serde_json::json!({
                    "authority_id": authority.clone(),
                    "session_id": "session-1",
                    "catalog": "single"
                }),
            )
            .on_get(
                "/v1/sessions/session-1/history",
                serde_json::json!({
                    "authority_id": authority,
                    "session_id": "session-1",
                    "turns": []
                }),
            ),
    );
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");
    let created = client
        .sessions()
        .create(&CreateSessionRequest {
            session_id: None,
            catalog: None,
            member_profile_ids: None,
            agent_profile_id: None,
            display_name: None,
        })
        .await
        .expect("create session");
    let history = client
        .sessions()
        .history(&created.session_id)
        .await
        .expect("session history");
    assert_eq!(created.authority_id, history.authority_id);
    assert_eq!(transport.call_count(), 2);
}

#[tokio::test]
async fn missing_history_authority_names_the_responder_build() {
    let transport = Arc::new(
        MockTransport::new()
            .on_get(
                "/v1/sessions/session-1/history",
                serde_json::json!({ "session_id": "session-1", "turns": [] }),
            )
            .on_get(
                "/v1/health",
                health_json(medousa_sdk::DAEMON_API_CONTRACT_REVISION),
            ),
    );
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");
    let error = client
        .sessions()
        .history("session-1")
        .await
        .expect_err("authority is required");
    assert!(matches!(error, medousa_sdk::SdkError::Compatibility(_)));
    let message = error.to_string();
    assert!(message.contains("GET /v1/sessions/session-1/history"));
    assert!(message.contains("test-build-42"));
    assert!(message.contains("authority_id"));
    assert_eq!(transport.call_count(), 2);
}

#[tokio::test]
async fn history_page_reuses_the_history_route_with_cursor_query() {
    let authority = format!("auth_{}", "c".repeat(64));
    let transport = Arc::new(MockTransport::new().on_get(
        "/v1/sessions/session-1/history?limit=24&cursor=25",
        serde_json::json!({
            "authority_id": authority,
            "session_id": "session-1",
            "turns": [],
            "next_cursor": "1"
        }),
    ));
    let client = medousa_sdk::MedousaClient::with_transport(transport, "http://127.0.0.1:8080");

    let page = client
        .sessions()
        .history_page("session-1", 24, Some("25"))
        .await
        .expect("paged session history");

    assert_eq!(page.next_cursor.as_deref(), Some("1"));
}

#[tokio::test]
async fn mock_transport_routes_prompt_stash_lifecycle() {
    use medousa_types::{CreatePromptStashRequest, PromptStashDraft};

    let stash = serde_json::json!({
        "stash_id": "pst_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "label": "Follow up",
        "draft": { "text": "ask this next" },
        "created_by": "user:local",
        "created_at": "2026-08-21T00:00:00Z",
        "updated_at": "2026-08-21T00:00:00Z"
    });
    let transport = Arc::new(
        MockTransport::new()
            .on_get(
                "/v1/prompt-stashes",
                serde_json::json!({ "stashes": [stash.clone()] }),
            )
            .on_post("/v1/prompt-stashes", stash)
            .on_delete(
                "/v1/prompt-stashes/pst_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                serde_json::json!({
                    "stash_id": "pst_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "deleted": true
                }),
            ),
    );
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");

    let listed = client.prompt_stashes().list().await.expect("list stashes");
    assert_eq!(listed.stashes.len(), 1);
    let created = client
        .prompt_stashes()
        .create(&CreatePromptStashRequest {
            label: Some("Follow up".to_string()),
            draft: PromptStashDraft {
                text: "ask this next".to_string(),
                media_refs: vec![],
                mode: None,
                model: None,
            },
            context_manifest_id: None,
            source_session: None,
        })
        .await
        .expect("create stash");
    assert_eq!(created.label.as_deref(), Some("Follow up"));
    let deleted = client
        .prompt_stashes()
        .delete(created.stash_id.as_str())
        .await
        .expect("delete stash");
    assert!(deleted.deleted);
    assert_eq!(transport.call_count(), 3);
}

#[tokio::test]
async fn mock_transport_routes_session_derivation_with_idempotency() {
    let authority = format!("auth_{}", "a".repeat(64));
    let created_at = "2026-08-21T12:00:00Z";
    let transport = Arc::new(MockTransport::new().on_post(
        "/v1/sessions/derive",
        serde_json::json!({
            "authority_id": authority,
            "session_id": "target-session",
            "catalog": "single",
            "reused": false,
            "derivation": {
                "derivation_id": format!("drv_{}", "d".repeat(32)),
                "target_session": {
                    "authority_id": authority,
                    "session_id": "target-session"
                },
                "manifest": {
                    "manifest_id": format!("ctx_{}", "c".repeat(32)),
                    "sources": [{
                        "selection": {
                            "session": {
                                "authority_id": authority,
                                "session_id": "source-session"
                            },
                            "through_entry_seq": 2
                        },
                        "selection_digest": "sha256:selection"
                    }],
                    "created_by": "profile:user:test",
                    "created_at": created_at
                },
                "intent": "fork",
                "created_by": "profile:user:test",
                "created_at": created_at
            }
        }),
    ));
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");
    let request = medousa_types::DeriveSessionRequest {
        sources: vec![medousa_types::ConversationRangeSelection {
            session: medousa_types::SessionRef {
                authority_id: medousa_types::AuthorityId::parse(&authority).unwrap(),
                session_id: medousa_types::SessionId::parse("source-session").unwrap(),
            },
            after_entry_seq: None,
            through_entry_seq: 2,
        }],
        intent: "fork".to_string(),
        target: medousa_types::DeriveSessionTarget {
            catalog: Some("single".to_string()),
            display_name: None,
        },
    };

    let response = client
        .sessions()
        .derive(&request, "derive-rust-sdk-1")
        .await
        .expect("derive response");

    assert_eq!(response.session_id, "target-session");
    assert_eq!(
        transport
            .request_headers
            .lock()
            .expect("request headers lock")
            .as_slice(),
        [vec![(
            "Idempotency-Key".to_string(),
            "derive-rust-sdk-1".to_string()
        )]]
    );
}

#[tokio::test]
async fn mock_transport_routes_jobs_enqueue_ask() {
    let transport = Arc::new(MockTransport::new().on_post(
        "/v1/jobs/ask",
        serde_json::json!({
            "job_id": "job-1",
            "queue": "default",
            "accepted_at_utc": "2026-01-01T00:00:00Z",
        }),
    ));
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");
    let response = client
        .jobs()
        .enqueue_ask(&medousa_types::EnqueueAskRequest {
            prompt: "hello".to_string(),
            policy_profile: None,
            model_hint: None,
            max_turns: None,
            identity_user_id: None,
            identity_persona_id: None,
            identity_channel_id: None,
            manuscript_id: None,
            additional_manuscript_ids: None,
            suggested_capability_ids: None,
        })
        .await
        .expect("enqueue ask");
    assert_eq!(response.job_id, "job-1");
    assert_eq!(transport.call_count(), 1);
}

#[tokio::test]
async fn mock_transport_routes_vault_list_roots() {
    let transport = Arc::new(MockTransport::new().on_get(
        "/v1/vault/roots",
        serde_json::json!({
            "roots": [],
            "activeRootId": "",
        }),
    ));
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");
    let roots = client.vault().list_roots().await.expect("vault roots");
    assert!(roots.roots.is_empty());
    assert_eq!(transport.call_count(), 1);
}
