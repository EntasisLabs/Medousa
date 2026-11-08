use super::*;
pub(super) fn ready() -> (App, mpsc::UnboundedReceiver<Request>) {
    let mut app = app();
    app.update();
    let (tx, rx) = mpsc::unbounded_channel();
    let m = app.world_mut().resource_mut::<Model>().unwrap();
    m.commands = Some(tx);
    m.chat = Chat::history("s".into(), false);
    m.models = vec![("mock".into(), "test-only".into())];
    m.selection = Some(0);
    app.update();
    (app, rx)
}
pub(super) fn reply(app: &mut App, generation: u64, update: Update) {
    rndr_core::send(app.world_mut(), Reply { generation, update });
    app.update();
}
#[test]
fn failed_admission_reload_preserves_visible_recovery_and_newer_draft() {
    for failure in ["401 unauthorized", "network failure before persistence"] {
        for newer in ["", "newer draft\nnever overwrite 👋"] {
            let (mut app, mut rx) = ready();
            let ui = *app.world().resource::<Ui>().unwrap();
            reset_prompt(app.world_mut(), ui.composer, "unique original prompt");
            app.update();
            try_activate(app.world_mut(), ui.send);
            app.update();
            assert!(matches!(rx.try_recv().unwrap().command, Command::Send(_)));
            reset_prompt(app.world_mut(), ui.composer, newer);
            reply(&mut app, 1, Update::Failure(failure.into()));
            assert!(!app.world().get::<Button>(ui.send).unwrap().enabled);
            let reload = app
                .world()
                .entities_with::<Button>()
                .into_iter()
                .find(|e| app.world().get::<Button>(*e).unwrap().action == "reload")
                .unwrap();
            try_activate(app.world_mut(), reload);
            app.update();
            let request = rx.try_recv().unwrap();
            assert!(matches!(request.command, Command::Open(_)));
            reply(
                &mut app,
                request.generation,
                Update::History("s".into(), Vec::new(), None),
            );
            assert_eq!(app.world().get::<Label>(ui.composer).unwrap().text, newer);
            let visible = app
                .world()
                .entities_with::<MarkdownView>()
                .into_iter()
                .map(|e| app.world().get::<MarkdownView>(e).unwrap().source())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(visible.contains("Recoverable prompt"));
            assert!(visible.contains("unique original prompt"));
            assert!(rx.try_recv().is_err(), "reload must never resubmit");
            assert_eq!(
                app.world().get::<Button>(ui.recover).unwrap().enabled,
                newer.is_empty()
            );
            try_activate(app.world_mut(), ui.recover);
            app.update();
            assert_eq!(
                app.world().get::<Label>(ui.composer).unwrap().text,
                if newer.is_empty() {
                    "unique original prompt"
                } else {
                    newer
                }
            );
            assert!(rx.try_recv().is_err(), "restoring must not send");
        }
    }
}
#[test]
fn terminal_waits_for_history_and_cancel_does_not_fake_settlement() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    assert!(!app.world().get::<Button>(ui.cancel).unwrap().enabled);
    reset_prompt(app.world_mut(), ui.composer, "hi");
    app.update();
    try_activate(app.world_mut(), ui.send);
    app.update();
    rx.try_recv().unwrap();
    reply(&mut app, 1, Update::Admitted("t".into()));
    try_activate(app.world_mut(), ui.cancel);
    app.update();
    assert!(matches!(rx.try_recv().unwrap().command, Command::Cancel(id) if id == "s"));
    reply(&mut app, 1, Update::Notice("cancel requested".into()));
    assert!(app.world().resource::<Model>().unwrap().chat.busy);
    let terminal = serde_json::from_value(serde_json::json!({"schema_version":3,"turn_id":"t","seq":1,"emitted_at_utc":"2026-11-07T00:00:00Z","event":{"type":"turn_completed","outcome":"cancelled","aggregate_text":""}})).unwrap();
    reply(&mut app, 1, Update::Stream(terminal));
    reset_prompt(app.world_mut(), ui.composer, "new draft");
    app.update();
    assert!(app.world().resource::<Model>().unwrap().chat.uncertain);
    assert!(!app.world().get::<Button>(ui.send).unwrap().enabled);
    reply(
        &mut app,
        1,
        Update::SettledHistory("s".into(), vec![("user".into(), "hi".into())]),
    );
    assert!(app.world().get::<Button>(ui.send).unwrap().enabled);
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .chat
            .status
            .contains("Cancelled")
    );
    assert_eq!(
        app.world().get::<Label>(ui.composer).unwrap().text,
        "new draft"
    );
}
#[test]
fn confirmed_admission_retires_recovery_without_touching_newer_draft() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    reset_prompt(app.world_mut(), ui.composer, "admitted original");
    app.update();
    try_activate(app.world_mut(), ui.send);
    app.update();
    rx.try_recv().unwrap();
    reset_prompt(app.world_mut(), ui.composer, "next draft");
    reply(&mut app, 1, Update::Admitted("t".into()));
    assert!(app.world().resource::<Model>().unwrap().recovery.is_empty());
    reply(
        &mut app,
        1,
        Update::History(
            "s".into(),
            vec![("user".into(), "admitted original".into())],
            None,
        ),
    );
    assert_eq!(
        app.world().get::<Label>(ui.composer).unwrap().text,
        "next draft"
    );
    assert!(
        app.world()
            .entities_with::<MarkdownView>()
            .into_iter()
            .all(|e| !app
                .world()
                .get::<MarkdownView>(e)
                .unwrap()
                .source()
                .contains("Recoverable prompt"))
    );
    assert!(rx.try_recv().is_err());
}
#[test]
fn offline_controls_are_honest_and_resize_preserves_draft() {
    let mut app = app();
    app.update();
    let ui = *app.world().resource::<Ui>().unwrap();
    assert!(!app.world().get::<Button>(ui.send).unwrap().enabled);
    assert!(
        app.world().get::<Button>(ui.model).unwrap().enabled,
        "offline menu explains missing catalog"
    );
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
        Update::History("wrong".into(), vec![("user".into(), "wrong".into())], None),
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
fn semantic_theme_and_surface_updates_preserve_editor_popup_and_messages() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    reset_prompt(
        app.world_mut(),
        ui.composer,
        "private draft 👋\nnever submit on restyle",
    );
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .history_messages = vec![
        ("user".into(), "literal **prompt**".into()),
        (
            "assistant".into(),
            "The **same** source.\n\n```rust\nlet x = 1;\n```".into(),
        ),
    ];
    for _ in 0..5 {
        app.update();
    }
    let views = app.world().entities_with::<MarkdownView>();
    menus::open(app.world_mut(), menus::Kind::Model, ui.model);
    app.update();
    let popup = app
        .world()
        .resource::<menus::Menus>()
        .unwrap()
        .root
        .unwrap();
    let focus = app.world().resource::<Focus>().unwrap().entity;
    let original_count = app.world().entities_with::<Layout>().len();
    for (appearance, contrast) in [
        (Appearance::Light, Contrast::Standard),
        (Appearance::Dark, Contrast::High),
        (Appearance::Dark, Contrast::Standard),
    ] {
        let theme = UiTheme {
            appearance,
            contrast,
            ..Default::default()
        };
        presentation::apply_theme(app.world_mut(), theme.clone());
        // Same retained surface/key, not a replacement editor subtree.
        let surface = rndr_ui::Surface::Raised
            .mount(app.world_mut(), ui.center, rndr_ui::UiId(1))
            .unwrap();
        surface.validate(app.world()).unwrap();
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world().get::<Label>(ui.composer).unwrap().text,
            "private draft 👋\nnever submit on restyle"
        );
        assert_eq!(app.world().resource::<Focus>().unwrap().entity, focus);
        assert!(!app.world().has::<Hidden>(popup));
        assert_eq!(app.world().entities_with::<MarkdownView>(), views);
        assert_eq!(app.world().entities_with::<Layout>().len(), original_count);
        assert_eq!(
            *app.world().get::<Color>(surface.root).unwrap(),
            theme.color(Token::Raised)
        );
        assert_eq!(
            app.world().get::<Border>(popup).unwrap().color,
            theme.color(Token::SubtleStroke)
        );
        assert_eq!(
            app.world().resource::<MarkdownTheme>().unwrap().surface,
            Color::rgba(0., 0., 0., 0.)
        );
        assert_eq!(
            app.world().resource::<Theme>().unwrap().text,
            theme.color(Token::Text)
        );
        assert!(
            rx.try_recv().is_err(),
            "restyling cannot dispatch inference or settings"
        );
    }
    assert!(conversation::copy_message(
        app.world_mut(),
        ui.conversation,
        "copy-message:1"
    ));
    assert_eq!(
        app.world().resource::<Clipboard>().unwrap().text,
        "The **same** source.\n\n```rust\nlet x = 1;\n```"
    );
}

