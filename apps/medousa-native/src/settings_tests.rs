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
#[ignore = "writes verification/native-settings-{look,connection,packages,agent,providers,agent-more,mcp,mcp-add,accounts,external,conversation-add,runtime,runtime-lists,runtime-more,sharing,channels,providers}-{dark,light}.png"]
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
            s.open.remove("model");
            s.open.remove("stance");
            s.open.insert("providers");
            s.providers = Some(providers_read(true));
            s.provider_open = Some("ollama".into());
            let e = providers::find("ollama").unwrap();
            let fields = providers::fill(s.providers.as_ref().unwrap(), &e);
            s.drafts.extend(fields);
            s.redraw = true;
        });
        rndr_paint::frame(&mut app);
        let body = state(app.world()).unwrap().body;
        let scroll = app.world().get::<Parent>(body).unwrap().entity;
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., -420.];
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "providers");
        with_state(app.world_mut(), |_, s| {
            s.open.remove("providers");
            s.provider_open = None;
            s.open.insert("coding");
            s.open.insert("modes");
            s.open.insert("artifacts");
            if let Some(d) = s.defaults.as_mut() {
                d["coding_runtime"] =
                    serde_json::json!({"preferred": "codex", "fallbacks": ["cursor", "medousa"]});
            }
            s.mode_policy = Some(Ok(medousa_types::AgentModeTransitionPolicy {
                proposal_ttl_seconds: 300,
                auto_accept: medousa_types::AgentModeAutoAccept::Task,
            }));
            s.artifacts = Some(Ok(serde_json::from_value(serde_json::json!({
                "settings": {"enabled": true, "max_age_days": 90, "max_per_session": 200,
                    "recurring_id": "r", "cron_expr": "0 3 * * *"},
                "scheduled": true, "enabled": true,
                "last_run_summary": "Removed 14 artifacts"
            }))
            .unwrap()));
            s.redraw = true;
        });
        rndr_paint::frame(&mut app);
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., -560.];
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "agent-more");
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., 0.];
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., 0.];
        with_state(app.world_mut(), |_, s| {
            s.section = Section::Mcp;
            s.mcp_reading = None;
            s.mcp = Some(mcp_status(true));
            s.mcp_confirm = Some("linear".into());
            s.mcp_restart_needed = true;
            s.mcp_oauth.insert(
                "github".into(),
                serde_json::from_value(serde_json::json!({
                    "server_id": "github", "status": "connected", "connected": true
                }))
                .unwrap(),
            );
            s.mcp_tools_open = Some("github".into());
            s.mcp_tools = Some((
                vec![
                    medousa_desktop_host::mcp::Tool {
                        name: "list_issues".into(),
                        title: "List issues".into(),
                        enabled: true,
                        available: true,
                        hints: vec!["bugs".into(), "tickets".into()],
                    },
                    medousa_desktop_host::mcp::Tool {
                        name: "merge_pull_request".into(),
                        title: "Merge pull request".into(),
                        enabled: false,
                        available: false,
                        hints: vec![],
                    },
                ],
                String::new(),
            ));
            s.redraw = true;
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "mcp");
        with_state(app.world_mut(), |_, s| {
            s.mcp_confirm = None;
            s.mcp_restart_needed = false;
            s.mcp_tools_open = None;
            s.mcp_form = Some(McpForm {
                transport: medousa_desktop_host::mcp::Transport::Http,
                ..McpForm::default()
            });
            s.drafts.insert("mcp.id", "linear".into());
            s.drafts.insert("mcp.title", "Linear".into());
            s.drafts
                .insert("mcp.url", "https://mcp.linear.app/mcp".into());
            s.drafts.insert("mcp.token", "lin_api_secret".into());
            s.redraw = true;
        });
        rndr_paint::frame(&mut app);
        let body = state(app.world()).unwrap().body;
        let scroll = app.world().get::<Parent>(body).unwrap().entity;
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., -300.];
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "mcp-add");
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., 0.];
        with_state(app.world_mut(), |_, s| {
            s.mcp_form = None;
            s.forget_fields(&MCP_FIELDS);
        });
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
            use medousa_types::{
                ExternalWhatsAppPairingState as Wa, ExternalWhatsAppPairingStatus,
            };
            // A checkerboard stands in for WhatsApp's QR.
            let mut cells = String::new();
            for y in 0..21 {
                for x in 0..21 {
                    if (x * 7 + y * 3) % 5 < 2 {
                        cells.push_str(&format!(r#"<rect x="{x}" y="{y}" width="1" height="1"/>"#));
                    }
                }
            }
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21">{cells}</svg>"#
            );
            let mut form = external::ConvForm::new(medousa_types::ExternalProvider::Muse);
            form.pairing = Some(external::PairingRead {
                pairing: Ok(external::pairing(ExternalWhatsAppPairingStatus {
                    state: Wa::QrReady,
                    qr_svg: Some(svg),
                    expires_at: Some(chrono::Utc::now() + chrono::Duration::seconds(60)),
                })),
                adapter_error: None,
            });
            form.discovery = Some(medousa_types::ExternalMuseDiscoveryStatus {
                challenge: "MDS-7Q4K".into(),
                observed_chat_jid: None,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(300),
            });
            s.conv_form = Some(form);
            s.wa_image += 1;
            s.agent_note.clear();
            s.drafts.insert("conv.label", "Muse".into());
            s.redraw = true;
        });
        rndr_paint::frame(&mut app);
        let body = state(app.world()).unwrap().body;
        let scroll = app.world().get::<Parent>(body).unwrap().entity;
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., -900.];
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "conversation-add");
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., 0.];
        with_state(app.world_mut(), |_, s| {
            s.conv_form = None;
            s.forget_fields(&external::CONV_FIELDS);
        });
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
            s.open.remove("host_turn_bus_mode");
            s.open.insert("runtime.allowlists");
            s.open.insert("runtime.env");
            if let Some(d) = s.defaults.as_mut() {
                d["env_overrides"] = "RUST_LOG=info\nOPENAI_ORG=org-123".into();
                d["shell_writable_roots"] = serde_json::json!(["/Users/dee/projects"]);
            }
            s.redraw = true;
        });
        rndr_paint::frame(&mut app);
        let body = state(app.world()).unwrap().body;
        let scroll = app.world().get::<Parent>(body).unwrap().entity;
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., -560.];
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "runtime-lists");
        with_state(app.world_mut(), |_, s| {
            s.open.remove("runtime.allowlists");
            s.open.remove("runtime.env");
            s.open.insert("runtime.workers");
            s.open.insert("runtime.versions");
            let workers = runtime::Workers {
                max_in_flight: 8,
                agents: 2,
                scheduled: 2,
                delivery: 1,
                maintenance: 1,
            };
            s.workers = Some(Ok(workers));
            s.workers_draft = Some(workers.stepped("agents", 1));
            s.versions = Some(Ok((
                serde_json::json!({"available": true, "version": "git version 2.47.1", "path": "/usr/bin/git", "enabled": true, "platformHint": ""}),
                serde_json::json!({"enabled": true, "available": true, "isRepo": true, "branch": "main", "dirtyCount": 3, "vaultRoot": "/v"}),
            )));
            s.redraw = true;
        });
        rndr_paint::frame(&mut app);
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., -1400.];
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "runtime-more");
        app.world_mut().get_mut::<Scroll>(scroll).unwrap().offset = [0., 0.];
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

