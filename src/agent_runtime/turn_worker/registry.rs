//! Tool registry wrappers for host-bus and worker allowlists.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use genai::chat::Tool;
use serde_json::Value;
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::StasisError;
use stasis::prelude::Result;

use super::policy::tool_allowed;

#[derive(Clone)]
pub struct AllowlistToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    allowlist: HashSet<String>,
}

impl AllowlistToolRegistry {
    pub fn new(inner: Arc<dyn ToolRegistry>, allowlist: HashSet<String>) -> Self {
        Self { inner, allowlist }
    }
}

#[async_trait]
impl ToolRegistry for AllowlistToolRegistry {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        let tools = self.inner.list_tools().await?;
        Ok(tools
            .into_iter()
            .filter(|tool| tool_allowed(&tool.name, &self.allowlist))
            .collect())
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        if !tool_allowed(tool_name, &self.allowlist) {
            return Err(StasisError::PortFailure(format!(
                "tool not allowed in this turn profile: {tool_name}"
            )));
        }
        self.inner.invoke_tool(tool_name, input).await
    }
}
