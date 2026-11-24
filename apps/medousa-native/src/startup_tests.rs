//! Hermetic configured-launch regressions. Saved registry and native preferences
//! are temporary fixtures; no live registry, credential store or daemon is used.
use super::*;
use crate::{routing::SessionKey, surfaces::SurfaceId, workshops};
use medousa_desktop_host::{connections, workshop_registry as saved};
use medousa_sdk::MedousaClient;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn registry() -> saved::WorkshopRegistry {
    let mut registry = saved::default_registry();
    let mut remote = saved::default_personal_workshop();
    remote.id = "studio".into();
    remote.label = "Studio".into();
    remote.kind = "portal".into();
    remote.url = "https://studio.invalid".into();
    remote.data_dir = None;
    remote.bind = None;
    remote.client_state = Some(saved::WorkshopClientState {
        last_session_id: Some("kept-chat".into()),
        color_theme_id: Some("catppuccin-mocha".into()),
    });
    // These are inert pairing references, never resolved against a keychain.
    remote.pairing = Some(serde_json::from_value(serde_json::json!({
        "workshopDeviceId": "fixture-device", "pairingId": "fixture-pairing",
        "phoneId": "fixture-client", "pairedAt": "2026-11-24T00:00:00Z", "credentialsRelPath": "fixture/credentials.json"
    })).unwrap());
    registry.workshops.push(remote);
    registry.active_workshop_id = "studio".into();
    registry
}
fn launch(path: &std::path::Path, registry: saved::WorkshopRegistry) -> App {
    let mut app = super::app();
    app.world_mut().insert_resource(Viewport::new(1200., 800.));
    workshops::restore_fixture(app.world_mut(), path.to_path_buf(), registry);
    app.update();
    app
}
fn key(owner: &str) -> String {
    SessionKey::new(owner, "kept-chat").handle()
}
fn connected(
    app: &mut App,
    id: &str,
    owner: &str,
    client: Arc<MedousaClient>,
) -> tokio::sync::mpsc::UnboundedReceiver<Request> {
    let rx = workshops::connected_fixture(app.world_mut(), id, owner, client);
    for _ in 0..3 {
        app.update();
    }
    rx
}
fn no_chrome(w: &mut World) {
    for (_, label) in w.query::<Label>() {
        let text = &label.text;
        assert!(
            !text.contains("New chat destination:")
                && !text.contains("Choose a workshop")
                && !text.contains("Workshops / setup"),
            "unexpected persistent source chrome: {text}"
        );
    }
}
// A real loopback HTTP endpoint exercises the production SDK + auxiliary settings
// request path. It does not stand in for a signed LAN/Iroh pairing handshake.
fn server() -> (Arc<MedousaClient>, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = Arc::new(MedousaClient::new(format!(
        "http://{}",
        listener.local_addr().unwrap()
    )));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            let Ok((mut socket, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut buffer = [0; 8192];
            let count = socket.read(&mut buffer).unwrap_or(0);
            if count == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buffer[..count]);
            let line = request.lines().next().unwrap_or("").to_string();
            log.lock().unwrap().push(line.clone());
            let body = if line.starts_with("GET /health ") {
                serde_json::json!({"status": "ok", "apiVersion": "v1"})
            } else if line.starts_with("GET /v1/health ") {
                serde_json::json!({"runtime": {"authority_id": format!("auth_{}", "a".repeat(64)), "product_version": "fixture", "build_revision": "fixture", "contract_revision": 1, "base_schema_revision": 1, "deployment_profile": "personal", "deployment_target": "desktop", "advertised_capabilities": []}, "status": "ok", "backend": "sqlite", "worker_id": "fixture", "now_utc": "2026-11-24T00:00:00Z", "tool_registry_count": 12})
            } else if line.contains("/v1/runtime/tui-defaults") {
                serde_json::json!({"stance": "direct", "depth_charter": "deep", "provider": "preserved-provider", "model": "preserved-model", "unrelated_setting": 42})
            } else { serde_json::json!({}) }.to_string();
            let _ = write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        }
    });
    (client, requests)
}

