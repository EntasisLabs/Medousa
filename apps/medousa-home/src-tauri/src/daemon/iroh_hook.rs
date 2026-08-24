//! Iroh HTTP hook for [`medousa_sdk_iroh::WorkshopTransport`] (desktop + mobile).

use std::future::Future;
use std::pin::Pin;

use futures_util::{Stream, TryStreamExt};
use medousa_sdk::SdkError;
use medousa_sdk_iroh::IrohHttpHook;

#[derive(Clone)]
pub struct TauriIrohHook {
    ticket: String,
}

impl TauriIrohHook {
    pub fn new(ticket: impl Into<String>) -> Self {
        Self {
            ticket: ticket.into(),
        }
    }
}

fn diagnostic_path(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

fn iroh_status_error(method: &str, path: &str, status: u16) -> SdkError {
    let path = diagnostic_path(path);
    if status == 404 && method == "GET" && path == "/v1/health" {
        return SdkError::Compatibility(format!(
            "GET /v1/health returned HTTP 404 over iroh; responder does not expose the health operation required by daemon contract revision {}",
            medousa_sdk::DAEMON_API_CONTRACT_REVISION,
        ));
    }
    SdkError::Http(format!(
        "workshop returned HTTP {status} over iroh for {method} {path}"
    ))
}

fn iroh_request_error(method: &str, path: &str, error: impl std::fmt::Display) -> SdkError {
    SdkError::Http(format!(
        "iroh request failed for {method} {}: {error}",
        diagnostic_path(path),
    ))
}

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
            let mut response =
                medousa_iroh_http::iroh_http_request(&ticket, &method, &path, &header_refs, body)
                    .await
                    .map_err(|error| iroh_request_error(&method, &path, error))?;
            if !(200..300).contains(&response.status) {
                return Err(iroh_status_error(&method, &path, response.status));
            }
            let mut out = Vec::new();
            while let Some(chunk) = response
                .body
                .read_chunk()
                .await
                .map_err(|error| iroh_request_error(&method, &path, error))?
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
                let response =
                    medousa_iroh_http::iroh_http_request(&ticket, "GET", &path, &header_refs, None)
                        .await
                        .map_err(|error| iroh_request_error("GET", &path, error))?;
                if !(200..300).contains(&response.status) {
                    return Err(iroh_status_error("GET", &path, response.status));
                }
                Ok(iroh_body_stream(response.body))
            })
            .try_flatten(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::iroh_status_error;

    #[test]
    fn status_errors_name_the_operation_without_a_response_body() {
        let error = iroh_status_error("POST", "/v1/sessions?token=secret", 404).to_string();
        assert!(error.contains("POST /v1/sessions"));
        assert!(error.contains("HTTP 404 over iroh"));
        assert!(!error.contains("token"));
        assert!(!error.contains("secret"));
    }

    #[test]
    fn missing_health_is_a_typed_contract_error() {
        let error = iroh_status_error("GET", "/v1/health", 404);
        assert!(matches!(error, medousa_sdk::SdkError::Compatibility(_)));
        assert!(error.to_string().contains("contract revision"));
    }
}

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
