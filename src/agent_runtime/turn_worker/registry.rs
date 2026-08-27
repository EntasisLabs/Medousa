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
use crate::browser_tools::BROWSER_COGNITION_TOOLS;
use crate::client_tools::ClientRegistry;
use crate::tool_bootstrap::{ToolSurfaceLane, effective_tool_names};

fn memory_tool_needs_session(tool_name: &str) -> bool {
    let lower = tool_name.to_ascii_lowercase();
    lower.contains("cognition_memory") || lower.contains("memory_")
}

/// Inject worker `session_id` before policy validation (models often pass null).
pub fn inject_worker_session_id(input: Value, session_id: &str) -> Value {
    let Some(session_id) = session_id.trim().non_empty() else {
        return input;
    };
    let mut value = input;
    let Some(map) = value.as_object_mut() else {
        return value;
    };
    let replace = match map.get("session_id") {
        None => true,
        Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::String(s)) => s.trim().is_empty(),
        _ => false,
    };
    if replace {
        map.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id.to_string()),
        );
    }
    value
}

trait NonEmpty {
    fn non_empty(self) -> Option<Self>
    where
        Self: Sized;
}

impl NonEmpty for &str {
    fn non_empty(self) -> Option<Self> {
        let t = self.trim();
        if t.is_empty() { None } else { Some(t) }
    }
}

#[derive(Clone)]
pub struct WorkerSessionToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    session_id: String,
}

impl WorkerSessionToolRegistry {
    pub fn new(inner: Arc<dyn ToolRegistry>, session_id: impl Into<String>) -> Self {
        Self {
            inner,
            session_id: session_id.into(),
        }
    }
}

#[derive(Clone)]
pub struct AllowlistToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    allowlist: HashSet<String>,
    include_public_api: bool,
    delegated_finish_only: bool,
}

impl AllowlistToolRegistry {
    pub fn new(inner: Arc<dyn ToolRegistry>, allowlist: HashSet<String>) -> Self {
        Self {
            inner,
            allowlist,
            include_public_api: true,
            delegated_finish_only: false,
        }
    }

    /// Apply an exact deployment ceiling without implicitly expanding it to
    /// every public API tool.
    ///
    /// This is a registry filter, not a capability grant or work-admission
    /// boundary. The daemon must still admit the turn through Stasis and apply
    /// any narrower authenticated policy.
    pub fn new_exact(inner: Arc<dyn ToolRegistry>, allowlist: HashSet<String>) -> Self {
        Self {
            inner,
            allowlist,
            include_public_api: false,
            delegated_finish_only: false,
        }
    }

    pub fn delegated(inner: Arc<dyn ToolRegistry>, allowlist: HashSet<String>) -> Self {
        Self {
            inner,
            allowlist,
            include_public_api: false,
            delegated_finish_only: true,
        }
    }

    fn allows(&self, tool_name: &str) -> bool {
        (self.include_public_api && crate::public_api::is_public_api_tool(tool_name))
            || tool_allowed(tool_name, &self.allowlist)
    }
}

#[derive(Clone)]
pub struct SessionBootstrapToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    session_id: String,
    lane: ToolSurfaceLane,
    full_allowlist: HashSet<String>,
    supports_ui_artifacts: bool,
    supports_browser_host: bool,
    channel_surface: Option<String>,
    client_registry: ClientRegistry,
}

impl SessionBootstrapToolRegistry {
    pub fn host(
        inner: Arc<dyn ToolRegistry>,
        session_id: impl Into<String>,
        full_allowlist: HashSet<String>,
        supports_ui_artifacts: bool,
        supports_browser_host: bool,
        channel_surface: Option<String>,
        client_registry: ClientRegistry,
    ) -> Self {
        let session_id = session_id.into();
        Self {
            inner,
            session_id,
            lane: ToolSurfaceLane::Host,
            full_allowlist,
            supports_ui_artifacts,
            supports_browser_host,
            channel_surface,
            client_registry,
        }
    }

    pub fn worker(
        inner: Arc<dyn ToolRegistry>,
        session_id: impl Into<String>,
        full_allowlist: HashSet<String>,
    ) -> Self {
        Self {
            inner,
            session_id: session_id.into(),
            lane: ToolSurfaceLane::Worker,
            full_allowlist,
            supports_ui_artifacts: false,
            supports_browser_host: false,
            channel_surface: None,
            client_registry: ClientRegistry::new(),
        }
    }

    /// Bound workshop lane — full execution surface including environment/canvas tools.
    pub fn bound_workshop(
        inner: Arc<dyn ToolRegistry>,
        session_id: impl Into<String>,
        full_allowlist: HashSet<String>,
        supports_ui_artifacts: bool,
        supports_browser_host: bool,
        channel_surface: Option<String>,
        client_registry: ClientRegistry,
    ) -> Self {
        let session_id = session_id.into();
        Self {
            inner,
            session_id,
            lane: ToolSurfaceLane::Worker,
            full_allowlist,
            supports_ui_artifacts,
            supports_browser_host,
            channel_surface,
            client_registry,
        }
    }

