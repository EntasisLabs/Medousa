//! Opt-in Detamu world-model cognition tools (domain `detamu`).
//!
//! Distinct from Locus AVEC and from the coding domain. Scores must be named
//! `code_avec` / never bare `avec`. See architecture/detamu-medousa-fit.md.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use stasis::prelude::{Result as StasisResult, StasisError};

use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

pub const COGNITION_DETAMU_STATUS: &str = "cognition_detamu_status";
pub const COGNITION_DETAMU_FILES: &str = "cognition_detamu_files";
pub const COGNITION_DETAMU_IMPACT: &str = "cognition_detamu_impact";
pub const COGNITION_DETAMU_CODE_AVEC: &str = "cognition_detamu_code_avec";
pub const COGNITION_DETAMU_FIND: &str = "cognition_detamu_find";

const COGNITION_DETAMU_STATUS_ID: ToolId = ToolId::new(COGNITION_DETAMU_STATUS);
const COGNITION_DETAMU_FILES_ID: ToolId = ToolId::new(COGNITION_DETAMU_FILES);
const COGNITION_DETAMU_IMPACT_ID: ToolId = ToolId::new(COGNITION_DETAMU_IMPACT);
const COGNITION_DETAMU_CODE_AVEC_ID: ToolId = ToolId::new(COGNITION_DETAMU_CODE_AVEC);
const COGNITION_DETAMU_FIND_ID: ToolId = ToolId::new(COGNITION_DETAMU_FIND);

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
    crate::daemon_self_url::daemon_self_base_url()
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
        return Err(StasisError::PortFailure(format!(
            "daemon {status}: {value}"
        )));
    }
    Ok(value)
}

async fn daemon_get_query(path: &str, query: &[(&str, String)]) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let mut url = reqwest::Url::parse(&format!("{}{path}", daemon_base().trim_end_matches('/')))
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
        return Err(StasisError::PortFailure(format!(
            "daemon {status}: {value}"
        )));
    }
    Ok(value)
}

pub struct CognitionDetamuStatusTool;
pub struct CognitionDetamuFilesTool;
pub struct CognitionDetamuImpactTool;
pub struct CognitionDetamuCodeAvecTool;
pub struct CognitionDetamuFindTool;

fn push_snapshot_query(
    work_id: Option<&str>,
    world: Option<&str>,
    version: Option<&str>,
    query: &mut Vec<(String, String)>,
) {
    if let Some(v) = work_id {
        query.push(("work_id".into(), v.to_owned()));
    }
    if let Some(v) = world {
        query.push(("world".into(), v.to_owned()));
    }
    if let Some(v) = version {
        query.push(("version".into(), v.to_owned()));
    }
}

fn require_snapshot_selector(
    work_id: Option<&str>,
    world: Option<&str>,
    version: Option<&str>,
) -> StasisResult<()> {
    let has_work = work_id.is_some_and(|value| !value.trim().is_empty());
    let has_world = world.map(|s| !s.trim().is_empty()).unwrap_or(false)
        && version.is_some_and(|value| !value.trim().is_empty());
    if !has_work && !has_world {
        return Err(StasisError::PortFailure(
            "provide work_id or world+version".into(),
        ));
    }
    Ok(())
}

async fn daemon_get_query_pairs(path: &str, query: &[(String, String)]) -> StasisResult<Value> {
    let refs: Vec<(&str, String)> = query.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    daemon_get_query(path, &refs).await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DetamuStatusInput {
    /// Optional Forge work id — return Detamu snapshot binding
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
}

#[medousa_tool(id = COGNITION_DETAMU_STATUS_ID)]
impl CognitionDetamuStatusTool {
    /// Probe Detamu world-model host readiness and optional Forge work binding (baseline/sealed snapshot pointers). Detamu domain only — opt-in.
    async fn invoke_typed(
        &self,
        input: DetamuStatusInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let mut status = daemon_get("/v1/world/status").await?;
        if let Some(work_id) = input.work_id.as_deref().filter(|s| !s.trim().is_empty()) {
            match daemon_get(&format!("/v1/world/bindings/{work_id}")).await {
                Ok(binding) => {
                    if let Some(obj) = status.as_object_mut() {
                        obj.insert("binding".into(), binding);
                    }
                }
                Err(err) => {
                    if let Some(obj) = status.as_object_mut() {
                        obj.insert("binding_error".into(), json!(err.to_string()));
                    }
                }
            }
        }
        Ok(ExternalJson::new(status))
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DetamuFilesInput {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    /// Snapshot version (commit OID)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional path filter
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_u64"
    )]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[medousa_tool(id = COGNITION_DETAMU_FILES_ID)]
impl CognitionDetamuFilesTool {
    /// List file entities from a Detamu snapshot (inventory). Prefer work_id when a Forge undertaking is bound. Detamu domain only — opt-in.
    async fn invoke_typed(&self, input: DetamuFilesInput) -> stasis::prelude::Result<ExternalJson> {
        let mut query = Vec::new();
        if let Some(v) = input.work_id.as_deref() {
            query.push(("work_id", v.to_owned()));
        }
        if let Some(v) = input.world.as_deref() {
            query.push(("world", v.to_owned()));
        }
        if let Some(v) = input.version.as_deref() {
            query.push(("version", v.to_owned()));
        }
        if let Some(v) = input.path.as_deref() {
            query.push(("path", v.to_owned()));
        }
        if let Some(v) = input.limit {
            query.push(("limit", v.to_string()));
        }
        if query.iter().all(|(k, _)| *k != "work_id" && *k != "world") {
            return Err(StasisError::PortFailure(
                "provide work_id or world+version".into(),
            ));
        }
        daemon_get_query("/v1/world/files", &query)
            .await
            .map(ExternalJson::new)
    }
}

pub fn register_detamu_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionDetamuStatusTool)?;
    registry.register_typed_tool(CognitionDetamuFilesTool)?;
    registry.register_typed_tool(CognitionDetamuImpactTool)?;
    registry.register_typed_tool(CognitionDetamuCodeAvecTool)?;
    registry.register_typed_tool(CognitionDetamuFindTool)?;
    Ok(())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DetamuImpactInput {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Detamu entity id (e.g. code:symbol:...)
    pub entity_id: String,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_u64"
    )]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u64>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_u64"
    )]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    pub max_nodes: Option<u64>,
}

