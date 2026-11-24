mod approvals;
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
mod native_style;
mod notes;
mod peer_management;
mod peers;
mod provider_timeline;
mod reviews;
mod routing;
mod runtimes;
mod saved_transport;
mod settings;
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
    workshops::install(app.world_mut());
    rndr_render::run(app);
    Ok(())
}
