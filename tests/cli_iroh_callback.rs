#![cfg(feature = "iroh-transport")]
use reqwest::header::{AUTHORIZATION, HeaderMap};
use serde_json::json;
use std::time::Duration;
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
    let status = tokio::process::Command::new("cargo")
        .args([
            "run",
            "--locked",
            "-p",
            "medousa-cli",
            "--features",
            "iroh-transport",
            "--",
            "daemon-external-event",
            "00000000-0000-0000-0000-000000000001",
            "event-1",
            "completed",
            "Full reply over Iroh 🌊",
            "--request-id",
            "request-1",
            "--iroh-ticket",
            &gateway.info().ticket,
            "--daemon-url",
            &format!("http://{}", lan.local_addr().unwrap()),
        ])
        .env("MEDOUSA_BRIDGE_KEY", "synthetic-key")
        .env("MEDOUSA_BRIDGE_BEARER", "synthetic-bearer")
        .status()
        .await
        .unwrap();
    assert!(status.success());
    let body = json!({"event_id":"event-1", "request_id":"request-1", "kind":"completed", "text":"Full reply over Iroh 🌊"});
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
