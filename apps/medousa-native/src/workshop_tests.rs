//! In-process native UI tests. No saved credentials, daemon processes or live
//! pairing are read: the two authorities use independent command channels.
use super::*;
use crate::{routing::qualify, surfaces::SurfaceId, workshops};
fn boot() -> (
    App,
    tempfile::TempDir,
    Vec<tokio::sync::mpsc::UnboundedReceiver<Request>>,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    let channels = workshops::install_fixture(
        app.world_mut(),
        dir.path().join("sources.json"),
        &[("authority-a", "Studio"), ("authority-b", "Laptop")],
    );
    app.world_mut().insert_resource(Viewport::new(1280., 900.));
    app.update();
    for (i, owner) in ["authority-a", "authority-b"].into_iter().enumerate() {
        apply_reply(
            app.world_mut(),
            Reply {
                generation: u64::MAX - i as u64,
                update: Update::SessionsPage {
                    query: String::new(),
                    rows: vec![(qualify(owner, "s"), "Same title".into())],
                    summaries: vec![],
                    cursor: None,
                    append: false,
                },
            },
        );
    }
    chat_sidebar::expand_fixture_groups(app.world_mut());
    app.update();
    (app, dir, channels)
}
#[test]
#[ignore = "Offscreen GPU workshop setup acceptance"]
fn workshop_setup_visual_acceptance() {
    let (mut app, _dir, _channels) = boot();
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    workshops::action(app.world_mut(), &HostIntent::Command(0x5753_3000));
    let kb = qualify("authority-b", "s");
    conversations::open(app.world_mut(), Some(kb.clone()));
    let generation = app.world().resource::<Model>().unwrap().generation;
    apply_reply(
        app.world_mut(),
        Reply {
            generation,
            update: Update::History(
                kb,
                "authority-b".into(),
                vec![
                    serde_json::json!({"entry_id":"e","entry_seq":1,"role":"user","content":"This conversation belongs to Laptop. New chats are destined for Studio."}),
                ],
                None,
            ),
        },
    );
    let mut exporter = rndr_render::Exporter::new().unwrap();
    for setup in [false, true] {
        if setup {
            workshops::action(app.world_mut(), &HostIntent::Command(0x5753_0001));
        }
        for _ in 0..4 {
            app.update();
        }
        let (_, frame) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, 1280, 900, 1.)
                .unwrap()
        });
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/verification-inputs/native/workshops-{}.png",
            if setup { "setup" } else { "chat" }
        ));
        image::save_buffer(
            path,
            &frame.rgba,
            frame.width,
            frame.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
#[test]
fn colliding_chat_ids_keep_owners_replies_actions_and_offline_recovery_separate() {
    let (mut app, _dir, mut channels) = boot();
    let mut b = channels.pop().unwrap();
    let mut a = channels.pop().unwrap();
    let ka = qualify("authority-a", "s");
    let kb = qualify("authority-b", "s");
    assert_eq!(app.world().resource::<Model>().unwrap().sessions.len(), 2);
    let labels: Vec<_> = app
        .world()
        .entities_with::<Label>()
        .into_iter()
        .filter_map(|e| app.world().get::<Label>(e))
        .map(|l| l.text.clone())
        .collect();
    assert!(labels.iter().any(|s| s == "Studio"));
    assert!(labels.iter().any(|s| s == "Laptop"));
    conversations::open(app.world_mut(), Some(ka.clone()));
    let ga = app.world().resource::<Model>().unwrap().generation;
    assert!(matches!(a.try_recv().unwrap().command,Command::Open(s) if s==ka));
    assert!(b.try_recv().is_err());
    apply_reply(
        app.world_mut(),
        Reply {
            generation: ga,
            update: Update::History(ka.clone(), "authority-a".into(), vec![], None),
        },
    );
    app.world_mut().resource_mut::<Model>().unwrap().depth = 2;
    conversations::open(app.world_mut(), Some(kb.clone()));
    let gb = app.world().resource::<Model>().unwrap().generation;
    assert_ne!(ga, gb);
    assert!(matches!(b.try_recv().unwrap().command,Command::Open(s) if s==kb));
    apply_reply(
        app.world_mut(),
        Reply {
            generation: gb,
            update: Update::History(kb.clone(), "authority-b".into(), vec![], None),
        },
    );
    apply_reply(
        app.world_mut(),
        Reply {
            generation: ga,
            update: Update::Notice("A background notice".into()),
        },
    );
    assert_eq!(
        app.world().resource::<Model>().unwrap().chat.session(),
        Some(kb.as_str())
    );
    assert_ne!(
        app.world().resource::<Model>().unwrap().chat.status,
        "A background notice"
    );
    conversations::open(app.world_mut(), Some(ka.clone()));
    assert_eq!(app.world().resource::<Model>().unwrap().depth, 2);
    // A dead owner channel is not substituted with B's live channel.
    drop(a);
    dispatch(
        app.world_mut(),
        Command::SetMode(ka.clone(), serde_json::from_str("\"general\"").unwrap()),
    );
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .chat
            .status
            .contains("Owning workshop unavailable")
    );
    assert!(b.try_recv().is_err());
    let mut recovered = workshops::recover_fixture(app.world_mut(), "authority-a");
    dispatch(
        app.world_mut(),
        Command::Rename {
            session: ka.clone(),
            title: "Recovered A".into(),
        },
    );
    assert!(
        matches!(recovered.try_recv().unwrap().command,Command::Rename{session,..} if session==ka)
    );
    assert!(b.try_recv().is_err());
}
#[test]
fn vault_and_canvas_destinations_are_view_local_and_epoch_fenced() {
    let (mut app, _dir, _channels) = boot();
    crate::surfaces::select(app.world_mut(), SurfaceId::Notes);
    app.update();
    assert!(workshops::action(
        app.world_mut(),
        &HostIntent::Command(0x5753_3000)
    ));
    let notes = workshops::binding(app.world(), SurfaceId::Notes).unwrap();
    assert_eq!(notes.authority, "authority-a");
    crate::surfaces::select(app.world_mut(), SurfaceId::Create);
    app.update();
    assert!(workshops::action(
        app.world_mut(),
        &HostIntent::Command(0x5753_3001)
    ));
    let old = workshops::binding(app.world(), SurfaceId::Create).unwrap();
    assert_eq!(old.authority, "authority-b");
    assert_eq!(
        workshops::binding(app.world(), SurfaceId::Notes).unwrap(),
        notes
    );
    workshops::action(app.world_mut(), &HostIntent::Command(0x5753_3000));
    let now = workshops::binding(app.world(), SurfaceId::Create).unwrap();
    assert_eq!(now.authority, "authority-a");
    assert!(!now.accepts(&old));
    assert_eq!(
        workshops::binding(app.world(), SurfaceId::Notes).unwrap(),
        notes
    );
}
