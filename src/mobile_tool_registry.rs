//! Personal-mobile tool registry recipe.
//!
//! This module selects canonical runtime capabilities for one deployment. It
//! does not own the agent loop or tool FSM.

use std::sync::Arc;

use medousa_types::daemon_api::{LocusTagsQuery, VaultWriteRequest};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_models::MemoryStoreRequest;
use stasis::prelude::RuntimeComposition;

use crate::embedded_daemon::{
    EmbeddedToolRegistryAssembly, EmbeddedToolRegistryBindings, EmbeddedToolRegistryRecipe,
};
use crate::typed_tools::{
    ToolCatalogHandle, ToolDomainId, ToolExposureQualifier, ToolExposureRef, ToolId, ToolModeId,
    ToolPlacementIndex, ToolRegistration, ToolSurfaceId,
};
use crate::web_search_tool::{
    CognitionWebSearchTool, WebSearchBackend, WebSearchMode, WebSearchRequest,
};

pub const PERSONAL_MOBILE_TOOL_NAMES: &[&str] = &[
    "cognition_tools_discover",
    "cognition_web_search",
    "cognition_store_read",
    "cognition_store_write",
    "cognition_memory_query",
    "cognition_memory_mutate",
    "cognition_grapheme_run",
    medousa_runtime::turn_control::COGNITION_TURN,
];

const GENERAL_MODE: ToolModeId = ToolModeId::new("general");
const DOMAIN_SURFACE: ToolSurfaceId = ToolSurfaceId::new("domain");

#[derive(Debug, Default)]
pub struct PersonalMobileToolRegistryRecipe;

impl EmbeddedToolRegistryRecipe for PersonalMobileToolRegistryRecipe {
    fn assemble(
        &self,
        bindings: EmbeddedToolRegistryBindings,
    ) -> StasisResult<EmbeddedToolRegistryAssembly> {
        let catalog = ToolCatalogHandle::default();
        let mut assembly = EmbeddedToolRegistryAssembly::new(personal_mobile_placements());
        let registry = assembly.registrar();
        registry.register_tool(PersonalMobileToolsDiscoverTool {
            catalog: catalog.clone(),
        })?;
        registry.register_typed_tool(CognitionWebSearchTool::new(Arc::new(
            BrowserLiteWebSearchBackend,
        )))?;
        registry.register_tool(PersonalMobileStoreReadTool)?;
        registry.register_tool(PersonalMobileStoreWriteTool)?;
        registry.register_tool(PersonalMobileMemoryQueryTool {
            locus: bindings.locus.clone(),
        })?;
        registry.register_tool(PersonalMobileMemoryMutateTool {
            memory_writer: bindings.memory_writer,
        })?;
        registry.register_tool(PersonalMobileGraphemeRunTool {
            runtime: bindings.runtime,
        })?;
        assembly.initialize_handle_after_finish(catalog);
        Ok(assembly)
    }
}

fn personal_mobile_placements() -> ToolPlacementIndex {
    let mut placements = ToolPlacementIndex::default();
    for (domain, names) in [
        ("web", &["cognition_web_search"][..]),
        (
            "vault",
            &["cognition_store_read", "cognition_store_write"][..],
        ),
        (
            "memory",
            &["cognition_memory_query", "cognition_memory_mutate"][..],
        ),
        (
            "runtime",
            &[
                "cognition_grapheme_run",
                medousa_runtime::turn_control::COGNITION_TURN,
                "cognition_tools_discover",
            ][..],
        ),
    ] {
        for name in names {
            placements.add_exposure(
                ToolId::new(name),
                ToolExposureRef::domain(GENERAL_MODE, DOMAIN_SURFACE, ToolDomainId::new(domain)),
            );
        }
    }
    placements
}

struct PersonalMobileToolsDiscoverTool {
    catalog: ToolCatalogHandle,
}

