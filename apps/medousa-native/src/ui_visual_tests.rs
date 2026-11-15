//! Synthetic integrated-shell acceptance; never connects to the daemon.
use super::*;
use std::sync::Arc;

fn fixture() -> (App, tokio::sync::mpsc::UnboundedReceiver<Request>) {
    let mut app = app();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let m = app.world_mut().resource_mut::<Model>().unwrap();
    m.commands = Some(tx);
    m.chat.hydrate("s".into(), "a".into(), vec![
        serde_json::json!({"entry_id":"u", "entry_seq":1, "role":"user", "content":"Can we make the native app feel more cohesive without changing how it works?"}),
        serde_json::json!({"entry_id":"a", "entry_seq":2, "role":"assistant", "content":"Absolutely. The strongest improvements are in **hierarchy, rhythm, and consistency**—not a different layout.\n\n### One visual language\n\nThe sidebar stays quiet, the conversation gets a clear reading lane, and the composer becomes one cohesive control.\n\n- Preserve native editing, selection, and keyboard navigation.\n- Keep model, runtime, mode, and behavior controls available.\n- Use opaque surfaces instead of expensive decorative effects.\n\nThe performance checkpoint is saved as `4443ac77`."}),
    ], None).unwrap();
    m.models = vec![("openai-codex".into(), "gpt-6.1-sol".into())];
    m.selected = Some(0);
    m.modes = vec![("coder".into(), "Coder".into(), true)];
    m.mode = Some("coder".into());
    m.sessions = vec![
        ("s".into(), "Native app polish".into()),
        ("other".into(), "RNDR performance".into()),
        ("notes".into(), "Workspace notes and Code".into()),
    ];
    for _ in 0..8 {
        app.update();
    }
    (app, rx)
}

#[test]
fn retained_composer_follows_live_appearance_without_pinning_mount_color() {
    let (mut app, _rx) = fixture();
    let ui = *app.world().resource::<Ui>().unwrap();
    let document = app.world().resource::<Model>().unwrap().document;
    for dark in [true, false, true] {
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(dark),
                ..Default::default()
            },
        );
        app.update();
        app.update();
        let w = app.world();
        let expected = visual::color(w, visual::raised());
        assert_eq!(w.get::<Color>(ui.composer.editor), Some(&expected));
        // `compose` is the unpainted column wrapper, not the Raised surface.
        assert!(
            children_of(w, ui.compose)
                .iter()
                .any(|e| w.get::<Color>(*e) == Some(&expected))
        );
        assert_eq!(w.resource::<Model>().unwrap().document, document);
        assert_eq!(
            w.resource::<Ui>().unwrap().composer.editor,
            ui.composer.editor
        );
    }
}

#[test]
fn cohesive_composer_geometry_and_header_accessibility() {
    let (mut app, _rx) = fixture();
    for width in [1200., 760., 1600.] {
        app.world_mut().insert_resource(Viewport::new(width, 850.));
        for _ in 0..8 {
            app.update();
        }
        let w = app.world();
        let ui = *w.resource::<Ui>().unwrap();
        let shell = w.get::<Rect>(ui.compose).unwrap();
        let editor = w.get::<Rect>(ui.composer.editor).unwrap();
        let mode = w.get::<Rect>(ui.mode).unwrap();
        assert!(mode.y >= editor.y + editor.h);
        assert!(mode.x >= shell.x && mode.x + mode.w <= shell.x + shell.w + 1.);
        assert!(
            mode.y + mode.h < shell.y + shell.h,
            "footer must stay inside frame"
        );
        assert!(shell.x + shell.w <= width);
        for name in ["Toggle conversation sidebar", "Toggle conversation details"] {
            assert!(
                w.query::<Semantics>()
                    .iter()
                    .any(|(_, semantics)| semantics.name.as_deref() == Some(name))
            );
        }
    }
}

fn behavior_trigger(w: &World) -> Entity {
    let popup = w.resource::<Ui>().unwrap().behavior;
    w.get::<AnchoredPopup>(popup).unwrap().anchor
}
fn settle(app: &mut App) {
    for _ in 0..4 {
        app.update();
    }
}
fn escape(app: &mut App) {
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())];
    app.update();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .clear();
    settle(app);
}

