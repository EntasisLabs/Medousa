//! `cognition_browser_act` — click/type automation on the shared human webview via Agent Browser.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stasis::domain::errors::StasisError;
use tokio::sync::mpsc;

use medousa_browser_bridge::BrowserObservation;

use crate::browser_host_client::{browser_host_act, browser_host_current_context};
use crate::browser_search::{client_executed, surface_from_scope};
use crate::browser_sessions::{
    BrowserSessionCreateRequest, BrowserSessionStatus, attach_browser_act_request,
    create_browser_session, get_browser_session,
};
use crate::browser_tools::{COGNITION_BROWSER_ACT, surface_supports_browser_host};
use crate::events::TuiEvent;
use crate::semantic_values::TrimmedText;
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{CompatOption, ExternalJson, ToolId, medousa_tool};

const ACT_ACTIONS: &[&str] = &["click", "type", "press", "scroll", "select", "wait"];
const HIGH_RISK_ACTIONS: &[&str] = &["click", "select"];
const CLIENT_ACT_WAIT_SECS: u64 = 120;
const CLIENT_ACT_POLL_MS: u64 = 500;
const COGNITION_BROWSER_ACT_ID: ToolId = ToolId::new(COGNITION_BROWSER_ACT);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserActAction {
    Click,
    Type,
    Press,
    Scroll,
    Select,
    Wait,
}

impl BrowserActAction {
    fn parse(value: Option<&str>) -> stasis::prelude::Result<Self> {
        let action = value.map(str::trim).filter(|value| !value.is_empty());
        match action {
            Some("click") => Ok(Self::Click),
            Some("type") => Ok(Self::Type),
            Some("press") => Ok(Self::Press),
            Some("scroll") => Ok(Self::Scroll),
            Some("select") => Ok(Self::Select),
            Some("wait") => Ok(Self::Wait),
            Some(action) => Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: unsupported action '{action}' (expected one of {ACT_ACTIONS:?})"
            ))),
            None => Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: action is required"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::Type => "type",
            Self::Press => "press",
            Self::Scroll => "scroll",
            Self::Select => "select",
            Self::Wait => "wait",
        }
    }

    fn needs_selector(self) -> bool {
        matches!(self, Self::Click | Self::Type | Self::Press | Self::Select)
    }

    fn world_effect_class(self) -> medousa_world::WorldEffectClass {
        match self {
            Self::Scroll | Self::Wait => medousa_world::WorldEffectClass::LocalReversible,
            Self::Click | Self::Type | Self::Press | Self::Select => {
                medousa_world::WorldEffectClass::LocalMutation
            }
        }
    }
}

