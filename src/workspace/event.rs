//! Workspace activity feed event helpers.

use chrono::Utc;
use uuid::Uuid;

use crate::daemon_api::{
    WorkBoardColumn, WorkCardDetail, WorkCardKind, WorkspaceEvent, WorkspaceEventActor,
    WorkspaceEventKind, WorkspaceEventRef,
};

pub fn new_event_id() -> String {
    format!("wse:{}", Uuid::new_v4().simple())
}

pub fn append_card_ref(refs: &mut Vec<WorkspaceEventRef>, card_id: &str) {
    refs.push(WorkspaceEventRef {
        ref_type: "card".to_string(),
        ref_id: card_id.to_string(),
    });
}

pub fn event_for_column_transition(
    detail: &WorkCardDetail,
    previous: Option<WorkBoardColumn>,
    current: WorkBoardColumn,
) -> Option<WorkspaceEvent> {
    let card = &detail.card;
    let mut refs = Vec::new();
    append_card_ref(&mut refs, &card.id.0);

    let detail_line = resolve_detail_line(detail);
    let (kind, summary) = match (previous, current) {
        (None, WorkBoardColumn::Backlog) => (
            WorkspaceEventKind::JobEnqueued,
            format!("Queued — {detail_line}"),
        ),
        (Some(WorkBoardColumn::Backlog), WorkBoardColumn::InFlight) => (
            WorkspaceEventKind::JobStarted,
            format!("Started — {detail_line}"),
        ),
        (_, WorkBoardColumn::WrappingUp) => (
            WorkspaceEventKind::WorkWrappingUp,
            format!(
                "Wrapping up — {detail_line} ({})",
                card.status_label
            ),
        ),
        (Some(WorkBoardColumn::WrappingUp), WorkBoardColumn::Done) => (
            WorkspaceEventKind::WorkUnblocked,
            format!("Finished — {detail_line}"),
        ),
        (_, WorkBoardColumn::Done) => (
            WorkspaceEventKind::JobSucceeded,
            format!("Completed — {detail_line}"),
        ),
        (_, WorkBoardColumn::Blocked) => (
            WorkspaceEventKind::JobFailed,
            format!("Blocked — {detail_line}"),
        ),
        (None, WorkBoardColumn::InFlight) => (
            WorkspaceEventKind::WorkDelegated,
            format!("Delegated — {detail_line}"),
        ),
        _ => return None,
    };

    let context_line = build_context_line(detail, kind);
    let intent = resolve_intent(detail);
    let tool_names = detail.tool_names.clone().unwrap_or_default();

    Some(WorkspaceEvent {
        id: new_event_id(),
        timestamp_utc: Utc::now(),
        kind,
        actor: WorkspaceEventActor::System,
        summary,
        refs,
        detail_line: Some(detail_line),
        context_line,
        intent,
        tool_names,
    })
}

pub fn event_for_vault_link(card_id: &str, vault_path: &str) -> Option<WorkspaceEvent> {
    let mut refs = Vec::new();
    append_card_ref(&mut refs, card_id);
    refs.push(WorkspaceEventRef {
        ref_type: "vault_path".to_string(),
        ref_id: vault_path.to_string(),
    });

    let detail_line = humanize_vault_path(vault_path);
    let context_line = Some(vault_path.to_string());

    Some(WorkspaceEvent {
        id: new_event_id(),
        timestamp_utc: Utc::now(),
        kind: WorkspaceEventKind::VaultNoteUpdated,
        actor: WorkspaceEventActor::Operator,
        summary: format!("Linked vault note {detail_line}"),
        refs,
        detail_line: Some(detail_line),
        context_line,
        intent: None,
        tool_names: Vec::new(),
    })
}

pub fn filter_events_by_card<'a>(
    events: &'a [WorkspaceEvent],
    card_id: &str,
) -> Vec<&'a WorkspaceEvent> {
    events
        .iter()
        .filter(|event| {
            event.refs.iter().any(|reference| {
                reference.ref_type == "card" && reference.ref_id == card_id
            })
        })
        .collect()
}

fn is_slug_like_title(title: &str) -> bool {
    let trimmed = title.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 28
        && !trimmed.contains(' ')
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn is_generic_workflow_title(title: &str) -> bool {
    let lower = title.trim().to_ascii_lowercase();
    lower.starts_with("workflow:") || lower == "workflow: cognitio"
}

fn truncate_line(text: &str, max: usize) -> String {
    let trimmed = text.trim().replace('\n', " ");
    if trimmed.chars().count() <= max {
        trimmed
    } else {
        format!("{}…", trimmed.chars().take(max).collect::<String>())
    }
}

fn format_worker_intent_label(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "research" | "delegate.research" | "web" | "websearch" => "Research".to_string(),
        "memory.context" | "memory_context" => "Memory".to_string(),
        "memory.avec_calibrate" | "avec_calibrate" => "Memory calibration".to_string(),
        "general" | "default" => "General task".to_string(),
        _ => raw.trim().replace('_', " "),
    }
}

fn format_job_family(job_type: &str) -> String {
    if job_type == "daemon.ask" {
        return "Background ask".to_string();
    }
    job_type
        .split('.')
        .next_back()
        .unwrap_or(job_type)
        .replace('_', " ")
}

