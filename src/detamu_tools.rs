//! Opt-in Detamu world-model cognition tools (domain `detamu`).
//!
//! Distinct from Locus AVEC and from the coding domain. Scores must be named
//! `code_avec` / never bare `avec`. See architecture/detamu-medousa-fit.md.

use async_trait::async_trait;
use serde_json::{json, Value};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};

pub const COGNITION_DETAMU_STATUS: &str = "cognition_detamu_status";
pub const COGNITION_DETAMU_FILES: &str = "cognition_detamu_files";
pub const COGNITION_DETAMU_IMPACT: &str = "cognition_detamu_impact";
pub const COGNITION_DETAMU_CODE_AVEC: &str = "cognition_detamu_code_avec";
pub const COGNITION_DETAMU_FIND: &str = "cognition_detamu_find";

pub const DETAMU_COGNITION_TOOLS: &[&str] = &[
    COGNITION_DETAMU_STATUS,
    COGNITION_DETAMU_FILES,
    COGNITION_DETAMU_IMPACT,
    COGNITION_DETAMU_CODE_AVEC,
    COGNITION_DETAMU_FIND,
];

pub fn is_detamu_cognition_tool(name: &str) -> bool {
    DETAMU_COGNITION_TOOLS.contains(&name)
}

fn daemon_base() -> String {
    std::env::var("MEDOUSA_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:8741".into())
}

async fn daemon_get(path: &str) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let url = format!("{}{path}", daemon_base().trim_end_matches('/'));
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| StasisError::PortFailure(format!("detamu proxy: {e}")))?;
    let status = resp.status();
    let value = resp
        .json::<Value>()
        .await
        .map_err(|e| StasisError::PortFailure(e.to_string()))?;
    if !status.is_success() {
        return Err(StasisError::PortFailure(format!("daemon {status}: {value}")));
    }
    Ok(value)
}

async fn daemon_get_query(path: &str, query: &[(&str, String)]) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let mut url = reqwest::Url::parse(&format!(
        "{}{path}",
        daemon_base().trim_end_matches('/')
    ))
    .map_err(|e| StasisError::PortFailure(e.to_string()))?;
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in query {
            q.append_pair(k, v);
        }
    }
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| StasisError::PortFailure(format!("detamu proxy: {e}")))?;
    let status = resp.status();
    let value = resp
        .json::<Value>()
        .await
        .map_err(|e| StasisError::PortFailure(e.to_string()))?;
    if !status.is_success() {
        return Err(StasisError::PortFailure(format!("daemon {status}: {value}")));
    }
    Ok(value)
}

pub struct CognitionDetamuStatusTool;
pub struct CognitionDetamuFilesTool;
pub struct CognitionDetamuImpactTool;
pub struct CognitionDetamuCodeAvecTool;
pub struct CognitionDetamuFindTool;

fn push_snapshot_query(input: &Value, query: &mut Vec<(String, String)>) {
    if let Some(v) = input.get("work_id").and_then(|v| v.as_str()) {
        query.push(("work_id".into(), v.to_owned()));
    }
    if let Some(v) = input.get("world").and_then(|v| v.as_str()) {
        query.push(("world".into(), v.to_owned()));
    }
    if let Some(v) = input.get("version").and_then(|v| v.as_str()) {
        query.push(("version".into(), v.to_owned()));
    }
}

fn require_snapshot_selector(input: &Value) -> StasisResult<()> {
    let has_work = input
        .get("work_id")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let has_world = input
        .get("world")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
        && input
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
    if !has_work && !has_world {
        return Err(StasisError::PortFailure(
            "provide work_id or world+version".into(),
        ));
    }
    Ok(())
}

async fn daemon_get_query_pairs(path: &str, query: &[(String, String)]) -> StasisResult<Value> {
    let refs: Vec<(&str, String)> = query
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    daemon_get_query(path, &refs).await
}

#[async_trait]
impl StasisTool for CognitionDetamuStatusTool {
    fn name(&self) -> &'static str {
        COGNITION_DETAMU_STATUS
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Probe Detamu world-model host readiness and optional Forge work binding \
             (baseline/sealed snapshot pointers). Detamu domain only — opt-in.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": {
                    "type": "string",
                    "description": "Optional Forge work id — return Detamu snapshot binding"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let mut status = daemon_get("/v1/world/status").await?;
        if let Some(work_id) = input
            .get("work_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            match daemon_get(&format!("/v1/world/bindings/{work_id}")).await {
                Ok(binding) => {
                    if let Some(obj) = status.as_object_mut() {
                        obj.insert("binding".into(), binding);
                    }
                }
                Err(err) => {
                    if let Some(obj) = status.as_object_mut() {
                        obj.insert(
                            "binding_error".into(),
                            json!(err.to_string()),
                        );
                    }
                }
            }
        }
        Ok(status)
    }
}

