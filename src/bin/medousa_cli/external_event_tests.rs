use super::*;
use clap::Parser;
use serde_json::json;

fn worker(id: &str, label: &str, ticket: Option<&str>) -> WorkerCredentials {
    serde_json::from_value(json!({
        "id": id, "label": label, "workshopDeviceId": "synthetic-workshop",
        "daemonUrl": "http://192.0.2.1:7419", "pairingId": "synthetic-pairing",
        "daemonPublicKey": "synthetic-public-key", "irohTicket": ticket,
        "connectedAt": "2026-01-01T00:00:00Z", "sessionToken": "synthetic-bearer"
    }))
    .unwrap()
}

#[test]
fn raw_ticket_precedes_http_and_empty_ticket_never_downgrades() {
    let destination = explicit_destination(Some("ticket".into()), "http://192.0.2.1:7419").unwrap();
    assert!(matches!(destination, Destination::Iroh(ticket) if ticket == "ticket"));
    assert!(explicit_destination(Some(" ".into()), "http://192.0.2.1:7419").is_err());
    assert!(
        matches!(explicit_destination(None, "http://localhost:7419/").unwrap(), Destination::Http(base) if base == "http://localhost:7419")
    );
}

#[tokio::test]
async fn pair_join_record_supplies_matching_ticket_and_bearer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workers.json");
    tokio::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version": 1, "connections": [{
                "id": "worker-1", "label": "Mac workshop", "workshopDeviceId": "workshop",
                "daemonUrl": "http://192.0.2.1:7419", "pairingId": "pairing",
                "daemonPublicKey": "public-key", "irohTicket": "saved-ticket",
                "connectedAt": "2026-01-01T00:00:00Z", "sessionToken": "saved-bearer"
            }]
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    let (destination, bearer) = worker_destination(&path, "Mac workshop").await.unwrap();
    assert!(matches!(destination, Destination::Iroh(ticket) if ticket == "saved-ticket"));
    assert_eq!(bearer, "saved-bearer");
}

#[test]
fn saved_worker_requires_a_unique_match_and_an_iroh_ticket() {
    assert!(select_worker(vec![worker("a", "Mac", None)], "a").is_err());
    assert!(select_worker(vec![worker("a", "Mac", Some("ticket"))], "missing").is_err());
    assert!(
        select_worker(
            vec![worker("a", "Mac", Some("a")), worker("b", "Mac", Some("b"))],
            "mac"
        )
        .is_err()
    );
    let (destination, _) = select_worker(
        vec![worker("a", "Mac", Some("a")), worker("b", "a", Some("b"))],
        "a",
    )
    .unwrap();
    assert!(matches!(destination, Destination::Iroh(ticket) if ticket == "a"));
}

#[test]
fn credential_headers_are_sensitive_and_cannot_inject_iroh_http_headers() {
    let headers = callback_headers("synthetic-bearer", "synthetic-key").unwrap();
    assert!(headers[AUTHORIZATION].is_sensitive());
    assert!(headers["x-medousa-bridge-key"].is_sensitive());
    assert!(callback_headers("token\r\nInjected: yes", "key").is_err());
    assert!(callback_headers("token", "key\nInjected: yes").is_err());
}

#[test]
fn cli_accepts_raw_ticket_and_worker_but_rejects_conflicting_targets() {
    let base = [
        "medousa_cli",
        "daemon-external-event",
        "00000000-0000-0000-0000-000000000001",
        "event",
        "completed",
        "done",
        "--request-id",
        "request",
    ];
    for options in [
        vec!["--iroh-ticket", "ticket"],
        vec!["--worker", "worker-1"],
    ] {
        assert!(crate::cli::Cli::try_parse_from(base.iter().copied().chain(options)).is_ok());
    }
    for options in [
        vec!["--iroh-ticket", "ticket", "--worker", "worker-1"],
        vec![
            "--worker",
            "worker-1",
            "--daemon-url",
            "http://localhost:7419",
        ],
    ] {
        assert!(crate::cli::Cli::try_parse_from(base.iter().copied().chain(options)).is_err());
    }
}

#[tokio::test]
async fn an_iroh_failure_never_contacts_a_reachable_http_destination() {
    let lan = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", lan.local_addr().unwrap());
    let destination = explicit_destination(Some("invalid-ticket".into()), &base).unwrap();
    assert!(
        post_event(
            &destination,
            "/callback",
            callback_headers("bearer", "key").unwrap(),
            b"{}"
        )
        .await
        .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), lan.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn http_callbacks_preserve_auth_and_surface_status() {
    use axum::{Json, Router, routing::post};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().route(
        "/callback",
        post(
            |headers: HeaderMap, Json(body): Json<serde_json::Value>| async move {
                assert_eq!(headers[AUTHORIZATION], "Bearer synthetic-bearer");
                assert_eq!(headers["x-medousa-bridge-key"], "synthetic-key");
                assert_eq!(body["request_id"], "request-1");
                reqwest::StatusCode::UNAUTHORIZED
            },
        ),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let error = post_event(
        &Destination::Http(base),
        "/callback",
        callback_headers("synthetic-bearer", "synthetic-key").unwrap(),
        br#"{"request_id":"request-1"}"#,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("401"));
    server.abort();
}

/// Real QUIC request through the production Iroh gateway. Run explicitly:
/// cargo test --bin medousa_cli --features iroh-transport live_iroh_callback -- --ignored
#[cfg(feature = "iroh-transport")]
#[tokio::test]
#[ignore = "uses live Iroh relay discovery; run separately from hermetic tests"]
async fn live_iroh_callback_preserves_headers_and_body_without_lan() {
    use axum::{Json, Router, routing::post};
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_url = format!("http://{}", upstream.local_addr().unwrap());
    let (received, mut receive) = tokio::sync::mpsc::channel(1);
    let app = Router::new().route(
        "/v1/external-conversations/00000000-0000-0000-0000-000000000001/events",
        post(
            move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                let received = received.clone();
                async move {
                    received.send((headers, body)).await.unwrap();
                    Json(json!({"accepted":true}))
                }
            },
        ),
    );
    let server = tokio::spawn(async move {
        axum::serve(upstream, app).await.unwrap();
    });
    let gateway = tokio::time::timeout(
        Duration::from_secs(30),
        medousa::iroh_transport::spawn_workshop_gateway(&upstream_url),
    )
    .await
    .unwrap()
    .unwrap();
    let lan = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = explicit_destination(
        Some(gateway.info().ticket.clone()),
        &format!("http://{}", lan.local_addr().unwrap()),
    )
    .unwrap();
    let body = json!({"event_id":"event-1", "request_id":"request-1", "kind":"completed", "text":"Full reply over Iroh 🌊"});
    post_event(
        &destination,
        "/v1/external-conversations/00000000-0000-0000-0000-000000000001/events",
        callback_headers("synthetic-bearer", "synthetic-key").unwrap(),
        &serde_json::to_vec(&body).unwrap(),
    )
    .await
    .unwrap();
    let (headers, received_body) = tokio::time::timeout(Duration::from_secs(5), receive.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(headers[AUTHORIZATION], "Bearer synthetic-bearer");
    assert_eq!(headers["x-medousa-bridge-key"], "synthetic-key");
    assert_eq!(received_body, body);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), lan.accept())
            .await
            .is_err()
    );
    gateway.shutdown().await.unwrap();
    server.abort();
}
