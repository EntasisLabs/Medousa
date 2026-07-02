//! Context pointer index — ranked breadcrumbs for turn bootstrap.

use chrono::{DateTime, Duration, Utc};
use medousa_types::daemon_api::WorkBoardColumn;
use medousa_types::environment::{
    ContextPointer, ContextPointerDigest, DIGEST_MAX_POINTERS, DIGEST_MIN_CONFIDENCE,
    POINTER_KIND_COMPONENT, POINTER_KIND_SESSION, POINTER_KIND_WORK_CARD, STALENESS_ARCHIVED,
    STALENESS_FRESH, STALENESS_RECENT, STALENESS_STALE,
};
use medousa_types::session::SessionHistorySummary;

use crate::environment_store::EnvironmentRecord;

const SESSION_HALF_LIFE_HOURS: f64 = 48.0;
const COMPONENT_HALF_LIFE_HOURS: f64 = 168.0;
const WORK_CARD_HALF_LIFE_HOURS: f64 = 12.0;
const RECENT_ACTIVITY_WINDOW: Duration = Duration::hours(1);

#[derive(Debug, Clone)]
pub struct WorkCardHint {
    pub id: String,
    pub label: String,
    pub last_active_at: DateTime<Utc>,
    pub session_id: Option<String>,
    pub column: WorkBoardColumn,
}

/// Ranked work-card breadcrumbs from the materialized workspace snapshot.
pub fn collect_work_card_hints(active_session_id: &str) -> Vec<WorkCardHint> {
    let Some(hub) = crate::workspace::projector::workspace_hub() else {
        return Vec::new();
    };
    let snapshot = hub.snapshot();
    let active_session_id = active_session_id.trim();
    let mut hints = Vec::new();

    for item in snapshot.items.values() {
        if item.detail.terminal {
            continue;
        }
        let linked_to_active = item
            .detail
            .session_id
            .as_deref()
            .is_some_and(|session_id| session_id == active_session_id);
        let in_motion = matches!(
            item.card.column,
            WorkBoardColumn::InFlight
                | WorkBoardColumn::WrappingUp
                | WorkBoardColumn::Blocked
                | WorkBoardColumn::Backlog
        );
        if !linked_to_active && !in_motion {
            continue;
        }
        hints.push(WorkCardHint {
            id: item.card.id.0.clone(),
            label: item.card.title.clone(),
            last_active_at: item.card.updated_at_utc,
            session_id: item.detail.session_id.clone(),
            column: item.card.column,
        });
    }

    hints.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    hints
}

