//! Rich HTML presentation tool (`cognition_ui_present`) for surfaces that opt in via
//! `TurnSurfaceContext.supports_ui_artifacts`.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::daemon_api::TurnSurfaceContext;
use crate::runtime_session::{require_active_chat_session_id_async, runtime_bootstrap_session_id};
use crate::turn_continuation::TurnContinuationScope;

pub const COGNITION_UI_PRESENT: &str = "cognition_ui_present";

pub const UI_PRESENT_COGNITION_TOOLS: &[&str] = &[COGNITION_UI_PRESENT];

pub fn is_ui_present_cognition_tool(name: &str) -> bool {
    name == COGNITION_UI_PRESENT
}

pub fn surface_supports_ui_artifacts(surface: Option<&TurnSurfaceContext>) -> bool {
    surface.is_some_and(|ctx| ctx.supports_ui_artifacts)
}

pub fn register_ui_present_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionUiPresentTool::new(turn_scope))?;
    Ok(())
}

pub struct CognitionUiPresentTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionUiPresentTool {
    pub fn new(turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>) -> Self {
        Self { turn_scope }
    }

    async fn resolve_session_id(&self) -> StasisResult<String> {
        require_active_chat_session_id_async(
            &self.turn_scope,
            runtime_bootstrap_session_id(),
            COGNITION_UI_PRESENT,
        )
        .await
    }

    async fn active_surface_supports_ui_artifacts(&self) -> bool {
        self.turn_scope
            .read()
            .await
            .as_ref()
            .is_some_and(|scope| scope.supports_ui_artifacts)
    }
}