fn providers_read(openai_key: bool) -> providers::Read {
    providers::Read {
        connections: serde_json::from_value(serde_json::json!([{
            "connection_id": format!("conn_{}", "b".repeat(32)), "kind": "openai",
            "secrets": {"api_key": openai_key},
            "created_at": "2026-11-01T00:00:00Z", "updated_at": "2026-11-01T00:00:00Z"
        }]))
        .unwrap(),
        custom_id: None,
    }
}

fn providers_open() -> App {
    let mut app = agent_open();
    app.world_mut().insert_resource(Viewport::new(1200., 2600.));
    let read = state(app.world())
        .unwrap()
        .providers_reading
        .expect("read on open");
    apply(
        app.world_mut(),
        Update {
            serial: read,
            result: Ok(Reply::Providers(Box::new(providers_read(true)))),
        },
    );
    take_commands(app.world_mut());
    app.update();
    app
}

#[test]
fn a_provider_key_is_typed_masked_saved_and_cleared_on_a_second_press() {
    let mut app = providers_open();
    let t = texts(app.world());
    assert!(t.iter().any(|x| x == "1 provider with a key"), "{t:?}");
    click(&mut app, "Provider keys");
    let t = texts(app.world());
    assert!(t.iter().any(|x| x == "Default URL · Key stored"), "{t:?}");

    click(&mut app, "Edit OpenAI API");
    assert_eq!(
        field_text(app.world(), "API base URL"),
        "https://api.openai.com/v1"
    );
    click(&mut app, "API key");
    type_text(&mut app, "sk-new");
    let key = named(app.world(), "API key").unwrap();
    assert!(app.world().has::<Secure>(key));
    click(&mut app, "Save OpenAI API");
    let (busy, _) = state(app.world()).unwrap().provider_busy.expect("saving");
    apply(
        app.world_mut(),
        Update {
            serial: busy,
            result: Ok(Reply::Providers(Box::new(providers_read(true)))),
        },
    );
    app.update();
    assert_eq!(
        field_text(app.world(), "API key"),
        "",
        "the key leaves the form"
    );
    assert!(
        texts(app.world())
            .iter()
            .any(|x| x == "Saved to this workshop.")
    );

    // Clearing asks first.
    click(&mut app, "Clear OpenAI API key");
    assert!(state(app.world()).unwrap().provider_busy.is_none());
    click(&mut app, "Keep OpenAI API key");
    assert!(!state(app.world()).unwrap().provider_clear);
    click(&mut app, "Clear OpenAI API key");
    click(&mut app, "Confirm clearing OpenAI API key");
    assert!(state(app.world()).unwrap().provider_busy.is_some());
}

