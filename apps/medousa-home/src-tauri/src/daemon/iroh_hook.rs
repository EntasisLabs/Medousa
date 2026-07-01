//! Mobile Iroh hook for [`medousa_sdk_iroh::WorkshopTransport`].

#[cfg(any(target_os = "ios", target_os = "android"))]
use std::future::Future;
#[cfg(any(target_os = "ios", target_os = "android"))]
use std::pin::Pin;

#[cfg(any(target_os = "ios", target_os = "android"))]
use futures_util::{Stream, StreamExt, TryStreamExt};

#[cfg(any(target_os = "ios", target_os = "android"))]
use medousa_sdk::SdkError;
#[cfg(any(target_os = "ios", target_os = "android"))]
use medousa_sdk_iroh::IrohHttpHook;

#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Clone)]
pub struct TauriIrohHook {
    ticket: String,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
impl TauriIrohHook {
    pub fn new(ticket: impl Into<String>) -> Self {
        Self {
            ticket: ticket.into(),
        }
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
impl IrohHttpHook for TauriIrohHook {
    fn request_json<'a>(
        &'a self,
        method: &'a str,
        path: &'a str,
        headers: &'a [(&'a str, &'a str)],
        body: Option<&'a [u8]>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, SdkError>> + Send + 'a>> {
        let ticket = self.ticket.clone();
        let method = method.to_string();
        let path = path.to_string();
        let headers = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<Vec<_>>();
        Box::pin(async move {
            let header_refs: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let mut response = medousa_iroh_http::iroh_http_request(
                &ticket,
                &method,
                &path,
                &header_refs,
                body,
            )
            .await
            .map_err(|err| SdkError::Http(err.to_string()))?;
            let mut out = Vec::new();
            while let Some(chunk) = response
                .body
                .read_chunk()
                .await
                .map_err(|err| SdkError::Http(err.to_string()))?
            {
                out.extend_from_slice(&chunk);
            }
            Ok(out)
        })
    }

    fn stream_sse(
        &self,
        path: String,
        headers: &[(&str, &str)],
    ) -> Pin<Box<dyn Stream<Item = Result<bytes::Bytes, SdkError>> + Send>> {
        let ticket = self.ticket.clone();
        let headers = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<Vec<_>>();
        Box::pin(
            futures_util::stream::once(async move {
                let header_refs: Vec<(&str, &str)> = headers
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let response = medousa_iroh_http::iroh_http_request(
                    &ticket,
                    "GET",
                    &path,
                    &header_refs,
                    None,
                )
                .await
                .map_err(|err| SdkError::Http(err.to_string()))?;
                if !(200..300).contains(&response.status) {
                    return Err(SdkError::Http(format!(
                        "workshop returned HTTP {} over iroh",
                        response.status
                    )));
                }
                Ok(iroh_body_stream(response.body))
            })
            .try_flatten(),
        )
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn iroh_body_stream(
    body: medousa_iroh_http::IrohHttpBody,
) -> impl Stream<Item = Result<bytes::Bytes, SdkError>> {
    futures_util::stream::unfold(body, |mut body| async move {
        match body.read_chunk().await {
            Ok(Some(chunk)) => Some((Ok(bytes::Bytes::from(chunk)), body)),
            Ok(None) => None,
            Err(err) => Some((Err(SdkError::Http(err.to_string())), body)),
        }
    })
}
