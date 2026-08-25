//! Transport-free Locus operations shared by daemon deployment adapters.

use std::sync::Arc;

use locus_core_rs::{ContextQueryService, NodeStore, SemanticIndexStore, SttpNode};
use serde_json::{Value, json};
use stasis::ports::outbound::memory::memory_context_reader::MemoryContextReader;
use stasis::ports::outbound::memory::memory_models::{
    MemoryFindRequest, MemorySortDirection, MemorySortField,
};

use crate::daemon_api::{
    LocusNodeDetailResponse, LocusNodeSummary, LocusNodesListResponse, LocusNodesQuery,
    LocusTagsListResponse, LocusTagsQuery,
};
use crate::locus_semantic_tags::{
    memory_filter_from_tag_input, parse_semantic_tags_from_value, resolve_workshop_tag_tenant_id,
};

#[derive(Debug)]
pub enum LocusServiceError {
    Invalid(String),
    NotFound(String),
    Internal(String),
}

impl std::fmt::Display for LocusServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) | Self::NotFound(message) | Self::Internal(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for LocusServiceError {}

#[derive(Clone)]
pub struct LocusService {
    locus_store: Arc<dyn NodeStore>,
    semantic_index: Arc<dyn SemanticIndexStore>,
    memory_reader: Arc<dyn MemoryContextReader>,
}

impl LocusService {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        semantic_index: Arc<dyn SemanticIndexStore>,
        memory_reader: Arc<dyn MemoryContextReader>,
    ) -> Self {
        Self {
            locus_store,
            semantic_index,
            memory_reader,
        }
    }

    pub async fn list_nodes(
        &self,
        query: LocusNodesQuery,
    ) -> Result<LocusNodesListResponse, LocusServiceError> {
        let limit = query.limit.unwrap_or(50).clamp(1, 200);
        let session_id = query
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let filter = tag_filter_from_query(&query);
        let use_tag_index = filter.indexed_tags.is_some() || filter.tag_prefix.is_some();

        let mut nodes: Vec<LocusNodeSummary> = if use_tag_index {
            self.find_nodes_by_tags(session_id, limit, &query)
                .await?
                .into_iter()
                .map(|node| summary_from_memory_node(&node))
                .collect()
        } else {
            self.list_sttp_nodes(session_id, limit)
                .await?
                .into_iter()
                .map(|node| summary_from_node(&node))
                .collect()
        };

        if let Some(needle) = query
            .q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase)
        {
            nodes.retain(|node| {
                let tag_text = node.semantic_tags.as_deref().unwrap_or_default().join(" ");
                [
                    node.sync_key.as_str(),
                    node.session_id.as_str(),
                    node.context_summary.as_str(),
                    node.tier.as_str(),
                    tag_text.as_str(),
                ]
                .join(" ")
                .to_lowercase()
                .contains(&needle)
            });
        }

        Ok(LocusNodesListResponse {
            retrieved: nodes.len(),
            nodes,
        })
    }

    pub async fn list_tags(
        &self,
        query: LocusTagsQuery,
    ) -> Result<LocusTagsListResponse, LocusServiceError> {
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
            .map(str::to_ascii_lowercase);
        let tenant = resolve_workshop_tag_tenant_id(session_id);
        let tags = self
            .semantic_index
            .find_tags_async(&tenant, prefix.as_deref(), limit)
            .await
            .map_err(|error| LocusServiceError::Internal(error.to_string()))?;

        Ok(LocusTagsListResponse {
            tenant_id: tenant,
            prefix,
            count: tags.len(),
            tags,
        })
    }

    pub async fn get_node(
        &self,
        sync_key: &str,
    ) -> Result<LocusNodeDetailResponse, LocusServiceError> {
        let sync_key = sync_key.trim();
        if sync_key.is_empty() {
            return Err(LocusServiceError::Invalid(
                "sync_key is required".to_string(),
            ));
        }
        let node = self
            .list_sttp_nodes(None, 200)
            .await?
            .into_iter()
            .find(|node| node.sync_key == sync_key)
            .ok_or_else(|| {
                LocusServiceError::NotFound(format!("locus node not found: {sync_key}"))
            })?;
        Ok(LocusNodeDetailResponse {
            node: summary_from_node(&node),
            raw: node.raw.clone(),
        })
    }

    /// Canonical model-facing STTP projection used by daemon memory tools.
    pub async fn list_node_values(
        &self,
        session_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Value>, LocusServiceError> {
        self.list_sttp_nodes(session_id, limit.clamp(1, 200))
            .await
            .map(|nodes| nodes.iter().map(sttp_node_to_json).collect())
    }

    async fn list_sttp_nodes(
        &self,
        session_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SttpNode>, LocusServiceError> {
        ContextQueryService::new(self.locus_store.clone())
            .list_nodes_async(limit, session_id)
            .await
            .map(|listed| listed.nodes)
            .map_err(|error| LocusServiceError::Internal(error.to_string()))
    }

    async fn find_nodes_by_tags(
        &self,
        session_id: Option<&str>,
        limit: usize,
        query: &LocusNodesQuery,
    ) -> Result<Vec<stasis::ports::outbound::memory::memory_models::MemoryNode>, LocusServiceError>
    {
        let filter = tag_filter_from_query(query);
        if filter.indexed_tags.is_none() && filter.tag_prefix.is_none() {
            return Ok(Vec::new());
        }
        let mut find = MemoryFindRequest {
            limit,
            sort_field: MemorySortField::Timestamp,
            sort_direction: MemorySortDirection::Desc,
            filter,
            ..Default::default()
        };
        if let Some(session_id) = session_id {
            find.scope.session_ids = Some(vec![session_id.to_string()]);
            let tenant = crate::locus_semantic_tags::derive_locus_tenant_id(session_id);
            if tenant != crate::locus_semantic_tags::LOCUS_DEFAULT_TENANT {
                find.scope.tenant_id = Some(tenant);
            }
        }
        self.memory_reader
            .find(&find)
            .await
            .map(|response| response.nodes)
            .map_err(|error| LocusServiceError::Internal(error.to_string()))
    }
}

fn tag_filter_from_query(
    query: &LocusNodesQuery,
) -> stasis::ports::outbound::memory::memory_models::MemoryFilter {
    let mut input = serde_json::Map::new();
    if let Some(tags) = query.tags.as_ref()
        && let Some(parsed) = parse_semantic_tags_from_value(Some(&Value::String(tags.clone())))
    {
        input.insert(
            "semantic_tags".to_string(),
            Value::Array(parsed.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(prefix) = query
        .tag_prefix
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        input.insert(
            "tag_prefix".to_string(),
            Value::String(prefix.trim().to_string()),
        );
    }
    memory_filter_from_tag_input(&Value::Object(input))
}

fn summary_from_node(node: &SttpNode) -> LocusNodeSummary {
    let node_json = sttp_node_to_json(node);
    LocusNodeSummary {
        sync_key: node.sync_key.clone(),
        session_id: node.session_id.clone(),
        tier: json_value_string(node_json.get("tier"), "unknown"),
        timestamp: node.timestamp,
        context_summary: node.context_summary.clone().unwrap_or_default(),
        semantic_tags: node.semantic_tags.clone(),
        psi: f64::from(node.psi),
        rho: f64::from(node.rho),
        kappa: f64::from(node.kappa),
        user_avec: node_json.get("user_avec").cloned(),
        model_avec: node_json.get("model_avec").cloned(),
    }
}

fn summary_from_memory_node(
    node: &stasis::ports::outbound::memory::memory_models::MemoryNode,
) -> LocusNodeSummary {
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

pub fn avec_to_json(avec: locus_core_rs::AvecState) -> Value {
    json!({
        "stability": avec.stability,
        "friction": avec.friction,
        "logic": avec.logic,
        "autonomy": avec.autonomy,
        "psi": avec.psi(),
    })
}

pub fn sttp_node_to_json(node: &SttpNode) -> Value {
    json!({
        "raw": node.raw,
        "session_id": node.session_id,
        "tier": node.tier,
        "timestamp": node.timestamp.to_rfc3339(),
        "context_summary": node.context_summary,
        "semantic_tags": node.semantic_tags,
        "psi": node.psi,
        "rho": node.rho,
        "kappa": node.kappa,
        "sync_key": node.sync_key,
        "user_avec": avec_to_json(node.user_avec),
        "model_avec": avec_to_json(node.model_avec),
    })
}
