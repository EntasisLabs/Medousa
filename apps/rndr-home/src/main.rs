mod backend;
mod composer;
mod controller;
mod conversation;
mod fixtures;
#[cfg(test)]
mod interaction_tests;
mod menus;
mod presentation;
#[cfg(test)]
mod qualification_tests;
mod shell;
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
#[cfg(test)]
use rndr_ui::Token;
use rndr_ui::{Appearance, Contrast, UiTheme};
#[cfg(test)]
use shell::Ui;
use shell::setup;
use state::Model;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Arc;
use tokio::sync::mpsc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut origin = "http://127.0.0.1:9777".to_string();
    let mut data_dir: Option<PathBuf> = None;
    let mut session = None;
    let mut export: Option<PathBuf> = None;
    let mut offline = false;
    let mut observe_offline = false;
    let mut theme = UiTheme {
        appearance: Appearance::Dark,
        ..Default::default()
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--daemon" => origin = args.next().ok_or("--daemon needs an origin")?,
            "--data-dir" => {
                data_dir = Some(args.next().ok_or("--data-dir needs a directory")?.into())
            }
            "--session" => session = Some(args.next().ok_or("--session needs an id")?),
            "--export" => {
                export = Some(args.next().ok_or("--export needs a directory")?.into());
                offline = true;
            }
            "--offline" => offline = true,
            "--light" => theme.appearance = Appearance::Light,
            "--high-contrast" => theme.contrast = Contrast::High,
            "--observe-offline" => {
                offline = true;
                observe_offline = true;
            }
            "--help" | "-h" => {
                println!(
                    "Medousa (rndr)\n  --data-dir PATH [--daemon http://127.0.0.1:9777] [--session ID]\n  --offline  No credentials or network\n  --light / --high-contrast  Explicit window appearance (default: dark)\n  --export DIRECTORY  Offline GPU PNG evidence; no network\n  --observe-offline  Synthetic live window capture, 15s normal exit (requires feature observe)\nUse the existing app to start/configure the daemon. Sending can incur inference charges."
                );
                return Ok(());
            }
            _ if arg.starts_with("-psn_") => {}
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let mut app = app();
    presentation::apply_theme(app.world_mut(), theme);
    let mut _feed = None;
    if !offline {
        medousa_rndr_home::local_url(&origin)?;
        let dir = data_dir.ok_or("--data-dir is required; use --offline for UI only. No credentials are discovered or created automatically.")?;
        let (tx, rx) = mpsc::unbounded_channel();
        app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx.clone());
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
    if let Some(dir) = export {
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
        chat: Chat::offline(),
        commands: None,
        models: vec![],
        selection: None,
        loading: false,
        generation: 0,
        recovery: Vec::new(),
        pending_recovery: None,
        history_messages: Vec::new(),
        mode: None,
        modes: Vec::new(),
        depth: "standard".into(),
    });
    app.add_plugin(rndr_ui::UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(SubmitBoundary::default());
    app.world_mut().insert_resource(menus::Menus::default());
    app.add_system_front(Schedule::Input, guard_submit_boundary);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, update);
    app
}
