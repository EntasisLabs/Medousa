//! `cognition_browser_fetch` — gated on `supports_browser_host`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use stasis::domain::errors::StasisError;
use tokio::sync::{mpsc, RwLock};

use crate::browser_host_client::{browser_host_fetch, browser_host_healthy};
use crate::browser_search::surface_from_scope;
use crate::browser_tools::{
    BrowserUrlCommand, COGNITION_BROWSER_FETCH, surface_supports_browser_host,
};
use crate::events::TuiEvent;
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

const COGNITION_BROWSER_FETCH_ID: ToolId = ToolId::new(COGNITION_BROWSER_FETCH);

pub struct CognitionBrowserFetchTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionBrowserFetchTool {
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

fn default_browser_max_chars() -> usize {
    4_000
}

fn deserialize_browser_max_chars<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_else(default_browser_max_chars))
}

#[derive(Debug, JsonSchema)]
pub struct BrowserFetchInput {
    /// Absolute URL to fetch
    #[schemars(required, with = "String")]
    url: CompatOption<String>,
    /// Maximum excerpt length in characters
    #[schemars(with = "i64", default = "default_browser_max_chars")]
    max_chars: usize,
}

impl<'de> Deserialize<'de> for BrowserFetchInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            url: CompatOption<String>,
            #[serde(
                default = "default_browser_max_chars",
                deserialize_with = "deserialize_browser_max_chars"
            )]
            max_chars: usize,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            url: input.url,
            max_chars: input.max_chars,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserFetchOutput {
    url: String,
    title: String,
    markdown: String,
    binding_used: String,
    decision: String,
}

#[medousa_tool(id = COGNITION_BROWSER_FETCH_ID)]
impl CognitionBrowserFetchTool {
    /// Fetch a public URL via Agent Browser and return a markdown excerpt for synthesis. Requires a browser-capable client (Home desktop/iOS).
    async fn invoke_typed(
        &self,
        input: BrowserFetchInput,
    ) -> stasis::prelude::Result<BrowserFetchOutput> {
        if !self.browser_enabled().await {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_FETCH}: requires supports_browser_host client (Home desktop/iOS)"
            )));
        }

        let command = BrowserUrlCommand::new(
            input.url.into_option(),
            input.max_chars,
            COGNITION_BROWSER_FETCH,
        )?;
        let url = command.url.into_string();
        let max_chars = command.max_chars;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_BROWSER_FETCH.to_string(),
                input_summary: url.to_string(),
            })
            .await;

        if browser_host_healthy().await {
            let fetched = browser_host_fetch(&url, max_chars).await.map_err(StasisError::PortFailure)?;
            return Ok(BrowserFetchOutput {
                url: fetched.url,
                title: fetched.title,
                markdown: fetched.markdown,
                binding_used: "browser_host".to_string(),
                decision: "allow".to_string(),
            });
        }

        let fetched = tokio::task::spawn_blocking(move || {
            medousa_browser_lite::fetch_url_markdown(&url, max_chars)
        })
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?
        .map_err(StasisError::PortFailure)?;

        Ok(BrowserFetchOutput {
            url: fetched.url,
            title: fetched.title,
            markdown: fetched.markdown,
            binding_used: "browser_host_lite".to_string(),
            decision: "allow".to_string(),
        })
    }
}

pub fn register_browser_fetch_tool(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionBrowserFetchTool::new(turn_scope, event_tx))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_wire_url_remains_lenient_for_legacy_values() {
        let input: BrowserFetchInput = serde_json::from_value(serde_json::json!({
            "url": false,
            "max_chars": [],
        }))
        .expect("fetch input");
        assert!(input.url.into_option().is_none());
        assert_eq!(input.max_chars, 4_000);
    }
}
