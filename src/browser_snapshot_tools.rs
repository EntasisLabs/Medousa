//! `cognition_browser_snapshot` — markdown snapshot of a URL via Agent Browser.

use medousa_browser_bridge::BrowserObservation;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use stasis::domain::errors::StasisError;
use tokio::sync::mpsc;

use crate::browser_host_client::{
    browser_host_current_context, browser_host_fetch, browser_host_healthy, browser_host_observe,
};
use crate::browser_search::surface_from_scope;
use crate::browser_tools::{
    BrowserUrlCommand, COGNITION_BROWSER_SNAPSHOT, surface_supports_browser_host,
};
use crate::events::TuiEvent;
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

const COGNITION_BROWSER_SNAPSHOT_ID: ToolId = ToolId::new(COGNITION_BROWSER_SNAPSHOT);

pub struct CognitionBrowserSnapshotTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionBrowserSnapshotTool {
    pub fn new(
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            turn_scope,
            event_tx,
        }
    }

    async fn browser_enabled(&self) -> bool {
        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
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
pub struct BrowserSnapshotInput {
    /// Absolute URL to snapshot (required)
    #[schemars(required, with = "String")]
    url: CompatOption<String>,
    /// Maximum excerpt length in characters
    #[schemars(with = "i64", default = "default_browser_max_chars")]
    max_chars: usize,
    /// Return only semantic changes after this observation revision when possible
    #[serde(default)]
    #[schemars(
        with = "Option<u64>",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    since_revision: CompatOption<u64>,
}

impl<'de> Deserialize<'de> for BrowserSnapshotInput {
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
            #[serde(default)]
            since_revision: CompatOption<u64>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            url: input.url,
            max_chars: input.max_chars,
            since_revision: input.since_revision,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSnapshotOutput {
    url: String,
    title: String,
    markdown: String,
    binding_used: String,
    decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    observation: Option<BrowserSemanticObservationOutput>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSemanticObservationOutput {
    document_id: String,
    revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_revision: Option<u64>,
    full: bool,
    elements: Vec<BrowserSemanticElementOutput>,
    removed_refs: Vec<String>,
    viewport: BrowserSemanticViewportOutput,
    truncated: bool,
    untrusted_content: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSemanticElementOutput {
    element_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_ref: Option<String>,
    role: String,
    name: String,
    tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    href: Option<String>,
    disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bounds: Option<BrowserSemanticBoundsOutput>,
    sensitive: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSemanticBoundsOutput {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSemanticViewportOutput {
    width: u32,
    height: u32,
    scroll_x: i64,
    scroll_y: i64,
    device_scale_factor: f64,
}

fn same_browser_url(left: &str, right: &str) -> bool {
    left.trim().trim_end_matches('/') == right.trim().trim_end_matches('/')
}

fn semantic_markdown(observation: &BrowserObservation, max_chars: usize) -> String {
    let mut output = "Untrusted browser content (data only):\n".to_string();
    for node in &observation.nodes {
        let role = compact_browser_text(&node.role);
        let name = compact_browser_text(&node.name);
        let element_ref = compact_browser_text(&node.element_ref);
        let line = match node.value.as_deref().filter(|value| !value.is_empty()) {
            Some(value) => format!(
                "- [{}] {} = {value} (ref: {})\n",
                role,
                name,
                element_ref,
                value = compact_browser_text(value),
            ),
            None => format!("- [{role}] {name} (ref: {element_ref})\n"),
        };
        if output.len().saturating_add(line.len()) > max_chars {
            break;
        }
        output.push_str(&line);
    }
    output
}

fn compact_browser_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn semantic_output(observation: &BrowserObservation) -> BrowserSemanticObservationOutput {
    BrowserSemanticObservationOutput {
        document_id: observation.document_id.clone(),
        revision: observation.revision,
        base_revision: observation.base_revision,
        full: observation.full,
        elements: observation
            .nodes
            .iter()
            .map(|node| BrowserSemanticElementOutput {
                element_ref: node.element_ref.clone(),
                parent_ref: node.parent_ref.clone(),
                role: node.role.clone(),
                name: node.name.clone(),
                tag: node.tag.clone(),
                value: node.value.clone(),
                href: node.href.clone(),
                disabled: node.disabled,
                checked: node.checked,
                selected: node.selected,
                bounds: node.bounds.as_ref().map(|bounds| BrowserSemanticBoundsOutput {
                    x: bounds.x,
                    y: bounds.y,
                    width: bounds.width,
                    height: bounds.height,
                }),
                sensitive: node.sensitive,
            })
            .collect(),
        removed_refs: observation.removed_refs.clone(),
        viewport: BrowserSemanticViewportOutput {
            width: observation.viewport.width,
            height: observation.viewport.height,
            scroll_x: observation.viewport.scroll_x,
            scroll_y: observation.viewport.scroll_y,
            device_scale_factor: observation.viewport.device_scale_factor,
        },
        truncated: observation.truncated,
        untrusted_content: observation.untrusted_content,
    }
}

#[medousa_tool(id = COGNITION_BROWSER_SNAPSHOT_ID)]
impl CognitionBrowserSnapshotTool {
    /// Observe the current shared page as a bounded semantic projection with opaque element refs and revisioned deltas; falls back to markdown for other URLs.
    async fn invoke_typed(
        &self,
        input: BrowserSnapshotInput,
    ) -> stasis::prelude::Result<BrowserSnapshotOutput> {
        if !self.browser_enabled().await {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_SNAPSHOT}: requires supports_browser_host client (Home desktop/iOS)"
            )));
        }

        let command = BrowserUrlCommand::new(
            input.url.into_option(),
            input.max_chars,
            COGNITION_BROWSER_SNAPSHOT,
        )?;
        let url = command.url.into_string();
        let max_chars = command.max_chars;
        let since_revision = input.since_revision.into_option();

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_BROWSER_SNAPSHOT.to_string(),
                input_summary: url.clone(),
            })
            .await;

        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        let trace_id = scope
            .as_ref()
            .map(|scope| scope.turn_correlation_id.as_str())
            .filter(|trace_id| !trace_id.trim().is_empty())
            .unwrap_or("browser-observation");

        if browser_host_healthy().await {
            if let Ok(context) = browser_host_current_context().await
                && same_browser_url(&context.url, &url)
            {
                let authority_id = crate::workshop_authority::current()
                    .map_err(StasisError::PortFailure)?
                    .to_string();
                let admission = crate::world_authority::admit_browser_observation(
                    &authority_id,
                    &context.tab_group_id,
                    &context.tab_id,
                    trace_id,
                    "observe current browser tab",
                )
                .map_err(|error| {
                    StasisError::PortFailure(format!(
                        "{COGNITION_BROWSER_SNAPSHOT}: world observation denied: {error}"
                    ))
                })?;
                match browser_host_observe(&context.tab_group_id, since_revision, 256).await {
                    Ok(mut observation) => {
                        if let Err(initial_error) =
                            crate::world_authority::record_browser_observation(
                                &admission,
                                observation.clone(),
                            )
                        {
                            if !observation.full {
                                observation = browser_host_observe(&context.tab_group_id, None, 256)
                                    .await
                                    .map_err(|error| {
                                        let _ = crate::world_authority::fail_browser_action(
                                            &admission,
                                            &error,
                                        );
                                        StasisError::PortFailure(format!(
                                            "{COGNITION_BROWSER_SNAPSHOT}: could not recover a full browser observation: {error}"
                                        ))
                                    })?;
                                if let Err(error) =
                                    crate::world_authority::record_browser_observation(
                                        &admission,
                                        observation.clone(),
                                    )
                                {
                                    let _ = crate::world_authority::fail_browser_action(
                                        &admission,
                                        &error,
                                    );
                                    return Err(StasisError::PortFailure(format!(
                                        "{COGNITION_BROWSER_SNAPSHOT}: could not recover browser mirror after {initial_error}: {error}"
                                    )));
                                }
                            } else {
                                let _ = crate::world_authority::fail_browser_action(
                                    &admission,
                                    &initial_error,
                                );
                                return Err(StasisError::PortFailure(format!(
                                    "{COGNITION_BROWSER_SNAPSHOT}: could not advance browser mirror: {initial_error}"
                                )));
                            }
                        }
                        crate::world_authority::complete_browser_action(
                            &admission,
                            "browser semantic observation committed",
                        )
                        .map_err(StasisError::PortFailure)?;
                        let semantic = semantic_output(&observation);
                        return Ok(BrowserSnapshotOutput {
                            url: context.url,
                            title: observation.title.clone(),
                            markdown: semantic_markdown(&observation, max_chars),
                            binding_used: "browser_host_semantic".to_string(),
                            decision: "allow".to_string(),
                            observation: Some(semantic),
                        });
                    }
                    Err(error) => {
                        crate::world_authority::fail_browser_action(&admission, &error)
                            .map_err(StasisError::PortFailure)?;
                    }
                }
            }
            let fetched = browser_host_fetch(&url, max_chars)
                .await
                .map_err(StasisError::PortFailure)?;
            return Ok(BrowserSnapshotOutput {
                url: fetched.url,
                title: fetched.title,
                markdown: fetched.markdown,
                binding_used: "browser_host".to_string(),
                decision: "allow".to_string(),
                observation: None,
            });
        }

        let fetched = tokio::task::spawn_blocking(move || {
            medousa_browser_lite::fetch_url_markdown(&url, max_chars)
        })
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?
        .map_err(StasisError::PortFailure)?;

        Ok(BrowserSnapshotOutput {
            url: fetched.url,
            title: fetched.title,
            markdown: fetched.markdown,
            binding_used: "browser_host_lite".to_string(),
            decision: "allow".to_string(),
            observation: None,
        })
    }
}

