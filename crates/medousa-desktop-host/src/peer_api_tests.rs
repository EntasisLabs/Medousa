//! Boundary tests: all network operations terminate at the selected daemon,
//! with its existing authentication. No peer credentials or legacy registry.
use super::*;
use crate::peers::PeerService;
use std::io::{Read, Write};

fn mock(
    replies: Vec<(&'static str, Value)>,
) -> (Arc<MedousaClient>, std::thread::JoinHandle<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        replies.into_iter().map(|(route, body)| {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut raw = Vec::new();
        loop {
            let mut b = [0;4096];
            let n = stream.read(&mut b).unwrap();
            assert_ne!(n, 0, "incomplete request");
            raw.extend_from_slice(&b[..n]);
            if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
                let len = headers.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|s| s.trim().parse::<usize>().ok()).unwrap_or(0);
                if raw.len() >= end+4+len { break; }
            }
        }
        let request = String::from_utf8(raw).unwrap();
        assert!(request.starts_with(route), "{request}");
        assert!(request.to_ascii_lowercase().contains("authorization: bearer selected-daemon"));
        let body = body.to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        request
    }).collect()
    });
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        "Bearer selected-daemon".parse().unwrap(),
    );
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    (
        Arc::new(MedousaClient::with_transport(
            Arc::new(medousa_sdk::HttpTransport::with_client(http)),
            base,
        )),
        server,
    )
}

#[tokio::test]
async fn pairing_is_a_command_to_daemon_not_a_client_ceremony() {
    let (client, server) = mock(vec![(
        "POST /v1/peers/pair ",
        json!({"workshopId":"peer-one"}),
    )]);
    let service = PeerManagementService::new(client);
    service
        .execute(&Request::Pair {
            invite: "medousa://invite".into(),
            url: "http://192.0.2.1:7419".into(),
            name: "Destination".into(),
        })
        .await
        .unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("192.0.2.1"));
    assert!(!requests[0].contains("phonePublicKey"));
}

#[tokio::test]
async fn mutations_forward_exact_identity_and_explicit_policy_clears() {
    let (client, server) = mock(vec![
        ("POST /v1/peers/forget ", json!({})),
        ("POST /v1/peers/policy ", json!({})),
        ("POST /v1/peers/share ", json!({})),
    ]);
    let service = PeerManagementService::new(client);
    service
        .execute(&Request::Forget {
            id: "peer-one".into(),
            device: "d".into(),
            pairing: "p".into(),
        })
        .await
        .unwrap();
    service
        .execute(&Request::Policy {
            device: "d".into(),
            pairing: "p".into(),
            expires: None,
            idle: None,
        })
        .await
        .unwrap();
    service
        .execute(&Request::Share {
            id: "peer-one".into(),
            device: "d".into(),
            pairing: "p".into(),
            item: "note.md".into(),
            artifact: false,
            mode: "ask".into(),
        })
        .await
        .unwrap();
    let requests = server.join().unwrap();
    assert!(requests[0].contains("\"pairing\":\"p\""));
    assert!(requests[1].contains("\"expires\":null"));
    assert!(requests[1].contains("\"idle\":null"));
    assert!(requests[2].contains("\"mode\":\"ask\""));
}

#[tokio::test]
async fn snapshot_send_read_and_preview_use_only_daemon() {
    let (client, server) = mock(vec![
        (
            "GET /v1/peers/snapshot ",
            json!({"peers":[],"messages":[],"errors":["peer offline"]}),
        ),
        ("POST /v1/peers/send ", json!({"id":"sent"})),
        ("POST /v1/peers/read ", json!({})),
        ("POST /v1/peers/attachment ", json!({})),
    ]);
    let service = PeerService::new(client);
    assert_eq!(
        service.snapshot().await.unwrap().errors,
        vec!["peer offline"]
    );
    service.send("peer-one", "hello").await.unwrap();
    let message = json!({"id":"a/b","sinkKind":"peer","workshopId":"peer-one","attachmentResult":{"vaultPaths":["forged.md"]}});
    service.mark_read(&message).await.unwrap();
    service
        .open_attachment(&message, "actual.md", false)
        .await
        .unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 4);
    assert!(requests[3].contains("actual.md"));
    assert!(!requests[3].contains("forged.md"));
}

#[tokio::test]
async fn malformed_snapshot_is_not_an_empty_success() {
    let (client, server) = mock(vec![("GET /v1/peers/snapshot ", json!({"peers":[]}))]);
    assert!(PeerService::new(client).snapshot().await.is_err());
    server.join().unwrap();
}

#[tokio::test]
async fn legacy_daemon_failure_is_not_replaced_with_client_networking() {
    let (client, server) = mock(vec![("GET /v1/peers/connections ", json!({"peers":[]}))]);
    PeerManagementService::new(client)
        .execute(&Request::Status)
        .await
        .unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
}
