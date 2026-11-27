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
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Terminal);
    let (tx, opens) = mpsc::channel(8);
    let (updates, rx) = mpsc::channel(64);
    let (wake, _) = feed::open::<Wake>(app.world_mut(), FeedConfig::latest());
    app.world_mut().insert_resource(Service {
        discover: None,
        tx,
        rx: Mutex::new(rx),
        _wake: wake,
    });
    sync(app.world_mut(), Some("one".into()));
    (app, opens, updates)
}
fn sync(w: &mut World, selected: Option<String>) {
    if let Some(work) = selected {
        open_work(w, work);
    }
    tick(w);
}
fn pane<'a>(w: &'a World, work: &str) -> &'a Pane {
    let d = w.resource::<ViewState>().unwrap().0.as_ref().unwrap();
    d.selected
        .and_then(|id| d.panes.get(&id))
        .filter(|p| p.work == work)
        .or_else(|| d.panes.values().find(|p| p.work == work))
        .unwrap()
}
fn fact(tx: &mpsc::Sender<Update>, open: &Open, fact: Fact) {
    tx.try_send(Update {
        tab: open.tab,
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
fn tabs_retain_multiple_shells_per_work_and_close_only_detaches() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    activate(w);
    assert!(opens.try_recv().is_err());
    action(w, &HostIntent::Command(NEW));
    let mut one = opens.try_recv().unwrap();
    assert!(one.create && one.fresh);
    connected(&updates, &one);
    tick(w);
    let entity = pane(w, "one").entity;
    action(w, &HostIntent::Command(NEW));
    let two = opens.try_recv().unwrap();
    assert_ne!(one.tab, two.tab);
    assert_eq!(one.work, two.work);
    connected(&updates, &two);
    tick(w);
    assert!(w.has::<Hidden>(entity));
    let mut d = w.resource_mut::<ViewState>().unwrap().0.take().unwrap();
    d.selected = Some(one.tab);
    d.redraw = true;
    w.resource_mut::<ViewState>().unwrap().0 = Some(d);
    tick(w);
    assert!(!w.has::<Hidden>(entity));
    assert!(pane(w, "one").connected);
    action(w, &HostIntent::Command(CLOSE));
    tick(w);
    assert!(one.cancelled.try_recv().is_err());
    assert!(
        !w.resource::<ViewState>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .panes
            .contains_key(&one.tab)
    );
    assert!(pane(w, "one").connected);
    assert!(!can_rebind(w));
    action(w, &HostIntent::Command(CLOSE));
    assert!(can_rebind(w));
    assert!(opens.try_recv().is_err());
}
#[test]
fn disconnect_discards_pending_input_and_fences_stale_updates() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    action(w, &HostIntent::Command(NEW));
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
    action(w, &HostIntent::Command(NEW));
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
    action(w, &HostIntent::Command(NEW));
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
        tab: 11,
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
        tab: 11,
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
        _second: Open,
    }
    let (tx, mut opens) = mpsc::channel(8);
    let (updates, rx) = mpsc::channel(64);
    let (wake, _) = feed::open::<Wake>(w, FeedConfig::latest());
    w.insert_resource(Service {
        discover: None,
        tx,
        rx: Mutex::new(rx),
        _wake: wake,
    });
    sync(w, Some(work.into()));
    action(w, &HostIntent::Command(NEW));
    let open = opens.try_recv().unwrap();
    connected(&updates, &open);
    fact(&updates,&open,Fact::Output(b"\x1b[32mMedousa\x1b[0m \x1b[90mworkshop shell\x1b[0m\r\n\r\n\x1b[36m~/project\x1b[0m $ cargo test\r\n   Compiling medousa-native v0.1.0\r\n\r\nrunning 9 tests\r\ntest terminal::human_shell_authority ... \x1b[32mok\x1b[0m\r\ntest terminal::persistent_sessions ... \x1b[32mok\x1b[0m\r\n\r\n\x1b[32mtest result: ok.\x1b[0m\r\n\r\n\x1b[36m~/project\x1b[0m $ ".to_vec()));
    sync(w, Some(work.into()));
    let entity = pane(w, work).entity;
    open_work(w, "preview-other".into());
    action(w, &HostIntent::Command(NEW));
    let second = opens.try_recv().unwrap();
    connected(&updates, &second);
    tick(w);
    let d = w.resource_mut::<ViewState>().unwrap().0.as_mut().unwrap();
    d.projects = vec![
        json!({"id":work,"title":"Medousa native", "state":"executing"}),
        json!({"id":"preview-other","title":"Documentation", "state":"ready"}),
    ];
    d.sessions = vec![
        shell(&format!("shell-{work}"), work),
        shell("shell-preview-other", "preview-other"),
    ];
    d.selected = Some(open.tab);
    d.status.clear();
    d.catalog_dirty = false;
    d.work = Some(work.into());
    d.redraw = true;
    tick(w);
    w.insert_resource(Preview {
        _opens: opens,
        _open: open,
        _second: second,
    });
    entity
}