#[test]
fn configured_remote_launch_restores_history_settings_and_new_chat_without_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("native-sources.json");
    let registry = registry();
    let saved_bytes = serde_json::to_vec(&registry).unwrap();
    let registry_path = tmp.path().join("workshops.json");
    std::fs::write(&registry_path, &saved_bytes).unwrap();
    let plan = connections::resolve(&registry, "studio", tmp.path()).unwrap();
    assert!(matches!(
        plan.credentials,
        connections::Credentials::Paired { .. }
    ));
    let mut app = launch(&path, registry);
    let (client, requests) = server();
    // A faster Personal connection must not steal an unresolved remote default.
    let _local = connected(&mut app, "personal", "owner-personal", client.clone());
    assert!(workshops::owner(app.world(), None).is_none());
    let mut remote = connected(&mut app, "studio", "owner-studio", client);
    assert_eq!(
        workshops::owner(app.world(), None).as_deref(),
        Some("owner-studio")
    );
    assert!(app.world().resource::<Model>().unwrap().commands.is_some());
    assert_eq!(
        workshops::binding(app.world(), SurfaceId::Settings)
            .unwrap()
            .authority,
        "owner-studio"
    );
    apply_reply(
        app.world_mut(),
        Reply {
            generation: u64::MAX - 1,
            update: Update::SessionsPage {
                query: String::new(),
                rows: vec![(key("owner-studio"), "Existing conversation".into())],
                summaries: vec![],
                cursor: None,
                append: false,
            },
        },
    );
    assert_eq!(
        app.world().resource::<Model>().unwrap().sessions[0].0,
        key("owner-studio")
    );
    no_chrome(app.world_mut());
    conversations::open(app.world_mut(), None);
    assert!(matches!(remote.try_recv().unwrap().command, Command::New));
    conversations::open(app.world_mut(), Some(key("owner-studio")));
    assert!(matches!(
        remote.try_recv().unwrap().command,
        Command::Open(_)
    ));
    crate::surfaces::select(app.world_mut(), SurfaceId::Settings);
    crate::settings::show_connection_fixture(app.world_mut());
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        app.update();
        if crate::settings::workshop_label_fixture(app.world()) == Some("Studio".into()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        crate::settings::workshop_label_fixture(app.world()).as_deref(),
        Some("Studio")
    );
    crate::settings::read_defaults_fixture(app.world_mut());
    while Instant::now() < deadline {
        app.update();
        if crate::settings::defaults_fixture(app.world()).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let defaults = crate::settings::defaults_fixture(app.world()).unwrap();
    assert_eq!(defaults["provider"], "preserved-provider");
    assert_eq!(defaults["unrelated_setting"], 42);
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.contains("/v1/runtime/tui-defaults"))
    );
    let recorded = requests.lock().unwrap().clone();
    assert!(
        recorded.iter().all(|r| r.starts_with("GET ")),
        "restoring settings must not write defaults or pairing state: {recorded:?}"
    );
    assert_eq!(std::fs::read(registry_path).unwrap(), saved_bytes);
    for surface in [
        SurfaceId::Chat,
        SurfaceId::Notes,
        SurfaceId::Create,
        SurfaceId::Reviews,
        SurfaceId::Code,
        SurfaceId::Calendar,
        SurfaceId::Peers,
    ] {
        crate::surfaces::select(app.world_mut(), surface);
        app.update();
        // Manager widgets may remain in the hidden Settings tree, never in a
        // view's layout or focus route; stale source commands must not work.
        assert!(!workshops::action(
            app.world_mut(),
            &HostIntent::Command(0x5753_3000)
        ));
    }
    assert_eq!(
        workshops::owner(app.world(), Some(&key("owner-studio"))).as_deref(),
        Some("owner-studio")
    );
}

