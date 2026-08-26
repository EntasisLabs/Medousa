//! Public turn primitive: begin, update, checkpoint, finish, and related signals.
//!
//! The model-facing entry is a tagged action enum. Parameter schemas live on
//! each variant type — `cognition_schema` reads those types, not a parallel catalog.

use std::sync::Arc;

use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::Value;

use crate::agent_runtime::turn_worker::TurnWorkerScheduler;
use crate::public_api::COGNITION_TURN;
use crate::schema_api::{
    TypedActionSchema, advertised_object_schema, string_enum_schema, typed_action_schema,
};
use crate::turn_control_tools::{
    CognitionTurnBeginWorkTool, CognitionTurnCheckpointTool, CognitionTurnFinishTool,
    CognitionTurnPrepareFinalTool, CognitionTurnProposeModeTool,
    CognitionTurnRequestMoreRoundsTool, CognitionTurnUpdateUserTool, TurnBeginWorkInput,
    TurnCheckpointInput, TurnFinishInput, TurnModeInput, TurnModeScopeInput, TurnPrepareFinalInput,
    TurnProposeModeInput, TurnRequestMoreRoundsInput, TurnUpdateUserInput,
};
use crate::typed_tools::{
    CompatOption, ExternalJson, ToolId, TypedTool, medousa_tool, serialize_output,
};

const TURN_ID: ToolId = ToolId::new(COGNITION_TURN);

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum TurnAction {
    #[serde(rename = "turn.begin_work")]
    BeginWork(TurnBeginWork),
    #[serde(rename = "turn.update_user")]
    UpdateUser(TurnUpdateUser),
    #[serde(rename = "turn.checkpoint")]
    Checkpoint(TurnCheckpoint),
    #[serde(rename = "turn.request_input")]
    RequestInput(TurnRequestInput),
    #[serde(rename = "turn.finish")]
    Finish(TurnFinish),
    #[serde(rename = "turn.prepare_final")]
    PrepareFinal(TurnPrepareFinal),
    #[serde(rename = "turn.request_more_rounds")]
    RequestMoreRounds(TurnRequestMoreRounds),
    #[serde(rename = "turn.propose_mode")]
    ProposeMode(TurnProposeMode),
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnBeginWork {
    /// Short principal-facing ack before workshop execution
    message: String,
    /// Focused execution task for the bound workshop
    goal: String,
    /// Optional worker profile: general | research
    #[serde(default)]
    intent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnUpdateUser {
    /// Short principal-facing status line
    message: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnCheckpoint {
    /// Principal-facing update and what happens next
    message: String,
    /// What you need from the principal before more work
    #[serde(default)]
    awaiting: Option<String>,
    /// Optional short note for logs
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnRequestInput {
    /// The concrete question or choice the principal must answer
    message: String,
    /// Optional short note for logs
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnFinish {
    /// Fallback final answer when this response has no assistant prose
    #[serde(default)]
    message: Option<String>,
    /// Optional short note for logs
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TurnPrepareFinal {
    /// Optional short note for logs
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnRequestMoreRounds {
    /// How many additional model/tool rounds you need
    requested_rounds: usize,
    /// Why the current budget is insufficient
    reason: String,
    /// What is done and what remains
    #[serde(default)]
    progress_summary: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnProposeMode {
    mode: TurnModeInput,
    #[serde(default)]
    scope: TurnModeScopeInput,
    #[serde(default)]
    task_id: Option<String>,
    /// Short user-facing reason this mode better fits the work
    reason: String,
}

impl JsonSchema for TurnAction {
    fn schema_name() -> String {
        "TurnAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&[
                "turn.begin_work",
                "turn.update_user",
                "turn.checkpoint",
                "turn.request_input",
                "turn.finish",
                "turn.request_more_rounds",
                "turn.propose_mode",
            ]),
            true,
        )])
    }
}

pub fn turn_type_schemas() -> Vec<TypedActionSchema> {
    vec![
        typed_action_schema::<TurnBeginWork>(
            TURN_ID,
            "turn.begin_work",
            "Enter bound Workshop for multi-tool execution",
        ),
        typed_action_schema::<TurnUpdateUser>(
            TURN_ID,
            "turn.update_user",
            "Short principal-facing status; turn continues",
        ),
        typed_action_schema::<TurnCheckpoint>(
            TURN_ID,
            "turn.checkpoint",
            "Mid-task handoff; wait for the principal",
        ),
        typed_action_schema::<TurnRequestInput>(
            TURN_ID,
            "turn.request_input",
            "Ask the principal for required input and end this turn",
        ),
        typed_action_schema::<TurnFinish>(
            TURN_ID,
            "turn.finish",
            "Deliver the final answer and end the turn",
        ),
        typed_action_schema::<TurnRequestMoreRounds>(
            TURN_ID,
            "turn.request_more_rounds",
            "Ask the principal for more tool rounds",
        ),
        typed_action_schema::<TurnProposeMode>(
            TURN_ID,
            "turn.propose_mode",
            "Propose General or Coder for this chat",
        ),
    ]
}

pub struct CognitionTurnTool {
    scheduler: Arc<TurnWorkerScheduler>,
    bootstrap_session_id: String,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionTurnTool {
    pub fn new(
        scheduler: Arc<TurnWorkerScheduler>,
        bootstrap_session_id: String,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            scheduler,
            bootstrap_session_id,
            turn_scope,
        }
    }
}

pub fn register_turn_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    scheduler: Arc<TurnWorkerScheduler>,
    bootstrap_session_id: String,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionTurnTool::new(
        scheduler,
        bootstrap_session_id,
        turn_scope,
    ))?;
    Ok(())
}

#[medousa_tool(id = TURN_ID)]
impl CognitionTurnTool {
    /// Control this turn: begin work, update the principal, checkpoint, finish, or request more rounds. action is a typed name (turn.finish, turn.checkpoint, …). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(&self, action: TurnAction) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch(self, action).await?))
    }
}

