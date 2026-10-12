//! Typed authority for runtime-injected wakes.
//!
//! A wake is evidence that something happened. It is not a fresh human goal.
//! Prompt text may describe the event; this module decides which tools and how
//! many rounds that event actually authorizes. Fits the work-unit epic's
//! separation of scope from authority, and `WorkEventSubscription`'s admitted
//! reaction policy: event data cannot mint execution.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CHECKPOINT_RESUME_MARKER: &str = "[MEDOUSA_CHECKPOINT_RESUME]";
pub const PROCESS_EVENT_MARKER: &str = "[MEDOUSA_PROCESS_EVENT]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WakeSource {
    Human,
    NativeCallback,
    TerminalCallback,
    PeerTerminal,
    AgentInbox,
    WorkerResult,
    CheckpointResume,
    SafetyReview,
    ProcessCompleted,
    #[serde(rename = "timed_out")]
    ProcessTimedOut,
}

impl WakeSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::NativeCallback => "native_callback",
            Self::TerminalCallback => "terminal_callback",
            Self::PeerTerminal => "peer_terminal",
            Self::AgentInbox => "agent_inbox",
            Self::WorkerResult => "worker_result",
            Self::CheckpointResume => "checkpoint_resume",
            Self::SafetyReview => "safety_review",
            Self::ProcessCompleted => "process_completed",
            Self::ProcessTimedOut => "timed_out",
        }
    }

    /// Inbox and peer wakes are non-executing. A peer terminal never spawns a
    /// review on its own, and neither does an inbox delivery, a terminal
    /// callback, a worker-result envelope, or a process wake.
    pub fn can_spawn_review(self) -> bool {
        matches!(
            self,
            Self::Human | Self::SafetyReview | Self::CheckpointResume
        )
    }

    pub fn non_executing(self) -> bool {
        matches!(
            self,
            Self::PeerTerminal
                | Self::AgentInbox
                | Self::TerminalCallback
                | Self::WorkerResult
                | Self::ProcessCompleted
                | Self::ProcessTimedOut
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WakeAuthority {
    Acknowledge,
    Report,
    ContinueBounded { rounds: u32 },
    Execute,
}

impl WakeAuthority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Acknowledge => "acknowledge",
            Self::Report => "report",
            Self::ContinueBounded { .. } => "continue_bounded",
            Self::Execute => "execute",
        }
    }

    /// Hard round ceiling. `None` means the turn's ordinary budget applies.
    pub fn round_cap(self) -> Option<u32> {
        match self {
            Self::Acknowledge => Some(2),
            Self::Report => Some(4),
            Self::ContinueBounded { rounds } => Some(rounds.max(1)),
            Self::Execute => None,
        }
    }

    pub fn allowed_action(self) -> &'static str {
        match self {
            Self::Acknowledge => {
                "acknowledge the message and finish the turn, or reply with cognition_agent_message"
            }
            Self::Report => {
                "report the outcome with read-only tools and finish the turn. Do not run shell, mutate, delegate, or spawn a review"
            }
            Self::ContinueBounded { .. } => {
                "continue the saved work within the remaining rounds, then finish"
            }
            Self::Execute => "use the tools this turn's human grant already allows",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WakeGrant {
    pub source: WakeSource,
    pub authority: WakeAuthority,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_gaps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_work: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_rounds: Option<u32>,
}

impl WakeGrant {
    pub fn for_source(source: WakeSource) -> Self {
        let authority = match source {
            WakeSource::Human => WakeAuthority::Execute,
            WakeSource::NativeCallback => WakeAuthority::ContinueBounded { rounds: 2 },
            WakeSource::TerminalCallback
            | WakeSource::PeerTerminal
            | WakeSource::WorkerResult
            | WakeSource::SafetyReview
            | WakeSource::ProcessCompleted
            | WakeSource::ProcessTimedOut => WakeAuthority::Report,
            WakeSource::AgentInbox => WakeAuthority::Acknowledge,
            WakeSource::CheckpointResume => WakeAuthority::ContinueBounded { rounds: 1 },
        };
        let remaining_rounds = authority.round_cap();
        Self {
            source,
            authority,
            open_gaps: Vec::new(),
            pending_work: Vec::new(),
            remaining_rounds,
        }
    }

    pub fn checkpoint(
        remaining_rounds: u32,
        open_gaps: Vec<String>,
        pending_work: Vec<String>,
    ) -> Self {
        let rounds = remaining_rounds.max(1);
        Self {
            source: WakeSource::CheckpointResume,
            authority: if remaining_rounds == 0 {
                WakeAuthority::Report
            } else {
                WakeAuthority::ContinueBounded { rounds }
            },
            open_gaps,
            pending_work,
            remaining_rounds: Some(if remaining_rounds == 0 { 2 } else { rounds }),
        }
    }

    pub fn with_gaps(mut self, gaps: Vec<String>, pending: Vec<String>) -> Self {
        self.open_gaps = gaps;
        self.pending_work = pending;
        self
    }
}

/// Why a tool call is outside the wake. The message names the allowed action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeRefusal {
    pub message: String,
}