#[test]
fn a_bad_base_url_is_named_and_an_unchanged_form_says_so() {
    let mut app = providers_open();
    click(&mut app, "Provider keys");
    click(&mut app, "Edit OpenAI API");
    click(&mut app, "Save OpenAI API");
    assert!(texts(app.world()).iter().any(|x| x == "Nothing changed."));
    click(&mut app, "API base URL");
    type_text(&mut app, " nope");
    click(&mut app, "Save OpenAI API");
    let s = state(app.world()).unwrap();
    assert!(s.provider_busy.is_none());
    assert_eq!(s.provider_note, "Enter a valid http(s) API base URL.");
    // Closing drops the form's text.
    click(&mut app, "Close OpenAI API");
    let s = state(app.world()).unwrap();
    assert!(s.provider_open.is_none() && !s.drafts.contains_key("provider.url"));
}

#[test]
fn runtime_lists_are_edited_and_saved_one_field_at_a_time() {
    let mut app = runtime_open(serde_json::json!({
        "shell_allowed_binaries": ["git"],
        "allowed_modules": [],
    }));
    click(&mut app, "Allowlists");
    assert_eq!(field_text(app.world(), "Allowed binaries"), "git");
    click(&mut app, "Allowed binaries");
    type_text(&mut app, ", rg");
    click(&mut app, "Save allowed binaries");
    let s = state(app.world()).unwrap();
    let (_, patch) = s.patching.clone().expect("saving");
    assert_eq!(
        patch,
        agent::Patch::Field("shell_allowed_binaries", serde_json::json!(["git", "rg"]))
    );
    saved(&mut app);
    assert_eq!(field_text(app.world(), "Allowed binaries"), "git, rg");

    click(&mut app, "Allowed tools");
    assert_eq!(field_text(app.world(), "Module names"), "");
    click(&mut app, "Module names");
    type_text(&mut app, "websearch.search,fetch.url");
    click(&mut app, "Save allowed tools");
    let (_, patch) = state(app.world())
        .unwrap()
        .patching
        .clone()
        .expect("saving");
    assert_eq!(
        patch,
        agent::Patch::Field(
            "allowed_modules",
            serde_json::json!(["websearch.search", "fetch.url"])
        )
    );
}

