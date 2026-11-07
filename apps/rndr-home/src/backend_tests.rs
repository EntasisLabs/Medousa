//! Test-only loopback HTTP fixtures. Never contact a daemon/provider or keychain.
use super::*;
use rndr_core::{App, FeedConfig};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

struct Wire {
    method: &'static str,
    path: String,
    status: u16,
    content_type: &'static str,
    body: String,
    check: Option<fn(&str)>,
}
fn json_wire(method: &'static str, path: &str, body: Value) -> Wire {
    Wire {
        method,
        path: path.into(),
        status: 200,
        content_type: "application/json",
        body: body.to_string(),
        check: None,
    }
}
fn fixture(wires: Vec<Wire>) -> (Arc<MedousaClient>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        for expected in wires {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!(
                        "missing fixture request {} {}: {e}",
                        expected.method, expected.path
                    ),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buffer = [0; 4096];
            let header_end = loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buffer[..n]);
                if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let header = String::from_utf8(raw[..header_end].to_vec()).unwrap();
            let length = header
                .lines()
                .find_map(|s| {
                    s.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|s| s.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while raw.len() < header_end + length {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buffer[..n]);
            }
            assert!(
                header.starts_with(&format!("{} {} HTTP/1.1", expected.method, expected.path)),
                "{header}"
            );
            if let Some(check) = expected.check {
                check(std::str::from_utf8(&raw).unwrap());
            }
            write!(socket, "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", expected.status, expected.content_type, expected.body.len(), expected.body).unwrap();
        }
    });
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    (
        Arc::new(MedousaClient::with_transport(
            Arc::new(HttpTransport::with_client(http)),
            origin,
        )),
        handle,
    )
}
fn sse(seq: u64, event: Value) -> String {
    format!(
        "data: {}\n\n",
        json!({"schema_version":3,"turn_id":"t","seq":seq,"emitted_at_utc":"2026-11-07T00:00:00Z","event":event})
    )
}
fn ticket() -> Value {
    json!({"turn_id":"t","accepted_at_utc":"2026-11-07T00:00:00Z","stream_url":"/v1/interactive/turn/t/stream","stream_ready":true,"fallback_to_local":false})
}
#[tokio::test]
async fn sdk_create_history_admit_stream_reconnect_and_cancel() {
    let active = json!({"active":false});
    let history_body = json!({"authority_id":format!("auth_{}", "a".repeat(64)),"session_id":"s","turns":[{"entry_id":format!("ent_{}", "b".repeat(32)),"entry_seq":1,"content_digest":"sha256:fixture","role":"user","content":"Existing message","timestamp":"2026-11-07T00:00:00Z","tool_names":[]}]});
    let mut admission = json_wire("POST", "/v1/interactive/turn", ticket());
    admission.check = Some(|raw| {
        let body: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["session_id"], "s");
        assert_eq!(body["persist_user_turn"], true);
        assert_eq!(body["code_project_setup_authorized"], false);
        assert_eq!(body["provider"], "mock");
    });
    let mut first = Wire {
        method: "GET",
        path: "/v1/interactive/turn/t/stream".into(),
        status: 200,
        content_type: "text/event-stream",
        body: sse(
            1,
            json!({"type":"content_append","segment_id":"a","text":"Hello"}),
        ),
        check: None,
    };
    first.check = Some(|raw| {
        assert!(
            raw.to_ascii_lowercase()
                .contains("accept: text/event-stream; medousa-version=3")
        )
    });
    let second = Wire {
        method: "GET",
        path: "/v1/interactive/turn/t/stream?since=1".into(),
        status: 200,
        content_type: "text/event-stream",
        body: sse(
            1,
            json!({"type":"content_append","segment_id":"a","text":"duplicate"}),
        ) + &sse(
            2,
            json!({"type":"content_append","segment_id":"a","text":" world"}),
        ) + &sse(
            3,
            json!({"type":"turn_completed","outcome":"completed","aggregate_text":"Hello world"}),
        ),
        check: None,
    };
    let (client, server) = fixture(vec![
        json_wire(
            "POST",
            "/v1/sessions",
            json!({"authority_id":format!("auth_{}", "a".repeat(64)),"session_id":"s","catalog":"single"}),
        ),
        json_wire("GET", "/v1/sessions/s/history", history_body),
        json_wire("GET", "/v1/sessions/s/active-turn", active),
        admission,
        first,
        second,
        json_wire(
            "POST",
            "/v1/sessions/s/active-turn",
            json!({"cancelled":true,"turn_id":"t","message":"cancel requested"}),
        ),
    ]);
    let session = client
        .sessions()
        .create(&CreateSessionRequest {
            session_id: None,
            catalog: Some("single".into()),
            member_profile_ids: None,
            agent_profile_id: None,
            display_name: Some("Today".into()),
        })
        .await
        .unwrap();
    let loaded = history(&client, &session.session_id).await.unwrap();
    let Update::History(id, transcript, None) = loaded else {
        panic!("unexpected history")
    };
    assert!(transcript.contains("Existing message"));
    let mut chat = medousa_rndr_home::Chat::history(id, transcript, false);
    chat.begin("hello").unwrap();
    let ticket = client
        .interactive()
        .start_turn(&medousa_rndr_home::turn_request(
            "s",
            "hello",
            "mock",
            "test-only",
        ))
        .await
        .unwrap();
    chat.admitted(ticket.turn_id);
    let mut app = App::new();
    let (_feed, sender) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    follow(client.clone(), ticket.stream_url, sender, 5).await;
    app.update();
    let updates = rndr_core::events::<Reply>(app.world()).to_vec();
    assert_eq!(updates.len(), 3);
    for reply in updates {
        assert_eq!(reply.generation, 5);
        let Update::Stream(e) = reply.update else {
            panic!("unexpected error")
        };
        chat.apply(e);
    }
    assert!(chat.transcript.ends_with("Hello world"));
    assert!(!chat.busy);
    assert!(
        client
            .sessions()
            .cancel_active_turn("s")
            .await
            .unwrap()
            .cancelled
    );
    server.join().unwrap();
}
#[tokio::test]
async fn unauthorized_admission_is_not_retried() {
    let mut denied = json_wire(
        "POST",
        "/v1/interactive/turn",
        json!({"error":"unauthorized"}),
    );
    denied.status = 401;
    let (client, server) = fixture(vec![denied]);
    assert!(
        client
            .interactive()
            .start_turn(&medousa_rndr_home::turn_request(
                "s",
                "hi",
                "mock",
                "test-only"
            ))
            .await
            .is_err()
    );
    server.join().unwrap();
}