#[test]
fn failed_attachment_retains_exact_bound_session_for_reconnect() {
    let (mut app, mut opens, updates) = fixture();
    sync(app.world_mut(), Some("one".into()));
    action(app.world_mut(), &HostIntent::Command(NEW));
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
    action(app.world_mut(), &HostIntent::Command(NEW));
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

#[test]
fn hidden_and_other_surface_input_is_dropped_but_output_keeps_draining() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    action(w, &HostIntent::Command(NEW));
    let mut first = opens.try_recv().unwrap();
    connected(&updates, &first);
    tick(w);
    let entity = pane(w, "one").entity;
    open_work(w, "two".into());
    action(w, &HostIntent::Command(NEW));
    let second = opens.try_recv().unwrap();
    connected(&updates, &second);
    tick(w);
    rndr_term::write_terminal(w, entity, b"hidden-command\r");
    fact(
        &updates,
        &first,
        Fact::Output(b"hidden-output\r\n".to_vec()),
    );
    tick(w);
    assert!(!w.has::<CapturesKeys>(entity));
    while let Ok(frame) = first.frames.try_recv() {
        assert!(!matches!(frame, Frame::Input(_)));
    }
    crate::surfaces::select(w, crate::surfaces::SurfaceId::Chat);
    tick(w);
    assert!(!w.has::<CapturesKeys>(pane(w, "two").entity));
    assert!(pane(w, "one").connected && pane(w, "two").connected);
}
#[test]
fn closed_tabs_and_wrong_work_updates_never_rebind_or_resurrect() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    action(w, &HostIntent::Command(NEW));
    let first = opens.try_recv().unwrap();
    connected(&updates, &first);
    tick(w);
    updates
        .try_send(Update {
            tab: first.tab,
            work: "other-owner-work".into(),
            generation: first.generation,
            fact: Fact::End("foreign".into()),
        })
        .unwrap();
    tick(w);
    assert!(pane(w, "one").connected);
    action(w, &HostIntent::Command(CLOSE));
    connected(&updates, &first);
    tick(w);
    assert!(can_rebind(w));
    assert!(
        w.resource::<ViewState>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .panes
            .is_empty()
    );
    action(w, &HostIntent::Command(NEW));
    let second = opens.try_recv().unwrap();
    assert_ne!(second.tab, first.tab);
    fact(&updates, &first, Fact::End("stale".into()));
    connected(&updates, &second);
    tick(w);
    assert!(pane(w, "one").connected);
}
#[test]
fn stale_discovery_cannot_cross_service_replacement() {
    let (mut app, _opens, updates) = fixture();
    let w = app.world_mut();
    let id = next();
    w.resource_mut::<ViewState>()
        .unwrap()
        .0
        .as_mut()
        .unwrap()
        .discovery = Some(id);
    disable(w);
    updates
        .try_send(Update {
            tab: 0,
            work: String::new(),
            generation: id,
            fact: Fact::Catalog(Ok((
                vec![json!({"id":"foreign"})],
                vec![shell("foreign", "other")],
            ))),
        })
        .ok();
    tick(w);
    let d = w.resource::<ViewState>().unwrap().0.as_ref().unwrap();
    assert!(d.projects.is_empty() && d.sessions.is_empty() && d.work.is_none());
    assert!(d.status.contains("Local authenticated"));
}