#[test]
fn env_overrides_add_a_masked_value_and_remove_on_a_second_press() {
    let mut app = runtime_open(serde_json::json!({"env_overrides": "A=1"}));
    click(&mut app, "Host env overrides");
    let t = texts(app.world());
    assert!(t.iter().any(|x| x == "A"));
    assert!(!t.iter().any(|x| x.contains("A=1")), "values aren't shown");

    click(&mut app, "Variable name");
    type_text(&mut app, "TOKEN");
    click(&mut app, "Value");
    type_text(&mut app, "s3cret");
    assert!(
        app.world()
            .has::<Secure>(named(app.world(), "Value").unwrap())
    );
    click(&mut app, "Add variable");
    let s = state(app.world()).unwrap();
    let (_, patch) = s.patching.clone().expect("saving");
    assert_eq!(
        patch,
        agent::Patch::Field("env_overrides", "A=1\nTOKEN=s3cret".into())
    );
    assert!(
        !s.drafts.contains_key("rt.env.value"),
        "the value is dropped"
    );
    saved(&mut app);
    assert_eq!(field_text(app.world(), "Value"), "");

    click(&mut app, "Add variable");
    assert_eq!(
        state(app.world()).unwrap().env_note,
        "A variable name has no spaces or “=”, like API_TIMEOUT."
    );

    click(&mut app, "Remove A");
    assert!(state(app.world()).unwrap().patching.is_none(), "asks first");
    click(&mut app, "Confirm removal of A");
    let (_, patch) = state(app.world())
        .unwrap()
        .patching
        .clone()
        .expect("removing");
    assert_eq!(
        patch,
        agent::Patch::Field("env_overrides", "TOKEN=s3cret".into())
    );
}

#[test]
fn adding_an_mcp_server_saves_it_and_restarts_the_gateway() {
    use medousa_desktop_host::mcp::{Transport, Upsert};
    let mut app = mcp_open(true);
    app.world_mut().insert_resource(Viewport::new(1200., 1600.));
    click(&mut app, "Add a server");
    for (name, text) in [
        ("Server id", "files"),
        ("Title", "Files"),
        ("Command", "npx"),
        ("Arguments", "-y server /tmp"),
    ] {
        click(&mut app, name);
        type_text(&mut app, text);
    }
    click(&mut app, "Save server");
    let s = state(app.world()).unwrap();
    let form = s.mcp_form.clone().unwrap();
    let serial = form.saving.expect("saving");
    assert_eq!(s.mcp_busy, Some((serial, true)), "a save restarts");
    assert_eq!(
        mcp_upsert(s, &form),
        Upsert {
            id: "files".into(),
            title: "Files".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "server".into(), "/tmp".into()],
            ..Upsert::default()
        }
    );
    // A refusal stays in the form; success closes it.
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Err("Title is required".into()),
        },
    );
    app.update();
    assert_eq!(
        state(app.world()).unwrap().mcp_form.as_ref().unwrap().note,
        "Title is required"
    );
    click(&mut app, "Save server");
    let serial = state(app.world()).unwrap().mcp_busy.unwrap().0;
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::McpDone("Gateway restarted".into())),
        },
    );
    let s = state(app.world()).unwrap();
    assert!(s.mcp_form.is_none() && !s.drafts.contains_key("mcp.title"));
    assert_eq!(s.message, "Gateway restarted");
}