#[async_trait::async_trait]
impl StasisTool for PersonalMobileToolsDiscoverTool {
    fn name(&self) -> &'static str {
        "cognition_tools_discover"
    }

    fn description(&self) -> Option<&'static str> {
        Some("List available tools with concise usage summaries.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "domain": {
                    "type": "string",
                    "enum": ["web", "vault", "memory", "runtime"]
                }
            },
            "additionalProperties": false
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let domain = optional_string(&input, "domain");
        let available_domains = ["web", "vault", "memory", "runtime"];
        if let Some(domain) = domain.as_deref()
            && !available_domains.contains(&domain)
        {
            return Err(StasisError::PortFailure(format!(
                "unknown tool domain '{domain}'"
            )));
        }
        let catalog = self.catalog.get().ok_or_else(|| {
            StasisError::PortFailure("tool catalog is not initialized".to_string())
        })?;
        let tools = catalog
            .entries()
            .filter_map(|entry| {
                let entry_domain = entry.placement.exposures.iter().find_map(|exposure| {
                    match exposure.qualifier {
                        Some(ToolExposureQualifier::Domain(domain)) => Some(domain.as_str()),
                        _ => None,
                    }
                })?;
                domain
                    .as_deref()
                    .is_none_or(|requested| requested == entry_domain)
                    .then(|| {
                        json!({
                            "domain": entry_domain,
                            "name": entry.id.as_str(),
                            "summary": catalog.presentation_summary(entry.id),
                        })
                    })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "ok": true,
            "domain": domain,
            "available_domains": available_domains,
            "tools": tools
        }))
    }
}

struct BrowserLiteWebSearchBackend;

#[async_trait::async_trait]
impl WebSearchBackend for BrowserLiteWebSearchBackend {
    async fn search(&self, request: WebSearchRequest) -> StasisResult<Value> {
        let query = request
            .query
            .as_deref()
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .ok_or_else(|| StasisError::PortFailure("query is required".to_string()))?;
        if request.mode != WebSearchMode::Search {
            return Err(StasisError::PortFailure(format!(
                "web search mode '{}' needs a research-provider adapter",
                request.mode.as_str()
            )));
        }
        let max_results = request.max_results.unwrap_or(5).clamp(1, 10) as usize;
        let response = medousa_browser_lite::search_ddg_html_cached_async(&query, max_results)
            .await
            .map_err(port_failure)?;
        serde_json::to_value(response).map_err(port_failure)
    }
}

struct PersonalMobileStoreReadTool;

#[async_trait::async_trait]
impl StasisTool for PersonalMobileStoreReadTool {
    fn name(&self) -> &'static str {
        "cognition_store_read"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Read, list, or search notes in this workshop's sandboxed Vault.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": { "type": "string", "enum": ["vault.list", "vault.read", "vault.search"] },
                "path": { "type": "string" },
                "query": { "type": "string" },
                "prefix": { "type": "string" },
                "semantic_tags": { "type": "array", "items": { "type": "string" } },
                "tag_prefix": { "type": "string" },
                "facet": { "type": "string", "enum": ["tags"] },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                "max_chars": { "type": "integer", "minimum": 256, "maximum": 20000 }
            },
            "additionalProperties": false
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        match required_string(&input, "action")?.as_str() {
            "vault.list" => portable_vault_list(input).await,
            "vault.read" => portable_vault_read(input).await,
            "vault.search" => portable_vault_search(input).await,
            action => Err(unsupported_action(self.name(), action)),
        }
    }
}

struct PersonalMobileStoreWriteTool;

#[async_trait::async_trait]
impl StasisTool for PersonalMobileStoreWriteTool {
    fn name(&self) -> &'static str {
        "cognition_store_write"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Create, update, move, or delete notes in this workshop's sandboxed Vault.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": { "type": "string", "enum": ["vault.write", "vault.move", "vault.delete"] },
                "path": { "type": "string" },
                "to_path": { "type": "string" },
                "content": { "type": "string" },
                "semantic_tags": { "type": "array", "items": { "type": "string" } },
                "if_match": { "type": "string" },
                "auto_workshop_tags": { "type": "boolean" }
            },
            "additionalProperties": false
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        match required_string(&input, "action")?.as_str() {
            "vault.write" => portable_vault_write(input).await,
            "vault.move" => portable_vault_move(input).await,
            "vault.delete" => portable_vault_delete(input).await,
            action => Err(unsupported_action(self.name(), action)),
        }
    }
}