async fn dispatch(tool: &CognitionTurnTool, action: TurnAction) -> stasis::prelude::Result<Value> {
    match action {
        TurnAction::BeginWork(params) => params.execute(tool).await,
        TurnAction::UpdateUser(params) => params.execute().await,
        TurnAction::Checkpoint(params) => params.execute().await,
        TurnAction::RequestInput(params) => params.execute().await,
        TurnAction::Finish(params) => params.execute().await,
        TurnAction::PrepareFinal(params) => params.execute().await,
        TurnAction::RequestMoreRounds(params) => params.execute().await,
        TurnAction::ProposeMode(params) => params.execute(tool).await,
    }
}

impl TurnBeginWork {
    async fn execute(self, tool: &CognitionTurnTool) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnBeginWorkTool::new(tool.scheduler.clone())
            .invoke_typed(TurnBeginWorkInput {
                message: Some(self.message),
                goal: Some(self.goal),
                intent: self.intent,
            })
            .await?;
        serialize_output(CognitionTurnBeginWorkTool::tool_id(), output)
    }
}

impl TurnUpdateUser {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnUpdateUserTool
            .invoke_typed(TurnUpdateUserInput {
                message: Some(self.message),
            })
            .await?;
        serialize_output(CognitionTurnUpdateUserTool::tool_id(), output)
    }
}

impl TurnCheckpoint {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnCheckpointTool
            .invoke_typed(TurnCheckpointInput {
                message: Some(self.message),
                awaiting: self.awaiting,
                reason: self.reason,
            })
            .await?;
        serialize_output(CognitionTurnCheckpointTool::tool_id(), output)
    }
}

impl TurnRequestInput {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnCheckpointTool
            .invoke_typed(TurnCheckpointInput {
                message: Some(self.message.clone()),
                awaiting: Some(self.message),
                reason: self.reason,
            })
            .await?;
        serialize_output(CognitionTurnCheckpointTool::tool_id(), output)
    }
}

impl TurnFinish {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnFinishTool
            .invoke_typed(TurnFinishInput {
                message: self.message,
                reason: self.reason,
            })
            .await?;
        serialize_output(CognitionTurnFinishTool::tool_id(), output)
    }
}

impl TurnPrepareFinal {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnPrepareFinalTool
            .invoke_typed(TurnPrepareFinalInput {
                reason: CompatOption::from(self.reason),
            })
            .await?;
        serialize_output(CognitionTurnPrepareFinalTool::tool_id(), output)
    }
}

impl TurnRequestMoreRounds {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnRequestMoreRoundsTool
            .invoke_typed(TurnRequestMoreRoundsInput {
                requested_rounds: Some(self.requested_rounds),
                reason: Some(self.reason),
                progress_summary: self.progress_summary,
            })
            .await?;
        serialize_output(CognitionTurnRequestMoreRoundsTool::tool_id(), output)
    }
}

impl TurnProposeMode {
    async fn execute(self, tool: &CognitionTurnTool) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnProposeModeTool::new(
            tool.bootstrap_session_id.clone(),
            tool.turn_scope.clone(),
        )
        .invoke_typed(TurnProposeModeInput {
            mode: self.mode,
            scope: self.scope,
            task_id: self.task_id,
            reason: self.reason,
        })
        .await?;
        serialize_output(CognitionTurnProposeModeTool::tool_id(), output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn turn_actions_carry_their_params() {
        let finish: TurnAction = serde_json::from_value(json!({
            "action": "turn.finish",
            "message": "Done."
        }))
        .expect("finish");
        match finish {
            TurnAction::Finish(TurnFinish { message, reason }) => {
                assert_eq!(message.as_deref(), Some("Done."));
                assert!(reason.is_none());
            }
            other => panic!("expected finish, got {other:?}"),
        }
    }

    #[test]
    fn advertised_schema_is_action_only() {
        let schema = serde_json::to_value(schemars::schema_for!(TurnAction)).expect("schema");
        let props = schema["properties"].as_object().expect("properties");
        assert_eq!(props.len(), 1);
        assert!(props.contains_key("action"));
        assert_eq!(schema["additionalProperties"], true);
        let actions = schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum");
        assert!(actions.iter().any(|value| value == "turn.request_input"));
        assert!(!actions.iter().any(|value| value == "turn.prepare_final"));
    }

    #[test]
    fn finish_message_is_optional_and_request_input_is_explicit() {
        let finish: TurnAction =
            serde_json::from_value(json!({ "action": "turn.finish" })).expect("silent finish");
        assert!(matches!(
            finish,
            TurnAction::Finish(TurnFinish { message: None, .. })
        ));

        let request: TurnAction = serde_json::from_value(json!({
            "action": "turn.request_input",
            "message": "Which repository?"
        }))
        .expect("request input");
        assert!(matches!(request, TurnAction::RequestInput(_)));
    }
}
