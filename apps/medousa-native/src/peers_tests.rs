use super::*;
use serde_json::json;
fn peer(id: &str) -> Value {
    json!({"workshopId":id,"workshopDeviceId":format!("device-{id}"),"label":format!("Workshop {id}"),"hasSessionToken":true})
}
fn message(id: &str) -> Value {
    json!({"id":format!("message-{id}"),"sinkKind":"peer","workshopId":id,"fromDeviceId":format!("device-{id}"),"toDeviceId":"me","direction":"in","body":"Hey! Shall we look at the native workspace together?","sentAt":"2026-11-19T09:30:00Z","readAt":null})
}
fn app() -> App {
    let mut a = crate::ui::app();
    a.world_mut().insert_resource(Viewport::new(1200., 850.));
    a.update();
    crate::surfaces::select(a.world_mut(), crate::surfaces::SurfaceId::Peers);
    let c = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command: c,
            result: Ok(Response::Snapshot {
                peers: vec![peer("one"), peer("two")],
                messages: vec![message("one"), message("two")],
                warnings: vec![],
            }),
        },
    );
    sync(a.world_mut());
    a
}
fn act(w: &mut World, a: Action) {
    let id = next();
    w.resource_mut::<State>().unwrap().actions.insert(id, a);
    assert!(action(w, &HostIntent::Command(id)));
    sync(w);
}
fn input(w: &mut World, t: &str) {
    let e = w.resource::<State>().unwrap().editor.unwrap();
    w.get_mut::<Label>(e).unwrap().text = t.into();
    sync(w);
}
#[test]
fn exact_identity_and_unread() {
    let p = Peer::parse(&peer("one")).unwrap();
    assert!(involves(&message("one"), &p));
    assert!(!involves(&message("two"), &p));
    let mut m = message("one");
    assert!(unread(&m));
    m["readAt"] = json!("now");
    assert!(!unread(&m));
    m["readAt"] = Value::Null;
    m["direction"] = json!("out");
    assert!(!unread(&m));
    assert!(!involves(
        &json!({"sinkKind":"peer","workshopId":"one-prefix","fromDeviceId":"device-one"}),
        &p
    ));
}
#[test]
fn drafts_survive_peer_and_surface_switches() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("one".into()));
    input(w, "draft one 🦀");
    act(w, Action::Select("two".into()));
    input(w, "draft two");
    act(w, Action::Select("one".into()));
    assert_eq!(w.resource::<State>().unwrap().draft(), "draft one 🦀");
    crate::surfaces::select(w, crate::surfaces::SurfaceId::Calendar);
    crate::surfaces::select(w, crate::surfaces::SurfaceId::Peers);
    sync(w);
    assert_eq!(w.resource::<State>().unwrap().draft(), "draft one 🦀");
}
#[test]
fn send_completion_does_not_clear_newer_draft_or_other_peer() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("one".into()));
    input(w, "old");
    act(w, Action::Send);
    let c = take_commands(w).pop().unwrap();
    input(w, "newer");
    act(w, Action::Select("two".into()));
    input(w, "other");
    apply(
        w,
        Update {
            command: c,
            result: Ok(Response::Sent(message("one"))),
        },
    );
    sync(w);
    assert_eq!(w.resource::<State>().unwrap().drafts["one"], "newer");
    assert_eq!(w.resource::<State>().unwrap().drafts["two"], "other");
}
#[test]
fn uncertain_send_is_never_retried_and_requires_explicit_unlock() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("one".into()));
    input(w, "keep me");
    act(w, Action::Send);
    let c = take_commands(w).pop().unwrap();
    apply(
        w,
        Update {
            command: c,
            result: Err("timeout".into()),
        },
    );
    sync(w);
    assert!(take_commands(w).is_empty());
    act(w, Action::Send);
    assert!(take_commands(w).is_empty());
    assert_eq!(w.resource::<State>().unwrap().draft(), "keep me");
    act(w, Action::AllowResend);
    act(w, Action::Send);
    assert_eq!(take_commands(w).len(), 1);
}
#[test]
fn refresh_keeps_editor_focus_scroll_and_ignores_stale_reply() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("one".into()));
    input(w, "stable");
    let editor = w.resource::<State>().unwrap().editor.unwrap();
    w.resource_mut::<Focus>().unwrap().entity = Some(editor);
    let thread = w.resource::<State>().unwrap().thread;
    w.get_mut::<Scroll>(thread).unwrap().offset[1] = -50.;
    act(w, Action::Refresh);
    let c = take_commands(w).pop().unwrap();
    apply(
        w,
        Update {
            command: c.clone(),
            result: Err("unavailable".into()),
        },
    );
    sync(w);
    assert_eq!(w.resource::<State>().unwrap().editor, Some(editor));
    assert_eq!(w.resource::<Focus>().unwrap().entity, Some(editor));
    assert_eq!(w.get::<Scroll>(thread).unwrap().offset[1], -50.);
    apply(
        w,
        Update {
            command: c,
            result: Ok(Response::Snapshot {
                peers: vec![],
                messages: vec![],
                warnings: vec![],
            }),
        },
    );
    assert_eq!(w.resource::<State>().unwrap().peers.len(), 2);
}
#[test]
fn read_is_explicit_and_scoped_to_thread() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("one".into()));
    assert!(take_commands(w).is_empty());
    act(w, Action::MarkRead);
    let c = take_commands(w).pop().unwrap();
    let Kind::Read { peer, messages } = c.kind else {
        panic!()
    };
    assert_eq!(peer, "one");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["workshopId"], "one");
}
#[test]
fn narrow_back_preserves_draft() {
    let mut a = app();
    let w = a.world_mut();
    w.insert_resource(Viewport::new(620., 850.));
    sync(w);
    assert!(w.has::<Hidden>(w.resource::<State>().unwrap().detail));
    act(w, Action::Select("one".into()));
    input(w, "narrow draft");
    assert!(w.has::<Hidden>(w.resource::<State>().unwrap().sidebar));
    act(w, Action::Back);
    assert!(!w.has::<Hidden>(w.resource::<State>().unwrap().sidebar));
    act(w, Action::Select("one".into()));
    assert_eq!(w.resource::<State>().unwrap().draft(), "narrow draft");
}
#[test]
#[ignore = "writes deterministic native peer visual fixtures"]
fn export_peers_visuals() {
    use std::sync::Arc;
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (name, width, height, light, selected) in [
        ("wide", 1280, 900, false, true),
        ("light", 1280, 900, true, true),
        ("narrow", 620, 850, false, true),
        ("list", 620, 850, false, false),
        ("empty", 1000, 800, false, false),
    ] {
        let mut a = app();
        a.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
        a.world_mut().insert_resource(engine);
        publish_system_appearance(
            a.world_mut(),
            SystemAppearance {
                dark: Some(!light),
                ..Default::default()
            },
        );
        if name == "empty" {
            let s = a.world_mut().resource_mut::<State>().unwrap();
            s.peers.clear();
            s.messages.clear();
            s.redraw = true;
        }
        if selected {
            act(a.world_mut(), Action::Select("one".into()));
            input(a.world_mut(), "Ayeee! Looking good — let's keep going.");
        }
        for _ in 0..6 {
            rndr_paint::frame(&mut a);
        }
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut a, |w, f| {
            exporter
                .render_frame(Some(w), f, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-peers-{name}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let s = a.world().resource::<State>().unwrap();
        let visible = if selected { s.compose } else { s.sidebar };
        let r = a.world().get::<Rect>(visible).unwrap();
        assert!(
            r.x >= 0. && r.x + r.w <= width as f32 + 1. && r.y + r.h <= height as f32 + 1.,
            "{name}: {r:?}"
        );
    }
}

#[test]
fn refresh_cannot_race_a_send() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("one".into()));
    input(w, "hello");
    act(w, Action::Refresh);
    let reads = take_commands(w);
    assert_eq!(reads.len(), 1);
    act(w, Action::Send);
    act(w, Action::MarkRead);
    assert!(take_commands(w).is_empty());
    assert!(w.resource::<State>().unwrap().mutation.is_none());
}
#[test]
fn edited_then_restored_draft_is_not_cleared_by_old_send() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("one".into()));
    input(w, "same");
    act(w, Action::Send);
    let c = take_commands(w).pop().unwrap();
    input(w, "changed");
    input(w, "same");
    apply(
        w,
        Update {
            command: c,
            result: Ok(Response::Sent(message("one"))),
        },
    );
    sync(w);
    assert_eq!(w.resource::<State>().unwrap().draft(), "same");
}
#[test]
fn ime_blocks_peer_navigation_and_preserves_composition_on_send_completion() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("one".into()));
    input(w, "pending");
    act(w, Action::Send);
    let c = take_commands(w).pop().unwrap();
    let e = w.resource::<State>().unwrap().editor.unwrap();
    w.insert(
        e,
        ImePreview {
            text: "界".into(),
            cursor: None,
        },
    );
    act(w, Action::Select("two".into()));
    assert_eq!(
        w.resource::<State>().unwrap().selected.as_deref(),
        Some("one")
    );
    apply(
        w,
        Update {
            command: c,
            result: Ok(Response::Sent(message("one"))),
        },
    );
    sync(w);
    assert_eq!(w.get::<ImePreview>(e).unwrap().text, "界");
    assert_eq!(w.get::<Label>(e).unwrap().text, "pending");
}
#[test]
fn redraw_does_not_accumulate_dead_controls() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("one".into()));
    let n = w.resource::<State>().unwrap().actions.len();
    for _ in 0..10 {
        w.resource_mut::<State>().unwrap().redraw = true;
        sync(w);
    }
    assert_eq!(w.resource::<State>().unwrap().actions.len(), n);
}
