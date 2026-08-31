#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),
    #[error("native inference is unavailable on this platform")]
    Unavailable,
}

pub type Result<T> = std::result::Result<T, Error>;
