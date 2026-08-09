//! Agent tools for custom view ergonomics — doctor, compose, and registration.

use std::sync::Arc;

use chrono::Utc;
use medousa_types::environment::{
    EnvironmentPatchOp, EnvironmentStatusResponse, SurfaceKind, SurfaceLayout,
};
use medousa_types::environment_validate::validate_environment_spec;
use medousa_types::feed::is_valid_feed_id;
use medousa_types::layout::LayoutNode;
use schemars::JsonSchema;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Serialize};
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use tokio::sync::{RwLock, mpsc};

use crate::custom_view_status::{
    DoctorDiagnosticOptions, build_environment_status, surface_nav_visible,
};
use crate::environment_patch::execute_environment_patch;
use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::environment_tools::make_presentation_component;
use crate::events::TuiEvent;
use crate::recurring_delivery::RecurringDeliverySpec;
use crate::recurring_feed::RecurringFeedSpec;
use crate::runtime_tools::{
    CognitionRuntimeRecurringRegisterTool, RuntimeRecurringRegisterInput,
    RuntimeRecurringRegisterOutput,
};
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{ToolId, medousa_tool};
use crate::ui_present_tools::{CognitionUiPresentTool, UiPresentInput, UiPresentOutput};

pub const COGNITION_CUSTOM_VIEW_DOCTOR: &str = "cognition_custom_view_doctor";
pub const COGNITION_CUSTOM_VIEW_COMPOSE: &str = "cognition_custom_view_compose";
const COGNITION_CUSTOM_VIEW_DOCTOR_ID: ToolId = ToolId::new(COGNITION_CUSTOM_VIEW_DOCTOR);
const COGNITION_CUSTOM_VIEW_COMPOSE_ID: ToolId = ToolId::new(COGNITION_CUSTOM_VIEW_COMPOSE);

pub fn register_custom_view_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    crate::environment_patch::register_environment_patch_tool(registry)?;
    registry.register_typed_tool(CognitionCustomViewDoctorTool::new(runtime.clone()))?;
    registry.register_typed_tool(CognitionCustomViewComposeTool::new(
        runtime, event_tx, turn_scope,
    ))?;
    Ok(())
}

pub struct CognitionCustomViewDoctorTool {
    runtime: Arc<RuntimeComposition>,
}

impl CognitionCustomViewDoctorTool {
    pub fn new(runtime: Arc<RuntimeComposition>) -> Self {
        Self { runtime }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CustomViewDoctorInput {
    /// Optional single custom surface id; omit to inspect all
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    surface_id: Option<String>,
    /// Optional presentation component id to narrow runtime diagnostics
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    component_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
    /// Optional chat session for artifact HTML resolution
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// Include MedousaStore lint and runtime log tail (default true)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
    )]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    include_runtime: Option<bool>,
    /// Lint artifact HTML for sandbox anti-patterns (default true)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
    )]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    include_static_lint: Option<bool>,
    /// Run active store self-test when Home client is open (default false)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
    )]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    probe: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct CustomViewDoctorOutput {
    #[schemars(with = "serde_json::Value")]
    status: EnvironmentStatusResponse,
}

#[medousa_tool(id = COGNITION_CUSTOM_VIEW_DOCTOR_ID)]
impl CognitionCustomViewDoctorTool {
    /// Diagnose custom environment surfaces: nav, feeds, recurring bindings, widget runtime logs, store lint, and static HTML checks.
    async fn invoke_typed(
        &self,
        input: CustomViewDoctorInput,
    ) -> stasis::prelude::Result<CustomViewDoctorOutput> {
        let profile_id = resolve_profile_id(input.profile_id.as_deref());
        let surface_filter = input
            .surface_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let component_id = input
            .component_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let include_runtime = input.include_runtime.unwrap_or(true);
        let include_static_lint = input.include_static_lint.unwrap_or(true);
        let probe = input.probe.unwrap_or(false);
        let session_id = input
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let diagnostics = DoctorDiagnosticOptions {
            component_id_filter: component_id,
            include_runtime,
            include_static_lint,
            probe,
            session_id,
        };

        let status = build_environment_status(
            environment_hub(),
            &profile_id,
            surface_filter,
            Some(self.runtime.as_ref()),
            Some(&diagnostics),
        )
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        Ok(CustomViewDoctorOutput { status })
    }
}

pub struct CognitionCustomViewComposeTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionCustomViewComposeTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct CustomViewLayoutRoot(LayoutNode);