#[test]
fn editing_a_remote_server_fills_the_form_and_keeps_its_token_hidden() {
    use medousa_desktop_host::mcp::{Transport, Upsert};
    let mut app = mcp_open(true);
    app.world_mut().insert_resource(Viewport::new(1200., 1600.));
    click(&mut app, "Edit GitHub");
    let loading = state(app.world())
        .unwrap()
        .mcp_form
        .as_ref()
        .unwrap()
        .loading
        .expect("read");
    apply(
        app.world_mut(),
        Update {
            serial: loading,
            result: Ok(Reply::McpServer(Box::new(Some((
                Upsert {
                    id: "github".into(),
                    title: "GitHub".into(),
                    transport: Transport::Http,
                    url: "https://api.githubcopilot.com/mcp".into(),
                    ..Upsert::default()
                },
                true,
            ))))),
        },
    );
    app.update();
    assert_eq!(
        field_text(app.world(), "URL"),
        "https://api.githubcopilot.com/mcp"
    );
    assert_eq!(field_text(app.world(), "Bearer token"), "");
    assert!(
        app.world()
            .has::<Secure>(named(app.world(), "Bearer token").unwrap())
    );
    click(&mut app, "Forget saved token");
    assert!(
        state(app.world())
            .unwrap()
            .mcp_form
            .as_ref()
            .unwrap()
            .forget_token
    );
    click(&mut app, "Local command");
    assert_eq!(
        state(app.world())
            .unwrap()
            .mcp_form
            .as_ref()
            .unwrap()
            .transport,
        Transport::Stdio
    );
    click(&mut app, "Cancel server");
    assert!(state(app.world()).unwrap().mcp_form.is_none());
}

fn conversations_open(kind: &str) -> App {
    let mut app = external_open(kind);
    app.world_mut().insert_resource(Viewport::new(1200., 3000.));
    click(&mut app, "Muse, Instinct, Dots & Grok");
    app
}

#[test]
fn adding_a_grok_bot_shows_its_callback_key_once() {
    let mut app = conversations_open("local");
    click(&mut app, "Add Grok Bot");
    for (name, text) in [
        ("Bot label", "Routines"),
        ("Bot name", "routine-bot"),
        ("Webhook POST URL", "https://x.ai/hook"),
        ("Webhook key", "wk-secret"),
    ] {
        click(&mut app, name);
        type_text(&mut app, text);
    }
    assert!(
        app.world()
            .has::<Secure>(named(app.world(), "Webhook key").unwrap())
    );
    click(&mut app, "Connect Grok Bot");
    let serial = state(app.world())
        .unwrap()
        .conv_form
        .as_ref()
        .unwrap()
        .saving
        .expect("connecting");
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::Created(Box::new(
                serde_json::from_value(serde_json::json!({
                    "conversation": conversation("grok-2", "grok_bot", "Routines", "routine-bot"),
                    "callback_key": "cb-123"
                }))
                .unwrap(),
            ))),
        },
    );
    app.update();
    let s = state(app.world()).unwrap();
    assert!(s.conv_form.is_none() && !s.drafts.contains_key("conv.webhook_key"));
    assert_eq!(s.callback_key, Some(("grok-2".into(), "cb-123".into())));
    assert!(s.agents_reading.is_some(), "the list is read again");
}

#[test]
fn a_dots_session_names_what_is_missing() {
    let mut app = conversations_open("local");
    click(&mut app, "Add Dots");
    click(&mut app, "Session name");
    type_text(&mut app, "Team");
    click(&mut app, "Connect Dots");
    assert_eq!(
        state(app.world()).unwrap().conv_form.as_ref().unwrap().note,
        "Enter the Slack channel ID."
    );
    click(&mut app, "Cancel adding Dots");
    assert!(state(app.world()).unwrap().conv_form.is_none());
}