struct PersonalMobileMemoryQueryTool {
    locus: crate::locus_service::LocusService,
}

#[async_trait::async_trait]
impl StasisTool for PersonalMobileMemoryQueryTool {
    fn name(&self) -> &'static str {
        "cognition_memory_query"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Read the STTP schema, recent Locus nodes, or indexed memory tags.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": { "type": "string", "enum": ["memory.schema", "memory.list", "memory.tags"] },
                "session_id": { "type": ["string", "null"] },
                "prefix": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
            },
            "additionalProperties": false
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        match required_string(&input, "action")?.as_str() {
            "memory.schema" => Ok(memory_schema()),
            "memory.list" => {
                let session_id = memory_list_session(&input)?;
                let limit = optional_usize(&input, "limit").unwrap_or(50).clamp(1, 200);
                let nodes = self
                    .locus
                    .list_node_values(session_id.as_deref(), limit)
                    .await
                    .map_err(port_failure)?;
                Ok(json!({ "retrieved": nodes.len(), "nodes": nodes }))
            }
            "memory.tags" => {
                let response = self
                    .locus
                    .list_tags(LocusTagsQuery {
                        session_id: optional_string(&input, "session_id").map(|value| {
                            crate::locus_memory::resolve_workshop_locus_session(&value)
                        }),
                        prefix: optional_string(&input, "prefix"),
                        limit: optional_usize(&input, "limit"),
                    })
                    .await
                    .map_err(port_failure)?;
                Ok(json!({
                    "tenant_id": response.tenant_id,
                    "prefix": response.prefix,
                    "tags": response.tags,
                    "count": response.count,
                    "usage": "Pass tags to cognition_memory_query recall/list actions when available."
                }))
            }
            action => Err(unsupported_action(self.name(), action)),
        }
    }
}

struct PersonalMobileMemoryMutateTool {
    memory_writer: Arc<dyn MemoryContextWriter>,
}

#[async_trait::async_trait]
impl StasisTool for PersonalMobileMemoryMutateTool {
    fn name(&self) -> &'static str {
        "cognition_memory_mutate"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Store a complete STTP node in this workshop's persistent Locus memory.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["action", "node"],
            "properties": {
                "action": { "const": "memory.store" },
                "node": { "type": "string" },
                "session_id": { "type": "string" },
                "semantic_tags": { "type": "array", "items": { "type": "string" } }
            },
            "additionalProperties": false
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let action = required_string(&input, "action")?;
        if action != "memory.store" {
            return Err(unsupported_action(self.name(), &action));
        }
        let node = required_string(&input, "node")?;
        let session_id = optional_string(&input, "session_id")
            .map(Ok)
            .unwrap_or_else(active_turn_session)?;
        let session_id = crate::locus_memory::resolve_workshop_locus_session(&session_id);
        let mut tags = crate::locus_semantic_tags::default_workshop_semantic_tags(&session_id);
        tags.extend(string_list(&input, "semantic_tags"));
        let raw_node = crate::locus_semantic_tags::inject_semantic_tags(&node, &tags);
        let response = self
            .memory_writer
            .store_context(&MemoryStoreRequest {
                session_id,
                raw_node,
            })
            .await?;
        let profile = crate::locus_memory::ingest_profile_name(
            crate::locus_memory::resolve_locus_ingest_profile(),
        );
        if response.valid {
            Ok(json!({
                "node_id": response.node_id,
                "psi": response.psi,
                "valid": true,
                "stored": true,
                "validation_error": response.validation_error,
                "profile_policy": profile
            }))
        } else {
            let message = response
                .validation_error
                .unwrap_or_else(|| "store rejected context".to_string());
            Ok(crate::locus_memory::store_failure_payload(
                response.node_id,
                response.psi,
                false,
                message,
                profile,
            ))
        }
    }
}

struct PersonalMobileGraphemeRunTool {
    runtime: Arc<RuntimeComposition>,
}

#[async_trait::async_trait]
impl StasisTool for PersonalMobileGraphemeRunTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_run"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Execute a portable Grapheme script through the daemon's Stasis workflow runtime.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["source"],
            "properties": { "source": { "type": "string" } },
            "additionalProperties": false
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let source = required_string(&input, "source")?;
        if let Some(context) = crate::execution_context::active_turn_execution_context() {
            context.remember_grapheme_source(&source);
        }
        crate::grapheme_runtime::run_grapheme_via_runtime(
            &self.runtime,
            &source,
            "cognition_embedded",
        )
        .await
    }
}

