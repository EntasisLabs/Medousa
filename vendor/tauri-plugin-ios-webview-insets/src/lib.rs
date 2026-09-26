use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_ios_webview_insets);

/// Keep the web canvas edge-to-edge without UIKit adding safe-area content
/// insets or an automatic scroll-edge separator.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("ios-webview-insets")
        .setup(|_app, api| {
            #[cfg(target_os = "ios")]
            {
                let _ = api.register_ios_plugin(init_plugin_ios_webview_insets)?;
            }
            #[cfg(not(target_os = "ios"))]
            let _ = api;
            Ok(())
        })
        .build()
}
