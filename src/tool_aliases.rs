//! Snake_case aliases for legacy dot-named agent tools (Phase C migration).

use async_trait::async_trait;
use serde_json::Value;
use stasis::application::orchestration::tool_registry::StasisTool;

/// Wraps a tool under an alternate model-facing name; delegates invoke/schema/description.
pub struct ToolNameAlias<T> {
    alias: &'static str,
    inner: T,
}

impl<T> ToolNameAlias<T> {
    pub fn new(alias: &'static str, inner: T) -> Self {
        Self { alias, inner }
    }
}

#[async_trait]
impl<T: StasisTool + Sync> StasisTool for ToolNameAlias<T> {
    fn name(&self) -> &'static str {
        self.alias
    }

    fn description(&self) -> Option<&'static str> {
        self.inner.description()
    }

    fn input_schema(&self) -> Option<Value> {
        self.inner.input_schema()
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        self.inner.invoke(input).await
    }
}
