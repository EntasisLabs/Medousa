//! Tiered context pools: user lane prefix vs mutable tool lane + turn scratchpad.

use std::hash::{Hash, Hasher};

use genai::chat::ChatMessage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
use stasis::ports::outbound::memory::memory_models::MemoryAvecState;

use super::vibe_signature::HandoffModelAvec;

pub const SCRATCH_PREFIX: &str = "[MEDOUSA_SCRATCH]";
pub const WORKER_HANDOFF_PREFIX: &str = "[MEDOUSA_WORKER_HANDOFF]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnScratchPhase {
    #[default]
    Discover,
    Execute,
    Finalize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkerDelegateScratch {
    pub work_id: String,
    pub intent: String,
}

/// Ephemeral working memory for one host or worker tool-loop execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnScratchpad {
    pub goal: String,
    pub phase: TurnScratchPhase,
    pub step: usize,
    pub last_tools: Vec<String>,
    pub last_error: Option<String>,
    pub open_gaps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegate: Option<WorkerDelegateScratch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub round_digests: Vec<String>,
}

impl TurnScratchpad {
    pub fn from_user_prompt(user_prompt: &str) -> Self {
        Self {
            goal: infer_goal_from_prompt(user_prompt),
            phase: TurnScratchPhase::Discover,
            ..Default::default()
        }
    }

    pub fn set_goal(&mut self, goal: impl Into<String>) {
        let g = goal.into();
        if !g.trim().is_empty() {
            self.goal = g;
        }
    }

    pub fn set_open_gaps(&mut self, gaps: &[String]) {
        self.open_gaps = gaps.to_vec();
    }

    pub fn set_delegate(&mut self, work_id: impl Into<String>, intent: impl Into<String>) {
        self.delegate = Some(WorkerDelegateScratch {
            work_id: work_id.into(),
            intent: intent.into(),
        });
        self.phase = TurnScratchPhase::Finalize;
    }

    pub fn on_tool_round_start(&mut self, round: usize) {
        self.step = round;
        if self.phase == TurnScratchPhase::Discover {
            self.phase = TurnScratchPhase::Execute;
        }
    }

    pub fn record_round_digest(&mut self, tool_results: &[(String, bool)]) {
        let tools: Vec<String> = tool_results
            .iter()
            .map(|(name, ok)| format_tool_digest_entry(name, *ok, None))
            .collect();
        self.apply_round_digest(&tool_results.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(), &tools);
    }

    /// Record a tool round with compact receipt hints so workers inherit host evidence.
    pub fn record_round_digest_from_invocations(&mut self, invocations: &[ToolInvocation]) {
        let tool_results: Vec<(String, bool)> = invocations
            .iter()
            .map(|inv| (inv.tool_name.clone(), tool_output_ok(&inv.tool_output)))
            .collect();
        let tools: Vec<String> = invocations
            .iter()
            .map(|inv| {
                let ok = tool_output_ok(&inv.tool_output);
                let hint = compact_tool_receipt_hint(&inv.tool_name, &inv.tool_output);
                format_tool_digest_entry(&inv.tool_name, ok, hint.as_deref())
            })
            .collect();
        let names: Vec<String> = tool_results.iter().map(|(n, _)| n.clone()).collect();
        self.apply_round_digest(&names, &tools);
    }

    fn apply_round_digest(&mut self, tool_names: &[String], digest_entries: &[String]) {
        self.last_tools = tool_names.to_vec();
        if let Some(name) = tool_names
            .iter()
            .zip(digest_entries.iter())
            .find(|(_, entry)| entry.contains(":fail"))
            .map(|(name, _)| name.clone())
        {
            self.last_error = Some(format!("{name} returned ok=false"));
        } else {
            self.last_error = None;
        }
        let digest = format!(
            "round={} tools=[{}]",
            self.step,
            digest_entries.join(", ")
        );
        self.round_digests.push(digest);
        const MAX_DIGESTS: usize = 12;
        if self.round_digests.len() > MAX_DIGESTS {
            let drain = self.round_digests.len() - MAX_DIGESTS;
            self.round_digests.drain(0..drain);
        }
    }

    pub fn format_control_body(&self, tool_rounds_remaining: usize) -> String {
        let phase = match self.phase {
            TurnScratchPhase::Discover => "discover",
            TurnScratchPhase::Execute => "execute",
            TurnScratchPhase::Finalize => "finalize",
        };
        let mut lines = vec![
            format!("goal={}", truncate_field(&self.goal, 240)),
            format!("phase={phase} step={} rounds_remaining={tool_rounds_remaining}", self.step),
        ];
        if !self.last_tools.is_empty() {
            lines.push(format!("last_tools={}", self.last_tools.join(", ")));
        }
        if let Some(err) = self.last_error.as_deref() {
            lines.push(format!("last_error={err}"));
        }
        if !self.open_gaps.is_empty() {
            lines.push(format!("open_gaps={}", self.open_gaps.join(", ")));
        }
        if let Some(delegate) = self.delegate.as_ref() {
            lines.push(format!(
                "delegate work_id={} intent={}",
                delegate.work_id, delegate.intent
            ));
        }
        if let Some(last) = self.round_digests.last() {
            lines.push(format!("last_digest={last}"));
        }
        lines.join("\n")
    }

