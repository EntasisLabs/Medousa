use serde_json::Value;
use tauri::plugin::PluginApi;
use tauri::{AppHandle, Runtime};

pub fn init<R: Runtime, C>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<NativeInference<R>> {
    Ok(NativeInference(std::marker::PhantomData))
}

pub struct NativeInference<R: Runtime>(std::marker::PhantomData<R>);

impl<R: Runtime> Clone for NativeInference<R> {
    fn clone(&self) -> Self {
        Self(std::marker::PhantomData)
    }
}

macro_rules! unavailable {
    ($name:ident $(, $arg:ident : $ty:ty)*) => {
        pub async fn $name(&self $(, $arg: $ty)*) -> crate::Result<Value> {
            $(let _ = $arg;)*
            Err(crate::Error::Unavailable)
        }
    };
}

impl<R: Runtime> NativeInference<R> {
    unavailable!(hardware);
    unavailable!(catalog);
    unavailable!(models);
    unavailable!(start_download, model_id: &str);
    unavailable!(download_status, job_id: &str);
    unavailable!(load_model, model_id: Option<&str>);
    unavailable!(status);
    unavailable!(unload);
    unavailable!(remove_model, model_id: &str);
    unavailable!(cancel, request_id: &str);

    pub async fn generate<F>(
        &self,
        _request_id: &str,
        _request: Value,
        _on_event: F,
    ) -> crate::Result<Value>
    where
        F: Fn(Value) + Send + Sync + 'static,
    {
        Err(crate::Error::Unavailable)
    }

    pub fn new_request_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}
