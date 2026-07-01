//! Optional Iroh HTTP hook for platforms that ship an Iroh client.

use std::future::Future;
use std::pin::Pin;

use medousa_sdk::SdkError;

#[cfg(feature = "sse")]
use futures_util::Stream;

/// Platform hook for HTTP over Iroh (mobile Tauri provides this via [`medousa_iroh_http`]).
pub trait IrohHttpHook: Send + Sync {
    fn request_json<'a>(
        &'a self,
        method: &'a str,
        path: &'a str,
        headers: &'a [(&'a str, &'a str)],
        body: Option<&'a [u8]>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, SdkError>> + Send + 'a>>;

    #[cfg(feature = "sse")]
    fn stream_sse(
        &self,
        path: String,
        headers: &[(&str, &str)],
    ) -> Pin<Box<dyn Stream<Item = Result<bytes::Bytes, SdkError>> + Send>>;
}
