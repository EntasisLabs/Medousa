//! Shared display filters used while projecting daemon facts into chat items.

use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::contract::{ChatItem, ChatItemRole, ChatLane, WorkerReturnFact};

fn startup_text_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:(?:(?:interactive|background) turn|ingest) accepted;\s*)?(?:daemon )?agent runtime (?:started|accepted|running|ready|streaming active|completed|finished)[.!]?$|^(?:(?:interactive|background) turn accepted|background turn started)(?:;\s*streaming to attached clients)?[.!]?$",
        )
        .expect("startup text regex")
    })
}

/// Routine lifecycle acknowledgements add no useful progress, including in saved turns.
pub fn is_runtime_startup_text(message: Option<&str>) -> bool {
    let text = message.map(str::trim).unwrap_or("");
    if text.is_empty() {
        return false;
    }
    startup_text_re().is_match(text)
}

pub fn visible_chat_status_line(message: Option<&str>) -> Option<String> {
    let text = message.map(str::trim).filter(|value| !value.is_empty())?;
    if is_runtime_startup_text(Some(text)) {
        None
    } else {
        Some(text.to_string())
    }
}

/// Completed handoff ack with no substance — do not paint as its own bubble.
pub fn is_empty_handoff_shell(item: &ChatItem) -> bool {
    if item.role != ChatItemRole::Assistant {
        return false;
    }
    if item.is_streaming() {
        return false;
    }
    if item.failed.unwrap_or(false) {
        return false;
    }
    if !item.content.trim().is_empty() {
        return false;
    }
    if item
        .tool_runs
        .as_ref()
        .is_some_and(|runs| !runs.is_empty())
    {
        return false;
    }
    if item.tools.as_ref().is_some_and(|tools| !tools.is_empty()) {
        return false;
    }
    if item
        .ui_artifacts
        .as_ref()
        .is_some_and(|artifacts| !artifacts.is_empty())
    {
        return false;
    }
    if item
        .media_attachments
        .as_ref()
        .is_some_and(|media| !media.is_empty())
    {
        return false;
    }
    if item
        .reasoning
        .as_ref()
        .is_some_and(|text| !text.trim().is_empty())
    {
        return false;
    }
    if item
        .budget_request_id
        .as_ref()
        .is_some_and(|id| !id.trim().is_empty())
    {
        return false;
    }
    item.stage_whisper
        .as_ref()
        .is_some_and(|whisper| !whisper.trim().is_empty())
}

fn worker_returns_index(worker_returns: &[WorkerReturnFact]) -> HashMap<&str, &WorkerReturnFact> {
    worker_returns
        .iter()
        .map(|fact| (fact.work_id.as_str(), fact))
        .collect()
}

/// Items to paint in the main chat list: drop empty handoff shells and exact
/// linked worker raw returns. Beats / worker-panel collapse stay in the renderer.
pub fn present_chat_items<'a>(
    items: &'a [ChatItem],
    worker_returns: &[WorkerReturnFact],
) -> Vec<&'a ChatItem> {
    let returns = worker_returns_index(worker_returns);
    items
        .iter()
        .filter(|item| {
            if is_empty_handoff_shell(item) {
                return false;
            }
            // Only suppress a linked raw return, never every worker-lane message:
            // that lane also carries host-authored synthesis and failure/retry UI.
            if item.role != ChatItemRole::Assistant
                || item.lane != Some(ChatLane::Worker)
                || item.work_id.as_ref().is_none_or(|id| id.trim().is_empty())
                || item.failed.unwrap_or(false)
                || item
                    .budget_request_id
                    .as_ref()
                    .is_some_and(|id| !id.trim().is_empty())
            {
                return true;
            }
            let Some(work_id) = item.work_id.as_deref() else {
                return true;
            };
            let Some(thread) = returns.get(work_id) else {
                return true;
            };
            let text = item.content.trim();
            text.is_empty()
                || !thread
                    .returned_texts
                    .iter()
                    .any(|result| result.trim() == text)
        })
        .collect()
}

pub fn present_chat_item_ids(items: &[ChatItem], worker_returns: &[WorkerReturnFact]) -> Vec<String> {
    present_chat_items(items, worker_returns)
        .into_iter()
        .map(|item| item.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ChatItemRole;

    fn item(partial: ChatItem) -> ChatItem {
        partial
    }

    fn assistant(id: &str, content: &str) -> ChatItem {
        ChatItem {
            id: id.into(),
            role: ChatItemRole::Assistant,
            content: content.into(),
            created_at: None,
            streaming: None,
            turn_id: None,
            turn_index: None,
            lane: None,
            ask_job_id: None,
            work_id: None,
            phase: None,
            status_line: None,
            tools: None,
            tool_runs: None,
            segments: None,
            ui_artifacts: None,
            reasoning: None,
            answer_state: None,
            response_provider: None,
            response_model: None,
            stage_whisper: None,
            budget_request_id: None,
            requested_rounds: None,
            failed: None,
            error_line: None,
            error_detail: None,
            speaker_profile_id: None,
            media_attachments: None,
            host_context: None,
            reactions: None,
            live_transcripts: None,
            transcript: None,
        }
    }

    #[test]
    fn detects_whisper_only_handoff_shells() {
        let shell = item(ChatItem {
            stage_whisper: Some("Background worker started".into()),
            streaming: Some(false),
            ..assistant("h", "")
        });
        assert!(is_empty_handoff_shell(&shell));
        assert!(!is_empty_handoff_shell(&item(ChatItem {
            stage_whisper: Some("x".into()),
            streaming: Some(true),
            ..assistant("h", "")
        })));
        assert!(!is_empty_handoff_shell(&item(ChatItem {
            stage_whisper: Some("x".into()),
            ..assistant("h", "hi")
        })));
        assert!(!is_empty_handoff_shell(&item(ChatItem {
            stage_whisper: Some("x".into()),
            budget_request_id: Some("b1".into()),
            ..assistant("h", "")
        })));
        assert!(!is_empty_handoff_shell(&item(ChatItem {
            stage_whisper: Some("x".into()),
            failed: Some(true),
            error_line: Some("e".into()),
            ..assistant("h", "")
        })));
    }

    #[test]
    fn present_drops_shells_and_exact_raw_returns() {
        let user = ChatItem {
            role: ChatItemRole::User,
            ..assistant("u", "go")
        };
        let handoff = item(ChatItem {
            stage_whisper: Some("Worker started".into()),
            ..assistant("h", "")
        });
        let answer = assistant("a", "done");
        let ids = present_chat_item_ids(&[user, handoff, answer], &[]);
        assert_eq!(ids, vec!["u", "a"]);

        let raw = item(ChatItem {
            lane: Some(ChatLane::Worker),
            work_id: Some("w".into()),
            ..assistant("raw", "Full result")
        });
        let synthesis = item(ChatItem {
            lane: Some(ChatLane::Worker),
            work_id: Some("w".into()),
            ..assistant("summary", "My synthesis and decision")
        });
        let failure = item(ChatItem {
            lane: Some(ChatLane::Worker),
            work_id: Some("w".into()),
            failed: Some(true),
            ..assistant("failed", "Full result")
        });
        let unknown = item(ChatItem {
            lane: Some(ChatLane::Worker),
            work_id: Some("other".into()),
            ..assistant("unknown", "Full result")
        });
        let returns = vec![WorkerReturnFact {
            work_id: "w".into(),
            returned_texts: vec!["Full result".into()],
        }];
        let ids = present_chat_item_ids(&[raw, synthesis, failure, unknown], &returns);
        assert_eq!(ids, vec!["summary", "failed", "unknown"]);
    }
}