impl JsonSchema for CustomViewLayoutRoot {
    fn schema_name() -> String {
        "CustomViewLayoutRoot".to_string()
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

fn default_compose_nav_enabled() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CustomViewNavInput {
    #[serde(default = "default_compose_nav_enabled")]
    #[schemars(default = "default_compose_nav_enabled")]
    add_to_active_preset: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CustomViewPresetRewriteInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    surfaces: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CustomViewRecurringInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    cron_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    poll_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    job_type: Option<String>,
    #[serde(default)]
    #[schemars(skip)]
    payload_template_ref: Option<String>,
    #[serde(default)]
    #[schemars(skip)]
    queue: Option<String>,
    #[serde(default, alias = "id")]
    #[schemars(skip)]
    recurring_id: Option<String>,
    #[serde(default)]
    #[schemars(skip)]
    jitter_seconds: Option<i64>,
    #[serde(default)]
    #[schemars(skip)]
    max_attempts: Option<u64>,
    #[serde(default)]
    #[schemars(skip)]
    enabled: Option<bool>,
    #[serde(default)]
    #[schemars(skip)]
    start_immediately: Option<bool>,
    #[serde(default)]
    #[schemars(skip)]
    delivery: Option<RecurringDeliverySpec>,
    #[serde(default)]
    #[schemars(skip)]
    feeds: Option<RecurringFeedSpec>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CustomViewComposeInput {
    #[schemars(required, with = "String")]
    surface_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[schemars(required, with = "String")]
    component_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    html: Option<String>,
    /// Revise-only path when html omitted
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    feed_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "CustomViewLayoutRoot", skip_serializing_if = "Option::is_none")]
    layout_root: Option<CustomViewLayoutRoot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "CustomViewRecurringInput",
        skip_serializing_if = "Option::is_none"
    )]
    recurring: Option<CustomViewRecurringInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "CustomViewNavInput", skip_serializing_if = "Option::is_none")]
    nav: Option<CustomViewNavInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "CustomViewPresetRewriteInput",
        skip_serializing_if = "Option::is_none"
    )]
    preset_rewrite: Option<CustomViewPresetRewriteInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
    #[serde(default)]
    #[schemars(skip)]
    presentation: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CustomViewComposeFailure {
    ok: bool,
    live: bool,
    pending_operator_approval: bool,
    errors: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CustomViewComposeUnsupported {
    ok: bool,
    unsupported_surface: bool,
    error: String,
    pending_operator_approval: bool,
    live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    nav_visible: Option<bool>,
    feeds_subscribed: Vec<String>,
    feeds_bound_recurring: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_run_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "serde_json::Value")]
    doctor: Option<EnvironmentStatusResponse>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CustomViewComposeUiFailure {
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
    pending_operator_approval: bool,
    live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    nav_visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
    feeds_subscribed: Vec<String>,
    feeds_bound_recurring: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_run_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "serde_json::Value")]
    doctor: Option<EnvironmentStatusResponse>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CustomViewComposeSuccess {
    ok: bool,
    live: bool,
    nav_visible: bool,
    pending_operator_approval: bool,
    feeds_subscribed: Vec<String>,
    feeds_bound_recurring: Vec<String>,
    next_run_at_utc: Option<String>,
    surface_id: String,
    component_id: String,
    #[schemars(with = "serde_json::Value")]
    doctor: EnvironmentStatusResponse,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum CustomViewComposeOutput {
    Failure(CustomViewComposeFailure),
    Unsupported(CustomViewComposeUnsupported),
    UiFailure(CustomViewComposeUiFailure),
    Success(CustomViewComposeSuccess),
}

struct ComposeStatusAugment {
    pending_operator_approval: bool,
    live: bool,
    nav_visible: Option<bool>,
    feeds_subscribed: Vec<String>,
    feeds_bound_recurring: Vec<String>,
    next_run_at_utc: Option<String>,
    doctor: Option<EnvironmentStatusResponse>,
}

#[medousa_tool(id = COGNITION_CUSTOM_VIEW_COMPOSE_ID)]
impl CognitionCustomViewComposeTool {
    /// Orchestrate a custom view: surface + HTML component + feed subscribe + layout + recurring poll in one call.
    async fn invoke_typed(
        &self,
        input: CustomViewComposeInput,
    ) -> stasis::prelude::Result<CustomViewComposeOutput> {
        let profile_id = resolve_profile_id(input.profile_id.as_deref());
        let surface_id = required_compose_string(input.surface_id, "surface_id")?;
        let component_id = required_compose_string(input.component_id, "component_id")?;
        let html = input
            .html
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let artifact_id = input
            .artifact_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if html.is_none() && artifact_id.is_none() {
            return Err(StasisError::PortFailure(
                "cognition_custom_view_compose: html or artifact_id is required".to_string(),
            ));
        }

        let feed_ids = input
            .feed_ids
            .unwrap_or_default()
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        for feed_id in &feed_ids {
            if !is_valid_feed_id(feed_id) {
                return Err(StasisError::PortFailure(format!(
                    "cognition_custom_view_compose: invalid feed_id '{feed_id}'"
                )));
            }
        }

        let add_to_preset = input
            .nav
            .as_ref()
            .map(|nav| nav.add_to_active_preset)
            .unwrap_or(true);

        let mut pending_operator_approval = false;
        let mut patch_ops: Vec<EnvironmentPatchOp> = Vec::new();

        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let surface_exists = record
            .spec
            .surfaces
            .iter()
            .any(|surface| surface.id == surface_id);

        if !surface_exists {
            let label = input.label.as_deref().unwrap_or(&surface_id).to_string();
            let icon = input.icon.as_deref().unwrap_or("layout-grid").to_string();
            patch_ops.push(EnvironmentPatchOp::AddCustomSurface {
                id: surface_id.clone(),
                label,
                icon,
                layout: Some(SurfaceLayout::Dashboard),
                add_to_active_preset: add_to_preset,
            });
        } else if add_to_preset && !surface_nav_visible(&record.spec, &surface_id) {
            patch_ops.push(EnvironmentPatchOp::AddToActivePreset {
                surface_id: surface_id.clone(),
            });
        }

        if let Some(rewrite) = input.preset_rewrite.as_ref() {
            patch_ops.push(EnvironmentPatchOp::RewriteActivePresetSurfaces {
                surfaces: rewrite.surfaces.clone().unwrap_or_default(),
            });
        }

        if !patch_ops.is_empty() {
            let patch_result =
                execute_environment_patch(environment_hub(), &profile_id, &patch_ops, "agent")
                    .await
                    .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            if patch_result.pending_operator_approval {
                pending_operator_approval = true;
            }
            if !patch_result.ok {
                return Ok(CustomViewComposeOutput::Failure(CustomViewComposeFailure {
                    ok: false,
                    live: false,
                    pending_operator_approval,
                    errors: patch_result.errors,
                }));
            }
        }

        let mut feeds_subscribed: Vec<String> = Vec::new();
        let mut feeds_bound_recurring: Vec<String> = Vec::new();
        let mut next_run_at_utc: Option<String> = None;

        if let Some(html) = html {
            let ui_input = UiPresentInput {
                title: Some(input.title.clone().unwrap_or_else(|| surface_id.clone())),
                html: Some(html),
                presentation: input.presentation.clone(),
                height: None,
                persist: Some(true),
                component_id: Some(component_id.clone()),
                surface_id: Some(surface_id.clone()),
                slot: Some("main".to_string()),
            };
            let ui_tool = CognitionUiPresentTool::new(self.turn_scope.clone());
            let ui_result = ui_tool.invoke_typed(ui_input).await?;
            let ui_failed = match &ui_result {
                UiPresentOutput::Unsupported { .. } => true,
                UiPresentOutput::Presented { ok, .. } => !ok,
            };
            if ui_failed {
                return merge_compose_status(
                    ui_result,
                    pending_operator_approval,
                    &surface_id,
                    &profile_id,
                    &feeds_subscribed,
                    &feeds_bound_recurring,
                    next_run_at_utc.as_deref(),
                    self.runtime.as_ref(),
                )
                .await;
            }
        } else if let Some(artifact_id) = artifact_id {
            let label = input
                .title
                .as_deref()
                .or(input.label.as_deref())
                .unwrap_or(&component_id)
                .to_string();
            let component =
                make_presentation_component(&component_id, &surface_id, &artifact_id, &label);
            let mut env_record = environment_hub()
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
                env_record.spec.components.push(component);
            }
            let errors = validate_environment_spec(&env_record.spec);
            if !errors.is_empty() {
                return Ok(CustomViewComposeOutput::Failure(CustomViewComposeFailure {
                    ok: false,
                    live: false,
                    pending_operator_approval,
                    errors,
                }));
            }
            environment_hub()
                .put(env_record.spec, "agent")
                .await
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        }

        if !feed_ids.is_empty() {
            feeds_subscribed =
                subscribe_component_feeds(&profile_id, &component_id, &feed_ids).await?;
        }

        if let Some(layout_root) = input.layout_root {
            apply_layout_root(&profile_id, &surface_id, layout_root.0).await?;
        }

        if let Some(recurring) = input.recurring
            && !feed_ids.is_empty()
        {
            let CustomViewRecurringInput {
                cron_expr,
                timezone,
                source,
                poll_url,
                job_type,
                payload_template_ref,
                queue,
                recurring_id,
                jitter_seconds,
                max_attempts,
                enabled,
                start_immediately,
                delivery,
                feeds,
            } = recurring;
            let source = source.or_else(|| {
                poll_url
                    .map(|poll_url| format!("http_poll url=\"{}\"", poll_url.replace('"', "\\\"")))
            });
            let mut recurring_input = if job_type
                .as_deref()
                .is_some_and(|value| value != "workflow.grapheme.run")
            {
                RuntimeRecurringRegisterInput::job(
                    job_type.clone().unwrap_or_default(),
                    payload_template_ref.clone().unwrap_or_default(),
                    cron_expr.clone().unwrap_or_default(),
                )
            } else {
                RuntimeRecurringRegisterInput::grapheme(
                    source.clone().unwrap_or_default(),
                    cron_expr.clone().unwrap_or_default(),
                )
            };
            recurring_input.source = source;
            recurring_input.job_type = job_type;
            recurring_input.payload_template_ref = payload_template_ref;
            recurring_input.cron_expr = cron_expr;
            recurring_input.timezone = timezone;
            recurring_input.queue = queue;
            recurring_input.recurring_id =
                recurring_id.or_else(|| Some(format!("{surface_id}-{component_id}")));
            recurring_input.jitter_seconds = jitter_seconds;
            recurring_input.max_attempts = max_attempts;
            recurring_input.enabled = enabled;
            recurring_input.start_immediately = start_immediately;
            recurring_input.delivery = delivery;
            recurring_input.feeds = Some(feeds.unwrap_or(RecurringFeedSpec {
                feed_ids: feed_ids.clone(),
                payload_mode: Default::default(),
            }));
            let register_tool = CognitionRuntimeRecurringRegisterTool::new(
                self.runtime.clone(),
                self.event_tx.clone(),
                self.turn_scope.clone(),
            );
            let register_result = register_tool.invoke_typed(recurring_input).await?;
            if let RuntimeRecurringRegisterOutput::Registered {
                feeds_bound,
                feeds_bound_recurring: bound_ids,
                next_run_at_utc: next,
                ..
            } = register_result
            {
                feeds_bound_recurring = if feeds_bound { bound_ids } else { Vec::new() };
                next_run_at_utc = Some(next);
            }
        }

        let nav_visible = environment_hub()
            .get(&profile_id)
            .await
            .map(|record| surface_nav_visible(&record.spec, &surface_id))
            .unwrap_or(false);

        let doctor = build_environment_status(
            environment_hub(),
            &profile_id,
            Some(&surface_id),
            Some(self.runtime.as_ref()),
            None,
        )
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        Ok(CustomViewComposeOutput::Success(CustomViewComposeSuccess {
            ok: true,
            live: !pending_operator_approval,
            nav_visible,
            pending_operator_approval,
            feeds_subscribed,
            feeds_bound_recurring,
            next_run_at_utc,
            surface_id,
            component_id,
            doctor,
        }))
    }
}

