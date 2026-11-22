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
    // RNDR eases fill changes; this checks where the fill lands, not the fade.
    app.world_mut()
        .insert_resource(rndr_ui::InteractionMotion::snap());
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
            mode.y + mode.h <= shell.y + shell.h + 1.,
            "footer must stay inside centered column"
        );
        let card = children_of(w, ui.compose)
            .into_iter()
            .find(|e| w.has::<Radius>(*e))
            .and_then(|e| w.get::<Rect>(e))
            .unwrap();
        assert!(
            mode.y + mode.h <= card.y + card.h + 1.,
            "agent, mode and behavior share the writing card's control row"
        );
        // Send must stay reachable at every width, not pushed off the card.
        let send = w
            .query::<Semantics>()
            .iter()
            .find(|(_, s)| s.name.as_deref() == Some("Send message"))
            .and_then(|(e, _)| w.get::<Rect>(*e))
            .copied()
            .unwrap();
        assert!(
            send.x + send.w <= card.x + card.w + 1.,
            "send {send:?} escapes card {card:?} at {width}"
        );
        assert!(card.h < 125., "idle card stays compact: {card:?}");
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
            {"kind":"text","segment_id":"intro","markdown":"Here is the **conversation-first** concept."},
            {"kind":"tool_run","run_id":"image","tool_name":"cognition_image_view","status":"succeeded","input_summary":"Conversation-first concept","started_at":"2026-11-15T15:00:00Z","artifact_refs":[
                {"role":"output","content_type":"image/png","byte_size":42,"hash64":"h","artifact_id":"art:fixture:concept","label":"Conversation first"}
                ]},
            {"kind":"tool_run","run_id":"image2","started_at":"2026-11-15T15:01:00Z","input_summary":"Alternate views","tool_name":"cognition_image_view","status":"succeeded","artifact_refs":[
                {"role":"output","content_type":"image/png","byte_size":42,"hash64":"i","artifact_id":"art:fixture:second","label":"Alternate"},
                {"role":"output","content_type":"image/png","byte_size":42,"hash64":"j","artifact_id":"art:fixture:third","label":"Detail"},
                {"role":"output","content_type":"image/png","byte_size":42,"hash64":"k","artifact_id":"art:fixture:intermediate","label":"Intermediate"}
            ]},
            {"kind":"text","segment_id":"reply","markdown":"Here are the options."}
        ]}),
    ], None).unwrap();
    let bytes = std::fs::read("verification/concepts/01-conversation-first.png").unwrap();
    let mut exporter = rndr_render::Exporter::new().unwrap();
    for (width, height, suffix) in [(1200, 850, "wide"), (760, 650, "narrow")] {
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(suffix == "wide"),
                ..Default::default()
            },
        );
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        for _ in 0..8 {
            app.update();
        }
        crate::media_preview::resolve_fixture(app.world_mut(), &bytes);
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        let images: Vec<_> = app
            .world()
            .entities_with::<ImageView>()
            .into_iter()
            .filter(|e| {
                app.world()
                    .get::<Button>(*e)
                    .is_some_and(|b| b.action == "expand-image")
            })
            .collect();
        assert_eq!(
            images.len(),
            2,
            "remaining image is available through overflow"
        );
        let badge = app
            .world()
            .query::<Semantics>()
            .iter()
            .find(|(_, s)| s.name.as_deref() == Some("Show 2 more images"))
            .map(|(e, _)| *e)
            .unwrap();
        assert!(
            app.world().get::<Rect>(badge).unwrap().w <= 49.,
            "overflow is a badge, not a bar"
        );
        let first = app.world().get::<Rect>(images[0]).unwrap();
        let second = app.world().get::<Rect>(images[1]).unwrap();
        assert!((first.y - second.y).abs() < 1. && (first.x - second.x).abs() >= first.w - 1.);
        for image in images {
            let rect = app.world().get::<Rect>(image).unwrap();
            assert!(rect.w > 0. && rect.h > 0. && rect.w <= 600.1 && rect.h <= 340.1);
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
            .find(|(e, s)| app.world().has::<Button>(*e) && s.name.as_deref() == Some("Zoom in"))
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
    // Capture settled fills; appearance switches below would otherwise catch
    // RNDR's 120 ms fill ease mid-way.
    app.world_mut()
        .insert_resource(rndr_ui::InteractionMotion::snap());
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
            editor.h > 80. && editor.h <= 297.5,
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
        grown > 80. && grown <= 297.5,
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

#[test]
fn sidebar_search_is_disclosed_and_clears_filter_on_close() {
    let (mut app, _rx) = fixture();
    let ui = *app.world().resource::<Ui>().unwrap();
    assert!(app.world().has::<Hidden>(ui.search));
    action(app.world_mut(), TOGGLE_SEARCH);
    settle(&mut app);
    assert!(!app.world().has::<Hidden>(ui.search));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(ui.search)
    );
    app.world_mut().get_mut::<Label>(ui.search).unwrap().text = "Native".into();
    settle(&mut app);
    action(app.world_mut(), TOGGLE_SEARCH);
    settle(&mut app);
    assert!(app.world().has::<Hidden>(ui.search));
    assert!(app.world().get::<Label>(ui.search).unwrap().text.is_empty());
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .search_query
            .is_empty()
    );
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(ui.composer.editor)
    );
}

