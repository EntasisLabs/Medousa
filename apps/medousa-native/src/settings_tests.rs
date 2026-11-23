use super::*;

fn app() -> App {
    let mut app = crate::ui::app();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Settings);
    app.update();
    app
}

fn texts(w: &World) -> Vec<String> {
    w.entities_with::<Label>()
        .into_iter()
        .filter_map(|e| w.get::<Label>(e).map(|l| l.text.clone()))
        .collect()
}

fn named(w: &World, name: &str) -> Option<Entity> {
    w.entities_with::<Semantics>().into_iter().find(|e| {
        w.has::<Focusable>(*e)
            && w.get::<Semantics>(*e).and_then(|s| s.name.as_deref()) == Some(name)
    })
}

fn click(app: &mut App, name: &str) {
    for _ in 0..2 {
        app.update();
    }
    let e = named(app.world(), name).unwrap_or_else(|| panic!("{name} shown"));
    let r = *app.world().get::<Rect>(e).unwrap();
    let at = [r.x + r.w / 2., r.y + r.h / 2.];
    for (pressed, down, released) in [
        (true, true, false),
        (false, false, true),
        (false, false, false),
    ] {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = at;
        if pressed {
            input.pointer.press_at = Some(at);
        }
        input.pointer.pressed = pressed;
        input.pointer.down = down;
        input.pointer.released = released;
        input.pointer.button = PointerButton::Left;
        app.update();
    }
}

#[test]
fn sections_follow_medousa_home_and_unported_ones_say_so() {
    let labels: Vec<_> = Section::ALL.iter().map(|s| s.label()).collect();
    assert_eq!(
        labels,
        [
            "Preferences",
            "Accounts",
            "Medousa Agent",
            "Runtime Controls",
            "Sharing",
            "External Agents",
            "Packages",
            "MCP",
            "Connection",
        ]
    );
    let machine: Vec<_> = Section::ALL
        .iter()
        .filter(|s| s.machine())
        .map(|s| s.label())
        .collect();
    assert_eq!(machine, ["Packages", "MCP", "Connection"]);
    let mut app = app();
    let t = texts(app.world());
    assert!(t.iter().any(|t| t == "Medousa") && t.iter().any(|t| t == "This Mac"));
    // A real click on a section row switches to it.
    click(&mut app, "Sharing");
    assert_eq!(state(app.world()).unwrap().section, Section::Sharing);
    assert!(
        texts(app.world())
            .iter()
            .any(|t| t.starts_with("Coming to this app soon"))
    );
}

#[test]
fn picking_a_theme_applies_it_now_and_saves_it_for_medousa_home() {
    let mut app = app();
    take_commands(app.world_mut());
    // The current theme leads; its picker opens in place.
    assert!(
        named(app.world(), "Nord").is_none(),
        "tiles wait behind Change"
    );
    click(&mut app, "Theme: Obsidian");
    click(&mut app, "Nord");
    let nord = crate::theme::resolve("nord");
    assert_eq!(state(app.world()).unwrap().theme, "nord");
    assert_eq!(
        app.world().resource::<rndr_ui::UiTheme>().unwrap().palette,
        nord.palette,
        "applied without a restart"
    );
    // The app's own update dispatches the save to the worker; it waits on it.
    assert!(
        state(app.world()).unwrap().saving.is_some(),
        "saved to the workshop registry"
    );
    let tile = named(app.world(), "Nord").unwrap();
    assert_eq!(
        app.world().get::<Semantics>(tile).unwrap().description,
        "Selected"
    );
}

