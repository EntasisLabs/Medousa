//! World activity and computer readiness, after medousa-home: what Medousa
//! did in governed browser and computer worlds, and whether each computer
//! driver has the permissions it needs. Both only read; opening them never
//! asks the operating system for anything.
use medousa_types::{WorldEvidenceResponse, WorldTimelineEvent, WorldTimelineResponse};
use serde_json::Value;

/// One row: the event, and whether it was promoted to evidence.
#[derive(Clone, Debug)]
pub(crate) struct Item {
    pub event: WorldTimelineEvent,
    pub evidence: bool,
}

/// medousa-home's `mergeWorldActivity`: newest first, evidence by sequence.
pub(crate) fn merge(
    timeline: WorldTimelineResponse,
    evidence: Option<WorldEvidenceResponse>,
) -> Vec<Item> {
    let promoted: std::collections::HashSet<u64> = evidence
        .into_iter()
        .flat_map(|e| e.evidence)
        .map(|r| r.ledger_sequence)
        .collect();
    timeline
        .events
        .into_iter()
        .rev()
        .map(|event| Item {
            evidence: promoted.contains(&event.sequence),
            event,
        })
        .collect()
}

/// medousa-home's `worldActorLabel`.
pub(crate) fn actor(e: &WorldTimelineEvent) -> &'static str {
    match e.principal_kind.as_deref() {
        Some("human") => "You",
        Some("agent") => "Medousa",
        Some("bot") => "Bot",
        Some("worker") => "Worker",
        Some("peer") => "Paired workshop",
        Some("system") => "System",
        _ => "World runtime",
    }
}

/// medousa-home's `worldEventLabel`.
pub(crate) fn title(e: &WorldTimelineEvent) -> String {
    match e.event_type.as_str() {
        "world_created" => "Created world".into(),
        "capability_granted" => "Granted access".into(),
        "capability_revoked" => "Revoked access".into(),
        "control_acquired" => "Took control".into(),
        "control_released" => "Returned control".into(),
        "action_admitted" => "Admitted action".into(),
        "action_committed" => "Completed action".into(),
        "action_failed" => "Action failed".into(),
        "action_interrupted" => "Action interrupted".into(),
        "external_mutation_observed" => "Observed outside change".into(),
        other => other.replace('_', " "),
    }
}

/// medousa-home's `worldActivityDetailLabel`: `needs_reconciliation` →
/// `Needs Reconciliation`.
pub(crate) fn detail(value: &str) -> String {
    value
        .split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The summary line and the meta line under an event's title.
pub(crate) fn lines(e: &WorldTimelineEvent) -> (String, String) {
    let summary = e
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("{} changed a {} world", actor(e), e.surface));
    let mut meta = format!("{} · {}", actor(e), detail(&e.surface));
    if let Some(status) = &e.status {
        meta.push_str(&format!(" · {}", detail(status)));
    }
    (summary, meta)
}

/// medousa-home's `worldActivityWhen`.
pub(crate) fn when(at_ms: u64, now_ms: u64) -> String {
    if at_ms == 0 {
        return String::new();
    }
    let minutes = now_ms.saturating_sub(at_ms) / 60_000;
    let hours = minutes / 60;
    let days = hours / 24;
    match () {
        _ if minutes < 1 => "now".into(),
        _ if minutes < 60 => format!("{minutes}m"),
        _ if hours < 24 => format!("{hours}h"),
        _ if days < 14 => format!("{days}d"),
        _ => chrono::DateTime::from_timestamp_millis(at_ms as i64)
            .map(|t| t.with_timezone(&chrono::Local).format("%b %-d").to_string())
            .unwrap_or_default(),
    }
}

/// The three permissions a computer driver reports, with what each is for.
pub(crate) const PERMISSIONS: [(&str, &str, &str); 3] = [
    (
        "accessibility",
        "Controls & text",
        "Semantic observation and native actions",
    ),
    (
        "screen_capture",
        "Focused-window pixels",
        "Bounded screenshots with secure regions redacted",
    ),
    (
        "input_control",
        "Foreground input",
        "Guarded click fallback when semantics are unavailable",
    ),
];

/// One driver and its preflight (`resource_id`, `preflight`, `control`).
#[derive(Clone, Debug)]
pub(crate) struct Driver {
    pub driver: Value,
    pub preflight: Result<Value, String>,
}