impl WakeRefusal {
    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn tool_refusal(grant: &WakeGrant, tool: &str, input: &Value) -> Option<WakeRefusal> {
    if is_safety_review_spawn(tool, input) && !grant.source.can_spawn_review() {
        return Some(WakeRefusal {
            message: format!(
                "wake authority is {} from a {}; spawning a review is outside this grant. Allowed action: {}. A {} wake cannot spawn a safety review.",
                grant.authority.as_str(),
                grant.source.as_str().replace('_', " "),
                grant.authority.allowed_action(),
                grant.source.as_str().replace('_', " "),
            ),
        });
    }
    if tool == "cognition_peer_review" && !source_may_record_acceptance(grant.source) {
        return Some(WakeRefusal {
            message: format!(
                "wake authority is {} from a {}; spawning a review is outside this grant. Allowed action: {}. A {} wake cannot spawn a safety review.",
                grant.authority.as_str(),
                grant.source.as_str().replace('_', " "),
                grant.authority.allowed_action(),
                grant.source.as_str().replace('_', " "),
            ),
        });
    }
    if tool_allowed(grant, tool, input) {
        return None;
    }
    Some(WakeRefusal {
        message: format!(
            "wake authority is {}; {tool} is outside this grant. Allowed action: {}.",
            grant.authority.as_str(),
            grant.authority.allowed_action(),
        ),
    })
}

/// Handoff acceptance is recorded on the native or terminal callback.
/// A peer-terminal wake is not that callback and cannot open the review.
fn source_may_record_acceptance(source: WakeSource) -> bool {
    matches!(
        source,
        WakeSource::Human
            | WakeSource::NativeCallback
            | WakeSource::TerminalCallback
            | WakeSource::CheckpointResume
            | WakeSource::SafetyReview
    )
}

fn tool_allowed(grant: &WakeGrant, tool: &str, input: &Value) -> bool {
    if tool == "cognition_peer_review" {
        return source_may_record_acceptance(grant.source);
    }
    if is_safety_review_spawn(tool, input) {
        if !grant.source.can_spawn_review() {
            return false;
        }
        return match grant.authority {
            WakeAuthority::Execute | WakeAuthority::ContinueBounded { .. } => true,
            WakeAuthority::Report => grant.source == WakeSource::SafetyReview,
            WakeAuthority::Acknowledge => false,
        };
    }
    match grant.authority {
        WakeAuthority::Execute => true,
        WakeAuthority::ContinueBounded { .. } => true,
        WakeAuthority::Acknowledge => is_acknowledge_tool(tool, input),
        WakeAuthority::Report => is_acknowledge_tool(tool, input) || is_report_tool(tool),
    }
}

fn is_acknowledge_tool(tool: &str, input: &Value) -> bool {
    match tool {
        "cognition_turn"
        | "cognition_turn_finish"
        | "cognition_turn_checkpoint"
        | "cognition_turn_update_user" => true,
        "cognition_agent_message" => {
            let action = input
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("reply");
            matches!(action, "reply" | "send" | "message")
        }
        _ => false,
    }
}

fn is_report_tool(tool: &str) -> bool {
    matches!(
        tool,
        "cognition_schema"
            | "cognition_runtime_query"
            | "cognition_memory_query"
            | "cognition_tool_history_summary"
            | "cognition_tool_history_detail"
            | "cognition_chat_history_search"
            | "cognition_chat_history_read"
            | "cognition_tools_discover"
            | "cognition_active_work_discover"
            | "cognition_peer_discover"
            | "cognition_capability"
            | "cognition_shell_status"
            | "cognition_process_status"
            | "cognition_peer_propose"
    )
}

/// A new safety review, or a peer-review call from a wake that is not the
/// handoff callback. `is_safety_review_spawn` is the workshop/runtime case.
pub fn is_review_spawn(tool: &str, input: &Value) -> bool {
    tool == "cognition_peer_review" || is_safety_review_spawn(tool, input)
}

pub fn is_safety_review_spawn(tool: &str, input: &Value) -> bool {
    match tool {
        "cognition_workshop_mutate" => {
            input.get("action").and_then(Value::as_str) == Some("workshop.spawn")
                && payload_mentions_review(input)
        }
        "cognition_runtime_mutate" => payload_mentions_review(input),
        _ => false,
    }
}

pub fn is_material_tool(tool: &str) -> bool {
    !is_acknowledge_tool(tool, &Value::Null)
        && !is_report_tool(tool)
        && tool != "cognition_peer_review"
}

fn payload_mentions_review(input: &Value) -> bool {
    let text = input.to_string().to_ascii_lowercase();
    text.contains("review")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenGapKind {
    InFlightWork,
    UndeliveredMessage,
    RunningProcess,
    PendingApproval,
}

impl OpenGapKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InFlightWork => "in_flight_work",
            Self::UndeliveredMessage => "undelivered_message",
            Self::RunningProcess => "running_process",
            Self::PendingApproval => "pending_approval",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenGap {
    pub id: String,
    pub kind: OpenGapKind,
    pub summary: String,
}

impl OpenGap {
    pub fn line(&self) -> String {
        format!("{}:{}: {}", self.kind.as_str(), self.id, self.summary)
    }
}

/// Gaps the scratchpad does not already mention. Finish and checkpoint must
/// not drop these.
pub fn untracked_gaps<'a>(scratch_gaps: &[String], known: &'a [OpenGap]) -> Vec<&'a OpenGap> {
    known
        .iter()
        .filter(|gap| {
            let line = gap.line();
            !scratch_gaps
                .iter()
                .any(|existing| existing == &line || existing.contains(&gap.id))
        })
        .collect()
}

