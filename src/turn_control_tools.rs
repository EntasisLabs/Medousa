//! Control-plane tools for agent turn boundaries (explicit finalize signaling).

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
#[cfg(test)]
use stasis::application::orchestration::tool_registry::StasisTool;

use crate::semantic_values::TrimmedText;
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

/// Canonical registry name (snake_case).
pub const COGNITION_TURN_PREPARE_FINAL: &str = "cognition_turn_prepare_final";

/// Dot alias accepted from models trained on dotted tool names.
pub const COGNITION_TURN_PREPARE_FINAL_DOTTED: &str = "cognition.turn.prepare_final";

/// Hard-stop: deliver final user-facing text in the tool call and end the loop immediately.
pub const COGNITION_TURN_FINISH: &str = "cognition_turn_finish";

pub const COGNITION_TURN_FINISH_DOTTED: &str = "cognition.turn.finish";

/// Hand mid-task update to the principal and end this agent turn (conversation continues on their reply).
pub const COGNITION_TURN_CHECKPOINT: &str = "cognition_turn_checkpoint";

pub const COGNITION_TURN_CHECKPOINT_DOTTED: &str = "cognition.turn.checkpoint";

pub const COGNITION_TURN_REQUEST_MORE_ROUNDS: &str = "cognition_turn_request_more_rounds";

pub const COGNITION_TURN_REQUEST_MORE_ROUNDS_DOTTED: &str = "cognition.turn.request_more_rounds";

/// Signal tool-loop entry with a principal-facing progress message (does not end the turn).
pub const COGNITION_TURN_BEGIN_WORK: &str = "cognition_turn_begin_work";

pub const COGNITION_TURN_BEGIN_WORK_DOTTED: &str = "cognition.turn.begin_work";

/// Short principal-facing status while the turn continues (retries, course-corrections, light updates).
pub const COGNITION_TURN_UPDATE_USER: &str = "cognition_turn_update_user";

pub const COGNITION_TURN_UPDATE_USER_DOTTED: &str = "cognition.turn.update_user";

/// Propose a mode transition for the current chat. Runtime policy decides whether it waits.
pub const COGNITION_TURN_PROPOSE_MODE: &str = "cognition_turn_propose_mode";

pub const COGNITION_TURN_PROPOSE_MODE_DOTTED: &str = "cognition.turn.propose_mode";

const COGNITION_TURN_UPDATE_USER_ID: ToolId = ToolId::new(COGNITION_TURN_UPDATE_USER);
const COGNITION_TURN_BEGIN_WORK_ID: ToolId = ToolId::new(COGNITION_TURN_BEGIN_WORK);
const COGNITION_TURN_PROPOSE_MODE_ID: ToolId = ToolId::new(COGNITION_TURN_PROPOSE_MODE);
const COGNITION_TURN_PREPARE_FINAL_ID: ToolId = ToolId::new(COGNITION_TURN_PREPARE_FINAL);
const COGNITION_TURN_FINISH_ID: ToolId = ToolId::new(COGNITION_TURN_FINISH);
const COGNITION_TURN_CHECKPOINT_ID: ToolId = ToolId::new(COGNITION_TURN_CHECKPOINT);
const COGNITION_TURN_REQUEST_MORE_ROUNDS_ID: ToolId =
    ToolId::new(COGNITION_TURN_REQUEST_MORE_ROUNDS);

pub struct RequestMoreRoundsPayload {
    pub requested_rounds: usize,
    pub reason: String,
    pub progress_summary: Option<String>,
}

pub fn is_prepare_final_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_PREPARE_FINAL
        || trimmed == COGNITION_TURN_PREPARE_FINAL_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed)
            == COGNITION_TURN_PREPARE_FINAL
}

pub fn is_finish_turn_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_FINISH
        || trimmed == COGNITION_TURN_FINISH_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed) == COGNITION_TURN_FINISH
}

pub fn is_checkpoint_turn_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_CHECKPOINT
        || trimmed == COGNITION_TURN_CHECKPOINT_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed) == COGNITION_TURN_CHECKPOINT
}

pub fn is_request_more_rounds_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_REQUEST_MORE_ROUNDS
        || trimmed == COGNITION_TURN_REQUEST_MORE_ROUNDS_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed)
            == COGNITION_TURN_REQUEST_MORE_ROUNDS
}

pub fn is_begin_work_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_BEGIN_WORK
        || trimmed == COGNITION_TURN_BEGIN_WORK_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed) == COGNITION_TURN_BEGIN_WORK
}

