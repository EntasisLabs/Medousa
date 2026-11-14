//! Domain-owned breadcrumbs. Selection never grants retrieval permissions.
use crate::environment_store::EnvironmentRecord;
use chrono::{DateTime, Duration, Utc};
use medousa_types::context_selection::{
    PointerRepresentation, PointerResolution, PointerRetrieval, PointerSelection,
};
use medousa_types::daemon_api::WorkBoardColumn;
use medousa_types::environment::{
    ContextPointer, ContextPointerDigest, DIGEST_MAX_POINTERS, POINTER_KIND_COMPONENT,
    POINTER_KIND_SESSION, POINTER_KIND_WORK_CARD, STALENESS_ARCHIVED, STALENESS_FRESH,
    STALENESS_RECENT, STALENESS_STALE,
};
use medousa_types::session::SessionHistorySummary;

#[derive(Debug, Clone)]
pub struct WorkCardHint {
    pub id: String,
    pub label: String,
    pub last_active_at: DateTime<Utc>,
    pub session_id: Option<String>,
    pub column: WorkBoardColumn,
}

/// Only cards linked to this chat are context-relevant. A workspace snapshot is
/// not authorization to advertise unrelated profile work.
pub fn collect_work_card_hints(active_session_id: &str) -> Vec<WorkCardHint> {
    #[cfg(not(feature = "full-daemon"))]
    {
        let _ = active_session_id;
        Vec::new()
    }
    #[cfg(feature = "full-daemon")]
    {
        let Some(hub) = crate::workspace::projector::workspace_hub() else {
            return Vec::new();
        };
        hub.snapshot()
            .items
            .values()
            .filter(|item| {
                !item.detail.terminal
                    && item.detail.session_id.as_deref() == Some(active_session_id)
            })
            .map(|item| WorkCardHint {
                id: item.card.id.0.clone(),
                label: item.card.title.clone(),
                last_active_at: item.card.updated_at_utc,
                session_id: item.detail.session_id.clone(),
                column: item.card.column,
            })
            .collect()
    }
}

pub fn build_pointer_digest(
    active: &str,
    sessions: &[SessionHistorySummary],
    env: Option<&EnvironmentRecord>,
    work: &[WorkCardHint],
) -> ContextPointerDigest {
    let mut digest = build_pointer_candidates(active, sessions, env, work, Utc::now());
    digest
        .pointers
        .retain(|p| p.staleness != STALENESS_ARCHIVED);
    digest.pointers.truncate(DIGEST_MAX_POINTERS);
    digest
}

