//! Tiered context pools: user lane prefix vs mutable tool lane + turn scratchpad.

#[cfg(test)]
use genai::chat::ChatMessage;
use serde::{Deserialize, Serialize};
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
use stasis::ports::outbound::memory::memory_models::MemoryAvecState;

use super::vibe_signature::HandoffModelAvec;

pub const WORKER_HANDOFF_PREFIX: &str = "[MEDOUSA_WORKER_HANDOFF]";

pub use medousa_engine::{TurnScratchPhase, TurnScratchpad, WorkerDelegateScratch};
pub use medousa_runtime::turn_context::{
    HostTurnContext, SCRATCH_PREFIX, ToolLaneState, ToolRoundContextProvider,
    compact_tool_receipt_hint, push_turn_scratch_message, push_turn_scratch_message_with_budget,
    record_round_digest_from_invocations, scratch_digest_hash, scratch_seed_for_tool_loop,
    strip_prior_scratch_messages, tool_output_ok, tool_results_from_invocations,
};

pub fn summarize_for_user_footer(invocations: &[ToolInvocation]) -> Option<String> {
    super::presentation::format_tools_footer_markdown_from_invocations(invocations)
}

/// Host → worker context passed at `cognition_workshop_mutate action=workshop.spawn` (Phase 3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHandoffCapsule {
    pub session_id: String,
    pub parent_stream_turn_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_turn_correlation_id: Option<String>,
    pub intent: String,
    pub task_prompt: String,
    pub parent_user_prompt: String,
    pub host_scratch: TurnScratchpad,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_tool_digests: Vec<String>,
    pub scratch_digest_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vibe_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_avec: Option<HandoffModelAvec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_continuity: Option<super::worker_continuity::HostContinuityBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manuscript: Option<crate::identity_manuscript::WorkerManuscriptHandoff>,
    /// Session slice ids with tool history (Phase 8C) — `turn:N` keys for detail drill-down.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relevant_slice_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_history_excerpt: Option<String>,
}

impl WorkerHandoffCapsule {
    #[allow(clippy::too_many_arguments)]
    pub fn from_host_context(
        session_id: &str,
        parent_stream_turn_id: u64,
        parent_turn_correlation_id: Option<String>,
        parent_user_prompt: &str,
        scratch: &TurnScratchpad,
        vibe_signature: Option<String>,
        model_avec: Option<MemoryAvecState>,
        host_continuity: Option<super::worker_continuity::HostContinuityBundle>,
    ) -> Self {
        let model_avec = model_avec.map(Into::into);
        const MAX_HOST_DIGESTS: usize = 6;
        let host_tool_digests: Vec<String> = scratch
            .round_digests
            .iter()
            .rev()
            .take(MAX_HOST_DIGESTS)
            .rev()
            .cloned()
            .collect();
        let host_scratch = scratch.clone();
        Self {
            session_id: session_id.to_string(),
            parent_stream_turn_id,
            parent_turn_correlation_id,
            intent: String::new(),
            task_prompt: String::new(),
            parent_user_prompt: truncate_field(parent_user_prompt, 2000),
            scratch_digest_hash: scratch_digest_hash(&host_scratch),
            host_scratch,
            host_tool_digests,
            constraints: default_worker_constraints(),
            vibe_signature,
            model_avec,
            host_continuity,
            manuscript: None,
            relevant_slice_ids: Vec::new(),
            tool_history_excerpt: None,
        }
    }

    pub fn apply_spawn(&mut self, intent: &str, task: &str, work_id: &str) {
        self.intent = intent.to_string();
        self.task_prompt = task.trim().to_string();
        self.host_scratch.set_delegate(work_id, intent);
        self.scratch_digest_hash = scratch_digest_hash(&self.host_scratch);
    }

    pub fn handoff_summary(&self) -> String {
        format!(
            "Delegated to worker intent={} (host step={}, digests={}, scratch_hash={})",
            self.intent,
            self.host_scratch.step,
            self.host_tool_digests.len(),
            &self.scratch_digest_hash[..self.scratch_digest_hash.len().min(12)]
        )
    }

    pub fn initial_worker_scratch(&self) -> TurnScratchpad {
        let mut scratch = self.host_scratch.clone();
        scratch.delegate = None;
        scratch.phase = TurnScratchPhase::Execute;
        if !self.task_prompt.trim().is_empty() {
            scratch.set_goal(&self.task_prompt);
        }
        scratch
    }