#[test]
fn composer_scrollbar_is_narrow_and_keeps_the_native_target() {
    let (app, _rx) = fixture();
    let w = app.world();
    let ui = w.resource::<Ui>().unwrap();
    let bar = children_of(w, ui.composer.root)
        .into_iter()
        .find(|e| w.has::<Scrollbar>(*e))
        .unwrap();
    assert_eq!(w.get::<Scrollbar>(bar).unwrap().target, ui.composer.editor);
    assert_eq!(w.get::<Layout>(bar).unwrap().width, Size::Px(8.));
    assert!(w.has::<Focusable>(bar));
    assert_eq!(
        w.get::<Color>(bar),
        Some(&visual::color(w, composer_color()))
    );
}

#[test]
fn composer_expands_during_incremental_typing_and_resize() {
    let (mut app, _rx) = fixture();
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    for _ in 0..4 {
        app.update();
    }
    let ui = *app.world().resource::<Ui>().unwrap();
    let document = app.world().resource::<Model>().unwrap().document;
    request_focus(app.world_mut(), ui.composer.editor);
    let initial = app.world().get::<Rect>(ui.composer.editor).unwrap().h;
    for ch in "A naturally wrapping prompt with enough words to grow while typing. "
        .repeat(8)
        .chars()
    {
        app.world_mut().resource_mut::<Input>().unwrap().chars = ch.to_string();
        app.update();
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .chars
            .clear();
        app.update();
    }
    let wide = *app.world().get::<Rect>(ui.composer.editor).unwrap();
    assert!(wide.h > initial + 40., "{initial} -> {wide:?}");
    app.world_mut().insert_resource(Viewport::new(760., 850.));
    for _ in 0..8 {
        app.update();
    }
    let narrow = *app.world().get::<Rect>(ui.composer.editor).unwrap();
    assert!(
        narrow.h >= wide.h && narrow.h <= 297.5,
        "{wide:?} -> {narrow:?}"
    );
    assert_eq!(app.world().resource::<Model>().unwrap().document, document);
    assert_eq!(
        app.world().resource::<Ui>().unwrap().composer.editor,
        ui.composer.editor
    );
}

#[test]
fn timeline_keeps_one_draggable_scrollbar_without_passive_overlay() {
    let (mut app, _rx) = fixture();
    for _ in 0..3 {
        settle(&mut app);
        let w = app.world();
        let root = w.resource::<Ui>().unwrap().timeline;
        assert!(w.has::<Scroll>(root));
        assert!(!w.has::<ScrollIndicator>(root));
        let bars: Vec<_> = w
            .entities_with::<Scrollbar>()
            .into_iter()
            .filter(|e| w.get::<Scrollbar>(*e).unwrap().target == root)
            .collect();
        assert_eq!(bars.len(), 1);
        assert!(w.has::<Focusable>(bars[0]));
    }
}

