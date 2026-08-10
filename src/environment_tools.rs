//! Agent tools for environment spec and component canvas CRUD.

use std::sync::Arc;

use chrono::Utc;
use medousa_types::environment::{
    ComponentDef, ComponentType, EnvironmentPendingProposal, EnvironmentSpec, SurfaceDef,
    SurfaceKind, SurfaceLayout, UiPresentation, activate_layout_preset,
};
use medousa_types::environment_validate::validate_environment_spec;
use schemars::JsonSchema;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::semantic_values::TrimmedText;
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_ENVIRONMENT_GET: &str = "cognition_environment_get";
pub const COGNITION_ENVIRONMENT_APPLY: &str = "cognition_environment_apply";
pub const COGNITION_ENVIRONMENT_ACTIVATE_PRESET: &str = "cognition_environment_activate_preset";
pub const COGNITION_ENVIRONMENT_PROPOSE: &str = "cognition_environment_propose";
pub const COGNITION_COMPONENT_LIST: &str = "cognition_component_list";
pub const COGNITION_COMPONENT_GET: &str = "cognition_component_get";
pub const COGNITION_COMPONENT_CREATE: &str = "cognition_component_create";
pub const COGNITION_COMPONENT_UPDATE: &str = "cognition_component_update";
pub const COGNITION_COMPONENT_DELETE: &str = "cognition_component_delete";

const COGNITION_ENVIRONMENT_GET_ID: ToolId = ToolId::new(COGNITION_ENVIRONMENT_GET);
const COGNITION_ENVIRONMENT_APPLY_ID: ToolId = ToolId::new(COGNITION_ENVIRONMENT_APPLY);
const COGNITION_ENVIRONMENT_ACTIVATE_PRESET_ID: ToolId =
    ToolId::new(COGNITION_ENVIRONMENT_ACTIVATE_PRESET);
const COGNITION_ENVIRONMENT_PROPOSE_ID: ToolId = ToolId::new(COGNITION_ENVIRONMENT_PROPOSE);
const COGNITION_COMPONENT_LIST_ID: ToolId = ToolId::new(COGNITION_COMPONENT_LIST);
const COGNITION_COMPONENT_GET_ID: ToolId = ToolId::new(COGNITION_COMPONENT_GET);
const COGNITION_COMPONENT_CREATE_ID: ToolId = ToolId::new(COGNITION_COMPONENT_CREATE);
const COGNITION_COMPONENT_UPDATE_ID: ToolId = ToolId::new(COGNITION_COMPONENT_UPDATE);
const COGNITION_COMPONENT_DELETE_ID: ToolId = ToolId::new(COGNITION_COMPONENT_DELETE);

const ENVIRONMENT_SPEC_PATCH_HINT: &str = "Patch surfaces/components on the full spec. Custom surfaces must be listed in the active layout preset surfaces array. Components render only on kind=custom surfaces.";

fn component_def_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "type", "surfaceId", "slot"],
        "properties": {
            "id": { "type": "string", "description": "Unique component id (kebab-case)" },
            "type": {
                "type": "string",
                "enum": ["presentation", "chrome_action", "artifact", "medousa_view", "builtin_panel", "media_embed", "scene"],
                "description": "presentation = HTML artifact; scene = native Liquid scene (config.scene:{ops:[…]}, preferred for interactive widgets); media_embed = native Spotify/Apple iframe"
            },
            "surfaceId": {
                "type": "string",
                "description": "Target surface id — agent components MUST use kind=custom surfaces (not home/chat builtins)"
            },
            "slot": {
                "type": "string",
                "enum": ["main", "header", "fab", "sidebar", "inline"],
                "description": "Layout zone on the surface"
            },
            "label": { "type": "string" },
            "config": {
                "type": "object",
                "description": "Type-specific config — presentation uses { artifactId: string } (art:… id from cognition_ui_present, not the component id); scene uses { scene: { ops: [...] } } (same op JSON as cognition_ui_scene, stored opaquely)"
            },
            "presentation": {
                "type": "string",
                "enum": ["inline", "panel", "fullscreen"]
            },
            "feeds": { "type": "array", "items": { "type": "string" } }
        },
        "example": {
            "id": "writing-manuscript",
            "type": "presentation",
            "surfaceId": "writing-studio",
            "slot": "main",
            "label": "Manuscript",
            "config": { "artifactId": "art-writing-demo" },
            "presentation": "inline"
        }
    })
}

