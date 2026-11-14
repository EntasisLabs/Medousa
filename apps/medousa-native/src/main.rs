mod approvals;
mod attachments;
mod auxiliary;
mod backend;
mod chat;
mod context;
mod media_preview;
mod provider_timeline;
mod runtimes;
mod timeline;
mod ui;
mod workspace;

use rndr_core::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    medousa_desktop_host::paths::load_env_overlay();
    let connection = medousa_desktop_host::connection::discover(Default::default());
    let mut app = ui::app();
    // Native window preferences belong to this device, even for a remote workshop.
    app.world_mut().insert_resource(workspace::Workspace::local(
        medousa_desktop_host::paths::medousa_config_dir().join("native-workspace.json"),
    ));
    let _feed = match connection {
        Ok(connection) => {
            match backend::client(&connection.origin, &connection.data_dir) {
                Ok(client) => {
                    let auxiliary = auxiliary::Auxiliary::start(client, app.world_mut());
                    app.world_mut().insert_resource(auxiliary);
                }
                Err(error) => {
                    app.world_mut()
                        .resource_mut::<ui::Model>()
                        .unwrap()
                        .chat
                        .status = error;
                }
            }
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            app.world_mut()
                .resource_mut::<ui::Model>()
                .unwrap()
                .commands = Some(tx);
            Some(feed::spawn_thread(
                app.world_mut(),
                FeedConfig::queue(256),
                move |out| backend::run(connection.origin, connection.data_dir, rx, out),
            ))
        }
        Err(error) => {
            app.world_mut()
                .resource_mut::<ui::Model>()
                .unwrap()
                .chat
                .status = error;
            None
        }
    };
    rndr_render::run(app);
    Ok(())
}