#[allow(clippy::too_many_arguments)]
async fn merge_compose_status(
    base: UiPresentOutput,
    pending_operator_approval: bool,
    surface_id: &str,
    profile_id: &str,
    feeds_subscribed: &[String],
    feeds_bound_recurring: &[String],
    next_run_at_utc: Option<&str>,
    runtime: &RuntimeComposition,
) -> StasisResult<CustomViewComposeOutput> {
    let augment = ComposeStatusAugment {
        pending_operator_approval,
        live: !pending_operator_approval,
        nav_visible: environment_hub()
            .get(profile_id)
            .await
            .ok()
            .map(|record| surface_nav_visible(&record.spec, surface_id)),
        feeds_subscribed: feeds_subscribed.to_vec(),
        feeds_bound_recurring: feeds_bound_recurring.to_vec(),
        next_run_at_utc: next_run_at_utc.map(str::to_string),
        doctor: build_environment_status(
            environment_hub(),
            profile_id,
            Some(surface_id),
            Some(runtime),
            None,
        )
        .await
        .ok(),
    };

    Ok(match base {
        UiPresentOutput::Unsupported {
            ok,
            unsupported_surface,
            error,
        } => CustomViewComposeOutput::Unsupported(CustomViewComposeUnsupported {
            ok,
            unsupported_surface,
            error,
            pending_operator_approval: augment.pending_operator_approval,
            live: augment.live,
            nav_visible: augment.nav_visible,
            feeds_subscribed: augment.feeds_subscribed,
            feeds_bound_recurring: augment.feeds_bound_recurring,
            next_run_at_utc: augment.next_run_at_utc,
            doctor: augment.doctor,
        }),
        UiPresentOutput::Presented {
            ok,
            artifact_id,
            label,
            mime,
            presentation,
            height_px,
            byte_size,
            persisted,
            errors,
            persisted_component_id,
            environment_revision,
            live: _,
            nav_visible,
            hint,
        } => CustomViewComposeOutput::UiFailure(CustomViewComposeUiFailure {
            ok,
            artifact_id,
            label,
            mime,
            presentation,
            height_px,
            byte_size,
            persisted,
            errors,
            persisted_component_id,
            environment_revision,
            pending_operator_approval: augment.pending_operator_approval,
            live: augment.live,
            nav_visible: augment.nav_visible.or(nav_visible),
            hint,
            feeds_subscribed: augment.feeds_subscribed,
            feeds_bound_recurring: augment.feeds_bound_recurring,
            next_run_at_utc: augment.next_run_at_utc,
            doctor: augment.doctor,
        }),
    })
}