#[medousa_tool(id = COGNITION_DETAMU_IMPACT_ID)]
impl CognitionDetamuImpactTool {
    /// Dependents of one code entity (callers/references/imports/types) from a Detamu snapshot. Prefer work_id when a Forge undertaking is bound. Empty graph returns ok:true with zero dependents. Detamu domain only — opt-in.
    async fn invoke_typed(
        &self,
        input: DetamuImpactInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        require_snapshot_selector(
            input.work_id.as_deref(),
            input.world.as_deref(),
            input.version.as_deref(),
        )?;
        let mut query = Vec::new();
        push_snapshot_query(
            input.work_id.as_deref(),
            input.world.as_deref(),
            input.version.as_deref(),
            &mut query,
        );
        query.push(("entity_id".into(), input.entity_id));
        if let Some(v) = input.max_depth {
            query.push(("max_depth".into(), v.to_string()));
        }
        if let Some(v) = input.max_nodes {
            query.push(("max_nodes".into(), v.to_string()));
        }
        daemon_get_query_pairs("/v1/world/impact", &query)
            .await
            .map(ExternalJson::new)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DetamuCodeAvecInput {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[medousa_tool(id = COGNITION_DETAMU_CODE_AVEC_ID)]
impl CognitionDetamuCodeAvecTool {
    /// Code AVEC gap/score summary for a Detamu snapshot (which entities lack measurements or scores). Response field is `code_avec` — never bare `avec`. Detamu domain only.
    async fn invoke_typed(
        &self,
        input: DetamuCodeAvecInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        require_snapshot_selector(
            input.work_id.as_deref(),
            input.world.as_deref(),
            input.version.as_deref(),
        )?;
        let mut query = Vec::new();
        push_snapshot_query(
            input.work_id.as_deref(),
            input.world.as_deref(),
            input.version.as_deref(),
            &mut query,
        );
        daemon_get_query_pairs("/v1/world/code_avec", &query)
            .await
            .map(ExternalJson::new)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DetamuFindInput {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Entity kind (symbol, module, file, …)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub name_contains: Option<String>,
    /// With path: resolve entity at line
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_u64"
    )]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_u64"
    )]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[medousa_tool(id = COGNITION_DETAMU_FIND_ID)]
impl CognitionDetamuFindTool {
    /// Find Detamu code entities by kind/path/name (symbols, modules, files). Optional path+line resolves the narrowest entity at that location. Detamu domain only.
    async fn invoke_typed(&self, input: DetamuFindInput) -> stasis::prelude::Result<ExternalJson> {
        require_snapshot_selector(
            input.work_id.as_deref(),
            input.world.as_deref(),
            input.version.as_deref(),
        )?;
        let mut query = Vec::new();
        push_snapshot_query(
            input.work_id.as_deref(),
            input.world.as_deref(),
            input.version.as_deref(),
            &mut query,
        );
        if let Some(line) = input.line {
            let path = input
                .path
                .as_deref()
                .ok_or_else(|| StasisError::PortFailure("path required with line".into()))?;
            query.push(("path".into(), path.to_owned()));
            query.push(("line".into(), line.to_string()));
            return daemon_get_query_pairs("/v1/world/at_location", &query)
                .await
                .map(ExternalJson::new);
        }
        if let Some(v) = input.kind {
            query.push(("kind".into(), v));
        }
        if let Some(v) = input.path {
            query.push(("path".into(), v));
        }
        if let Some(v) = input.name_contains {
            query.push(("name_contains".into(), v));
        }
        if let Some(v) = input.limit {
            query.push(("limit".into(), v.to_string()));
        }
        daemon_get_query_pairs("/v1/world/find", &query)
            .await
            .map(ExternalJson::new)
    }
}