#[test]
fn behavior_overlay_is_anchored_stable_and_escape_returns_focus() {
    let (mut app, _rx) = fixture();
    let ui = *app.world().resource::<Ui>().unwrap();
    let trigger = behavior_trigger(app.world());
    for width in [1200., 760.] {
        app.world_mut().insert_resource(Viewport::new(width, 650.));
        settle(&mut app);
        let before = *app.world().get::<Rect>(ui.compose).unwrap();
        assert_eq!(
            try_activate(app.world_mut(), trigger),
            ActivationResult::Accepted
        );
        settle(&mut app);
        let w = app.world();
        assert!(!w.has::<Hidden>(ui.behavior));
        assert_eq!(w.resource::<Focus>().unwrap().entity, Some(ui.depth));
        assert_eq!(*w.get::<Rect>(ui.compose).unwrap(), before);
        let r = w.get::<Rect>(ui.behavior).unwrap();
        assert!(
            r.x >= 0. && r.y >= 0. && r.x + r.w <= width && r.y + r.h <= 650.,
            "{r:?}"
        );
        let state = w.get::<SelectionAccess>(trigger).unwrap();
        assert_eq!(state.expanded, Some(true));
        assert_eq!(state.controls, Some(ui.behavior));
        escape(&mut app);
        let w = app.world();
        assert!(w.has::<Hidden>(ui.behavior) && !w.has::<Modal>(ui.behavior));
        assert_eq!(w.resource::<Focus>().unwrap().entity, Some(trigger));
        assert_eq!(
            w.get::<SelectionAccess>(trigger).unwrap().expanded,
            Some(false)
        );
    }
}

#[test]
fn nested_depth_selector_dismisses_inside_out_and_accepts_selection() {
    let (mut app, _rx) = fixture();
    let ui = *app.world().resource::<Ui>().unwrap();
    let trigger = behavior_trigger(app.world());
    assert_eq!(
        try_activate(app.world_mut(), trigger),
        ActivationResult::Accepted
    );
    settle(&mut app);
    let depth = selection::SelectionHandle { root: ui.depth };
    assert_eq!(
        try_activate(app.world_mut(), ui.depth),
        ActivationResult::Accepted
    );
    settle(&mut app);
    let popup = depth.popup(app.world()).unwrap();
    assert!(!app.world().has::<Hidden>(popup));
    escape(&mut app);
    assert!(app.world().has::<Hidden>(popup));
    assert!(!app.world().has::<Hidden>(ui.behavior));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(ui.depth)
    );
    assert_eq!(
        try_activate(app.world_mut(), ui.depth),
        ActivationResult::Accepted
    );
    settle(&mut app);
    let option = depth.option(app.world(), OptionId(2)).unwrap();
    assert_eq!(
        try_activate(app.world_mut(), option),
        ActivationResult::Accepted
    );
    settle(&mut app);
    assert_eq!(app.world().resource::<Model>().unwrap().depth, 2);
    assert!(app.world().has::<Hidden>(popup));
    assert!(!app.world().has::<Hidden>(ui.behavior));
    escape(&mut app);
    assert!(app.world().has::<Hidden>(ui.behavior));
}

#[test]
fn outside_click_and_route_change_leave_no_behavior_modal() {
    let (mut app, _rx) = fixture();
    let ui = *app.world().resource::<Ui>().unwrap();
    let trigger = behavior_trigger(app.world());
    for _ in 0..6 {
        assert_eq!(
            try_activate(app.world_mut(), trigger),
            ActivationResult::Accepted
        );
        settle(&mut app);
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [400., 200.];
        input.pointer.press_at = Some(input.pointer.position);
        input.pointer.pressed = true;
        input.pointer.down = true;
        input.pointer.button = PointerButton::Left;
        app.update();
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.pressed = false;
        input.pointer.down = false;
        input.pointer.released = true;
        app.update();
        settle(&mut app);
        assert!(app.world().has::<Hidden>(ui.behavior));
        assert!(!app.world().has::<Modal>(ui.behavior));
    }
    assert_eq!(
        try_activate(app.world_mut(), trigger),
        ActivationResult::Accepted
    );
    settle(&mut app);
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
    settle(&mut app);
    assert!(app.world().has::<Hidden>(ui.behavior));
    assert!(!app.world().has::<Modal>(ui.behavior));
}

