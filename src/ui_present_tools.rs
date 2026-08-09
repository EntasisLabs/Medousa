//! Rich HTML presentation tool (`cognition_ui_present`) for surfaces that opt in via
//! `TurnSurfaceContext.supports_ui_artifacts`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::daemon_api::TurnSurfaceContext;
use crate::runtime_session::{require_active_chat_session_id_async, runtime_bootstrap_session_id};
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_UI_PRESENT: &str = "cognition_ui_present";
const COGNITION_UI_PRESENT_ID: ToolId = ToolId::new(COGNITION_UI_PRESENT);

pub const UI_PRESENT_COGNITION_TOOLS: &[&str] = &[COGNITION_UI_PRESENT];

pub fn is_ui_present_cognition_tool(name: &str) -> bool {
    name == COGNITION_UI_PRESENT
}

pub fn surface_supports_ui_artifacts(surface: Option<&TurnSurfaceContext>) -> bool {
    surface.is_some_and(|ctx| ctx.supports_ui_artifacts)
}

pub fn register_ui_present_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionUiPresentTool::new(turn_scope))?;
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

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum UiPresentationInputSchema {
    Inline,
    Panel,
    Fullscreen,
}

#[derive(Debug, JsonSchema)]
pub struct UiPresentInput {
    /// Short label shown in the artifact header/chip
    #[schemars(required, with = "String")]
    pub(crate) title: Option<String>,
    /// HTML fragment or document. For panel/fullscreen prefer transparent outer background; avoid full-page black fills. Card layouts up to ~900px centered are ideal. Persisted canvas widgets: use MedousaStore (not localStorage); await MedousaStore.get/set/delete — see cognition_environment_wiki(topic=artifact_runtime).
    #[schemars(required, with = "String")]
    pub(crate) html: Option<String>,
    /// How Home should render the artifact
    #[schemars(required, with = "UiPresentationInputSchema")]
    pub(crate) presentation: Option<String>,
    /// Optional inline max height hint in pixels (default ~360)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    pub(crate) height: Option<u64>,
    /// When true, also upsert a presentation component on the environment canvas
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    pub(crate) persist: Option<bool>,
    /// Canvas component id when persist=true
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) component_id: Option<String>,
    /// Target custom surface id for persisted component (required when persist=true; never builtin home/chat)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) surface_id: Option<String>,
    /// Slot zone for persisted component (default main)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) slot: Option<String>,
}

impl UiPresentInput {
    pub(crate) fn inline(
        title: impl Into<String>,
        html: impl Into<String>,
    ) -> StasisResult<Self> {
        let title = TrimmedText::new(title)
            .map_err(|_| StasisError::PortFailure("title is required".to_string()))?;
        let html = RequiredContent::new(html)
            .map_err(|_| StasisError::PortFailure("html is required".to_string()))?;

        Ok(Self {
            title: Some(title.into_string()),
            html: Some(html.into_string()),
            presentation: Some("inline".to_string()),
            height: None,
            persist: None,
            component_id: None,
            surface_id: None,
            slot: None,
        })
    }

    pub(crate) fn persistent_component(
        title: impl Into<String>,
        html: impl Into<String>,
        presentation: Option<String>,
        component_id: impl Into<String>,
        surface_id: impl Into<String>,
        slot: impl Into<String>,
    ) -> StasisResult<Self> {
        let mut input = Self::inline(title, html)?;
        let component_id = TrimmedText::new(component_id)
            .map_err(|_| {
                StasisError::PortFailure(
                    "component_id is required when persist=true".to_string(),
                )
            })?;
        let surface_id = TrimmedText::new(surface_id)
            .map_err(|_| {
                StasisError::PortFailure("surface_id is required when persist=true".to_string())
            })?;

        input.presentation = presentation;
        input.persist = Some(true);
        input.component_id = Some(component_id.into_string());
        input.surface_id = Some(surface_id.into_string());
        input.slot = Some(slot.into());
        Ok(input)
    }
}

impl<'de> Deserialize<'de> for UiPresentInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            title: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            html: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            presentation: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_u64"
            )]
            height: Option<u64>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
            )]
            persist: Option<bool>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            component_id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            surface_id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            slot: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            title: input.title,
            html: input.html,
            presentation: input.presentation,
            height: input.height,
            persist: input.persist,
            component_id: input.component_id,
            surface_id: input.surface_id,
            slot: input.slot,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum UiPresentOutput {
    Unsupported {
        ok: bool,
        unsupported_surface: bool,
        error: String,
    },
    Presented {
        ok: bool,
        artifact_id: String,
        label: Option<String>,
        mime: String,
        presentation: Option<String>,
        height_px: Option<u32>,
        byte_size: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        persisted: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        errors: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        persisted_component_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        environment_revision: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        live: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        nav_visible: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
    },
}