/// Unreduced authorized candidates for follow, independent of ambient ranking.
/// Inputs must be profile-visible; source membership, not selection, admits reads.
pub fn build_pointer_candidates(
    active: &str,
    sessions: &[SessionHistorySummary],
    env: Option<&EnvironmentRecord>,
    work: &[WorkCardHint],
    now: DateTime<Utc>,
) -> ContextPointerDigest {
    let mut pointers = Vec::new();
    for summary in sessions.iter().filter(|s| s.session_id != active) {
        let last = summary
            .last_timestamp
            .or(summary.last_verification_timestamp)
            .unwrap_or(now);
        pointers.push(pointer(
            active,
            &summary.session_id,
            POINTER_KIND_SESSION,
            summary
                .display_name
                .as_deref()
                .unwrap_or(&summary.session_id),
            topic_from_preview(&summary.preview),
            last,
            now,
            summary
                .last_verification_confidence
                .unwrap_or(0.85)
                .clamp(0.0, 1.0),
            500,
            48.0,
            "chat_catalog",
            Some(summary.turns as u32),
        ));
    }
    if let Some(env) = env {
        for component in &env.spec.components {
            pointers.push(pointer(
                active,
                &component.id,
                POINTER_KIND_COMPONENT,
                component.label.as_deref().unwrap_or(&component.id),
                None,
                component.updated_at.unwrap_or(env.spec.updated_at),
                now,
                0.85,
                300,
                168.0,
                "environment",
                None,
            ));
        }
    }
    for hint in work
        .iter()
        .filter(|h| h.session_id.as_deref() == Some(active))
    {
        pointers.push(pointer(
            active,
            &hint.id,
            POINTER_KIND_WORK_CARD,
            &hint.label,
            None,
            hint.last_active_at,
            now,
            0.85,
            1000,
            12.0,
            "workspace_projector",
            None,
        ));
    }
    pointers.sort_by(|a, b| {
        a.selection.as_ref().expect("selection").compare(
            &a.id,
            b.selection.as_ref().expect("selection"),
            &b.id,
        )
    });
    ContextPointerDigest {
        session_gap_human: sessions
            .iter()
            .find(|s| s.session_id == active)
            .and_then(|s| s.last_timestamp)
            .map(|last| human_age(last, now)),
        pointers,
        canvas_summary: env.map(|e| {
            format!(
                "{} ({} components)",
                e.spec.active_preset_id.as_deref().unwrap_or("default"),
                e.spec.components.len()
            )
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn pointer(
    scope: &str,
    id: &str,
    kind: &str,
    label: &str,
    topic: Option<String>,
    last: DateTime<Utc>,
    now: DateTime<Utc>,
    confidence: f32,
    relevance: u16,
    half_life: f64,
    source: &str,
    turns: Option<u32>,
) -> ContextPointer {
    let hours = (now - last).num_minutes().max(0) as f64 / 60.0;
    ContextPointer {
        id: id.into(),
        kind: kind.into(),
        label: label.into(),
        topic,
        last_active_at: last,
        confidence,
        selection: Some(PointerSelection {
            provenance: source.into(),
            scope: scope.into(),
            relevance,
            recency: ((-std::f64::consts::LN_2 * hours / half_life).exp() * 1000.0) as u16,
            activity: if hours < 1.0 { 150 } else { 0 },
            reasons: vec![
                if relevance == 1000 {
                    "active_session_work_link"
                } else {
                    "visible_source_activity"
                }
                .into(),
            ],
            representation: PointerRepresentation::Breadcrumb,
            retrieval: if kind == POINTER_KIND_SESSION {
                PointerRetrieval::ChatHistory {
                    session_id: id.into(),
                }
            } else {
                PointerRetrieval::ContextFollow {
                    pointer_id: id.into(),
                }
            },
            resolution: PointerResolution::Unknown,
        }),
        staleness: staleness_band(&last, now).into(),
        age_human: human_age(last, now),
        turn_count: turns,
        metadata: None,
    }
}

pub fn format_pointer_digest_block(digest: &ContextPointerDigest) -> String {
    let mut lines = vec!["[MEDOUSA_POINTERS]".to_string()];
    if let Some(gap) = &digest.session_gap_human {
        lines.push(format!("session_gap={gap}"));
    }
    lines.push(format!("selected_pointers={}", digest.pointers.len()));
    for p in &digest.pointers {
        // JSON quoting keeps source prose data rather than prompt syntax.
        lines.push(format!(
            "pointer={}",
            serde_json::to_string(p).expect("pointer serializes")
        ));
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
    let max_turns = parse_turn_scope(scope).unwrap_or(5).clamp(1, 100);
    match pointer.kind.as_str() {
        POINTER_KIND_SESSION => {
            if let Some(turns) = session_history {
                let start = turns.len().saturating_sub(max_turns);
                let out = turns[start..]
                    .iter()
                    .map(|t| format!("[{}] {}\n", t.role, t.content))
                    .collect::<String>();
                return (out, turns.len() > max_turns);
            }
            (
                format!(
                    "Session {} history not loaded; use cognition_chat_history_read with session_id.",
                    pointer.id
                ),
                false,
            )
        }
        POINTER_KIND_COMPONENT | POINTER_KIND_WORK_CARD => (
            format!(
                "{} '{}' (source projection only; not task completion, age {}).",
                pointer.kind, pointer.label, pointer.age_human
            ),
            false,
        ),
        _ => (format!("Unknown pointer kind for {}", pointer.id), false),
    }
}

fn staleness_band(last: &DateTime<Utc>, now: DateTime<Utc>) -> &'static str {
    let age = now - *last;
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
fn human_age(last: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let mins = (now - last).num_minutes().max(0);
    if mins < 60 {
        format!("{mins}m")
    } else if mins < 2880 {
        format!("{}h", mins / 60)
    } else {
        format!("{}d", mins / 1440)
    }
}
fn topic_from_preview(preview: &str) -> Option<String> {
    let line = preview.lines().next()?.trim();
    (!line.is_empty()).then(|| line.chars().take(64).collect())
}
fn parse_turn_scope(scope: &str) -> Option<usize> {
    scope
        .trim()
        .to_ascii_lowercase()
        .strip_prefix("last_")?
        .strip_suffix("_turns")?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hint(id: &str, session: &str, now: DateTime<Utc>) -> WorkCardHint {
        WorkCardHint {
            id: id.into(),
            label: id.into(),
            last_active_at: now,
            session_id: Some(session.into()),
            column: WorkBoardColumn::InFlight,
        }
    }
    #[test]
    fn selection_is_deterministic_scoped_and_deferral_does_not_remove_native_candidates() {
        let now = Utc::now();
        let mut hints = (0..20)
            .map(|i| hint(&format!("card-{i:02}"), "active", now))
            .collect::<Vec<_>>();
        hints.push(hint("private", "other", now));
        let all = build_pointer_candidates("active", &[], None, &hints, now);
        hints.reverse();
        let reversed = build_pointer_candidates("active", &[], None, &hints, now);
        assert_eq!(
            serde_json::to_value(&all).unwrap(),
            serde_json::to_value(&reversed).unwrap()
        );
        assert!(!all.pointers.iter().any(|p| p.id == "private"));
        let digest = build_pointer_digest("active", &[], None, &hints);
        assert!(digest.pointers.len() < all.pointers.len());
        let deferred = all.pointers.last().unwrap();
        assert_eq!(
            deferred.selection.as_ref().unwrap().retrieval,
            PointerRetrieval::ContextFollow {
                pointer_id: deferred.id.clone()
            }
        );
        assert!(
            resolve_pointer_slice(deferred, "last_5_turns", None)
                .0
                .contains("source projection only")
        );
    }
    #[test]
    fn confidence_is_not_recency_activity_or_relevance() {
        let now = Utc::now();
        let fresh = pointer(
            "s",
            "x",
            POINTER_KIND_SESSION,
            "x",
            None,
            now,
            now,
            0.1,
            900,
            48.0,
            "catalog",
            None,
        );
        let old = pointer(
            "s",
            "y",
            POINTER_KIND_SESSION,
            "y",
            None,
            now - Duration::days(10),
            now,
            0.99,
            100,
            48.0,
            "catalog",
            None,
        );
        assert!(fresh.selection.as_ref().unwrap().rank() > old.selection.as_ref().unwrap().rank());
        assert_eq!(fresh.confidence, 0.1);
        assert_eq!(
            fresh.selection.as_ref().unwrap().resolution,
            PointerResolution::Unknown
        );
    }
}
