use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use medousa_sdk::{SdkError, Transport};

type Handler = Box<dyn Fn() -> serde_json::Value + Send + Sync>;

struct MockTransport {
    handlers: HashMap<(String, String), Handler>,
    calls: Arc<Mutex<Vec<(String, String)>>>,
    sse_batches: Arc<Mutex<VecDeque<Vec<bytes::Bytes>>>>,
}

impl MockTransport {
    fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            calls: Arc::new(Mutex::new(Vec::new())),
            sse_batches: Arc::new(Mutex::new(VecDeque::new())),
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
        serde_json::json!({
            "status": "ok",
            "backend": "test",
            "worker_id": "worker-1",
            "now_utc": "2026-01-01T00:00:00Z",
        }),
    ));
    let client =
        medousa_sdk::MedousaClient::with_transport(transport.clone(), "http://127.0.0.1:8080");
    let health = client.health().get().await.expect("health get");
    assert_eq!(health.status, "ok");
    assert_eq!(transport.call_count(), 1);
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