#[medousa_tool(id = COGNITION_UI_PRESENT_ID)]
impl CognitionUiPresentTool {
    /// Present an HTML artifact in chat (inline card, side panel, or fullscreen) when the connected client advertises supports_ui_artifacts. Persist workflow: publish HTML here, then persist=true + surface_id (custom surface only) + component_id + slot to pin on the canvas. For first-time publish only — use cognition_artifact_write to revise an existing artifact. Canvas widgets: use MedousaStore (not localStorage); get/set/delete are async — await them in async handlers. inline: compact preview card. panel/fullscreen: use a transparent outer page background (no hard-coded #000 body); center content up to ~900px wide. Use height only for inline preview cap.
    pub(crate) async fn invoke_typed(
        &self,
        input: UiPresentInput,
    ) -> stasis::prelude::Result<UiPresentOutput> {
        if !self.active_surface_supports_ui_artifacts().await {
            return Ok(UiPresentOutput::Unsupported {
                ok: false,
                unsupported_surface: true,
                error: "This channel does not support HTML UI artifacts (supports_ui_artifacts=false). Answer in markdown instead."
                    .to_string(),
            });
        }

        let title = TrimmedText::new(input.title.as_deref().unwrap_or_default())
            .map_err(|_| StasisError::PortFailure("title is required".to_string()))?;
        let html = input
            .html
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(RequiredContent::new)
            .transpose()
            .map_err(|_| StasisError::PortFailure("html is required".to_string()))?
            .ok_or_else(|| StasisError::PortFailure("html is required".to_string()))?;
        let presentation = input.presentation.as_deref().unwrap_or("inline");
        let height_px = input.height.map(|value| value.clamp(120, 1200) as u32);

        let session_id = self.resolve_session_id().await?;
        let session_id_for_alias = session_id.clone();
        let title = title.into_string();
        let html = html.into_string();
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

        let mut ok = true;
        let mut persisted = None;
        let mut errors = None;
        let mut persisted_component_id = None;
        let mut environment_revision = None;
        let mut live = None;
        let mut nav_visible = None;
        let mut hint = None;

        if input.persist == Some(true) {
            let component_id = input
                .component_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    StasisError::PortFailure(
                        "component_id is required when persist=true".to_string(),
                    )
                })?;
            let surface_id = input
                .surface_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    StasisError::PortFailure("surface_id is required when persist=true".to_string())
                })?;
            let slot = input.slot.as_deref().unwrap_or("main");
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
                env_record.spec.components[index] = component.clone();
            } else {
                env_record.spec.components.push(component.clone());
            }
            let validation_errors =
                medousa_types::environment_validate::validate_environment_spec(&env_record.spec);
            if !validation_errors.is_empty() {
                if let Some(index) = env_record
                    .spec
                    .components
                    .iter()
                    .position(|entry| entry.id == component_id)
                {
                    env_record.spec.components.remove(index);
                }
                ok = false;
                persisted = Some(false);
                errors = Some(validation_errors);
                return Ok(UiPresentOutput::Presented {
                    ok,
                    artifact_id: record.artifact_id,
                    label: record.label,
                    mime: record.content_type,
                    presentation: record.presentation,
                    height_px: record.height_px,
                    byte_size: record.byte_size,
                    persisted,
                    errors,
                    persisted_component_id,
                    environment_revision,
                    live,
                    nav_visible,
                    hint,
                });
            }
            let updated = crate::environment_store::environment_hub()
                .put(env_record.spec, "agent")
                .await
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            let _ = crate::artifact_store::register_artifact_alias(
                &session_id_for_alias,
                component_id,
                &record.artifact_id,
            );
            persisted = Some(true);
            persisted_component_id = Some(component_id.to_string());
            environment_revision = Some(updated.revision);
            let visible = crate::custom_view_status::surface_nav_visible(&updated.spec, surface_id);
            live = Some(true);
            nav_visible = Some(visible);
            if !visible {
                hint = Some(format!(
                    "Surface '{surface_id}' is not in the active layout preset — call cognition_environment_patch with add_to_active_preset or cognition_custom_view_compose."
                ));
            }
        }

        Ok(UiPresentOutput::Presented {
            ok,
            artifact_id: record.artifact_id,
            label: record.label,
            mime: record.content_type,
            presentation: record.presentation,
            height_px: record.height_px,
            byte_size: record.byte_size,
            persisted,
            errors,
            persisted_component_id,
            environment_revision,
            live,
            nav_visible,
            hint,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_api::TurnSurfaceContext;
    use crate::runtime_session::{RUNTIME_BOOTSTRAP_SESSION_ID, is_runtime_bootstrap_session_id};

    #[test]
    fn surface_supports_ui_artifacts_requires_client_flag() {
        assert!(!surface_supports_ui_artifacts(None));
        assert!(!surface_supports_ui_artifacts(Some(
            &TurnSurfaceContext::tui()
        )));
        assert!(surface_supports_ui_artifacts(Some(
            &TurnSurfaceContext::tui().with_ui_artifacts(true)
        )));
    }

    #[test]
    fn persistent_constructor_keeps_content_bytes_and_requires_identity() {
        let input = UiPresentInput::persistent_component(
            "  title  ",
            "  <div>hello</div>\n",
            Some("panel".to_string()),
            " component ",
            " surface ",
            "main",
        )
        .expect("persistent input");

        assert_eq!(input.title.as_deref(), Some("title"));
        assert_eq!(input.html.as_deref(), Some("  <div>hello</div>\n"));
        assert_eq!(input.component_id.as_deref(), Some("component"));
        assert_eq!(input.surface_id.as_deref(), Some("surface"));
        assert_eq!(input.persist, Some(true));
        assert!(UiPresentInput::persistent_component(
            "title", "html", None, " ", "surface", "main"
        )
        .is_err());
    }

    #[tokio::test]
    async fn ui_present_rejects_bootstrap_only_session_resolution() {
        let turn_scope = Arc::new(RwLock::new(None::<TurnContinuationScope>));
        let tool = CognitionUiPresentTool::new(turn_scope);
        let err = tool.resolve_session_id().await.expect_err("bootstrap-only");
        assert!(err.to_string().contains("not a chat session"));
        assert!(is_runtime_bootstrap_session_id(
            RUNTIME_BOOTSTRAP_SESSION_ID
        ));
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
            supports_liquid_markdown: true,
            supports_browser_host: false,
            channel_surface: Some("home-desktop".to_string()),
        })));
        let tool = CognitionUiPresentTool::new(turn_scope);
        let session_id = tool.resolve_session_id().await.expect("turn scope session");
        assert_eq!(session_id, "medousa-home");
    }
}