    pub fn digest_hash(&self) -> String {
        scratch_digest_hash(self)
    }

    pub fn summarize_for_user_footer(invocations: &[ToolInvocation]) -> Option<String> {
        super::presentation::format_tools_footer_markdown_from_invocations(invocations)
    }
}

/// Host → worker context passed at `cognition_spawn_turn_worker` (Phase 3).
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
}

impl WorkerHandoffCapsule {
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
        let parent_corr = self
            .parent_turn_correlation_id
            .as_deref()
            .unwrap_or("(none)");
        let vibe = self
            .vibe_signature
            .as_deref()
            .unwrap_or("(none)");
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

/// Mutable tool-lane transcript (control, scratch, tool calls/responses).
#[derive(Debug, Clone, Default)]
pub struct ToolLaneState {
    pub messages: Vec<ChatMessage>,
}

/// Host turn: fixed user-visible prefix + growing tool lane.
#[derive(Debug, Clone)]
pub struct HostTurnContext {
    pub user_lane_prefix: Vec<ChatMessage>,
    pub tool_lane: ToolLaneState,
    pub scratchpad: TurnScratchpad,
}

impl HostTurnContext {
    pub fn new(
        prior_messages: Vec<ChatMessage>,
        user_prompt: String,
    ) -> Self {
        let scratchpad = TurnScratchpad::from_user_prompt(&user_prompt);
        let mut user_lane_prefix = prior_messages;
        user_lane_prefix.push(ChatMessage::user(user_prompt));
        Self {
            user_lane_prefix,
            tool_lane: ToolLaneState::default(),
            scratchpad,
        }
    }

    pub fn build_model_messages(&self, system_prompt: Option<&str>) -> Vec<ChatMessage> {
        let mut messages = Vec::with_capacity(
            self.user_lane_prefix.len() + self.tool_lane.messages.len() + 1,
        );
        if let Some(system) = system_prompt.filter(|s| !s.trim().is_empty()) {
            messages.push(ChatMessage::system(system.to_string()));
        }
        messages.extend(self.user_lane_prefix.clone());
        messages.extend(self.tool_lane.messages.clone());
        messages
    }
}

pub fn push_turn_scratch_message(messages: &mut Vec<ChatMessage>, scratchpad: &TurnScratchpad) {
    let body = scratchpad.format_control_body(0);
    push_scratch_body(messages, &body);
}

pub fn push_turn_scratch_message_with_budget(
    messages: &mut Vec<ChatMessage>,
    scratchpad: &TurnScratchpad,
    tool_rounds_remaining: usize,
) {
    let body = scratchpad.format_control_body(tool_rounds_remaining);
    push_scratch_body(messages, &body);
}

fn push_scratch_body(messages: &mut Vec<ChatMessage>, body: &str) {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    messages.push(ChatMessage::system(format!(
        "{SCRATCH_PREFIX}\n{trimmed}"
    )));
}

pub fn tool_output_ok(output: &Value) -> bool {
    matches!(output.get("ok").and_then(|v| v.as_bool()), Some(false)) == false
}

/// Snapshot host scratch for the next `cognition_spawn_turn_worker` (updated each tool round).
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

pub fn tool_results_from_invocations(invocations: &[ToolInvocation]) -> Vec<(String, bool)> {
    invocations
        .iter()
        .map(|inv| (inv.tool_name.clone(), tool_output_ok(&inv.tool_output)))
        .collect()
}

fn default_worker_constraints() -> Vec<String> {
    vec![
        "Complete WORKER_TASK only — host already orchestrated; do not redo its discovery".to_string(),
        "Read HOST_TOOL_DIGESTS before capability_search, resolve, or grapheme modules search".to_string(),
        "Use session_id on all cognition_memory_* tools".to_string(),
        "Ground final worker text in tool receipts; do not invent results".to_string(),
        "Prefer cognition_turn_finish with the full reply when done; or call cognition_turn_prepare_final once before final prose; end early when task is done".to_string(),
    ]
}

fn format_tool_digest_entry(name: &str, ok: bool, hint: Option<&str>) -> String {
    let status = if ok { "ok" } else { "fail" };
    match hint.filter(|value| !value.trim().is_empty()) {
        Some(hint) => format!("{name}:{status} ({hint})"),
        None => format!("{name}:{status}"),
    }
}

/// One-line receipt hint for host→worker handoff digests.
pub fn compact_tool_receipt_hint(tool_name: &str, output: &Value) -> Option<String> {
    if matches!(output.get("ok").and_then(|value| value.as_bool()), Some(false)) {
        return output
            .get("error")
            .or_else(|| output.get("message"))
            .and_then(|value| value.as_str())
            .map(|text| truncate_field(text, 96));
    }

    let normalized = tool_name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "cognition_capability_resolve" | "cognition.capability.resolve" => output
            .get("recommended")
            .and_then(|value| value.get("reference"))
            .and_then(|value| value.as_str())
            .or_else(|| output.get("capability").and_then(|value| value.as_str()))
            .map(|reference| format!("recommended={reference}")),
        "cognition_capability_search" | "cognition.capability.search" => output
            .get("matches")
            .and_then(|value| value.as_array())
            .and_then(|matches| matches.first())
            .and_then(|entry| entry.get("capability"))
            .and_then(|value| value.as_str())
            .map(|capability| format!("top={capability}")),
        "cognition_memory_context" => output
            .get("status")
            .and_then(|value| value.as_str())
            .map(|status| format!("status={status}")),
        "cognition_memory_recall" => output
            .get("hits")
            .or_else(|| output.get("snippets"))
            .and_then(|value| value.as_array())
            .map(|hits| format!("hits={}", hits.len())),
        "cognition_capability_invoke" | "cognition.capability.invoke" => output
            .get("binding")
            .and_then(|value| value.get("reference"))
            .and_then(|value| value.as_str())
            .or_else(|| output.get("capability").and_then(|value| value.as_str()))
            .map(|reference| format!("binding={reference}")),
        "cognition_spawn_turn_worker" => output
            .get("intent")
            .and_then(|value| value.as_str())
            .map(|intent| format!("intent={intent}")),
        _ if normalized.contains("grapheme_modules") => output
            .get("stdout")
            .and_then(|value| value.as_str())
            .and_then(extract_grapheme_module_ids_from_stdout)
            .map(|modules| format!("modules={modules}")),
        _ => None,
    }
}

