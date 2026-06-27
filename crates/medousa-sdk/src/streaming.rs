//! SSE byte-stream parsing for daemon event streams.

#[cfg(feature = "sse")]
use std::pin::Pin;
#[cfg(feature = "sse")]
use std::task::{Context, Poll};

#[cfg(feature = "sse")]
use futures_util::Stream;

#[cfg(feature = "sse")]
use crate::SdkError;

/// Parse SSE `data:` lines from a byte stream into JSON payload strings.
#[cfg(feature = "sse")]
pub struct SseLineStream<S> {
    inner: S,
    buffer: String,
}

#[cfg(feature = "sse")]
impl<S> SseLineStream<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
        }
    }
}

#[cfg(feature = "sse")]
impl<S> Stream for SseLineStream<S>
where
    S: Stream<Item = Result<bytes::Bytes, SdkError>> + Unpin,
{
    type Item = Result<String, SdkError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(line) = self.buffer.lines().next() {
                let line = line.to_string();
                let consumed = line.len() + 1;
                self.buffer.drain(..consumed);
                if line.starts_with(':') {
                    continue;
                }
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim().to_string();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    return Poll::Ready(Some(Ok(data)));
                }
                continue;
            }
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.buffer.push_str(&String::from_utf8_lossy(&chunk));
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    if self.buffer.is_empty() {
                        return Poll::Ready(None);
                    }
                    self.buffer.push('\n');
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(feature = "sse")]
pub fn decode_sse_json<T: serde::de::DeserializeOwned>(
    data: &str,
) -> Result<T, SdkError> {
    serde_json::from_str(data).map_err(Into::into)
}