pub fn is_update_user_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_UPDATE_USER
        || trimmed == COGNITION_TURN_UPDATE_USER_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed) == COGNITION_TURN_UPDATE_USER
}

pub fn is_propose_mode_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_PROPOSE_MODE
        || trimmed == COGNITION_TURN_PROPOSE_MODE_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed)
            == COGNITION_TURN_PROPOSE_MODE
}

/// Extract the latest successful user-update line from a tool batch.
pub fn update_user_message_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_update_user_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(message) = message_from_turn_control_message_payload(&inv.tool_input) {
            return Some(message);
        }
        if let Some(message) = message_from_turn_control_message_payload(&inv.tool_output) {
            return Some(message);
        }
    }
    None
}

/// Latest principal-visible progress line from update_user or begin_work in this batch.
pub fn turn_progress_message_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    update_user_message_from_invocations(invocations)
        .or_else(|| begin_work_message_from_invocations(invocations))
}

/// Extract the latest successful begin-work progress message from a tool batch.
pub fn begin_work_message_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_begin_work_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(message) = message_from_begin_work_payload(&inv.tool_input) {
            return Some(message);
        }
        if let Some(message) = message_from_begin_work_payload(&inv.tool_output) {
            return Some(message);
        }
    }
    None
}

fn note_from_begin_work_payload(payload: &Value) -> Option<String> {
    payload
        .get("note")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Extract the latest successful begin-work sticky note from a tool batch.
pub fn begin_work_note_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_begin_work_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(note) = note_from_begin_work_payload(&inv.tool_input) {
            return Some(note);
        }
        if let Some(note) = note_from_begin_work_payload(&inv.tool_output) {
            return Some(note);
        }
    }
    None
}

fn message_from_begin_work_payload(payload: &Value) -> Option<String> {
    payload
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Extract the operator-facing final message from a tool batch, if `cognition_turn_finish` ran.
pub fn finish_turn_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_finish_turn_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(message) = message_from_finish_turn_payload(&inv.tool_input) {
            return Some(message);
        }
        if let Some(message) = message_from_finish_turn_payload(&inv.tool_output) {
            return Some(message);
        }
    }
    None
}

/// Preserve the model's terminal prose exactly. Completion policy is based on
/// response/tool events, never a semantic classification of this text.
pub fn terminal_text_for_fsm_end(_termination_reason: &str, draft_text: String) -> String {
    draft_text
}

