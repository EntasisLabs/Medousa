//! Test-only loopback HTTP fixtures. Never contact a daemon/provider or keychain.
use super::*;
use medousa_rndr_home::Chat;
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
        json_wire("GET", "/v1/sessions/s/active-turn", active.clone()),
        json_wire("GET", "/v1/sessions/s/history", history_body.clone()),
        json_wire("GET", "/v1/sessions/s/active-turn", active.clone()),
        admission,
        first,
        second,
        json_wire("GET", "/v1/sessions/s/active-turn", active.clone()),
        json_wire("GET", "/v1/sessions/s/history", history_body),
        json_wire("GET", "/v1/sessions/s/active-turn", active),
        mode_wire("general"),
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
    follow(client.clone(), ticket.stream_url, sender, 5, "s".into()).await;
    app.update();
    let updates = rndr_core::events::<Reply>(app.world()).to_vec();
    assert_eq!(updates.len(), 6);
    for reply in updates {
        assert_eq!(reply.generation, 5);
        match reply.update {
            Update::Stream(e) => chat.apply(e),
            Update::SettledHistory(_, _) => assert!(!chat.busy),
            Update::Messages(messages) => {
                assert_eq!(messages, vec![("user".into(), "Existing message".into())])
            }
            Update::Mode(value) => assert_eq!(value, "general"),
            _ => panic!("unexpected error"),
        }
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
async fn canonical_roles_preserve_heading_like_user_prose_without_inventing_turns() {
    let prompt = "### Medousa\n\nnot an assistant message\n\n### You\n\nplain text";
    let mut body = history_body(true);
    body["turns"][0]["content"] = json!(prompt);
    let (client, server) = fixture(vec![
        active_wire(false),
        json_wire("GET", "/v1/sessions/s/history", body),
        active_wire(false),
    ]);
    let (_, messages) = history_with_messages(&client, "s").await.unwrap();
    assert_eq!(
        messages,
        vec![
            ("user".into(), prompt.into()),
            ("assistant".into(), "unique assistant prose".into())
        ]
    );
    server.join().unwrap();
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
async fn active_reattach_full_replay_terminal_has_no_duplicate_or_missing_prose() {
    // Includes already-persisted assistant prose of this execution. A timestamp
    // or naive append boundary would duplicate it. Terminal reconciliation also
    // covers user persistence occurring AFTER the initial history read.
    for user_already_persisted in [true, false] {
        let mut initial = history_body(true);
        if !user_already_persisted {
            initial["turns"] = json!([]);
        }
        let (client, server) = fixture(vec![
            active_wire(true),
            json_wire("GET", "/v1/sessions/s/history", initial),
            active_wire(true),
            Wire {
                method: "GET",
                path: "/v1/interactive/turn/t/stream".into(),
                status: 200,
                content_type: "text/event-stream",
                check: None,
                body: sse(
                    1,
                    json!({"type":"assistant_text_started","segment_id":"a","model_round":1}),
                ) + &sse(
                    2,
                    json!({"type":"content_append","segment_id":"a","text":"unique assistant prose"}),
                ) + &sse(
                    2,
                    json!({"type":"content_append","segment_id":"a","text":"duplicate"}),
                ) + &sse(
                    3,
                    json!({"type":"turn_completed","outcome":"completed","aggregate_text":"unique assistant prose"}),
                ),
            },
            active_wire(false),
            history_wire(true),
            active_wire(false),
            mode_wire("general"),
        ]);
        let Update::History(id, text, Some(turn)) = history(&client, "s").await.unwrap() else {
            panic!("missing active turn")
        };
        assert!(!text.contains("unique assistant prose"));
        let mut chat = Chat::history(id, text, true);
        chat.resume(turn.turn_id);
        let mut app = App::new();
        let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
        follow(client, turn.stream_url, tx, 1, "s".into()).await;
        app.update();
        for reply in rndr_core::events::<Reply>(app.world()).to_vec() {
            match reply.update {
                Update::Stream(e) => chat.apply(e),
                Update::SettledHistory(id, text) => chat = Chat::history(id, text, false),
                Update::Mode(value) => assert_eq!(value, "general"),
                Update::Messages(messages) => assert_eq!(
                    messages,
                    vec![
                        ("user".into(), "unique user prose".into()),
                        ("assistant".into(), "unique assistant prose".into())
                    ]
                ),
                _ => panic!("reattach failed"),
            }
        }
        assert_eq!(chat.transcript.matches("unique user prose").count(), 1);
        assert_eq!(chat.transcript.matches("unique assistant prose").count(), 1);
        assert!(!chat.transcript.contains("duplicate"));
        assert!(!chat.busy && !chat.uncertain);
        server.join().unwrap();
    }
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
    let Update::History(_, text, None) = history(&client, "s").await.unwrap() else {
        panic!("stale history")
    };
    assert_eq!(text.matches("unique user prose").count(), 1);
    assert_eq!(text.matches("unique assistant prose").count(), 1);
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
        Update::SettledHistory(_, _)
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
async fn fatal_terminal_never_replaces_live_text_with_incomplete_history() {
    let (client, server) = fixture(vec![Wire {
        method: "GET",
        path: "/v1/interactive/turn/t/stream".into(),
        status: 200,
        content_type: "text/event-stream",
        check: None,
        body: sse(
            1,
            json!({"type":"content_append","segment_id":"a","text":"unsaved assistant prose"}),
        ) + &sse(
            2,
            json!({"type":"turn_completed","outcome":"fatal","aggregate_text":"unsaved assistant prose"}),
        ),
    }]);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    follow(
        client,
        "/v1/interactive/turn/t/stream".into(),
        tx,
        1,
        "s".into(),
    )
    .await;
    app.update();
    let mut chat = Chat::history("s".into(), String::new(), false);
    chat.begin("original user prose").unwrap();
    chat.admitted("t".into());
    for reply in rndr_core::events::<Reply>(app.world()).to_vec() {
        match reply.update {
            Update::Stream(e) => chat.apply(e),
            Update::Failure(message) => chat.fail(message),
            _ => panic!("must not reconcile away fatal unsaved text"),
        }
    }
    assert!(chat.transcript.contains("original user prose"));
    assert!(chat.transcript.contains("unsaved assistant prose"));
    assert!(chat.uncertain);
    server.join().unwrap();
}
#[tokio::test]
async fn catalog_uses_real_snake_case_record_and_is_read_only() {
    let (client, server) = fixture(vec![
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
    assert!(matches!(&rndr_core::events::<Reply>(app.world())[0].update,
        Update::Models(models) if models == &vec![("mock".into(), "test-only".into())]));
    server.join().unwrap();
}
#[tokio::test]
async fn ui_reload_drives_real_worker_reattach_and_settlement() {
    let (client, server) = fixture(vec![
        json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
        registry_wire(),
        active_wire(true),
        history_wire(true),
        active_wire(true),
        mode_wire("general"),
        Wire {
            method: "GET",
            path: "/v1/interactive/turn/t/stream".into(),
            status: 200,
            content_type: "text/event-stream",
            check: None,
            body: sse(
                1,
                json!({"type":"content_append","segment_id":"a","text":"unique assistant prose"}),
            ) + &sse(
                2,
                json!({"type":"turn_completed","outcome":"completed","aggregate_text":"unique assistant prose"}),
            ),
        },
        active_wire(false),
        history_wire(true),
        active_wire(false),
        mode_wire("general"),
    ]);
    let mut app = crate::app();
    app.update();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    let model = app.world_mut().resource_mut::<crate::Model>().unwrap();
    model.commands = Some(commands);
    model.chat = Chat::history("s".into(), String::new(), false);
    let worker = tokio::spawn(serve(client, rx, tx));
    app.update();
    let reload = app
        .world()
        .entities_with::<rndr_core::Button>()
        .into_iter()
        .find(|e| app.world().get::<rndr_core::Button>(*e).unwrap().action == "reload")
        .unwrap();
    rndr_core::try_activate(app.world_mut(), reload);
    app.update();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            app.update();
            let model = app.world().resource::<crate::Model>().unwrap();
            if model.chat.status.contains("Completed · history reconciled") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let model = app.world().resource::<crate::Model>().unwrap();
    assert_eq!(model.generation, 1);
    assert_eq!(
        model.chat.transcript.matches("unique user prose").count(),
        1
    );
    assert_eq!(
        model
            .chat
            .transcript
            .matches("unique assistant prose")
            .count(),
        1
    );
    assert!(!model.chat.busy && !model.chat.uncertain && !model.loading);
    assert_eq!(
        model.history_messages,
        vec![
            ("user".into(), "unique user prose".into()),
            ("assistant".into(), "unique assistant prose".into())
        ]
    );
    assert!(model.chat.live_messages.is_empty());
    // Let the terminal mode refresh complete before shutting down the worker.
    tokio::task::spawn_blocking(move || server.join().unwrap())
        .await
        .unwrap();
    app.world_mut()
        .resource_mut::<crate::Model>()
        .unwrap()
        .commands = None;
    worker.await.unwrap();
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

#[tokio::test]
async fn session_mode_mutation_uses_session_scope_and_server_effective_value() {
    let mut change = json_wire("PUT", "/v1/sessions/s/agent-mode", mode_body("teacher"));
    change.check = Some(|raw| {
        let body: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body, json!({"mode":"coder", "scope":"session"}));
    });
    let (client, server) = fixture(vec![
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
    let replies = rndr_core::events::<Reply>(app.world());
    assert!(matches!(&replies[1].update, Update::Modes(modes)
        if modes == &vec![("general".into(), "General".into(), true), ("coder".into(), "Coder".into(), false)]));
    assert_eq!(replies[2].generation, 9);
    assert!(matches!(&replies[2].update, Update::Mode(value) if value == "teacher"));
    server.join().unwrap();
}

#[tokio::test]
async fn new_recent_and_open_refresh_effective_mode_after_canonical_history() {
    let mut wires = vec![
        json_wire("GET", "/v1/models/catalog", json!({"models":[]})),
        registry_wire(),
        json_wire(
            "POST",
            "/v1/sessions",
            json!({"authority_id":format!("auth_{}", "a".repeat(64)),"session_id":"s","catalog":"single"}),
        ),
    ];
    let reads = || {
        vec![
            active_wire(false),
            history_wire(true),
            active_wire(false),
            mode_wire("instant"),
        ]
    };
    wires.extend(reads());
    wires.push(json_wire(
        "GET",
        "/v1/sessions?limit=1",
        json!({"sessions":[{"session_id":"s","catalog":"single", "turns":2, "verification_runs":0, "preview":"Fixture"}]}),
    ));
    wires.extend(reads());
    wires.extend(reads());
    let (client, server) = fixture(wires);
    let mut app = App::new();
    let (_feed, tx) = feed::open::<Reply>(app.world_mut(), FeedConfig::queue(32));
    let (commands, rx) = mpsc::unbounded_channel();
    for (generation, command) in [
        (1, Command::New),
        (2, Command::Recent),
        (3, Command::Open("s".into())),
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
    let replies = rndr_core::events::<Reply>(app.world());
    assert_eq!(replies.len(), 11);
    for (index, pair) in replies[2..].as_chunks::<3>().0.iter().enumerate() {
        assert_eq!(pair[0].generation, index as u64 + 1);
        assert_eq!(pair[1].generation, index as u64 + 1);
        assert!(matches!(&pair[0].update, Update::History(id, _, None) if id == "s"));
        assert_eq!(pair[2].generation, index as u64 + 1);
        assert!(matches!(&pair[1].update, Update::Messages(messages) if messages.len() == 2));
        assert!(matches!(&pair[2].update, Update::Mode(value) if value == "instant"));
    }
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
