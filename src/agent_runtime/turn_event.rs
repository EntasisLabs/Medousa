//! Phase 1(a) — the typed turn event + envelope: the domain vocabulary the
//! engine emits, independent of any daemon/transport type.
//!
//! This is the public shape later phases build on:
//! * [`TurnEnvelope`] is the metadata that rides with a turn through every
//!   bounded context (`turn_id`, `correlation_id`, `principal`, `surface`,
//!   `seq`) — it sets up multi-principal / federation routing later.
//! * [`TurnEvent`] is the typed event the engine emits (content delta,
//!   progress, terminal body, checkpoint, ack, needs-input, tool lifecycle,
//!   notice, error, budget gate). It is `Serialize`/`Deserialize` so it can be
//!   journaled into the durable per-turn event log (the spine) and folded into
//!   SSE replay + history projections.
//!
//! The existing [`crate::agent_runtime::stream_sink::AgentStreamSink`] port is
//! the in-tree emitter today; `TurnEvent` is the typed payload that port will
//! carry once the conflated sink is fully split. Mapping helpers
//! ([`TurnEvent::is_terminal`], [`TurnEvent::kind`]) keep that future projection
//! code aligned with the live SSE `event_type` taxonomy.

use serde::{Deserialize, Serialize};

/// Who/what initiated the turn. Lets a single engine serve multiple principals
/// (operator, channel user, system scheduler, background worker) and is the
/// hook for federation/authz later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    /// The local human operator (TUI / desktop).
    Operator,
    /// A messaging-channel user (Telegram, WhatsApp, …).
    Channel,
    /// The daemon itself (heartbeat, scheduler, recovery).
    System,
    /// A spawned turn worker acting on behalf of a parent turn.
    Worker,
}

/// Identity of the turn's principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub kind: PrincipalKind,
    /// Stable id within `kind` (operator id, channel user id, worker id, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl Principal {
    pub fn operator() -> Self {
        Self {
            kind: PrincipalKind::Operator,
            id: None,
        }
    }

    pub fn system() -> Self {
        Self {
            kind: PrincipalKind::System,
            id: None,
        }
    }

    pub fn channel(id: impl Into<String>) -> Self {
        Self {
            kind: PrincipalKind::Channel,
            id: Some(id.into()),
        }
    }
}

/// Delivery surface the turn is bound to (mirrors the fields the daemon already
/// threads via `TurnSurfaceContext`, but transport-free).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnSurface {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_surface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

/// Metadata that rides with a turn through every bounded context.
///
/// `seq` is the monotonic per-turn event sequence. The turn-level envelope (as
/// passed into `run_turn`) carries `seq = 0`; each emitted event clones the
/// envelope and stamps the next sequence number, which is exactly the value SSE
/// `?since=N` replay dedupes on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnEnvelope {
    pub turn_id: String,
    pub correlation_id: String,
    pub principal: Principal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<TurnSurface>,
    #[serde(default)]
    pub seq: u64,
}

impl TurnEnvelope {
    /// Construct a turn-level envelope (`seq = 0`). `correlation_id` defaults to
    /// `turn_id` when not otherwise threaded.
    pub fn new(turn_id: impl Into<String>, principal: Principal) -> Self {
        let turn_id = turn_id.into();
        Self {
            correlation_id: turn_id.clone(),
            turn_id,
            principal,
            surface: None,
            seq: 0,
        }
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = correlation_id.into();
        self
    }

    pub fn with_surface(mut self, surface: Option<TurnSurface>) -> Self {
        self.surface = surface;
        self
    }

    /// Clone this envelope with `seq` set — used to stamp an emitted event.
    pub fn at_seq(&self, seq: u64) -> Self {
        let mut next = self.clone();
        next.seq = seq;
        next
    }
}

