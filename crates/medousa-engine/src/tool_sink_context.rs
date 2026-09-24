//! Turn-scoped delivery context for events emitted by tools.
//!
//! Tool implementations still use registries whose invocation API does not
//! accept an explicit context. Keeping the scope next to `ToolSinkPort` lets
//! the host establish delivery once and lets runtimes preserve it at task
//! boundaries where Tokio task locals do not propagate automatically.

use std::future::Future;
use std::sync::Arc;

use crate::ToolSinkPort;

tokio::task_local! {
    static ACTIVE_TOOL_SINK: Arc<dyn ToolSinkPort + Send + Sync>;
}

/// Run a turn or tool invocation with its event-delivery capability scoped to
/// that future. Concurrent turns retain independent sinks.
pub async fn with_active_tool_sink<F>(
    sink: Arc<dyn ToolSinkPort + Send + Sync>,
    future: F,
) -> F::Output
where
    F: Future,
{
    ACTIVE_TOOL_SINK.scope(sink, future).await
}

/// Read the sink for the current turn, if this future is inside a scoped run.
pub async fn active_tool_sink() -> Option<Arc<dyn ToolSinkPort + Send + Sync>> {
    ACTIVE_TOOL_SINK.try_with(Arc::clone).ok()
}