pub fn build_pointer_digest(
    active_session_id: &str,
    sessions: &[SessionHistorySummary],
    environment: Option<&EnvironmentRecord>,
    work_card_hints: &[WorkCardHint],
) -> ContextPointerDigest {
    let mut pointers = Vec::new();
    let now = Utc::now();

    for summary in sessions {
        if summary.session_id == active_session_id {
            continue;
        }
        let last = summary
            .last_timestamp
            .or(summary.last_verification_timestamp)
            .unwrap_or(now);
        let mut activity_boost = 0.0_f32;
        if (now - last) < RECENT_ACTIVITY_WINDOW {
            activity_boost += 0.15;
        }
        let relevance = summary
            .last_verification_confidence
            .map(|value| value.clamp(0.6, 1.0))
            .unwrap_or(0.85);
        let confidence =
            score_pointer(last, SESSION_HALF_LIFE_HOURS, 1.0, activity_boost, relevance);
        let staleness = staleness_band(&last, now);
        pointers.push(ContextPointer {
            id: summary.session_id.clone(),
            kind: POINTER_KIND_SESSION.to_string(),
            label: summary
                .display_name
                .clone()
                .unwrap_or_else(|| summary.session_id.clone()),
            topic: topic_from_preview(&summary.preview),
            last_active_at: last,
            confidence,
            staleness: staleness.to_string(),
            age_human: human_age(last, now),
            turn_count: Some(summary.turns as u32),
            metadata: None,
        });
    }

    if let Some(env) = environment {
        for component in &env.spec.components {
            let last = component.updated_at.unwrap_or(env.spec.updated_at);
            let confidence = score_pointer(last, COMPONENT_HALF_LIFE_HOURS, 0.85, 0.0, 0.85);
            let staleness = staleness_band(&last, now);
            pointers.push(ContextPointer {
                id: component.id.clone(),
                kind: POINTER_KIND_COMPONENT.to_string(),
                label: component
                    .label
                    .clone()
                    .unwrap_or_else(|| component.id.clone()),
                topic: None,
                last_active_at: last,
                confidence,
                staleness: staleness.to_string(),
                age_human: human_age(last, now),
                turn_count: None,
                metadata: None,
            });
        }
    }

    for hint in work_card_hints {
        let mut activity_boost = 0.0_f32;
        if hint
            .session_id
            .as_deref()
            .is_some_and(|session_id| session_id == active_session_id)
            && matches!(
                hint.column,
                WorkBoardColumn::InFlight
                    | WorkBoardColumn::WrappingUp
                    | WorkBoardColumn::Blocked
            )
        {
            activity_boost += 0.20;
        }
        if (now - hint.last_active_at) < RECENT_ACTIVITY_WINDOW {
            activity_boost += 0.15;
        }
        let confidence = score_pointer(
            hint.last_active_at,
            WORK_CARD_HALF_LIFE_HOURS,
            0.9,
            activity_boost,
            0.85,
        );
        let staleness = staleness_band(&hint.last_active_at, now);
        pointers.push(ContextPointer {
            id: hint.id.clone(),
            kind: POINTER_KIND_WORK_CARD.to_string(),
            label: hint.label.clone(),
            topic: None,
            last_active_at: hint.last_active_at,
            confidence,
            staleness: staleness.to_string(),
            age_human: human_age(hint.last_active_at, now),
            turn_count: None,
            metadata: None,
        });
    }

    pointers.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    pointers.retain(|p| {
        p.staleness != STALENESS_ARCHIVED && p.confidence >= DIGEST_MIN_CONFIDENCE
    });
    pointers.truncate(DIGEST_MAX_POINTERS);

    let session_gap_human = sessions
        .iter()
        .find(|s| s.session_id == active_session_id)
        .and_then(|s| s.last_timestamp)
        .map(|last| human_age(last, now));

    let canvas_summary = environment.map(|env| {
        format!(
            "{} ({} components)",
            env.spec.active_preset_id.as_deref().unwrap_or("default"),
            env.spec.components.len()
        )
    });

    ContextPointerDigest {
        session_gap_human,
        pointers,
        canvas_summary,
    }
}

pub fn format_pointer_digest_block(digest: &ContextPointerDigest) -> String {
    let mut lines = vec!["[MEDOUSA_POINTERS]".to_string()];
    if let Some(gap) = &digest.session_gap_human {
        lines.push(format!("session_gap={gap}"));
    }
    if !digest.pointers.is_empty() {
        lines.push(format!("active_chats={}", digest.pointers.len()));
        lines.push("pointers:".to_string());
        for pointer in &digest.pointers {
            lines.push(format!(
                "  - id={} confidence={:.2} staleness={} age={} topic=\"{}\"",
                pointer.id,
                pointer.confidence,
                pointer.staleness,
                pointer.age_human,
                pointer.topic.as_deref().unwrap_or(&pointer.label)
            ));
        }
    }
    if let Some(canvas) = &digest.canvas_summary {
        lines.push(format!("canvas={canvas}"));
    }
    lines.join("\n")
}

pub fn resolve_pointer_slice(
    pointer: &ContextPointer,
    scope: &str,
    session_history: Option<&[medousa_types::session::ConversationTurn]>,
) -> (String, bool) {
    let max_turns = parse_turn_scope(scope).unwrap_or(5);
    match pointer.kind.as_str() {
        POINTER_KIND_SESSION => {
            if let Some(turns) = session_history {
                let start = turns.len().saturating_sub(max_turns);
                let slice = &turns[start..];
                let mut out = String::new();
                for turn in slice {
                    out.push_str(&format!("[{}] {}\n", turn.role, turn.content));
                }
                let truncated = turns.len() > max_turns;
                return (out, truncated);
            }
            (
                format!(
                    "Pointer {} ({}) — no session history loaded. Use session id to open thread.",
                    pointer.id, pointer.label
                ),
                false,
            )
        }
        POINTER_KIND_COMPONENT => (
            format!(
                "Component '{}' on canvas (confidence {:.2}, age {}).",
                pointer.label, pointer.confidence, pointer.age_human
            ),
            false,
        ),
        POINTER_KIND_WORK_CARD => (
            format!(
                "Work card '{}' (confidence {:.2}, age {}).",
                pointer.label, pointer.confidence, pointer.age_human
            ),
            false,
        ),
        _ => (format!("Unknown pointer kind for {}", pointer.id), false),
    }
}

