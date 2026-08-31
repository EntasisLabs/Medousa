use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::plugin::{PluginApi, PluginHandle};
use tauri::{AppHandle, Runtime};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_native_inference);

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<NativeInference<R>> {
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_native_inference)?;
    Ok(NativeInference(handle))
}

pub struct NativeInference<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Clone for NativeInference<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<R: Runtime> NativeInference<R> {
    async fn call<T: DeserializeOwned>(
        &self,
        command: &str,
        payload: impl Serialize,
    ) -> crate::Result<T> {
        Ok(self.0.run_mobile_plugin_async(command, payload).await?)
    }

    pub async fn hardware(&self) -> crate::Result<Value> {
        self.call("hardware", json!({})).await
    }

    pub async fn catalog(&self) -> crate::Result<Value> {
        self.call("catalog", json!({})).await
    }

    pub async fn models(&self) -> crate::Result<Value> {
        self.call("models", json!({})).await
    }

    pub async fn start_download(&self, model_id: &str) -> crate::Result<Value> {
        self.call("startDownload", json!({ "modelId": model_id }))
            .await
    }

    pub async fn download_status(&self, job_id: &str) -> crate::Result<Value> {
        self.call("downloadStatus", json!({ "jobId": job_id }))
            .await
    }

    pub async fn load_model(&self, model_id: Option<&str>) -> crate::Result<Value> {
        self.call("loadModel", json!({ "modelId": model_id })).await
    }

    pub async fn status(&self) -> crate::Result<Value> {
        self.call("status", json!({})).await
    }

    pub async fn unload(&self) -> crate::Result<Value> {
        self.call("unload", json!({})).await
    }

    pub async fn remove_model(&self, model_id: &str) -> crate::Result<Value> {
        self.call("removeModel", json!({ "modelId": model_id }))
            .await
    }

    pub async fn generate<F>(
        &self,
        request_id: &str,
        request: Value,
        on_event: F,
    ) -> crate::Result<Value>
    where
        F: Fn(Value) + Send + Sync + 'static,
    {
        let channel = Channel::new(move |body| {
            if let InvokeResponseBody::Json(payload) = body {
                if let Ok(event) = serde_json::from_str(&payload) {
                    on_event(event);
                }
            }
            Ok(())
        });
        self.call(
            "generate",
            GeneratePayload {
                request_id,
                request,
                on_event: channel,
            },
        )
        .await
    }

    pub async fn cancel(&self, request_id: &str) -> crate::Result<Value> {
        self.call("cancel", json!({ "requestId": request_id }))
            .await
    }

    pub fn new_request_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeneratePayload<'a> {
    request_id: &'a str,
    request: Value,
    on_event: Channel<Value>,
}
