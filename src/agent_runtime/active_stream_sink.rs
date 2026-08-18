//! Per-turn tool sink ambient context (replaces legacy `active_stream_sink`).

pub use crate::engine_adapters::{
    AgentStreamToolSinkAdapter, active_tool_sink, with_active_tool_sink,
};