pub fn request_more_rounds_from_invocations(
    invocations: &[ToolInvocation],
) -> Option<RequestMoreRoundsPayload> {
    for inv in invocations.iter().rev() {
        if !is_request_more_rounds_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        let requested_rounds = inv
            .tool_input
            .get("requested_rounds")
            .or_else(|| inv.tool_output.get("requested_rounds"))
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(1)
            .clamp(1, crate::turn_budget_request::MAX_REQUESTED_ROUNDS_PER_ASK);
        let reason = inv
            .tool_input
            .get("reason")
            .or_else(|| inv.tool_output.get("reason"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)?;
        let progress_summary = inv
            .tool_input
            .get("progress_summary")
            .or_else(|| inv.tool_output.get("progress_summary"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        return Some(RequestMoreRoundsPayload {
            requested_rounds,
            reason,
            progress_summary,
        });
    }
    None
}

fn message_from_finish_turn_payload(payload: &Value) -> Option<String> {
    message_from_turn_control_message_payload(payload)
}

fn message_from_turn_control_message_payload(payload: &Value) -> Option<String> {
    payload
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_trimmed(value: Option<String>) -> Option<TrimmedText> {
    value.and_then(|value| TrimmedText::new(value).ok())
}

#[derive(Debug)]
struct TurnBeginWorkCommand {
    message: Option<TrimmedText>,
    goal: Option<TrimmedText>,
    intent: crate::agent_runtime::turn_worker::TurnWorkerIntent,
}

impl From<TurnBeginWorkInput> for TurnBeginWorkCommand {
    fn from(input: TurnBeginWorkInput) -> Self {
        let intent = input
            .intent
            .as_deref()
            .and_then(crate::agent_runtime::turn_worker::TurnWorkerIntent::parse)
            .unwrap_or(crate::agent_runtime::turn_worker::TurnWorkerIntent::General);
        Self {
            message: optional_trimmed(input.message),
            goal: optional_trimmed(input.goal),
            intent,
        }
    }
}

#[derive(Debug)]
struct TurnUpdateUserCommand {
    message: Option<TrimmedText>,
}

impl From<TurnUpdateUserInput> for TurnUpdateUserCommand {
    fn from(input: TurnUpdateUserInput) -> Self {
        Self {
            message: optional_trimmed(input.message),
        }
    }
}

#[derive(Debug)]
struct TurnProposeModeCommand {
    mode: crate::daemon_api::AgentModeId,
    scope: crate::daemon_api::AgentModeScope,
    task_id: Option<TrimmedText>,
    reason: Option<TrimmedText>,
}

impl From<TurnProposeModeInput> for TurnProposeModeCommand {
    fn from(input: TurnProposeModeInput) -> Self {
        Self {
            mode: input.mode.into(),
            scope: input.scope.into(),
            task_id: optional_trimmed(input.task_id),
            reason: TrimmedText::new(input.reason).ok(),
        }
    }
}

#[derive(Debug)]
struct TurnPrepareFinalCommand {
    reason: Option<TrimmedText>,
}

impl From<TurnPrepareFinalInput> for TurnPrepareFinalCommand {
    fn from(input: TurnPrepareFinalInput) -> Self {
        Self {
            reason: optional_trimmed(input.reason.into_option()),
        }
    }
}

#[derive(Debug)]
struct TurnFinishCommand {
    message: Option<TrimmedText>,
    reason: Option<TrimmedText>,
}

impl From<TurnFinishInput> for TurnFinishCommand {
    fn from(input: TurnFinishInput) -> Self {
        Self {
            message: optional_trimmed(input.message),
            reason: optional_trimmed(input.reason),
        }
    }
}

#[derive(Debug)]
struct TurnCheckpointCommand {
    message: Option<TrimmedText>,
    awaiting: Option<TrimmedText>,
    reason: Option<TrimmedText>,
}

impl From<TurnCheckpointInput> for TurnCheckpointCommand {
    fn from(input: TurnCheckpointInput) -> Self {
        Self {
            message: optional_trimmed(input.message),
            awaiting: optional_trimmed(input.awaiting),
            reason: optional_trimmed(input.reason),
        }
    }
}

#[derive(Debug)]
struct TurnRequestMoreRoundsCommand {
    requested_rounds: usize,
    reason: Option<TrimmedText>,
    progress_summary: Option<TrimmedText>,
}

impl From<TurnRequestMoreRoundsInput> for TurnRequestMoreRoundsCommand {
    fn from(input: TurnRequestMoreRoundsInput) -> Self {
        Self {
            requested_rounds: input
                .requested_rounds
                .unwrap_or(0)
                .clamp(1, crate::turn_budget_request::MAX_REQUESTED_ROUNDS_PER_ASK),
            reason: optional_trimmed(input.reason),
            progress_summary: optional_trimmed(input.progress_summary),
        }
    }
}

/// Extract the principal-facing checkpoint from a tool batch, if `cognition_turn_checkpoint` ran.
pub fn checkpoint_turn_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_checkpoint_turn_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(message) = message_from_turn_control_message_payload(&inv.tool_input) {
            return Some(message);
        }
        if let Some(message) = message_from_turn_control_message_payload(&inv.tool_output) {
            return Some(message);
        }
    }
    None
}

/// Signal tool-loop entry with a principal-facing progress line (loop continues).
pub struct CognitionTurnBeginWorkTool {
    scheduler: std::sync::Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
}

impl CognitionTurnBeginWorkTool {
    pub fn new(
        scheduler: std::sync::Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
    ) -> Self {
        Self { scheduler }
    }
}

#[derive(Debug, JsonSchema)]
pub struct TurnBeginWorkInput {
    /// Short principal-facing ack before workshop execution
    #[schemars(required, with = "String")]
    message: Option<String>,
    /// Focused execution task for the bound workshop (tools, surfaces, constraints)
    #[schemars(required, with = "String")]
    goal: Option<String>,
    /// Optional worker profile: general | research (default general)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    intent: Option<String>,
}

impl<'de> Deserialize<'de> for TurnBeginWorkInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            message: CompatOption<String>,
            #[serde(default)]
            goal: CompatOption<String>,
            #[serde(default)]
            intent: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            message: input.message.into_option(),
            goal: input.goal.into_option(),
            intent: input.intent.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_TURN_BEGIN_WORK_ID)]