async fn portable_vault_list(input: Value) -> StasisResult<Value> {
    let facet = optional_string(&input, "facet");
    let prefix = optional_string(&input, "prefix");
    let tag_prefix = optional_string(&input, "tag_prefix");
    let limit = optional_usize(&input, "limit").unwrap_or(50).clamp(1, 200);
    if facet.as_deref() == Some("tags") {
        return vault_io(crate::vault::io::VaultIoClass::Scan, move || {
            Ok(crate::vault::VaultService::list_tags(
                tag_prefix.as_deref().or(prefix.as_deref()),
                limit,
            ))
        })
        .await;
    }
    let tags = string_list(&input, "semantic_tags");
    let tags = (!tags.is_empty()).then(|| tags.join(","));
    vault_io(crate::vault::io::VaultIoClass::Scan, move || {
        Ok(crate::vault::VaultService::list_notes(
            prefix.as_deref(),
            limit,
            tags.as_deref(),
            tag_prefix.as_deref(),
        ))
    })
    .await
}

async fn portable_vault_read(input: Value) -> StasisResult<Value> {
    let path = required_string(&input, "path")?;
    let max_chars = optional_usize(&input, "max_chars")
        .unwrap_or(12_000)
        .clamp(256, 20_000);
    let note = crate::vault::io::vault_io()
        .run_anyhow(crate::vault::io::VaultIoClass::Scan, move || {
            crate::vault::VaultService::get_note(&path)
        })
        .await
        .map_err(port_failure)?;
    let total_lines = note.content.lines().count();
    let total_chars = note.content.chars().count();
    let truncated = total_chars > max_chars;
    let content = if truncated {
        format!(
            "{}…",
            note.content.chars().take(max_chars).collect::<String>()
        )
    } else {
        note.content
    };
    Ok(json!({
        "note": note.note,
        "content": content,
        "truncated": truncated,
        "total_lines": total_lines,
        "total_chars": total_chars
    }))
}

async fn portable_vault_search(input: Value) -> StasisResult<Value> {
    let query = optional_string(&input, "query");
    let limit = optional_usize(&input, "limit").unwrap_or(20).clamp(1, 200);
    let tags = string_list(&input, "semantic_tags");
    let tags = (!tags.is_empty()).then(|| tags.join(","));
    vault_io(crate::vault::io::VaultIoClass::SearchRebuild, move || {
        crate::vault::VaultService::search(query.as_deref(), limit, tags.as_deref())
    })
    .await
}

async fn portable_vault_write(input: Value) -> StasisResult<Value> {
    let path = required_string(&input, "path")?;
    let content = required_string(&input, "content")?;
    let if_match = optional_string(&input, "if_match");
    let session_id = crate::locus_memory::resolve_workshop_locus_session(&active_turn_session()?);
    let semantic_tags = string_list(&input, "semantic_tags");
    let request = VaultWriteRequest {
        path: Some(path.clone()),
        content,
        session_id: Some(session_id),
        semantic_tags: (!semantic_tags.is_empty()).then_some(semantic_tags),
        auto_workshop_tags: input
            .get("auto_workshop_tags")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    };
    vault_io(crate::vault::io::VaultIoClass::Mutation, move || {
        crate::vault::VaultService::write_note_with_actor(
            Some(&path),
            &request,
            if_match.as_deref(),
            medousa_types::daemon_api::WorkspaceEventActor::Agent,
            Some("cognition_store_write"),
        )
    })
    .await
}

async fn portable_vault_move(input: Value) -> StasisResult<Value> {
    let path = required_string(&input, "path")?;
    let to_path = required_string(&input, "to_path")?;
    vault_io(crate::vault::io::VaultIoClass::Mutation, move || {
        crate::vault::VaultService::relocate_note(&path, &to_path)
    })
    .await
}

