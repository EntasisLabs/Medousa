//! Focused outbound ports consumed by the foreground loop.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use medousa_engine::TurnScratchpad;
use serde_json::Value;
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::loop_state::TurnLedgerRecord;

pub type RuntimePortFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

pub trait TurnLedgerSink: Send + Sync {
    fn persist(&self, record: &TurnLedgerRecord);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSteerMessage {
    pub text: String,
    pub speaker_profile_id: Option<String>,
}

/// Full-runtime worker cancellation and steer inbox.
pub trait DelegationControlPort: Send + Sync {
    fn is_cancelled(&self, work_id: &str) -> bool;
    fn drain_steer_messages(&self, work_id: &str) -> Vec<TurnSteerMessage>;
}

#[derive(Debug, Clone)]
pub struct ToolRunStart {
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_round: usize,
}

#[derive(Debug, Clone)]
pub struct ToolRunFinish {
    pub tool_run_id: String,
    pub tool_round: usize,
    pub invocation: ToolInvocation,
}

/// Presentation-only tool lifecycle events. Authoritative receipts stay in the loop.
pub trait ToolRunEventPort: Send + Sync {
    fn started(&self, event: ToolRunStart) -> RuntimePortFuture<String>;
    fn finished(&self, event: ToolRunFinish) -> RuntimePortFuture<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelResponseCompleted {
    pub model_round: usize,
}

/// Optional fence after every complete model response and before its tools or
/// completion decision. Hosts use it to flush streamed prose without inferring
/// response boundaries from chunks or final aggregate text.
pub trait ModelResponseEventPort: Send + Sync {
    fn completed(&self, event: ModelResponseCompleted) -> RuntimePortFuture<()>;
}

/// Foreground-loop presentation events. Runtime state and receipts remain
/// authoritative when this optional port is absent.
pub trait TurnPresentationPort: Send + Sync {
    fn notice(&self, message: String) -> RuntimePortFuture<()>;
    fn scratch_reset(&self, stream_turn_id: u64) -> RuntimePortFuture<()>;
    fn turn_progress(
        &self,
        stream_turn_id: u64,
        message: String,
        tool_names: Vec<String>,
    ) -> RuntimePortFuture<()>;
    fn pack_hold(
        &self,
        stream_turn_id: u64,
        fragments: Vec<String>,
        tool_names: Vec<String>,
    ) -> RuntimePortFuture<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnBudgetApprovalRequest {
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub requested_rounds: usize,
    pub reason: String,
    pub progress_summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnBudgetApprovalResolution {
    Approved { granted_rounds: usize },
    Denied,
}

pub struct PendingTurnBudgetApproval {
    pub request_id: String,
    resolution: RuntimePortFuture<TurnBudgetApprovalResolution>,
}

impl PendingTurnBudgetApproval {
    pub fn new(
        request_id: String,
        resolution: RuntimePortFuture<TurnBudgetApprovalResolution>,
    ) -> Self {
        Self {
            request_id,
            resolution,
        }
    }

    pub async fn resolve(self) -> TurnBudgetApprovalResolution {
        self.resolution.await
    }
}

/// Two-stage approval lets the loop checkpoint the outstanding request before waiting.
pub trait TurnBudgetApprovalPort: Send + Sync {
    fn begin(
        &self,
        request: TurnBudgetApprovalRequest,
    ) -> RuntimePortFuture<Result<PendingTurnBudgetApproval, String>>;
}

/// Full-runtime publication of the latest host scratch for worker handoff.
pub trait HostHandoffPort: Send + Sync {
    fn publish(&self, scratch: TurnScratchpad) -> RuntimePortFuture<()>;
}

pub struct PerceptionEvidenceRequest<'a> {
    pub tool_name: &'a str,
    pub source_call_id: Option<&'a str>,
    pub output: &'a Value,
    pub failed: bool,
}

#[derive(Debug, Clone)]
pub struct PersistedPerceptionEvidence {
    pub receipt: Value,
    pub logical_bytes: u64,
    pub durable_receipt_staged: bool,
    pub receipt_stage_error: Option<String>,
}

/// Optional Coder storage for oversized, non-replayable model observations.
pub trait PerceptionEvidencePort: Send + Sync {
    fn persist(
        &self,
        request: PerceptionEvidenceRequest<'_>,
    ) -> Result<PersistedPerceptionEvidence, String>;
}

/// Cloneable per-turn adapter composition.
///
/// Stasis capabilities and queue ownership decide which work a daemon accepts.
/// This value only supplies the adapters needed by an already-admitted turn;
/// an absent adapter cannot create a second runtime capability source of truth.
#[derive(Clone, Default)]
pub struct RuntimePorts {
    ledger_sink: Option<Arc<dyn TurnLedgerSink>>,
    delegation_control: Option<Arc<dyn DelegationControlPort>>,
    tool_run_events: Option<Arc<dyn ToolRunEventPort>>,
    model_response_events: Option<Arc<dyn ModelResponseEventPort>>,
    turn_presentation: Option<Arc<dyn TurnPresentationPort>>,
    budget_approval: Option<Arc<dyn TurnBudgetApprovalPort>>,
    host_handoff: Option<Arc<dyn HostHandoffPort>>,
    perception_evidence: Option<Arc<dyn PerceptionEvidencePort>>,
}

impl RuntimePorts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_ledger_sink(mut self, sink: Arc<dyn TurnLedgerSink>) -> Self {
        self.ledger_sink = Some(sink);
        self
    }