fn extract_grapheme_module_ids_from_stdout(stdout: &str) -> Option<String> {
    let mut modules = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module_id:") {
            let id = rest.trim();
            if !id.is_empty() {
                modules.push(id.to_string());
            }
        }
    }
    if modules.is_empty() {
        return None;
    }
    modules.sort();
    modules.dedup();
    Some(truncate_field(&modules.join(","), 96))
}

pub fn scratch_digest_hash(scratch: &TurnScratchpad) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    scratch.goal.hash(&mut hasher);
    scratch.step.hash(&mut hasher);
    for digest in &scratch.round_digests {
        digest.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn infer_goal_from_prompt(user_prompt: &str) -> String {
    let collapsed: String = user_prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_field(&collapsed, 240)
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
    use serde_json::json;

    #[test]
    fn scratchpad_records_digest_and_errors() {
        let mut s = TurnScratchpad::from_user_prompt("calibrate my avec");
        s.on_tool_round_start(1);
        s.record_round_digest(&[
            ("cognition_memory_schema".to_string(), true),
            ("cognition_memory_calibrate".to_string(), false),
        ]);
        assert_eq!(s.step, 1);
        assert_eq!(s.phase, TurnScratchPhase::Execute);
        assert!(s.last_error.as_ref().unwrap().contains("calibrate"));
        let body = s.format_control_body(5);
        assert!(body.contains("goal="));
        assert!(body.contains("last_digest=round=1"));
    }

    #[test]
    fn host_context_splits_lanes() {
        let mut ctx = HostTurnContext::new(
            vec![ChatMessage::user("prior")],
            "current ask".to_string(),
        );
        let mut model = ctx.build_model_messages(Some("sys"));
        assert_eq!(model.len(), 3);
        ctx.tool_lane.messages.push(ChatMessage::system("tool-only"));
        model = ctx.build_model_messages(Some("sys"));
        assert_eq!(model.len(), 4);
        assert_eq!(ctx.user_lane_prefix.len(), 2);
        assert_eq!(ctx.tool_lane.messages.len(), 1);
    }

    #[test]
    fn handoff_capsule_seeds_worker_scratch() {
        let mut host = TurnScratchpad::from_user_prompt("calibrate session");
        host.on_tool_round_start(2);
        host.record_round_digest(&[("cognition_memory_schema".to_string(), true)]);
        host.set_open_gaps(&["cognition_memory_calibrate".to_string()]);
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
        cap.apply_spawn("memory.avec_calibrate", "run full calibrate ritual", "work-1");
        let worker = cap.initial_worker_scratch();
        assert_eq!(worker.goal, "run full calibrate ritual");
        assert!(worker.delegate.is_none());
        assert_eq!(worker.open_gaps.len(), 1);
        assert!(cap.worker_tier_user_prompt("[POLICY]").contains(WORKER_HANDOFF_PREFIX));
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
        scratch.record_round_digest_from_invocations(&[ToolInvocation {
            tool_name: "cognition_capability_resolve".to_string(),
            tool_input: json!({}),
            tool_output: json!({
                "capability": "web_research",
                "recommended": { "reference": "web.duckduckgo" }
            }),
        }]);
        assert!(scratch.round_digests[0].contains("recommended=web.duckduckgo"));
    }

    #[test]
    fn compact_hint_surfaces_capability_search_top_match() {
        let hint = compact_tool_receipt_hint(
            "cognition_capability_search",
            &json!({
                "matches": [{ "capability": "web_research", "score": 90 }]
            }),
        );
        assert_eq!(hint.as_deref(), Some("top=web_research"));
    }
}