pub struct CognitionBrowserActTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionBrowserActTool {
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

fn target_is_high_risk(action: &str, selector: Option<&str>) -> bool {
    if !HIGH_RISK_ACTIONS.contains(&action) {
        return false;
    }
    let Some(selector) = selector else {
        return false;
    };
    let selector = selector.to_lowercase();
    selector.contains("password")
        || selector.contains("type=submit")
        || selector.contains("submit")
        || selector.contains("checkout")
        || selector.contains("purchase")
        || selector.contains("delete")
}

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum BrowserActActionSchema {
    Click,
    Type,
    Press,
    Scroll,
    Select,
    Wait,
}

fn default_browser_act_wait_ms() -> i64 {
    1000
}

fn default_browser_act_allow_high_risk() -> bool {
    false
}

#[derive(Debug, JsonSchema)]
pub struct BrowserActInput {
    /// Interaction to perform
    #[schemars(with = "Option<BrowserActActionSchema>")]
    action: CompatOption<String>,
    /// Guarded actions to execute locally in order (maximum 16); use instead of action
    #[serde(default)]
    #[schemars(
        with = "Option<Vec<BrowserActStepInput>>",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    actions: CompatOption<Vec<BrowserActStepInput>>,
    /// Opaque element ref from cognition_browser_snapshot (preferred over CSS)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    target_ref: CompatOption<String>,
    /// CSS selector of the target element (required for click/type/press/select)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    selector: CompatOption<String>,
    /// Observation revision that produced target_ref
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    observation_revision: CompatOption<u64>,
    /// Observation document id that produced target_ref
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    document_id: CompatOption<String>,
    /// Optional semantic preconditions checked immediately before the action
    #[serde(default)]
    #[schemars(
        with = "Option<BrowserActGuardInput>",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    guard: CompatOption<BrowserActGuardInput>,
    /// Text to type (action=type)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    text: CompatOption<String>,
    /// Key name such as Enter/Tab/Escape (action=press)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    key: CompatOption<String>,
    /// Vertical scroll delta in px (action=scroll; positive = down)
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    delta_y: CompatOption<i64>,
    /// Option value to choose (action=select)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    value: CompatOption<String>,
    /// Wait duration in milliseconds (action=wait)
    #[schemars(with = "i64", default = "default_browser_act_wait_ms")]
    ms: CompatOption<i64>,
    /// Set true to act on submit/password/checkout-like targets
    #[schemars(with = "bool", default = "default_browser_act_allow_high_risk")]
    allow_high_risk: CompatOption<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct BrowserActGuardInput {
    #[serde(default)]
    role: CompatOption<String>,
    #[serde(default)]
    name: CompatOption<String>,
    #[serde(default)]
    value: CompatOption<String>,
}

impl BrowserActGuardInput {
    fn is_empty(&self) -> bool {
        self.role.is_none() && self.name.is_none() && self.value.is_none()
    }
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct BrowserActStepInput {
    #[serde(default)]
    #[schemars(required, with = "BrowserActActionSchema")]
    action: CompatOption<String>,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    target_ref: CompatOption<String>,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    selector: CompatOption<String>,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    text: CompatOption<String>,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    key: CompatOption<String>,
    #[serde(default)]
    #[schemars(with = "Option<i64>")]
    delta_y: CompatOption<i64>,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    value: CompatOption<String>,
    #[serde(default)]
    #[schemars(with = "Option<i64>")]
    ms: CompatOption<i64>,
    #[serde(default)]
    #[schemars(with = "Option<BrowserActGuardInput>")]
    guard: CompatOption<BrowserActGuardInput>,
}

impl<'de> Deserialize<'de> for BrowserActInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            action: CompatOption<String>,
            #[serde(default)]
            actions: CompatOption<Vec<BrowserActStepInput>>,
            #[serde(default)]
            target_ref: CompatOption<String>,
            #[serde(default)]
            selector: CompatOption<String>,
            #[serde(default)]
            observation_revision: CompatOption<u64>,
            #[serde(default)]
            document_id: CompatOption<String>,
            #[serde(default)]
            guard: CompatOption<BrowserActGuardInput>,
            #[serde(default)]
            text: CompatOption<String>,
            #[serde(default)]
            key: CompatOption<String>,
            #[serde(default)]
            delta_y: CompatOption<i64>,
            #[serde(default)]
            value: CompatOption<String>,
            #[serde(default)]
            ms: CompatOption<i64>,
            #[serde(default)]
            allow_high_risk: CompatOption<bool>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            action: input.action,
            actions: input.actions,
            target_ref: input.target_ref,
            selector: input.selector,
            observation_revision: input.observation_revision,
            document_id: input.document_id,
            guard: input
                .guard
                .into_option()
                .filter(|guard| !guard.is_empty())
                .into(),
            text: input.text,
            key: input.key,
            delta_y: input.delta_y,
            value: input.value,
            ms: input.ms,
            allow_high_risk: input.allow_high_risk,
        })
    }
}

#[derive(Debug)]
struct BrowserActCommand {
    action: BrowserActAction,
    target_ref: Option<TrimmedText>,
    selector: Option<TrimmedText>,
    guard: Option<BrowserActGuardInput>,
    text: Option<String>,
    key: Option<String>,
    delta_y: Option<i64>,
    value: Option<String>,
    ms: Option<i64>,
}