    /// Tier C user-lane body: continuity + handoff + tool policy.
    pub fn worker_tier_user_prompt(&self, tool_loop_policy: &str) -> String {
        let continuity_prefix = self
            .host_continuity
            .as_ref()
            .map(|bundle| format!("{}\n\n", bundle.format_user_block()))
            .unwrap_or_default();
        let manuscript_prefix = self
            .manuscript
            .as_ref()
            .map(|manuscript| {
                format!(
                    "{}\n\n",
                    crate::identity_manuscript::format_worker_manuscript_block(manuscript)
                )
            })
            .unwrap_or_default();
        let digests = if self.host_tool_digests.is_empty() {
            "(none yet)".to_string()
        } else {
            self.host_tool_digests.join("\n")
        };
        let gaps = if self.host_scratch.open_gaps.is_empty() {
            "(none)".to_string()
        } else {
            self.host_scratch.open_gaps.join(", ")
        };
        let constraints = self.constraints.join("; ");
        let slice_ids = if self.relevant_slice_ids.is_empty() {
            "(none)".to_string()
        } else {
            self.relevant_slice_ids.join(", ")
        };
        let slice_excerpt = self
            .tool_history_excerpt
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("(see HOST_TOOL_DIGESTS)");
        let parent_corr = self
            .parent_turn_correlation_id
            .as_deref()
            .unwrap_or("(none)");
        let vibe = self.vibe_signature.as_deref().unwrap_or("(none)");
        let avec_line = self
            .model_avec
            .as_ref()
            .map(|avec| {
                format!(
                    "stability={:.2} friction={:.2} logic={:.2} autonomy={:.2}",
                    avec.stability, avec.friction, avec.logic, avec.autonomy
                )
            })
            .unwrap_or_else(|| "(none)".to_string());
        format!(
            "{continuity_prefix}{manuscript_prefix}{WORKER_HANDOFF_PREFIX}\n\
             session_id={}\n\
             parent_stream_turn_id={}\n\
             parent_turn_correlation_id={parent_corr}\n\
             intent={}\n\
             host_scratch_digest={}\n\
             vibe_signature={vibe}\n\
             model_avec={avec_line}\n\
             constraints={constraints}\n\n\
             HOST_GOAL:\n{host_goal}\n\n\
             HOST_TOOL_DIGESTS (recent host tool rounds, compact):\n{digests}\n\n\
             HOST_TOOL_SLICES (session index — slice_id turn:N):\n{slice_excerpt}\n\
             relevant_slice_ids={slice_ids}\n\n\
             HOST_OPEN_GAPS (finish or honor on worker):\n{gaps}\n\n\
             WORKER_TASK:\n{task}\n\n\
             ORIGINAL_USER_MESSAGE:\n{parent}\n\n\
             {tool_loop_policy}",
            self.session_id,
            self.parent_stream_turn_id,
            self.intent,
            self.scratch_digest_hash,
            host_goal = truncate_field(&self.host_scratch.goal, 240),
            task = self.task_prompt,
            parent = self.parent_user_prompt,
            tool_loop_policy = tool_loop_policy,
        )
    }