#[test]
fn semantic_appearance_actions_use_one_fifo_and_never_send() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    reset_prompt(app.world_mut(), ui.composer, "do not send me");
    menus::open(app.world_mut(), menus::Kind::Behavior, ui.behavior);
    app.update();
    let appearance = app
        .world()
        .entities_with::<Button>()
        .into_iter()
        .find(|e| app.world().get::<Button>(*e).unwrap().action == "appearance:open")
        .unwrap();
    try_activate(app.world_mut(), appearance);
    app.update();
    let control = |app: &App, id| {
        app.world()
            .entities_with::<Button>()
            .into_iter()
            .find(|e| rndr_ui::identity(app.world(), *e) == Some(rndr_ui::UiId(id)))
            .unwrap()
    };
    let light = control(&app, presentation::LIGHT);
    let contrast = control(&app, presentation::CONTRAST);
    assert_eq!(
        try_activate(app.world_mut(), light),
        ActivationResult::Accepted
    );
    assert_eq!(
        try_activate(app.world_mut(), contrast),
        ActivationResult::Accepted
    );
    app.update();
    assert_eq!(
        presentation::theme(app.world()).appearance,
        Appearance::Light
    );
    assert_eq!(presentation::theme(app.world()).contrast, Contrast::High);
    // Multiplicity is meaningful: two toggles must not be deduplicated.
    assert_eq!(
        try_activate(app.world_mut(), contrast),
        ActivationResult::Accepted
    );
    assert_eq!(
        try_activate(app.world_mut(), contrast),
        ActivationResult::Accepted
    );
    app.update();
    assert_eq!(presentation::theme(app.world()).contrast, Contrast::High);
    assert_eq!(
        app.world().get::<Label>(ui.composer).unwrap().text,
        "do not send me"
    );
    assert!(rx.try_recv().is_err());
    // A queued request from a replaced popup cannot act on its new generation.
    assert_eq!(
        try_activate(app.world_mut(), light),
        ActivationResult::Accepted
    );
    menus::open(app.world_mut(), menus::Kind::Model, ui.model);
    let before = presentation::theme(app.world());
    app.update();
    assert_eq!(presentation::theme(app.world()), before);
    assert!(rx.try_recv().is_err());
}