/// The typed event the engine emits during a turn.
///
/// Variants are a faithful superset of the principal-facing
/// `AgentStreamSink` surface so this enum can become *the* event payload the
/// output port carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnEvent {
    /// Streamed answer token(s).
    ContentDelta { delta: String },
    /// Streamed reasoning/thought token(s).
    ReasoningDelta { delta: String },
    /// Non-terminal status line (begin-work / wrapping-up / interim note).
    Progress {
        message: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    /// In-flight assistant scratch was reset before the next model round.
    ScratchReset,
    /// A tool run began.
    ToolRunStarted {
        tool_run_id: String,
        tool_name: String,
        input_summary: String,
        tool_round: usize,
    },
    /// A tool run finished.
    ToolRunFinished {
        tool_run_id: String,
        tool_name: String,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_summary: Option<String>,
        tool_round: usize,
    },
    /// Orchestration notice (debug/telemetry line).
    Notice { message: String },
    /// Terminal: the committed final answer for this turn.
    FinalResponse {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    /// Terminal: Medousa needs operator input (clarifying question / pivot).
    NeedsInput {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    /// Terminal (handoff): substantive mid-task update; the turn ends but the
    /// conversation continues on the principal's reply.
    Checkpoint {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    /// Non-terminal delivery: host acknowledgement while a background worker runs.
    WorkerAck {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_id: Option<String>,
    },
    /// Turn paused awaiting operator approval to extend the tool-round budget.
    BudgetApprovalRequired {
        request_id: String,
        rounds_executed: usize,
        max_tool_rounds: usize,
        requested_rounds: usize,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        progress_summary: Option<String>,
    },
    /// Terminal: the turn failed.
    Error { message: String },
}

impl TurnEvent {
    /// Stable discriminator string. Kept aligned with the live SSE `event_type`
    /// taxonomy so the future projection off the durable log produces identical
    /// wire events.
    pub fn kind(&self) -> &'static str {
        match self {
            TurnEvent::ContentDelta { .. } => "content_delta",
            TurnEvent::ReasoningDelta { .. } => "reasoning_delta",
            TurnEvent::Progress { .. } => "turn_progress",
            TurnEvent::ScratchReset => "scratch_reset",
            TurnEvent::ToolRunStarted { .. } => "tool_started",
            TurnEvent::ToolRunFinished { .. } => "tool_finished",
            TurnEvent::Notice { .. } => "status",
            TurnEvent::FinalResponse { .. } => "final",
            TurnEvent::NeedsInput { .. } => "needs_input",
            TurnEvent::Checkpoint { .. } => "checkpoint",
            TurnEvent::WorkerAck { .. } => "worker_ack",
            TurnEvent::BudgetApprovalRequired { .. } => "budget_approval",
            TurnEvent::Error { .. } => "error",
        }
    }

    /// Whether this event ends the agent turn from the principal's view.
    ///
    /// `WorkerAck` and `BudgetApprovalRequired` are explicitly **non-terminal**
    /// handoffs — matching `turn_ticket::phase_from_stream_event` / the existing
    /// stream-event `terminal` flags.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TurnEvent::FinalResponse { .. }
                | TurnEvent::NeedsInput { .. }
                | TurnEvent::Checkpoint { .. }
                | TurnEvent::Error { .. }
        )
    }

    /// True when this event carries a body that folds into persisted history.
    pub fn contributes_to_history(&self) -> bool {
        matches!(
            self,
            TurnEvent::FinalResponse { .. }
                | TurnEvent::NeedsInput { .. }
                | TurnEvent::Checkpoint { .. }
                | TurnEvent::WorkerAck { .. }
        )
    }
}

/// A `TurnEvent` stamped with its envelope (turn identity + monotonic `seq`).
/// This is the unit appended to the durable per-turn event log (the spine).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SequencedTurnEvent {
    pub envelope: TurnEnvelope,
    pub event: TurnEvent,
}

impl SequencedTurnEvent {
    pub fn seq(&self) -> u64 {
        self.envelope.seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_defaults_correlation_to_turn_id_and_stamps_seq() {
        let env = TurnEnvelope::new("turn-1", Principal::operator());
        assert_eq!(env.correlation_id, "turn-1");
        assert_eq!(env.seq, 0);
        assert_eq!(env.at_seq(7).seq, 7);
        // at_seq does not mutate the base envelope.
        assert_eq!(env.seq, 0);
    }

    #[test]
    fn terminality_matches_stream_event_taxonomy() {
        assert!(TurnEvent::FinalResponse {
            text: "x".into(),
            tool_names: vec![]
        }
        .is_terminal());
        assert!(TurnEvent::Checkpoint {
            text: "x".into(),
            tool_names: vec![]
        }
        .is_terminal());
        assert!(TurnEvent::NeedsInput {
            text: "x".into(),
            tool_names: vec![]
        }
        .is_terminal());
        // Handoffs are non-terminal.
        assert!(!TurnEvent::WorkerAck {
            text: "on it".into(),
            tool_names: vec![],
            work_id: Some("w1".into())
        }
        .is_terminal());
        assert!(!TurnEvent::Progress {
            message: "working".into(),
            tool_names: vec![]
        }
        .is_terminal());
    }

    #[test]
    fn event_roundtrips_through_json_for_journaling() {
        let original = SequencedTurnEvent {
            envelope: TurnEnvelope::new("turn-9", Principal::channel("user-7"))
                .with_correlation_id("corr-9")
                .at_seq(3),
            event: TurnEvent::FinalResponse {
                text: "the answer".into(),
                tool_names: vec!["data_probe".into()],
            },
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: SequencedTurnEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.seq(), 3);
        assert_eq!(decoded.event.kind(), "final");
    }
}