pub fn register_environment_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    crate::environment_wiki_tools::register_environment_wiki_tools(registry)?;
    registry.register_typed_tool(CognitionEnvironmentGetTool)?;
    registry.register_typed_tool(CognitionEnvironmentProposeTool)?;
    registry.register_typed_tool(CognitionEnvironmentApplyTool)?;
    registry.register_typed_tool(CognitionEnvironmentActivatePresetTool)?;
    registry.register_typed_tool(CognitionComponentListTool)?;
    registry.register_typed_tool(CognitionComponentGetTool)?;
    registry.register_typed_tool(CognitionComponentCreateTool::new(turn_scope.clone()))?;
    registry.register_typed_tool(CognitionComponentUpdateTool::new(turn_scope.clone()))?;
    registry.register_typed_tool(CognitionComponentDeleteTool)?;
    Ok(())
}

struct CognitionEnvironmentGetTool;

#[derive(Debug, Deserialize, JsonSchema)]
struct EnvironmentProfileInput {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
}

#[derive(Debug)]
struct EnvironmentProfileCommand {
    profile_id: Option<TrimmedText>,
}

impl TryFrom<EnvironmentProfileInput> for EnvironmentProfileCommand {
    type Error = StasisError;

    fn try_from(input: EnvironmentProfileInput) -> Result<Self, Self::Error> {
        Ok(Self {
            profile_id: input
                .profile_id
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct EnvironmentGetOutput {
    ok: bool,
    revision: u64,
    #[schemars(with = "serde_json::Value")]
    spec: EnvironmentSpec,
}

#[medousa_tool(id = COGNITION_ENVIRONMENT_GET_ID)]
impl CognitionEnvironmentGetTool {
    /// Read the persisted environment spec and component canvas for the active profile.
    async fn invoke_typed(
        &self,
        input: EnvironmentProfileInput,
    ) -> stasis::prelude::Result<EnvironmentGetOutput> {
        let command = EnvironmentProfileCommand::try_from(input)?;
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(EnvironmentGetOutput {
            ok: true,
            revision: record.revision,
            spec: record.spec,
        })
    }
}

struct CognitionEnvironmentProposeTool;

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct ProposedEnvironmentSpecInput(EnvironmentSpec);

impl JsonSchema for ProposedEnvironmentSpecInput {
    fn schema_name() -> String {
        "ProposedEnvironmentSpecInput".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        serde_json::from_value(json!({
            "type": "object",
            "description": ENVIRONMENT_SPEC_PATCH_HINT,
            "properties": {
                "surfaces": { "type": "array", "items": { "type": "object" } },
                "components": { "type": "array", "items": component_def_schema() },
                "layoutPresets": { "type": "array" },
                "activePresetId": { "type": "string" }
            }
        }))
        .expect("valid proposed environment compatibility schema")
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EnvironmentProposeInput {
    spec: ProposedEnvironmentSpecInput,
}

#[derive(Debug, Serialize, JsonSchema)]
struct EnvironmentProposeOutput {
    ok: bool,
    errors: Vec<String>,
    diff_summary: String,
    #[schemars(with = "serde_json::Value")]
    proposed_spec: EnvironmentSpec,
    pending_operator_approval: bool,
}

#[medousa_tool(id = COGNITION_ENVIRONMENT_PROPOSE_ID)]
impl CognitionEnvironmentProposeTool {
    /// Validate a proposed environment spec before applying. Returns errors[] on failure. Add custom surfaces to spec.surfaces AND include their ids in the active preset surfaces list.
    async fn invoke_typed(
        &self,
        input: EnvironmentProposeInput,
    ) -> stasis::prelude::Result<EnvironmentProposeOutput> {
        let spec = input.spec.0;
        let profile_id = resolve_profile_id(Some(spec.profile_id.as_str()));
        let errors = validate_environment_spec(&spec);
        let diff_summary = format!(
            "surfaces={} components={} preset={}",
            spec.surfaces.len(),
            spec.components.len(),
            spec.active_preset_id.as_deref().unwrap_or("default")
        );
        environment_hub()
            .set_pending(
                &profile_id,
                EnvironmentPendingProposal {
                    proposed_spec: spec.clone(),
                    diff_summary: diff_summary.clone(),
                    errors: errors.clone(),
                    proposed_at: Utc::now(),
                    proposed_by: "agent".to_string(),
                },
            )
            .await;
        Ok(EnvironmentProposeOutput {
            ok: errors.is_empty(),
            errors,
            diff_summary,
            proposed_spec: spec,
            pending_operator_approval: true,
        })
    }
}

struct CognitionEnvironmentApplyTool;

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct ApprovedEnvironmentSpecInput(EnvironmentSpec);

impl JsonSchema for ApprovedEnvironmentSpecInput {
    fn schema_name() -> String {
        "ApprovedEnvironmentSpecInput".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        serde_json::from_value(json!({
            "type": "object",
            "description": ENVIRONMENT_SPEC_PATCH_HINT
        }))
        .expect("valid approved environment compatibility schema")
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EnvironmentApplyInput {
    spec: ApprovedEnvironmentSpecInput,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
enum EnvironmentApplyOutput {
    Success {
        ok: bool,
        revision: u64,
        #[schemars(with = "serde_json::Value")]
        spec: Box<EnvironmentSpec>,
    },
    Failure {
        ok: bool,
        errors: Vec<String>,
    },
}

#[medousa_tool(id = COGNITION_ENVIRONMENT_APPLY_ID)]
impl CognitionEnvironmentApplyTool {
    /// Apply an approved environment spec to the daemon store. Surfaces, components, and chrome sync to Home.
    async fn invoke_typed(
        &self,
        input: EnvironmentApplyInput,
    ) -> stasis::prelude::Result<EnvironmentApplyOutput> {
        let spec = input.spec.0;
        let errors = validate_environment_spec(&spec);
        if !errors.is_empty() {
            return Ok(EnvironmentApplyOutput::Failure { ok: false, errors });
        }
        let record = environment_hub()
            .put(spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        environment_hub()
            .clear_pending(&record.spec.profile_id)
            .await;
        Ok(EnvironmentApplyOutput::Success {
            ok: true,
            revision: record.revision,
            spec: Box::new(record.spec),
        })
    }
}

struct CognitionEnvironmentActivatePresetTool;

#[derive(Debug, Deserialize, JsonSchema)]
struct EnvironmentActivatePresetInput {
    /// Layout preset id from environment_get layoutPresets
    preset_id: String,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
}

#[derive(Debug)]
struct EnvironmentActivatePresetCommand {
    preset_id: TrimmedText,
    profile_id: Option<TrimmedText>,
}

impl TryFrom<EnvironmentActivatePresetInput> for EnvironmentActivatePresetCommand {
    type Error = StasisError;

    fn try_from(input: EnvironmentActivatePresetInput) -> Result<Self, Self::Error> {
        let preset_id = TrimmedText::new(input.preset_id)
            .map_err(|_| StasisError::PortFailure("preset_id is required".to_string()))?;
        Ok(Self {
            preset_id,
            profile_id: input
                .profile_id
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct EnvironmentActivatePresetOutput {
    ok: bool,
    revision: u64,
    active_preset_id: Option<String>,
}

#[medousa_tool(id = COGNITION_ENVIRONMENT_ACTIVATE_PRESET_ID)]
impl CognitionEnvironmentActivatePresetTool {
    /// Switch the active layout preset (morning vs focus vs custom). Updates nav surfaces and shell chrome from the preset.
    async fn invoke_typed(
        &self,
        input: EnvironmentActivatePresetInput,
    ) -> stasis::prelude::Result<EnvironmentActivatePresetOutput> {
        let command = EnvironmentActivatePresetCommand::try_from(input)?;
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let preset_id = command.preset_id.into_string();
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        activate_layout_preset(&mut record.spec, &preset_id).map_err(StasisError::PortFailure)?;
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(EnvironmentActivatePresetOutput {
            ok: true,
            revision: updated.revision,
            active_preset_id: updated.spec.active_preset_id,
        })
    }
}

struct CognitionComponentListTool;

#[derive(Debug, Serialize, JsonSchema)]
struct ComponentListOutput {
    ok: bool,
    components: Vec<ComponentDef>,
}

#[medousa_tool(id = COGNITION_COMPONENT_LIST_ID)]
impl CognitionComponentListTool {
    /// List all persisted components on the environment canvas.
    async fn invoke_typed(
        &self,
        input: EnvironmentProfileInput,
    ) -> stasis::prelude::Result<ComponentListOutput> {
        let command = EnvironmentProfileCommand::try_from(input)?;
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(ComponentListOutput {
            ok: true,
            components: record.spec.components,
        })
    }
}

struct CognitionComponentGetTool;

#[derive(Debug, Deserialize, JsonSchema)]
struct ComponentIdInput {
    component_id: String,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
}

#[derive(Debug)]
struct ComponentIdCommand {
    component_id: TrimmedText,
    profile_id: Option<TrimmedText>,
}

impl TryFrom<ComponentIdInput> for ComponentIdCommand {
    type Error = StasisError;

    fn try_from(input: ComponentIdInput) -> Result<Self, Self::Error> {
        Ok(Self {
            component_id: TrimmedText::new(input.component_id)
                .map_err(|_| StasisError::PortFailure("component_id required".to_string()))?,
            profile_id: input
                .profile_id
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct ComponentGetOutput {
    ok: bool,
    component: ComponentDef,
}

#[medousa_tool(id = COGNITION_COMPONENT_GET_ID)]
impl CognitionComponentGetTool {
    /// Read one component by id from the canvas.
    async fn invoke_typed(
        &self,
        input: ComponentIdInput,
    ) -> stasis::prelude::Result<ComponentGetOutput> {
        let command = ComponentIdCommand::try_from(input)?;
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let component_id = command.component_id.into_string();
        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let component = record
            .spec
            .components
            .iter()
            .find(|component| component.id == component_id.as_str())
            .cloned()
            .ok_or_else(|| {
                StasisError::PortFailure(format!("component not found: {component_id}"))
            })?;
        Ok(ComponentGetOutput {
            ok: true,
            component,
        })
    }
}

struct CognitionComponentCreateTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionComponentCreateTool {
    fn new(turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>) -> Self {
        Self { turn_scope }
    }
}

#[derive(Debug, Default)]
enum CompatibleComponentInput {
    #[default]
    Missing,
    Parsed(ComponentDef),
    Invalid(String),
}

impl CompatibleComponentInput {
    fn into_result(self) -> Result<ComponentDef, String> {
        match self {
            Self::Missing => Err("component required".to_string()),
            Self::Parsed(component) => Ok(component),
            Self::Invalid(error) => Err(error),
        }
    }
}

impl<'de> Deserialize<'de> for CompatibleComponentInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(match serde_json::from_value(value) {
            Ok(component) => Self::Parsed(component),
            Err(error) => Self::Invalid(format!("invalid component: {error}")),
        })
    }
}

impl JsonSchema for CompatibleComponentInput {
    fn schema_name() -> String {
        "CompatibleComponentInput".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        serde_json::from_value(component_def_schema())
            .expect("valid component compatibility schema")
    }
}

#[derive(Debug, JsonSchema)]
struct ComponentCreateInput {
    #[schemars(required)]
    component: CompatibleComponentInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
}

impl<'de> Deserialize<'de> for ComponentCreateInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            component: CompatibleComponentInput,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            profile_id: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            component: input.component,
            profile_id: input.profile_id,
        })
    }
}

#[derive(Debug)]
struct ComponentCreateCommand {
    component: ComponentDef,
    profile_id: Option<TrimmedText>,
}

impl TryFrom<ComponentCreateInput> for ComponentCreateCommand {
    type Error = String;

    fn try_from(input: ComponentCreateInput) -> Result<Self, Self::Error> {
        Ok(Self {
            component: input.component.into_result()?,
            profile_id: input
                .profile_id
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
enum ComponentCreateOutput {
    Success {
        ok: bool,
        revision: u64,
        component: Option<Box<ComponentDef>>,
        live: bool,
        nav_visible: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
    },
    Failure {
        ok: bool,
        errors: Vec<String>,
    },
}

#[medousa_tool(id = COGNITION_COMPONENT_CREATE_ID)]
impl CognitionComponentCreateTool {
    /// Add a presentation or chrome_action component to a custom surface slot. Use camelCase fields (surfaceId, type). Verify with cognition_component_list.
    async fn invoke_typed(
        &self,
        input: ComponentCreateInput,
    ) -> stasis::prelude::Result<ComponentCreateOutput> {
        let command = match ComponentCreateCommand::try_from(input) {
            Ok(command) => command,
            Err(error) => {
                return Ok(ComponentCreateOutput::Failure {
                    ok: false,
                    errors: vec![error],
                });
            }
        };
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let component = command.component;
        let session_id = tool_session_id(&self.turn_scope).await;
        if let Some(err) =
            validate_presentation_component_artifact(session_id.as_deref(), &component)
        {
            return Ok(ComponentCreateOutput::Failure {
                ok: false,
                errors: vec![err],
            });
        }
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if record.spec.components.iter().any(|c| c.id == component.id) {
            return Ok(ComponentCreateOutput::Failure {
                ok: false,
                errors: vec![format!("component already exists: {}", component.id)],
            });
        }
        record.spec.components.push(component.clone());
        let errors = validate_environment_spec(&record.spec);
        if !errors.is_empty() {
            record.spec.components.pop();
            return Ok(ComponentCreateOutput::Failure { ok: false, errors });
        }
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if let Some(session_id) = session_id.as_deref() {
            register_presentation_aliases(session_id, &component);
        }
        let nav_visible = surface_nav_visible_for_spec(&updated.spec, &component.surface_id);
        Ok(ComponentCreateOutput::Success {
            ok: true,
            revision: updated.revision,
            component: updated.spec.components.last().cloned().map(Box::new),
            live: true,
            nav_visible,
            hint: crate::custom_view_status::nav_visibility_hint(
                &component.surface_id,
                nav_visible,
            ),
        })
    }
}

struct CognitionComponentUpdateTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionComponentUpdateTool {
    fn new(turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>) -> Self {
        Self { turn_scope }
    }
}

#[derive(Debug, Default)]
enum CompatibleComponentConfig {
    #[default]
    Missing,
    Value(Value),
}

impl JsonSchema for CompatibleComponentConfig {
    fn schema_name() -> String {
        "CompatibleComponentConfig".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::Object.into()),
            ..SchemaObject::default()
        })
    }
}

#[derive(Debug, Default)]
struct ComponentPatchInput {
    label: Option<String>,
    surface_id: Option<String>,
    slot: Option<String>,
    config: CompatibleComponentConfig,
    presentation: Option<UiPresentation>,
}

impl<'de> Deserialize<'de> for ComponentPatchInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let config = value
            .get("config")
            .cloned()
            .map(CompatibleComponentConfig::Value)
            .unwrap_or_default();
        Ok(Self {
            label: value
                .get("label")
                .and_then(Value::as_str)
                .map(str::to_string),
            surface_id: value
                .get("surfaceId")
                .or_else(|| value.get("surface_id"))
                .and_then(Value::as_str)
                .map(str::to_string),
            slot: value
                .get("slot")
                .and_then(Value::as_str)
                .map(str::to_string),
            config,
            presentation: value
                .get("presentation")
                .and_then(Value::as_str)
                .map(|presentation| match presentation {
                    "panel" => UiPresentation::Panel,
                    "fullscreen" => UiPresentation::Fullscreen,
                    _ => UiPresentation::Inline,
                }),
        })
    }
}

#[allow(dead_code)]
#[derive(JsonSchema)]
struct ComponentPatchSchema {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        rename = "surfaceId",
        with = "String",
        skip_serializing_if = "Option::is_none"
    )]
    surface_id_camel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    surface_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    slot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "CompatibleComponentConfig",
        skip_serializing_if = "Option::is_none"
    )]
    config: Option<CompatibleComponentConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "UiPresentation", skip_serializing_if = "Option::is_none")]
    presentation: Option<UiPresentation>,
}

impl JsonSchema for ComponentPatchInput {
    fn schema_name() -> String {
        "ComponentPatchInput".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        ComponentPatchSchema::json_schema(generator)
    }
}

#[derive(Debug, JsonSchema)]
struct ComponentUpdateInput {
    #[schemars(required, with = "String")]
    component_id: Option<String>,
    /// Partial update — label, surfaceId|surface_id, slot, config, presentation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "ComponentPatchInput", skip_serializing_if = "Option::is_none")]
    patch: Option<ComponentPatchInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
}

impl<'de> Deserialize<'de> for ComponentUpdateInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            component_id: Option<String>,
            #[serde(default)]
            patch: Option<ComponentPatchInput>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            profile_id: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            component_id: input.component_id,
            patch: input.patch,
            profile_id: input.profile_id,
        })
    }
}

#[derive(Debug)]
struct ComponentUpdateCommand {
    component_id: TrimmedText,
    patch: ComponentPatchInput,
    profile_id: Option<TrimmedText>,
}

impl TryFrom<ComponentUpdateInput> for ComponentUpdateCommand {
    type Error = StasisError;

