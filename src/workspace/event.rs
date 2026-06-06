//! Workspace activity feed event helpers.

use chrono::Utc;
use uuid::Uuid;

use crate::daemon_api::{
    WorkBoardColumn, WorkCard, WorkspaceEvent, WorkspaceEventActor, WorkspaceEventKind,
    WorkspaceEventRef,
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
    card: &WorkCard,
    previous: Option<WorkBoardColumn>,
    current: WorkBoardColumn,
) -> Option<WorkspaceEvent> {
    let mut refs = Vec::new();
    append_card_ref(&mut refs, &card.id.0);

    let (kind, summary) = match (previous, current) {
        (None, WorkBoardColumn::Backlog) => (
            WorkspaceEventKind::JobEnqueued,
            format!("Queued — {}", card.title),
        ),
        (Some(WorkBoardColumn::Backlog), WorkBoardColumn::InFlight) => (
            WorkspaceEventKind::JobStarted,
            format!("Started — {}", card.title),
        ),
        (_, WorkBoardColumn::WrappingUp) => (
            WorkspaceEventKind::WorkWrappingUp,
            format!("Wrapping up — {} ({})", card.title, card.status_label),
        ),
        (Some(WorkBoardColumn::WrappingUp), WorkBoardColumn::Done) => (
            WorkspaceEventKind::WorkUnblocked,
            format!("Finished — {}", card.title),
        ),
        (_, WorkBoardColumn::Done) => (
            WorkspaceEventKind::JobSucceeded,
            format!("Completed — {}", card.title),
        ),
        (_, WorkBoardColumn::Blocked) => (
            WorkspaceEventKind::JobFailed,
            format!("Blocked — {}", card.title),
        ),
        (None, WorkBoardColumn::InFlight) => (
            WorkspaceEventKind::WorkDelegated,
            format!("Delegated — {}", card.title),
        ),
        _ => return None,
    };

    Some(WorkspaceEvent {
        id: new_event_id(),
        timestamp_utc: Utc::now(),
        kind,
        actor: WorkspaceEventActor::System,
        summary,
        refs,
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
