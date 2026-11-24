//! SDK bridge to the same pairing refresh, pinned identity and LAN/Iroh routes
//! as the Tauri host. Config is resolved by saved id, never by global selection.
use medousa_desktop_host::{pairing_client::WorkshopTransportConfig, workshop_transport as host};
use medousa_sdk::{SdkError, Transport};
use serde_json::Value;
use std::{future::Future, pin::Pin};
type Json<'a> = Pin<Box<dyn Future<Output = Result<Value, SdkError>> + Send + 'a>>;
#[derive(Clone)]
pub struct SavedTransport(pub WorkshopTransportConfig, pub String);
fn error(e: String) -> SdkError {
    if e.starts_with("workshop returned HTTP ") {
        return SdkError::Http(e.trim_start_matches("workshop returned HTTP ").into());
    }
    SdkError::Transport(e)
}
fn path(path: &str) -> Result<(), SdkError> {
    if !path.starts_with('/') || path.starts_with("//") || path.contains('\\') || path.contains('#')
    {
        return Err(error(
            "Workshop request must use a relative API path".into(),
        ));
    }
    Ok(())
}
impl SavedTransport {
    async fn config(&self) -> Result<WorkshopTransportConfig, SdkError> {
        let id = self.1.clone();
        let pinned = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let raw =
                std::fs::read(medousa_desktop_host::workshop_registry::workshops_registry_path())
                    .map_err(|_| "Saved workshop unavailable".to_string())?;
            let registry: medousa_desktop_host::workshop_registry::WorkshopRegistry =
                serde_json::from_slice(&raw)
                    .map_err(|_| "Invalid saved workshop registry".to_string())?;
            let plan = medousa_desktop_host::connections::resolve(
                &registry,
                &id,
                &medousa_desktop_host::paths::medousa_data_dir(),
            )?;
            let config =
                medousa_desktop_host::pairing_client::load_workshop_transport_config_for_id(
                    &id,
                    &plan.origin,
                )
                .ok_or("Saved workshop credentials unavailable")?;
            if config.workshop_device_id != pinned.workshop_device_id
                || config.daemon_public_key != pinned.daemon_public_key
            {
                return Err("Saved workshop identity changed; existing items remain pinned".into());
            }
            Ok(config)
        })
        .await
        .map_err(|e| error(e.to_string()))?
        .map_err(error)
    }
    fn request<'a>(&'a self, method: &'a str, p: &'a str, body: Option<Value>) -> Json<'a> {
        Box::pin(async move {
            path(p)?;
            host::workshop_json_request(&self.config().await?, method, p, body.as_ref())
                .await
                .map_err(error)
        })
    }
    fn body<'a>(
        &'a self,
        method: &'a str,
        p: &'a str,
        body: Vec<u8>,
        mime: &'a str,
        headers: Vec<(&'static str, String)>,
    ) -> Json<'a> {
        Box::pin(async move {
            path(p)?;
            host::workshop_body_request(
                &self.config().await?,
                method,
                p,
                body,
                mime,
                headers.into_iter().map(|(k, v)| (k.into(), v)).collect(),
            )
            .await
            .map_err(error)
        })
    }
}
impl Transport for SavedTransport {
    fn get_json<'a>(&'a self, _: &'a str, p: &'a str) -> Json<'a> {
        self.request("GET", p, None)
    }
    fn post_json<'a>(&'a self, _: &'a str, p: &'a str, body: Value) -> Json<'a> {
        self.request("POST", p, Some(body))
    }
    fn put_json<'a>(&'a self, _: &'a str, p: &'a str, body: Value) -> Json<'a> {
        self.request("PUT", p, Some(body))
    }
    fn patch_json<'a>(&'a self, _: &'a str, p: &'a str, body: Value) -> Json<'a> {
        self.request("PATCH", p, Some(body))
    }
    fn delete_json<'a>(&'a self, _: &'a str, p: &'a str) -> Json<'a> {
        self.request("DELETE", p, None)
    }
    fn post_empty_json<'a>(&'a self, _: &'a str, p: &'a str) -> Json<'a> {
        self.request("POST", p, None)
    }
    fn post_json_with_headers<'a>(
        &'a self,
        _: &'a str,
        p: &'a str,
        body: Value,
        headers: Vec<(&'static str, String)>,
    ) -> Json<'a> {
        self.body(
            "POST",
            p,
            serde_json::to_vec(&body).expect("JSON serializes"),
            "application/json",
            headers,
        )
    }
    fn put_text<'a>(
        &'a self,
        _: &'a str,
        p: &'a str,
        body: String,
        headers: Vec<(&'static str, String)>,
    ) -> Json<'a> {
        self.body(
            "PUT",
            p,
            body.into_bytes(),
            "text/plain; charset=utf-8",
            headers,
        )
    }
    fn post_multipart<'a>(
        &'a self,
        _: &'a str,
        p: &'a str,
        filename: &'a str,
        mime: &'a str,
        bytes: Vec<u8>,
        label: Option<&'a str>,
    ) -> Json<'a> {
        Box::pin(async move {
            path(p)?;
            let mut fields = vec![host::MultipartField {
                name: "file".into(),
                filename: Some(filename.into()),
                mime: Some(mime.into()),
                data: bytes,
            }];
            if let Some(label) = label {
                fields.push(host::MultipartField {
                    name: "label".into(),
                    filename: None,
                    mime: None,
                    data: label.as_bytes().into(),
                });
            }
            host::workshop_post_multipart(&self.config().await?, p, &fields)
                .await
                .map_err(error)
        })
    }
    fn get_bytes<'a>(
        &'a self,
        _: &'a str,
        p: &'a str,
        max: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, SdkError>> + Send + 'a>> {
        Box::pin(async move {
            path(p)?;
            let mut stream = host::workshop_get_bytes_stream_with_accept(
                &self.config().await?,
                p,
                Some("application/octet-stream"),
            )
            .await
            .map_err(error)?;
            let mut bytes = Vec::new();
            while let Some(chunk) =
                tokio::time::timeout(std::time::Duration::from_secs(30), stream.next_chunk())
                    .await
                    .map_err(|_| error("Binary read timed out".into()))?
                    .map_err(error)?
            {
                if bytes.len().saturating_add(chunk.len()) > max {
                    return Err(error("Binary response exceeds size limit".into()));
                }
                bytes.extend(chunk);
            }
            Ok(bytes)
        })
    }
    fn stream_sse<'a>(
        &'a self,
        base: &'a str,
        p: String,
    ) -> Pin<Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, SdkError>> + Send + 'a>> {
        self.stream_sse_with_accept(base, p, "text/event-stream")
    }
    fn stream_sse_with_accept<'a>(
        &'a self,
        _: &'a str,
        p: String,
        accept: &'static str,
    ) -> Pin<Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, SdkError>> + Send + 'a>> {
        let config = self.clone();
        Box::pin(futures_util::stream::try_unfold(
            (None, Some((config, p))),
            move |(stream, opening)| async move {
                let mut stream = match stream {
                    Some(stream) => stream,
                    None => {
                        let (config, p) = opening.expect("stream initialized once");
                        path(&p)?;
                        host::workshop_get_bytes_stream_with_accept(
                            &config.config().await?,
                            &p,
                            Some(accept),
                        )
                        .await
                        .map_err(error)?
                    }
                };
                match stream.next_chunk().await.map_err(error)? {
                    Some(bytes) => Ok(Some((bytes::Bytes::from(bytes), (Some(stream), None)))),
                    None => Ok(None),
                }
            },
        ))
    }
}