fn required_compose_string(input: Option<String>, key: &str) -> StasisResult<String> {
    input
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            StasisError::PortFailure(format!("cognition_custom_view_compose: {key} is required"))
        })
}

async fn subscribe_component_feeds(
    profile_id: &str,
    component_id: &str,
    feed_ids: &[String],
) -> StasisResult<Vec<String>> {
    let mut record = environment_hub()
        .get(profile_id)
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
    let Some(index) = record
        .spec
        .components
        .iter()
        .position(|component| component.id == component_id)
    else {
        return Err(StasisError::PortFailure(format!(
            "cognition_custom_view_compose: component not found: {component_id}"
        )));
    };
    let surface_id = record.spec.components[index].surface_id.clone();
    let surface_kind = record
        .spec
        .surfaces
        .iter()
        .find(|surface| surface.id == surface_id)
        .map(|surface| surface.kind.clone());
    if surface_kind != Some(SurfaceKind::Custom) {
        return Err(StasisError::PortFailure(format!(
            "cognition_custom_view_compose: component '{component_id}' must be on a custom surface"
        )));
    }
    record.spec.components[index].feeds = feed_ids.to_vec();
    record.spec.components[index].updated_at = Some(Utc::now());
    let errors = validate_environment_spec(&record.spec);
    if !errors.is_empty() {
        return Err(StasisError::PortFailure(errors.join("; ")));
    }
    environment_hub()
        .put(record.spec, "agent")
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
    Ok(feed_ids.to_vec())
}

