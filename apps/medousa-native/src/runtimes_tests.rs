//! Runtime tests use only deterministic loopback fixtures; no daemon/provider calls.
use super::*;
use medousa_sdk::transport::HttpTransport;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
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
fn world() -> World {
    let mut w = World::new();
    w.insert_resource(Runtimes::default());
    w
}
fn agent(w: &mut World, session: &str, agent: &str) {
    let r = w.resource_mut::<Runtimes>().unwrap();
    r.visible = Some(session.into());
    r.sessions.insert(
        session.into(),
        Session {
            active: ActiveRuntime::Agent {
                runtime: "fixture".into(),
                agent_session_id: agent.into(),
            },
            ..Session::default()
        },
    );
}
fn command(operation: Operation) -> RuntimeCommand {
    RuntimeCommand {
        session: "s".into(),
        serial: 1,
        operation,
    }
}
fn runtime() -> Value {
    json!({"runtime":"fixture","available":true,"uses_native_turns":false,"binary_present":true,"auth_status":"signed_in"})
}
fn catalog() -> Wire {
    json_wire(
        "GET",
        "/v1/agents/runtimes",
        json!({"runtimes":[runtime()]}),
    )
}
fn guards(work: Option<&str>, mode: &str) -> Vec<Wire> {
    vec![
        json_wire(
            "GET",
            "/v1/sessions/s/bot",
            json!({"session_id":"s","binding":null}),
        ),
        json_wire(
            "GET",
            "/v1/sessions/s/code-binding",
            json!({"session_id":"s","work_id":work}),
        ),
        json_wire(
            "GET",
            "/v1/sessions/s/agent-mode",
            json!({"session_id":"s","effective_mode":mode,"effective_source":"default","revision":0}),
        ),
    ]
}
fn conversation_body() -> Value {
    json!({"id":"connected","provider":"muse","label":"My agent","target":"fixture","created_at":"2026-11-07T00:00:00Z","updated_at":"2026-11-07T00:00:00Z","events":[]})
}
#[test]
fn runtime_gates_are_daemon_driven() {
    let mut row: AgentRuntimeInfo = serde_json::from_value(runtime()).unwrap();
    assert!(enabled(&row));
    row.auth_status = Some("signed_out".into());
    assert!(!enabled(&row));
    row.auth_status = None;
    row.binary_present = false;
    assert!(!enabled(&row));
    row.uses_native_turns = true;
    assert!(enabled(&row));
    row.available = false;
    assert!(!enabled(&row));
}
#[test]
fn send_route_is_explicit_and_owns_scope() {
    let mut w = world();
    assert!(!send(&mut w, "native", "hello".into(), false).unwrap());
    agent(&mut w, "s", "a");
    assert!(send(&mut w, "s", "hello".into(), false).unwrap());
    w.resource_mut::<Runtimes>().unwrap().visible = Some("other".into());
    let commands = drain_commands(&mut w);
    assert_eq!(commands[0].session, "s");
    assert!(
        matches!(&commands[0].operation, Operation::Prompt { agent, prompt, .. } if agent == "a" && prompt == "hello")
    );
    assert!(send(&mut w, "s", "second".into(), false).is_err());
}
#[test]
fn attachments_never_silently_fall_back_to_native() {
    let mut w = world();
    agent(&mut w, "s", "a");
    assert!(send(&mut w, "s", "hi".into(), true).is_err());
    assert!(drain_commands(&mut w).is_empty());
}
#[test]
fn late_reply_only_changes_its_origin_session() {
    let mut w = world();
    agent(&mut w, "s", "a");
    send(&mut w, "s", "hi".into(), false).unwrap();
    let c = drain_commands(&mut w).pop().unwrap();
    agent(&mut w, "other", "b");
    apply(
        &mut w,
        RuntimeUpdate {
            command: c,
            result: Ok(Payload::Prompt),
            may_have_mutated: true,
        },
    );
    assert_eq!(
        active(&w),
        ActiveRuntime::Agent {
            runtime: "fixture".into(),
            agent_session_id: "b".into()
        }
    );
    assert!(w.resource::<Runtimes>().unwrap().sessions["s"].running);
}
#[test]
fn stale_reply_cannot_release_newer_pending_action() {
    let mut w = world();
    agent(&mut w, "s", "a");
    send(&mut w, "s", "one".into(), false).unwrap();
    let c = drain_commands(&mut w).pop().unwrap();
    w.resource_mut::<Runtimes>()
        .unwrap()
        .sessions
        .get_mut("s")
        .unwrap()
        .pending = Some(c.serial + 1);
    apply(
        &mut w,
        RuntimeUpdate {
            command: c,
            result: Ok(Payload::Prompt),
            may_have_mutated: true,
        },
    );
    assert!(
        w.resource::<Runtimes>().unwrap().sessions["s"]
            .pending
            .is_some()
    );
    assert!(w.resource::<Runtimes>().unwrap().sessions["s"].running);
}
#[test]
fn unknown_mutation_is_not_replayed_or_resolved_by_catalog() {
    let mut w = world();
    agent(&mut w, "s", "a");
    send(&mut w, "s", "one".into(), false).unwrap();
    let c = drain_commands(&mut w).pop().unwrap();
    apply(
        &mut w,
        RuntimeUpdate {
            command: c,
            result: Err("Connection lost".into()),
            may_have_mutated: true,
        },
    );
    assert!(send(&mut w, "s", "one".into(), false).is_err());
    refresh(&mut w, "s");
    let c = drain_commands(&mut w).pop().unwrap();
    apply(
        &mut w,
        RuntimeUpdate {
            command: c,
            result: Ok(Payload::Catalog {
                runtimes: Ok(vec![]),
                conversations: Ok(vec![]),
            }),
            may_have_mutated: false,
        },
    );
    assert!(send(&mut w, "s", "one".into(), false).is_err());
    reconciled(&mut w, "s");
    assert!(send(&mut w, "s", "different".into(), false).unwrap());
}
#[test]
fn terminal_settle_requires_exact_agent() {
    let mut w = world();
    agent(&mut w, "s", "a");
    w.resource_mut::<Runtimes>()
        .unwrap()
        .sessions
        .get_mut("s")
        .unwrap()
        .running = true;
    settled(&mut w, "s", "old");
    assert!(busy(&w));
    settled(&mut w, "s", "a");
    assert!(!busy(&w));
}
#[test]
fn preflight_failure_does_not_poison_session() {
    let mut w = world();
    refresh(&mut w, "s");
    let c = drain_commands(&mut w).pop().unwrap();
    apply(
        &mut w,
        RuntimeUpdate {
            command: c,
            result: Err("No project".into()),
            may_have_mutated: false,
        },
    );
    assert!(!w.resource::<Runtimes>().unwrap().sessions["s"].uncertain);
}
#[tokio::test]
async fn create_uses_real_runtime_and_bound_project_no_prompt() {
    let mut wires = vec![catalog()];
    wires.extend(guards(Some("work"), "coder"));
    let mut create = json_wire(
        "POST",
        "/v1/agents/sessions",
        json!({"agent_session_id":"a","session_id":"s","runtime":"fixture","phase":"accepted","stream_url":"/v1/agents/sessions/a/stream","stream_ready":true,"accepted_at_utc":"2026-11-07T00:00:00Z","work_id":"work","capabilities":{"resume":false,"fork":false,"images":false,"audio":false,"terminal":false}}),
    );
    create.check = Some(|raw| {
        let body: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["session_id"], "s");
        assert_eq!(body["runtime"], "fixture");
        assert_eq!(body["work_id"], "work");
        assert!(body.get("prompt").is_none());
        assert!(body.get("cwd").is_none());
    });
    wires.push(create);
    let (client, h) = fixture(wires);
    let update = execute(
        &client,
        command(Operation::Select {
            runtime: "fixture".into(),
            previous: None,
        }),
    )
    .await
    .unwrap();
    assert!(matches!(update.result, Ok(Payload::Selected(Some(_)))));
    assert!(update.may_have_mutated);
    h.join().unwrap();
}
#[tokio::test]
async fn coder_without_project_does_not_create_agent() {
    let mut wires = vec![catalog()];
    wires.extend(guards(None, "coder"));
    let (client, h) = fixture(wires);
    let update = execute(
        &client,
        command(Operation::Select {
            runtime: "fixture".into(),
            previous: None,
        }),
    )
    .await
    .unwrap();
    assert!(update.result.unwrap_err().contains("project"));
    assert!(!update.may_have_mutated);
    h.join().unwrap();
}
#[tokio::test]
async fn project_change_prevents_prompt_on_old_agent() {
    let (client, h) = fixture(guards(Some("new-work"), "coder"));
    let update = execute(
        &client,
        command(Operation::Prompt {
            agent: "a".into(),
            prompt: "do work".into(),
            work_id: Some("old-work".into()),
        }),
    )
    .await
    .unwrap();
    assert!(update.result.unwrap_err().contains("binding changed"));
    assert!(!update.may_have_mutated);
    h.join().unwrap();
}
#[tokio::test]
async fn unavailable_runtime_never_starts() {
    let (client, h) = fixture(vec![json_wire(
        "GET",
        "/v1/agents/runtimes",
        json!({"runtimes":[]}),
    )]);
    let update = execute(
        &client,
        command(Operation::Select {
            runtime: "invented".into(),
            previous: None,
        }),
    )
    .await
    .unwrap();
    assert!(update.result.is_err());
    assert!(!update.may_have_mutated);
    h.join().unwrap();
}
#[tokio::test]
async fn config_keeps_exact_agent_option_and_typed_value() {
    let mut wire = json_wire(
        "POST",
        "/v1/agents/sessions/a/config",
        json!({"agent_session_id":"a","config_options":[]}),
    );
    wire.check = Some(|raw| {
        let body: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["config_id"], "reasoning");
        assert_eq!(body["value"], 42);
    });
    let (client, h) = fixture(vec![wire]);
    let update = execute(
        &client,
        command(Operation::Config {
            agent: "a".into(),
            config: "reasoning".into(),
            value: json!(42),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(update.result, Ok(Payload::Config(_))));
    h.join().unwrap();
}
#[tokio::test]
async fn provider_send_keeps_request_and_conversation_ids() {
    let mut wire = json_wire(
        "POST",
        "/v1/external/conversations/connected/messages",
        conversation_body(),
    );
    // The generated op is authoritative, not a guessed alternate transport.
    wire.path = expand_path(
        ops::EXTERNAL_CONVERSATIONS_BY_ID_MESSAGES_POST.path,
        &[("id", "connected")],
    )
    .unwrap();
    wire.check = Some(|raw| {
        let body: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["request_id"], "req-fixed");
        assert_eq!(body["text"], "hello");
    });
    let (client, h) = fixture(vec![wire]);
    let update = execute(
        &client,
        command(Operation::SendProvider {
            conversation: "connected".into(),
            text: "hello".into(),
            request_id: "req-fixed".into(),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(update.result, Ok(Payload::Provider(_))));
    h.join().unwrap();
}
#[tokio::test]
async fn provider_list_survives_runtime_catalog_failure() {
    let mut first = json_wire("GET", "/v1/agents/runtimes", json!({"error":"offline"}));
    first.status = 503;
    let second = json_wire(
        "GET",
        ops::EXTERNAL_CONVERSATIONS_GET.path,
        json!({"conversations":[conversation_body()]}),
    );
    let (client, h) = fixture(vec![first, second]);
    let update = execute(&client, command(Operation::Refresh)).await.unwrap();
    assert!(matches!(
        update.result,
        Ok(Payload::Catalog {
            runtimes: Err(_),
            conversations: Ok(_)
        })
    ));
    h.join().unwrap();
}
#[test]
fn terminal_before_http_ack_does_not_resurrect_running() {
    let mut w = world();
    agent(&mut w, "s", "a");
    send(&mut w, "s", "hello".into(), false).unwrap();
    let c = drain_commands(&mut w).pop().unwrap();
    settled(&mut w, "s", "a");
    apply(
        &mut w,
        RuntimeUpdate {
            command: c,
            result: Ok(Payload::Prompt),
            may_have_mutated: true,
        },
    );
    assert!(!busy(&w));
}
#[test]
fn cancel_marks_closed_without_falling_back_to_native() {
    let mut w = world();
    agent(&mut w, "s", "a");
    assert!(cancel(&mut w, "s"));
    let c = drain_commands(&mut w).pop().unwrap();
    apply(
        &mut w,
        RuntimeUpdate {
            command: c,
            result: Ok(Payload::Cancel),
            may_have_mutated: true,
        },
    );
    assert!(matches!(active(&w), ActiveRuntime::Agent { .. }));
    assert!(
        send(&mut w, "s", "hello".into(), false)
            .unwrap_err()
            .contains("stopped")
    );
}
#[tokio::test]
async fn stream_negotiates_v3_and_returns_canonical_session_scoped_commands() {
    let envelope = json!({"schema_version":3,"turn_id":"a","seq":2,"emitted_at_utc":"2026-11-07T00:00:00Z","event":{"type":"turn_completed","outcome":"completed","aggregate_text":"done"}});
    let wire = Wire {
        method: "GET",
        path: "/v1/agents/sessions/a/stream".into(),
        status: 200,
        content_type: "text/event-stream",
        body: format!("data: {envelope}\n\n"),
        check: Some(|raw| {
            assert!(
                raw.to_lowercase()
                    .contains(medousa_types::turn_stream::TURN_STREAM_V3_MEDIA_TYPE)
            )
        }),
    };
    let (client, h) = fixture(vec![wire]);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    stream(
        &client,
        RuntimeStream {
            session: "original".into(),
            agent_session_id: "a".into(),
            stream_url: "/v1/agents/sessions/a/stream".into(),
        },
        tx,
    )
    .await;
    assert!(
        matches!(rx.recv().await.unwrap().unwrap(),ChatCommand::ApplyStreamEvent{session_id,envelope,..} if session_id=="original" && envelope.seq==2 && envelope.event.is_terminal())
    );
    assert!(rx.recv().await.unwrap().is_err());
    h.join().unwrap();
}
#[test]
fn runtime_menu_lists_ready_runtimes_and_folds_setup_into_one_entry() {
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    app.world_mut().insert_resource(Runtimes::default());
    let anchor = app.world_mut().spawn((Layout {
        width: Size::Px(120.),
        height: Size::Px(30.),
        ..Layout::row()
    },));
    app.update();
    let missing = json!({"runtime":"hermes","available":true,"uses_native_turns":false,"binary_present":false,"detail":"Hermes CLI not found"});
    app.world_mut().resource_mut::<Runtimes>().unwrap().catalog =
        serde_json::from_value(json!([runtime(), missing])).unwrap();
    open(app.world_mut(), anchor, "s");
    app.update();
    let w = app.world();
    let shown = |mut e: Entity| loop {
        if w.has::<Hidden>(e) {
            return false;
        }
        match w.get::<Parent>(e) {
            Some(p) => e = p.entity,
            None => return true,
        }
    };
    let labels: Vec<String> = w
        .query::<Label>()
        .iter()
        .filter(|(e, _)| shown(*e))
        .map(|(_, l)| l.text.clone())
        .collect();
    for expected in ["Medousa", "fixture", "Needs setup (1)", "Refresh runtimes"] {
        assert!(
            labels.iter().any(|l| l == expected),
            "{expected} in {labels:?}"
        );
    }
    // An unavailable runtime never sits among the choices themselves.
    assert!(!labels.iter().any(|l| l.starts_with("hermes")));
    // Picking a ready runtime queues the same select the panel used to.
    let menu = w.resource::<Runtimes>().unwrap().menu.as_ref().unwrap();
    let (root, item) = (
        menu.handle.root,
        *menu
            .picks
            .iter()
            .find(|(_, p)| matches!(p, Pick::Run(Operation::Select { runtime, .. }) if runtime == "fixture"))
            .unwrap()
            .0,
    );
    rndr_core::send(
        app.world_mut(),
        rndr_ui::menu::MenuPicked { menu: root, item },
    );
    app.update();
    menu_events(app.world_mut(), Some("s"));
    let queued = drain_commands(app.world_mut());
    assert!(matches!(
        queued.as_slice(),
        [RuntimeCommand { operation: Operation::Select { runtime, .. }, .. }] if runtime == "fixture"
    ));
}
