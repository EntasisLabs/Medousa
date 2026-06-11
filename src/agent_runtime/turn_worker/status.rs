//! Host-visible worker bus status (active workers block + session listing).

use super::store::{TurnWorkStatus, turn_worker_store};

pub fn append_active_workers_hint(prompt: &str, session_id: &str) -> String {
    match format_active_workers_block(session_id) {
        Some(block) => format!("{prompt}\n\n{block}"),
        None => prompt.to_string(),
    }
}

pub fn format_active_workers_block(session_id: &str) -> Option<String> {
    let active = turn_worker_store()
        .list_for_session(session_id)
        .into_iter()
        .filter(|record| {
            matches!(
                record.status,
                TurnWorkStatus::Pending | TurnWorkStatus::Running
            )
        })
        .collect::<Vec<_>>();

    if active.is_empty() {
        return None;
    }

    let mut lines = vec![
        "[MEDOUSA_ACTIVE_WORKERS]".to_string(),
        format!("session_id={session_id}"),
        format!("count={}", active.len()),
        "Use cognition_turn_worker_status (session_id optional on host turn) for drill-down.".to_string(),
    ];

    for record in active.iter().take(8) {
        let stage = record
            .stage_role
            .as_deref()
            .unwrap_or("-");
        lines.push(format!(
            "- work_id={} status={:?} intent={} stage_role={stage} task={}",
            record.work_id,
            record.status,
            record.intent,
            truncate_task_preview(&record.task_prompt, 96),
        ));
    }

    Some(lines.join("\n"))
}

fn truncate_task_preview(task: &str, max_chars: usize) -> String {
    let trimmed = task.trim().replace('\n', " ");
    if trimmed.chars().count() <= max_chars {
        return trimmed;
    }
    trimmed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::turn_worker::store::{TurnWorkRecord, TurnWorkStatus};

    #[test]
    fn active_workers_block_lists_pending_and_running() {
        let store = turn_worker_store();
        store.insert(TurnWorkRecord {
            work_id: "work-status-test".to_string(),
            session_id: "sess-active".to_string(),
            parent_turn_correlation_id: None,
            parent_stream_turn_id: 0,
            intent: "research".to_string(),
            task_prompt: "Scan the repo".to_string(),
            status: TurnWorkStatus::Running,
            result_text: None,
            tool_names: Vec::new(),
            termination_reason: None,
            error: None,
            user_ack: "On it".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            response_depth_mode: "normal".to_string(),
            max_tool_rounds: 8,
            delivery_target: None,
            parent_user_prompt: None,
            handoff_capsule: None,
            worker_scratch: None,
            synthesis_delivered: false,
            stasis_job_id: Some("work-status-test".to_string()),
            thread_id: None,
            stage_role: Some("extractor".to_string()),
            model_hint: None,
            manuscript_id: None,
            branch_group_id: None,
            archived: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });

        let block = format_active_workers_block("sess-active").expect("block");
        assert!(block.contains("[MEDOUSA_ACTIVE_WORKERS]"));
        assert!(block.contains("work-status-test"));
        assert!(block.contains("extractor"));
    }
}
