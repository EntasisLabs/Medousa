use super::*;
fn ready() -> (App, mpsc::UnboundedReceiver<Request>) {
    let mut app = app();
    app.update();
    let (tx, rx) = mpsc::unbounded_channel();
    let m = app.world_mut().resource_mut::<Model>().unwrap();
    m.commands = Some(tx);
    m.chat = Chat::history("s".into(), String::new(), false);
    m.models = vec![("mock".into(), "test-only".into())];
    m.selection = Some(0);
    app.update();
    (app, rx)
}
fn reply(app: &mut App, generation: u64, update: Update) {
    rndr_core::send(app.world_mut(), Reply { generation, update });
    app.update();
}
#[test]
fn offline_controls_are_honest_and_resize_preserves_draft() {
    let mut app = app();
    app.update();
    let ui = *app.world().resource::<Ui>().unwrap();
    assert!(!app.world().get::<Button>(ui.send).unwrap().enabled);
    assert!(!app.world().get::<Button>(ui.model).unwrap().enabled);
    reset_prompt(app.world_mut(), ui.composer, "draft 👋\nnext line");
    app.world_mut().insert_resource(Viewport::new(520., 760.));
    app.update();
    assert_eq!(
        app.world().get::<Label>(ui.composer).unwrap().text,
        "draft 👋\nnext line"
    );
    assert_eq!(
        app.world().get::<Layout>(ui.center).unwrap().width,
        Size::Px(472.)
    );
    assert!(!app.world().has::<FieldTransactions>(ui.composer));
}
#[test]
fn send_is_once_clears_draft_and_blocks_overlap() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    reset_prompt(app.world_mut(), ui.composer, "hello");
    app.update();
    assert!(app.world().get::<Button>(ui.send).unwrap().enabled);
    try_activate(app.world_mut(), ui.send);
    app.update();
    let request = rx.try_recv().unwrap();
    assert_eq!(request.generation, 1);
    let Command::Send(request) = request.command else {
        panic!("not send")
    };
    assert_eq!(request.session_id, "s");
    assert_eq!(request.prompt, "hello");
    assert_eq!(app.world().get::<Label>(ui.composer).unwrap().text, "");
    assert!(app.world().resource::<Model>().unwrap().chat.busy);
    assert!(!app.world().get::<Button>(ui.send).unwrap().enabled);
    try_activate(app.world_mut(), ui.send);
    app.update();
    assert!(rx.try_recv().is_err());
    reply(&mut app, 1, Update::Failure("network lost".into()));
    reset_prompt(app.world_mut(), ui.composer, "do not resubmit");
    app.update();
    assert!(!app.world().get::<Button>(ui.send).unwrap().enabled);
    assert!(
        app.world()
            .get::<Label>(ui.composer)
            .unwrap()
            .text
            .contains("do not resubmit")
    );
}
#[test]
fn disconnected_worker_does_not_leave_spinner_or_erase_draft() {
    let (mut app, rx) = ready();
    drop(rx);
    let ui = *app.world().resource::<Ui>().unwrap();
    reset_prompt(app.world_mut(), ui.composer, "keep me");
    app.update();
    try_activate(app.world_mut(), ui.send);
    app.update();
    let m = app.world().resource::<Model>().unwrap();
    assert!(!m.chat.busy);
    assert!(m.commands.is_none());
    assert_eq!(
        app.world().get::<Label>(ui.composer).unwrap().text,
        "keep me"
    );
}
#[test]
fn stale_reply_and_startup_auth_failure() {
    let (mut app, _) = ready();
    app.world_mut().resource_mut::<Model>().unwrap().generation = 2;
    reply(
        &mut app,
        1,
        Update::History("wrong".into(), "wrong".into(), None),
    );
    reply(&mut app, 1, Update::Failure("old stream".into()));
    assert_eq!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .chat
            .session
            .as_deref(),
        Some("s")
    );
    assert!(!app.world().resource::<Model>().unwrap().chat.uncertain);
    reply(&mut app, 0, Update::Unavailable("auth unavailable".into()));
    let m = app.world().resource::<Model>().unwrap();
    assert!(m.commands.is_none());
    assert!(!m.loading);
}
#[test]
fn shift_enter_multiline_survives_clear_and_plain_enter_sends() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    reset_prompt(app.world_mut(), ui.composer, "line");
    request_focus(app.world_mut(), ui.composer);
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![KeyEvent::named(
        NamedKey::Enter,
        Modifiers {
            shift: true,
            ..Modifiers::new()
        },
    )];
    app.update();
    assert!(
        app.world()
            .get::<Label>(ui.composer)
            .unwrap()
            .text
            .contains('\n')
    );
    assert!(rx.try_recv().is_err());
    // Native runners enable the input method for a focused field even when
    // there is no active composition. That alone must not suppress Enter.
    app.world_mut().resource_mut::<Input>().unwrap().ime_enabled = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())];
    app.update();
    assert!(matches!(rx.try_recv().unwrap().command, Command::Send(_)));
    assert_eq!(app.world().get::<Label>(ui.composer).unwrap().text, "");
}
#[test]
fn ambiguous_queued_submit_is_not_sent_and_preserves_newer_edits() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    reset_prompt(app.world_mut(), ui.composer, "hi");
    request_focus(app.world_mut(), ui.composer);
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![
        KeyEvent::named(NamedKey::Enter, Modifiers::new()),
        KeyEvent::character('!', Modifiers::new()),
    ];
    app.update();
    assert!(rx.try_recv().is_err());
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .chat
            .status
            .contains("nothing sent")
    );
    assert_eq!(app.world().get::<Label>(ui.composer).unwrap().text, "hi!");
}
#[test]
fn tab_leaves_composer_using_canonical_focus_policy() {
    let (mut app, _) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    request_focus(app.world_mut(), ui.composer);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Tab, Modifiers::new())];
    app.update();
    assert_ne!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(ui.composer)
    );
}
#[cfg(feature = "observe")]
#[test]
fn offline_observer_denies_any_user_draft() {
    use rndr_render::observe::ObservationContent;
    let mut app = app();
    app.update();
    let ui = *app.world().resource::<Ui>().unwrap();
    app.world_mut().insert_resource(FixtureObservation);
    app.update();
    assert!(matches!(
        app.world().resource::<ObservationContent>(),
        Some(ObservationContent::Public)
    ));
    reset_prompt(app.world_mut(), ui.composer, "private draft");
    app.update();
    assert!(matches!(
        app.world().resource::<ObservationContent>(),
        Some(ObservationContent::Deny)
    ));
}
#[test]
fn permissions_are_not_actionable_ui_approvals() {
    let (mut app, _) = ready();
    let mut chat = Chat::history("s".into(), String::new(), false);
    chat.begin("hi").unwrap();
    chat.admitted("t".into());
    let e = serde_json::from_value(serde_json::json!({"schema_version":3,"turn_id":"t","seq":1,"emitted_at_utc":"2026-11-07T00:00:00Z","event":{"type":"permission_request","request_id":"r","message":"Allow tool?"}})).unwrap();
    chat.apply(e);
    app.world_mut().resource_mut::<Model>().unwrap().chat = chat;
    app.update();
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .chat
            .status
            .contains("existing Medousa app")
    );
    assert!(app.world().entities_with::<Button>().iter().all(|e| {
        !app.world()
            .get::<Button>(*e)
            .unwrap()
            .action
            .contains("approve")
    }));
}
