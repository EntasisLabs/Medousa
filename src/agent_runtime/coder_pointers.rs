//! Ranked, bounded references into one undertaking's engineering activity.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::coder_activity::{CoderActivityEvent, CoderActivityKind};

pub const MAX_AMBIENT_POINTERS: usize = 6;
pub const MAX_HISTORY_EVENTS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoderPointerKind {
    Activity,
    File,
    Symbol,
    DiagnosticSet,
    Process,
    Verification,
    ChangeSet,
}

impl CoderPointerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Activity => "activity",
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::DiagnosticSet => "diagnostic_set",
            Self::Process => "process",
            Self::Verification => "verification",
            Self::ChangeSet => "change_set",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct CoderEngineeringPointer {
    pub pointer_id: String,
    pub kind: CoderPointerKind,
    pub label: String,
    pub score: f32,
    pub age_human: String,
    pub revision: u64,
    pub agent_id: String,
    pub tool: Option<String>,
    pub intent: Option<String>,
    pub targets: Vec<String>,
    pub status: CoderActivityKind,
}

#[derive(Debug, Clone, Default)]
pub struct CoderHistoryQuery<'a> {
    pub before_revision: Option<u64>,
    pub tool: Option<&'a str>,
    pub agent_id: Option<&'a str>,
    pub target: Option<&'a str>,
    pub failed_only: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoderPointerDetail {
    pub pointer_id: String,
    pub primary: CoderActivityEvent,
    pub causal_events: Vec<CoderActivityEvent>,
}

pub fn rank_engineering_pointers(
    events: &[CoderActivityEvent],
    self_agent_id: &str,
    focus_targets: &[String],
    limit: usize,
) -> Vec<CoderEngineeringPointer> {
    let now = Utc::now();
    let focus = focus_targets
        .iter()
        .map(|target| target.trim().to_ascii_lowercase())
        .filter(|target| !target.is_empty())
        .collect::<Vec<_>>();
    let mut latest_by_cause = HashMap::<String, &CoderActivityEvent>::new();
    for event in events {
        if !matches!(
            event.kind,
            CoderActivityKind::ToolPlanned
                | CoderActivityKind::ToolBlocked
                | CoderActivityKind::ToolCompleted
                | CoderActivityKind::ToolFailed
        ) {
            continue;
        }
        let cause = event
            .call_id
            .clone()
            .unwrap_or_else(|| event.event_id.clone());
        latest_by_cause
            .entry(cause)
            .and_modify(|current| {
                if event.revision > current.revision {
                    *current = event;
                }
            })
            .or_insert(event);
    }

    let mut pointers = latest_by_cause
        .into_iter()
        .map(|(cause, event)| {
            let kind = pointer_kind(event);
            let age_hours = (now - event.occurred_at_utc).num_seconds().max(0) as f32 / 3_600.0;
            let recency = 1.0 / (1.0 + age_hours / 6.0);
            let status = match event.kind {
                CoderActivityKind::ToolBlocked => 0.48,
                CoderActivityKind::ToolFailed => 0.42,
                CoderActivityKind::ToolPlanned => 0.30,
                CoderActivityKind::ToolCompleted => 0.12,
                _ => 0.0,
            };
            let focused = event.targets.iter().any(|target| {
                let target = target.to_ascii_lowercase();
                focus
                    .iter()
                    .any(|focus| target.contains(focus) || focus.contains(&target))
            });
            let score = (0.30 * recency
                + status
                + if focused { 0.22 } else { 0.0 }
                + if event.agent_id != self_agent_id {
                    0.08
                } else {
                    0.0
                }
                + if matches!(
                    kind,
                    CoderPointerKind::Verification | CoderPointerKind::DiagnosticSet
                ) {
                    0.08
                } else {
                    0.0
                })
            .clamp(0.0, 1.0);
            CoderEngineeringPointer {
                pointer_id: if event.call_id.is_some() {
                    format!("engineering:call:{cause}")
                } else {
                    format!("engineering:event:{cause}")
                },
                kind,
                label: event
                    .intent
                    .clone()
                    .or_else(|| event.tool.clone())
                    .unwrap_or_else(|| "Engineering activity".to_string()),
                score,
                age_human: human_age(event.occurred_at_utc, now),
                revision: event.revision,
                agent_id: event.agent_id.clone(),
                tool: event.tool.clone(),
                intent: event.intent.clone(),
                targets: event.targets.clone(),
                status: event.kind,
            }
        })
        .collect::<Vec<_>>();
    pointers.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.revision.cmp(&a.revision))
    });
    pointers.truncate(limit.clamp(1, 24));
    pointers
}