#[async_trait]
impl StasisTool for CognitionDetamuFilesTool {
    fn name(&self) -> &'static str {
        COGNITION_DETAMU_FILES
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "List file entities from a Detamu snapshot (inventory). Prefer work_id when a \
             Forge undertaking is bound. Detamu domain only — opt-in.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": { "type": "string" },
                "world": { "type": "string" },
                "version": { "type": "string", "description": "Snapshot version (commit OID)" },
                "path": { "type": "string", "description": "Optional path filter" },
                "limit": { "type": "integer" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let mut query = Vec::new();
        if let Some(v) = input.get("work_id").and_then(|v| v.as_str()) {
            query.push(("work_id", v.to_owned()));
        }
        if let Some(v) = input.get("world").and_then(|v| v.as_str()) {
            query.push(("world", v.to_owned()));
        }
        if let Some(v) = input.get("version").and_then(|v| v.as_str()) {
            query.push(("version", v.to_owned()));
        }
        if let Some(v) = input.get("path").and_then(|v| v.as_str()) {
            query.push(("path", v.to_owned()));
        }
        if let Some(v) = input.get("limit").and_then(|v| v.as_u64()) {
            query.push(("limit", v.to_string()));
        }
        if query
            .iter()
            .all(|(k, _)| *k != "work_id" && *k != "world")
        {
            return Err(StasisError::PortFailure(
                "provide work_id or world+version".into(),
            ));
        }
        daemon_get_query("/v1/world/files", &query).await
    }
}

pub fn register_detamu_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionDetamuStatusTool)?;
    registry.register_tool(CognitionDetamuFilesTool)?;
    registry.register_tool(CognitionDetamuImpactTool)?;
    registry.register_tool(CognitionDetamuCodeAvecTool)?;
    registry.register_tool(CognitionDetamuFindTool)?;
    Ok(())
}

#[async_trait]
impl StasisTool for CognitionDetamuImpactTool {
    fn name(&self) -> &'static str {
        COGNITION_DETAMU_IMPACT
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Dependents of one code entity (callers/references/imports/types) from a Detamu \
             snapshot. Prefer work_id when a Forge undertaking is bound. Empty graph returns \
             ok:true with zero dependents. Detamu domain only — opt-in.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": { "type": "string" },
                "world": { "type": "string" },
                "version": { "type": "string" },
                "entity_id": { "type": "string", "description": "Detamu entity id (e.g. code:symbol:...)" },
                "max_depth": { "type": "integer" },
                "max_nodes": { "type": "integer" }
            },
            "required": ["entity_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        require_snapshot_selector(&input)?;
        let entity_id = input
            .get("entity_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StasisError::PortFailure("entity_id required".into()))?;
        let mut query = Vec::new();
        push_snapshot_query(&input, &mut query);
        query.push(("entity_id".into(), entity_id.to_owned()));
        if let Some(v) = input.get("max_depth").and_then(|v| v.as_u64()) {
            query.push(("max_depth".into(), v.to_string()));
        }
        if let Some(v) = input.get("max_nodes").and_then(|v| v.as_u64()) {
            query.push(("max_nodes".into(), v.to_string()));
        }
        daemon_get_query_pairs("/v1/world/impact", &query).await
    }
}

#[async_trait]
impl StasisTool for CognitionDetamuCodeAvecTool {
    fn name(&self) -> &'static str {
        COGNITION_DETAMU_CODE_AVEC
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Code AVEC gap/score summary for a Detamu snapshot (which entities lack measurements \
             or scores). Response field is `code_avec` — never bare `avec`. Detamu domain only.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": { "type": "string" },
                "world": { "type": "string" },
                "version": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        require_snapshot_selector(&input)?;
        let mut query = Vec::new();
        push_snapshot_query(&input, &mut query);
        daemon_get_query_pairs("/v1/world/code_avec", &query).await
    }
}

#[async_trait]
impl StasisTool for CognitionDetamuFindTool {
    fn name(&self) -> &'static str {
        COGNITION_DETAMU_FIND
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Find Detamu code entities by kind/path/name (symbols, modules, files). Optional \
             path+line resolves the narrowest entity at that location. Detamu domain only.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": { "type": "string" },
                "world": { "type": "string" },
                "version": { "type": "string" },
                "kind": { "type": "string", "description": "Entity kind (symbol, module, file, …)" },
                "path": { "type": "string" },
                "name_contains": { "type": "string" },
                "line": { "type": "integer", "description": "With path: resolve entity at line" },
                "limit": { "type": "integer" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        require_snapshot_selector(&input)?;
        let mut query = Vec::new();
        push_snapshot_query(&input, &mut query);
        if let Some(line) = input.get("line").and_then(|v| v.as_u64()) {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| StasisError::PortFailure("path required with line".into()))?;
            query.push(("path".into(), path.to_owned()));
            query.push(("line".into(), line.to_string()));
            return daemon_get_query_pairs("/v1/world/at_location", &query).await;
        }
        if let Some(v) = input.get("kind").and_then(|v| v.as_str()) {
            query.push(("kind".into(), v.to_owned()));
        }
        if let Some(v) = input.get("path").and_then(|v| v.as_str()) {
            query.push(("path".into(), v.to_owned()));
        }
        if let Some(v) = input.get("name_contains").and_then(|v| v.as_str()) {
            query.push(("name_contains".into(), v.to_owned()));
        }
        if let Some(v) = input.get("limit").and_then(|v| v.as_u64()) {
            query.push(("limit".into(), v.to_string()));
        }
        daemon_get_query_pairs("/v1/world/find", &query).await
    }
}