#[test]
fn shared_action_hints_preserve_focus_and_escape_suppression() {
    let (mut app, _rx) = fixture();
    let target = app
        .world()
        .query::<Semantics>()
        .iter()
        .find(|(e, s)| {
            s.name.as_deref() == Some("Toggle conversation sidebar")
                && !app.world().has::<PassivePopup>(*e)
        })
        .unwrap()
        .0;
    let tip = visual::action_hint(app.world_mut(), target, "Toggle conversation sidebar");
    for reduced_motion in [true, false] {
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                reduced_motion,
                ..Default::default()
            },
        );
        request_focus(app.world_mut(), target);
        settle(&mut app);
        assert!(!app.world().has::<Hidden>(tip));
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(target)
        );
        escape(&mut app);
        assert!(app.world().has::<Hidden>(tip));
        settle(&mut app);
        assert!(app.world().has::<Hidden>(tip));
        let editor = app.world().resource::<Ui>().unwrap().composer.editor;
        request_focus(app.world_mut(), editor);
        settle(&mut app);
    }
}

#[test]
#[ignore = "Synthetic integrated Parley/GPU offscreen visual acceptance"]
fn native_polish_visual_acceptance() {
    let (mut app, _rx) = fixture();
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine.clone());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, suffix) in [
        (1200, 850, "wide"),
        (760, 650, "narrow"),
        (1600, 950, "large"),
        (1200, 850, "tooltip"),
        (1200, 850, "frameless"),
        (1400, 850, "split"),
        (760, 650, "split-narrow"),
        (1200, 850, "overflow"),
        (1200, 850, "light"),
        (760, 650, "light-zoom"),
        (1200, 850, "dark-zoom"),
        (1200, 850, "behavior"),
        (760, 650, "light-behavior"),
        (760, 650, "depth-popup"),
    ] {
        if suffix.contains("behavior") || suffix == "depth-popup" {
            app = fixture().0;
            app.world_mut().insert_resource(engine.clone());
        }
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(!suffix.starts_with("light")),
                ..Default::default()
            },
        );
        app.world_mut()
            .resource_mut::<UiTheme>()
            .unwrap()
            .reading_scale = if suffix.ends_with("zoom") { 1.5 } else { 1. };
        if suffix == "tooltip" {
            let target = app
                .world()
                .query::<Semantics>()
                .iter()
                .find(|(e, s)| {
                    s.name.as_deref() == Some("Toggle conversation sidebar")
                        && !app.world().has::<PassivePopup>(*e)
                })
                .unwrap()
                .0;
            app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(target);
        } else {
            app.world_mut().resource_mut::<Focus>().unwrap().entity = None;
        }
        app.world_mut().insert_resource(WindowChrome {
            draws_controls: suffix == "frameless",
            ..Default::default()
        });
        if suffix == "split" || suffix == "overflow" {
            let count = if suffix == "split" { 1 } else { 6 };
            for i in 0..count {
                let session = format!("visual-{suffix}-{i}");
                conversations::open(app.world_mut(), Some(session.clone()));
                let generation = app.world().resource::<Model>().unwrap().generation;
                conversations::reply(
                    app.world_mut(),
                    Reply {
                        generation,
                        update: Update::History(
                            session,
                            "a".into(),
                            vec![
                                serde_json::json!({"entry_id":"g", "entry_seq":1,"role":"assistant","content":"A second retained workspace. Its editor, timeline, and draft stay independent."}),
                            ],
                            None,
                        ),
                    },
                );
                for _ in 0..8 {
                    app.update();
                }
            }
            conversations::action(app.world_mut(), if suffix == "split" { 40 } else { 41 });
        }
        if suffix.contains("behavior") || suffix == "depth-popup" {
            for _ in 0..8 {
                rndr_paint::frame(&mut app);
            }
            let trigger = behavior_trigger(app.world());
            assert_eq!(
                try_activate(app.world_mut(), trigger),
                ActivationResult::Accepted
            );
            settle(&mut app);
            if suffix == "depth-popup" {
                let depth = app.world().resource::<Ui>().unwrap().depth;
                try_activate(app.world_mut(), depth);
            }
        }
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        if suffix.contains("behavior") || suffix == "depth-popup" {
            let popup = app.world().resource::<Ui>().unwrap().behavior;
            assert!(
                !app.world().has::<Hidden>(popup),
                "overlay fixture {suffix} must actually be open; focus={:?}, anchor={:?}, clip={:?}",
                app.world().resource::<Focus>().unwrap().entity,
                app.world().get::<Rect>(behavior_trigger(app.world())),
                app.world().get::<Clip>(behavior_trigger(app.world()))
            );
        }
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        let behavior = app.world().resource::<Ui>().unwrap().behavior;
        close_popup(app.world_mut(), behavior, false);
        image::save_buffer(
            format!("verification/native-shared-polish-{suffix}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