#[test]
fn connection_shows_the_workshop_and_the_engine() {
    let mut app = app();
    click(&mut app, "Connection");
    let w = app.world_mut();
    let reads: Vec<u64> = state(w).unwrap().reading.clone();
    assert_eq!(reads.len(), 2, "health and registry reads");
    let health: HealthResponse = serde_json::from_value(serde_json::json!({
        "runtime": {
            "authority_id": format!("auth_{}", "a".repeat(64)),
            "product_version": "0.9.2",
            "build_revision": "abc123",
            "contract_revision": 1,
            "base_schema_revision": 1,
            "deployment_profile": "personal",
            "deployment_target": "desktop",
            "advertised_capabilities": []
        },
        "status": "ok",
        "backend": "sqlite",
        "worker_id": "w1",
        "now_utc": "2026-11-22T00:00:00Z",
        "tool_registry_count": 42,
        "active_profile_display_name": "Dee"
    }))
    .unwrap();
    apply(
        w,
        Update {
            serial: reads[0],
            result: Ok(Reply::Health(Box::new(health))),
        },
    );
    apply(
        w,
        Update {
            serial: reads[1],
            result: Ok(Reply::Workshop(Some(Workshop {
                label: "Studio".into(),
                kind: "local".into(),
                url: "http://127.0.0.1:7878".into(),
                data_dir: None,
            }))),
        },
    );
    app.update();
    let t = texts(app.world());
    assert!(
        !t.iter().any(|x| x == "Dee"),
        "details wait behind their row"
    );
    click(&mut app, "Engine details");
    let t = texts(app.world());
    for expected in ["Studio", "On this Mac", "0.9.2 (abc123)", "Dee", "42", "ok"] {
        assert!(t.iter().any(|x| x == expected), "{expected} in {t:?}");
    }
}

#[test]
fn command_comma_opens_settings_from_anywhere() {
    let mut app = crate::ui::app();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    app.world_mut().insert_resource(KeymapFired {
        actions: vec![OPEN.into()],
    });
    update(app.world_mut());
    assert_eq!(
        crate::surfaces::active(app.world()),
        crate::surfaces::SurfaceId::Settings
    );
}

