//! Real HTTP contract fixtures. No user registry, credentials or daemon effects.
use super::*;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

fn health() -> Value {
    json!({
        "runtime": {
            "authority_id": format!("auth_{}", "a".repeat(64)),
            "product_version": "fixture", "build_revision": "fixture",
            "contract_revision": medousa_types::DAEMON_API_CONTRACT_REVISION,
            "base_schema_revision": 1, "deployment_profile": "personal",
            "deployment_target": "desktop", "advertised_capabilities": []
        },
        "status": "ok", "backend": "surrealkv", "worker_id": "fixture",
        "now_utc": "2026-11-24T00:00:00Z", "tool_registry_count": 12
    })
}

// Explicit request sequence catches /health accidentally being used for identity,
// anonymous /v1/health requests, writes, retries and fallback probes.
fn server(
    responses: Vec<(&'static str, u16, Value)>,
    authenticated: bool,
) -> (MedousaClient, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let task = thread::spawn(move || {
        for (path, status, body) in responses {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => panic!("missing expected health request"),
                }
            };
            // macOS inherits O_NONBLOCK from the listening socket. The request
            // can arrive after accept, especially under parallel native tests.
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(8)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buf = [0; 1024];
            while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buf[..n]);
            }
            let request = String::from_utf8(raw).unwrap();
            assert_eq!(
                request.lines().next().unwrap(),
                format!("GET {path} HTTP/1.1")
            );
            assert_eq!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer fixture-only"),
                authenticated
            );
            let body = body.to_string();
            write!(socket, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let mut headers = reqwest::header::HeaderMap::new();
    if authenticated {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_static("Bearer fixture-only"),
        );
    }
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    (
        MedousaClient::with_transport(
            Arc::new(medousa_sdk::HttpTransport::with_client(http)),
            origin,
        ),
        task,
    )
}

#[tokio::test]
async fn public_liveness_has_no_identity_but_authenticated_runtime_connects_and_rechecks() {
    let (client, task) = server(
        vec![
            ("/health", 200, json!({"status":"ok", "apiVersion":"v1"})),
            ("/v1/health", 200, health()),
            ("/v1/health", 200, health()),
        ],
        true,
    );
    let public = client
        .transport()
        .get_json(client.base_url(), "/health")
        .await
        .unwrap();
    assert!(public.get("runtime").is_none());
    let owner = authority(&client).await.unwrap();
    assert_eq!(owner, format!("auth_{}", "a".repeat(64)));
    assert_eq!(authority(&client).await.unwrap(), owner); // existing-source health check
    task.join().unwrap();
}

#[tokio::test]
async fn protected_health_requires_authentication_and_never_falls_back_to_liveness() {
    for status in [401, 403, 404, 503] {
        let (client, task) = server(
            vec![("/v1/health", status, json!({"error":"fixture"}))],
            false,
        );
        assert!(authority(&client).await.is_err());
        task.join().unwrap();
    }
}

#[tokio::test]
async fn runtime_identity_and_contract_fail_closed() {
    let valid = health();
    let mut invalid_id = valid.clone();
    invalid_id["runtime"]["authority_id"] = json!("not-an-authority");
    let mut missing_id = valid.clone();
    missing_id["runtime"]
        .as_object_mut()
        .unwrap()
        .remove("authority_id");
    let mut revision = valid.clone();
    revision["runtime"]["contract_revision"] = json!(999);
    let mut schema = valid.clone();
    schema["runtime"]["base_schema_revision"] = json!(0);
    for body in [
        json!({"status":"ok", "apiVersion":"v1"}),
        invalid_id,
        missing_id,
        revision,
        schema,
    ] {
        let (client, task) = server(vec![("/v1/health", 200, body)], true);
        assert!(authority(&client).await.is_err());
        task.join().unwrap();
    }
}

#[tokio::test]
async fn changed_runtime_identity_is_returned_not_replaced_with_a_saved_or_local_owner() {
    let mut changed = health();
    changed["runtime"]["authority_id"] = json!(format!("auth_{}", "b".repeat(64)));
    let (client, task) = server(
        vec![("/v1/health", 200, health()), ("/v1/health", 200, changed)],
        true,
    );
    let expected = authority(&client).await.unwrap();
    let observed = authority(&client).await.unwrap();
    assert_ne!(expected, observed); // connect's expected_owner fence must see this change
    task.join().unwrap();
}
