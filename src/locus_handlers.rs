//! Read-only HTTP handlers for Locus STTP nodes (`/v1/locus/*`).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use locus_core_rs::{ContextQueryService, NodeStore, SttpNode};
use serde_json::Value;

use crate::daemon_api::{LocusNodeDetailResponse, LocusNodeSummary, LocusNodesListResponse, LocusNodesQuery};
use crate::locus_memory::sttp_node_to_json;

#[derive(Clone)]
pub struct LocusApiState {
    pub locus_store: Arc<dyn NodeStore>,
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
        psi: f64::from(node.psi),
        rho: f64::from(node.rho),
        kappa: f64::from(node.kappa),
        user_avec: json.get("user_avec").cloned(),
        model_avec: json.get("model_avec").cloned(),
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

pub fn locus_router(locus_store: Arc<dyn NodeStore>) -> Router {
    Router::new()
        .route("/v1/locus/nodes", get(list_locus_nodes))
        .route("/v1/locus/nodes/{sync_key}", get(get_locus_node))
        .with_state(LocusApiState { locus_store })
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

    let mut nodes = list_nodes(state.locus_store.clone(), session_id, limit)
        .await
        .map_err(map_locus_error)?;

    if let Some(needle) = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_lowercase())
    {
        nodes.retain(|node| {
            let summary = node.context_summary.clone().unwrap_or_default();
            let haystack = [
                node.sync_key.as_str(),
                node.session_id.as_str(),
                summary.as_str(),
                format!("{:?}", node.tier).as_str(),
            ]
            .join(" ")
            .to_lowercase();
            haystack.contains(&needle)
        });
    }

    let summaries: Vec<LocusNodeSummary> = nodes.iter().map(summary_from_node).collect();
    Ok(Json(LocusNodesListResponse {
        retrieved: summaries.len(),
        nodes: summaries,
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
