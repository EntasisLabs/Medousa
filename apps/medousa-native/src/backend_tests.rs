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
    fixture_gated(wires, None)
}

fn fixture_gated(
    wires: Vec<Wire>,
    gate: Option<(
        &'static str,
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    )>,
) -> (Arc<MedousaClient>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut pending = wires;
        let mut gate = gate;
        let mut blocked_response = None;
        while !pending.is_empty() {
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
                        pending[0].method, pending[0].path
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
            // Independent catalogs may arrive in any order. Match wire routes,
            // preserving same-route order (e.g. active/history reconciliation).
            let index = pending
                .iter()
                .position(|expected| {
                    header.starts_with(&format!("{} {} HTTP/1.1", expected.method, expected.path))
                })
                .unwrap_or_else(|| panic!("unexpected request: {header}"));
            let expected = pending.remove(index);
            if let Some(check) = expected.check {
                check(std::str::from_utf8(&raw).unwrap());
            }
            let release = if gate
                .as_ref()
                .is_some_and(|(path, _, _)| *path == expected.path)
            {
                gate.take().map(|(_, release, started)| {
                    started.send(()).unwrap();
                    release
                })
            } else {
                None
            };
            let gated = release.is_some();
            let respond = move || {
                if let Some(release) = release {
                    release
                        .recv_timeout(Duration::from_secs(5))
                        .expect("test must release gated response");
                }
                let result = write!(
                    socket,
                    "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    expected.status,
                    expected.content_type,
                    expected.body.len(),
                    expected.body
                );
                // Gated read-only work may be intentionally cancelled by newer
                // navigation or a mode mutation before its response is released.
                if !gated {
                    result.unwrap();
                }
            };
            // Keep accepting requests while a deliberately blocked endpoint waits.
            if gated {
                blocked_response = Some(thread::spawn(respond));
            } else {
                respond();
            }
        }
        if let Some(response) = blocked_response {
            response.join().unwrap();
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
fn mode_body(effective: &str) -> Value {
    json!({"session_id":"s", "selected_mode":"general", "effective_mode":effective,
        "effective_source":"task", "revision":7})
}
fn mode_wire(effective: &str) -> Wire {
    json_wire("GET", "/v1/sessions/s/agent-mode", mode_body(effective))
}
fn registry_wire() -> Wire {
    json_wire(
        "GET",
        "/v1/agent-modes",
        json!({"modes":[
            {"mode":"general","label":"General","available":true},
            {"mode":"coder","label":"Coder","available":false,"unavailable_reason":"Fixture"}
        ]}),
    )
}
fn sse(seq: u64, event: Value) -> String {
    format!(
        "data: {}\n\n",
        json!({"schema_version":3,"turn_id":"t","seq":seq,"emitted_at_utc":"2026-11-07T00:00:00Z","event":event})
    )
}
fn active_body(active: bool) -> Value {
    if active {
        json!({"active":true,"turn":{"turn_id":"t","session_id":"s","stream_url":"/v1/interactive/turn/t/stream","phase":"running","composer_handoff":false,"started_at":"2026-11-07T00:00:00Z"}})
    } else {
        json!({"active":false})
    }
}
fn history_body(assistant: bool) -> Value {
    let authority = format!("auth_{}", "a".repeat(64));
    let cause = json!({"authority_id":authority,"session_id":"s","execution_id":"t"});
    let mut entries = vec![
        json!({"entry_id":format!("ent_{}", "b".repeat(32)),"entry_seq":1,"caused_by":cause,"content_digest":"sha256:user","role":"user","content":"unique user prose","timestamp":"2026-11-07T00:00:00Z","tool_names":[]}),
    ];
    if assistant {
        entries.push(json!({"entry_id":format!("ent_{}", "c".repeat(32)),"entry_seq":2,"caused_by":cause,"content_digest":"sha256:assistant","role":"assistant","content":"unique assistant prose","timestamp":"2026-11-07T00:00:01Z","tool_names":[]}));
    }
    json!({"authority_id":authority,"session_id":"s","turns":entries})
}
fn active_wire(active: bool) -> Wire {
    json_wire("GET", "/v1/sessions/s/active-turn", active_body(active))
}
fn history_wire(assistant: bool) -> Wire {
    json_wire("GET", "/v1/sessions/s/history", history_body(assistant))
}
#[tokio::test]
async fn completion_between_history_and_active_reads_retries_fresh_history() {
    let (client, server) = fixture(vec![
        active_wire(true),
        history_wire(false),
        active_wire(false),
        active_wire(false),
        history_wire(true),
        active_wire(false),
    ]);
    let Update::History(_, _, messages, None) = history(&client, "s").await.unwrap() else {
        panic!("stale history")
    };
    assert_eq!(
        pairs(&messages),
        vec![
            ("user".into(), "unique user prose".into()),
            ("assistant".into(), "unique assistant prose".into()),
        ]
    );
    server.join().unwrap();
}
#[tokio::test]
async fn continuous_activity_is_bounded_and_remains_uncertain() {
    let mut wires = vec![];
    for _ in 0..3 {
        wires.extend([active_wire(true), history_wire(false), active_wire(false)]);
    }
    let (client, server) = fixture(wires);
    assert!(matches!(
        history(&client, "s").await.unwrap(),
        Update::Failure(_)
    ));
    server.join().unwrap();
}
#[tokio::test]
async fn terminal_ticket_cleanup_retries_are_bounded_and_handoff_is_honest() {
    let (client, server) = fixture(vec![
        active_wire(true),
        history_wire(true),
        active_wire(true),
        active_wire(false),
        history_wire(true),
        active_wire(false),
    ]);
    assert!(matches!(
        settled_history(&client, "s").await,
        Update::SettledHistory(_, _, _)
    ));
    server.join().unwrap();
    let mut wires = vec![];
    for _ in 0..3 {
        wires.extend([active_wire(true), history_wire(true), active_wire(true)]);
    }
    let (client, server) = fixture(wires);
    assert!(
        matches!(settled_history(&client, "s").await, Update::Failure(message) if message.contains("possibly a handoff"))
    );
    server.join().unwrap();
}
#[tokio::test]
async fn catalog_uses_real_snake_case_record_and_is_read_only() {
    let (client, server) = fixture(vec![
        list_wire(),
        json_wire(
            "GET",
            "/v1/models/catalog",
            json!({"freshness":{},"models":[{"provider":"mock","model_id":"test-only","display_name":"Fixture","input_modalities":["text"],"output_modalities":["text"]}]}),
        ),
        registry_wire(),
    ]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    drop(commands);
    serve(client, rx, tx).await;
    app.update();
    assert!(matches!(&observed(&app)[1].update,
        Update::Models(models) if models == &vec![("mock".into(), "test-only".into())]));
    server.join().unwrap();
}
#[tokio::test]
async fn session_mode_mutation_uses_session_scope_and_server_effective_value() {
    let mut change = json_wire("PUT", "/v1/sessions/s/agent-mode", mode_body("teacher"));
    change.check = Some(|raw| {
        let body: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body, json!({"mode":"coder", "scope":"session"}));
    });
    let (client, server) = fixture(vec![
        list_wire(),
        json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
        registry_wire(),
        change,
    ]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    commands
        .send(Request {
            generation: 9,
            command: Command::SetMode("s".into(), AgentModeId::Coder),
        })
        .unwrap();
    drop(commands);
    serve(client, rx, tx).await;
    app.update();
    let replies = observed(&app);
    assert!(matches!(&replies[2].update, Update::Modes(modes)
        if modes == &vec![("general".into(), "General".into(), true), ("coder".into(), "Coder".into(), false)]));
    assert_eq!(replies[3].generation, 9);
    assert!(matches!(&replies[3].update, Update::Mode(value) if value == "teacher"));
    server.join().unwrap();
}

#[tokio::test]
async fn mode_errors_are_redacted_nonfatal_and_mutation_is_not_retried() {
    let mut get = json_wire(
        "GET",
        "/v1/sessions/s/agent-mode",
        json!({"error":"private diagnostic"}),
    );
    get.status = 401;
    let mut put = json_wire(
        "PUT",
        "/v1/sessions/s/agent-mode",
        json!({"error":"private diagnostic"}),
    );
    put.status = 409;
    let (client, server) = fixture(vec![get, put]);
    assert!(
        matches!(mode(&client, "s").await, Update::Notice(value) if !value.contains("private diagnostic"))
    );
    assert!(
        matches!(set_mode(&client, "s", AgentModeId::Coder).await, Update::Notice(value)
        if value.contains("outcome may be unknown") && !value.contains("private diagnostic"))
    );
    server.join().unwrap();
}

#[test]
fn connected_client_missing_auth_is_load_only_and_sanitized() {
    let temp = tempfile::tempdir().unwrap();
    let error = client("http://127.0.0.1:7419", temp.path())
        .err()
        .expect("missing auth must fail");
    assert!(error.contains("Local credential unavailable"));
    assert_eq!(
        std::fs::read_dir(temp.path()).unwrap().count(),
        0,
        "client must not provision credentials"
    );
    let error = client("http://user:fixture-secret@remote.invalid", temp.path())
        .err()
        .unwrap();
    assert!(!error.contains("fixture-secret"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

fn pairs(messages: &[Value]) -> Vec<(String, String)> {
    messages
        .iter()
        .map(|entry| {
            (
                entry["role"].as_str().unwrap().into(),
                entry["content"].as_str().unwrap().into(),
            )
        })
        .collect()
}

#[test]
fn streams_cannot_escape_local_api_origin() {
    assert!(stream_path("/v1/interactive/turn/t/stream", "http://127.0.0.1:9775").is_ok());
    for path in [
        "https://remote/stream",
        "//remote/stream",
        "/v1/../secret",
        "/v1/%2e%2e/secret",
        "/v1/stream?redirect=http://remote",
        "/v1/stream\\remote",
    ] {
        assert!(
            stream_path(path, "http://127.0.0.1:9775").is_err(),
            "{path}"
        );
    }
}

fn list_wire() -> Wire {
    json_wire("GET", "/v1/sessions?limit=100", json!({"sessions":[]}))
}

#[tokio::test]
async fn catalog_search_and_cursor_use_encoded_query_and_preserve_request_identity() {
    let (client, server) = fixture(vec![json_wire(
        "GET",
        "/v1/sessions?limit=100&q=hello%20world&cursor=opaque%2B%2F%3D",
        json!({"sessions":[
            {"session_id":"s", "display_name":"Named chat", "preview":"prose", "turns":1,"verification_runs":0}
        ], "next_cursor":"next"}),
    )]);
    assert!(
        matches!(sessions(&client, " hello world ", Some("opaque+/=")).await.unwrap(),
        Update::SessionsPage { query, rows, cursor, append: true }
        if query == "hello world" && rows == vec![("s".into(), "Named chat".into())] && cursor.as_deref() == Some("next"))
    );
    server.join().unwrap();
}

#[tokio::test]
async fn fatal_stream_emits_failure_without_replacing_live_facts() {
    let (client, server) = fixture(vec![Wire {
        method: "GET",
        path: "/v1/interactive/turn/t/stream".into(),
        status: 200,
        content_type: "text/event-stream",
        check: None,
        body: sse(
            1,
            json!({"type":"content_append", "segment_id":"a", "text":"unsaved"}),
        ) + &sse(
            2,
            json!({"type":"turn_completed", "outcome":"fatal", "aggregate_text":"unsaved"}),
        ),
    }]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    follow(
        client,
        "/v1/interactive/turn/t/stream".into(),
        tx,
        7,
        "s".into(),
    )
    .await;
    app.update();
    let replies = observed(&app);
    assert_eq!(replies.len(), 3);
    assert!(replies.iter().all(|reply| reply.generation == 7));
    assert!(matches!(replies[0].update, Update::Stream(_)));
    assert!(matches!(replies[1].update, Update::Stream(_)));
    assert!(matches!(replies[2].update, Update::Failure(_)));
    server.join().unwrap();
}

#[tokio::test]
async fn terminal_stream_reconciles_canonical_history_and_mode() {
    let (client, server) = fixture(vec![
        Wire {
            method: "GET",
            path: "/v1/interactive/turn/t/stream".into(),
            status: 200,
            content_type: "text/event-stream",
            check: None,
            body: sse(
                1,
                json!({"type":"content_append", "segment_id":"a", "text":"unique assistant prose"}),
            ) + &sse(
                2,
                json!({"type":"turn_completed", "outcome":"completed", "aggregate_text":"unique assistant prose"}),
            ),
        },
        active_wire(false),
        history_wire(true),
        active_wire(false),
        mode_wire("general"),
    ]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    follow(
        client,
        "/v1/interactive/turn/t/stream".into(),
        tx,
        8,
        "s".into(),
    )
    .await;
    app.update();
    let replies = observed(&app);
    assert_eq!(replies.len(), 4);
    assert!(replies.iter().all(|reply| reply.generation == 8));
    assert!(
        matches!(&replies[2].update, Update::SettledHistory(id, _, messages)
        if id == "s" && pairs(messages).len() == 2)
    );
    assert!(matches!(&replies[3].update, Update::Mode(mode) if mode == "general"));
    server.join().unwrap();
}

#[tokio::test]
async fn stale_navigation_and_mutations_are_not_dispatched() {
    let (client, server) = fixture(vec![
        list_wire(),
        json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
        registry_wire(),
        active_wire(false),
        history_wire(false),
        active_wire(false),
        mode_wire("general"),
    ]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    commands
        .send(Request {
            generation: 2,
            command: Command::Open("s".into()),
        })
        .unwrap();
    for command in [
        Command::Open("old".into()),
        Command::Cancel("old".into()),
        Command::SetMode("old".into(), AgentModeId::Coder),
    ] {
        commands
            .send(Request {
                generation: 1,
                command,
            })
            .unwrap();
    }
    drop(commands);
    serve(client, rx, tx).await;
    app.update();
    let replies = observed(&app);
    assert_eq!(replies.len(), 5);
    assert!(replies[3..].iter().all(|reply| reply.generation == 2));
    server.join().unwrap();
}

#[tokio::test]
async fn list_uses_canonical_titles_and_preview_fallback() {
    let (client, server) = fixture(vec![json_wire(
        "GET",
        "/v1/sessions?limit=100",
        json!({"sessions":[
            {"session_id":"s", "display_name":"Title", "preview":"prose", "turns":1, "verification_runs":0},
            {"session_id":"t", "preview":"Fallback", "turns":1, "verification_runs":0},
            {"session_id":"u", "display_name":"   ", "preview":"", "turns":0, "verification_runs":0}
        ]}),
    )]);
    assert!(
        matches!(sessions(&client, "  ", None).await.unwrap(), Update::SessionsPage {rows, append: false, query, cursor: None}
        if query.is_empty() && rows == vec![("s".into(), "Title".into()), ("t".into(), "Fallback".into()), ("u".into(), "u".into())])
    );
    server.join().unwrap();
}

#[tokio::test]
async fn startup_lists_generation_zero_even_when_model_catalog_fails() {
    let mut broken = json_wire("GET", "/v1/models/catalog", json!({"error":"private"}));
    broken.status = 503;
    let (client, server) = fixture(vec![list_wire(), broken, registry_wire()]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    drop(commands);
    serve(client, rx, tx).await;
    app.update();
    let replies = observed(&app);
    assert_eq!(replies[0].generation, 0);
    assert!(
        matches!(&replies[0].update, Update::SessionsPage { query, append: false, .. } if query.is_empty())
    );
    assert!(
        matches!(&replies[1].update, Update::Notice(s) if !s.contains("existing app") && !s.contains("private"))
    );
    server.join().unwrap();
}

#[tokio::test]
async fn rename_delete_and_search_do_not_replace_view_generation_or_replay_mutations() {
    let mut rename = json_wire(
        "PUT",
        "/v1/sessions/s/name",
        json!({"session_id":"s","display_name":"Canonical title"}),
    );
    rename.check = Some(|raw| {
        let value: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(value, json!({"display_name":"Requested title"}));
    });
    let mut delete = json_wire(
        "DELETE",
        "/v1/sessions/s?purge_memory=false",
        json!({"error":"private diagnostic"}),
    );
    delete.status = 503;
    let (client, server) = fixture(vec![
        list_wire(),
        json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
        registry_wire(),
        rename,
        json_wire(
            "GET",
            "/v1/sessions?limit=100&q=other",
            json!({"sessions":[]}),
        ),
        delete,
    ]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    for (generation, command) in [
        (
            2,
            Command::Rename {
                session: "s".into(),
                title: "Requested title".into(),
            },
        ),
        (99, Command::List("other".into())),
        (2, Command::Delete("s".into())),
        (
            1,
            Command::Rename {
                session: "stale".into(),
                title: "must not send".into(),
            },
        ),
    ] {
        commands
            .send(Request {
                generation,
                command,
            })
            .unwrap();
    }
    drop(commands);
    serve(client, rx, tx).await;
    app.update();
    let replies = observed(&app);
    assert!(
        matches!(&replies[3].update, Update::Renamed {session,title} if session == "s" && title == "Canonical title")
    );
    assert_eq!(replies[4].generation, 99);
    assert!(
        matches!(&replies[5].update, Update::Notice(message) if message.contains("outcome may be unknown") && !message.contains("private"))
    );
    assert_eq!(replies.len(), 6);
    server.join().unwrap();
}

#[tokio::test]
async fn delete_success_is_only_reported_for_complete_deletion() {
    let (client, server) = fixture(vec![
        list_wire(),
        json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
        registry_wire(),
        json_wire(
            "DELETE",
            "/v1/sessions/s?purge_memory=false",
            json!({
                "session_id":"s", "status":"complete", "deleted":true, "locus_purged":false,
                "locus_nodes_deleted":0,"cancelled_active_turn":false
            }),
        ),
        json_wire(
            "DELETE",
            "/v1/sessions/t?purge_memory=false",
            json!({
                "session_id":"t", "status":"retryable_partial", "deleted":false, "locus_purged":false,
                "locus_nodes_deleted":0,"cancelled_active_turn":false
            }),
        ),
    ]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    for id in ["s", "t"] {
        commands
            .send(Request {
                generation: 2,
                command: Command::Delete(id.into()),
            })
            .unwrap();
    }
    drop(commands);
    serve(client, rx, tx).await;
    app.update();
    let replies = observed(&app);
    assert!(matches!(&replies[3].update, Update::Deleted(id) if id == "s"));
    assert!(matches!(&replies[4].update, Update::Notice(s) if s.contains("not complete")));
    server.join().unwrap();
}

#[tokio::test]
async fn fork_freezes_committed_range_and_unknown_derivation_is_not_replayed() {
    let mut derive = json_wire("POST", "/v1/sessions/derive", json!({"error":"private"}));
    derive.status = 503;
    derive.check = Some(|raw| {
        assert!(
            raw.to_ascii_lowercase()
                .contains("idempotency-key: native-fork-")
        );
        let v: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(v["intent"], "fork");
        assert_eq!(v["sources"][0]["session"]["session_id"], "s");
        assert_eq!(v["sources"][0]["through_entry_seq"], 2);
        assert_eq!(v["target"]["catalog"], "single");
    });
    let (client, server) = fixture(vec![history_wire(true), derive]);
    assert!(fork(&client, "s").await.is_err());
    server.join().unwrap();
}

#[tokio::test]
async fn unknown_admission_never_emits_admitted_or_replays_prompt_and_can_reload() {
    let mut admission = json_wire(
        "POST",
        "/v1/interactive/turn",
        json!({"error":"private diagnostic"}),
    );
    admission.status = 503;
    let (client, server) = fixture(vec![
        list_wire(),
        json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
        registry_wire(),
        admission,
        active_wire(false),
        history_wire(false),
        active_wire(false),
        mode_wire("general"),
    ]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    let request: InteractiveTurnRequest = serde_json::from_value(json!({
        "session_id":"s","prompt":"retain this draft","persist_user_turn":true,
        "response_depth_mode":"normal","provider":"fixture","model":"fixture","stage_routing":medousa_types::StageRoutingMatrix::default_for("fixture", "fixture")
    }))
    .unwrap();
    commands
        .send(Request {
            generation: 1,
            command: Command::Send(Box::new(request)),
        })
        .unwrap();
    let task = tokio::spawn(serve(client, rx, tx));
    let mut failure = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(5)).await;
        app.update();
        let replies = observed(&app);
        assert!(
            !replies
                .iter()
                .any(|r| matches!(r.update, Update::Admitted(_, _)))
        );
        if replies.iter().any(|r| matches!(&r.update, Update::Failure(s) if s.contains("outcome may be unknown") && !s.contains("private"))) {
            failure = true;
            break;
        }
    }
    assert!(failure, "unknown admission not reported");
    commands
        .send(Request {
            generation: 2,
            command: Command::Open("s".into()),
        })
        .unwrap();
    drop(commands);
    task.await.unwrap();
    app.update();
    assert!(
        observed(&app)
            .iter()
            .any(|r| matches!(r.update, Update::History(_, _, _, None)))
    );
    server.join().unwrap();
}

#[tokio::test]
async fn paging_keeps_search_query_even_with_same_view_generation() {
    let (client, server) = fixture(vec![
        list_wire(),
        json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
        registry_wire(),
        json_wire(
            "GET",
            "/v1/sessions?limit=100&q=first",
            json!({"sessions":[],"next_cursor":"page2"}),
        ),
        json_wire(
            "GET",
            "/v1/sessions?limit=100&q=second",
            json!({"sessions":[]}),
        ),
        json_wire(
            "GET",
            "/v1/sessions?limit=100&q=first&cursor=page2",
            json!({"sessions":[]}),
        ),
    ]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    for command in [
        Command::List("first".into()),
        Command::List("second".into()),
        Command::More {
            query: "first".into(),
            cursor: "page2".into(),
        },
    ] {
        commands
            .send(Request {
                generation: 7,
                command,
            })
            .unwrap();
    }
    drop(commands);
    serve(client, rx, tx).await;
    app.update();
    let replies = observed(&app);
    assert!(replies[3..].iter().all(|r| r.generation == 7));
    assert!(
        matches!(&replies[3].update, Update::SessionsPage {query, cursor:Some(c),append:false,..} if query == "first" && c == "page2")
    );
    assert!(
        matches!(&replies[4].update, Update::SessionsPage {query,append:false,..} if query == "second")
    );
    assert!(
        matches!(&replies[5].update, Update::SessionsPage {query,append:true,..} if query == "first")
    );
    server.join().unwrap();
}

// Startup catalogs are deliberately unordered; only normalize them for older
// assertions about subsequent serialized mutation replies.
fn observed(app: &App) -> Vec<&Reply> {
    let mut replies: Vec<_> = rndr_core::events::<Reply>(app.world()).iter().collect();
    replies.sort_by_key(|r| {
        if r.generation != 0 {
            return 3;
        }
        match r.update {
            Update::SessionsPage { .. } => 0,
            Update::Modes(_) => 2,
            _ => 1,
        }
    });
    replies
}

#[tokio::test]
async fn blocked_model_catalog_does_not_gate_navigation_or_other_catalogs() {
    let (release, wait) = std::sync::mpsc::channel();
    let (started, requested) = std::sync::mpsc::channel();
    let (client, server) = fixture_gated(
        vec![
            list_wire(),
            json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
            registry_wire(),
            active_wire(false),
            history_wire(true),
            active_wire(false),
            mode_wire("general"),
        ],
        Some(("/v1/models/catalog", wait, started)),
    );
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    commands
        .send(Request {
            generation: 3,
            command: Command::Open("s".into()),
        })
        .unwrap();
    let task = tokio::spawn(serve(client, rx, tx));
    let mut loaded = false;
    let mut sessions = false;
    let mut modes = false;
    let mut model_requested = false;
    for _ in 0..200 {
        model_requested |= requested.try_recv().is_ok();
        tokio::time::sleep(Duration::from_millis(5)).await;
        app.update();
        for reply in observed(&app) {
            loaded |=
                matches!(reply.update, Update::History(_, _, _, None)) && reply.generation == 3;
            sessions |= matches!(reply.update, Update::SessionsPage { .. });
            modes |= matches!(reply.update, Update::Modes(_));
            assert!(
                !matches!(reply.update, Update::Models(_)),
                "blocked response escaped gate"
            );
        }
        if loaded && sessions && modes && model_requested {
            break;
        }
    }
    // Release even on failure so the fixture cannot hang in teardown.
    release.send(()).unwrap();
    drop(commands);
    task.await.unwrap();
    server.join().unwrap();
    assert!(
        loaded && sessions && modes && model_requested,
        "history, sessions and modes must load while model response is blocked"
    );
}

#[tokio::test]
async fn blocked_session_mode_does_not_gate_or_overwrite_same_generation_mutation() {
    let (release, wait) = std::sync::mpsc::channel();
    let (started, requested) = std::sync::mpsc::channel();
    let (client, server) = fixture_gated(
        vec![
            list_wire(),
            json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
            registry_wire(),
            active_wire(false),
            history_wire(true),
            active_wire(false),
            mode_wire("general"),
            json_wire("PUT", "/v1/sessions/s/agent-mode", mode_body("teacher")),
        ],
        Some(("/v1/sessions/s/agent-mode", wait, started)),
    );
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    commands
        .send(Request {
            generation: 1,
            command: Command::Open("s".into()),
        })
        .unwrap();
    let task = tokio::spawn(serve(client, rx, tx));
    let mut history_loaded = false;
    let mut mode_requested = false;
    for _ in 0..200 {
        mode_requested |= requested.try_recv().is_ok();
        tokio::time::sleep(Duration::from_millis(5)).await;
        app.update();
        if observed(&app)
            .iter()
            .any(|r| matches!(r.update, Update::History(_, _, _, _)))
        {
            history_loaded = true;
        }
        if history_loaded && mode_requested {
            break;
        }
    }
    commands
        .send(Request {
            generation: 1,
            command: Command::SetMode("s".into(), AgentModeId::Teacher),
        })
        .unwrap();
    let mut mutation_finished = false;
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(5)).await;
        app.update();
        if observed(&app)
            .iter()
            .any(|r| r.generation == 1 && matches!(&r.update, Update::Mode(m) if m == "teacher"))
        {
            mutation_finished = true;
            break;
        }
    }
    release.send(()).unwrap();
    drop(commands);
    task.await.unwrap();
    app.update();
    assert!(
        !observed(&app)
            .iter()
            .any(|r| matches!(&r.update, Update::Mode(m) if m == "general")),
        "cancelled metadata must not overwrite the newer mutation"
    );
    server.join().unwrap();
    assert!(
        history_loaded && mode_requested && mutation_finished,
        "session metadata must not block commands"
    );
}

#[test]
fn daemon_absolute_stream_urls_are_local_only() {
    let origin = "http://127.0.0.1:9775";
    assert_eq!(
        stream_path("http://127.0.0.1:9775/v1/interactive/turn/t/stream", origin),
        Ok("/v1/interactive/turn/t/stream")
    );
    for path in [
        "http://127.0.0.1:9776/v1/stream",
        "http://127.0.0.1:9775.evil/v1/stream",
        "http://127.0.0.1:9775@evil/v1/stream",
        "http://127.0.0.1:9775/v1/../secret",
    ] {
        assert!(stream_path(path, origin).is_err(), "{path}");
    }
}

#[test]
fn advertised_lan_stream_is_rebased_without_following_its_host() {
    assert_eq!(
        stream_path(
            "http://10.27.27.180:7419/v1/interactive/turn/daemon-turn-abcd/stream",
            "http://127.0.0.1:7419"
        ),
        Ok("/v1/interactive/turn/daemon-turn-abcd/stream")
    );
    for path in [
        "http://remote/v1/interactive/turn/../stream",
        "http://remote/v1/interactive/turn/%61/stream",
        "http://remote/v1/interactive/turn/a/stream?token=x",
        "http://remote/v1/interactive/turn/a/stream#x",
    ] {
        assert!(stream_path(path, "http://127.0.0.1:7419").is_err());
    }
}
