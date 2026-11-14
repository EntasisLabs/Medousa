mod admin_backend;
mod admin_model;
mod admin_views;
mod backend;
mod composer;
mod content;
#[cfg(test)]
mod content_tests;
mod controller;
mod conversation;
mod desktop;
mod desktop_fixtures;
#[cfg(test)]
mod desktop_layout_tests;
#[cfg(test)]
mod desktop_tests;
mod fixtures;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod interaction_tests;
mod menus;
mod presentation;
#[cfg(test)]
mod qualification_tests;
mod settings_views;
mod shell;
mod startup;
mod state;
#[cfg(test)]
mod ui_tests;
use backend::{Command, Request};
#[cfg(test)]
use backend::{Reply, Update};
#[cfg(test)]
use composer::reset_prompt;
use composer::{SubmitBoundary, guard_submit_boundary};
use controller::update;
#[cfg(feature = "observe")]
use fixtures::FixtureObservation;
use medousa_rndr_home::Chat;
use rndr_core::*;
use rndr_ui::{Appearance, UiTheme};
#[cfg(test)]
use rndr_ui::{Contrast, Token};
#[cfg(test)]
use shell::Ui;
use shell::setup;
use state::Model;
#[cfg(test)]
use std::sync::Arc;
use tokio::sync::mpsc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = startup::Cli::parse(std::env::args().skip(1))?;
    if cli.help {
        println!("{}", startup::HELP);
        return Ok(());
    }
    // This closure is unreachable for every offline/export/observe mode. Load the
    // same overlay at process startup as Tauri, before any worker threads exist.
    let connection = cli.connection_with(|overrides| {
        medousa_desktop_host::paths::load_env_overlay();
        medousa_desktop_host::connection::discover(overrides)
    })?;
    if let Some(connection) = &connection {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(connection.ensure_ready())?;
    }
    let startup::Cli {
        session,
        export,
        desktop_export,
        notes_export,
        observe_offline,
        theme,
        ..
    } = cli;
    let mut app = app();
    presentation::apply_theme(app.world_mut(), theme);
    let mut _feed = None;
    let mut _read_feed = None;
    if let Some(connection) = connection {
        let origin = connection.origin;
        let dir = connection.data_dir;
        let (tx, rx) = mpsc::unbounded_channel();
        app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx.clone());
        let (read_tx, read_rx) = mpsc::unbounded_channel();
        app.world_mut()
            .resource_mut::<desktop::Connection>()
            .unwrap()
            .sender = Some(read_tx);
        let read_origin = origin.clone();
        let read_dir = dir.clone();
        _read_feed = Some(feed::spawn_thread(
            app.world_mut(),
            FeedConfig::queue(32),
            move |out| admin_backend::run(read_origin, read_dir, read_rx, out),
        ));
        _feed = Some(feed::spawn_thread(
            app.world_mut(),
            FeedConfig::queue(256),
            move |out| backend::run(origin, dir, rx, out),
        ));
        if let Some(id) = session {
            app.world_mut().resource_mut::<Model>().unwrap().loading = true;
            tx.send(Request {
                generation: 0,
                command: Command::Open(id),
            })?;
        }
    }
    #[cfg(feature = "observe")]
    let _exit_timer = if observe_offline {
        // No data-dir/session/daemon data can enter this branch: offline=true
        // before any worker is created. Only the audited empty fixture is public.
        use rndr_render::observe::{ObservationContent, ObservationSession};
        app.world_mut().insert_resource(FixtureObservation);
        app.world_mut().insert_resource(ObservationContent::Deny);
        let descriptor = ObservationSession::enable(
            app.world_mut(),
            "Medousa offline empty fixture · explicit theme · parley",
        )?;
        eprintln!("observation descriptor: {}", descriptor.display());
        #[derive(Clone)]
        struct Exit;
        impl Event for Exit {}
        app.add_system(Schedule::Update, |world| {
            if !events::<Exit>(world).is_empty() {
                world.resource_mut::<Input>().unwrap().quit = true;
            }
        });
        Some(after(
            app.world_mut(),
            std::time::Duration::from_secs(15),
            Exit,
        ))
    } else {
        None
    };
    #[cfg(not(feature = "observe"))]
    if observe_offline {
        return Err("--observe-offline requires --features observe".into());
    }
    if let Some(dir) = notes_export {
        desktop_fixtures::export_notes(&mut app, dir)?;
    } else if let Some(dir) = desktop_export {
        desktop_fixtures::export(&mut app, dir)?;
    } else if let Some(dir) = export {
        fixtures::export(&mut app, dir)?;
    } else {
        rndr_render::run(app);
    }
    Ok(())
}
fn app() -> App {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "Medousa · rndr preview".into(),
        size: LogicalSize::new(1120., 820.),
        min_size: Some(LogicalSize::new(480., 650.)),
        ..Default::default()
    });
    presentation::apply_theme(
        app.world_mut(),
        UiTheme {
            appearance: Appearance::Dark,
            ..Default::default()
        },
    );
    app.world_mut().insert_resource(Model {
        #[cfg(test)]
        fixture_rows: vec![],
        #[cfg(test)]
        last_fixture_rows: vec![],
        chat: Chat::offline(),
        commands: None,
        models: vec![],
        selection: None,
        loading: false,
        generation: 0,
        mode: None,
        modes: Vec::new(),
        depth: "standard".into(),
    });
    app.add_plugin(rndr_ui::UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(SubmitBoundary::default());
    app.world_mut().insert_resource(menus::Menus::default());
    app.world_mut()
        .insert_resource(admin_model::Desktop::default());
    app.world_mut()
        .insert_resource(desktop::Connection::default());
    app.add_system_front(Schedule::Input, menus::settings_keyboard);
    app.add_system_front(Schedule::Input, guard_submit_boundary);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, update);
    app
}

#[cfg(test)]
mod desktop_preservation_tests;
#[cfg(test)]
mod shell_activity_tests;