#[test]
#[ignore = "writes verification/native-settings-{look,connection,packages,agent,mcp,accounts,external}-{dark,light}.png"]
fn settings_visual_acceptance() {
    use std::sync::Arc;
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (dark, suffix) in [(true, "dark"), (false, "light")] {
        let mut app = crate::ui::app();
        app.world_mut()
            .insert_resource(rndr_ui::InteractionMotion::snap());
        app.world_mut().insert_resource(engine.clone());
        app.world_mut().insert_resource(Viewport::new(1400., 900.));
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(dark),
                ..Default::default()
            },
        );
        rndr_paint::frame(&mut app);
        crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Settings);
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        let mut shoot = |app: &mut App, name: &str| {
            let (_, image) = rndr_paint::frame_with_snapshot_frame(app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, 1400, 900, 1.)
                    .unwrap()
            });
            image::save_buffer(
                format!("verification/native-settings-{name}-{suffix}.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        };
        shoot(&mut app, "look");
        with_state(app.world_mut(), |_, s| {
            s.section = Section::Connection;
            s.reading.clear();
            s.workshop = Some(Workshop {
                label: "Studio".into(),
                kind: "local".into(),
                url: "http://127.0.0.1:7878".into(),
                data_dir: Some("~/Library/Application Support/Medousa".into()),
            });
            s.health = serde_json::from_value(serde_json::json!({
                "runtime": {
                    "authority_id": format!("auth_{}", "a".repeat(64)),
                    "product_version": "0.9.2",
                    "build_revision": "abc123",
                    "contract_revision": 1,
                    "base_schema_revision": 1,
                    "deployment_profile": "personal",
                    "deployment_target": "desktop",
                    "advertised_capabilities": []
                },
                "status": "ok",
                "backend": "sqlite",
                "worker_id": "w1",
                "now_utc": "2026-11-22T00:00:00Z",
                "tool_registry_count": 42,
                "last_agent_turn_latency_ms": 840,
                "active_profile_display_name": "Dee"
            }))
            .ok();
            s.redraw = true;
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "connection");
        with_state(app.world_mut(), |_, s| {
            s.section = Section::Packages;
            s.listing = None;
            s.packages = Some(Catalog {
                packages: vec![
                    row("computer-control", "Computer control", false, true),
                    row("coding-runtime", "Coding runtime", false, true),
                    row("local-brain", "Offline brain", true, true),
                    row("channels", "Channel adapters", true, true),
                ],
                installer_available: true,
                release_version: Some("1.0.0".into()),
            });
            s.busy = Some((1, "computer-control".into()));
            *s.progress.lock().unwrap() = Some(Progress {
                package_id: "computer-control".into(),
                display_name: "Computer control".into(),
                phase: "downloading".into(),
                phase_label: "Downloading".into(),
                percent: 40.,
                message: "19 MB of 48 MB".into(),
            });
            s.redraw = true;
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "packages");
        crate::ui::set_models(
            app.world_mut(),
            vec![
                ("anthropic".into(), "claude-opus-5-5".into()),
                ("anthropic".into(), "claude-sonnet-5-5".into()),
                ("openai".into(), "gpt-6".into()),
            ],
        );
        with_state(app.world_mut(), |_, s| {
            s.section = Section::Agent;
            s.busy = None;
            s.defaults_reading = None;
            s.open.insert("model");
            s.open.insert("stance");
            s.defaults = Some(serde_json::json!({
                "active_voice_id": "direct",
                "response_depth_mode": "deep",
                "provider": "anthropic",
                "model": "claude-opus-5-5",
                "inference_profiles": {
                    "main": {"provider": "anthropic", "model": "claude-opus-5-5", "fallbacks": []},
                    "stt": {"provider": "openai", "model": "whisper-2", "fallbacks": []}
                }
            }));
            s.redraw = true;
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "agent");
        with_state(app.world_mut(), |_, s| {
            s.section = Section::Mcp;
            s.mcp_reading = None;
            s.mcp = Some(mcp_status(true));
            s.mcp_confirm = Some("linear".into());
            s.mcp_restart_needed = true;
            s.redraw = true;
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "mcp");
        with_state(app.world_mut(), |_, s| {
            s.section = Section::Accounts;
            s.accounts_reading = None;
            s.accounts =
                Some(Accounts {
                    providers: vec![
                        serde_json::from_value(serde_json::json!({
                            "protocol_version": 1, "id": "github", "label": "GitHub",
                            "hosts": ["github.com"], "operations": ["device_auth"]
                        }))
                        .unwrap(),
                    ],
                    connections: vec![serde_json::from_value(serde_json::json!({
                    "connection_id": format!("conn_{}", "a".repeat(32)), "kind": "github",
                    "label": "octocat", "base_url": "https://github.com",
                    "created_at": "2026-11-01T00:00:00Z", "updated_at": "2026-11-01T00:00:00Z"
                }))
                .unwrap()],
                });
            s.sign_in = Some(SignIn {
                provider: "github".into(),
                flow: "flow-1".into(),
                code: "ABCD-1234".into(),
                url: "https://github.com/login/device".into(),
                interval: 600,
                paused: false,
                copied: false,
                next: std::time::Instant::now() + std::time::Duration::from_secs(600),
            });
            s.redraw = true;
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "accounts");
        with_state(app.world_mut(), |_, s| {
            s.section = Section::ExternalAgents;
            s.sign_in = None;
            s.agents = Some(sample_agents());
            s.agent_note = "Signed in. Ready. In Chat, pick Codex from the runtime control under the composer.".into();
            s.open.insert("conversations");
            s.redraw = true;
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "external");
    }
}

fn row(
    id: &str,
    name: &str,
    installed: bool,
    optional: bool,
) -> medousa_desktop_host::packages::PackageRow {
    medousa_desktop_host::packages::PackageRow {
        id: id.into(),
        display_name: name.into(),
        hint: format!("{name} hint"),
        category_label: "Tools".into(),
        installed,
        installed_version: installed.then(|| "1.0.0".into()),
        available_version: Some("1.0.0".into()),
        update_available: false,
        size_bytes: 48 * 1024 * 1024,
        optional,
    }
}

fn packages_open() -> App {
    let mut app = app();
    click(&mut app, "Packages");
    let w = app.world_mut();
    let listing = state(w).unwrap().listing.expect("catalog read on open");
    apply(
        w,
        Update {
            serial: listing,
            result: Ok(Reply::Packages(Catalog {
                packages: vec![
                    row("computer-control", "Computer control", false, true),
                    row("local-brain", "Offline brain", true, true),
                    row("engine-core", "Engine core", true, false),
                ],
                installer_available: false,
                release_version: Some("1.0.0".into()),
            })),
        },
    );
    app.update();
    app
}

#[test]
fn packages_list_what_is_installed_and_offer_the_rest() {
    let app = packages_open();
    let w = app.world();
    let t = texts(w);
    assert!(
        t.iter()
            .any(|x| x == "Computer control hint · ~48 MB · v1.0.0"),
        "{t:?}"
    );
    assert!(named(w, "Install Computer control").is_some());
    assert!(
        named(w, "Remove Offline brain").is_some(),
        "optional and installed"
    );
    assert!(
        named(w, "Remove Engine core").is_none(),
        "required packages stay"
    );
    assert!(
        !t.iter()
            .any(|x| x.starts_with("Medousa Installer isn’t on this Mac")),
        "repairs wait under Advanced"
    );
    let mut app = app;
    click(&mut app, "Advanced");
    let t = texts(app.world());
    assert!(
        t.iter()
            .any(|x| x.starts_with("Medousa Installer isn’t on this Mac"))
    );
}

#[test]
fn installing_shows_progress_then_reads_the_catalog_again() {
    let mut app = packages_open();
    click(&mut app, "Install Computer control");
    let w = app.world_mut();
    let (serial, id) = state(w).unwrap().busy.clone().expect("install in flight");
    assert_eq!(id, "computer-control");
    // Other installs wait for this one.
    assert!(named(w, "Remove Offline brain").is_some());
    // The worker reports progress through the shared slot.
    *state(w).unwrap().progress.lock().unwrap() = Some(Progress {
        package_id: id.clone(),
        display_name: "Computer control".into(),
        phase: "downloading".into(),
        phase_label: "Downloading".into(),
        percent: 40.,
        message: "19 MB of 48 MB".into(),
    });
    app.update();
    let t = texts(app.world());
    assert!(
        t.iter()
            .any(|x| x == "Computer control · Downloading · 19 MB of 48 MB"),
        "{t:?}"
    );
    assert!(
        app.world().resource::<MotionWatch>().unwrap().active,
        "frames keep coming"
    );
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::Done),
        },
    );
    let s = state(app.world()).unwrap();
    assert!(s.busy.is_none());
    assert!(
        s.listing.is_some(),
        "installed state is read from disk again"
    );
}

fn agent_open() -> App {
    let mut app = app();
    crate::ui::set_models(
        app.world_mut(),
        vec![
            ("anthropic".into(), "claude-opus-5-5".into()),
            ("openai".into(), "gpt-6".into()),
        ],
    );
    click(&mut app, "Medousa Agent");
    let w = app.world_mut();
    let read = state(w).unwrap().defaults_reading.expect("read on open");
    apply(
        w,
        Update {
            serial: read,
            result: Ok(Reply::Defaults(serde_json::json!({
                "active_voice_id": "default",
                "response_depth_mode": "standard",
                "provider": "anthropic",
                "model": "claude-opus-5-5",
                "inference_profiles": {"main": {"provider": "anthropic", "model": "claude-opus-5-5", "fallbacks": []}}
            }))),
        },
    );
    app.update();
    app
}

#[test]
fn stance_and_depth_save_to_the_workshop_defaults() {
    let mut app = agent_open();
    let t = texts(app.world());
    assert!(
        t.iter()
            .any(|x| x == "Default · Standard · claude-opus-5-5"),
        "{t:?}"
    );
    click(&mut app, "Stance: Default");
    click(&mut app, "Direct");
    let s = state(app.world()).unwrap();
    assert_eq!(
        agent::stance(s.defaults.as_ref().unwrap()),
        "direct",
        "shown at once"
    );
    let (serial, patch) = s.patching.clone().expect("saved through the worker");
    assert_eq!(patch, agent::Patch::Stance("direct".into()));
    // A failed write re-reads what the record really holds.
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Err("409".into()),
        },
    );
    let s = state(app.world()).unwrap();
    assert!(s.message.starts_with("That change wasn’t saved"));
    assert!(s.defaults_reading.is_some());
}

#[test]
fn the_conversation_model_is_picked_from_the_engine_models() {
    let mut app = agent_open();
    click(&mut app, "Conversation model: claude-opus-5-5");
    click(&mut app, "gpt-6 · openai");
    let s = state(app.world()).unwrap();
    assert_eq!(
        agent::role(s.defaults.as_ref().unwrap(), "main"),
        Some(("openai".into(), "gpt-6".into()))
    );
    assert!(!s.open.contains("model"), "the list closes on a pick");
    assert!(matches!(s.patching, Some((_, agent::Patch::Model(..)))));
}

fn mcp_status(local: bool) -> medousa_desktop_host::mcp::Status {
    use medousa_desktop_host::mcp::{Server, Status};
    Status {
        gateway_url: "http://127.0.0.1:7420".into(),
        reachable: true,
        message: "ok".into(),
        connected_servers: Some(1),
        catalog_entries: Some(12),
        servers: vec![
            Server {
                id: "github".into(),
                title: "GitHub".into(),
                enabled: true,
                connected: true,
                tool_count: 12,
            },
            Server {
                id: "linear".into(),
                title: "Linear".into(),
                enabled: false,
                connected: false,
                tool_count: 0,
            },
        ],
        local,
        config_path: "~/.config/medousa/mcp-gateway.toml".into(),
    }
}

fn mcp_open(local: bool) -> App {
    let mut app = app();
    click(&mut app, "MCP");
    let w = app.world_mut();
    let read = state(w).unwrap().mcp_reading.expect("status read on open");
    apply(
        w,
        Update {
            serial: read,
            result: Ok(Reply::Mcp(Box::new(mcp_status(local)))),
        },
    );
    app.update();
    app
}

#[test]
fn mcp_lists_servers_and_edits_wait_for_a_restart() {
    let mut app = mcp_open(true);
    let t = texts(app.world());
    for expected in [
        "Running · 1 connected · 12 tools",
        "github · Connected · 12 tools",
        "linear · Off",
    ] {
        assert!(t.iter().any(|x| x == expected), "{expected} in {t:?}");
    }
    click(&mut app, "Turn off GitHub");
    let (serial, restart) = state(app.world())
        .unwrap()
        .mcp_busy
        .expect("edit in flight");
    assert!(!restart);
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::McpDone("Server disabled".into())),
        },
    );
    let s = state(app.world()).unwrap();
    assert!(s.mcp_restart_needed);
    assert!(s.mcp_reading.is_some(), "status read again");
    app.update();
    assert!(
        texts(app.world())
            .iter()
            .any(|x| x == "Restart the gateway to apply your changes.")
    );
}