#[test]
fn configured_offline_remote_stays_pinned_and_does_not_borrow_healthy_personal() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("native-sources.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&crate::source_preferences::Preferences {
            aliases: [("studio".into(), "owner-studio".into())].into(),
            ..Default::default()
        })
        .unwrap(),
    )
    .unwrap();
    let mut app = launch(&path, registry());
    workshops::fail_fixture(app.world_mut(), "studio");
    let (client, _) = server();
    let mut local = connected(&mut app, "personal", "owner-personal", client);
    assert_eq!(
        workshops::owner(app.world(), None).as_deref(),
        Some("owner-studio")
    );
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .chat
            .status
            .contains("unavailable")
    );
    assert!(!app.world().resource::<Model>().unwrap().loading);
    assert!(workshops::commands(app.world(), "owner-studio").is_none());
    assert_eq!(
        workshops::binding(app.world(), SurfaceId::Notes)
            .unwrap()
            .authority,
        "owner-studio"
    );
    conversations::open(app.world_mut(), None);
    assert!(
        local.try_recv().is_err(),
        "offline new chat must not write to Personal"
    );
    no_chrome(app.world_mut());
}

#[test]
fn persisted_view_preferences_override_migration_seed_without_changing_existing_owners() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("native-sources.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&crate::source_preferences::Preferences {
            new_chat: Some("owner-not-default".into()),
            views: [("Notes".into(), "owner-notes".into())].into(),
            aliases: [("studio".into(), "owner-studio".into())].into(),
        })
        .unwrap(),
    )
    .unwrap();
    let mut app = launch(&path, registry());
    assert_eq!(
        workshops::owner(app.world(), None).as_deref(),
        Some("owner-not-default")
    );
    assert_eq!(
        workshops::binding(app.world(), SurfaceId::Notes)
            .unwrap()
            .authority,
        "owner-notes"
    );
    assert_eq!(
        workshops::binding(app.world(), SurfaceId::Create)
            .unwrap()
            .authority,
        "owner-studio"
    );
    assert_eq!(
        workshops::owner(app.world(), Some(&key("owner-retained"))).as_deref(),
        Some("owner-retained")
    );
    no_chrome(app.world_mut());
}

#[test]
fn configured_personal_launch_seeds_defaults_without_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let mut app = launch(
        &tmp.path().join("native-sources.json"),
        saved::default_registry(),
    );
    let (client, _) = server();
    let mut commands = connected(&mut app, "personal", "owner-personal", client);
    assert_eq!(
        workshops::owner(app.world(), None).as_deref(),
        Some("owner-personal")
    );
    assert_eq!(
        workshops::binding(app.world(), SurfaceId::Notes)
            .unwrap()
            .authority,
        "owner-personal"
    );
    conversations::open(app.world_mut(), None);
    assert!(matches!(commands.try_recv().unwrap().command, Command::New));
    no_chrome(app.world_mut());
}

#[test]
fn missing_prior_identity_never_chooses_the_only_healthy_source() {
    let tmp = tempfile::tempdir().unwrap();
    let mut registry = registry();
    registry.active_workshop_id = "missing".into();
    let mut app = launch(&tmp.path().join("native-sources.json"), registry);
    let (client, _) = server();
    let mut commands = connected(&mut app, "personal", "owner-personal", client);
    assert!(workshops::owner(app.world(), None).is_none());
    conversations::open(app.world_mut(), None);
    assert!(commands.try_recv().is_err());
}

#[test]
fn corrupt_native_preferences_are_not_replaced_by_successful_connection() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("native-sources.json");
    std::fs::write(&path, b"preserve corrupt configuration").unwrap();
    let mut app = launch(&path, registry());
    let (client, _) = server();
    let _remote = connected(&mut app, "studio", "owner-studio", client);
    assert!(workshops::owner(app.world(), None).is_none());
    assert_eq!(
        std::fs::read(path).unwrap(),
        b"preserve corrupt configuration"
    );
}
