mod approvals;
mod artifact_preview;
mod artifacts;
mod attachments;
mod auxiliary;
mod backend;
mod calendar;
mod chat;
mod code;
mod context;
mod create;
mod diff_presentation;
mod media_preview;
mod multiplexer;
mod multiplexer_model;
mod multiplexer_storage;
mod native_style;
mod navigation;
mod notes;
mod peer_management;
mod peers;
mod provider_timeline;
mod reviews;
mod routing;
mod runtimes;
mod saved_transport;
mod settings;
mod sidebars;
mod source_preferences;
mod surfaces;
mod terminal;
mod theme;
mod timeline;
mod ui;
mod workshops;
mod workspace;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    medousa_desktop_host::paths::load_env_overlay();
    let mut app = ui::app();
    // Device chrome is local; it must not follow a daemon or legacy selection.
    app.world_mut().insert_resource(workspace::Workspace::local(
        medousa_desktop_host::paths::medousa_config_dir().join("native-workspace.json"),
    ));
    multiplexer::install(
        app.world_mut(),
        medousa_desktop_host::paths::medousa_config_dir().join("native-desktops.json"),
    );
    workshops::install(app.world_mut());
    let layouts = multiplexer::storage(app.world());
    rndr_render::Session::new(app)
        .native_host(|_| Box::new(artifact_preview::Host::default()))
        .run();
    // UI is closed: the bounded flush never blocks an interactive or async task.
    if let Some(layouts) = layouts {
        if let Err(error) = layouts.flush(std::time::Duration::from_secs(2)) {
            eprintln!("Desktop layout was not saved: {error}");
        }
    }
    Ok(())
}