#[test]
fn removing_a_server_takes_a_second_press() {
    let mut app = mcp_open(true);
    click(&mut app, "Remove Linear");
    assert_eq!(
        state(app.world()).unwrap().mcp_confirm.as_deref(),
        Some("linear")
    );
    assert!(
        state(app.world()).unwrap().mcp_busy.is_none(),
        "nothing removed yet"
    );
    click(&mut app, "Remove Linear");
    let s = state(app.world()).unwrap();
    assert!(s.mcp_busy.is_some());
    assert!(s.mcp_confirm.is_none());
}

#[test]
fn a_workshop_elsewhere_manages_its_own_servers() {
    let app = mcp_open(false);
    let w = app.world();
    assert!(named(w, "Turn off GitHub").is_none());
    assert!(named(w, "Restart gateway").is_none());
    assert!(
        texts(w)
            .iter()
            .any(|x| x == "This workshop manages its own MCP servers.")
    );
}

fn accounts_open() -> App {
    let mut app = app();
    click(&mut app, "Accounts");
    let w = app.world_mut();
    let read = state(w)
        .unwrap()
        .accounts_reading
        .expect("accounts read on open");
    let accounts = Accounts {
        providers: vec![
            serde_json::from_value(serde_json::json!({
                "protocol_version": 1,
                "id": "github",
                "label": "GitHub",
                "hosts": ["github.com"],
                "operations": ["device_auth", "list"]
            }))
            .unwrap(),
        ],
        connections: vec![
            serde_json::from_value(serde_json::json!({
                "connection_id": format!("conn_{}", "a".repeat(32)),
                "kind": "github",
                "label": "octocat",
                "base_url": "https://github.com",
                "created_at": "2026-11-01T00:00:00Z",
                "updated_at": "2026-11-01T00:00:00Z"
            }))
            .unwrap(),
        ],
    };
    apply(
        w,
        Update {
            serial: read,
            result: Ok(Reply::Accounts(Box::new(accounts))),
        },
    );
    app.update();
    app
}

