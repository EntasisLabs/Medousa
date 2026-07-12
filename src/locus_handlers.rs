//! Read-only HTTP handlers for Locus STTP nodes (`/v1/locus/*`).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use locus_core_rs::{ContextQueryService, NodeStore, SemanticIndexStore, SttpNode};
use serde_json::Value;
use stasis::ports::outbound::memory::memory_context_reader::MemoryContextReader;
use stasis::ports::outbound::memory::memory_models::{
    MemoryFindRequest, MemorySortDirection, MemorySortField,
};

use crate::daemon_api::{
    LocusNodeDetailResponse, LocusNodeSummary, LocusNodesListResponse, LocusNodesQuery,
    LocusTagsListResponse, LocusTagsQuery,
};
use crate::locus_memory::sttp_node_to_json;
use crate::locus_semantic_tags::{
    memory_filter_from_tag_input, parse_semantic_tags_from_value, resolve_workshop_tag_tenant_id,
};

#[derive(Clone)]
pub struct LocusApiState {
    pub locus_store: Arc<dyn NodeStore>,
    pub semantic_index: Arc<dyn SemanticIndexStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
}

fn map_locus_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

fn summary_from_node(node: &SttpNode) -> LocusNodeSummary {
    let json = sttp_node_to_json(node);
    LocusNodeSummary {
        sync_key: node.sync_key.clone(),
        session_id: node.session_id.clone(),
        tier: json_value_string(json.get("tier"), "unknown"),
        timestamp: node.timestamp,
        context_summary: node.context_summary.clone().unwrap_or_default(),
        semantic_tags: node.semantic_tags.clone(),
        psi: f64::from(node.psi),
        rho: f64::from(node.rho),
        kappa: f64::from(node.kappa),
        user_avec: json.get("user_avec").cloned(),
        model_avec: json.get("model_avec").cloned(),
    }
}

fn summary_from_memory_node(node: &stasis::ports::outbound::memory::memory_models::MemoryNode) -> LocusNodeSummary {
    LocusNodeSummary {
        sync_key: node.sync_key.clone(),
        session_id: node.session_id.clone(),
        tier: node.tier.clone(),
        timestamp: node.timestamp,
        context_summary: node.context_summary.clone().unwrap_or_default(),
        semantic_tags: node.semantic_tags.clone(),
        psi: f64::from(node.psi),
        rho: f64::from(node.rho),
        kappa: f64::from(node.kappa),
        user_avec: None,
        model_avec: None,
    }
}

fn json_value_string(value: Option<&Value>, fallback: &str) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => fallback.to_string(),
    }
}

async fn list_nodes(
    locus_store: Arc<dyn NodeStore>,
    session_id: Option<&str>,
    limit: usize,
) -> Result<Vec<SttpNode>, String> {
    let context_query = ContextQueryService::new(locus_store);
    context_query
        .list_nodes_async(limit, session_id)
        .await
        .map(|listed| listed.nodes)
        .map_err(|err| err.to_string())
}

fn tag_filter_from_query(query: &LocusNodesQuery) -> stasis::ports::outbound::memory::memory_models::MemoryFilter {
    let mut input = serde_json::Map::new();
    if let Some(tags) = query.tags.as_ref()
        && let Some(parsed) = parse_semantic_tags_from_value(Some(&Value::String(tags.clone()))) {
            input.insert(
                "semantic_tags".to_string(),
                Value::Array(parsed.into_iter().map(Value::String).collect()),
            );
        }
    if let Some(prefix) = query.tag_prefix.as_ref().filter(|v| !v.trim().is_empty()) {
        input.insert("tag_prefix".to_string(), Value::String(prefix.trim().to_string()));
    }
    memory_filter_from_tag_input(&Value::Object(input))
}