pub fn engineering_pointer_prompt_appendix(pointers: &[CoderEngineeringPointer]) -> String {
    let timestamp = Utc::now().to_rfc3339();
    let pointer_value = serde_json::to_value(pointers).unwrap_or_else(|_| json!([]));
    let mut out = String::new();
    let _ = writeln!(
        out,
        "⊕⟨ ⏣0{{ trigger: threshold, response_format: temporal_node, origin_session: \"medousa-coder-engineering-pointers\", compression_depth: 1, parent_node: ref:⏣0, prime: {{ attractor_config: {{ stability: 0.95, friction: 0.14, logic: 0.99, autonomy: 0.85 }}, context_summary: \"Ranked references to recent or unresolved engineering state in the active undertaking.\", relevant_tier: raw, retrieval_budget: 8 }} }} ⟩"
    );
    let _ = writeln!(
        out,
        "⦿⟨ ⏣0{{ timestamp: \"{timestamp}\", tier: raw, session_id: \"medousa-coder-engineering-pointers\", schema_version: \"sttp-1.0\", user_avec: {{ stability: 0.90, friction: 0.20, logic: 0.96, autonomy: 0.84, psi: 2.90 }}, model_avec: {{ stability: 0.95, friction: 0.14, logic: 0.99, autonomy: 0.85, psi: 2.95 }} }} ⟩"
    );
    let _ = writeln!(out, "◈⟨ ⏣0{{");
    let _ = writeln!(
        out,
        "    ranked_engineering_pointers(.99): {pointer_value},"
    );
    let _ = writeln!(
        out,
        "    retrieval_contract(.99): \"Follow a pointer with cognition_engineering_pointer_follow. cognition_coder_tools_discover describes the bounded history catalog without changing the tool surface.\""
    );
    let _ = writeln!(out, "}} ⟩");
    let _ = write!(
        out,
        "⍉⟨ ⏣0{{ rho: 0.99, kappa: 0.99, psi: 2.95, compression_avec: {{ stability: 0.95, friction: 0.14, logic: 0.99, autonomy: 0.85, psi: 2.95 }} }} ⟩"
    );
    debug_assert!(
        super::sttp::validate_canonical_sttp_node(&out).is_ok(),
        "Coder engineering pointer compiler emitted invalid STTP"
    );
    out
}

pub fn engineering_history(
    events: &[CoderActivityEvent],
    query: &CoderHistoryQuery<'_>,
) -> Vec<CoderActivityEvent> {
    let tool = normalized(query.tool);
    let agent_id = normalized(query.agent_id);
    let target = normalized(query.target);
    let mut seen = HashSet::new();
    events
        .iter()
        .rev()
        .filter(|event| {
            query
                .before_revision
                .is_none_or(|revision| event.revision < revision)
        })
        .filter(|event| !query.failed_only || event.kind == CoderActivityKind::ToolFailed)
        .filter(|event| {
            tool.as_ref().is_none_or(|needle| {
                event
                    .tool
                    .as_deref()
                    .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
            })
        })
        .filter(|event| {
            agent_id
                .as_ref()
                .is_none_or(|needle| event.agent_id.to_ascii_lowercase().contains(needle))
        })
        .filter(|event| {
            target.as_ref().is_none_or(|needle| {
                event
                    .targets
                    .iter()
                    .any(|value| value.to_ascii_lowercase().contains(needle))
            })
        })
        .filter(|event| seen.insert(event.event_id.clone()))
        .take(query.limit.clamp(1, MAX_HISTORY_EVENTS))
        .cloned()
        .collect()
}

pub fn follow_engineering_pointer(
    events: &[CoderActivityEvent],
    pointer_id: &str,
) -> Result<CoderPointerDetail, String> {
    let pointer_id = pointer_id.trim();
    let (primary, causal_events) =
        if let Some(call_id) = pointer_id.strip_prefix("engineering:call:") {
            let causal_events = events
                .iter()
                .filter(|event| event.call_id.as_deref() == Some(call_id))
                .cloned()
                .collect::<Vec<_>>();
            let primary = causal_events
                .iter()
                .max_by_key(|event| event.revision)
                .cloned()
                .ok_or_else(|| format!("engineering pointer not found: {pointer_id}"))?;
            (primary, causal_events)
        } else if let Some(event_id) = pointer_id.strip_prefix("engineering:event:") {
            let primary = events
                .iter()
                .find(|event| event.event_id == event_id)
                .cloned()
                .ok_or_else(|| format!("engineering pointer not found: {pointer_id}"))?;
            (primary.clone(), vec![primary])
        } else {
            return Err("invalid engineering pointer id".to_string());
        };
    Ok(CoderPointerDetail {
        pointer_id: pointer_id.to_string(),
        primary,
        causal_events,
    })
}