impl TryFrom<BrowserActStepInput> for BrowserActCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: BrowserActStepInput) -> Result<Self, Self::Error> {
        let action = BrowserActAction::parse(input.action.into_option().as_deref())?;
        let selector_value = input.selector.into_option();
        let selector = selector_value
            .as_deref()
            .and_then(|value| TrimmedText::new(value).ok());
        let target_ref_value = input.target_ref.into_option();
        let target_ref = target_ref_value
            .as_deref()
            .and_then(|value| TrimmedText::new(value).ok());
        if selector.is_some() && target_ref.is_some() {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: use target_ref or selector, not both"
            )));
        }
        if action.needs_selector() && selector.is_none() && target_ref.is_none() {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: target_ref or selector is required for action '{}'",
                action.as_str()
            )));
        }
        Ok(Self {
            action,
            target_ref,
            selector,
            guard: input
                .guard
                .into_option()
                .filter(|guard| !guard.is_empty()),
            text: input.text.into_option(),
            key: input.key.into_option(),
            delta_y: input.delta_y.into_option(),
            value: input.value.into_option(),
            ms: input.ms.into_option(),
        })
    }
}

#[derive(Debug)]
struct BrowserActInvocation {
    steps: Vec<BrowserActCommand>,
    is_batch: bool,
    observation_revision: Option<u64>,
    document_id: Option<TrimmedText>,
    allow_high_risk: bool,
}

impl BrowserActInvocation {
    fn world_effect_class(&self) -> medousa_world::WorldEffectClass {
        if self.steps.iter().any(|step| {
            step.action.world_effect_class() == medousa_world::WorldEffectClass::LocalMutation
        }) {
            medousa_world::WorldEffectClass::LocalMutation
        } else {
            medousa_world::WorldEffectClass::LocalReversible
        }
    }
}

impl TryFrom<BrowserActInput> for BrowserActInvocation {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: BrowserActInput) -> Result<Self, Self::Error> {
        const MAX_BATCH_STEPS: usize = 16;
        const MAX_BATCH_WAIT_MS: i64 = 5_000;
        let BrowserActInput {
            action,
            actions,
            target_ref,
            selector,
            observation_revision,
            document_id,
            guard,
            text,
            key,
            delta_y,
            value,
            ms,
            allow_high_risk,
        } = input;
        let batch = actions.into_option();
        let is_batch = batch.is_some();
        let steps = if let Some(steps) = batch {
            if !action.is_none()
                || !target_ref.is_none()
                || !selector.is_none()
                || !guard.is_none()
                || !text.is_none()
                || !key.is_none()
                || !delta_y.is_none()
                || !value.is_none()
                || !ms.is_none()
            {
                return Err(StasisError::PortFailure(format!(
                    "{COGNITION_BROWSER_ACT}: actions cannot be combined with singular action fields"
                )));
            }
            if steps.is_empty() || steps.len() > MAX_BATCH_STEPS {
                return Err(StasisError::PortFailure(format!(
                    "{COGNITION_BROWSER_ACT}: actions must contain 1 to {MAX_BATCH_STEPS} steps"
                )));
            }
            steps
                .into_iter()
                .map(BrowserActCommand::try_from)
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![BrowserActCommand::try_from(BrowserActStepInput {
                action,
                target_ref,
                selector,
                text,
                key,
                delta_y,
                value,
                ms,
                guard,
            })?]
        };
        let total_wait_ms = steps
            .iter()
            .filter(|step| step.action == BrowserActAction::Wait)
            .map(|step| step.ms.unwrap_or(1_000).max(0))
            .sum::<i64>();
        if total_wait_ms > MAX_BATCH_WAIT_MS {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: batch waits exceed {MAX_BATCH_WAIT_MS} ms"
            )));
        }

        let document_id_value = document_id.into_option();
        let document_id = document_id_value
            .as_deref()
            .and_then(|value| TrimmedText::new(value).ok());
        let observation_revision = observation_revision.into_option();
        if steps.iter().any(|step| step.target_ref.is_some())
            && (document_id.is_none() || observation_revision.is_none())
        {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: target_ref requires document_id and observation_revision"
            )));
        }
        Ok(Self {
            steps,
            is_batch,
            observation_revision,
            document_id,
            allow_high_risk: allow_high_risk.into_option().unwrap_or(false),
        })
    }
}