#[async_trait]
impl StasisTool for CognitionUiPresentTool {
    fn name(&self) -> &'static str {
        COGNITION_UI_PRESENT
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Present an HTML artifact in chat (inline card, side panel, or fullscreen) when the connected client advertises supports_ui_artifacts. \
             Persist workflow: publish HTML here, then persist=true + surface_id (custom surface only) + component_id + slot to pin on the canvas. \
             For first-time publish only — use cognition_artifact_write to revise an existing artifact. \
             inline: compact preview card. panel/fullscreen: use a transparent outer page background (no hard-coded #000 body); center content up to ~900px wide. \
             Use height only for inline preview cap.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["title", "html", "presentation"],
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Short label shown in the artifact header/chip"
                },
                "html": {
                    "type": "string",
                    "description": "HTML fragment or document. For panel/fullscreen prefer transparent outer background; avoid full-page black fills. Card layouts up to ~900px centered are ideal."
                },
                "presentation": {
                    "type": "string",
                    "enum": ["inline", "panel", "fullscreen"],
                    "description": "How Home should render the artifact"
                },
                "height": {
                    "type": "integer",
                    "description": "Optional inline max height hint in pixels (default ~360)"
                },
                "persist": {
                    "type": "boolean",
                    "description": "When true, also upsert a presentation component on the environment canvas"
                },
                "component_id": {
                    "type": "string",
                    "description": "Canvas component id when persist=true"
                },
                "surface_id": {
                    "type": "string",
                    "description": "Target custom surface id for persisted component (required when persist=true; never builtin home/chat)"
                },
                "slot": {
                    "type": "string",
                    "description": "Slot zone for persisted component (default main)"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        if !self.active_surface_supports_ui_artifacts().await {
            return Ok(json!({
                "ok": false,
                "unsupported_surface": true,
                "error": "This channel does not support HTML UI artifacts (supports_ui_artifacts=false). Answer in markdown instead.",
            }));
        }

        let title = input
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("title is required".to_string()))?;
        let html = input
            .get("html")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("html is required".to_string()))?;
        let presentation = input
            .get("presentation")
            .and_then(|value| value.as_str())
            .unwrap_or("inline");
        let height_px = input.get("height").and_then(|value| value.as_u64()).map(|value| {
            value.clamp(120, 1200) as u32
        });

        let session_id = self.resolve_session_id().await?;
        let title = title.to_string();
        let html = html.to_string();
        let presentation = presentation.to_string();
        let label_for_component = title.clone();

        let record = tokio::task::spawn_blocking(move || {
            crate::artifact_store::persist_ui_artifact(
                &session_id,
                &html,
                &title,
                &presentation,
                height_px,
            )
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("ui present join error: {err}")))?
        .map_err(StasisError::PortFailure)?;

        let mut response = json!({
            "ok": true,
            "artifact_id": record.artifact_id,
            "label": record.label,
            "mime": record.content_type,
            "presentation": record.presentation,
            "height_px": record.height_px,
            "byte_size": record.byte_size,
        });

        if input.get("persist").and_then(Value::as_bool) == Some(true) {
            let component_id = input
                .get("component_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    StasisError::PortFailure(
                        "component_id is required when persist=true".to_string(),
                    )
                })?;
            let surface_id = input
                .get("surface_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    StasisError::PortFailure("surface_id is required when persist=true".to_string())
                })?;
            let slot = input
                .get("slot")
                .and_then(Value::as_str)
                .unwrap_or("main");
            let presentation_component = crate::environment_tools::make_presentation_component(
                component_id,
                surface_id,
                &record.artifact_id,
                record.label.as_deref().unwrap_or(&label_for_component),
            );
            let mut component = presentation_component;
            component.slot = slot.to_string();
            component.presentation = match record.presentation.as_deref().unwrap_or("inline") {
                "panel" => Some(medousa_types::environment::UiPresentation::Panel),
                "fullscreen" => Some(medousa_types::environment::UiPresentation::Fullscreen),
                _ => Some(medousa_types::environment::UiPresentation::Inline),
            };
            let profile_id = crate::environment_store::resolve_profile_id(None);
            let mut env_record = crate::environment_store::environment_hub()
                .get(&profile_id)
                .await
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            if let Some(index) = env_record
                .spec
                .components
                .iter()
                .position(|entry| entry.id == component_id)
            {
                env_record.spec.components[index] = component;
            } else {
                env_record.spec.components.push(component);
            }
            let updated = crate::environment_store::environment_hub()
                .put(env_record.spec, "agent")
                .await
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            response["persisted_component_id"] = json!(component_id);
            response["environment_revision"] = json!(updated.revision);
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_api::TurnSurfaceContext;
    use crate::runtime_session::{
        RUNTIME_BOOTSTRAP_SESSION_ID, is_runtime_bootstrap_session_id,
    };

    #[test]
    fn surface_supports_ui_artifacts_requires_client_flag() {
        assert!(!surface_supports_ui_artifacts(None));
        assert!(!surface_supports_ui_artifacts(Some(&TurnSurfaceContext::tui())));
        assert!(surface_supports_ui_artifacts(Some(
            &TurnSurfaceContext::tui().with_ui_artifacts(true)
        )));
    }

    #[tokio::test]
    async fn ui_present_rejects_bootstrap_only_session_resolution() {
        let turn_scope = Arc::new(RwLock::new(None::<TurnContinuationScope>));
        let tool = CognitionUiPresentTool::new(turn_scope);
        let err = tool.resolve_session_id().await.expect_err("bootstrap-only");
        assert!(err.to_string().contains("not a chat session"));
        assert!(is_runtime_bootstrap_session_id(RUNTIME_BOOTSTRAP_SESSION_ID));
    }

    #[tokio::test]
    async fn ui_present_uses_active_turn_session_not_bootstrap() {
        let turn_scope = Arc::new(RwLock::new(Some(TurnContinuationScope {
            turn_correlation_id: "turn-1".to_string(),
            session_id: "medousa-home".to_string(),
            original_prompt: "hi".to_string(),
            delivery_target: None,
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            response_depth_mode: "standard".to_string(),
            supports_ui_artifacts: true,
            supports_browser_host: false,
            channel_surface: Some("home-desktop".to_string()),
        })));
        let tool = CognitionUiPresentTool::new(turn_scope);
        let session_id = tool.resolve_session_id().await.expect("turn scope session");
        assert_eq!(session_id, "medousa-home");
    }
}