#[test]
fn popup_content_follows_anchor_width_without_horizontal_overflow() {
    let mut app = app();
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    for width in [520., 1120.] {
        app.world_mut().insert_resource(Viewport::new(width, 820.));
        app.update();
        let ui = *app.world().resource::<Ui>().unwrap();
        for kind in [
            menus::Kind::Appearance,
            menus::Kind::Behavior,
            menus::Kind::Depth,
            menus::Kind::Runtime,
            menus::Kind::Model,
        ] {
            menus::open(app.world_mut(), kind, ui.behavior);
            for _ in 0..4 {
                app.update();
            }
            let root = app
                .world()
                .resource::<menus::Menus>()
                .unwrap()
                .root
                .unwrap();
            let rect = app.world().get::<Rect>(root).unwrap();
            let scroll = app.world().get::<Scroll>(root).unwrap();
            assert!(
                scroll.content[0] <= rect.w + 1.,
                "popup content={} viewport={}",
                scroll.content[0],
                rect.w
            );
            assert!(rect.x >= 0. && rect.x + rect.w <= width + 1.);
            menus::close(app.world_mut());
        }
    }
}

#[test]
fn semantic_reading_measure_resizes_without_losing_draft() {
    let mut app = app();
    app.update();
    let ui = *app.world().resource::<Ui>().unwrap();
    reset_prompt(app.world_mut(), ui.composer, "retained draft");
    for width in [1120., 520., 480., 1600.] {
        app.world_mut().insert_resource(Viewport::new(width, 820.));
        for _ in 0..3 {
            app.update();
        }
        let expected = presentation::reading_width(width - 48., &presentation::theme(app.world()));
        assert_eq!(
            app.world().get::<Layout>(ui.center).unwrap().width,
            Size::Px(expected)
        );
        assert!(expected <= width - 48.);
        assert_eq!(
            app.world().get::<Label>(ui.composer).unwrap().text,
            "retained draft"
        );
    }
}

#[test]
fn permissions_are_not_actionable_ui_approvals() {
    let (mut app, _) = ready();
    let mut chat = Chat::history("s".into(), false);
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