fn browser_step_body(command: BrowserActCommand) -> stasis::prelude::Result<Value> {
    let mut body = json!({ "action": command.action.as_str() });
    if let Some(target_ref) = command.target_ref {
        body["target_ref"] = json!(target_ref.into_string());
    }
    if let Some(selector) = command.selector {
        body["selector"] = json!(selector.into_string());
    }
    if let Some(guard) = command.guard {
        body["guard"] = serde_json::to_value(guard).map_err(|error| {
            StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: could not encode semantic guard: {error}"
            ))
        })?;
    }
    if let Some(value) = command.text {
        body["text"] = json!(value);
    }
    if let Some(value) = command.key {
        body["key"] = json!(value);
    }
    if let Some(value) = command.value {
        body["value"] = json!(value);
    }
    if let Some(value) = command.delta_y {
        body["delta_y"] = json!(value);
    }
    if let Some(value) = command.ms {
        body["ms"] = json!(value);
    }
    Ok(body)
}

#[medousa_tool(id = COGNITION_BROWSER_ACT_ID)]
impl CognitionBrowserActTool {
    /// Act on the shared Web tab using opaque refs from cognition_browser_snapshot. A guarded actions batch executes locally in order and stops at the first failure.
    async fn invoke_typed(&self, input: BrowserActInput) -> stasis::prelude::Result<ExternalJson> {
        if !self.browser_enabled().await {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: requires supports_browser_host client (Home desktop/iOS)"
            )));
        }

        let invocation = BrowserActInvocation::try_from(input)?;
        if !invocation.allow_high_risk
            && invocation.steps.iter().any(|step| {
                target_is_high_risk(
                    step.action.as_str(),
                    step.selector.as_ref().map(|selector| selector.as_str()),
                )
            })
        {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: target looks high-risk (submit/password/checkout-like). \
                 Re-run with allow_high_risk=true only if the operator asked for this action."
            )));
        }
        let world_effect_class = invocation.world_effect_class();
        let is_batch = invocation.is_batch;
        let allow_high_risk = invocation.allow_high_risk;
        let observation_revision = invocation.observation_revision;
        let semantic_document_id = invocation
            .document_id
            .as_ref()
            .map(|document_id| document_id.as_str().to_string());
        let semantic_targets = invocation
            .steps
            .iter()
            .filter_map(|step| {
                step.target_ref.as_ref().map(|target_ref| {
                    (
                        step.action.as_str().to_string(),
                        target_ref.as_str().to_string(),
                    )
                })
            })
            .collect::<Vec<_>>();
        let document_id = invocation.document_id;
        let mut steps = invocation
            .steps
            .into_iter()
            .map(browser_step_body)
            .collect::<Result<Vec<_>, _>>()?;
        let mut body = if is_batch {
            json!({ "actions": steps })
        } else {
            steps.pop().expect("one validated browser action")
        };
        body["allow_high_risk"] = json!(allow_high_risk);
        if let Some(revision) = observation_revision {
            body["expected_observation_revision"] = json!(revision);
        }
        if let Some(document_id) = document_id {
            body["expected_document_id"] = json!(document_id.into_string());
        }
        let summary = if is_batch {
            format!(
                "guarded browser batch ({} steps)",
                body.get("actions")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
            )
        } else {
            let action = body
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("browser action");
            body.get("target_ref")
                .or_else(|| body.get("selector"))
                .and_then(Value::as_str)
                .map(|target| format!("{action} {target}"))
                .unwrap_or_else(|| action.to_string())
        };
        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_BROWSER_ACT.to_string(),
                input_summary: summary.clone(),
            })
            .await;

        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        if client_executed(scope.as_ref()) {
            if is_batch {
                return Err(StasisError::PortFailure(format!(
                    "{COGNITION_BROWSER_ACT}: guarded batches currently require the desktop BrowserHost"
                )));
            }
            return self
                .invoke_client_executed(body, &scope)
                .await
                .map(ExternalJson::new);
        }

        let browser_context = browser_host_current_context()
            .await
            .map_err(StasisError::PortFailure)?;
        if browser_context.control != "agent" {
            let (code, error) = if browser_context.control == "awaiting_operator" {
                (
                    "awaiting_operator",
                    "tab is awaiting operator verification (CAPTCHA/login). Act is blocked until control returns to the agent.",
                )
            } else {
                (
                    "control_required",
                    "agent does not control this tab. Hand control to the agent before acting.",
                )
            };
            return Ok(ExternalJson::new(json!({
                "ok": false,
                "code": code,
                "error": error,
                "binding_used": "human_webview",
                "decision": "block",
            })));
        }

        let authority_id = crate::workshop_authority::current()
            .map_err(StasisError::PortFailure)?
            .to_string();
        if !semantic_targets.is_empty() {
            crate::world_authority::validate_browser_element_refs(
                crate::world_authority::BrowserElementRefFence {
                    authority_id: &authority_id,
                    driver_id: &browser_context.driver_id,
                    tab_group_id: &browser_context.tab_group_id,
                    tab_id: &browser_context.tab_id,
                    expected_url: &browser_context.url,
                    document_id: semantic_document_id.as_deref().unwrap_or_default(),
                    revision: observation_revision.unwrap_or_default(),
                    targets: &semantic_targets,
                    allow_high_risk,
                },
            )
            .map_err(|error| {
                StasisError::PortFailure(format!(
                    "{COGNITION_BROWSER_ACT}: semantic target denied: {error}"
                ))
            })?;
        }
        let trace_id = scope
            .as_ref()
            .map(|scope| scope.turn_correlation_id.as_str())
            .filter(|trace_id| !trace_id.trim().is_empty())
            .unwrap_or("browser-action");
        let admission = crate::world_authority::admit_browser_action(
            &authority_id,
            &browser_context.driver_id,
            &browser_context.tab_group_id,
            &browser_context.tab_id,
            trace_id,
            &summary,
            world_effect_class,
        )
        .map_err(|error| {
            StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: world admission denied: {error}"
            ))
        })?;
        body["world_permit"] = serde_json::to_value(&admission.permit).map_err(|error| {
            StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: could not encode world permit: {error}"
            ))
        })?;
        // A browser tab keeps the same id across navigation. Carry the state
        // observed at admission so the driver cannot execute this permit on a
        // different page that happened to load before dispatch.
        body["world_expected_url"] = json!(browser_context.url);

        let mut outcome = match browser_host_act(&browser_context.tab_group_id, body).await {
            Ok(outcome) => outcome,
            Err(error) => {
                let world_outcome = crate::world_authority::mark_browser_action_indeterminate(
                    &admission,
                    &format!("BrowserHost result unavailable: {error}"),
                )
                .map_err(StasisError::PortFailure)?;
                return Err(StasisError::PortFailure(format!(
                    "{error}; world action {} is indeterminate at revision {}",
                    world_outcome.intent_id, world_outcome.committed_revision
                )));
            }
        };
        mirror_browser_observation_from_outcome(&admission, &mut outcome);
        if outcome
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            let world_outcome = crate::world_authority::complete_browser_action(
                &admission,
                &format!("BrowserHost completed {summary}"),
            )
            .map_err(StasisError::PortFailure)?;
            return Ok(ExternalJson::new(with_world_provenance(
                outcome,
                admission.provenance(Some(world_outcome)),
                "allow",
            )));
        }

        let code = outcome
            .get("code")
            .and_then(|value| value.as_str())
            .unwrap_or("act_failed")
            .to_string();
        let error = outcome
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("browser act failed")
            .to_string();
        let has_applied_steps = outcome
            .get("executed_steps")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0);
        let world_outcome = if has_applied_steps
            || matches!(
                code.as_str(),
                "act_failed" | "stale_surface_lease" | "batch_partial"
            )
        {
            Some(
                crate::world_authority::mark_browser_action_indeterminate(
                    &admission,
                    &format!("BrowserHost could not prove the action outcome: {error}"),
                )
                .map_err(StasisError::PortFailure)?,
            )
        } else {
            crate::world_authority::fail_browser_action(&admission, &error)
                .map_err(StasisError::PortFailure)?;
            None
        };
        Ok(ExternalJson::new(with_world_provenance(
            outcome,
            admission.provenance(world_outcome),
            "block",
        )))
    }
}