    fn try_from(input: ComponentUpdateInput) -> Result<Self, Self::Error> {
        Ok(Self {
            component_id: input
                .component_id
                .ok_or_else(|| StasisError::PortFailure("component_id required".to_string()))
                .and_then(|value| {
                    TrimmedText::new(value)
                        .map_err(|_| StasisError::PortFailure("component_id required".to_string()))
                })?,
            patch: input.patch.unwrap_or_default(),
            profile_id: input
                .profile_id
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
enum ComponentUpdateOutput {
    Success {
        ok: bool,
        revision: u64,
        component: ComponentDef,
    },
    Failure {
        ok: bool,
        errors: Vec<String>,
    },
}

#[medousa_tool(id = COGNITION_COMPONENT_UPDATE_ID)]
impl CognitionComponentUpdateTool {
    /// Patch an existing canvas component by id.
    async fn invoke_typed(
        &self,
        input: ComponentUpdateInput,
    ) -> stasis::prelude::Result<ComponentUpdateOutput> {
        let command = ComponentUpdateCommand::try_from(input)?;
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let component_id = command.component_id.into_string();
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let Some(index) = record
            .spec
            .components
            .iter()
            .position(|c| c.id == component_id.as_str())
        else {
            return Ok(ComponentUpdateOutput::Failure {
                ok: false,
                errors: vec![format!("component not found: {component_id}")],
            });
        };
        let previous = record.spec.components[index].clone();
        let mut existing = previous.clone();
        apply_component_patch(&mut existing, command.patch);
        existing.updated_at = Some(Utc::now());
        let session_id = tool_session_id(&self.turn_scope).await;
        if let Some(err) =
            validate_presentation_component_artifact(session_id.as_deref(), &existing)
        {
            return Ok(ComponentUpdateOutput::Failure {
                ok: false,
                errors: vec![err],
            });
        }
        record.spec.components[index] = existing.clone();
        let errors = validate_environment_spec(&record.spec);
        if !errors.is_empty() {
            record.spec.components[index] = previous;
            return Ok(ComponentUpdateOutput::Failure { ok: false, errors });
        }
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if let Some(session_id) = session_id.as_deref() {
            register_presentation_aliases(session_id, &existing);
        }
        Ok(ComponentUpdateOutput::Success {
            ok: true,
            revision: updated.revision,
            component: existing,
        })
    }
}

struct CognitionComponentDeleteTool;

#[derive(Debug, Serialize, JsonSchema)]
struct ComponentDeleteOutput {
    ok: bool,
    revision: u64,
}

#[medousa_tool(id = COGNITION_COMPONENT_DELETE_ID)]
impl CognitionComponentDeleteTool {
    /// Remove a component from the canvas.
    async fn invoke_typed(
        &self,
        input: ComponentIdInput,
    ) -> stasis::prelude::Result<ComponentDeleteOutput> {
        let command = ComponentIdCommand::try_from(input)?;
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let component_id = command.component_id.into_string();
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let before = record.spec.components.len();
        record
            .spec
            .components
            .retain(|component| component.id != component_id.as_str());
        if record.spec.components.len() == before {
            return Err(StasisError::PortFailure(format!(
                "component not found: {component_id}"
            )));
        }
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(ComponentDeleteOutput {
            ok: true,
            revision: updated.revision,
        })
    }
}

fn profile_from_typed(profile_id: Option<&str>) -> String {
    resolve_profile_id(profile_id)
}

async fn tool_session_id(
    turn_scope: &Arc<RwLock<Option<TurnContinuationScope>>>,
) -> Option<String> {
    turn_scope
        .read()
        .await
        .as_ref()
        .map(|scope| scope.session_id.clone())
        .filter(|id| !id.trim().is_empty())
}

fn presentation_artifact_id(config: &Value) -> Option<String> {
    config
        .get("artifactId")
        .or_else(|| config.get("artifact_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn validate_presentation_component_artifact(
    session_id: Option<&str>,
    component: &ComponentDef,
) -> Option<String> {
    if component.component_type != ComponentType::Presentation {
        return None;
    }
    let Some(artifact_ref) = presentation_artifact_id(&component.config) else {
        return Some(
            "presentation components require config.artifactId from cognition_ui_present"
                .to_string(),
        );
    };
    let Some(session_id) = session_id.filter(|id| !id.trim().is_empty()) else {
        if artifact_ref.starts_with("art:") {
            return None;
        }
        return Some(format!(
            "config.artifactId '{artifact_ref}' must be a canonical art:… id from cognition_ui_present"
        ));
    };
    if crate::artifact_store::presentation_artifact_exists(session_id, &artifact_ref) {
        return None;
    }
    Some(format!(
        "artifact not found for session '{session_id}': {artifact_ref}. \
         Call cognition_ui_present first and set config.artifactId to the returned artifact_id \
         (component id '{}' is not an artifact id).",
        component.id
    ))
}

fn register_presentation_aliases(session_id: &str, component: &ComponentDef) {
    if component.component_type != ComponentType::Presentation {
        return;
    }
    let Some(artifact_ref) = presentation_artifact_id(&component.config) else {
        return;
    };
    let resolved = crate::artifact_store::resolve_artifact_reference(session_id, &artifact_ref);
    if !resolved.starts_with("art:") {
        return;
    }
    let _ = crate::artifact_store::register_artifact_alias(session_id, &component.id, &resolved);
    if artifact_ref != resolved && artifact_ref != component.id {
        let _ =
            crate::artifact_store::register_artifact_alias(session_id, &artifact_ref, &resolved);
    }
}

fn apply_component_patch(component: &mut ComponentDef, patch: ComponentPatchInput) {
    if let Some(label) = patch.label {
        component.label = Some(label);
    }
    if let Some(surface_id) = patch.surface_id {
        component.surface_id = surface_id;
    }
    if let Some(slot) = patch.slot {
        component.slot = slot;
    }
    if let CompatibleComponentConfig::Value(config) = patch.config {
        component.config = config;
    }
    if let Some(presentation) = patch.presentation {
        component.presentation = Some(presentation);
    }
}

pub fn surface_nav_visible_for_spec(spec: &EnvironmentSpec, surface_id: &str) -> bool {
    crate::custom_view_status::surface_nav_visible(spec, surface_id)
}

/// Helper for agent-driven custom surface creation.
pub fn make_custom_surface(id: &str, label: &str, icon: &str) -> SurfaceDef {
    SurfaceDef {
        id: id.to_string(),
        label: label.to_string(),
        icon: icon.to_string(),
        kind: SurfaceKind::Custom,
        builtin_id: None,
        layout: SurfaceLayout::Dashboard,
        slots: vec![],
        mobile_tab: None,
        layout_root: None,
    }
}

pub fn make_presentation_component(
    id: &str,
    surface_id: &str,
    artifact_id: &str,
    label: &str,
) -> ComponentDef {
    ComponentDef {
        id: id.to_string(),
        component_type: ComponentType::Presentation,
        surface_id: surface_id.to_string(),
        slot: "main".to_string(),
        label: Some(label.to_string()),
        config: json!({ "artifactId": artifact_id }),
        presentation: Some(UiPresentation::Inline),
        feeds: vec![],
        updated_at: Some(Utc::now()),
    }
}

/// Build a durable Liquid scene component. `scene` is the opaque payload
/// (e.g. `{ "ops": [...] }`) the daemon stores verbatim and the client renders.
pub fn make_scene_component(id: &str, surface_id: &str, scene: Value, label: &str) -> ComponentDef {
    ComponentDef {
        id: id.to_string(),
        component_type: ComponentType::Scene,
        surface_id: surface_id.to_string(),
        slot: "main".to_string(),
        label: Some(label.to_string()),
        config: json!({ "scene": scene }),
        presentation: None,
        feeds: vec![],
        updated_at: Some(Utc::now()),
    }
}

pub fn make_chrome_action_component(
    id: &str,
    surface_id: &str,
    slot: &str,
    action: &str,
    label: &str,
) -> ComponentDef {
    ComponentDef {
        id: id.to_string(),
        component_type: ComponentType::ChromeAction,
        surface_id: surface_id.to_string(),
        slot: slot.to_string(),
        label: Some(label.to_string()),
        config: json!({ "action": action }),
        presentation: None,
        feeds: vec![],
        updated_at: Some(Utc::now()),
    }
}

#[cfg(test)]
mod demo_tests {
    use medousa_types::environment_default::writing_studio_demo_spec;
    use medousa_types::environment_validate::is_valid_environment_spec;

    use super::*;

    #[test]
    fn writing_studio_demo_spec_validates() {
        let spec = writing_studio_demo_spec("personal");
        assert!(is_valid_environment_spec(&spec));
    }

    #[test]
    fn presentation_component_rejects_missing_artifact_id() {
        let component = ComponentDef {
            id: "demo".to_string(),
            component_type: ComponentType::Presentation,
            surface_id: "writing-studio".to_string(),
            slot: "main".to_string(),
            label: Some("Demo".to_string()),
            config: json!({}),
            presentation: Some(UiPresentation::Inline),
            feeds: vec![],
            updated_at: None,
        };
        let err = validate_presentation_component_artifact(Some("sess-1"), &component)
            .expect("missing artifactId");
        assert!(err.contains("artifactId"));
    }

    #[test]
    fn component_create_input_preserves_legacy_error_receipts() {
        let missing: ComponentCreateInput =
            serde_json::from_value(json!({})).expect("missing component stays handler-visible");
        assert_eq!(
            missing.component.into_result().unwrap_err(),
            "component required"
        );

        let invalid: ComponentCreateInput = serde_json::from_value(json!({ "component": null }))
            .expect("invalid component stays handler-visible");
        assert!(
            invalid
                .component
                .into_result()
                .unwrap_err()
                .starts_with("invalid component:")
        );
    }

    #[test]
    fn component_patch_preserves_alias_precedence_and_opaque_config() {
        let patch: ComponentPatchInput = serde_json::from_value(json!({
            "surfaceId": 42,
            "surface_id": "ignored-fallback",
            "config": null,
            "presentation": "future-value"
        }))
        .expect("legacy-compatible patch");

        assert_eq!(patch.surface_id, None);
        assert!(matches!(
            patch.config,
            CompatibleComponentConfig::Value(Value::Null)
        ));
        assert_eq!(patch.presentation, Some(UiPresentation::Inline));
    }

    #[test]
    fn scene_component_bypasses_artifact_check_and_validates_on_custom_surface() {
        let scene = json!({ "ops": [{ "op": "plan_layout" }] });
        let component = make_scene_component("trip-scene", "japan-trip", scene, "Itinerary");
        assert_eq!(component.component_type, ComponentType::Scene);
        // Scene never triggers the presentation artifact-existence gate.
        assert!(validate_presentation_component_artifact(Some("sess-1"), &component).is_none());

        // On a custom surface the opaque scene config validates.
        let mut spec = writing_studio_demo_spec("personal");
        let mut scene_component = component.clone();
        scene_component.surface_id = "writing-studio".to_string();
        spec.components.push(scene_component);
        assert!(is_valid_environment_spec(&spec));
    }

    #[test]
    fn environment_commands_normalize_ids_and_keep_component_config_opaque() {
        let profile = EnvironmentProfileCommand::try_from(EnvironmentProfileInput {
            profile_id: Some(" profile-a ".to_string()),
        })
        .expect("profile command");
        assert_eq!(
            profile.profile_id.as_ref().map(TrimmedText::as_str),
            Some("profile-a")
        );

        let component = ComponentDef {
            id: "component-a".to_string(),
            component_type: ComponentType::Presentation,
            surface_id: "surface-a".to_string(),
            slot: "main".to_string(),
            label: Some("Chart".to_string()),
            config: json!({"artifactId": "art:chart", "future": {"keep": true}}),
            presentation: Some(UiPresentation::Inline),
            feeds: vec![],
            updated_at: None,
        };
        let create = ComponentCreateCommand::try_from(ComponentCreateInput {
            component: CompatibleComponentInput::Parsed(component.clone()),
            profile_id: Some(" profile-a ".to_string()),
        })
        .expect("create command");
        assert_eq!(create.component.id, "component-a");
        assert_eq!(create.component.config["future"]["keep"], Value::Bool(true));

        let update = ComponentUpdateCommand::try_from(ComponentUpdateInput {
            component_id: Some(" component-a ".to_string()),
            patch: None,
            profile_id: None,
        })
        .expect("update command");
        assert_eq!(update.component_id.as_str(), "component-a");
        assert!(matches!(
            update.patch.config,
            CompatibleComponentConfig::Missing
        ));
    }

    #[test]
    fn component_commands_reject_blank_identifiers() {
        let error = ComponentIdCommand::try_from(ComponentIdInput {
            component_id: " \n\t".to_string(),
            profile_id: None,
        })
        .expect_err("blank component id should fail");
        assert!(error.to_string().contains("component_id required"));

        let error = EnvironmentActivatePresetCommand::try_from(EnvironmentActivatePresetInput {
            preset_id: " \n\t".to_string(),
            profile_id: None,
        })
        .expect_err("blank preset id should fail");
        assert!(error.to_string().contains("preset_id is required"));
    }
}