impl CognitionTurnBeginWorkTool {
    /// Enter the bound workshop for multi-tool execution (environment/canvas, components, vault writes). Provide a short principal-facing message and a concrete goal for the workshop executor. Host turn ends with the ack; synthesis delivers on the same thread when the workshop finishes.
    async fn invoke_typed(
        &self,
        input: TurnBeginWorkInput,
    ) -> stasis::prelude::Result<crate::agent_runtime::turn_worker::EnterBoundWorkshopOutput> {
        let command = TurnBeginWorkCommand::from(input);
        let Some(message) = command.message else {
            return Ok(
                crate::agent_runtime::turn_worker::EnterBoundWorkshopOutput::Failure {
                    ok: false,
                    workshop_entered: false,
                    error: "message is required and must be non-empty".to_string(),
                },
            );
        };
        let Some(goal) = command.goal else {
            return Ok(
                crate::agent_runtime::turn_worker::EnterBoundWorkshopOutput::Failure {
                    ok: false,
                    workshop_entered: false,
                    error: "goal is required and must be non-empty".to_string(),
                },
            );
        };

        self.scheduler
            .enter_bound_workshop(message.as_str(), goal.as_str(), command.intent)
            .await
    }
}

pub fn workshop_entered_from_invocations(
    invocations: &[ToolInvocation],
) -> Option<(String, String)> {
    invocations.iter().rev().find_map(|inv| {
        if !is_begin_work_tool_name(&inv.tool_name) {
            return None;
        }
        let entered = inv
            .tool_output
            .get("workshop_entered")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !entered {
            return None;
        }
        let work_id = inv.tool_output.get("work_id")?.as_str()?.to_string();
        let ack = inv
            .tool_output
            .get("user_ack")
            .and_then(|v| v.as_str())
            .or_else(|| inv.tool_output.get("message").and_then(|v| v.as_str()))
            .unwrap_or("Working on that in the workshop.")
            .to_string();
        Some((work_id, ack))
    })
}

/// Short principal-facing status while the turn continues (not a final answer).
pub struct CognitionTurnUpdateUserTool;

#[derive(Debug, JsonSchema)]
pub struct TurnUpdateUserInput {
    /// Short principal-facing status line
    #[schemars(required, with = "String")]
    message: Option<String>,
}

impl<'de> Deserialize<'de> for TurnUpdateUserInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            message: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            message: input.message.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TurnUpdateUserOutput {
    Success {
        ok: bool,
        update_user: bool,
        message: String,
    },
    Failure {
        ok: bool,
        update_user: bool,
        error: String,
    },
}

#[medousa_tool(id = COGNITION_TURN_UPDATE_USER_ID)]
impl CognitionTurnUpdateUserTool {
    /// Tell the principal what you are doing right now — retries, quick course-corrections, "pulling schemas", "one sec". Call in the same model round as your next tool. Does not end the turn. Prefer this over naked chat prose (prose without tools fights the turn loop). For heavy/long-running work starting, use cognition_turn_begin_work instead.
    async fn invoke_typed(
        &self,
        input: TurnUpdateUserInput,
    ) -> stasis::prelude::Result<TurnUpdateUserOutput> {
        let command = TurnUpdateUserCommand::from(input);
        let Some(message) = command.message else {
            return Ok(TurnUpdateUserOutput::Failure {
                ok: false,
                update_user: false,
                error: "message is required and must be non-empty".to_string(),
            });
        };

        Ok(TurnUpdateUserOutput::Success {
            ok: true,
            update_user: true,
            message: message.into_string(),
        })
    }
}

pub struct CognitionTurnProposeModeTool {
    bootstrap_session_id: String,
    turn_scope: std::sync::Arc<
        tokio::sync::RwLock<Option<crate::turn_continuation::TurnContinuationScope>>,
    >,
}

