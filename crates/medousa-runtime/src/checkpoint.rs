//! Portable checkpoint boundary vocabulary shared by runtime compositions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use genai::chat::ChatMessage;
use medousa_engine::TurnScratchpad;
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::budget::TurnOrchestrationState;

pub const TOOL_ROUND_BUDGET_EXHAUSTED_REASON: &str = "tool_round_budget_exhausted";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveTurnCheckpointStatus {
    Active,
    AwaitingUser,
    BudgetExhausted,
    RecoverableFailure,
    Completed,
    Superseded,
}

impl ActiveTurnCheckpointStatus {
    pub fn is_resume_candidate(self) -> bool {
        matches!(
            self,
            Self::Active | Self::AwaitingUser | Self::BudgetExhausted | Self::RecoverableFailure
        )
    }

    pub fn restores_interrupted_budget(self) -> bool {
        matches!(self, Self::Active | Self::RecoverableFailure)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeCheckpointBoundary {
    TurnStarted,
    ModelResponseCompleted,
    ToolBatchCompleted,
    AwaitingApproval,
    AwaitingUser,
    BudgetExhausted,
    RecoverableFailure,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutstandingTurnBoundary {
    UserInput {
        reason: String,
    },
    BudgetApproval {
        request_id: String,
        requested_rounds: usize,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveTurnCounters {
    pub model_rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub tool_batches_completed: usize,
    pub text_only_continues_without_new_tools: usize,
    pub invocations_at_last_text_continue: usize,
    pub user_responses_sent: usize,
    pub last_response_preview: Option<String>,
    pub retry_count: usize,
    pub orchestration: Option<TurnOrchestrationState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveTurnTranscript {
    pub user_lane_prefix: Vec<ChatMessage>,
    pub tool_lane_messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointToolInvocation {
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_output: Value,
}

impl CheckpointToolInvocation {
    /// Capture the logical invocation. Concrete persistence adapters own
    /// redaction and byte bounding before durable writes.
    pub fn from_runtime(invocation: &ToolInvocation) -> Self {
        Self {
            tool_name: invocation.tool_name.clone(),
            tool_input: invocation.tool_input.clone(),
            tool_output: invocation.tool_output.clone(),
        }
    }

    pub fn into_runtime(self) -> ToolInvocation {
        ToolInvocation {
            tool_name: self.tool_name,
            tool_input: self.tool_input,
            tool_output: self.tool_output,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActiveTurnResumeState {
    pub source_daemon_turn_id: String,
    pub restore_turn_budget: bool,
    pub append_current_user_message: bool,
    pub counters: ActiveTurnCounters,
    pub transcript: ActiveTurnTranscript,
    pub invocations: Vec<CheckpointToolInvocation>,
    pub scratch: TurnScratchpad,
}

#[derive(Debug, Clone)]
pub struct ToolLoopCheckpointState {
    pub boundary: SafeCheckpointBoundary,
    pub status: ActiveTurnCheckpointStatus,
    pub counters: ActiveTurnCounters,
    pub user_lane_prefix: Vec<ChatMessage>,
    pub tool_lane_messages: Vec<ChatMessage>,
    pub invocations: Vec<CheckpointToolInvocation>,
    pub scratch: TurnScratchpad,
    pub outstanding_boundary: Option<OutstandingTurnBoundary>,
    pub tool_names: Vec<String>,
    pub provider_call_ids: Vec<String>,
    pub termination_reason: Option<String>,
}

pub trait ActiveTurnCheckpointSink: Send + Sync {
    fn persist_boundary(&self, state: ToolLoopCheckpointState) -> Result<(), String>;
    fn mark_status(
        &self,
        status: ActiveTurnCheckpointStatus,
        boundary: SafeCheckpointBoundary,
        reason: Option<&str>,
        orchestration: Option<&TurnOrchestrationState>,
    ) -> Result<(), String>;
    fn latest_safe_resume(&self) -> Result<Option<ActiveTurnResumeState>, String>;
    fn set_model_route(&self, provider: &str, model: &str) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_exhaustion_can_resume_but_receives_a_fresh_budget() {
        assert!(ActiveTurnCheckpointStatus::BudgetExhausted.is_resume_candidate());
        assert!(!ActiveTurnCheckpointStatus::BudgetExhausted.restores_interrupted_budget());
    }

    #[test]
    fn checkpoint_boundary_wire_names_remain_stable() {
        assert_eq!(
            serde_json::to_string(&SafeCheckpointBoundary::ToolBatchCompleted).unwrap(),
            "\"tool_batch_completed\""
        );
        let outstanding = OutstandingTurnBoundary::BudgetApproval {
            request_id: "request-1".to_string(),
            requested_rounds: 4,
        };
        assert_eq!(
            serde_json::to_value(outstanding).unwrap()["kind"],
            "budget_approval"
        );
    }

    #[test]
    fn logical_invocations_round_trip_without_host_storage_policy() {
        let runtime = ToolInvocation {
            tool_name: "query".to_string(),
            tool_input: serde_json::json!({ "range": 4 }),
            tool_output: serde_json::json!({ "ok": true }),
        };
        let restored = CheckpointToolInvocation::from_runtime(&runtime).into_runtime();
        assert_eq!(restored.tool_name, runtime.tool_name);
        assert_eq!(restored.tool_input, runtime.tool_input);
        assert_eq!(restored.tool_output, runtime.tool_output);
    }
}