fn score_pointer(
    last_active: DateTime<Utc>,
    half_life_hours: f64,
    base: f32,
    activity_boost: f32,
    relevance: f32,
) -> f32 {
    let hours = (Utc::now() - last_active).num_minutes().max(0) as f64 / 60.0;
    let lambda = std::f64::consts::LN_2 / half_life_hours;
    let recency = (-lambda * hours).exp() as f32;
    let activity = (1.0 + activity_boost).min(1.35);
    (base * recency * activity * relevance).clamp(0.0, 1.0)
}

fn staleness_band(last_active: &DateTime<Utc>, now: DateTime<Utc>) -> &'static str {
    let age = now - *last_active;
    if age < Duration::hours(1) {
        STALENESS_FRESH
    } else if age < Duration::hours(24) {
        STALENESS_RECENT
    } else if age < Duration::days(7) {
        STALENESS_STALE
    } else {
        STALENESS_ARCHIVED
    }
}

fn human_age(last_active: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let mins = (now - last_active).num_minutes().max(0);
    if mins < 60 {
        format!("{mins}m")
    } else if mins < 60 * 48 {
        format!("{}h", mins / 60)
    } else {
        format!("{}d", mins / (60 * 24))
    }
}

fn topic_from_preview(preview: &str) -> Option<String> {
    let line = preview.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.chars().take(64).collect())
    }
}

fn parse_turn_scope(scope: &str) -> Option<usize> {
    let scope = scope.trim().to_ascii_lowercase();
    if let Some(rest) = scope.strip_prefix("last_") {
        if let Some(num) = rest.strip_suffix("_turns") {
            return num.parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archived_pointers_filtered_from_digest() {
        let old = Utc::now() - Duration::days(30);
        let digest = build_pointer_digest(
            "active",
            &[SessionHistorySummary {
                session_id: "old".to_string(),
                display_name: Some("Old".to_string()),
                turns: 1,
                verification_runs: 0,
                last_timestamp: Some(old),
                last_verification_timestamp: None,
                last_verification_confidence: None,
                last_verification_coverage: None,
                last_verification_verified: None,
                preview: "runtime".to_string(),
            }],
            None,
            &[],
        );
        assert!(digest.pointers.is_empty());
    }

    #[test]
    fn recent_session_activity_boosts_confidence() {
        let recent = Utc::now() - Duration::minutes(10);
        let digest = build_pointer_digest(
            "active",
            &[SessionHistorySummary {
                session_id: "recent-thread".to_string(),
                display_name: Some("Recent".to_string()),
                turns: 4,
                verification_runs: 0,
                last_timestamp: Some(recent),
                last_verification_timestamp: None,
                last_verification_confidence: None,
                last_verification_coverage: None,
                last_verification_verified: None,
                preview: "runtime tuning".to_string(),
            }],
            None,
            &[],
        );
        let pointer = digest
            .pointers
            .iter()
            .find(|pointer| pointer.id == "recent-thread")
            .expect("recent session pointer");
        assert!(pointer.confidence > 0.9);
    }

    #[test]
    fn work_card_hints_rank_in_digest() {
        let digest = build_pointer_digest(
            "sess-active",
            &[],
            None,
            &[WorkCardHint {
                id: "card-1".to_string(),
                label: "Bound workshop".to_string(),
                last_active_at: Utc::now() - Duration::minutes(5),
                session_id: Some("sess-active".to_string()),
                column: WorkBoardColumn::InFlight,
            }],
        );
        let pointer = digest
            .pointers
            .iter()
            .find(|pointer| pointer.kind == POINTER_KIND_WORK_CARD)
            .expect("work card pointer");
        assert_eq!(pointer.label, "Bound workshop");
        assert!(pointer.confidence > 0.85);
    }
}
