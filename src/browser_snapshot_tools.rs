//! `cognition_browser_snapshot` — markdown snapshot of a URL via Agent Browser.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::StasisError;
use tokio::sync::{mpsc, RwLock};

use crate::browser_host_client::{browser_host_fetch, browser_host_healthy};
use crate::browser_search::surface_from_scope;
use crate::browser_tools::{surface_supports_browser_host, COGNITION_BROWSER_SNAPSHOT};
use crate::events::TuiEvent;
use crate::turn_continuation::TurnContinuationScope;

pub struct CognitionBrowserSnapshotTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionBrowserSnapshotTool {
    pub fn new(
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            turn_scope,
            event_tx,
        }
    }

    async fn browser_enabled(&self) -> bool {
        let scope = self.turn_scope.read().await.clone();
        surface_supports_browser_host(surface_from_scope(scope.as_ref()).as_ref())
    }
}

#[async_trait]
impl StasisTool for CognitionBrowserSnapshotTool {
    fn name(&self) -> &'static str {
        COGNITION_BROWSER_SNAPSHOT
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Capture a markdown snapshot of the current page or a URL via Agent Browser. \
             Requires a browser-capable client (Home desktop/iOS).",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Absolute URL to snapshot (required)"
                },
                "max_chars": {
                    "type": "integer",
                    "default": 4000,
                    "description": "Maximum excerpt length in characters"
                }
            },
            "required": ["url"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        if !self.browser_enabled().await {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_SNAPSHOT}: requires supports_browser_host client (Home desktop/iOS)"
            )));
        }

        let url = input
            .get("url")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                StasisError::PortFailure(format!("{COGNITION_BROWSER_SNAPSHOT}: url is required"))
            })?;
        let max_chars = input
            .get("max_chars")
            .and_then(|value| value.as_u64())
            .unwrap_or(4000) as usize;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: url.clone(),
            })
            .await;

        if browser_host_healthy().await {
            let fetched = browser_host_fetch(&url, max_chars)
                .await
                .map_err(StasisError::PortFailure)?;
            return Ok(json!({
                "url": fetched.url,
                "title": fetched.title,
                "markdown": fetched.markdown,
                "binding_used": "browser_host",
                "decision": "allow",
            }));
        }

        let fetched = tokio::task::spawn_blocking(move || {
            medousa_browser_lite::fetch_url_markdown(&url, max_chars)
        })
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?
        .map_err(StasisError::PortFailure)?;

        Ok(json!({
            "url": fetched.url,
            "title": fetched.title,
            "markdown": fetched.markdown,
            "binding_used": "browser_host_lite",
            "decision": "allow",
        }))
    }
}

pub fn register_browser_snapshot_tool(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionBrowserSnapshotTool::new(turn_scope, event_tx))?;
    Ok(())
}
