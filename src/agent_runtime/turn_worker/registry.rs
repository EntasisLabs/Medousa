//! Tool registry wrappers for host-bus and worker allowlists.

use std::collections::{BTreeSet, HashSet};
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
use crate::peer_execution_policy::{PeerExecutionPolicyStore, TaskExecutionGrant};
use crate::tool_bootstrap::{ToolSurfaceLane, effective_tool_names};

fn world_tool_surface(tool_name: &str) -> Option<medousa_world::WorldSurfaceKind> {
    if tool_name.starts_with("cognition_browser_") {
        Some(medousa_world::WorldSurfaceKind::Browser)
    } else if tool_name.starts_with("cognition_computer_") {
        Some(medousa_world::WorldSurfaceKind::Desktop)
    } else {
        None
    }
}

#[async_trait]
trait WorldBindingResolver: Send + Sync {
    async fn resolve(
        &self,
        world_id: &str,
        principal_id: &str,
        expires_at_ms: Option<u64>,
    ) -> std::result::Result<crate::world_execution::WorldExecutionBinding, String>;
}

struct DestinationWorldBindingResolver;

#[async_trait]
impl WorldBindingResolver for DestinationWorldBindingResolver {
    async fn resolve(
        &self,
        world_id: &str,
        principal_id: &str,
        expires_at_ms: Option<u64>,
    ) -> std::result::Result<crate::world_execution::WorldExecutionBinding, String> {
        crate::world_execution::WorldExecutionBinding::resolve_local(
            world_id,
            principal_id,
            expires_at_ms,
        )
        .await
    }
}

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

#[derive(Clone)]
pub struct WorldScopedToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    world_ids: BTreeSet<String>,
    principal_id: String,
    remote_authority: Option<(PeerExecutionPolicyStore, TaskExecutionGrant)>,
    resolver: Arc<dyn WorldBindingResolver>,
}

impl WorldScopedToolRegistry {
    pub fn new(
        inner: Arc<dyn ToolRegistry>,
        world_ids: impl IntoIterator<Item = String>,
        principal_id: impl Into<String>,
        remote_authority: Option<(PeerExecutionPolicyStore, TaskExecutionGrant)>,
    ) -> Self {
        Self {
            inner,
            world_ids: world_ids.into_iter().collect(),
            principal_id: principal_id.into(),
            remote_authority,
            resolver: Arc::new(DestinationWorldBindingResolver),
        }
    }

    #[cfg(test)]
    fn with_resolver(mut self, resolver: Arc<dyn WorldBindingResolver>) -> Self {
        self.resolver = resolver;
        self
    }

    fn verify_remote_world_set(&self) -> Result<Option<u64>> {
        let Some((policies, grant)) = self.remote_authority.as_ref() else {
            return Ok(None);
        };
        let granted_worlds = grant
            .effective_world_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if granted_worlds != self.world_ids {
            return Err(StasisError::PortFailure(
                "destination task grant does not authorize this exact world set".to_string(),
            ));
        }
        match policies.world_grant_is_active(grant) {
            Ok(true) => Ok(u64::try_from(grant.expires_at.timestamp_millis()).ok()),
            Ok(false) => Err(StasisError::PortFailure(
                "governed world authority was revoked by the destination workshop".to_string(),
            )),
            Err(error) => Err(StasisError::PortFailure(format!(
                "cannot revalidate governed world authority: {error}"
            ))),
        }
    }

