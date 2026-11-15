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

#[test]
#[ignore = "Synthetic integrated Parley/GPU offscreen visual acceptance"]
fn native_polish_visual_acceptance() {
    let (mut app, _rx) = fixture();
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
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
    ] {
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
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
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
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