    pub fn with_optional_ledger_sink(mut self, sink: Option<Arc<dyn TurnLedgerSink>>) -> Self {
        self.ledger_sink = sink;
        self
    }

    pub fn with_delegation_control(mut self, control: Arc<dyn DelegationControlPort>) -> Self {
        self.delegation_control = Some(control);
        self
    }

    pub fn with_tool_run_events(mut self, events: Arc<dyn ToolRunEventPort>) -> Self {
        self.tool_run_events = Some(events);
        self
    }

    pub fn with_optional_tool_run_events(
        mut self,
        events: Option<Arc<dyn ToolRunEventPort>>,
    ) -> Self {
        self.tool_run_events = events;
        self
    }

    pub fn with_model_response_events(mut self, events: Arc<dyn ModelResponseEventPort>) -> Self {
        self.model_response_events = Some(events);
        self
    }

    pub fn with_optional_model_response_events(
        mut self,
        events: Option<Arc<dyn ModelResponseEventPort>>,
    ) -> Self {
        self.model_response_events = events;
        self
    }

    pub fn with_turn_presentation(mut self, presentation: Arc<dyn TurnPresentationPort>) -> Self {
        self.turn_presentation = Some(presentation);
        self
    }

    pub fn with_optional_turn_presentation(
        mut self,
        presentation: Option<Arc<dyn TurnPresentationPort>>,
    ) -> Self {
        self.turn_presentation = presentation;
        self
    }

    pub fn with_budget_approval(mut self, approval: Arc<dyn TurnBudgetApprovalPort>) -> Self {
        self.budget_approval = Some(approval);
        self
    }

    pub fn with_optional_budget_approval(
        mut self,
        approval: Option<Arc<dyn TurnBudgetApprovalPort>>,
    ) -> Self {
        self.budget_approval = approval;
        self
    }

    pub fn with_host_handoff(mut self, handoff: Arc<dyn HostHandoffPort>) -> Self {
        self.host_handoff = Some(handoff);
        self
    }

    pub fn with_perception_evidence(mut self, evidence: Arc<dyn PerceptionEvidencePort>) -> Self {
        self.perception_evidence = Some(evidence);
        self
    }

    pub fn ledger_sink(&self) -> Option<&dyn TurnLedgerSink> {
        self.ledger_sink.as_deref()
    }

    pub fn delegation_control(&self) -> Option<&dyn DelegationControlPort> {
        self.delegation_control.as_deref()
    }

    pub fn tool_run_events(&self) -> Option<&dyn ToolRunEventPort> {
        self.tool_run_events.as_deref()
    }

    pub fn model_response_events(&self) -> Option<&dyn ModelResponseEventPort> {
        self.model_response_events.as_deref()
    }

    pub fn turn_presentation(&self) -> Option<&dyn TurnPresentationPort> {
        self.turn_presentation.as_deref()
    }

    pub fn budget_approval(&self) -> Option<&dyn TurnBudgetApprovalPort> {
        self.budget_approval.as_deref()
    }

    pub fn host_handoff(&self) -> Option<&dyn HostHandoffPort> {
        self.host_handoff.as_deref()
    }