    fn verify_remote_authority(&self, tool_name: &str) -> Result<Option<u64>> {
        let expires_at_ms = self.verify_remote_world_set()?;
        if let Some((_, grant)) = self.remote_authority.as_ref()
            && !grant
                .effective_tool_names
                .iter()
                .any(|name| name == tool_name)
        {
            return Err(StasisError::PortFailure(
                "destination task grant does not authorize this exact world tool".to_string(),
            ));
        }
        Ok(expires_at_ms)
    }
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
        supports_browser_host: bool,
    ) -> Self {
        Self {
            inner,
            session_id: session_id.into(),
            lane: ToolSurfaceLane::Worker,
            full_allowlist,
            supports_ui_artifacts: false,
            supports_browser_host,
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

fn bind_world_tool_schema(tool: &mut Tool, world_ids: &BTreeSet<String>) {
    let schema = tool.schema.get_or_insert_with(|| {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    });
    let Some(schema) = schema.as_object_mut() else {
        return;
    };
    let properties = schema
        .entry("properties")
        .or_insert_with(|| serde_json::json!({}));
    let Some(properties) = properties.as_object_mut() else {
        return;
    };
    properties.insert(
        "world_id".to_string(),
        serde_json::json!({
            "type": "string",
            "enum": world_ids.iter().collect::<Vec<_>>(),
            "description": "Exact eligible world to use for this operation."
        }),
    );
    if tool.name.as_str().starts_with("cognition_computer_") {
        properties.remove("driver_id");
    }
    let required = schema
        .entry("required")
        .or_insert_with(|| serde_json::json!([]));
    if let Some(required) = required.as_array_mut() {
        required.retain(|field| field.as_str() != Some("driver_id"));
        if !required.iter().any(|field| field.as_str() == Some("world_id")) {
            required.push(Value::String("world_id".to_string()));
        }
    }
}

fn take_world_id(tool_name: &str, input: &mut Value) -> Result<String> {
    let input = input.as_object_mut().ok_or_else(|| {
        StasisError::PortFailure(format!("{tool_name}: world tool input must be an object"))
    })?;
    input
        .remove("world_id")
        .and_then(|value| value.as_str().map(str::to_string))
        .filter(|value| !value.trim().is_empty() && value.trim() == value)
        .ok_or_else(|| {
            StasisError::PortFailure(format!("{tool_name}: exact world_id is required"))
        })
}

fn bind_computer_driver(
    tool_name: &str,
    input: &mut Value,
    driver_id: &str,
) -> Result<()> {
    let input = input.as_object_mut().ok_or_else(|| {
        StasisError::PortFailure(format!("{tool_name}: world tool input must be an object"))
    })?;
    if let Some(requested) = input
        .get("driver_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && requested != driver_id
    {
        return Err(StasisError::PortFailure(format!(
            "{tool_name}: driver_id does not belong to the selected world"
        )));
    }
    input.insert(
        "driver_id".to_string(),
        Value::String(driver_id.to_string()),
    );
    Ok(())
}

fn redact_world_mechanics(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("driver_id");
            map.remove("authority_id");
            for value in map.values_mut() {
                redact_world_mechanics(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_world_mechanics(value);
            }
        }
        _ => {}
    }
}

#[async_trait]
impl ToolRegistry for WorldScopedToolRegistry {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        let tools = self.inner.list_tools().await?;
        let expires_at_ms = if self.world_ids.is_empty() {
            None
        } else {
            self.verify_remote_world_set()?
        };
        let mut bindings = Vec::with_capacity(self.world_ids.len());
        for world_id in &self.world_ids {
            bindings.push(
                self.resolver
                    .resolve(world_id, &self.principal_id, expires_at_ms)
                    .await
                    .map_err(|error| {
                        StasisError::PortFailure(format!(
                            "selected world is unavailable at this destination: {error}"
                        ))
                    })?,
            );
        }
        Ok(tools
            .into_iter()
            .filter_map(|mut tool| {
                if let Some(surface) = world_tool_surface(tool.name.as_str()) {
                    let eligible_ids = bindings
                        .iter()
                        .filter(|binding| binding.surface() == surface)
                        .map(|binding| binding.world_id().as_str().to_string())
                        .collect::<BTreeSet<_>>();
                    if eligible_ids.is_empty() {
                        return None;
                    }
                    bind_world_tool_schema(&mut tool, &eligible_ids);
                }
                Some(tool)
            })
            .collect())
    }

    async fn invoke_tool(&self, tool_name: &str, mut input: Value) -> Result<Value> {
        let Some(expected_surface) = world_tool_surface(tool_name) else {
            return self.inner.invoke_tool(tool_name, input).await;
        };
        let expires_at_ms = self.verify_remote_authority(tool_name)?;
        let world_id = take_world_id(tool_name, &mut input)?;
        if !self.world_ids.contains(&world_id) {
            return Err(StasisError::PortFailure(format!(
                "{tool_name}: world_id is outside this worker's immutable authority"
            )));
        }
        let binding = self
            .resolver
            .resolve(&world_id, &self.principal_id, expires_at_ms)
            .await
            .map_err(|error| {
                StasisError::PortFailure(format!(
                    "{tool_name}: selected world is unavailable at this destination: {error}"
                ))
            })?;
        if binding.surface() != expected_surface {
            return Err(StasisError::PortFailure(format!(
                "{tool_name}: selected world has the wrong surface"
            )));
        }
        if expected_surface == medousa_world::WorldSurfaceKind::Desktop {
            bind_computer_driver(tool_name, &mut input, binding.driver_id().as_str())?;
        }
        let mut output = crate::world_execution::with_world_execution(
            binding,
            self.inner.invoke_tool(tool_name, input),
        )
        .await?;
        redact_world_mechanics(&mut output);
        Ok(output)
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
    use std::sync::Mutex;

    struct RecordingRegistry {
        tool_name: &'static str,
        seen: Arc<Mutex<Vec<Value>>>,
    }

    #[async_trait]
    impl ToolRegistry for RecordingRegistry {
        async fn list_tools(&self) -> Result<Vec<Tool>> {
            Ok(vec![Tool::new(self.tool_name).with_schema(json!({
                "type": "object",
                "properties": {
                    "driver_id": { "type": "string" },
                    "value": { "type": "string" }
                },
                "required": ["driver_id", "value"],
                "additionalProperties": false
            }))])
        }

        async fn invoke_tool(&self, _tool_name: &str, input: Value) -> Result<Value> {
            self.seen.lock().unwrap().push(input.clone());
            let binding = crate::world_execution::active_world_execution()
                .expect("world binding should cover the inner invocation");
            Ok(json!({
                "input": input,
                "world_id": binding.world_id().as_str(),
                "driver_id": binding.driver_id().as_str(),
                "authority_id": binding.authority_id()
            }))
        }
    }

    struct TestWorldResolver {
        surface: medousa_world::WorldSurfaceKind,
    }

    #[async_trait]
    impl WorldBindingResolver for TestWorldResolver {
        async fn resolve(
            &self,
            world_id: &str,
            _principal_id: &str,
            _expires_at_ms: Option<u64>,
        ) -> std::result::Result<crate::world_execution::WorldExecutionBinding, String> {
            Ok(crate::world_execution::WorldExecutionBinding::for_test(
                world_id,
                "driver:test",
                self.surface,
            ))
        }
    }

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

    #[tokio::test]
    async fn world_registry_binds_only_opaque_world_and_hides_driver_mechanics() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let inner = Arc::new(RecordingRegistry {
            tool_name: "cognition_computer_snapshot",
            seen: seen.clone(),
        });
        let registry = WorldScopedToolRegistry::new(
            inner,
            ["world:computer:test".to_string()],
            "worker:test",
            None,
        )
        .with_resolver(Arc::new(TestWorldResolver {
            surface: medousa_world::WorldSurfaceKind::Desktop,
        }));

        let tools = registry.list_tools().await.unwrap();
        let schema = tools[0].schema.as_ref().unwrap();
        assert_eq!(
            schema["properties"]["world_id"]["enum"],
            json!(["world:computer:test"])
        );
        assert!(schema["properties"].get("driver_id").is_none());
        assert_eq!(schema["required"], json!(["value", "world_id"]));

        let output = registry
            .invoke_tool(
                "cognition_computer_snapshot",
                json!({ "world_id": "world:computer:test", "value": "hello" }),
            )
            .await
            .unwrap();
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            [json!({ "driver_id": "driver:test", "value": "hello" })]
        );
        assert_eq!(output["world_id"], "world:computer:test");
        assert!(output.get("driver_id").is_none());
        assert!(output.get("authority_id").is_none());
        assert!(crate::world_execution::active_world_execution().is_none());
    }

    #[tokio::test]
    async fn world_registry_hides_ambient_tools_and_rejects_other_worlds() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let inner = Arc::new(RecordingRegistry {
            tool_name: "cognition_browser_snapshot",
            seen,
        });
        let empty = WorldScopedToolRegistry::new(
            inner.clone(),
            Vec::new(),
            "worker:test",
            None,
        );
        assert!(empty.list_tools().await.unwrap().is_empty());

        let scoped = WorldScopedToolRegistry::new(
            inner,
            ["world:browser:allowed".to_string()],
            "worker:test",
            None,
        )
        .with_resolver(Arc::new(TestWorldResolver {
            surface: medousa_world::WorldSurfaceKind::Browser,
        }));
        let error = scoped
            .invoke_tool(
                "cognition_browser_snapshot",
                json!({ "world_id": "world:browser:other" }),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("immutable authority"));
    }
}
