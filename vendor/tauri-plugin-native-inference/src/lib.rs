use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

mod error;

#[cfg(mobile)]
mod mobile;
#[cfg(mobile)]
pub use mobile::NativeInference;

#[cfg(desktop)]
mod desktop;
#[cfg(desktop)]
pub use desktop::NativeInference;

pub use error::{Error, Result};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::<R, ()>::new("native-inference")
        .setup(|app, api| {
            #[cfg(mobile)]
            let inference = mobile::init(app, api)?;
            #[cfg(desktop)]
            let inference = desktop::init(app, api)?;
            app.manage(inference);
            Ok(())
        })
        .build()
}