#[test]
fn accounts_sign_in_with_a_device_code_and_reread_when_approved() {
    use medousa_types::{ReviewProviderAuthRequest as Q, ReviewProviderAuthResponse as R};
    let mut app = accounts_open();
    let t = texts(app.world());
    assert!(
        t.iter().any(|x| x == "octocat") && t.iter().any(|x| x == "github.com"),
        "{t:?}"
    );
    click(&mut app, "Sign in with GitHub");
    let (serial, who) = state(app.world())
        .unwrap()
        .account_busy
        .clone()
        .expect("begin sent");
    assert_eq!(who, "github");
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::Auth(R::Pending {
                flow_id: "flow-1".into(),
                user_code: "ABCD-1234".into(),
                verification_uri: "https://github.com/login/device".into(),
                expires_in: 900,
                interval: 5,
            })),
        },
    );
    // The page opens once, with the code shown to type there.
    let commands = take_commands(app.world_mut());
    assert!(
        commands
            .iter()
            .any(|c| matches!(&c.kind, Kind::OpenUrl(u) if u == "https://github.com/login/device"))
    );
    app.update();
    assert!(texts(app.world()).iter().any(|x| x == "ABCD-1234"));
    click(&mut app, "Copy code");
    assert_eq!(
        app.world().resource::<Clipboard>().unwrap().text,
        "ABCD-1234"
    );
    // Checked at the provider's interval.
    state_mut(app.world_mut())
        .unwrap()
        .sign_in
        .as_mut()
        .unwrap()
        .next = std::time::Instant::now();
    sync(app.world_mut());
    let (serial, _) = state(app.world())
        .unwrap()
        .account_busy
        .clone()
        .expect("check sent");
    let commands = take_commands(app.world_mut());
    assert!(
        commands
            .iter()
            .any(|c| matches!(&c.kind, Kind::Auth(Q::Poll { flow_id }) if flow_id == "flow-1"))
    );
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::Auth(R::Connected {
                connection_id: format!("conn_{}", "b".repeat(32)),
                account: "octocat".into(),
            })),
        },
    );
    let s = state(app.world()).unwrap();
    assert!(s.sign_in.is_none());
    assert_eq!(s.notice, "Connected as octocat.");
    assert!(s.accounts_reading.is_some(), "accounts read again");
}

