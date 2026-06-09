//! Host → worker continuity bundle (Phase A worker-continuity plan).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::session::ConversationTurn;

use super::prompt_prep::RecallSnippet;
use super::turn_orchestrator::PreparedTurnPrompt;

const HANDOFF_EXCERPT_TARGET_COUNT: usize = 4;
const HANDOFF_EXCERPT_FALLBACK_COUNT: usize = 2;
const HANDOFF_EXCERPT_BUDGET_CHARS: usize = 2_800;
const HANDOFF_EXCERPT_PER_MESSAGE_MAX: usize = 720;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffConversationExcerpt {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostContinuityBundle {
    pub parent_turn_correlation_id: Option<String>,
    pub identity_summary: Option<String>,
    pub recall_status: String,
    pub recall_snippet_lines: Vec<String>,
    pub ambient_appendix: String,
    pub compiler_summary: String,
    pub vibe_signature: String,
    pub model_avec_line: String,
    pub recent_excerpts: Vec<HandoffConversationExcerpt>,
    pub excerpt_turn_count: usize,
    pub built_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InProcessDelegationRecord {
    pub work_id: String,
    pub session_id: String,
    pub intent: String,
    pub parent_turn_correlation_id: Option<String>,
    pub parent_stream_turn_id: u64,
    pub sequential: bool,
    pub continuity_summary: String,
    pub manuscript_id: Option<String>,
    pub spawned_at: DateTime<Utc>,
}

pub fn build_host_continuity_bundle(
    prepared: &PreparedTurnPrompt,
    conversation: &[ConversationTurn],
    parent_turn_correlation_id: Option<String>,
) -> HostContinuityBundle {
    let recall_status = if !prepared.recall_probe.attempted {
        "not_attempted".to_string()
    } else if prepared.recall_probe.retrieved > 0 {
        "hit".to_string()
    } else {
        "miss".to_string()
    };

    let recall_snippet_lines = format_recall_snippet_lines(&prepared.recall_probe.snippets);
    let recent_excerpts = select_handoff_conversation_excerpts(conversation);
    let excerpt_turn_count = recent_excerpts.len();
    let model_avec_line = format!(
        "stability={:.2} friction={:.2} logic={:.2} autonomy={:.2}",
        prepared.handoff_model_avec.stability,
        prepared.handoff_model_avec.friction,
        prepared.handoff_model_avec.logic,
        prepared.handoff_model_avec.autonomy,
    );

    HostContinuityBundle {
        parent_turn_correlation_id,
        identity_summary: prepared.identity_probe.summary.clone(),
        recall_status,
        recall_snippet_lines,
        ambient_appendix: prepared.ambient_appendix.clone(),
        compiler_summary: prepared.compiler_output.compiler_summary.clone(),
        vibe_signature: prepared.handoff_vibe_signature.clone(),
        model_avec_line,
        recent_excerpts,
        excerpt_turn_count,
        built_at: Utc::now(),
    }
}

fn format_recall_snippet_lines(snippets: &[RecallSnippet]) -> Vec<String> {
    snippets
        .iter()
        .take(3)
        .map(|snippet| {
            format!(
                "key={} summary={} excerpt={}",
                snippet.sync_key,
                truncate_chars(&snippet.context_summary, 120),
                truncate_chars(&snippet.excerpt, 160),
            )
        })
        .collect()
}

pub fn select_handoff_conversation_excerpts(
    conversation: &[ConversationTurn],
) -> Vec<HandoffConversationExcerpt> {
    let mut candidates: Vec<HandoffConversationExcerpt> = conversation
        .iter()
        .filter(|turn| matches!(turn.role.as_str(), "user" | "agent" | "assistant"))
        .filter(|turn| !turn.content.trim().is_empty())
        .map(|turn| HandoffConversationExcerpt {
            role: normalize_handoff_role(&turn.role),
            content: truncate_chars(turn.content.trim(), HANDOFF_EXCERPT_PER_MESSAGE_MAX),
        })
        .collect();

    if candidates.is_empty() {
        return candidates;
    }

    let target = HANDOFF_EXCERPT_TARGET_COUNT.min(candidates.len());
    let mut selected: Vec<HandoffConversationExcerpt> = candidates
        .drain(candidates.len().saturating_sub(target)..)
        .collect();

    while excerpt_payload_chars(&selected) > HANDOFF_EXCERPT_BUDGET_CHARS
        && selected.len() > HANDOFF_EXCERPT_FALLBACK_COUNT
    {
        selected.remove(0);
    }

    while excerpt_payload_chars(&selected) > HANDOFF_EXCERPT_BUDGET_CHARS {
        if let Some(front) = selected.first_mut() {
            front.content = truncate_chars(&front.content, front.content.len() / 2);
        } else {
            break;
        }
    }

    selected
}

fn excerpt_payload_chars(excerpts: &[HandoffConversationExcerpt]) -> usize {
    excerpts
        .iter()
        .map(|entry| entry.role.len() + entry.content.len())
        .sum()
}

fn normalize_handoff_role(role: &str) -> String {
    match role {
        "agent" | "assistant" => "assistant".to_string(),
        _ => "user".to_string(),
    }
}

impl HostContinuityBundle {
    pub fn log_summary(&self) -> String {
        format!(
            "excerpts={} recall_status={} recall_snippets={} identity={} ambient={} vibe=yes",
            self.excerpt_turn_count,
            self.recall_status,
            self.recall_snippet_lines.len(),
            if self.identity_summary.is_some() {
                "ready"
            } else {
                "missing"
            },
            !self.ambient_appendix.trim().is_empty(),
        )
    }

    pub fn format_user_block(&self) -> String {
        let identity = self
            .identity_summary
            .as_deref()
            .map(|summary| truncate_chars(summary, 240))
            .unwrap_or_else(|| "(none)".to_string());
        let recall_lines = if self.recall_snippet_lines.is_empty() {
            "(none)".to_string()
        } else {
            self.recall_snippet_lines.join("\n")
        };
        let ambient = if self.ambient_appendix.trim().is_empty() {
            "(none)".to_string()
        } else {
            self.ambient_appendix.clone()
        };
        let parent_corr = self
            .parent_turn_correlation_id
            .as_deref()
            .unwrap_or("(none)");

        let mut out = format!(
            "[MEDOUSA_CONTINUATION]\n\
             You are still Medousa — same partner as the host lane, now in the workshop executing delegated work. \
             The operator is not in this tool thread; the host will synthesize your receipts back to them.\n\n\
             [HOST_CONTINUITY]\n\
             parent_turn_correlation_id={parent_corr}\n\
             identity_summary={identity}\n\
             recall_status={}\n\
             recall_snippets:\n{recall_lines}\n\
             compiler_summary={}\n\
             vibe_signature={}\n\
             model_avec={}\n\
             ambient:\n{ambient}\n",
            self.recall_status,
            truncate_chars(&self.compiler_summary, 240),
            self.vibe_signature,
            self.model_avec_line,
        );

        if !self.recent_excerpts.is_empty() {
            out.push_str("\n[RECENT_OPERATOR_THREAD]\n");
            for excerpt in &self.recent_excerpts {
                out.push_str(&format!(
                    "- {}: {}\n",
                    excerpt.role,
                    excerpt.content.replace('\n', " ")
                ));
            }
        }

        out
    }
}

pub fn record_in_process_delegation(record: &InProcessDelegationRecord) {
    eprintln!(
        "medousa worker_delegation work_id={} session_id={} intent={} parent_turn={} parent_stream_turn_id={} sequential={} manuscript={} continuity={} spawned_at={}",
        record.work_id,
        record.session_id,
        record.intent,
        record
            .parent_turn_correlation_id
            .as_deref()
            .unwrap_or("-"),
        record.parent_stream_turn_id,
        record.sequential,
        record.manuscript_id.as_deref().unwrap_or("-"),
        record.continuity_summary,
        record.spawned_at.to_rfc3339(),
    );
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
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
    use crate::agent_runtime::prompt_prep::{CheapRecallProbe, IdentityContextProbe};
    use crate::engine_context::ContextCompilerOutput;

    fn sample_prepared() -> PreparedTurnPrompt {
        PreparedTurnPrompt {
            resolved_prompt: "resolved".to_string(),
            pack_note: None,
            verification_state: None,
            recall_probe: CheapRecallProbe {
                attempted: true,
                retrieved: 1,
                snippets: vec![RecallSnippet {
                    sync_key: "k1".to_string(),
                    context_summary: "summary".to_string(),
                    excerpt: "excerpt body".to_string(),
                }],
                ..Default::default()
            },
            identity_probe: IdentityContextProbe {
                attempted: true,
                summary: Some("persona_present=true".to_string()),
                error: None,
            },
            recall_readiness: crate::engine_context::RecallReadiness::Verified,
            compiler_output: ContextCompilerOutput {
                compiled_prompt: "compiled".to_string(),
                compiler_summary: "lane=interactive".to_string(),
                allow_no_tools_fallback: false,
                lane_policy_profile: "interactive",
            },
            handoff_vibe_signature: "Evening focus".to_string(),
            handoff_model_avec: super::super::vibe_signature::default_handoff_model_avec(),
            ambient_appendix: "[MEDOUSA_AMBIENT]\nlocal_time=12:00".to_string(),
        }
    }

    #[test]
    fn selects_recent_excerpts_with_budget_fallback() {
        let conversation = (0..6)
            .map(|idx| ConversationTurn {
                role: if idx % 2 == 0 { "user" } else { "agent" }.to_string(),
                content: format!("message {idx} {}", "x".repeat(400)),
                timestamp: Utc::now(),
                tool_names: vec![],
                answer_state: None,
                parts: None,
                slice_summary: None,
            })
            .collect::<Vec<_>>();

        let excerpts = select_handoff_conversation_excerpts(&conversation);
        assert!(excerpts.len() >= HANDOFF_EXCERPT_FALLBACK_COUNT);
        assert!(excerpts.len() <= HANDOFF_EXCERPT_TARGET_COUNT);
    }

    #[test]
    fn continuity_block_includes_identity_and_thread() {
        let bundle = build_host_continuity_bundle(
            &sample_prepared(),
            &[ConversationTurn {
                role: "user".to_string(),
                content: "find events in phoenix".to_string(),
                timestamp: Utc::now(),
                tool_names: vec![],
                answer_state: None,
                parts: None,
                slice_summary: None,
            }],
            Some("turn-abc".to_string()),
        );
        let block = bundle.format_user_block();
        assert!(block.contains("[MEDOUSA_CONTINUATION]"));
        assert!(block.contains("persona_present=true"));
        assert!(block.contains("[RECENT_OPERATOR_THREAD]"));
    }
}