async fn find_nodes_by_tags(
    state: &LocusApiState,
    session_id: Option<&str>,
    limit: usize,
    query: &LocusNodesQuery,
) -> Result<Vec<stasis::ports::outbound::memory::memory_models::MemoryNode>, String> {
    let filter = tag_filter_from_query(query);
    let has_tags = filter.indexed_tags.is_some() || filter.tag_prefix.is_some();
    if !has_tags {
        return Ok(Vec::new());
    }

    let mut find = MemoryFindRequest::default();
    find.limit = limit;
    find.sort_field = MemorySortField::Timestamp;
    find.sort_direction = MemorySortDirection::Desc;
    find.filter = filter;
    if let Some(session_id) = session_id {
        find.scope.session_ids = Some(vec![session_id.to_string()]);
        let tenant = crate::locus_memory::derive_locus_tenant_id(session_id);
        if tenant != crate::locus_memory::LOCUS_DEFAULT_TENANT {
            find.scope.tenant_id = Some(tenant);
        }
    }

    state
        .memory_reader
        .find(&find)
        .await
        .map(|response| response.nodes)
        .map_err(|err| err.to_string())
}

pub fn locus_router(
    locus_store: Arc<dyn NodeStore>,
    semantic_index: Arc<dyn SemanticIndexStore>,
    memory_reader: Arc<dyn MemoryContextReader>,
) -> Router {
    Router::new()
        .route("/v1/locus/nodes", get(list_locus_nodes))
        .route("/v1/locus/nodes/{sync_key}", get(get_locus_node))
        .route("/v1/locus/tags", get(list_locus_tags))
        .with_state(LocusApiState {
            locus_store,
            semantic_index,
            memory_reader,
        })
}

pub async fn list_locus_nodes(
    State(state): State<LocusApiState>,
    Query(query): Query<LocusNodesQuery>,
) -> Result<Json<LocusNodesListResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let session_id = query
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let filter = tag_filter_from_query(&query);
    let use_tag_index = filter.indexed_tags.is_some() || filter.tag_prefix.is_some();

    let mut nodes: Vec<LocusNodeSummary> = if use_tag_index {
        find_nodes_by_tags(&state, session_id, limit, &query)
            .await
            .map_err(map_locus_error)?
            .into_iter()
            .map(|node| summary_from_memory_node(&node))
            .collect()
    } else {
        list_nodes(state.locus_store.clone(), session_id, limit)
            .await
            .map_err(map_locus_error)?
            .into_iter()
            .map(|node| summary_from_node(&node))
            .collect()
    };

    if let Some(needle) = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_lowercase())
    {
        nodes.retain(|node| {
            let tag_text = node
                .semantic_tags
                .as_deref()
                .unwrap_or_default()
                .join(" ");
            let haystack = [
                node.sync_key.as_str(),
                node.session_id.as_str(),
                node.context_summary.as_str(),
                node.tier.as_str(),
                tag_text.as_str(),
            ]
            .join(" ")
            .to_lowercase();
            haystack.contains(&needle)
        });
    }

    let retrieved = nodes.len();
    Ok(Json(LocusNodesListResponse {
        retrieved,
        nodes,
    }))
}

pub async fn list_locus_tags(
    State(state): State<LocusApiState>,
    Query(query): Query<LocusTagsQuery>,
) -> Result<Json<LocusTagsListResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let session_id = query
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let prefix = query
        .prefix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    let tenant = resolve_workshop_tag_tenant_id(session_id);
    let tags = state
        .semantic_index
        .find_tags_async(&tenant, prefix.as_deref(), limit)
        .await
        .map_err(map_locus_error)?;

    Ok(Json(LocusTagsListResponse {
        tenant_id: tenant,
        prefix,
        tags: tags.clone(),
        count: tags.len(),
    }))
}

pub async fn get_locus_node(
    State(state): State<LocusApiState>,
    Path(sync_key): Path<String>,
) -> Result<Json<LocusNodeDetailResponse>, (StatusCode, String)> {
    let sync_key = sync_key.trim();
    if sync_key.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "sync_key is required".to_string()));
    }

    let nodes = list_nodes(state.locus_store.clone(), None, 200)
        .await
        .map_err(map_locus_error)?;

    let node = nodes
        .into_iter()
        .find(|node| node.sync_key == sync_key)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("locus node not found: {sync_key}")))?;

    Ok(Json(LocusNodeDetailResponse {
        node: summary_from_node(&node),
        raw: node.raw.clone(),
    }))
}