#[test]
fn a_failed_check_waits_for_retry_and_accounts_disconnect() {
    let mut app = accounts_open();
    state_mut(app.world_mut()).unwrap().sign_in = Some(SignIn {
        provider: "github".into(),
        flow: "flow-1".into(),
        code: "ABCD-1234".into(),
        url: "https://github.com/login/device".into(),
        interval: 5,
        paused: false,
        copied: false,
        next: std::time::Instant::now(),
    });
    sync(app.world_mut());
    let (serial, _) = state(app.world()).unwrap().account_busy.clone().unwrap();
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Err("connection reset".into()),
        },
    );
    assert!(state(app.world()).unwrap().sign_in.as_ref().unwrap().paused);
    sync(app.world_mut());
    assert!(
        state(app.world()).unwrap().account_busy.is_none(),
        "no repeat until Retry"
    );
    click(&mut app, "Cancel");
    assert!(state(app.world()).unwrap().sign_in.is_none());
    click(&mut app, "Disconnect octocat");
    let (_, id) = state(app.world())
        .unwrap()
        .account_busy
        .clone()
        .expect("disconnect sent");
    assert!(id.starts_with("conn_"));
}

#[test]
fn only_https_pages_are_opened() {
    assert!(open_url("file:///etc/passwd").is_err());
    assert!(open_url("javascript:alert(1)").is_err());
}