    pub fn perception_evidence(&self) -> Option<Arc<dyn PerceptionEvidencePort>> {
        self.perception_evidence.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopDelegation;

    impl DelegationControlPort for NoopDelegation {
        fn is_cancelled(&self, _work_id: &str) -> bool {
            false
        }

        fn drain_steer_messages(&self, _work_id: &str) -> Vec<TurnSteerMessage> {
            Vec::new()
        }
    }

    struct NoopHandoff;

    impl HostHandoffPort for NoopHandoff {
        fn publish(&self, _scratch: TurnScratchpad) -> RuntimePortFuture<()> {
            Box::pin(async {})
        }
    }

    struct NoopToolEvents;

    impl ToolRunEventPort for NoopToolEvents {
        fn started(&self, _event: ToolRunStart) -> RuntimePortFuture<String> {
            Box::pin(async { "run".to_string() })
        }

        fn finished(&self, _event: ToolRunFinish) -> RuntimePortFuture<()> {
            Box::pin(async {})
        }
    }

    struct NoopModelResponses;

    impl ModelResponseEventPort for NoopModelResponses {
        fn completed(&self, _event: ModelResponseCompleted) -> RuntimePortFuture<()> {
            Box::pin(async {})
        }
    }

    struct NoopPresentation;

    impl TurnPresentationPort for NoopPresentation {
        fn notice(&self, _message: String) -> RuntimePortFuture<()> {
            Box::pin(async {})
        }

        fn scratch_reset(&self, _stream_turn_id: u64) -> RuntimePortFuture<()> {
            Box::pin(async {})
        }

        fn turn_progress(
            &self,
            _stream_turn_id: u64,
            _message: String,
            _tool_names: Vec<String>,
        ) -> RuntimePortFuture<()> {
            Box::pin(async {})
        }

        fn pack_hold(
            &self,
            _stream_turn_id: u64,
            _fragments: Vec<String>,
            _tool_names: Vec<String>,
        ) -> RuntimePortFuture<()> {
            Box::pin(async {})
        }
    }

    struct NoopApproval;

    impl TurnBudgetApprovalPort for NoopApproval {
        fn begin(
            &self,
            _request: TurnBudgetApprovalRequest,
        ) -> RuntimePortFuture<Result<PendingTurnBudgetApproval, String>> {
            Box::pin(async { Err("not configured".to_string()) })
        }
    }

    struct NoopEvidence;

    impl PerceptionEvidencePort for NoopEvidence {
        fn persist(
            &self,
            _request: PerceptionEvidenceRequest<'_>,
        ) -> Result<PersistedPerceptionEvidence, String> {
            Err("not configured".to_string())
        }
    }

    #[test]
    fn empty_composition_has_no_daemon_only_ports() {
        let ports = RuntimePorts::new();
        assert!(ports.delegation_control().is_none());
        assert!(ports.host_handoff().is_none());
        assert!(ports.perception_evidence().is_none());
        assert!(ports.model_response_events().is_none());
    }

    #[test]
    fn composition_accepts_model_response_fences() {
        let ports = RuntimePorts::new().with_model_response_events(Arc::new(NoopModelResponses));
        assert!(ports.model_response_events().is_some());
    }

    #[test]
    fn composition_accepts_worker_control_port() {
        let ports = RuntimePorts::new().with_delegation_control(Arc::new(NoopDelegation));
        assert!(ports.delegation_control().is_some());
    }

    #[test]
    fn composition_accepts_host_handoff_publication() {
        let ports = RuntimePorts::new().with_host_handoff(Arc::new(NoopHandoff));
        assert!(ports.host_handoff().is_some());
    }

    #[test]
    fn composition_accepts_foreground_presentation_and_approval_ports() {
        let ports = RuntimePorts::new()
            .with_tool_run_events(Arc::new(NoopToolEvents))
            .with_turn_presentation(Arc::new(NoopPresentation))
            .with_budget_approval(Arc::new(NoopApproval));
        assert!(ports.tool_run_events().is_some());
        assert!(ports.turn_presentation().is_some());
        assert!(ports.budget_approval().is_some());
    }

    #[test]
    fn composition_accepts_coder_evidence_storage() {
        let ports = RuntimePorts::new().with_perception_evidence(Arc::new(NoopEvidence));
        assert!(ports.perception_evidence().is_some());
    }
}
