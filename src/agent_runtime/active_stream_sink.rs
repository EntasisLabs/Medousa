//! Per-turn stream sink for tools that emit SSE (browser challenge, UI artifacts, …).

use std::sync::Arc;

use once_cell::sync::Lazy;
use tokio::sync::RwLock;

use super::stream_sink::SharedAgentStreamSink;

static ACTIVE: Lazy<RwLock<Option<SharedAgentStreamSink>>> =
    Lazy::new(|| RwLock::new(None));

pub async fn set_active_stream_sink(sink: Option<SharedAgentStreamSink>) {
    *ACTIVE.write().await = sink;
}

pub async fn active_stream_sink() -> Option<SharedAgentStreamSink> {
    ACTIVE.read().await.clone()
}

pub async fn with_active_stream_sink<F, Fut, T>(f: F) -> T
where
    F: FnOnce(Option<SharedAgentStreamSink>) -> Fut,
    Fut: std::future::Future<Output = T>,
{
    f(active_stream_sink().await).await
}