async fn apply_layout_root(
    profile_id: &str,
    surface_id: &str,
    layout_root: LayoutNode,
) -> StasisResult<()> {
    let mut record = environment_hub()
        .get(profile_id)
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
    let Some(index) = record
        .spec
        .surfaces
        .iter()
        .position(|entry| entry.id == surface_id)
    else {
        return Err(StasisError::PortFailure(format!(
            "cognition_custom_view_compose: unknown surface '{surface_id}'"
        )));
    };
    if record.spec.surfaces[index].kind != SurfaceKind::Custom {
        return Err(StasisError::PortFailure(format!(
            "cognition_custom_view_compose: surface '{surface_id}' is not custom"
        )));
    }
    record.spec.surfaces[index].layout_root = Some(layout_root);
    let errors = validate_environment_spec(&record.spec);
    if !errors.is_empty() {
        return Err(StasisError::PortFailure(errors.join("; ")));
    }
    environment_hub()
        .put(record.spec, "agent")
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_patch::patch_requires_proposal;
    use medousa_types::environment_default::default_environment_spec;

    #[test]
    fn patch_requires_proposal_when_preset_rewrite_in_compose_ops() {
        let ops = vec![EnvironmentPatchOp::RewriteActivePresetSurfaces {
            surfaces: vec!["chat".to_string()],
        }];
        assert!(patch_requires_proposal(&ops));
    }

    #[test]
    fn surface_nav_visible_after_add_custom_surface() {
        let mut spec = default_environment_spec("default");
        let ops = vec![EnvironmentPatchOp::AddCustomSurface {
            id: "trip-london".to_string(),
            label: "Trip".to_string(),
            icon: "train-front".to_string(),
            layout: Some(SurfaceLayout::Dashboard),
            add_to_active_preset: true,
        }];
        crate::environment_patch::apply_patch_ops(&mut spec, &ops).expect("patch");
        assert!(surface_nav_visible(&spec, "trip-london"));
    }
}