pub fn finish_refusal(scratch_gaps: &[String], known: &[OpenGap]) -> Option<String> {
    let missing = untracked_gaps(scratch_gaps, known);
    if missing.is_empty() {
        return None;
    }
    let listed = missing
        .iter()
        .map(|gap| gap.line())
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "finish refused: known work is untracked ({listed}). Keep these open_gaps visible, or resolve the work before finishing. Allowed action: report the unfinished work and checkpoint."
    ))
}

pub fn merge_gap_lines(scratch_gaps: &mut Vec<String>, known: &[OpenGap]) {
    for gap in known {
        let line = gap.line();
        if !scratch_gaps
            .iter()
            .any(|existing| existing == &line || existing.contains(&gap.id))
        {
            scratch_gaps.push(line);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkApproval {
    pub work_id: String,
    pub question_id: String,
    pub evidence_digest: String,
    pub approved_by: String,
}

/// A new safety review is admitted only when no approval covers the current
/// evidence, or the evidence changed since that approval.
pub fn review_blocked_by_approval(
    source: WakeSource,
    approval: Option<&WorkApproval>,
    current_digest: &str,
) -> Option<String> {
    if !source.can_spawn_review() {
        return Some(format!(
            "a {} wake cannot spawn a safety review. Allowed action: report the outcome and finish the turn.",
            source.as_str().replace('_', " ")
        ));
    }
    let approval = approval?;
    if approval.evidence_digest == current_digest {
        Some(format!(
            "review question {} on work {} is already approved (evidence unchanged). A new safety review is only spawned when something material changed since the last approved review. Allowed action: report the existing approval.",
            approval.question_id, approval.work_id
        ))
    } else {
        None
    }
}

pub fn is_approval_utterance(text: &str) -> bool {
    matches!(
        text.trim()
            .trim_matches(['.', '!', '"', '\''])
            .to_ascii_lowercase()
            .as_str(),
        "approved" | "approve" | "approval" | "lgtm" | "yes" | "yes approved" | "ship it"
    )
}

pub fn is_checkpoint_resume_prompt(prompt: &str) -> bool {
    prompt.trim_start().starts_with(CHECKPOINT_RESUME_MARKER)
}

/// True when the prompt reissues the human sentence as the thing to do.
/// A checkpoint resume marker is never that, even if it quotes the saved goal.
pub fn replays_human_as_goal(prompt: &str, human: &str) -> bool {
    if is_checkpoint_resume_prompt(prompt) {
        return false;
    }
    let human = human.trim();
    if human.is_empty() {
        return false;
    }
    let prompt = prompt.trim();
    prompt == human || prompt.starts_with(human)
}

pub fn checkpoint_resume_prompt(
    saved_goal: &str,
    open_gaps: &[String],
    pending_work: &[String],
    remaining_rounds: u32,
) -> String {
    let gaps = if open_gaps.is_empty() {
        "(none recorded)".to_string()
    } else {
        open_gaps.join(" | ")
    };
    let pending = if pending_work.is_empty() {
        "(none recorded)".to_string()
    } else {
        pending_work.join(" | ")
    };
    let goal = if saved_goal.trim().is_empty() {
        "(none recorded)"
    } else {
        saved_goal.trim()
    };
    format!(
        "{CHECKPOINT_RESUME_MARKER}\n\
         source=checkpoint_resume\n\
         authority=continue_bounded\n\
         remaining_rounds={remaining_rounds}\n\
         saved_goal={goal}\n\
         open_gaps={gaps}\n\
         pending_work={pending}\n\
         The saved human request is already in the transcript. It is not a new goal and it is not a fresh execution license. Continue only unfinished saved work within the remaining rounds, then finish."
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeTurnMessage {
    /// A different human sentence. Keep it as the turn's goal.
    NewHuman,
    /// Do not append the human sentence. Hand the model the resume prompt and
    /// cap the loop at `round_cap`.
    CheckpointResume { prompt: String, round_cap: u32 },
}

pub fn resume_turn_message(
    user_prompt: &str,
    saved_human_prompt: &str,
    saved_goal: &str,
    open_gaps: &[String],
    pending_work: &[String],
    remaining_rounds: u32,
) -> ResumeTurnMessage {
    if is_checkpoint_resume_prompt(user_prompt)
        || replays_human_as_goal(user_prompt, saved_human_prompt)
    {
        let prompt = if is_checkpoint_resume_prompt(user_prompt) {
            user_prompt.to_string()
        } else {
            checkpoint_resume_prompt(saved_goal, open_gaps, pending_work, remaining_rounds)
        };
        let round_cap = if remaining_rounds == 0 {
            2
        } else {
            remaining_rounds.max(1)
        };
        ResumeTurnMessage::CheckpointResume { prompt, round_cap }
    } else {
        ResumeTurnMessage::NewHuman
    }
}

pub fn process_event_prompt(
    kind: &str,
    handle: &str,
    detail: &str,
    authority: WakeAuthority,
) -> String {
    format!(
        "{PROCESS_EVENT_MARKER}\n\
         source={kind}\n\
         authority={}\n\
         handle={handle}\n\
         detail={detail}\n\
         This event is not a new human goal. Allowed action: {}.",
        authority.as_str(),
        authority.allowed_action(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inbox_and_peer_wakes_are_non_executing_and_refuse_shell_and_review() {
        let inbox = WakeGrant::for_source(WakeSource::AgentInbox);
        assert_eq!(inbox.authority, WakeAuthority::Acknowledge);
        let shell = tool_refusal(&inbox, "cognition_shell_run", &json!({"command": "ls"})).unwrap();
        assert!(shell.message.contains("Allowed action:"));
        assert!(shell.message.contains("acknowledge"));

        let peer = WakeGrant::for_source(WakeSource::PeerTerminal);
        assert_eq!(peer.authority, WakeAuthority::Report);
        assert!(peer.source.non_executing());
        let review = tool_refusal(
            &peer,
            "cognition_workshop_mutate",
            &json!({"action": "workshop.spawn", "intent": "Stasis safety review"}),
        )
        .unwrap();
        assert!(review.message.contains("cannot spawn a safety review"));
        assert!(review.message.contains("Allowed action:"));
        assert!(tool_refusal(&peer, "cognition_runtime_query", &json!({})).is_none());
        assert!(tool_refusal(&peer, "cognition_shell_run", &json!({"argv": ["ls"]})).is_some());
        assert!(tool_refusal(&peer, "cognition_peer_review", &json!({})).is_some());
        let terminal = WakeGrant::for_source(WakeSource::TerminalCallback);
        assert!(tool_refusal(&terminal, "cognition_peer_review", &json!({})).is_none());
        assert!(
            tool_refusal(
                &terminal,
                "cognition_workshop_mutate",
                &json!({"action": "workshop.spawn", "intent": "safety review"}),
            )
            .is_some()
        );
    }

    #[test]
    fn checkpoint_resume_does_not_reissue_the_human_sentence_as_a_goal() {
        let human = "deploy the lab config to both workers";
        assert!(replays_human_as_goal(human, human));
        let prompt = checkpoint_resume_prompt(
            "deploy the lab config",
            &["running_process:proc_1: cargo build".into()],
            &["worker-aa3378a2 handback".into()],
            6,
        );
        assert!(is_checkpoint_resume_prompt(&prompt));
        assert!(!replays_human_as_goal(&prompt, human));
        assert!(prompt.contains("not a new goal"));
        assert!(prompt.contains("remaining_rounds=6"));
        assert!(prompt.contains("proc_1"));
        assert!(prompt.contains("worker-aa3378a2"));
        match resume_turn_message(
            human,
            human,
            "deploy the lab config",
            &["gap".into()],
            &[],
            6,
        ) {
            ResumeTurnMessage::CheckpointResume { prompt, round_cap } => {
                assert_eq!(round_cap, 6);
                assert!(!prompt.starts_with(human));
                assert!(prompt.contains("open_gaps=gap"));
            }
            ResumeTurnMessage::NewHuman => panic!("replay must not be a new human goal"),
        }
        assert!(matches!(
            resume_turn_message("check the other service", human, "deploy", &[], &[], 6),
            ResumeTurnMessage::NewHuman
        ));
    }

    #[test]
    fn approval_closes_the_question_until_evidence_changes() {
        let approval = WorkApproval {
            work_id: "work-1".into(),
            question_id: "review-1".into(),
            evidence_digest: "head-abc".into(),
            approved_by: "human".into(),
        };
        assert!(
            review_blocked_by_approval(WakeSource::PeerTerminal, None, "head-abc")
                .unwrap()
                .contains("cannot spawn")
        );
        assert!(
            review_blocked_by_approval(WakeSource::Human, Some(&approval), "head-abc")
                .unwrap()
                .contains("already approved")
        );
        assert!(
            review_blocked_by_approval(WakeSource::Human, Some(&approval), "head-def").is_none()
        );
        assert!(is_approval_utterance("approved"));
        assert!(is_approval_utterance("  Approved. "));
        assert!(!is_approval_utterance(
            "approved the wrong config, restart it"
        ));
    }

    #[test]
    fn untracked_work_refuses_finish() {
        let known = vec![OpenGap {
            id: "msg_1".into(),
            kind: OpenGapKind::UndeliveredMessage,
            summary: "coder handback".into(),
        }];
        assert!(finish_refusal(&[], &known).unwrap().contains("msg_1"));
        let mut scratch = Vec::new();
        merge_gap_lines(&mut scratch, &known);
        assert!(finish_refusal(&scratch, &known).is_none());
    }

    #[test]
    fn native_callback_is_continue_bounded_not_execute() {
        let grant = WakeGrant::for_source(WakeSource::NativeCallback);
        assert_eq!(
            grant.authority,
            WakeAuthority::ContinueBounded { rounds: 2 }
        );
        assert_eq!(grant.authority.round_cap(), Some(2));
        assert!(tool_refusal(&grant, "cognition_shell_run", &json!({})).is_none());
    }
}