    pub fn invocations_summary(invocations: &[ToolInvocation]) -> String {
        invocations
            .iter()
            .take(24)
            .map(|inv| {
                let status = if tool_output_ok(&inv.tool_output) {
                    "ok"
                } else {
                    "fail"
                };
                format!("- {} ({status})", inv.tool_name)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[allow(clippy::too_many_arguments)]
/// Snapshot host scratch for the next `cognition_workshop_mutate action=workshop.spawn` (updated each tool round).
pub async fn publish_host_handoff_snapshot(
    session_id: Option<&str>,
    stream_turn_id: u64,
    parent_turn_correlation_id: Option<String>,
    parent_user_prompt: &str,
    scratch: &TurnScratchpad,
    handoff_slot: Option<&std::sync::Arc<tokio::sync::RwLock<Option<WorkerHandoffCapsule>>>>,
    vibe_signature: Option<String>,
    model_avec: Option<MemoryAvecState>,
    host_continuity: Option<super::worker_continuity::HostContinuityBundle>,
) {
    if parent_user_prompt.trim().is_empty() {
        return;
    }
    let Some(slot) = handoff_slot else {
        return;
    };
    let session_id = session_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or("default");
    let capsule = WorkerHandoffCapsule::from_host_context(
        session_id,
        stream_turn_id,
        parent_turn_correlation_id,
        parent_user_prompt,
        scratch,
        vibe_signature,
        model_avec,
        host_continuity,
    );
    *slot.write().await = Some(capsule);
}

#[derive(Clone)]
pub struct DaemonHostHandoffPort {
    session_id: Option<String>,
    stream_turn_id: u64,
    parent_turn_correlation_id: Option<String>,
    parent_user_prompt: String,
    handoff_slot: std::sync::Arc<tokio::sync::RwLock<Option<WorkerHandoffCapsule>>>,
    vibe_signature: Option<String>,
    model_avec: Option<MemoryAvecState>,
    host_continuity: Option<super::worker_continuity::HostContinuityBundle>,
}

impl DaemonHostHandoffPort {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: Option<String>,
        stream_turn_id: u64,
        parent_turn_correlation_id: Option<String>,
        parent_user_prompt: String,
        handoff_slot: std::sync::Arc<tokio::sync::RwLock<Option<WorkerHandoffCapsule>>>,
        vibe_signature: Option<String>,
        model_avec: Option<MemoryAvecState>,
        host_continuity: Option<super::worker_continuity::HostContinuityBundle>,
    ) -> Self {
        Self {
            session_id,
            stream_turn_id,
            parent_turn_correlation_id,
            parent_user_prompt,
            handoff_slot,
            vibe_signature,
            model_avec,
            host_continuity,
        }
    }
}

impl medousa_runtime::HostHandoffPort for DaemonHostHandoffPort {
    fn publish(&self, scratch: TurnScratchpad) -> medousa_runtime::RuntimePortFuture<()> {
        let session_id = self.session_id.clone();
        let stream_turn_id = self.stream_turn_id;
        let parent_turn_correlation_id = self.parent_turn_correlation_id.clone();
        let parent_user_prompt = self.parent_user_prompt.clone();
        let handoff_slot = self.handoff_slot.clone();
        let vibe_signature = self.vibe_signature.clone();
        let model_avec = self.model_avec;
        let host_continuity = self.host_continuity.clone();
        Box::pin(async move {
            publish_host_handoff_snapshot(
                session_id.as_deref(),
                stream_turn_id,
                parent_turn_correlation_id,
                &parent_user_prompt,
                &scratch,
                Some(&handoff_slot),
                vibe_signature,
                model_avec,
                host_continuity,
            )
            .await;
        })
    }
}

fn default_worker_constraints() -> Vec<String> {
    vec![
        "Complete WORKER_TASK only — host already orchestrated; do not redo its discovery".to_string(),
        "Read HOST_TOOL_DIGESTS before cognition_capability action=capability.find".to_string(),
        "Use session_id on cognition_memory_query and cognition_memory_mutate".to_string(),
        "Ground final worker text in tool receipts; do not invent results".to_string(),
        "After tools: cognition_turn action=turn.finish commits the final reply — naked prose ends the turn with a stub. cognition_turn action=turn.update_user for mid-turn status; cognition_turn action=turn.begin_work before heavy work; cognition_turn action=turn.checkpoint for mid-task handoff; call tools for more work, never plan-only prose".to_string(),
    ]
}

fn truncate_field(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars().take(max_chars) {
        out.push(ch);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_runtime::HostHandoffPort as _;
    use serde_json::json;

    #[test]
    fn scratchpad_records_digest_and_errors() {
        let mut s = TurnScratchpad::from_user_prompt("calibrate my avec");
        s.on_tool_round_start(1);
        s.record_round_digest(&[
            ("cognition_memory_query".to_string(), true),
            ("cognition_memory_mutate".to_string(), false),
        ]);
        assert_eq!(s.step, 1);
        assert_eq!(s.phase, TurnScratchPhase::Execute);
        assert!(s.last_error.as_ref().unwrap().contains("mutate"));
        let body = s.format_control_body(5);
        assert!(body.contains("goal="));
        assert!(body.contains("digests_recent=round=1"));
        assert!(body.contains("tools_this_turn=cognition_memory_query"));
    }

    #[test]
    fn host_context_splits_lanes() {
        let mut ctx =
            HostTurnContext::new(vec![ChatMessage::user("prior")], "current ask".to_string());
        let mut model = ctx.build_model_messages(Some("sys"));
        assert_eq!(model.len(), 3);
        ctx.tool_lane
            .messages
            .push(ChatMessage::system("tool-only"));
        model = ctx.build_model_messages(Some("sys"));
        assert_eq!(model.len(), 4);
        assert_eq!(ctx.user_lane_prefix.len(), 2);
        assert_eq!(ctx.tool_lane.messages.len(), 1);
    }

    #[test]
    fn handoff_capsule_seeds_worker_scratch() {
        let mut host = TurnScratchpad::from_user_prompt("calibrate session");
        host.on_tool_round_start(2);
        host.record_round_digest(&[("cognition_memory_query".to_string(), true)]);
        host.set_open_gaps(&["cognition_memory_mutate".to_string()]);
        let mut cap = WorkerHandoffCapsule::from_host_context(
            "sess-1",
            42,
            Some("corr-abc".to_string()),
            "user asked calibrate",
            &host,
            Some("focused calibration energy".to_string()),
            None,
            None,
        );
        cap.apply_spawn(
            "memory.avec_calibrate",
            "run full calibrate ritual",
            "work-1",
        );
        let worker = cap.initial_worker_scratch();
        assert_eq!(worker.goal, "run full calibrate ritual");
        assert!(worker.delegate.is_none());
        assert_eq!(worker.open_gaps.len(), 1);
        assert!(
            cap.worker_tier_user_prompt("[POLICY]")
                .contains(WORKER_HANDOFF_PREFIX)
        );
    }

    #[tokio::test]
    async fn daemon_handoff_port_publishes_the_portable_scratch_snapshot() {
        let slot = std::sync::Arc::new(tokio::sync::RwLock::new(None));
        let port = DaemonHostHandoffPort::new(
            Some("sess-port".to_string()),
            7,
            Some("corr-port".to_string()),
            "ship the portable loop".to_string(),
            slot.clone(),
            Some("focused".to_string()),
            None,
            None,
        );
        let mut scratch = TurnScratchpad::from_user_prompt("ship the portable loop");
        scratch.on_tool_round_start(2);

        port.publish(scratch).await;

        let published = slot.read().await.clone().expect("handoff capsule");
        assert_eq!(published.session_id, "sess-port");
        assert_eq!(published.parent_stream_turn_id, 7);
        assert_eq!(published.host_scratch.step, 2);
        assert_eq!(
            published.parent_turn_correlation_id.as_deref(),
            Some("corr-port")
        );
    }

    #[test]
    fn tool_output_ok_detects_failure() {
        assert!(!tool_output_ok(&json!({"ok": false, "error": "x"})));
        assert!(tool_output_ok(&json!({"ok": true})));
        assert!(tool_output_ok(&json!({"data": 1})));
    }

    #[test]
    fn digest_includes_capability_resolve_hint() {
        use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

        let mut scratch = TurnScratchpad::default();
        scratch.on_tool_round_start(1);
        record_round_digest_from_invocations(
            &mut scratch,
            &[ToolInvocation {
                tool_name: "cognition_capability".to_string(),
                tool_input: json!({}),
                tool_output: json!({
                    "capability": "web_research",
                    "recommended": { "reference": "web.duckduckgo" }
                }),
            }],
        );
        assert!(scratch.round_digests[0].contains("recommended=web.duckduckgo"));
    }

    #[test]
    fn scratch_messages_dedup_to_one_snapshot() {
        let mut scratch = TurnScratchpad::from_user_prompt("build canvas");
        let mut messages = Vec::new();
        push_turn_scratch_message_with_budget(&mut messages, &scratch, 3);
        scratch.on_tool_round_start(1);
        scratch.record_round_digest(&[("cognition_environment_get".to_string(), true)]);
        push_turn_scratch_message_with_budget(&mut messages, &scratch, 2);
        push_turn_scratch_message_with_budget(&mut messages, &scratch, 1);
        let scratch_count = messages
            .iter()
            .filter(|m| {
                m.content
                    .first_text()
                    .is_some_and(|t| t.starts_with(SCRATCH_PREFIX))
            })
            .count();
        assert_eq!(scratch_count, 1);
        assert!(messages.iter().any(|m| {
            m.content
                .first_text()
                .is_some_and(|t| t.contains("digests_recent="))
        }));
    }

    #[test]
    fn scratch_seed_prefers_in_turn_progress() {
        let session = TurnScratchpad::from_user_prompt("session goal");
        let mut in_turn = session.clone();
        in_turn.on_tool_round_start(2);
        in_turn.record_round_digest(&[("cognition_environment_get".to_string(), true)]);
        let seed = scratch_seed_for_tool_loop(&session, Some(&in_turn));
        assert_eq!(seed.step, 2);
        assert!(!seed.round_digests.is_empty());
    }
}