#[test]
fn attach_limit_allows_existing_tab_activation_only() {
    let (mut app, _opens, _updates) = fixture();
    let w = app.world_mut();
    let mut d = w.resource_mut::<ViewState>().unwrap().0.take().unwrap();
    for n in 0..32 {
        add_tab(w, &mut d, "one".into(), Some(format!("s-{n}")));
    }
    let selected = d.selected;
    let blocked = next();
    let existing = next();
    d.actions
        .insert(blocked, Action::Attach("one".into(), "new".into()));
    d.actions
        .insert(existing, Action::Attach("one".into(), "s-0".into()));
    w.resource_mut::<ViewState>().unwrap().0 = Some(d);
    action(w, &HostIntent::Command(blocked));
    let d = w.resource::<ViewState>().unwrap().0.as_ref().unwrap();
    assert_eq!(d.panes.len(), 32);
    assert_eq!(d.selected, selected);
    action(w, &HostIntent::Command(existing));
    let d = w.resource::<ViewState>().unwrap().0.as_ref().unwrap();
    assert_eq!(d.panes.len(), 32);
    assert_ne!(d.selected, selected);
}
#[test]
fn catalog_event_during_inflight_read_schedules_fresh_snapshot() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    let (discover, mut requests) = mpsc::channel(1);
    w.resource_mut::<Service>().unwrap().discover = Some(discover);
    activate(w);
    let first = requests.try_recv().unwrap();
    action(w, &HostIntent::Command(NEW));
    let open = opens.try_recv().unwrap();
    fact(
        &updates,
        &open,
        Fact::Bound {
            session: "new-shell".into(),
            cwd: "/work".into(),
        },
    );
    tick(w);
    assert!(requests.try_recv().is_err());
    updates
        .try_send(Update {
            tab: 0,
            work: String::new(),
            generation: first,
            fact: Fact::Catalog(Ok((vec![], vec![]))),
        })
        .unwrap();
    tick(w);
    let second = requests.try_recv().unwrap();
    assert_ne!(first, second);
    updates
        .try_send(Update {
            tab: 0,
            work: String::new(),
            generation: second,
            fact: Fact::Catalog(Ok((vec![], vec![shell("new-shell", "one")]))),
        })
        .unwrap();
    tick(w);
    assert_eq!(
        w.resource::<ViewState>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .sessions
            .len(),
        1
    );
}

#[test]
fn terminal_tabs_have_visible_independent_titlebar_and_survive_last_close() {
    fn visible(w: &World, mut e: Entity) -> bool {
        for _ in 0..128 {
            if w.has::<Hidden>(e) {
                return false;
            }
            match w.get::<Parent>(e) {
                Some(p) => e = p.entity,
                None => return true,
            }
        }
        false
    }
    let (mut app, mut opens, updates) = fixture();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    action(app.world_mut(), &HostIntent::Command(NEW));
    let open = opens.try_recv().unwrap();
    connected(&updates, &open);
    for _ in 0..5 {
        app.update();
    }
    let strip = app
        .world()
        .resource::<ViewState>()
        .unwrap()
        .0
        .as_ref()
        .unwrap()
        .strip
        .unwrap();
    assert!(
        visible(app.world(), strip.scroll),
        "Terminal tab bar must not live in the hidden Code host"
    );
    let rect = app.world().get::<Rect>(strip.scroll).unwrap();
    assert!(rect.w > 100. && rect.h >= 24., "{rect:?}");
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    app.update();
    assert!(!visible(app.world(), strip.scroll));
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Terminal);
    app.update();
    assert!(visible(app.world(), strip.scroll));
    action(app.world_mut(), &HostIntent::Command(CLOSE));
    app.update();
    assert!(!visible(app.world(), strip.scroll));
    action(app.world_mut(), &HostIntent::Command(NEW));
    let reopened = opens.try_recv().unwrap();
    connected(&updates, &reopened);
    for _ in 0..4 {
        app.update();
    }
    let strip = app
        .world()
        .resource::<ViewState>()
        .unwrap()
        .0
        .as_ref()
        .unwrap()
        .strip
        .unwrap();
    assert!(visible(app.world(), strip.scroll));
}

