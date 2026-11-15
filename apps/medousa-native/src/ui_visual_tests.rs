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
        for name in ["Toggle conversation sidebar", "Open work threads"] {
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
        eprintln!(
            "ANCHOR {:?} MAIN {:?} COMPOSE {:?}",
            app.world().get::<Rect>(trigger),
            app.world().get::<Rect>(ui.exchange.main),
            app.world().get::<Rect>(ui.compose)
        );
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
        eprintln!(
            "ANCHOR {:?} MAIN {:?} COMPOSE {:?}",
            app.world().get::<Rect>(trigger),
            app.world().get::<Rect>(ui.exchange.main),
            app.world().get::<Rect>(ui.compose)
        );
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
#[ignore = "Assistant image persisted-history GPU acceptance"]
fn assistant_image_visual_acceptance() {
    let (mut app, _rx) = fixture();
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    app.world_mut().resource_mut::<Model>().unwrap().chat.hydrate("s".into(), "a".into(), vec![
        serde_json::json!({"entry_id":"u","entry_seq":1,"role":"user","content":"Can you show me what the calmer layout would look like?"}),
        serde_json::json!({"entry_id":"a","entry_seq":2,"role":"assistant","content":"Here is the conversation-first concept.","parts":[
            {"kind":"text","markdown":"Here is the **conversation-first** concept."},
            {"kind":"tool_run","run_id":"image","tool_name":"cognition_image_view","status":"succeeded","input_summary":"Conversation-first concept","started_at":"2026-11-15T15:00:00Z","artifact_refs":[
                {"role":"output","content_type":"image/png","byte_size":42,"hash64":"h","artifact_id":"art:fixture:concept","label":"Conversation first"}
            ]}
        ]}),
    ], None).unwrap();
    let bytes = std::fs::read("verification/concepts/01-conversation-first.png").unwrap();
    let mut exporter = rndr_render::Exporter::new().unwrap();
    for (width, height, suffix) in [(1200, 850, "wide"), (760, 650, "narrow")] {
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        for _ in 0..8 {
            app.update();
        }
        crate::media_preview::resolve_fixture(app.world_mut(), &bytes);
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        let images = app.world().entities_with::<ImageView>();
        assert!(
            !images.is_empty(),
            "assistant output must produce pixels, not only a tool card"
        );
        for image in images {
            let rect = app.world().get::<Rect>(image).unwrap();
            assert!(rect.w > 0. && rect.h > 0. && rect.w <= 400.1 && rect.h <= 280.1);
        }
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-assistant-image-{suffix}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let thumbnail = app
            .world()
            .query::<Button>()
            .iter()
            .find(|(_, b)| b.action == "expand-image")
            .map(|(e, _)| *e)
            .unwrap();
        assert_eq!(
            try_activate(app.world_mut(), thumbnail),
            ActivationResult::Accepted
        );
        settle(&mut app);
        crate::media_preview::resolve_fixture(app.world_mut(), &bytes);
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        let large = app
            .world()
            .query::<ImageView>()
            .iter()
            .filter_map(|(e, _)| app.world().get::<Rect>(*e))
            .any(|r| r.w > 500. && r.h > 300.);
        assert!(large, "expanded preview must use available window space");
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-image-viewer-{suffix}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let zoom = app
            .world()
            .query::<Semantics>()
            .iter()
            .find(|(_, s)| s.name.as_deref() == Some("Zoom in"))
            .map(|(e, _)| *e)
            .unwrap();
        for _ in 0..2 {
            assert_eq!(
                try_activate(app.world_mut(), zoom),
                ActivationResult::Accepted
            );
            settle(&mut app);
        }
        for _ in 0..8 {
            rndr_paint::frame(&mut app);
        }
        let scroll = app
            .world()
            .query::<Scroll>()
            .iter()
            .find(|(e, s)| {
                app.world()
                    .get::<Rect>(*e)
                    .is_some_and(|r| s.content[0] > r.w + 100. && s.content[1] > r.h + 100.)
            })
            .map(|(e, _)| *e)
            .expect("zoom must provide scrollable overflow on both axes");
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [-50., -50.];
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        assert_eq!(
            app.world().get::<Scroll>(scroll).unwrap().offset,
            [-50., -50.]
        );
        app.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())];
        settle(&mut app);
        assert!(
            !app.world()
                .query::<Semantics>()
                .iter()
                .any(|(_, s)| s.name.as_deref() == Some("Image preview"))
        );
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

#[test]
#[ignore = "Composer long-draft Parley/GPU acceptance"]
fn composer_long_draft_visual_acceptance() {
    let (mut app, _rx) = fixture();
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    let mut exporter = rndr_render::Exporter::new().unwrap();
    for (width, suffix) in [(1200, "wide"), (760, "narrow")] {
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, 850.));
        for _ in 0..8 {
            rndr_paint::frame(&mut app);
        }
        let ui = *app.world().resource::<Ui>().unwrap();
        request_focus(app.world_mut(), ui.composer.editor);
        app.world_mut().resource_mut::<Input>().unwrap().chars = "This is a longer composer draft that should wrap naturally inside the message box, keeping every word and the insertion point visible while I continue typing. ".repeat(3);
        rndr_paint::frame(&mut app);
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .chars
            .clear();
        for _ in 0..8 {
            rndr_paint::frame(&mut app);
        }
        let w = app.world();
        let editor = w.get::<Rect>(ui.composer.editor).unwrap();
        let shell = w.get::<Rect>(ui.compose).unwrap();
        assert!(
            editor.h > 80. && editor.h <= 140.,
            "bounded growth: {editor:?}"
        );
        assert!(editor.x >= shell.x && editor.x + editor.w <= shell.x + shell.w);
        let caret = w.get::<Caret>(ui.composer.editor).unwrap();
        let layout = w.get::<TextLayout>(ui.composer.editor).unwrap();
        let row = layout
            .lines
            .iter()
            .position(|l| caret.index >= l.start && caret.index <= l.end)
            .expect("caret in visible source window");
        assert!(
            layout.lines[row].y + layout.lines[row].h <= editor.h - 8. + 0.5,
            "caret row {row} outside {editor:?}"
        );
        assert!(layout.lines.iter().all(|l| l.width <= editor.w + 0.5));
        assert!(
            w.get::<Label>(ui.composer.editor)
                .unwrap()
                .text
                .ends_with("typing. ")
        );
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, 850, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-composer-wrap-{suffix}.png"),
            &image.rgba,
            width,
            850,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn composer_grows_with_draft_then_shrinks_without_replacing_document() {
    let (mut app, _rx) = fixture();
    let document = app.world().resource::<Model>().unwrap().document;
    let editor = app.world().resource::<Ui>().unwrap().composer;
    request_focus(app.world_mut(), editor.editor);
    app.world_mut().resource_mut::<Input>().unwrap().chars =
        "A long naturally wrapped draft. ".repeat(80);
    for _ in 0..8 {
        app.update();
    }
    let grown = app.world().get::<Rect>(editor.editor).unwrap().h;
    assert!(
        grown > 80. && grown <= 140.,
        "bounded editor height {grown}"
    );
    {
        let doc = app
            .world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(document)
            .unwrap();
        doc.reload(doc.revision(), "short draft").unwrap();
    }
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(app.world().resource::<Model>().unwrap().document, document);
    assert!(app.world().get::<Rect>(editor.editor).unwrap().h <= 60.);
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .text(),
        "short draft"
    );
}