#[test]
fn muse_pairs_through_whatsapp_and_finds_its_chat() {
    use medousa_types::{ExternalWhatsAppPairingState as Wa, ExternalWhatsAppPairingStatus};
    let mut app = conversations_open("local");
    click(&mut app, "Add Muse");
    let serial = state(app.world())
        .unwrap()
        .conv_form
        .as_ref()
        .unwrap()
        .pairing_reading
        .expect("pairing read on open");
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2 2"><rect width="1" height="1"/></svg>"#;
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::Pairing(Box::new(external::PairingRead {
                pairing: Ok(external::pairing(ExternalWhatsAppPairingStatus {
                    state: Wa::QrReady,
                    qr_svg: Some(svg.into()),
                    expires_at: Some(chrono::Utc::now() + chrono::Duration::seconds(60)),
                })),
                adapter_error: None,
            }))),
        },
    );
    app.update();
    assert!(
        app.world()
            .entities_with::<Semantics>()
            .into_iter()
            .any(|e| app
                .world()
                .get::<Semantics>(e)
                .and_then(|s| s.name.as_deref())
                == Some("WhatsApp Linked Devices pairing QR"))
    );

    click(&mut app, "Find Muse chat");
    let found = |app: &mut App, jid: Option<&str>| {
        let serial = state(app.world())
            .unwrap()
            .conv_form
            .as_ref()
            .unwrap()
            .discovery_reading
            .expect("discovery in flight");
        apply(
            app.world_mut(),
            Update {
                serial,
                result: Ok(Reply::Discovery(Box::new(
                    medousa_types::ExternalMuseDiscoveryStatus {
                        challenge: "MDS-42".into(),
                        observed_chat_jid: jid.map(String::from),
                        expires_at: chrono::Utc::now() + chrono::Duration::seconds(300),
                    },
                ))),
            },
        );
        app.update();
    };
    found(&mut app, None);
    assert!(
        texts(app.world())
            .iter()
            .any(|x| x == "Send this code in your Muse chat on WhatsApp: MDS-42")
    );
    click(&mut app, "Check for code");
    found(&mut app, None);
    assert_eq!(
        state(app.world()).unwrap().conv_form.as_ref().unwrap().note,
        "The linked adapter has not seen this code yet."
    );
    click(&mut app, "Check for code");
    found(&mut app, Some("120@g.us"));
    click(&mut app, "Session name");
    type_text(&mut app, "Muse");
    click(&mut app, "Connect Muse");
    assert!(
        state(app.world())
            .unwrap()
            .conv_form
            .as_ref()
            .unwrap()
            .saving
            .is_some()
    );
}

#[test]
fn sharing_opens_the_peers_screen() {
    let mut app = sharing_open();
    click(&mut app, "Open Peers");
    assert_eq!(
        crate::surfaces::active(app.world()),
        crate::surfaces::SurfaceId::Peers
    );
}

fn agent_extras_open(defaults: serde_json::Value) -> App {
    let mut app = agent_open();
    app.world_mut().insert_resource(Viewport::new(1200., 3200.));
    with_state(app.world_mut(), |_, s| {
        s.defaults = Some(defaults);
        s.redraw = true;
    });
    app.update();
    app
}

fn patch_in_flight(app: &App) -> agent::Patch {
    state(app.world())
        .unwrap()
        .patching
        .clone()
        .expect("a change in flight")
        .1
}

#[test]
fn fallback_models_and_coding_runtime_save_one_change_at_a_time() {
    let mut app = agent_extras_open(serde_json::json!({
        "provider": "anthropic", "model": "claude-opus-5-5",
    }));
    click(&mut app, "Fallback models");
    assert!(
        named(app.world(), "Change second fallback").is_none(),
        "one slot at a time"
    );
    click(&mut app, "Change first fallback");
    click(&mut app, "gpt-6 · openai");
    assert_eq!(
        patch_in_flight(&app),
        agent::Patch::Fallback(0, Some(("openai".into(), "gpt-6".into())))
    );
    saved(&mut app);
    assert!(
        !state(app.world()).unwrap().open.contains("fallback.0"),
        "the picker closes"
    );
    assert!(named(app.world(), "Change second fallback").is_some());

    click(&mut app, "Coding runtime");
    click(&mut app, "Add Codex fallback");
    assert_eq!(
        patch_in_flight(&app),
        agent::Patch::Field(
            "coding_runtime",
            serde_json::json!({"preferred": "medousa", "fallbacks": ["codex"]})
        )
    );
    saved(&mut app);
    click(&mut app, "Add Cursor fallback");
    saved(&mut app);
    click(&mut app, "Move Cursor earlier");
    assert_eq!(
        patch_in_flight(&app),
        agent::Patch::Field(
            "coding_runtime",
            serde_json::json!({"preferred": "medousa", "fallbacks": ["cursor", "codex"]})
        )
    );
}

