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

/// A Settings control by its spoken name. Other surfaces may have one by
/// the same name (Peers has its own pairing QR), so only Settings counts.
fn named(w: &World, name: &str) -> Option<Entity> {
    let root = state(w).map(|s| s.root);
    w.entities_with::<Semantics>().into_iter().find(|e| {
        w.has::<Focusable>(*e)
            && w.get::<Semantics>(*e).and_then(|s| s.name.as_deref()) == Some(name)
            && root.is_none_or(|r| crate::surfaces::descendant(w, *e, r))
    })
}

fn click(app: &mut App, name: &str) {
    for _ in 0..2 {
        app.update();
    }
    // A read landing mid-click redraws the page; find the control again
    // once it is laid out.
    let mut r = None;
    for _ in 0..4 {
        let e = named(app.world(), name).unwrap_or_else(|| panic!("{name} shown"));
        r = app.world().get::<Rect>(e).copied();
        if r.is_some() {
            break;
        }
        app.update();
    }
    let r = r.unwrap_or_else(|| panic!("{name} laid out"));
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
fn sections_follow_medousa_home() {
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
#[ignore = "writes verification/native-settings-{look,connection,packages,agent,mcp,accounts,external,runtime,sharing,channels}-{dark,light}.png"]
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
        with_state(app.world_mut(), |_, s| {
            s.section = Section::Runtime;
            s.defaults = Some(serde_json::json!({
                "tool_call_mode": "strict",
                "host_turn_bus_mode": "auto",
                "web_search_preferred_provider": "tavily",
                "max_tool_rounds": 24,
                "shell_agent_tools_enabled": true,
                "shell_allowed_binaries": ["git", "rg"],
            }));
            s.open.insert("host_turn_bus_mode");
            s.redraw = true;
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "runtime");
        with_state(app.world_mut(), |_, s| {
            s.section = Section::Sharing;
            let read = sharing_read(true);
            s.shared = read.shared.ok();
            s.devices = read.pairs.ok().map(|p| p.paired_devices);
            s.lan = Some(Ok(sharing::Lan {
                enabled: false,
                message: "LAN pairing is off — engine is loopback-only.".into(),
            }));
            s.qr = Some(sample_qr());
            s.qr_image += 1;
            s.open.insert("pair.qr");
            s.redraw = true;
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "sharing");
        with_state(app.world_mut(), |_, s| {
            s.open.remove("pair.qr");
            s.reading.clear();
            s.workshop = None;
            let mut summary = channels_summary();
            summary.discord.credentials_set = true;
            summary.discord.adapter_running = true;
            s.channels = Some(summary);
            s.open.insert("ch.telegram");
            s.open.insert("ch.telegram.advanced");
            s.seed_channel("telegram");
            s.drafts
                .insert("telegram.token", "123456:ABC-secret".into());
            s.redraw = true;
        });
        rndr_paint::frame(&mut app);
        // Scrolled down to Channels.
        let body = state(app.world()).unwrap().body;
        let scroll = app.world().get::<Parent>(body).unwrap().entity;
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., -760.];
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "channels");
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

fn runtime_open(defaults: serde_json::Value) -> App {
    let mut app = app();
    app.world_mut().insert_resource(Viewport::new(1200., 2400.));
    click(&mut app, "Runtime Controls");
    let w = app.world_mut();
    let read = state(w).unwrap().defaults_reading.expect("read on open");
    apply(
        w,
        Update {
            serial: read,
            result: Ok(Reply::Defaults(defaults)),
        },
    );
    app.update();
    app
}

/// Lets the change in flight land as the workshop wrote it.
fn saved(app: &mut App) {
    let w = app.world_mut();
    let s = state(w).unwrap();
    let (serial, _) = s.patching.clone().expect("a change in flight");
    let written = s.defaults.clone().unwrap();
    apply(
        w,
        Update {
            serial,
            result: Ok(Reply::Defaults(written)),
        },
    );
    app.update();
}

#[test]
fn runtime_controls_pick_toggle_and_step_one_field_at_a_time() {
    let mut app = runtime_open(serde_json::json!({ "slice_hot_window_turns": 9 }));
    let t = texts(app.world());
    assert!(
        t.iter()
            .any(|x| x == "Flexible · When needed · Shell off · Versions off · 30 rounds"),
        "{t:?}"
    );
    click(&mut app, "Tool posture: Flexible");
    click(&mut app, "Careful");
    let s = state(app.world()).unwrap();
    assert_eq!(
        s.patching.as_ref().map(|(_, p)| p.clone()),
        Some(agent::Patch::Field("tool_call_mode", "strict".into()))
    );
    assert!(!s.open.contains("tool_call_mode"), "the picker closes");
    saved(&mut app);
    click(&mut app, "Raise General tool rounds");
    assert_eq!(
        state(app.world()).unwrap().defaults.as_ref().unwrap()["max_tool_rounds"],
        31
    );
    saved(&mut app);
    click(&mut app, "Telemetry export");
    let d = state(app.world()).unwrap().defaults.clone().unwrap();
    assert_eq!(d["stasis_otel_enabled"], true);
    assert_eq!(d["slice_hot_window_turns"], 9, "other fields survive");
    saved(&mut app);
    // Budgets open behind their disclosure.
    assert!(named(app.world(), "Raise Specialist rounds").is_none());
    click(&mut app, "Tool budgets");
    click(&mut app, "Lower Specialist rounds");
    assert_eq!(
        state(app.world()).unwrap().defaults.as_ref().unwrap()["host_bus_max_tool_rounds"],
        7
    );
}

#[test]
fn shell_tools_with_no_allowlist_take_a_second_press() {
    let mut app = runtime_open(serde_json::json!({}));
    click(&mut app, "Agent shell tools");
    let s = state(app.world()).unwrap();
    assert!(s.shell_confirm && s.patching.is_none(), "asks first");
    click(&mut app, "Keep off");
    assert!(!state(app.world()).unwrap().shell_confirm);
    click(&mut app, "Agent shell tools");
    click(&mut app, "Turn on anyway");
    let s = state(app.world()).unwrap();
    assert!(!s.shell_confirm);
    assert_eq!(
        s.defaults.as_ref().unwrap()["shell_agent_tools_enabled"],
        true
    );
    saved(&mut app);
    assert!(
        texts(app.world())
            .iter()
            .any(|t| t.starts_with("Agents are unlocked with an empty binary allowlist"))
    );
    // With binaries listed it turns on at once.
    let mut app = runtime_open(serde_json::json!({ "shell_allowed_binaries": ["git"] }));
    click(&mut app, "Agent shell tools");
    assert!(state(app.world()).unwrap().patching.is_some());
}

fn sample_qr() -> sharing::Qr {
    use base64::Engine;
    let mut png = Vec::new();
    image::RgbaImage::from_fn(21, 21, |x, y| {
        if (x + y) % 2 == 0 {
            image::Rgba([0, 0, 0, 255])
        } else {
            image::Rgba([255, 255, 255, 255])
        }
    })
    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
    .unwrap();
    sharing::decode_qr(serde_json::json!({
        "url": "medousa://pair?invite=abc",
        "expiresAt": (chrono::Utc::now() + chrono::Duration::seconds(125)).to_rfc3339(),
        "shortCode": "abc123xyz",
        "pngBase64": base64::engine::general_purpose::STANDARD.encode(&png),
    }))
    .unwrap()
}

fn sharing_read(shared: bool) -> sharing::Read {
    sharing::Read {
        shared: Ok(serde_json::from_value(serde_json::json!({
            "mode": if shared { "shared" } else { "personal" },
            "root_profile_id": "user:root",
            "general_profile_id": "user:general",
        }))
        .unwrap()),
        pairs: Ok(serde_json::from_value(serde_json::json!({
            "pairedDevices": [{
                "pairingId": "p1", "phoneId": "ph1", "phoneName": "Pixel",
                "pairedAt": "2026-11-01T00:00:00Z", "lastSeen": "2026-11-20T15:04:00Z",
                "trustActive": true
            }]
        }))
        .unwrap()),
        lan: None,
    }
}

fn sharing_open() -> App {
    let mut app = app();
    app.world_mut().insert_resource(Viewport::new(1200., 2000.));
    click(&mut app, "Sharing");
    let w = app.world_mut();
    let read = state(w).unwrap().sharing_reading.expect("read on open");
    apply(
        w,
        Update {
            serial: read,
            result: Ok(Reply::Sharing(Box::new(sharing_read(false)))),
        },
    );
    app.update();
    app
}

#[test]
fn sharing_turns_shared_mode_on_and_lists_paired_phones() {
    let mut app = sharing_open();
    let t = texts(app.world());
    assert!(
        t.iter().any(|x| x == "Personal · phone, peers & channels"),
        "{t:?}"
    );
    assert!(t.iter().any(|x| x == "Pixel"));
    click(&mut app, "Shared mode");
    let busy = state(app.world())
        .unwrap()
        .sharing_busy
        .expect("saved through the worker");
    apply(
        app.world_mut(),
        Update {
            serial: busy,
            result: Ok(Reply::Shared(sharing_read(true).shared.unwrap())),
        },
    );
    app.update();
    let t = texts(app.world());
    assert!(
        t.iter().any(|x| x == "Shared · phone, peers & channels"),
        "{t:?}"
    );
    assert!(
        t.iter()
            .any(|x| x == "Seats via Phone invites · admin user:root")
    );
}

#[test]
fn removing_a_phone_takes_a_second_press_then_reads_again() {
    let mut app = sharing_open();
    click(&mut app, "Remove Pixel");
    let s = state(app.world()).unwrap();
    assert_eq!(s.device_confirm.as_deref(), Some("p1"));
    assert!(s.sharing_busy.is_none(), "asks first");
    click(&mut app, "Keep Pixel");
    assert!(state(app.world()).unwrap().device_confirm.is_none());
    click(&mut app, "Remove Pixel");
    click(&mut app, "Confirm removal of Pixel");
    let busy = state(app.world()).unwrap().sharing_busy.expect("removing");
    apply(
        app.world_mut(),
        Update {
            serial: busy,
            result: Ok(Reply::Done),
        },
    );
    assert!(state(app.world()).unwrap().sharing_reading.is_some());
}

#[test]
fn the_pairing_qr_shows_its_code_and_copies_the_link() {
    let mut app = sharing_open();
    click(&mut app, "Show pairing QR");
    let read = state(app.world())
        .unwrap()
        .qr_reading
        .expect("fetched when shown");
    apply(
        app.world_mut(),
        Update {
            serial: read,
            result: Ok(Reply::Qr(Box::new(sample_qr()))),
        },
    );
    app.update();
    let t = texts(app.world());
    assert!(t.iter().any(|x| x == "ABC-123-XYZ"), "{t:?}");
    assert!(t.iter().any(|x| x.starts_with("Refreshes in 2:0")), "{t:?}");
    click(&mut app, "Copy link");
    assert_eq!(
        app.world().resource::<Clipboard>().unwrap().text,
        "medousa://pair?invite=abc"
    );
    click(&mut app, "Hide pairing QR");
    assert!(!state(app.world()).unwrap().open.contains("pair.qr"));
}

fn channels_summary() -> medousa_desktop_host::channels::Summary {
    let mut s = medousa_desktop_host::channels::Summary::default();
    s.telegram.credentials_set = true;
    s.telegram.allowed_user_ids = vec![7];
    s.discord.command_prefix = "!".into();
    s
}

fn channels_open(kind: &str) -> App {
    let mut app = sharing_open();
    with_state(app.world_mut(), |_, s| {
        s.reading.clear();
        s.workshop = Some(Workshop {
            label: "Studio".into(),
            kind: kind.into(),
            url: "http://127.0.0.1:7878".into(),
            data_dir: None,
        });
        s.read_channels();
        s.redraw = true;
    });
    if let Some(read) = state(app.world()).unwrap().channels_reading {
        apply(
            app.world_mut(),
            Update {
                serial: read,
                result: Ok(Reply::Channels(Box::new(channels_summary()))),
            },
        );
    }
    take_commands(app.world_mut());
    app.update();
    app
}

fn field_text(w: &World, name: &str) -> String {
    let e = named(w, name).unwrap_or_else(|| panic!("{name} shown"));
    w.get::<Label>(e).unwrap().text.clone()
}

fn type_text(app: &mut App, text: &str) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .chars
        .push_str(text);
    app.update();
}

#[test]
fn a_channel_form_keeps_what_was_typed_across_redraws_and_saves_the_token() {
    let mut app = channels_open("local");
    let t = texts(app.world());
    assert!(t.iter().any(|x| x == "1 of 4 ready"), "{t:?}");
    assert!(t.iter().any(|x| x == "DMs, groups, and topics · Ready"));
    assert!(
        named(app.world(), "Bot token").is_none(),
        "forms wait behind Edit"
    );

    click(&mut app, "Edit Telegram");
    assert_eq!(field_text(app.world(), "Your Telegram user ID"), "7");
    assert_eq!(
        field_text(app.world(), "Bot token"),
        "",
        "a kept token isn’t shown"
    );
    click(&mut app, "Bot token");
    type_text(&mut app, "123:abc");
    let token = named(app.world(), "Bot token").unwrap();
    assert!(app.world().has::<Secure>(token), "shown as bullets");
    assert_eq!(field_text(app.world(), "Bot token"), "123:abc");

    // A redraw rebuilds the page; the text and the focus come back.
    with_state(app.world_mut(), |_, s| s.redraw = true);
    app.update();
    let again = named(app.world(), "Bot token").unwrap();
    assert_ne!(again, token, "rebuilt");
    assert_eq!(field_text(app.world(), "Bot token"), "123:abc");
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(again),
        "still typing in it"
    );
    type_text(&mut app, "x");
    assert_eq!(field_text(app.world(), "Bot token"), "123:abcx");

    click(&mut app, "Save Telegram");
    let (busy, id) = state(app.world()).unwrap().channel_busy.expect("saving");
    assert_eq!(id, "telegram");
    // The worker is handed this plan (the app's update dispatches it).
    let s = state(app.world()).unwrap();
    let plan = channels::plan("telegram", &s.drafts, false, &s.forget).unwrap();
    assert_eq!(plan.tokens, [("bot_token", Some("123:abcx".to_string()))]);

    apply(
        app.world_mut(),
        Update {
            serial: busy,
            result: Ok(Reply::Channels(Box::new(channels_summary()))),
        },
    );
    app.update();
    assert_eq!(
        field_text(app.world(), "Bot token"),
        "",
        "cleared once kept"
    );
    let t = texts(app.world());
    assert!(
        t.iter()
            .any(|x| x == "Saved — Medousa starts this channel for you."),
        "{t:?}"
    );
}

#[test]
fn a_bad_id_is_named_and_nothing_is_saved() {
    let mut app = channels_open("local");
    click(&mut app, "Edit Telegram");
    click(&mut app, "Your Telegram user ID");
    type_text(&mut app, ", me");
    click(&mut app, "Save Telegram");
    assert!(state(app.world()).unwrap().channel_busy.is_none());
    let t = texts(app.world());
    assert!(
        t.iter().any(|x| x.starts_with("“me” isn’t a number")),
        "{t:?}"
    );
    // Closing the form drops what was typed.
    click(&mut app, "Close Telegram");
    assert!(state(app.world()).unwrap().drafts.is_empty());
}

#[test]
fn a_workshop_elsewhere_sets_its_channels_up_there() {
    let app = channels_open("remote");
    let s = state(app.world()).unwrap();
    assert!(s.channels.is_none() && s.channels_reading.is_none());
    let t = texts(app.world());
    assert!(
        t.iter()
            .any(|x| x == "This workshop runs on another machine; its channels are set up there."),
        "{t:?}"
    );
}