fn mirror_browser_observation_from_outcome(
    admission: &crate::world_authority::BrowserWorldAdmission,
    outcome: &mut Value,
) {
    let Some(observation) = outcome
        .get("observation")
        .filter(|value| !value.is_null())
        .cloned()
        .and_then(|value| serde_json::from_value::<BrowserObservation>(value).ok())
    else {
        return;
    };
    let Err(error) = crate::world_authority::record_browser_observation(admission, observation)
    else {
        return;
    };
    let Some(object) = outcome.as_object_mut() else {
        return;
    };
    object.insert("observation_mirror_error".to_string(), json!(error));
}

fn with_world_provenance(
    mut value: Value,
    provenance: crate::world_authority::BrowserWorldProvenance,
    decision: &str,
) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert("decision".to_string(), json!(decision));
        object.insert("world".to_string(), json!(provenance));
    }
    value
}

impl CognitionBrowserActTool {
    /// iOS/Android: no :7422 host — hand the act to the client via a browser session and
    /// wait for Home to execute it in the overlay webview (mirrors client-executed search).
    async fn invoke_client_executed(
        &self,
        body: Value,
        scope: &Option<TurnContinuationScope>,
    ) -> stasis::prelude::Result<Value> {
        let Some(scope) = scope else {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: missing turn scope for client-executed act"
            )));
        };
        let session = create_browser_session(BrowserSessionCreateRequest {
            turn_id: scope.turn_correlation_id.clone(),
            chat_session_id: scope.session_id.clone(),
            world_driver_id: scope.browser_driver_id.clone(),
            query: String::new(),
            max_results: 0,
            client_executed: true,
        });
        let _ = attach_browser_act_request(&session.session_id, body.clone());

        if let Some(sink) = crate::engine_adapters::active_tool_sink().await {
            sink.emit(medousa_engine::ToolSinkEvent::BrowserChallenge {
                turn_correlation_id: scope.turn_correlation_id.clone(),
                session_id: session.session_id.clone(),
                challenge_url: String::new(),
                reason: "client_act".to_string(),
            })
            .await;
        }

        let deadline = std::time::Duration::from_secs(CLIENT_ACT_WAIT_SECS);
        let started = std::time::Instant::now();
        while started.elapsed() < deadline {
            if let Some(current) = get_browser_session(&session.session_id) {
                match current.status {
                    BrowserSessionStatus::Completed => {
                        let outcome = current.act_result.ok_or_else(|| {
                            StasisError::PortFailure(
                                "browser act session completed without result".to_string(),
                            )
                        })?;
                        if outcome.ok {
                            return Ok(json!({
                                "ok": true,
                                "action": body.get("action").cloned().unwrap_or(Value::Null),
                                "selector": body.get("selector").cloned().unwrap_or(Value::Null),
                                "url": outcome.url,
                                "binding_used": "human_webview",
                                "decision": "allow",
                            }));
                        }
                        return Ok(json!({
                            "ok": false,
                            "code": "act_failed",
                            "error": outcome
                                .error
                                .unwrap_or_else(|| "browser act failed".to_string()),
                            "binding_used": "human_webview",
                            "decision": "block",
                        }));
                    }
                    BrowserSessionStatus::Failed => {
                        return Err(StasisError::PortFailure(
                            current
                                .error
                                .unwrap_or_else(|| "browser act session failed".to_string()),
                        ));
                    }
                    BrowserSessionStatus::ChallengeRequired
                    | BrowserSessionStatus::PendingClient => {}
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(CLIENT_ACT_POLL_MS)).await;
        }
        Err(StasisError::PortFailure(
            "browser act timed out waiting for client".to_string(),
        ))
    }
}

