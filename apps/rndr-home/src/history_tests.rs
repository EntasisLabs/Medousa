//! Synthetic interaction evidence; no native keyboard, clipboard, daemon or IME proof.
use super::*;
use crate::ui_tests::ready_unverified as ready;

fn settle(app: &mut App) {
    for _ in 0..10 {
        app.update();
    }
}

#[test]
fn latest_is_explicit_preserves_draft_and_resumes_follow_at_all_widths() {
    for width in [480., 520., 1120.] {
        let (mut app, mut rx) = ready();
        app.world_mut().insert_resource(Viewport::new(width, 820.));
        let ui = *app.world().resource::<Ui>().unwrap();
        reset_prompt(app.world_mut(), ui.composer, "unsent 👋 draft");
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .history_messages = (0..1000)
            .map(|i| {
                (
                    "assistant".into(),
                    format!("Message {i}\n\n{}", "Paragraph.\n\n".repeat(8)),
                )
            })
            .collect();
        settle(&mut app);
        assert!(app.world().has::<Hidden>(ui.latest));
        app.world_mut()
            .get_mut::<Scroll>(ui.conversation)
            .unwrap()
            .offset[1] = 0.;
        settle(&mut app);
        assert!(!app.world().has::<Hidden>(ui.latest));
        let r = *app.world().get::<Rect>(ui.latest).unwrap();
        assert!(r.w > 100. && r.x >= 0. && r.x + r.w <= width + 1.);
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .history_messages[999]
            .1
            .push_str(" new tail");
        settle(&mut app);
        assert!(app.world().get::<Scroll>(ui.conversation).unwrap().offset[1].abs() < 2.);
        assert!(app.world().entities_with::<MarkdownView>().len() < 20);
        // Canonical pointer activation on the actual visible control.
        let p = [r.x + r.w / 2., r.y + r.h / 2.];
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: p,
            pressed: true,
            released: true,
            press_at: Some(p),
            ..Default::default()
        };
        settle(&mut app);
        let scroll = app.world().get::<Scroll>(ui.conversation).unwrap();
        let rect = app.world().get::<Rect>(ui.conversation).unwrap();
        assert!((scroll.offset[1] + (scroll.content[1] - rect.h).max(0.)).abs() < 2.);
        assert!(app.world().has::<Hidden>(ui.latest));
        assert!(app.world().entities_with::<MarkdownView>().len() < 20);
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .history_messages[999]
            .1
            .push_str(" more streaming");
        settle(&mut app);
        assert!(app.world().has::<Hidden>(ui.latest));
        assert_eq!(
            app.world().get::<Label>(ui.composer).unwrap().text,
            "unsent 👋 draft"
        );
        assert_eq!(
            app.world()
                .resource::<Model>()
                .unwrap()
                .chat
                .session
                .as_deref(),
            Some("s")
        );
        assert_eq!(app.world().resource::<Model>().unwrap().generation, 0);
        assert!(
            rx.try_recv().is_err(),
            "viewport interaction has no runtime authority"
        );
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .history_messages
            .clear();
        settle(&mut app);
        assert!(app.world().has::<Hidden>(ui.latest));
        assert!(!conversation::jump_to_latest(
            app.world_mut(),
            ui.conversation
        ));
    }
}

#[test]
fn latest_keyboard_activation_is_viewport_only_even_busy() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .history_messages = (0..40)
        .map(|i| {
            (
                "assistant".into(),
                format!("{i}\n\n{}", "History. ".repeat(200)),
            )
        })
        .collect();
    app.world_mut().resource_mut::<Model>().unwrap().chat.busy = true;
    settle(&mut app);
    app.world_mut()
        .get_mut::<Scroll>(ui.conversation)
        .unwrap()
        .offset[1] = 0.;
    settle(&mut app);
    assert!(request_focus(app.world_mut(), ui.latest));
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())];
    app.update();
    assert!(
        app.world().has::<Hidden>(ui.latest),
        "activation: focus={:?} scroll={:?} status={}",
        app.world().resource::<Focus>().unwrap().entity,
        app.world().get::<Scroll>(ui.conversation),
        app.world().resource::<Model>().unwrap().chat.status
    );
    settle(&mut app);
    let scroll = app.world().get::<Scroll>(ui.conversation).unwrap();
    let rect = app.world().get::<Rect>(ui.conversation).unwrap();
    assert!((scroll.offset[1] + (scroll.content[1] - rect.h).max(0.)).abs() < 2.);
    let focus = app.world().resource::<Focus>().unwrap().entity.unwrap();
    assert_eq!(
        app.world().get::<Scrollbar>(focus).unwrap().target,
        ui.conversation
    );
    // Deliberate keyboard scrolling cancels follow despite Input-time layout
    // refreshes; later streaming must not steal this earlier reading position.
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Home, Modifiers::new())];
    settle(&mut app);
    assert!(app.world().get::<Scroll>(ui.conversation).unwrap().offset[1].abs() < 2.);
    assert!(!app.world().has::<Hidden>(ui.latest));
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .history_messages[39]
        .1
        .push_str(" new tail");
    settle(&mut app);
    assert!(app.world().get::<Scroll>(ui.conversation).unwrap().offset[1].abs() < 2.);
    // Return once more through semantic activation to check reentrancy.
    assert_eq!(
        try_activate(app.world_mut(), ui.latest),
        ActivationResult::Accepted
    );
    settle(&mut app);
    assert!(
        app.world().has::<Hidden>(ui.latest),
        "settled: scroll={:?} rect={:?}",
        app.world().get::<Scroll>(ui.conversation),
        app.world().get::<Rect>(ui.conversation)
    );
    assert!(app.world().resource::<Model>().unwrap().chat.busy);
    assert!(rx.try_recv().is_err());
}

#[test]
fn unknown_native_input_blocks_buttons_forged_actions_and_submissions() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    reset_prompt(app.world_mut(), ui.composer, "preserve this draft");
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .recovery
        .push(("s".into(), "recover me".into()));
    settle(&mut app);
    for action in ["send", "recover", "new", "recent", "reload"] {
        let e = app
            .world()
            .entities_with::<Button>()
            .into_iter()
            .find(|e| app.world().get::<Button>(*e).unwrap().action == action)
            .unwrap();
        assert!(!app.world().get::<Button>(e).unwrap().enabled);
        assert_eq!(try_activate(app.world_mut(), e), ActivationResult::Rejected);
        // Deliberately bypass visual disabling to exercise the controller fence.
        app.world_mut().get_mut::<Button>(e).unwrap().enabled = true;
        try_activate(app.world_mut(), e);
        app.update();
    }
    rndr_core::send(
        app.world_mut(),
        Submitted {
            entity: ui.composer,
            text: "preserve this draft".into(),
        },
    );
    app.update();
    assert_eq!(
        app.world().get::<Label>(ui.composer).unwrap().text,
        "preserve this draft"
    );
    assert!(rx.try_recv().is_err());
    reset_prompt(app.world_mut(), ui.composer, "");
    settle(&mut app);
    // Empty Label and quiet frames are still not proof that Restore is safe.
    app.world_mut()
        .get_mut::<Button>(ui.recover)
        .unwrap()
        .enabled = true;
    try_activate(app.world_mut(), ui.recover);
    app.update();
    assert_eq!(app.world().get::<Label>(ui.composer).unwrap().text, "");
    let m = app.world().resource::<Model>().unwrap();
    assert_eq!(m.recovery, vec![("s".into(), "recover me".into())]);
    assert_eq!(m.generation, 0);
    assert!(!m.loading && !m.chat.busy);
    assert!(rx.try_recv().is_err());
}