#[test]
fn a_stage_route_is_edited_and_saved_with_the_rest_kept() {
    let route = |role: &str| {
        serde_json::json!({"role": role, "provider": "anthropic", "model": "a",
            "policy_profile": "default", "fallback_chain": []})
    };
    let matrix: serde_json::Map<_, _> = agent::STAGES
        .iter()
        .map(|(k, _)| (k.to_string(), route(k)))
        .collect();
    let mut app = agent_extras_open(serde_json::json!({ "stage_routing": matrix }));
    click(&mut app, "Stage routing");
    click(&mut app, "Verifier");
    assert_eq!(field_text(app.world(), "Model"), "a");
    click(&mut app, "Fallback chain");
    type_text(&mut app, "openai:gpt-6");
    click(&mut app, "Save Verifier stage");
    let agent::Patch::Field(key, value) = patch_in_flight(&app) else {
        panic!("a field patch");
    };
    assert_eq!(key, "stage_routing");
    assert_eq!(
        value["verifier"]["fallback_chain"],
        serde_json::json!(["openai:gpt-6"])
    );
    assert_eq!(value["orchestrator"], route("orchestrator"));
}

#[test]
fn mode_suggestions_and_artifact_cleanup_save_through_the_engine() {
    let mut app = agent_extras_open(serde_json::json!({}));
    let (mode, arts) = {
        let s = state(app.world()).unwrap();
        (s.mode_reading.unwrap(), s.artifacts_reading.unwrap())
    };
    apply(
        app.world_mut(),
        Update {
            serial: mode,
            result: Ok(Reply::ModePolicy(
                medousa_types::AgentModeTransitionPolicy {
                    proposal_ttl_seconds: 30,
                    auto_accept: medousa_types::AgentModeAutoAccept::Never,
                },
            )),
        },
    );
    apply(
        app.world_mut(),
        Update {
            serial: arts,
            result: Ok(Reply::Artifacts(Box::new(
                serde_json::from_value(serde_json::json!({
                    "settings": {"enabled": true, "max_age_days": 90, "max_per_session": 200,
                        "recurring_id": "r", "cron_expr": "0 3 * * *"},
                    "scheduled": true, "enabled": true,
                }))
                .unwrap(),
            ))),
        },
    );
    app.update();
    click(&mut app, "Mode suggestions");
    let t = texts(app.world());
    assert!(t.iter().any(|x| x == "30 s"), "{t:?}");
    click(&mut app, "Task-scoped only");
    assert!(state(app.world()).unwrap().mode_busy.is_some());
    click(&mut app, "Artifacts");
    click(&mut app, "More keep for");
    assert!(state(app.world()).unwrap().artifacts_busy.is_some());
}

#[test]
fn a_servers_tools_turn_on_and_off_and_take_discovery_hints() {
    use medousa_desktop_host::mcp::Tool;
    let mut app = mcp_open(true);
    app.world_mut().insert_resource(Viewport::new(1200., 1800.));
    click(&mut app, "Tools for GitHub");
    let read = state(app.world())
        .unwrap()
        .mcp_tools_reading
        .expect("listed");
    let tool = |name: &str, title: &str, on: bool, hints: &[&str]| Tool {
        name: name.into(),
        title: title.into(),
        enabled: on,
        available: on,
        hints: hints.iter().map(|h| h.to_string()).collect(),
    };
    apply(
        app.world_mut(),
        Update {
            serial: read,
            result: Ok(Reply::McpToolList(
                vec![
                    tool("issues", "List issues", true, &["bugs"]),
                    tool("merge", "Merge PR", false, &[]),
                ],
                String::new(),
            )),
        },
    );
    app.update();
    click(&mut app, "Merge PR");
    let (serial, restart) = state(app.world()).unwrap().mcp_busy.expect("saving");
    assert!(restart, "a tool change restarts the gateway");
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::McpDone("merge updated".into())),
        },
    );
    assert!(
        state(app.world()).unwrap().mcp_tools_reading.is_some(),
        "listed again"
    );

    click(&mut app, "Hints for List issues");
    assert_eq!(field_text(app.world(), "Discovery hints"), "bugs");
    click(&mut app, "Discovery hints");
    type_text(&mut app, ", tickets");
    click(&mut app, "Save hints for List issues");
    let s = state(app.world()).unwrap();
    assert!(s.mcp_busy.is_some() && s.mcp_hint_tool.is_none());
}