fn runtime(id: &str, present: bool, auth: Option<&str>) -> medousa_types::AgentRuntimeInfo {
    serde_json::from_value(serde_json::json!({
        "runtime": id,
        "available": present,
        "binary_present": present,
        "auth_status": auth,
    }))
    .unwrap()
}

fn conversation(
    id: &str,
    provider: &str,
    label: &str,
    target: &str,
) -> medousa_types::ExternalConversationView {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "provider": provider,
        "label": label,
        "target": target,
        "created_at": "2026-11-01T00:00:00Z",
        "updated_at": "2026-11-01T00:00:00Z",
        "events": []
    }))
    .unwrap()
}

fn sample_agents() -> external::Agents {
    external::Agents {
        runtimes: vec![
            runtime("codex", true, Some("signed_in")),
            runtime("cursor", true, Some("signed_out")),
            runtime("claude", true, None),
        ],
        conversations: vec![
            conversation("grok-1", "grok_bot", "Ops bot", "opsbot"),
            conversation(
                "inst-1",
                "instinct",
                "Instinct",
                "15551234567@s.whatsapp.net",
            ),
        ],
        conversations_error: None,
    }
}

fn agents_reply(app: &mut App, agents: external::Agents) {
    let w = app.world_mut();
    let serial = state(w).unwrap().agents_reading.expect("agents read");
    apply(
        w,
        Update {
            serial,
            result: Ok(Reply::Agents(Box::new(agents))),
        },
    );
    app.update();
}

fn external_open(kind: &str) -> App {
    let mut app = app();
    click(&mut app, "External Agents");
    with_state(app.world_mut(), |_, s| {
        s.reading.clear();
        s.workshop = Some(Workshop {
            label: "Studio".into(),
            kind: kind.into(),
            url: "http://127.0.0.1:7878".into(),
            data_dir: None,
        });
    });
    agents_reply(&mut app, sample_agents());
    take_commands(app.world_mut());
    app
}

#[test]
fn external_agents_lead_with_what_can_be_set_up() {
    let mut app = external_open("local");
    let t = texts(app.world());
    for shown in [
        "Codex",
        "Cursor",
        "Hermes",
        "Claude Code",
        "Signed in",
        "Signed out",
    ] {
        assert!(t.iter().any(|x| x == shown), "{shown} in {t:?}");
    }
    // Agents not on this workshop wait behind a disclosure.
    assert!(!t.iter().any(|x| x == "Devin"));
    assert!(named(app.world(), "Open Chat with Codex").is_some());
    click(&mut app, "More agents");
    assert!(texts(app.world()).iter().any(|x| x == "Devin"));
    // Hermes isn't installed: Install runs the vendor's installer.
    click(&mut app, "Install Hermes");
    let (serial, who) = state(app.world()).unwrap().agent_busy.clone().unwrap();
    assert_eq!(who, "hermes", "sent");
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::AgentNote(
                "Hermes Agent CLI installed — you can sign in now.".into(),
            )),
        },
    );
    let s = state(app.world()).unwrap();
    assert!(s.agents_reading.is_some(), "read again after install");
    assert!(s.agent_note.starts_with("Hermes Agent CLI installed"));
}

