use super::*;
fn shell(id: &str, work: &str) -> Value {
    json!({"session_id":id,"work_id":work,"root_kind":"workspace","argv":[],"exited":false,"cwd":"/work/project"})
}
#[test]
fn discovery_is_human_only_scoped_and_preserves_binding() {
    let good = shell("human", "one");
    let mut agent = shell("agent", "one");
    agent["root_kind"] = json!("forge");
    let mut task = shell("task", "one");
    task["argv"] = json!(["cargo", "test"]);
    let mut dead = shell("dead", "one");
    dead["exited"] = json!(true);
    let list = json!({"sessions":[agent,task,shell("other","two"),dead,good]});
    assert_eq!(
        choose(&list, "one", None).unwrap().unwrap()["session_id"],
        "human"
    );
    for id in ["agent", "task", "other", "missing"] {
        assert!(choose(&list, "one", Some(id)).is_err());
    }
    assert_eq!(
        choose(&list, "one", Some("dead")).unwrap().unwrap()["exited"],
        true
    );
    assert!(choose(&list, "three", None).unwrap().is_none());
    assert!(choose(&json!({}), "one", None).is_err());
}
#[test]
fn authentication_never_escapes_local_workshop() {
    let req = request("http://127.0.0.1:9999", "shell/id ?", "test-token").unwrap();
    assert_eq!(
        req.uri().to_string(),
        "ws://127.0.0.1:9999/v1/sessions/shell/shell%2Fid%20%3F"
    );
    assert_eq!(req.headers()[AUTHORIZATION], "Bearer test-token");
    assert!(req.headers()[AUTHORIZATION].is_sensitive());
    assert!(!req.uri().to_string().contains("test-token"));
    for origin in [
        "http://example.com",
        "file:///tmp",
        "http://localhost/a",
        "http://user@localhost",
        "http://localhost/?token=x",
    ] {
        assert!(request(origin, "s", "token").is_err());
    }
    assert!(request("http://localhost", "s", "invalid\r\nheader").is_err());
}
#[test]
fn protocol_preserves_raw_bytes_and_names_failures() {
    let raw = b"\x1b[31mred\x1b[0m\r\n\xff";
    assert!(
        matches!(decode_text(&json!({"type":"stdout","data":STANDARD.encode(raw)}).to_string()).unwrap(),Some(Fact::Output(b)) if b==raw)
    );
    assert!(decode_text(r#"{"type":"stdout","data":"!!"}"#).is_err());
    assert!(decode_text("invalid").is_err());
    assert!(
        matches!(decode_text(r#"{"type":"exit","exit_code":7}"#).unwrap(),Some(Fact::End(s)) if s.contains('7'))
    );
    assert!(
        decode_text(r#"{"type":"error","message":"read only"}"#)
            .unwrap_err()
            .contains("read only")
    );
    assert!(
        decode_text(r#"{"type":"resize","cols":80,"rows":24}"#)
            .unwrap()
            .is_none()
    );
}
fn fixture() -> (App, mpsc::Receiver<Open>, mpsc::Sender<Update>) {
    let mut app = crate::ui::app();
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    let (tx, opens) = mpsc::channel(8);
    let (updates, rx) = mpsc::channel(64);
    let (wake, _) = feed::open::<Wake>(app.world_mut(), FeedConfig::latest());
    app.world_mut().insert_resource(Service {
        tx,
        rx: Mutex::new(rx),
        _wake: wake,
    });
    sync(app.world_mut(), Some("one".into()));
    (app, opens, updates)
}
fn pane<'a>(w: &'a World, work: &str) -> &'a Pane {
    &w.resource::<DockState>().unwrap().0.as_ref().unwrap().panes[work]
}
fn fact(tx: &mpsc::Sender<Update>, open: &Open, fact: Fact) {
    tx.try_send(Update {
        work: open.work.clone(),
        generation: open.generation,
        fact,
    })
    .unwrap_or_else(|_| panic!("update channel"));
}
fn connected(tx: &mpsc::Sender<Update>, open: &Open) {
    fact(
        tx,
        open,
        Fact::Connected {
            session: format!("shell-{}", open.work),
            cwd: "/work/project".into(),
        },
    );
}
#[test]
fn dock_retains_shells_and_never_creates_on_undertaking_switch() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    assert!(opens.try_recv().is_err());
    action(w, &HostIntent::Command(TOGGLE));
    let mut one = opens.try_recv().unwrap();
    assert!(one.create && !one.fresh);
    // Pending creation is not repeated by extra reconnect/new clicks.
    action(w, &HostIntent::Command(RECONNECT));
    action(w, &HostIntent::Command(NEW));
    assert!(opens.try_recv().is_err());
    connected(&updates, &one);
    sync(w, Some("one".into()));
    let entity = pane(w, "one").entity;
    action(w, &HostIntent::Command(HIDE));
    sync(w, Some("one".into()));
    assert!(pane(w, "one").connected && pane(w, "one").frames.is_some());
    assert!(w.has::<Hidden>(entity));
    action(w, &HostIntent::Command(TOGGLE));
    sync(w, Some("two".into()));
    let two = opens.try_recv().unwrap();
    assert!(!two.create && !two.fresh);
    connected(&updates, &two);
    sync(w, Some("two".into()));
    assert!(w.has::<Hidden>(entity));
    assert_ne!(pane(w, "two").entity, entity);
    sync(w, Some("one".into()));
    assert_eq!(pane(w, "one").entity, entity);
    assert!(!w.has::<Hidden>(entity));
    assert!(opens.try_recv().is_err());
    // Resizes are retained on the original attachment rather than recreation.
    assert!(matches!(one.frames.try_recv(), Ok(Frame::Resize(..))));
}
#[test]
fn disconnect_discards_pending_input_and_fences_stale_updates() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    action(w, &HostIntent::Command(TOGGLE));
    let first = opens.try_recv().unwrap();
    connected(&updates, &first);
    sync(w, Some("one".into()));
    let entity = pane(w, "one").entity;
    rndr_term::write_terminal(w, entity, b"never-replay\r");
    fact(&updates, &first, Fact::End("lost connection".into()));
    sync(w, Some("one".into()));
    assert!(!pane(w, "one").connected);
    assert!(!w.has::<CapturesKeys>(entity));
    assert!(
        rndr_term::take_terminal_bytes(w, entity)
            .unwrap()
            .is_empty()
    );
    action(w, &HostIntent::Command(RECONNECT));
    let mut second = opens.try_recv().unwrap();
    assert!(!second.create && !second.fresh);
    assert_eq!(second.session.as_deref(), Some("shell-one"));
    connected(&updates, &second);
    fact(&updates, &first, Fact::End("stale".into()));
    sync(w, Some("one".into()));
    assert!(pane(w, "one").connected);
    while let Ok(frame) = second.frames.try_recv() {
        assert!(!matches!(frame, Frame::Input(_)));
    }
    fact(&updates, &second, Fact::End("Shell exited (0)".into()));
    sync(w, Some("one".into()));
    action(w, &HostIntent::Command(NEW));
    let new = opens.try_recv().unwrap();
    assert!(new.fresh && new.create && new.session.is_none());
}
#[test]
fn backpressure_disconnects_instead_of_silently_dropping_keystrokes() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    action(w, &HostIntent::Command(TOGGLE));
    let open = opens.try_recv().unwrap();
    connected(&updates, &open);
    sync(w, Some("one".into()));
    let p = pane(w, "one");
    let entity = p.entity;
    let tx = p.frames.as_ref().unwrap().clone();
    while tx.try_send(Frame::Input(vec![b'x'])).is_ok() {}
    rndr_term::write_terminal(w, entity, b"command\r");
    sync(w, Some("one".into()));
    assert!(!pane(w, "one").connected);
    assert!(pane(w, "one").status.contains("backpressure"));
}
#[test]
fn focused_terminal_keeps_shell_control_keys_out_of_editor_shortcuts() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    action(w, &HostIntent::Command(TOGGLE));
    let mut open = opens.try_recv().unwrap();
    connected(&updates, &open);
    sync(w, Some("one".into()));
    let entity = pane(w, "one").entity;
    w.resource_mut::<Focus>().unwrap().entity = Some(entity);
    let keys = vec![
        KeyEvent::character(
            'c',
            Modifiers {
                ctrl: true,
                ..Default::default()
            },
        ),
        KeyEvent::named(NamedKey::ArrowUp, Modifiers::default()),
    ];
    w.resource_mut::<Input>().unwrap().keys = keys;
    crate::code::keyboard(w);
    assert_eq!(w.resource::<Input>().unwrap().keys.len(), 2);
    // Exercise actual RNDR input and dispatch, not a synthetic command textbox.
    app.update();
    sync(app.world_mut(), Some("one".into()));
    let mut bytes = Vec::new();
    while let Ok(frame) = open.frames.try_recv() {
        if let Frame::Input(b) = frame {
            bytes.extend(b);
        }
    }
    assert!(
        bytes.contains(&3),
        "Ctrl-C must be delivered as ETX: {bytes:?}"
    );
    assert!(
        bytes.windows(3).any(|b| b == b"\x1b[A"),
        "arrow sequence: {bytes:?}"
    );
}
#[tokio::test]
async fn websocket_bridge_orders_resize_and_stdin_and_preserves_output() {
    use tokio_tungstenite::tungstenite::protocol::Role;
    let mut app = App::new();
    let (_feed, wake) = feed::open(app.world_mut(), FeedConfig::latest());
    let (a, b) = tokio::io::duplex(8192);
    let client = tokio_tungstenite::WebSocketStream::from_raw_socket(a, Role::Client, None).await;
    let mut server =
        tokio_tungstenite::WebSocketStream::from_raw_socket(b, Role::Server, None).await;
    let (frames, rx) = mpsc::channel(4);
    let (tx, mut updates) = mpsc::channel(4);
    let (_cancel, cancelled) = oneshot::channel();
    let mut open = Open {
        work: "one".into(),
        generation: 7,
        session: Some("shell-one".into()),
        create: false,
        fresh: false,
        frames: rx,
        cancelled,
    };
    let worker = tokio::spawn(async move { bridge(client, &mut open, &tx, &wake).await });
    frames.send(Frame::Resize(120, 32)).await.unwrap();
    frames
        .send(Frame::Input(b"\x03echo hi\r".to_vec()))
        .await
        .unwrap();
    let a: Value =
        serde_json::from_str(server.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    let b: Value =
        serde_json::from_str(server.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(a, json!({"type":"resize","cols":120,"rows":32}));
    assert_eq!(b["type"], "stdin");
    assert_eq!(
        STANDARD.decode(b["data"].as_str().unwrap()).unwrap(),
        b"\x03echo hi\r"
    );
    server
        .send(Message::Text(
            json!({"type":"stdout","data":STANDARD.encode(b"\x1b[32mhi\x1b[0m")})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let update = updates.recv().await.unwrap();
    assert_eq!(update.work, "one");
    assert_eq!(update.generation, 7);
    assert!(matches!(update.fact,Fact::Output(b) if b==b"\x1b[32mhi\x1b[0m"));
    server
        .send(Message::Binary(b"raw".to_vec().into()))
        .await
        .unwrap();
    assert!(matches!(updates.recv().await.unwrap().fact,Fact::Output(b) if b==b"raw"));
    server
        .send(Message::Text(
            json!({"type":"exit","exit_code":9}).to_string().into(),
        ))
        .await
        .unwrap();
    assert!(worker.await.unwrap().unwrap_err().contains('9'));
}

#[tokio::test]
async fn cancellation_drops_queued_input_before_detach() {
    use tokio_tungstenite::tungstenite::protocol::Role;
    let mut app = App::new();
    let (_feed, wake) = feed::open::<Wake>(app.world_mut(), FeedConfig::latest());
    let (a, b) = tokio::io::duplex(8192);
    let client = tokio_tungstenite::WebSocketStream::from_raw_socket(a, Role::Client, None).await;
    let mut server =
        tokio_tungstenite::WebSocketStream::from_raw_socket(b, Role::Server, None).await;
    let (frames, rx) = mpsc::channel(4);
    let (tx, _updates) = mpsc::channel(4);
    let (cancel, cancelled) = oneshot::channel();
    frames
        .send(Frame::Input(b"must-not-run\r".to_vec()))
        .await
        .unwrap();
    drop(cancel);
    let mut open = Open {
        work: "one".into(),
        generation: 1,
        session: None,
        create: false,
        fresh: false,
        frames: rx,
        cancelled,
    };
    bridge(client, &mut open, &tx, &wake).await.unwrap();
    assert!(!matches!(server.next().await, Some(Ok(Message::Text(_)))));
}

/// Synthetic only: keeps the input mailbox alive; never connects or spawns.
pub(crate) fn preview(w: &mut World, work: &str) -> Entity {
    struct Preview {
        _opens: mpsc::Receiver<Open>,
        _open: Open,
    }
    let (tx, mut opens) = mpsc::channel(8);
    let (updates, rx) = mpsc::channel(64);
    let (wake, _) = feed::open::<Wake>(w, FeedConfig::latest());
    w.insert_resource(Service {
        tx,
        rx: Mutex::new(rx),
        _wake: wake,
    });
    sync(w, Some(work.into()));
    action(w, &HostIntent::Command(TOGGLE));
    let open = opens.try_recv().unwrap();
    connected(&updates, &open);
    fact(&updates,&open,Fact::Output(b"\x1b[32mMedousa\x1b[0m \x1b[90mworkshop shell\x1b[0m\r\n\r\n\x1b[36m~/project\x1b[0m $ cargo test\r\n   Compiling medousa-native v0.1.0\r\n\r\nrunning 9 tests\r\ntest terminal::human_shell_authority ... \x1b[32mok\x1b[0m\r\ntest terminal::persistent_sessions ... \x1b[32mok\x1b[0m\r\n\r\n\x1b[32mtest result: ok.\x1b[0m\r\n\r\n\x1b[36m~/project\x1b[0m $ ".to_vec()));
    sync(w, Some(work.into()));
    let entity = pane(w, work).entity;
    w.insert_resource(Preview {
        _opens: opens,
        _open: open,
    });
    entity
}

#[test]
fn failed_attachment_retains_exact_bound_session_for_reconnect() {
    let (mut app, mut opens, updates) = fixture();
    sync(app.world_mut(), Some("one".into()));
    action(app.world_mut(), &HostIntent::Command(TOGGLE));
    let open = opens.try_recv().unwrap();
    fact(
        &updates,
        &open,
        Fact::Bound {
            session: "newly-created".into(),
            cwd: "/workshop/project".into(),
        },
    );
    fact(&updates, &open, Fact::End("Handshake failed".into()));
    sync(app.world_mut(), Some("one".into()));
    action(app.world_mut(), &HostIntent::Command(RECONNECT));
    let retry = opens.try_recv().unwrap();
    assert_eq!(retry.session.as_deref(), Some("newly-created"));
    assert!(!retry.create);
}

#[test]
fn emulator_full_screen_and_bracketed_paste_round_trip() {
    let (mut app, mut opens, updates) = fixture();
    sync(app.world_mut(), Some("one".into()));
    action(app.world_mut(), &HostIntent::Command(TOGGLE));
    let mut open = opens.try_recv().unwrap();
    connected(&updates, &open);
    fact(
        &updates,
        &open,
        Fact::Output(b"\x1b[?1049h\x1b[?2004h\x1b[2J\x1b[1;1Hinteractive program".to_vec()),
    );
    sync(app.world_mut(), Some("one".into()));
    let entity = pane(app.world(), "one").entity;
    app.world_mut().insert_resource(Platform::Mac);
    app.world_mut().insert_resource(Clipboard {
        text: "echo paste\n".into(),
        ..Default::default()
    });
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![KeyEvent::character(
        'v',
        Modifiers {
            super_key: true,
            ..Default::default()
        },
    )];
    app.update();
    sync(app.world_mut(), Some("one".into()));
    let mut bytes = Vec::new();
    while let Ok(frame) = open.frames.try_recv() {
        if let Frame::Input(input) = frame {
            bytes.extend(input);
        }
    }
    assert_eq!(bytes, b"\x1b[200~echo paste\n\x1b[201~");
    fact(
        &updates,
        &open,
        Fact::Output(b"\x1b[?1049l\x1b[?2004l".to_vec()),
    );
    sync(app.world_mut(), Some("one".into()));
    assert!(pane(app.world(), "one").connected);
}
