//! Adapters from existing runtime signals into environment feed bus.

use medousa_types::feed::{FeedRef, WORKSHOP_PULSE_FEED_ID};
use serde_json::{json, Value};

use crate::agent_runtime::turn_worker::{TurnWorkRecord, TurnWorkStatus};
use crate::feed_bus::{publish, FeedPublishRequest};

pub const WORKSHOP_PULSE: &str = WORKSHOP_PULSE_FEED_ID;

pub async fn publish_workshop_started(record: &TurnWorkRecord) {
    let _ = publish(FeedPublishRequest {
        profile_id: None,
        feed_id: WORKSHOP_PULSE.to_string(),
        source: medousa_types::feed::FeedSource::BoundWorkshop,
        summary: format!("Bound workshop started — {}", truncate(&record.task_prompt, 80)),
        refs: workshop_refs(record),
        payload_slice: Some(json!({
            "phase": "started",
            "goal": truncate(&record.task_prompt, 160),
            "workId": record.work_id,
            "sessionId": record.session_id,
        })),
        payload_max_bytes: None,
    })
    .await;
}

pub async fn publish_workshop_working(
    record: &TurnWorkRecord,
    round: u32,
    tools: &[String],
) {
    let tool_list: Vec<_> = tools.iter().take(12).cloned().collect();
    let _ = publish(FeedPublishRequest {
        profile_id: None,
        feed_id: WORKSHOP_PULSE.to_string(),
        source: medousa_types::feed::FeedSource::BoundWorkshop,
        summary: format!("Workshop round {round} — {}", tool_list.join(", ")),
        refs: workshop_refs(record),
        payload_slice: Some(json!({
            "phase": "working",
            "round": round,
            "tools": tool_list,
            "workId": record.work_id,
        })),
        payload_max_bytes: None,
    })
    .await;
}

pub async fn publish_workshop_synthesis(record: &TurnWorkRecord, excerpt: &str) {
    let _ = publish(FeedPublishRequest {
        profile_id: None,
        feed_id: WORKSHOP_PULSE.to_string(),
        source: medousa_types::feed::FeedSource::BoundWorkshop,
        summary: format!("Synthesis ready — {}", truncate(excerpt, 80)),
        refs: workshop_refs(record),
        payload_slice: Some(json!({
            "phase": "synthesis",
            "excerpt": truncate(excerpt, 240),
            "workId": record.work_id,
        })),
        payload_max_bytes: None,
    })
    .await;
}

pub async fn publish_workshop_terminal(record: &TurnWorkRecord, phase: &str, excerpt: Option<&str>) {
    let status = match record.status {
        TurnWorkStatus::Completed => "done",
        TurnWorkStatus::Failed => "failed",
        TurnWorkStatus::Cancelled => "cancelled",
        _ => "wrapping_up",
    };
    let mut payload = json!({
        "phase": phase,
        "status": status,
        "workId": record.work_id,
    });
    if let Some(excerpt) = excerpt.filter(|value| !value.trim().is_empty()) {
        payload["excerpt"] = Value::String(truncate(excerpt, 240));
    }
    let _ = publish(FeedPublishRequest {
        profile_id: None,
        feed_id: WORKSHOP_PULSE.to_string(),
        source: medousa_types::feed::FeedSource::BoundWorkshop,
        summary: format!("Workshop {status}"),
        refs: workshop_refs(record),
        payload_slice: Some(payload),
        payload_max_bytes: None,
    })
    .await;
}

fn workshop_refs(record: &TurnWorkRecord) -> Vec<FeedRef> {
    vec![
        FeedRef {
            ref_type: "work".to_string(),
            ref_id: record.work_id.clone(),
        },
        FeedRef {
            ref_type: "session".to_string(),
            ref_id: record.session_id.clone(),
        },
    ]
}

fn truncate(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    trimmed.chars().take(max).collect::<String>() + "…"
}