#[test]
fn general_shell_creation_has_no_undertaking_or_device_path() {
    let (route, body) = creation("");
    assert_eq!(route, "/v1/sessions/shell");
    assert_eq!(body, json!({"cols":80,"rows":24}));
    let (route, body) = creation("one");
    assert_eq!(route, "/v1/sessions/workspace-shell");
    assert_eq!(body["work_id"], "one");
    let (mut app, mut opens, _) = fixture();
    // Even when Code selected an undertaking, the top-level + is independent.
    action(app.world_mut(), &HostIntent::Command(GENERAL));
    let open = opens.try_recv().unwrap();
    assert!(open.work.is_empty());
    assert!(open.create && open.fresh && open.session.is_none());
}
#[test]
fn unbound_discovery_and_reconnect_cannot_attach_task_or_forge_shells() {
    let general = json!({"session_id":"general","work_id":null,"root_kind":"scripts","argv":[],"exited":false});
    assert!(active_human(&general));
    let mut task = general.clone();
    task["argv"] = json!(["cargo", "test"]);
    assert!(!active_human(&task));
    let mut dead = general.clone();
    dead["exited"] = json!(true);
    assert!(!active_human(&dead));
    let list = json!({"sessions":[general, task, shell("work", "one")]});
    assert_eq!(
        choose(&list, "", Some("general")).unwrap().unwrap()["session_id"],
        "general"
    );
    assert!(choose(&list, "one", Some("general")).is_err());
    assert!(choose(&list, "", Some("work")).is_err());
}
#[test]
fn undertaking_group_excludes_terminal_and_unknown_states() {
    for state in [
        "draft",
        "provisioning",
        "ready",
        "executing",
        "sealing",
        "awaiting_review",
        "applying_decision",
    ] {
        assert!(open_project(&json!({"id":"one","state":state})));
    }
    for state in ["accepted", "discarded", "failed", "unknown"] {
        assert!(!open_project(&json!({"id":"one","state":state})));
    }
    assert!(!open_project(&json!({"id":"one"})));
}
#[test]
fn shell_names_survive_binding_refresh_detach_and_reattach() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    action(w, &HostIntent::Command(GENERAL));
    let first = opens.try_recv().unwrap();
    assert_eq!(pane(w, "").name, "Shell 1");
    let raw = "session-raw-identifier-not-a-name";
    fact(
        &updates,
        &first,
        Fact::Bound {
            session: raw.into(),
            cwd: "/work".into(),
        },
    );
    fact(
        &updates,
        &first,
        Fact::Connected {
            session: raw.into(),
            cwd: "/work".into(),
        },
    );
    tick(w);
    assert_eq!(pane(w, "").name, "Shell 1");
    action(w, &HostIntent::Command(NEW));
    let _second = opens.try_recv().unwrap();
    assert_eq!(pane(w, "one").name, "Shell 2");
    let mut general = shell(raw, "");
    general["root_kind"] = json!("scripts");
    general["work_id"] = Value::Null;
    let d = w.resource_mut::<ViewState>().unwrap().0.as_mut().unwrap();
    d.sessions = vec![shell("other-session", "one"), general];
    d.redraw = true;
    tick(w);
    let mut d = w.resource_mut::<ViewState>().unwrap().0.take().unwrap();
    assert_eq!(d.names[raw], "Shell 1");
    assert_eq!(d.names["other-session"], "Shell 3");
    close(w, &mut d, first.tab);
    let attached = add_tab(w, &mut d, String::new(), Some(raw.into()));
    assert_eq!(d.panes[&attached].name, "Shell 1");
    d.sessions.reverse();
    render(w, &mut d);
    assert_eq!(d.names[raw], "Shell 1");
    assert_eq!(d.names["other-session"], "Shell 3");
    let strip = d.strip.unwrap();
    let tab = strip.tab(w, TabId(attached)).unwrap();
    assert!(
        w.get::<Semantics>(tab)
            .unwrap()
            .name
            .as_deref()
            .unwrap()
            .starts_with("Shell 1")
    );
    w.resource_mut::<ViewState>().unwrap().0 = Some(d);
}
#[test]
fn sidebar_only_offers_running_sessions_and_open_undertakings() {
    let (mut app, mut opens, _) = fixture();
    let w = app.world_mut();
    let d = w.resource_mut::<ViewState>().unwrap().0.as_mut().unwrap();
    d.projects = vec![
        json!({"id":"one","title":"Open","state":"ready"}),
        json!({"id":"old","title":"Old","state":"accepted"}),
    ];
    let mut dead = shell("dead", "one");
    dead["exited"] = json!(true);
    d.sessions = vec![shell("live", "old"), dead];
    d.redraw = true;
    tick(w);
    let d = w.resource::<ViewState>().unwrap().0.as_ref().unwrap();
    // The sidebar and the landing offer the same running shells; count shells.
    let offered: std::collections::HashSet<_> = d
        .actions
        .values()
        .filter_map(|a| match a {
            Action::Attach(work, shell) => Some((work.clone(), shell.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(offered.len(), 1);
    let id = *d
        .actions
        .iter()
        .find(|(_, a)| matches!(a, Action::NewWork(work) if work == "one"))
        .unwrap()
        .0;
    assert!(
        !d.actions
            .values()
            .any(|a| matches!(a, Action::NewWork(work) if work == "old"))
    );
    // A live shell from closed work remains attachable, but cannot create stale work.
    assert!(
        d.actions
            .values()
            .any(|a| matches!(a, Action::Attach(work, _) if work == "old"))
    );
    assert!(
        children_of(w, d.header)
            .iter()
            .all(|child| w.has::<Label>(*child)),
        "no action toolbar above the terminal"
    );
    action(w, &HostIntent::Command(id));
    assert_eq!(opens.try_recv().unwrap().work, "one");
}

#[test]
fn workspace_reparents_live_panes_and_routes_only_focused_input() {
    let (mut app, mut opens, updates) = fixture();
    let w = app.world_mut();
    let first_id = workspace_new(w).unwrap();
    let mut first = opens.try_recv().unwrap();
    assert!(first.work.is_empty() && first.create);
    connected(&updates, &first);
    tick(w);
    let second_id = workspace_attach(w, "two".into(), "known".into()).unwrap();
    let mut second = opens.try_recv().unwrap();
    assert!(!second.create && second.session.as_deref() == Some("known"));
    fact(
        &updates,
        &second,
        Fact::Connected {
            session: "known".into(),
            cwd: "/two".into(),
        },
    );
    tick(w);
    assert_eq!(
        workspace_attach(w, "two".into(), "known".into()),
        Some(second_id)
    );
    assert!(opens.try_recv().is_err());
    assert_eq!(workspace_selected(w), Some(second_id));
    let panes = workspace_panes(w);
    assert_eq!(
        panes.iter().map(|p| p.0).collect::<Vec<_>>(),
        [first_id, second_id]
    );
    assert_eq!(panes[0].1, "Shell 1");
    assert_eq!(panes[1].4.as_deref(), Some("known"));
    let (one, two) = (panes[0].2, panes[1].2);
    let group = w.spawn((Layout::row(),));
    set_parent(w, one, group);
    set_parent(w, two, group);
    crate::surfaces::select(w, crate::surfaces::SurfaceId::Chat);
    workspace_show(w, &[first_id, second_id, u64::MAX]);
    tick(w);
    for e in [one, two] {
        assert!(!w.has::<Hidden>(e));
        assert!(w.has::<CapturesKeys>(e));
    }
    while first.frames.try_recv().is_ok() {}
    while second.frames.try_recv().is_ok() {}
    w.resource_mut::<Focus>().unwrap().entity = Some(one);
    rndr_term::write_terminal(w, one, b"focused\r");
    rndr_term::write_terminal(w, two, b"not-focused\r");
    tick(w);
    assert!(matches!(first.frames.try_recv(), Ok(Frame::Input(b)) if b == b"focused\r"));
    assert!(second.frames.try_recv().is_err());
    workspace_show(w, &[second_id]);
    tick(w);
    assert!(w.has::<Hidden>(one));
    assert!(!w.has::<CapturesKeys>(one));
    assert_ne!(w.resource::<Focus>().unwrap().entity, Some(one));
    fact(
        &updates,
        &first,
        Fact::Output(b"retained output\r\n".to_vec()),
    );
    tick(w);
    assert!(first.cancelled.try_recv().is_err());
    workspace_restore(w);
    crate::surfaces::select(w, crate::surfaces::SurfaceId::Terminal);
    tick(w);
    assert!(w.has::<Hidden>(one));
    assert!(!w.has::<Hidden>(two));
    assert_eq!(workspace_panes(w)[0].2, one);
    assert!(opens.try_recv().is_err());
    assert!(second.cancelled.try_recv().is_err());
    assert!(!can_rebind(w));
}

#[test]
fn workspace_api_is_safe_without_mounted_terminal_or_service() {
    let mut w = World::new();
    assert!(workspace_panes(&w).is_empty());
    assert!(workspace_selected(&w).is_none());
    assert!(workspace_owner(&w).is_none());
    assert!(workspace_new(&mut w).is_none());
    assert!(workspace_attach(&mut w, "one".into(), "known".into()).is_none());
    workspace_show(&mut w, &[1]);
    workspace_restore(&mut w);
}