    fn effective_allowlist(&self) -> HashSet<String> {
        let mut allowed = effective_tool_names(&self.session_id, self.lane, &self.full_allowlist);
        allowed.extend(
            self.client_registry
                .tool_names_for_surface(self.channel_surface.as_deref()),
        );
        if !self.supports_ui_artifacts {
            allowed.remove(crate::ui_present_tools::COGNITION_UI_PRESENT);
            allowed.remove(crate::ui_scene_tools::COGNITION_UI_SCENE);
            allowed.remove(crate::ui_build_tools::COGNITION_UI_BUILD);
        }
        if !self.supports_browser_host {
            for name in BROWSER_COGNITION_TOOLS {
                allowed.remove(*name);
            }
        }
        allowed
    }
}

#[async_trait]
impl ToolRegistry for SessionBootstrapToolRegistry {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        let allowed = self.effective_allowlist();
        let tools = self.inner.list_tools().await?;
        Ok(tools
            .into_iter()
            .filter(|tool| {
                crate::public_api::is_public_api_tool(tool.name.as_str())
                    || tool_allowed(tool.name.as_str(), &allowed)
            })
            .collect())
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        if !crate::public_api::is_public_api_tool(tool_name)
            && !tool_allowed(tool_name, &self.effective_allowlist())
        {
            return Err(StasisError::PortFailure(format!(
                "tool is outside this session's immutable lane ceiling: {tool_name}"
            )));
        }
        self.inner.invoke_tool(tool_name, input).await
    }
}

#[async_trait]
impl ToolRegistry for AllowlistToolRegistry {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        let tools = self.inner.list_tools().await?;
        Ok(tools
            .into_iter()
            .filter_map(|mut tool| {
                if !self.allows(tool.name.as_str()) {
                    return None;
                }
                if self.delegated_finish_only
                    && tool.name.as_str() == crate::public_api::COGNITION_TURN
                {
                    tool.description = Some(
                        "Finish authenticated delegated work and return its final result."
                            .to_string(),
                    );
                    tool.schema = Some(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "action": { "type": "string", "enum": ["turn.finish"] },
                            "message": { "type": "string", "minLength": 1 },
                            "reason": { "type": "string" }
                        },
                        "required": ["action", "message"],
                        "additionalProperties": false
                    }));
                    tool.strict = Some(true);
                }
                Some(tool)
            })
            .collect())
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        if !self.allows(tool_name) {
            return Err(StasisError::PortFailure(format!(
                "tool not allowed in this turn profile: {tool_name}"
            )));
        }
        if self.delegated_finish_only
            && tool_name == crate::public_api::COGNITION_TURN
            && input.get("action").and_then(Value::as_str) != Some("turn.finish")
        {
            return Err(StasisError::PortFailure(
                "delegated workers may only use cognition_turn action=turn.finish".to_string(),
            ));
        }
        self.inner.invoke_tool(tool_name, input).await
    }
}

#[async_trait]
impl ToolRegistry for WorkerSessionToolRegistry {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        self.inner.list_tools().await
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        let input = if memory_tool_needs_session(tool_name) {
            inject_worker_session_id(input, &self.session_id)
        } else {
            input
        };
        self.inner.invoke_tool(tool_name, input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn injects_session_when_null() {
        let out = inject_worker_session_id(
            json!({ "session_id": null, "stability": 0.9 }),
            "my-session",
        );
        assert_eq!(out["session_id"], "my-session");
    }

    #[test]
    fn preserves_explicit_session() {
        let out = inject_worker_session_id(json!({ "session_id": "other" }), "my-session");
        assert_eq!(out["session_id"], "other");
    }

    #[test]
    fn browser_tools_stripped_when_host_disabled() {
        use stasis::application::orchestration::tool_registry::InMemoryToolRegistry;
        use std::collections::HashSet;
        use std::sync::Arc;

        let inner = Arc::new(InMemoryToolRegistry::default());
        let registry = SessionBootstrapToolRegistry::host(
            inner,
            "sess-1",
            HashSet::from([
                "cognition_web_search".to_string(),
                crate::browser_tools::COGNITION_BROWSER_FETCH.to_string(),
            ]),
            false,
            false,
            None,
            ClientRegistry::new(),
        );
        let allowed = registry.effective_allowlist();
        assert!(allowed.contains("cognition_web_search"));
        assert!(!allowed.contains(crate::browser_tools::COGNITION_BROWSER_FETCH));
    }

    #[test]
    fn exact_allowlist_does_not_implicitly_expand_to_public_api_tools() {
        use stasis::application::orchestration::tool_registry::InMemoryToolRegistry;

        let inner = Arc::new(InMemoryToolRegistry::default());
        let exact = AllowlistToolRegistry::new_exact(
            inner.clone(),
            HashSet::from(["cognition_utility_uuid".to_string()]),
        );
        assert!(exact.allows("cognition_utility_uuid"));
        assert!(!exact.allows(crate::public_api::COGNITION_IDENTITY_QUERY));

        let lane = AllowlistToolRegistry::new(inner, HashSet::new());
        assert!(lane.allows(crate::public_api::COGNITION_IDENTITY_QUERY));
    }
}