#[test]
fn a_server_with_a_sign_in_signs_in_through_the_engine() {
    let mut app = mcp_open(true);
    let read = state(app.world())
        .unwrap()
        .mcp_oauth_reading
        .expect("asked which servers sign in");
    apply(
        app.world_mut(),
        Update {
            serial: read,
            result: Ok(Reply::McpOAuth(vec![
                serde_json::from_value(serde_json::json!({
                    "server_id": "github", "status": "disconnected", "connected": false
                }))
                .unwrap(),
            ])),
        },
    );
    app.update();
    assert!(
        named(app.world(), "Sign in to Linear").is_none(),
        "no sign-in, no button"
    );
    click(&mut app, "Sign in to GitHub");
    let (serial, id) = state(app.world())
        .unwrap()
        .mcp_signin
        .clone()
        .expect("signing in");
    assert_eq!(id, "github");
    apply(
        app.world_mut(),
        Update {
            serial,
            result: Ok(Reply::McpDone("Signed in".into())),
        },
    );
    let s = state(app.world()).unwrap();
    assert_eq!(s.message, "Signed in");
    assert!(s.mcp_reading.is_some(), "status is read again");
}

#[test]
fn worker_capacity_steps_a_draft_and_saves_only_when_shares_fit() {
    let mut app = runtime_open(serde_json::json!({}));
    let read = state(app.world())
        .unwrap()
        .workers_reading
        .expect("read on open");
    let saved = runtime::Workers {
        max_in_flight: 4,
        agents: 2,
        scheduled: 1,
        delivery: 1,
        maintenance: 0,
    };
    apply(
        app.world_mut(),
        Update {
            serial: read,
            result: Ok(Reply::Workers(saved)),
        },
    );
    app.update();
    click(&mut app, "Worker capacity");
    click(&mut app, "More agents");
    click(&mut app, "Save worker capacity");
    let s = state(app.world()).unwrap();
    assert!(s.workers_busy.is_none(), "5 shares don't fit in 4");
    assert_eq!(
        s.workers_note,
        "Lane shares total 5, which exceeds global capacity 4."
    );
    click(&mut app, "More global capacity");
    click(&mut app, "Save worker capacity");
    let busy = state(app.world()).unwrap().workers_busy.expect("saving");
    let next = state(app.world()).unwrap().workers_draft.unwrap();
    assert_eq!((next.max_in_flight, next.agents), (5, 3));
    apply(
        app.world_mut(),
        Update {
            serial: busy,
            result: Ok(Reply::Workers(next)),
        },
    );
    assert_eq!(
        state(app.world()).unwrap().workers_note,
        "Saved — restart the engine to apply the new capacity."
    );
}

#[test]
fn versions_turn_on_through_the_engine_and_show_the_vault() {
    let mut app = runtime_open(serde_json::json!({}));
    let read = state(app.world())
        .unwrap()
        .versions_busy
        .expect("read on open");
    let versions = |on: bool| {
        Reply::Versions(Box::new((
            serde_json::json!({"available": true, "version": "git 2.47", "enabled": on, "platformHint": ""}),
            serde_json::json!({"enabled": on, "available": true, "isRepo": on, "branch": "main",
                "dirtyCount": 2, "vaultRoot": "/v"}),
        )))
    };
    apply(
        app.world_mut(),
        Update {
            serial: read,
            result: Ok(versions(false)),
        },
    );
    app.update();
    click(&mut app, "Versions");
    click(&mut app, "Vault versioning");
    let busy = state(app.world())
        .unwrap()
        .versions_busy
        .expect("turning on");
    apply(
        app.world_mut(),
        Update {
            serial: busy,
            result: Ok(versions(true)),
        },
    );
    app.update();
    let t = texts(app.world());
    assert!(t.iter().any(|x| x == "On · main · 2 changed"), "{t:?}");
}
