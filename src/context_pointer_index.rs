//! Context pointer index — ranked breadcrumbs for turn bootstrap.

use chrono::{DateTime, Duration, Utc};
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

pub fn build_pointer_digest(
    active_session_id: &str,
    sessions: &[SessionHistorySummary],
    environment: Option<&EnvironmentRecord>,
    work_card_hints: &[(String, String, DateTime<Utc>)],
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
        let confidence = score_pointer(last, SESSION_HALF_LIFE_HOURS, 1.0);
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
            let confidence = score_pointer(last, COMPONENT_HALF_LIFE_HOURS, 0.85);
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

    for (card_id, label, last) in work_card_hints {
        let confidence = score_pointer(*last, WORK_CARD_HALF_LIFE_HOURS, 0.9);
        let staleness = staleness_band(last, now);
        pointers.push(ContextPointer {
            id: card_id.clone(),
            kind: POINTER_KIND_WORK_CARD.to_string(),
            label: label.clone(),
            topic: None,
            last_active_at: *last,
            confidence,
            staleness: staleness.to_string(),
            age_human: human_age(*last, now),
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

fn score_pointer(last_active: DateTime<Utc>, half_life_hours: f64, base: f32) -> f32 {
    let hours = (Utc::now() - last_active).num_minutes().max(0) as f64 / 60.0;
    let lambda = std::f64::consts::LN_2 / half_life_hours;
    let recency = (-lambda * hours).exp() as f32;
    (base * recency).clamp(0.0, 1.0)
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
}
