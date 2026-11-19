use super::*;
use serde_json::json;
fn app() -> App {
    let mut a = crate::ui::app();
    a.world_mut().insert_resource(Viewport::new(1200., 900.));
    a.update();
    crate::surfaces::select(a.world_mut(), crate::surfaces::SurfaceId::Peers);
    crate::peers::take_commands(a.world_mut());
    open(a.world_mut());
    let c = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command: c,
            result: Ok(snapshot()),
        },
    );
    sync(a.world_mut());
    a
}
fn snapshot() -> Value {
    json!({"peers":[{"id":"inbound-device-a","device":"device-a","pairing":"pair-a","name":"Studio workshop","inbound":true,"active":true,"expires":null,"idle":null},{"id":"remote-b","device":"device-b","pairing":"pair-b","name":"Travel workshop","inbound":false}]})
}
fn act(w: &mut World, a: Action) {
    let id = next();
    w.resource_mut::<State>().unwrap().actions.insert(id, a);
    assert!(action(w, &HostIntent::Command(id)));
    sync(w);
}
#[test]
fn opening_only_reads_status_and_discovery_never_pairs() {
    let mut a = app();
    let w = a.world_mut();
    assert!(take_commands(w).is_empty());
    act(w, Action::Discover);
    let c = take_commands(w).pop().unwrap();
    assert!(matches!(c.request, Request::Discover));
    apply(
        w,
        Update {
            command: c,
            result: Ok(
                json!({"workshops":[{"peerName":"Nearby","daemonUrl":"http://192.0.2.1:7419"}]}),
            ),
        },
    );
    sync(w);
    act(w, Action::ChooseUrl("http://192.0.2.1:7419".into()));
    assert!(take_commands(w).is_empty());
    assert_eq!(
        value(w, w.resource::<State>().unwrap().url),
        "http://192.0.2.1:7419"
    );
}
#[test]
fn pair_is_explicit_confirmed_and_snapshot_bound() {
    let mut a = app();
    let w = a.world_mut();
    let s = w.resource::<State>().unwrap().clone();
    set(w, s.invite, "medousa://pair?signed=fixture");
    set(w, s.url, "http://192.0.2.2:7419");
    act(w, Action::Pair);
    assert!(take_commands(w).is_empty());
    set(w, s.url, "http://other.invalid");
    act(w, Action::Confirm);
    let c = take_commands(w).pop().unwrap();
    assert!(matches!(c.request,Request::Pair{url,..} if url=="http://192.0.2.2:7419"));
    act(w, Action::Confirm);
    act(w, Action::Pair);
    assert!(take_commands(w).is_empty());
}
#[test]
fn cancellation_and_identity_bound_forget() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Trust);
    act(w, Action::Select("inbound-device-a".into()));
    act(w, Action::Forget);
    assert!(take_commands(w).is_empty());
    act(w, Action::Cancel);
    act(w, Action::Confirm);
    assert!(take_commands(w).is_empty());
    act(w, Action::Forget);
    act(w, Action::Confirm);
    let c = take_commands(w).pop().unwrap();
    assert!(
        matches!(c.request,Request::Forget{device,pairing,..} if device=="device-a"&&pairing=="pair-a")
    );
}
#[test]
fn timeout_blocks_mutations_until_explicit_refresh_and_unlock() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("remote-b".into()));
    act(w, Action::Forget);
    act(w, Action::Confirm);
    let c = take_commands(w).pop().unwrap();
    apply(
        w,
        Update {
            command: c.clone(),
            result: Err("timeout".into()),
        },
    );
    sync(w);
    act(w, Action::Forget);
    act(w, Action::Confirm);
    act(w, Action::Unlock);
    assert!(take_commands(w).is_empty());
    assert!(w.resource::<State>().unwrap().uncertain);
    act(w, Action::Refresh);
    let refresh = take_commands(w).pop().unwrap();
    apply(
        w,
        Update {
            command: refresh,
            result: Ok(snapshot()),
        },
    );
    sync(w);
    assert!(w.resource::<State>().unwrap().uncertain);
    act(w, Action::Unlock);
    assert!(!w.resource::<State>().unwrap().uncertain);
    assert!(take_commands(w).is_empty());
    apply(
        w,
        Update {
            command: c,
            result: Ok(json!({})),
        },
    );
    assert!(!w.resource::<State>().unwrap().uncertain);
}
#[test]
fn policy_validation_and_remote_policy_ownership() {
    let mut a = app();
    let w = a.world_mut();
    act(w, Action::Select("remote-b".into()));
    act(w, Action::Policy);
    assert!(w.resource::<State>().unwrap().pending.is_none());
    act(w, Action::Select("inbound-device-a".into()));
    let s = w.resource::<State>().unwrap().clone();
    set(w, s.expires, "yesterday");
    act(w, Action::Policy);
    assert!(w.resource::<State>().unwrap().pending.is_none());
    set(w, s.expires, "2099-01-01T00:00:00Z");
    set(w, s.idle, "7");
    act(w, Action::Policy);
    act(w, Action::Confirm);
    assert!(matches!(
        take_commands(w).pop().unwrap().request,
        Request::Policy {
            idle: Some(604800),
            ..
        }
    ));
}
#[test]
fn fields_and_ime_survive_redraw_and_surface_switch() {
    let mut a = app();
    let w = a.world_mut();
    let s = w.resource::<State>().unwrap().clone();
    set(w, s.invite, "private draft");
    w.insert(
        s.invite,
        ImePreview {
            text: "pending".into(),
            cursor: None,
        },
    );
    act(w, Action::Close);
    assert!(active(w));
    assert!(!crate::peers::can_deactivate(w));
    w.remove::<ImePreview>(s.invite);
    act(w, Action::Trust);
    act(w, Action::Connect);
    assert_eq!(value(w, s.invite), "private draft");
    assert_eq!(w.resource::<State>().unwrap().invite, s.invite);
    crate::surfaces::select(w, crate::surfaces::SurfaceId::Calendar);
    crate::surfaces::select(w, crate::surfaces::SurfaceId::Peers);
    crate::peers::sync(w);
    assert_eq!(value(w, s.invite), "private draft");
}
#[test]
fn management_routes_hide_thread_and_preserve_draft() {
    let mut a = app();
    let w = a.world_mut();
    crate::peers::sync(w);
    assert!(active(w));
    assert!(!w.has::<Hidden>(w.resource::<State>().unwrap().root));
    act(w, Action::Close);
    crate::peers::sync(w);
    assert!(!active(w));
    assert!(w.has::<Hidden>(w.resource::<State>().unwrap().root));
}
#[test]
#[ignore = "writes deterministic peer management visual fixtures"]
fn export_peer_management_visuals() {
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (name, width, height, light, trust, confirm) in [
        ("connect", 1280, 900, false, false, false),
        ("narrow", 620, 850, false, false, false),
        ("trust", 1280, 900, true, true, false),
        ("confirm", 620, 850, false, true, true),
    ] {
        let mut a = app();
        let w = a.world_mut();
        w.insert_resource(Viewport::new(width as f32, height as f32));
        let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
        w.insert_resource(engine);
        publish_system_appearance(
            w,
            SystemAppearance {
                dark: Some(!light),
                ..Default::default()
            },
        );
        if trust {
            act(w, Action::Trust);
            act(w, Action::Select("inbound-device-a".into()));
        }
        if confirm {
            act(w, Action::Forget);
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
            format!("verification/native-peer-management-{name}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let s = a.world().resource::<State>().unwrap();
        let r = a.world().get::<Rect>(s.root).unwrap();
        assert!(
            r.x >= 0. && r.x + r.w <= width as f32 + 1. && r.y + r.h <= height as f32 + 1.,
            "{name}: {r:?}"
        );
    }
}
