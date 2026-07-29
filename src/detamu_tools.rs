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

pub const DETAMU_COGNITION_TOOLS: &[&str] = &[COGNITION_DETAMU_STATUS, COGNITION_DETAMU_FILES];

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
    Ok(())
}