#[test]
#[ignore = "Overflowing timeline scrollbar GPU acceptance"]
fn timeline_single_scrollbar_visual_acceptance() {
    let (mut app, _rx) = fixture();
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    let content = "The conversation keeps one draggable scrollbar while the left navigation stays available.\n\n".repeat(40);
    app.world_mut().resource_mut::<Model>().unwrap().chat.hydrate("s".into(), "a".into(), vec![
        serde_json::json!({"entry_id":"u", "entry_seq":1, "role":"user", "content":"Keep the left nav; remove the duplicate scrollbar."}),
        serde_json::json!({"entry_id":"a", "entry_seq":2, "role":"assistant", "content":content}),
    ], None).unwrap();
    let mut exporter = rndr_render::Exporter::new().unwrap();
    for (width, height, suffix) in [(1200, 850, "wide"), (760, 650, "narrow")] {
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        let root = app.world().resource::<Ui>().unwrap().timeline;
        let scroll = app.world().get::<Scroll>(root).unwrap();
        assert!(scroll.content[1] > app.world().get::<Rect>(root).unwrap().h);
        assert!(!app.world().has::<ScrollIndicator>(root));
        let bars: Vec<_> = app
            .world()
            .entities_with::<Scrollbar>()
            .into_iter()
            .filter(|e| app.world().get::<Scrollbar>(*e).unwrap().target == root)
            .collect();
        assert_eq!(bars.len(), 1);
        assert_eq!(app.world().get::<Opacity>(bars[0]).unwrap().0, 1.);
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-single-scrollbar-{suffix}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
#[ignore = "Sealed Forge Reviews Parley/GPU visual acceptance"]
fn native_reviews_visual_acceptance() {
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, light, changes, confirm, name) in [
        (1200, 850, false, false, false, "summary-wide"),
        (760, 850, false, false, false, "summary-narrow"),
        (1200, 850, false, true, false, "changes-wide"),
        (760, 760, false, true, false, "changes-narrow"),
        (1200, 850, true, true, false, "changes-light"),
        (760, 850, true, false, true, "confirm-narrow"),
    ] {
        let (mut app, _rx) = fixture();
        let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
        app.world_mut().insert_resource(engine);
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(!light),
                ..Default::default()
            },
        );
        crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Reviews);
        crate::reviews::take_commands(app.world_mut());
        crate::reviews::tests::seed_visual(app.world_mut(), changes, confirm);
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        crate::reviews::tests::assert_visual_geometry(app.world(), changes);
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-reviews-{name}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
        if changes {
            let root = app
                .world()
                .query::<Semantics>()
                .into_iter()
                .find(|(_, sem)| sem.name.as_deref() == Some("Sealed changes to src/main.rs"))
                .unwrap()
                .0;
            let port = *app.world().get::<Rect>(root).unwrap();
            let extent = app.world().get::<Scroll>(root).unwrap().content[0];
            assert!(
                extent > port.w + 100.,
                "Long source must establish a horizontal range"
            );
            app.world_mut().get_mut::<Scroll>(root).unwrap().offset[0] = port.w - extent;
            for _ in 0..4 {
                rndr_paint::frame(&mut app);
            }
            assert!(app.world().get::<Scroll>(root).unwrap().offset[0] < -100.);
            let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, width, height, 1.)
                    .unwrap()
            });
            image::save_buffer(
                format!("verification/native-reviews-{name}-horizontal-tail.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            app.world_mut().get_mut::<Scroll>(root).unwrap().offset[0] = 0.;
            crate::reviews::tests::scroll_visual_diff_to_end(app.world_mut());
            for _ in 0..4 {
                rndr_paint::frame(&mut app);
            }
            crate::reviews::tests::assert_visual_diff_end(app.world());
            crate::reviews::tests::assert_visual_geometry(app.world(), true);
            let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, width, height, 1.)
                    .unwrap()
            });
            image::save_buffer(
                format!("verification/native-reviews-{name}-end.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            for expanded in [true, false] {
                crate::reviews::tests::visual_accordions(app.world_mut(), expanded);
                for _ in 0..4 {
                    rndr_paint::frame(&mut app);
                }
                let count = app
                    .world()
                    .query::<Semantics>()
                    .into_iter()
                    .filter(|(_, sem)| {
                        sem.name
                            .as_deref()
                            .is_some_and(|n| n.starts_with("Sealed changes to "))
                    })
                    .count();
                assert_eq!(count, if expanded { 2 } else { 0 });
                let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                    exporter
                        .render_frame(Some(world), frame, width, height, 1.)
                        .unwrap()
                });
                image::save_buffer(
                    format!(
                        "verification/native-reviews-{name}-accordions-{}.png",
                        if expanded { "expanded" } else { "collapsed" }
                    ),
                    &image.rgba,
                    image.width,
                    image.height,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            for expanded in [false, true] {
                crate::reviews::tests::visual_context(app.world_mut(), expanded);
                for _ in 0..4 {
                    rndr_paint::frame(&mut app);
                }
                let control = if expanded {
                    "Collapse unchanged context"
                } else {
                    "Show unchanged context"
                };
                assert!(
                    app.world()
                        .query::<Label>()
                        .iter()
                        .any(|(_, l)| l.text == control)
                );
                let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                    exporter
                        .render_frame(Some(world), frame, width, height, 1.)
                        .unwrap()
                });
                image::save_buffer(
                    format!(
                        "verification/native-reviews-{name}-context-{}.png",
                        if expanded { "expanded" } else { "collapsed" }
                    ),
                    &image.rgba,
                    image.width,
                    image.height,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
}

#[test]
#[ignore = "Empty conversation presence Parley/GPU acceptance"]
fn empty_presence_visual_acceptance() {
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, suffix) in [
        (1200, 850, "dark"),
        (1200, 850, "light"),
        (760, 650, "narrow"),
    ] {
        let (mut app, _rx) = fixture();
        app.world_mut().insert_resource(engine.clone());
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(suffix != "light"),
                ..Default::default()
            },
        );
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.chat
            .hydrate("empty".into(), "a".into(), vec![], None)
            .unwrap();
        app.world_mut().resource_mut::<Focus>().unwrap().entity = None;
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        let ui = *app.world().resource::<Ui>().unwrap();
        assert!(!app.world().has::<Hidden>(ui.empty_presence));
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-empty-presence-{suffix}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