fn pointer_kind(event: &CoderActivityEvent) -> CoderPointerKind {
    let tool = event
        .tool
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let intent = event
        .intent
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if tool.contains("diagnostic") {
        CoderPointerKind::DiagnosticSet
    } else if tool.contains("store_write")
        || tool.contains("apply_patch")
        || tool.contains("change_set_apply")
    {
        CoderPointerKind::ChangeSet
    } else if tool.contains("affected_tests")
        || (tool.contains("shell")
            && ["test", "build", "check", "verify", "lint", "compile"]
                .iter()
                .any(|needle| intent.contains(needle)))
    {
        CoderPointerKind::Verification
    } else if tool.contains("shell") {
        CoderPointerKind::Process
    } else if tool.contains("symbol") || tool.contains("hover") || tool.contains("definition") {
        CoderPointerKind::Symbol
    } else if event
        .targets
        .iter()
        .any(|target| target.starts_with("file://"))
    {
        CoderPointerKind::File
    } else {
        CoderPointerKind::Activity
    }
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

fn human_age(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let seconds = (now - then).num_seconds().max(0);
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3_599 => format!("{}m", seconds / 60),
        3_600..=86_399 => format!("{}h", seconds / 3_600),
        _ => format!("{}d", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn event(revision: u64, kind: CoderActivityKind, tool: &str) -> CoderActivityEvent {
        CoderActivityEvent {
            event_id: format!("evt-{revision}"),
            revision,
            call_id: Some("call-1".to_string()),
            work_id: "work-1".to_string(),
            agent_id: "agent-a".to_string(),
            session_id: "session-a".to_string(),
            turn_id: "1".to_string(),
            attempt_id: "attempt-a".to_string(),
            kind,
            occurred_at_utc: Utc::now(),
            tool: Some(tool.to_string()),
            intent: Some("Run focused tests for the changed parser".to_string()),
            targets: vec!["file://src/parser.rs".to_string()],
            claims: Vec::new(),
            overlaps: Vec::new(),
            detail: Some("ok".to_string()),
        }
    }

    #[test]
    fn pointers_are_causal_stable_ranked_and_followable() {
        let events = vec![
            event(
                1,
                CoderActivityKind::ToolPlanned,
                "cognition_shell_session_run",
            ),
            event(
                2,
                CoderActivityKind::ToolFailed,
                "cognition_shell_session_run",
            ),
        ];
        let pointers =
            rank_engineering_pointers(&events, "agent-b", &["src/parser.rs".to_string()], 6);
        assert_eq!(pointers.len(), 1);
        assert_eq!(pointers[0].pointer_id, "engineering:call:call-1");
        assert_eq!(pointers[0].kind, CoderPointerKind::Verification);
        assert_eq!(pointers[0].status, CoderActivityKind::ToolFailed);
        let appendix = engineering_pointer_prompt_appendix(&pointers);
        super::super::sttp::validate_canonical_sttp_node(&appendix).expect("canonical STTP");
        let detail = follow_engineering_pointer(&events, &pointers[0].pointer_id).expect("follow");
        assert_eq!(detail.causal_events.len(), 2);
        assert_eq!(detail.primary.revision, 2);
    }

    #[test]
    fn blocked_claim_becomes_a_ranked_unresolved_pointer() {
        let mut blocked = event(
            1,
            CoderActivityKind::ToolBlocked,
            "cognition_code_apply_patch",
        );
        blocked.detail = Some("hazardous shared resource is already claimed".into());
        let pointers = rank_engineering_pointers(&[blocked], "agent-b", &[], 4);
        assert_eq!(pointers.len(), 1);
        assert_eq!(pointers[0].status, CoderActivityKind::ToolBlocked);
        assert_eq!(pointers[0].kind, CoderPointerKind::ChangeSet);
    }

    #[test]
    fn history_filters_and_bounds_results() {
        let mut first = event(1, CoderActivityKind::ToolCompleted, "cognition_code_read");
        first.call_id = Some("call-1".to_string());
        let mut second = event(
            2,
            CoderActivityKind::ToolFailed,
            "cognition_code_diagnostics",
        );
        second.call_id = Some("call-2".to_string());
        let events = vec![first, second];
        let history = engineering_history(
            &events,
            &CoderHistoryQuery {
                failed_only: true,
                limit: 10,
                ..Default::default()
            },
        );
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].revision, 2);
    }
}