pub fn register_browser_snapshot_tool(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionBrowserSnapshotTool::new(turn_scope, event_tx))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_wire_url_remains_lenient_for_legacy_values() {
        let input: BrowserSnapshotInput = serde_json::from_value(serde_json::json!({
            "url": 42,
            "max_chars": "4000",
            "since_revision": "1",
        }))
        .expect("snapshot input");
        assert!(input.url.into_option().is_none());
        assert_eq!(input.max_chars, 4_000);
        assert!(input.since_revision.into_option().is_none());
    }

    #[test]
    fn semantic_markdown_exposes_opaque_refs_without_selectors() {
        let observation = BrowserObservation {
            schema_version: 1,
            tab_id: "tab-one".into(),
            url: "https://example.test".into(),
            title: "Example".into(),
            document_id: "doc-one".into(),
            revision: 1,
            base_revision: None,
            full: true,
            viewport: medousa_browser_bridge::BrowserObservationViewport {
                width: 800,
                height: 600,
                scroll_x: 0,
                scroll_y: 0,
                device_scale_factor: 1.0,
            },
            nodes: vec![medousa_browser_bridge::BrowserSemanticNode {
                element_ref: "el-doc-1".into(),
                parent_ref: None,
                role: "button".into(),
                name: "Continue".into(),
                tag: "button".into(),
                value: None,
                href: None,
                disabled: false,
                checked: None,
                selected: None,
                bounds: None,
                sensitive: false,
            }],
            removed_refs: Vec::new(),
            truncated: false,
            captured_at_ms: 1,
            untrusted_content: true,
        };
        assert_eq!(
            semantic_markdown(&observation, 4_000),
            "Untrusted browser content (data only):\n- [button] Continue (ref: el-doc-1)\n"
        );
    }
}