pub fn register_browser_act_tool(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionBrowserActTool::new(turn_scope, event_tx))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_risk_targets_flagged() {
        assert!(target_is_high_risk("click", Some("button[type=submit]")));
        assert!(target_is_high_risk("click", Some("input.password-field")));
        assert!(target_is_high_risk("select", Some("#checkout-plan")));
        assert!(target_is_high_risk("click", Some(".delete-account")));
        assert!(!target_is_high_risk("click", Some("#search-button")));
        assert!(!target_is_high_risk("type", Some("input.password")));
        assert!(!target_is_high_risk("click", None));
    }

    #[test]
    fn browser_act_command_normalizes_controls_and_preserves_text() {
        let invocation = BrowserActInvocation::try_from(BrowserActInput {
            action: Some("  type  ".to_string()).into(),
            actions: None.into(),
            target_ref: None.into(),
            selector: Some("  #search  ".to_string()).into(),
            observation_revision: None.into(),
            document_id: None.into(),
            guard: None.into(),
            text: Some("  keep surrounding text  ".to_string()).into(),
            key: Some(" Enter ".to_string()).into(),
            delta_y: None.into(),
            value: None.into(),
            ms: None.into(),
            allow_high_risk: None.into(),
        })
        .expect("invocation");
        let command = &invocation.steps[0];

        assert_eq!(command.action, BrowserActAction::Type);
        assert_eq!(
            command.selector.as_ref().map(TrimmedText::as_str),
            Some("#search")
        );
        assert_eq!(command.text.as_deref(), Some("  keep surrounding text  "));
        assert_eq!(command.key.as_deref(), Some(" Enter "));
        assert!(!invocation.allow_high_risk);
    }

    #[test]
    fn browser_act_command_requires_selector_for_targeted_actions() {
        let error = BrowserActInvocation::try_from(BrowserActInput {
            action: Some("click".to_string()).into(),
            actions: None.into(),
            target_ref: None.into(),
            selector: None.into(),
            observation_revision: None.into(),
            document_id: None.into(),
            guard: None.into(),
            text: None.into(),
            key: None.into(),
            delta_y: None.into(),
            value: None.into(),
            ms: None.into(),
            allow_high_risk: None.into(),
        })
        .expect_err("click without selector should fail");

        assert!(
            error
                .to_string()
                .contains("target_ref or selector is required for action 'click'")
        );
    }

    #[test]
    fn browser_act_wire_optionals_remain_lenient_for_legacy_values() {
        let input: BrowserActInput = serde_json::from_value(serde_json::json!({
            "action": "click",
            "target_ref": 42,
            "selector": 42,
            "observation_revision": "1",
            "document_id": false,
            "guard": [],
            "text": false,
            "key": [],
            "delta_y": "10",
            "value": null,
            "ms": "1000",
            "allow_high_risk": "true",
        }))
        .expect("browser act input");
        assert_eq!(input.action.into_option().as_deref(), Some("click"));
        assert!(input.actions.into_option().is_none());
        assert!(input.target_ref.into_option().is_none());
        assert!(input.selector.into_option().is_none());
        assert!(input.observation_revision.into_option().is_none());
        assert!(input.document_id.into_option().is_none());
        assert!(input.guard.into_option().is_none());
        assert!(input.text.into_option().is_none());
        assert!(input.key.into_option().is_none());
        assert!(input.delta_y.into_option().is_none());
        assert!(input.value.into_option().is_none());
        assert!(input.ms.into_option().is_none());
        assert!(input.allow_high_risk.into_option().is_none());
    }

    fn batch_step(action: &str) -> BrowserActStepInput {
        BrowserActStepInput {
            action: Some(action.to_string()).into(),
            ..BrowserActStepInput::default()
        }
    }

    #[test]
    fn browser_act_batch_is_bounded_and_classifies_the_strongest_effect() {
        let invocation = BrowserActInvocation::try_from(BrowserActInput {
            action: None.into(),
            actions: Some(vec![batch_step("wait"), BrowserActStepInput {
                action: Some("click".to_string()).into(),
                target_ref: Some("el:doc:1".to_string()).into(),
                ..BrowserActStepInput::default()
            }])
            .into(),
            target_ref: None.into(),
            selector: None.into(),
            observation_revision: Some(7).into(),
            document_id: Some("doc:one".to_string()).into(),
            guard: None.into(),
            text: None.into(),
            key: None.into(),
            delta_y: None.into(),
            value: None.into(),
            ms: None.into(),
            allow_high_risk: None.into(),
        })
        .expect("batch invocation");

        assert!(invocation.is_batch);
        assert_eq!(invocation.steps.len(), 2);
        assert_eq!(
            invocation.world_effect_class(),
            medousa_world::WorldEffectClass::LocalMutation
        );
    }

    #[test]
    fn browser_act_batch_rejects_mixed_singular_fields_and_excess_waits() {
        let mixed = BrowserActInvocation::try_from(BrowserActInput {
            action: Some("wait".to_string()).into(),
            actions: Some(vec![batch_step("wait")]).into(),
            target_ref: None.into(),
            selector: None.into(),
            observation_revision: None.into(),
            document_id: None.into(),
            guard: None.into(),
            text: None.into(),
            key: None.into(),
            delta_y: None.into(),
            value: None.into(),
            ms: None.into(),
            allow_high_risk: None.into(),
        })
        .expect_err("mixed invocation must fail");
        assert!(mixed.to_string().contains("cannot be combined"));

        let waits = BrowserActInvocation::try_from(BrowserActInput {
            action: None.into(),
            actions: Some(vec![
                BrowserActStepInput {
                    action: Some("wait".to_string()).into(),
                    ms: Some(3_000).into(),
                    ..BrowserActStepInput::default()
                },
                BrowserActStepInput {
                    action: Some("wait".to_string()).into(),
                    ms: Some(3_000).into(),
                    ..BrowserActStepInput::default()
                },
            ])
            .into(),
            target_ref: None.into(),
            selector: None.into(),
            observation_revision: None.into(),
            document_id: None.into(),
            guard: None.into(),
            text: None.into(),
            key: None.into(),
            delta_y: None.into(),
            value: None.into(),
            ms: None.into(),
            allow_high_risk: None.into(),
        })
        .expect_err("excess wait budget must fail");
        assert!(waits.to_string().contains("batch waits exceed"));
    }

    #[test]
    fn browser_act_ref_requires_observation_identity() {
        let error = BrowserActInvocation::try_from(BrowserActInput {
            action: Some("click".to_string()).into(),
            actions: None.into(),
            target_ref: Some("el:doc:1".to_string()).into(),
            selector: None.into(),
            observation_revision: None.into(),
            document_id: None.into(),
            guard: None.into(),
            text: None.into(),
            key: None.into(),
            delta_y: None.into(),
            value: None.into(),
            ms: None.into(),
            allow_high_risk: None.into(),
        })
        .expect_err("unbound ref must fail");
        assert!(
            error
                .to_string()
                .contains("target_ref requires document_id and observation_revision")
        );
    }
}
