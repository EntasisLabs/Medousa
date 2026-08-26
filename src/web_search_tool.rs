//! Canonical `cognition_web_search` contract over an injected search backend.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::domain::errors::Result as StasisResult;

use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

const COGNITION_WEB_SEARCH_ID: ToolId = ToolId::new("cognition_web_search");

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchMode {
    #[default]
    Search,
    Facade,
    ResearchMaterials,
    ResearchReport,
}

impl WebSearchMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Facade => "facade",
            Self::ResearchMaterials => "research_materials",
            Self::ResearchReport => "research_report",
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WebSearchRequest {
    /// Search query or research topic
    #[schemars(required, with = "String")]
    pub query: Option<String>,
    /// Search directly or use a configured research pipeline
    #[serde(default)]
    #[schemars(default)]
    pub mode: WebSearchMode,
    /// Optional configured provider id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Try lower-priority bindings when the preferred provider fails
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    pub try_fallbacks: Option<bool>,
    /// Maximum direct-search results
    #[serde(default)]
    pub max_results: Option<u64>,
}

#[async_trait::async_trait]
pub trait WebSearchBackend: Send + Sync {
    async fn search(&self, request: WebSearchRequest) -> StasisResult<Value>;
}

pub struct CognitionWebSearchTool {
    backend: Arc<dyn WebSearchBackend>,
}

impl CognitionWebSearchTool {
    pub fn new(backend: Arc<dyn WebSearchBackend>) -> Self {
        Self { backend }
    }
}

#[medousa_tool(id = COGNITION_WEB_SEARCH_ID)]
impl CognitionWebSearchTool {
    /// Search the public web with one call. The registered backend selects provider bindings and fallbacks.
    async fn invoke_typed(
        &self,
        request: WebSearchRequest,
    ) -> stasis::prelude::Result<ExternalJson> {
        self.backend.search(request).await.map(ExternalJson::new)
    }
}