impl CognitionTurnProposeModeTool {
    pub fn new(
        bootstrap_session_id: String,
        turn_scope: std::sync::Arc<
            tokio::sync::RwLock<Option<crate::turn_continuation::TurnContinuationScope>>,
        >,
    ) -> Self {
        Self {
            bootstrap_session_id,
            turn_scope,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnModeInput {
    General,
    Coder,
}

impl From<TurnModeInput> for crate::daemon_api::AgentModeId {
    fn from(value: TurnModeInput) -> Self {
        match value {
            TurnModeInput::General => Self::General,
            TurnModeInput::Coder => Self::Coder,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnModeScopeInput {
    #[default]
    Session,
    Task,
}

impl From<TurnModeScopeInput> for crate::daemon_api::AgentModeScope {
    fn from(value: TurnModeScopeInput) -> Self {
        match value {
            TurnModeScopeInput::Session => Self::Session,
            TurnModeScopeInput::Task => Self::Task,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnProposeModeInput {
    mode: TurnModeInput,
    #[serde(default)]
    #[schemars(default)]
    scope: TurnModeScopeInput,
    /// Required for task scope; use the active undertaking/work id when relevant
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    /// Short user-facing reason this mode better fits the work
    reason: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TurnProposeModeOutput {
    Failure {
        ok: bool,
        error: String,
    },
    Success {
        ok: bool,
        mode_proposal: crate::daemon_api::AgentModeProposalResponse,
        message: String,
    },
}

#[medousa_tool(id = COGNITION_TURN_PROPOSE_MODE_ID)]
impl CognitionTurnProposeModeTool {
    /// Propose switching Medousa's mode for the current chat. Use Coder only when repository inspection, edits, commands, or tests would materially help; programming explanations stay in General. The runtime applies the user's auto-accept/expiry policy and never expands authority from this tool alone.
    async fn invoke_typed(
        &self,
        input: TurnProposeModeInput,
    ) -> stasis::prelude::Result<TurnProposeModeOutput> {
        let command = TurnProposeModeCommand::from(input);
        let Some(reason) = command.reason else {
            return Ok(TurnProposeModeOutput::Failure {
                ok: false,
                error: "reason is required".to_string(),
            });
        };
        let session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.bootstrap_session_id,
        )
        .await;
        let proposal = crate::agent_mode_state::propose_mode_transition(
            &session_id,
            command.mode,
            command.scope,
            command.task_id.as_ref().map(TrimmedText::as_str),
            reason.as_str(),
        )
        .map_err(stasis::domain::errors::StasisError::PortFailure)?;
        let message = if proposal.resolution
            == Some(crate::daemon_api::AgentModeProposalResolution::AutoAccepted)
        {
            "Mode switched automatically under the user's policy; it applies on the next turn."
        } else {
            "Mode change proposed to the user; continue in the current mode for this turn."
        };
        Ok(TurnProposeModeOutput::Success {
            ok: true,
            mode_proposal: proposal,
            message: message.to_string(),
        })
    }
}

/// Signal that the **next** assistant message (text-only) should be the user-facing final answer.
pub struct CognitionTurnPrepareFinalTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnPrepareFinalInput {
    /// Optional short note for logs (not shown to the user)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    reason: CompatOption<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TurnPrepareFinalOutput {
    ok: bool,
    prepare_final: bool,
    deprecated: bool,
    message: String,
    reason: Option<String>,
}

#[medousa_tool(id = COGNITION_TURN_PREPARE_FINAL_ID)]
impl CognitionTurnPrepareFinalTool {
    /// Deprecated — prefer cognition_turn_finish with the complete answer. Workshop workers may still call this; host turns should use cognition_turn_update_user for quick status, cognition_turn_begin_work before heavy work, and cognition_turn_finish to commit.
    async fn invoke_typed(
        &self,
        input: TurnPrepareFinalInput,
    ) -> stasis::prelude::Result<TurnPrepareFinalOutput> {
        let command = TurnPrepareFinalCommand::from(input);
        Ok(TurnPrepareFinalOutput {
            ok: true,
            prepare_final: true,
            deprecated: true,
            message: "Deprecated — call cognition_turn_finish with the complete principal-facing reply. Workshop lane may still send one final prose round.".to_string(),
            reason: command.reason.map(TrimmedText::into_string),
        })
    }
}

/// End the turn immediately with the final user-facing answer (bypasses gatekeeper continue).
pub struct CognitionTurnFinishTool;

#[derive(Debug, JsonSchema)]
pub struct TurnFinishInput {
    /// Complete principal-facing final answer for this turn
    #[schemars(required, with = "String")]
    message: Option<String>,
    /// Optional short note for logs (not shown to the user)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl<'de> Deserialize<'de> for TurnFinishInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            message: CompatOption<String>,
            #[serde(default)]
            reason: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            message: input.message.into_option(),
            reason: input.reason.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TurnFinishOutput {
    Success {
        ok: bool,
        finish_turn: bool,
        message: String,
        reason: Option<String>,
    },
    Failure {
        ok: bool,
        finish_turn: bool,
        error: String,
    },
}

#[medousa_tool(id = COGNITION_TURN_FINISH_ID)]
impl CognitionTurnFinishTool {
    /// Deliver the complete principal-facing final answer now and end this turn immediately. Use it as an explicit hard stop after tool work; synthesis-bound workers require it for direct pass-through, while principal-facing turns may also commit after two consecutive non-tool responses. Mid-task handoffs use cognition_turn_checkpoint.
    async fn invoke_typed(
        &self,
        input: TurnFinishInput,
    ) -> stasis::prelude::Result<TurnFinishOutput> {
        let command = TurnFinishCommand::from(input);
        let Some(message) = command.message else {
            return Ok(TurnFinishOutput::Failure {
                ok: false,
                finish_turn: false,
                error: "message is required and must be non-empty".to_string(),
            });
        };

        Ok(TurnFinishOutput::Success {
            ok: true,
            finish_turn: true,
            message: message.into_string(),
            reason: command.reason.map(TrimmedText::into_string),
        })
    }
}

/// Hand a mid-task update to the principal and end this agent turn (await their reply to continue).
pub struct CognitionTurnCheckpointTool;

#[derive(Debug, JsonSchema)]
pub struct TurnCheckpointInput {
    /// Principal-facing update: what you did, what you found, and what happens next or what you need from them
    #[schemars(required, with = "String")]
    message: Option<String>,
    /// Optional: what you need from the principal before more tool work (decision, confirmation, missing detail)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    awaiting: Option<String>,
    /// Optional short note for logs (not shown to the principal)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl<'de> Deserialize<'de> for TurnCheckpointInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            message: CompatOption<String>,
            #[serde(default)]
            awaiting: CompatOption<String>,
            #[serde(default)]
            reason: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            message: input.message.into_option(),
            awaiting: input.awaiting.into_option(),
            reason: input.reason.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TurnCheckpointOutput {
    Success {
        ok: bool,
        checkpoint_turn: bool,
        message: String,
        awaiting: Option<String>,
        reason: Option<String>,
    },
    Failure {
        ok: bool,
        checkpoint_turn: bool,
        error: String,
    },
}

#[medousa_tool(id = COGNITION_TURN_CHECKPOINT_ID)]
impl CognitionTurnCheckpointTool {
    /// Share a substantive mid-task update with the principal and hand the turn back to them. The conversation is not over — you may continue after they reply. Use when tool work produced real progress but you are not done (not a final answer). Prefer this over streaming long interim prose that the runtime may loop on.
    async fn invoke_typed(
        &self,
        input: TurnCheckpointInput,
    ) -> stasis::prelude::Result<TurnCheckpointOutput> {
        let command = TurnCheckpointCommand::from(input);
        let Some(message) = command.message else {
            return Ok(TurnCheckpointOutput::Failure {
                ok: false,
                checkpoint_turn: false,
                error: "message is required and must be non-empty".to_string(),
            });
        };

        Ok(TurnCheckpointOutput::Success {
            ok: true,
            checkpoint_turn: true,
            message: message.into_string(),
            awaiting: command.awaiting.map(TrimmedText::into_string),
            reason: command.reason.map(TrimmedText::into_string),
        })
    }
}

/// Pause the turn and ask the operator for more tool rounds.
pub struct CognitionTurnRequestMoreRoundsTool;

#[derive(Debug, JsonSchema)]
pub struct TurnRequestMoreRoundsInput {
    /// How many additional model/tool rounds you need
    #[schemars(
        required,
        with = "i64",
        range(
            min = 1,
            max = "crate::turn_budget_request::MAX_REQUESTED_ROUNDS_PER_ASK"
        )
    )]
    requested_rounds: Option<usize>,
    /// Why the current budget is insufficient
    #[schemars(required, with = "String")]
    reason: Option<String>,
    /// What is done and what remains
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    progress_summary: Option<String>,
}

impl<'de> Deserialize<'de> for TurnRequestMoreRoundsInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            requested_rounds: CompatOption<usize>,
            #[serde(default)]
            reason: CompatOption<String>,
            #[serde(default)]
            progress_summary: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            requested_rounds: input.requested_rounds.into_option(),
            reason: input.reason.into_option(),
            progress_summary: input.progress_summary.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TurnRequestMoreRoundsOutput {
    Success {
        ok: bool,
        budget_request: bool,
        requested_rounds: usize,
        reason: String,
        progress_summary: Option<String>,
        message: String,
    },
    Failure {
        ok: bool,
        budget_request: bool,
        error: String,
    },
}

#[medousa_tool(id = COGNITION_TURN_REQUEST_MORE_ROUNDS_ID)]
impl CognitionTurnRequestMoreRoundsTool {
    /// Request additional tool rounds when the current budget is too tight. Pauses until the principal approves or denies. Include reason and progress summary.
    async fn invoke_typed(
        &self,
        input: TurnRequestMoreRoundsInput,
    ) -> stasis::prelude::Result<TurnRequestMoreRoundsOutput> {
        let command = TurnRequestMoreRoundsCommand::from(input);
        let Some(reason) = command.reason else {
            return Ok(TurnRequestMoreRoundsOutput::Failure {
                ok: false,
                budget_request: false,
                error: "reason is required".to_string(),
            });
        };

        Ok(TurnRequestMoreRoundsOutput::Success {
            ok: true,
            budget_request: true,
            requested_rounds: command.requested_rounds,
            reason: reason.into_string(),
            progress_summary: command.progress_summary.map(TrimmedText::into_string),
            message: "Turn paused — awaiting principal approval for additional tool rounds."
                .to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn turn_control_commands_normalize_text_and_defaults_once() {
        let begin = TurnBeginWorkCommand::from(TurnBeginWorkInput {
            message: Some("  Starting work.  ".into()),
            goal: Some("  inspect the repository  ".into()),
            intent: Some(" research ".into()),
        });
        assert_eq!(
            begin.message.as_ref().map(TrimmedText::as_str),
            Some("Starting work.")
        );
        assert_eq!(
            begin.goal.as_ref().map(TrimmedText::as_str),
            Some("inspect the repository")
        );
        assert_eq!(begin.intent.as_str(), "research");

        let request = TurnRequestMoreRoundsCommand::from(TurnRequestMoreRoundsInput {
            requested_rounds: Some(usize::MAX),
            reason: Some("  need another pass  ".into()),
            progress_summary: Some("  schemas are ready  ".into()),
        });
        assert_eq!(
            request.requested_rounds,
            crate::turn_budget_request::MAX_REQUESTED_ROUNDS_PER_ASK
        );
        assert_eq!(
            request.reason.as_ref().map(TrimmedText::as_str),
            Some("need another pass")
        );
        assert_eq!(
            request.progress_summary.as_ref().map(TrimmedText::as_str),
            Some("schemas are ready")
        );

        let finish = TurnFinishCommand::from(TurnFinishInput {
            message: Some("  exact final text  ".into()),
            reason: Some("  complete  ".into()),
        });
        assert_eq!(
            finish.message.as_ref().map(TrimmedText::as_str),
            Some("exact final text")
        );
        assert_eq!(
            finish.reason.as_ref().map(TrimmedText::as_str),
            Some("complete")
        );
    }

    #[test]
    fn preserves_interim_text_on_prose_requires_finish() {
        let draft = "Now I see what went wrong — let me grab the schemas.".to_string();
        let out = terminal_text_for_fsm_end("prose_requires_finish", draft.clone());
        assert_eq!(out, draft);
    }

    #[test]
    fn substantive_after_tools_is_preserved() {
        let draft = "Focused preset pulled and applied: stability is now 0.95, friction dropped \
                       to 0.12, and autonomy holds at 0.80. I stored the calibration summary.";
        let out = terminal_text_for_fsm_end("prose_requires_finish", draft.to_string());
        assert_eq!(out, draft);
    }

    #[test]
    fn recognizes_update_user_names() {
        assert!(is_update_user_tool_name("cognition_turn_update_user"));
        assert!(is_update_user_tool_name("cognition.turn.update_user"));
        assert!(!is_update_user_tool_name("cognition_turn_begin_work"));
    }

    #[test]
    fn recognizes_propose_mode_names() {
        assert!(is_propose_mode_tool_name("cognition_turn_propose_mode"));
        assert!(is_propose_mode_tool_name("cognition.turn.propose_mode"));
        assert!(!is_propose_mode_tool_name("cognition_turn_finish"));
    }

    #[test]
    fn update_user_from_invocations_reads_latest_successful_call() {
        let invocations = vec![ToolInvocation {
            tool_name: COGNITION_TURN_UPDATE_USER.to_string(),
            tool_input: json!({ "message": "Retrying propose with custom surface." }),
            tool_output: json!({ "ok": true, "update_user": true }),
        }];
        assert_eq!(
            update_user_message_from_invocations(&invocations).as_deref(),
            Some("Retrying propose with custom surface.")
        );
    }

    #[test]
    fn turn_progress_prefers_update_user_over_begin_work() {
        let invocations = vec![
            ToolInvocation {
                tool_name: COGNITION_TURN_BEGIN_WORK.to_string(),
                tool_input: json!({ "message": "Starting research worker." }),
                tool_output: json!({ "ok": true, "begin_work": true }),
            },
            ToolInvocation {
                tool_name: COGNITION_TURN_UPDATE_USER.to_string(),
                tool_input: json!({ "message": "Grabbing wiki schemas." }),
                tool_output: json!({ "ok": true, "update_user": true }),
            },
        ];
        assert_eq!(
            turn_progress_message_from_invocations(&invocations).as_deref(),
            Some("Grabbing wiki schemas.")
        );
    }

    #[test]
    fn recognizes_begin_work_names() {
        assert!(is_begin_work_tool_name("cognition_turn_begin_work"));
        assert!(is_begin_work_tool_name("cognition.turn.begin_work"));
        assert!(!is_begin_work_tool_name("cognition_turn_finish"));
    }

    #[test]
    fn begin_work_from_invocations_reads_latest_successful_call() {
        let invocations = vec![
            ToolInvocation {
                tool_name: COGNITION_TURN_BEGIN_WORK.to_string(),
                tool_input: json!({ "message": "Checking memory nodes." }),
                tool_output: json!({ "ok": true, "begin_work": true }),
            },
            ToolInvocation {
                tool_name: "cognition_memory_list".to_string(),
                tool_input: Value::Null,
                tool_output: Value::Null,
            },
        ];
        assert_eq!(
            begin_work_message_from_invocations(&invocations).as_deref(),
            Some("Checking memory nodes.")
        );
    }

    #[test]
    fn recognizes_prepare_final_names() {
        assert!(is_prepare_final_tool_name("cognition_turn_prepare_final"));
        assert!(is_prepare_final_tool_name("cognition.turn.prepare_final"));
        assert!(!is_prepare_final_tool_name("cognition_memory_store"));
    }

    #[test]
    fn recognizes_finish_turn_names() {
        assert!(is_finish_turn_tool_name("cognition_turn_finish"));
        assert!(is_finish_turn_tool_name("cognition.turn.finish"));
        assert!(!is_finish_turn_tool_name("cognition_turn_prepare_final"));
    }

    #[test]
    fn finish_turn_from_invocations_reads_latest_successful_call() {
        let invocations = vec![
            ToolInvocation {
                tool_name: "cognition_memory_recall".to_string(),
                tool_input: json!({}),
                tool_output: json!({"ok": true}),
            },
            ToolInvocation {
                tool_name: COGNITION_TURN_FINISH.to_string(),
                tool_input: json!({"message": "Here is the complete answer."}),
                tool_output: json!({"ok": true, "finish_turn": true, "message": "Here is the complete answer."}),
            },
        ];
        assert_eq!(
            finish_turn_from_invocations(&invocations).as_deref(),
            Some("Here is the complete answer.")
        );
    }

    #[test]
    fn finish_turn_from_invocations_skips_failed_tool_output() {
        let invocations = vec![ToolInvocation {
            tool_name: COGNITION_TURN_FINISH.to_string(),
            tool_input: json!({"message": ""}),
            tool_output: json!({"ok": false, "error": "message is required and must be non-empty"}),
        }];
        assert!(finish_turn_from_invocations(&invocations).is_none());
    }

    #[tokio::test]
    async fn finish_turn_tool_requires_message() {
        let tool = CognitionTurnFinishTool;
        let out = tool.invoke(json!({})).await.expect("invoke");
        assert_eq!(out["ok"], false);
    }

    #[tokio::test]
    async fn compatibility_option_keeps_wrong_typed_optional_input_lenient() {
        let tool = CognitionTurnFinishTool;
        let out = tool
            .invoke(json!({ "message": 42, "reason": false }))
            .await
            .expect("wrong-typed optionals stay handler-visible");
        assert_eq!(out["ok"], false);
        assert!(
            out["error"]
                .as_str()
                .is_some_and(|value| value.contains("message is required"))
        );
    }

    #[tokio::test]
    async fn finish_turn_tool_returns_message() {
        let tool = CognitionTurnFinishTool;
        let out = tool
            .invoke(json!({"message": "Done.", "reason": "task complete"}))
            .await
            .expect("invoke");
        assert_eq!(out["ok"], true);
        assert_eq!(out["finish_turn"], true);
        assert_eq!(out["message"], "Done.");
        assert_eq!(out["reason"], "task complete");
    }

    #[test]
    fn recognizes_checkpoint_turn_names() {
        assert!(is_checkpoint_turn_tool_name("cognition_turn_checkpoint"));
        assert!(is_checkpoint_turn_tool_name("cognition.turn.checkpoint"));
        assert!(!is_checkpoint_turn_tool_name("cognition_turn_finish"));
    }

    #[test]
    fn checkpoint_turn_from_invocations_reads_latest_successful_call() {
        let invocations = vec![ToolInvocation {
            tool_name: COGNITION_TURN_CHECKPOINT.to_string(),
            tool_input: json!({"message": "Found three blockers — need your pick on scope."}),
            tool_output: json!({"ok": true, "checkpoint_turn": true}),
        }];
        assert_eq!(
            checkpoint_turn_from_invocations(&invocations).as_deref(),
            Some("Found three blockers — need your pick on scope.")
        );
    }

    #[tokio::test]
    async fn checkpoint_turn_tool_requires_message() {
        let tool = CognitionTurnCheckpointTool;
        let out = tool.invoke(json!({})).await.expect("invoke");
        assert_eq!(out["ok"], false);
    }
}