pub(crate) fn driver_name(d: &Driver) -> String {
    let platform = match d
        .preflight
        .as_ref()
        .ok()
        .and_then(|p| p["preflight"]["platform"].as_str())
    {
        Some("macos") => "macOS".to_string(),
        Some("windows") => "Windows".into(),
        Some("linux") => "Linux".into(),
        Some(other) if !other.trim().is_empty() => other.into(),
        _ => "Native desktop".into(),
    };
    match d.driver["display_name"]
        .as_str()
        .filter(|n| !n.trim().is_empty())
    {
        Some(name) => format!("{name} · {platform}"),
        None => platform,
    }
}

/// A permission's state and the line under it, as medousa-home words them.
pub(crate) fn permission(d: &Driver, id: &str, ready_hint: &str) -> (&'static str, String) {
    let report = d.preflight.as_ref().ok().and_then(|p| {
        p["preflight"]["permissions"]
            .as_array()?
            .iter()
            .find(|r| r["permission"] == id)
            .cloned()
    });
    let status = report.as_ref().and_then(|r| r["status"].as_str());
    let state = match status {
        Some("granted") => "Ready",
        Some("denied" | "not_determined") => "Needed",
        Some("restricted") => "Restricted",
        _ => "Unavailable",
    };
    let hint = if status == Some("granted") {
        ready_hint.to_string()
    } else {
        report
            .as_ref()
            .and_then(|r| r["guidance"].as_str())
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .unwrap_or("This driver did not report the permission as available.")
            .to_string()
    };
    (state, hint)
}

/// medousa-home's `controlLabel`.
pub(crate) fn control(d: &Driver) -> &'static str {
    let c = d.preflight.as_ref().ok().map(|p| &p["control"]);
    match c {
        Some(c) if c["requester_has_control"].as_bool() == Some(true) => "You have control",
        Some(c) if c["holder"] == "human" => "Another person has control",
        Some(c) if c.is_object() && c["holder"] != "available" => "Medousa is operating",
        _ => "Ready to watch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(sequence: u64, kind: &str, principal: &str, status: Option<&str>) -> Value {
        serde_json::json!({
            "schema_version": 1, "sequence": sequence, "recorded_at_ms": 1, "world_id": "w",
            "authority_id": "a", "driver_id": "d", "ownership": "o", "surface": "browser",
            "world_revision": 1, "occurred_at_ms": 1, "principal_kind": principal,
            "event_type": kind, "status": status,
        })
    }

    #[test]
    fn events_read_newest_first_with_evidence_and_medousa_homes_words() {
        let timeline: WorldTimelineResponse = serde_json::from_value(serde_json::json!({
            "events": [event(1, "world_created", "system", None),
                       event(2, "action_failed", "agent", Some("needs_reconciliation"))],
            "next_sequence": 3, "has_more": false,
        }))
        .unwrap();
        let evidence: WorldEvidenceResponse = serde_json::from_value(serde_json::json!({
            "evidence": [{"schema_version": 1, "evidence_id": "e", "ledger_sequence": 2,
                "promoted_at_ms": 1, "world_id": "w", "authority_id": "a", "driver_id": "d",
                "ownership": "o", "surface": "browser", "event_type": "action_failed",
                "reasons": []}],
            "next_sequence": 3, "has_more": false,
        }))
        .unwrap();
        let items = merge(timeline, Some(evidence));
        assert_eq!(items[0].event.sequence, 2);
        assert_eq!(title(&items[0].event), "Action failed");
        let (summary, meta) = lines(&items[0].event);
        assert_eq!(summary, "Medousa changed a browser world");
        assert_eq!(meta, "Medousa · Browser · Needs Reconciliation");
        assert_eq!(title(&items[1].event), "Created world");
        assert_eq!(when(1, 1 + 5 * 60_000), "5m");
        assert_eq!(when(1, 1 + 3 * 3_600_000), "3h");
        assert_eq!(when(1, 30_000), "now");
    }

    #[test]
    fn a_driver_reads_its_permissions_and_control() {
        let d = Driver {
            driver: serde_json::json!({"driver_id": "mac", "display_name": "Mac"}),
            preflight: Ok(serde_json::json!({
                "preflight": {"platform": "macos", "permissions": [
                    {"permission": "accessibility", "status": "granted"},
                    {"permission": "screen_capture", "status": "denied", "guidance": "Allow in Privacy & Security"}
                ]},
                "control": {"holder": "available", "requester_has_control": false}
            })),
        };
        assert_eq!(driver_name(&d), "Mac · macOS");
        assert_eq!(permission(&d, "accessibility", "ok").0, "Ready");
        assert_eq!(
            permission(&d, "screen_capture", "ok"),
            ("Needed", "Allow in Privacy & Security".into())
        );
        assert_eq!(permission(&d, "input_control", "ok").0, "Unavailable");
        assert_eq!(control(&d), "Ready to watch");
    }
}