#[test]
fn signing_in_waits_for_the_cli_to_report_it() {
    let mut app = external_open("local");
    click(&mut app, "Sign in to Cursor");
    let (serial, who) = state(app.world()).unwrap().agent_busy.clone().unwrap();
    assert_eq!(who, "cursor", "sent");
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::SigningIn(
                "Sign in in the terminal that opened.".into(),
            )),
        },
    );
    assert!(state(app.world()).unwrap().waiting.is_some());
    app.update();
    assert!(texts(app.world()).iter().any(|x| x == "Waiting…"));
    // Read again on the wait's interval, until the CLI says signed in.
    state_mut(app.world_mut())
        .unwrap()
        .waiting
        .as_mut()
        .unwrap()
        .next = std::time::Instant::now();
    sync(app.world_mut());
    let mut signed = sample_agents();
    signed.runtimes[1] = runtime("cursor", true, Some("signed_in"));
    agents_reply(&mut app, signed);
    let s = state(app.world()).unwrap();
    assert!(s.waiting.is_none());
    assert!(
        s.agent_note
            .starts_with("Signed in. Ready. In Chat, pick Cursor")
    );
    assert!(named(app.world(), "Sign out Cursor").is_some());

    // A wait that runs out says how to finish.
    with_state(app.world_mut(), |_, s| {
        let past = std::time::Instant::now() - std::time::Duration::from_secs(1);
        s.waiting = Some(Waiting {
            account: "chatgpt",
            until: past,
            next: past,
        });
    });
    sync(app.world_mut());
    let s = state(app.world()).unwrap();
    assert!(s.waiting.is_none());
    assert!(s.agent_note.starts_with("Still waiting for sign-in"));
}

#[test]
fn a_workshop_elsewhere_sets_agents_up_on_its_host() {
    let app = external_open("remote");
    let t = texts(app.world());
    assert!(
        t.iter()
            .any(|x| x.starts_with("This workshop runs on another machine"))
    );
    for setup in ["Install Hermes", "Sign in to Cursor", "Sign out Codex"] {
        assert!(named(app.world(), setup).is_none(), "{setup} hidden");
    }
    assert!(named(app.world(), "Open Chat with Codex").is_some());
}

#[test]
fn conversations_remove_on_a_second_press_and_rotate_shows_the_key_once() {
    let mut app = external_open("local");
    // Tall enough that the conversations sit on screen.
    app.world_mut().insert_resource(Viewport::new(1200., 1600.));
    assert!(
        named(app.world(), "Remove Ops bot").is_none(),
        "behind a disclosure"
    );
    click(&mut app, "Muse, Instinct, Dots & Grok");
    let t = texts(app.world());
    assert!(t.iter().any(|x| x == "WhatsApp · +15551234567"), "{t:?}");
    assert!(t.iter().any(|x| x == "Bot · opsbot"));
    click(&mut app, "Remove Ops bot");
    assert!(
        state(app.world()).unwrap().agent_busy.is_none(),
        "asks first"
    );
    click(&mut app, "Confirm removal of Ops bot");
    let (serial, who) = state(app.world()).unwrap().agent_busy.clone().unwrap();
    assert_eq!(who, "grok-1", "sent");
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::Done),
        },
    );
    assert!(state(app.world()).unwrap().agents_reading.is_some());
    agents_reply(&mut app, sample_agents());

    click(&mut app, "Rotate callback key for Ops bot");
    let (serial, who) = state(app.world()).unwrap().agent_busy.clone().unwrap();
    assert_eq!(who, "grok-1");
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::CallbackKey("cb-secret".into())),
        },
    );
    app.update();
    assert!(texts(app.world()).iter().any(|x| x == "cb-secret"));
    click(&mut app, "Copy key");
    assert_eq!(
        app.world().resource::<Clipboard>().unwrap().text,
        "cb-secret"
    );
    click(&mut app, "Done");
    assert!(state(app.world()).unwrap().callback_key.is_none());
    assert!(!texts(app.world()).iter().any(|x| x == "cb-secret"));
}