async fn portable_vault_delete(input: Value) -> StasisResult<Value> {
    let path = required_string(&input, "path")?;
    vault_io(crate::vault::io::VaultIoClass::Mutation, move || {
        crate::vault::VaultService::delete_note(&path)
    })
    .await
}

async fn vault_io<T>(
    class: crate::vault::io::VaultIoClass,
    operation: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> StasisResult<Value>
where
    T: serde::Serialize + Send + 'static,
{
    let output = crate::vault::io::vault_io()
        .run_anyhow(class, operation)
        .await
        .map_err(port_failure)?;
    serde_json::to_value(output).map_err(port_failure)
}

fn memory_schema() -> Value {
    let profile = crate::locus_memory::ingest_profile_name(
        crate::locus_memory::resolve_locus_ingest_profile(),
    );
    json!({
        "canonical_example": crate::locus_memory::CANONICAL_STTP_SCHEMA_EXAMPLE,
        "ingest_profile_policy": profile,
        "semantic_index": crate::locus_memory::typed_semantic_index_schema_guidance(),
        "workflow": [
            "call cognition_memory_query action=memory.schema",
            "cognition_memory_mutate action=memory.store with a complete STTP node",
            "cognition_memory_query action=memory.list for recent workshop memory"
        ],
        "model_guidance": crate::locus_memory::typed_schema_first_guidance(
            "Build a complete four-layer STTP node before store.",
            profile,
        )
    })
}

fn memory_list_session(input: &Value) -> StasisResult<Option<String>> {
    match input.get("session_id") {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(
            crate::locus_memory::resolve_workshop_locus_session(value),
        )),
        Some(_) => Err(StasisError::PortFailure(
            "session_id must be a non-empty string or null".to_string(),
        )),
        None => active_turn_session().map(|session| {
            Some(crate::locus_memory::resolve_workshop_locus_session(
                &session,
            ))
        }),
    }
}

fn active_turn_session() -> StasisResult<String> {
    crate::execution_context::active_turn_execution_context()
        .map(|context| context.session_id().to_string())
        .ok_or_else(|| {
            StasisError::PortFailure("active turn execution context required".to_string())
        })
}

fn required_string(input: &Value, field: &str) -> StasisResult<String> {
    optional_string(input, field)
        .ok_or_else(|| StasisError::PortFailure(format!("{field} is required")))
}

fn optional_string(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_usize(input: &Value, field: &str) -> Option<usize> {
    input
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn string_list(input: &Value, field: &str) -> Vec<String> {
    let Some(value) = input.get(field) else {
        return Vec::new();
    };
    crate::locus_semantic_tags::parse_semantic_tags_from_value(Some(value)).unwrap_or_default()
}

fn unsupported_action(tool: &str, action: &str) -> StasisError {
    StasisError::PortFailure(format!(
        "{tool}: action '{action}' is outside this deployment's capability ceiling"
    ))
}

fn port_failure(error: impl std::fmt::Display) -> StasisError {
    StasisError::PortFailure(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use stasis::application::orchestration::tool_registry::ToolRegistry as _;

    #[tokio::test]
    async fn personal_mobile_discovery_reads_the_assembled_catalog() {
        let catalog = ToolCatalogHandle::default();
        let mut registrar = crate::typed_tools::ToolRegistrar::new(personal_mobile_placements());
        registrar
            .register_tool(PersonalMobileToolsDiscoverTool {
                catalog: catalog.clone(),
            })
            .expect("register discovery");
        registrar
            .register_typed_tool(CognitionWebSearchTool::new(Arc::new(
                BrowserLiteWebSearchBackend,
            )))
            .expect("register web search");
        let (registry, assembled) = registrar.finish();
        catalog.initialize(assembled).expect("initialize catalog");
        let output = registry
            .invoke_tool("cognition_tools_discover", json!({}))
            .await
            .expect("inspect personal mobile tools");
        let names = output["tools"]
            .as_array()
            .expect("tool summaries")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"cognition_tools_discover"));
        assert!(names.contains(&"cognition_web_search"));
        assert!(PERSONAL_MOBILE_TOOL_NAMES.contains(&"cognition_tools_discover"));
        assert!(PERSONAL_MOBILE_TOOL_NAMES.contains(&"cognition_web_search"));
    }
}