fn format_tool_name(tool: &str) -> String {
    tool.strip_prefix("cognition_")
        .unwrap_or(tool)
        .replace('_', " ")
}

fn format_activity_tools(names: &[String], max: usize) -> String {
    if names.is_empty() {
        return String::new();
    }
    let mut out = names
        .iter()
        .take(max)
        .map(|name| format_tool_name(name))
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > max {
        out.push_str(&format!(" +{}", names.len() - max));
    }
    out
}

fn humanize_vault_path(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.trim_end_matches(".md").replace('-', " ")
}

pub fn resolve_detail_line(detail: &WorkCardDetail) -> String {
    let title = detail.card.title.trim();

    if detail.kind == WorkCardKind::TurnWorker {
        if let Some(task) = detail
            .task_line
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if is_slug_like_title(title) || is_generic_workflow_title(title) {
                return truncate_line(task, 88);
            }
        }
        if !title.is_empty() && !is_slug_like_title(title) && !is_generic_workflow_title(title) {
            return title.to_string();
        }
        if let Some(subtitle) = detail.subtitle.as_deref() {
            let label = format_worker_intent_label(subtitle);
            if !label.is_empty() {
                return label;
            }
        }
    }

    if detail.kind == WorkCardKind::AskJob {
        if !title.is_empty() {
            return truncate_line(title, 88);
        }
        return "background ask".to_string();
    }

    if !title.is_empty() && !is_generic_workflow_title(title) {
        return title.to_string();
    }

    if let Some(job_type) = detail.job_type.as_deref() {
        let family = format_job_family(job_type);
        if !family.is_empty() {
            return family;
        }
    }

    title.to_string()
}

fn resolve_intent(detail: &WorkCardDetail) -> Option<String> {
    match detail.kind {
        WorkCardKind::TurnWorker => detail.subtitle.clone(),
        WorkCardKind::StasisJob | WorkCardKind::AskJob => detail.job_type.clone(),
        _ => None,
    }
}

fn build_context_line(detail: &WorkCardDetail, kind: WorkspaceEventKind) -> Option<String> {
    let mut parts = Vec::new();
    let detail_line = resolve_detail_line(detail).to_ascii_lowercase();

    if detail.kind == WorkCardKind::TurnWorker {
        if let Some(subtitle) = detail.subtitle.as_deref() {
            let label = format_worker_intent_label(subtitle);
            if !label.is_empty() && label.to_ascii_lowercase() != detail_line {
                parts.push(label);
            }
        }
        if let Some(tools) = detail.tool_names.as_ref() {
            let formatted = format_activity_tools(tools, 3);
            if !formatted.is_empty() {
                parts.push(formatted);
            }
        }
    } else if detail.kind == WorkCardKind::AskJob {
        parts.push("Background ask".to_string());
    } else if let Some(job_type) = detail.job_type.as_deref() {
        let family = format_job_family(job_type);
        if !family.is_empty() {
            parts.push(family);
        }
    }

    if kind == WorkspaceEventKind::WorkWrappingUp && !detail.wrapping_up_reasons.is_empty() {
        parts.push(
            detail
                .wrapping_up_reasons
                .iter()
                .map(|reason| reason.replace('_', " "))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_api::{WorkCard, WorkCardAssociations, WorkCardId};

    fn sample_turn_worker_detail() -> WorkCardDetail {
        WorkCardDetail {
            card: WorkCard {
                id: WorkCardId("work-1".to_string()),
                column: WorkBoardColumn::Done,
                title: "cognitio".to_string(),
                status_label: "completed".to_string(),
                created_at_utc: Utc::now(),
                updated_at_utc: Utc::now(),
            },
            kind: WorkCardKind::TurnWorker,
            subtitle: Some("research".to_string()),
            session_id: Some("sess-1".to_string()),
            correlation_id: None,
            manuscript_id: None,
            job_id: None,
            work_id: Some("work-1".to_string()),
            job_type: None,
            user_ack: Some("cognitio".to_string()),
            wrapping_up_reasons: Vec::new(),
            terminal: true,
            error: None,
            result_excerpt: None,
            task_line: Some(
                "Researching the latest OpenClaw trends give me a moment.".to_string(),
            ),
            tool_names: Some(vec![
                "cognition_capability_invoke".to_string(),
                "cognition_grapheme_run".to_string(),
            ]),
            associations: WorkCardAssociations::default(),
        }
    }

    #[test]
    fn resolve_detail_line_prefers_task_when_user_ack_is_slug() {
        let detail = sample_turn_worker_detail();
        assert_eq!(
            resolve_detail_line(&detail),
            "Researching the latest OpenClaw trends give me a moment."
        );
    }

    #[test]
    fn column_transition_emits_structured_metadata() {
        let detail = sample_turn_worker_detail();
        let event = event_for_column_transition(
            &detail,
            Some(WorkBoardColumn::InFlight),
            WorkBoardColumn::Done,
        )
        .expect("event");

        assert_eq!(
            event.detail_line.as_deref(),
            Some("Researching the latest OpenClaw trends give me a moment.")
        );
        assert_eq!(event.intent.as_deref(), Some("research"));
        assert_eq!(event.tool_names.len(), 2);
        assert!(event
            .context_line
            .as_deref()
            .is_some_and(|line| line.contains("capability invoke")));
    }
}
