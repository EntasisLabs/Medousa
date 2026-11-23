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
#[ignore = "writes verification/native-settings-{look,connection,packages}-{dark,light}.png"]
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
