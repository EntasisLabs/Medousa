//! Deterministic fold of Forge transition events into a `WorkItem`.

use crate::error::{ForgeError, Result};
use crate::events::{EventPayload, TransitionEvent};
use crate::model::WorkItem;

/// Apply one event payload to a folded work item. Operation-journal events
/// are crash-recovery records and leave state unchanged.
pub fn apply_payload(item: &mut WorkItem, event: &TransitionEvent) -> Result<()> {
    match &event.payload {
        EventPayload::ItemRegistered { .. } => {
            Err(ForgeError::Store("duplicate item_registered".into()))
        }
        EventPayload::EnvironmentProvisioned { env } => {
            item.environment = Some((**env).clone());
            Ok(())
        }
        EventPayload::StateChanged { to, .. } => {
            item.state = *to;
            item.updated_at = event.at;
            Ok(())
        }
        EventPayload::AttemptStarted { attempt } => {
            item.activate_attempt(attempt.id.clone());
            item.attempts.push((**attempt).clone());
            Ok(())
        }
        EventPayload::AttemptEnded {
            attempt_id,
            state,
            recovery,
        } => {
            if let Some(att) = item.attempt_mut(attempt_id) {
                att.state = *state;
                att.recovery = Some(recovery.clone());
                att.ended_at = Some(event.at);
                att.lease = None;
            }
            item.deactivate_attempt(attempt_id);
            Ok(())
        }
        EventPayload::LeaseAcquired {
            attempt_id,
            lease_id,
            generation,
            owner_instance_id,
        } => {
            if let Some(att) = item.attempt_mut(attempt_id)
                && let Some(lease) = att.lease.as_mut()
            {
                lease.lease_id = lease_id.clone();
                lease.generation = *generation;
                lease.owner_instance_id = owner_instance_id.clone();
            }
            Ok(())
        }
        EventPayload::EvidenceSealed {
            attempt_id,
            evidence_id,
            ..
        } => {
            if let Some(att) = item.attempt_mut(attempt_id) {
                att.evidence_id = Some(evidence_id.clone());
            }
            Ok(())
        }
        EventPayload::ReviewDecided { decision } => {
            item.review_decisions.push((**decision).clone());
            Ok(())
        }
        EventPayload::ReviewCommentAdded { comment } => {
            if let Some(existing) = item.review_comments.iter_mut().find(|c| c.id == comment.id) {
                *existing = (**comment).clone();
            } else {
                item.review_comments.push((**comment).clone());
            }
            Ok(())
        }
        EventPayload::ReviewCommentResolved {
            comment_id,
            resolved_by,
            resolved_at,
        } => {
            if let Some(comment) = item
                .review_comments
                .iter_mut()
                .find(|c| &c.id == comment_id)
            {
                comment.resolved_at = Some(*resolved_at);
                comment.resolved_by = Some(resolved_by.clone());
            }
            Ok(())
        }
        EventPayload::ReviewCommentDeleted { comment_id } => {
            item.review_comments.retain(|c| &c.id != comment_id);
            Ok(())
        }
        EventPayload::ChangesRequested { request } => {
            item.changes_requested.push((**request).clone());
            Ok(())
        }
        EventPayload::DecisionInvalidated { decision_id, .. } => {
            item.review_decisions.retain(|d| &d.id != decision_id);
            Ok(())
        }
        EventPayload::DispositionApplied { disposition, .. } => {
            item.disposition = Some(*disposition);
            Ok(())
        }
        EventPayload::OperationStarted { .. }
        | EventPayload::OperationSideEffect { .. }
        | EventPayload::OperationCommitted { .. }
        | EventPayload::OperationAborted { .. } => Ok(()),
    }
}

/// Fold events into state. The first event must be `item_registered`.
pub fn fold(events: &[TransitionEvent]) -> Result<WorkItem> {
    let mut iter = events.iter();
    let mut item = match iter.next().map(|e| &e.payload) {
        Some(EventPayload::ItemRegistered { item }) => (**item).clone(),
        _ => {
            return Err(ForgeError::Store(
                "event log does not start with item_registered".into(),
            ));
        }
    };
    for event in iter {
        apply_payload(&mut item, event)?;
    }
    Ok(item)
}
