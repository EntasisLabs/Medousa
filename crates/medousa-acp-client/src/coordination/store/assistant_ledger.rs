//! Durable Assistant ownership projection over native execution records.
//!
//! Origins, events, commands, and the first terminal are create-only. Reads fold
//! the journal; the ledger never launches, retries, or mutates a native executor.

use super::{CoordinationStore, object_path};
use anyhow::{Result, bail};
use medousa_store::StorePath;
use medousa_types::AuthorityId;
use medousa_types::assistant_assignment::*;
use medousa_types::coordination::{
    CoordinationChannelRef, ExternalPeerAssignmentBinding, ExternalPeerAssignmentReceipt,
    ExternalPeerAssignmentRequest, PeerAssignmentOutcome,
};
use std::collections::BTreeMap;

const MAX_LEDGER_ENTRIES: usize = 20_000;
const MAX_PAGE_SIZE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantCommandClaim {
    Claimed,
    Existing,
}

fn scope(authority_id: &AuthorityId) -> CoordinationChannelRef {
    CoordinationChannelRef {
        authority_id: authority_id.clone(),
        channel_id: "assistant-assignment-ledger".to_string(),
    }
}

fn require_text(label: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("assistant assignment {label} is required");
    }
    Ok(())
}

fn validate_assignment(value: &AssistantAssignment) -> Result<()> {
    if value.schema_version != ASSISTANT_ASSIGNMENT_SCHEMA_VERSION {
        bail!("unsupported assistant assignment schema version");
    }
    for (label, field) in [
        ("id", value.assignment_id.as_str()),
        ("owner", value.owner_id.as_str()),
        ("principal", value.principal_id.as_str()),
        ("intent", value.intent.as_str()),
        ("native execution id", value.execution.native_id.as_str()),
        ("actor", value.actor_id.as_str()),
        ("correlation id", value.correlation_id.as_str()),
    ] {
        require_text(label, field)?;
    }
    if value.source.authority_id != value.execution.workshop_authority_id {
        // Cross-workshop assignments are valid, but source and destination must
        // still be explicitly represented rather than normalized together.
        require_text(
            "destination workshop authority",
            value.execution.workshop_authority_id.as_str(),
        )?;
    }
    if value.updated_at < value.created_at {
        bail!("assistant assignment update predates creation");
    }
    Ok(())
}

fn validate_event(value: &AssistantAssignmentEvent) -> Result<()> {
    for (label, field) in [
        ("event id", value.event_id.as_str()),
        ("assignment id", value.assignment_id.as_str()),
        ("event actor", value.actor_id.as_str()),
        ("event correlation id", value.correlation_id.as_str()),
    ] {
        require_text(label, field)?;
    }
    Ok(())
}

impl CoordinationStore {
    pub(crate) fn project_external_request(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<()> {
        if self
            .assistant_assignment(&request.channel.authority_id, &request.assignment_id)
            .is_ok()
        {
            return Ok(());
        }
        let now = chrono::Utc::now();
        let origin = AssistantAssignment {
            schema_version: ASSISTANT_ASSIGNMENT_SCHEMA_VERSION,
            assignment_id: request.assignment_id.clone(),
            owner_id: request.owner_principal_id.clone(),
            principal_id: request.owner_principal_id.clone(),
            intent: request.instructions.clone(),
            source: AssistantAssignmentSource {
                authority_id: request.channel.authority_id.clone(),
                session: Some(request.owner_session.clone()),
                coordination_channel_id: Some(request.channel.channel_id.clone()),
            },
            execution: AssistantExecutionBinding {
                kind: AssistantAssignmentKind::ExternalPeer,
                native_id: request.assignment_id.clone(),
                runtime_id: Some(request.target.execution_runtime_id.clone()),
                execution_session: Some(request.execution_session.clone()),
                workshop_authority_id: request.target.authority_id.clone(),
                forge_work_id: Some(request.forge_work_id.clone()),
            },
            budget: AssistantAssignmentBudget::default(),
            status: AssistantAssignmentStatus::Pending,
            references: AssistantAssignmentReferences {
                grant_id: Some(request.execution_grant_id.clone()),
                ..Default::default()
            },
            actor_id: request.owner_principal_id.clone(),
            correlation_id: request.idempotency_key.clone(),
            causation_id: None,
            next_action: Some("await peer custody".into()),
            created_at: now,
            updated_at: now,
        };
        match self.create_assistant_assignment(&origin) {
            Ok(_) => Ok(()),
            Err(error) => match self
                .assistant_assignment(&request.channel.authority_id, &request.assignment_id)
            {
                Ok(existing)
                    if existing.assignment.owner_id == request.owner_principal_id
                        && existing.assignment.execution.native_id == request.assignment_id =>
                {
                    Ok(())
                }
                _ => Err(error),
            },
        }
    }

    pub(crate) fn project_external_binding(
        &self,
        binding: &ExternalPeerAssignmentBinding,
    ) -> Result<()> {
        let request = self.assignment(&binding.channel, &binding.assignment_id)?;
        self.project_external_request(&request)?;
        let view =
            self.assistant_assignment(&binding.channel.authority_id, &binding.assignment_id)?;
        if view.assignment.status != AssistantAssignmentStatus::Pending {
            return Ok(());
        }
        self.record_assistant_assignment_event(
            &binding.channel.authority_id,
            &AssistantAssignmentEvent {
                event_id: format!("peer-running:{}", binding.assignment_id),
                assignment_id: binding.assignment_id.clone(),
                native_sequence: 1,
                status: AssistantAssignmentStatus::Running,
                actor_id: binding.agent_session_id.clone(),
                correlation_id: request.idempotency_key,
                causation_id: Some(binding.agent_session_id.clone()),
                detail: None,
                next_action: Some("await peer terminal receipt".into()),
                references: AssistantAssignmentReferencePatch::default(),
                observed_at: view.assignment.created_at,
            },
        )?;
        Ok(())
    }

    pub(crate) fn project_external_receipt(
        &self,
        receipt: &ExternalPeerAssignmentReceipt,
    ) -> Result<()> {
        let view = self.assistant_assignment(
            &receipt.binding.channel.authority_id,
            &receipt.binding.assignment_id,
        )?;
        if view.assignment.status.is_terminal() {
            return Ok(());
        }
        let status = match receipt.outcome {
            PeerAssignmentOutcome::Completed => AssistantAssignmentStatus::Completed,
            PeerAssignmentOutcome::Failed => AssistantAssignmentStatus::Failed,
            PeerAssignmentOutcome::Cancelled => AssistantAssignmentStatus::Cancelled,
            PeerAssignmentOutcome::Interrupted => AssistantAssignmentStatus::Interrupted,
        };
        self.record_assistant_assignment_event(
            &receipt.binding.channel.authority_id,
            &AssistantAssignmentEvent {
                event_id: receipt.receipt_id.clone(),
                assignment_id: receipt.binding.assignment_id.clone(),
                native_sequence: 2,
                status,
                actor_id: receipt.binding.agent_session_id.clone(),
                correlation_id: receipt.binding.assignment_id.clone(),
                causation_id: Some(receipt.binding.agent_session_id.clone()),
                detail: Some(receipt.result.clone()),
                next_action: Some(
                    if status.is_terminal() {
                        "inspect receipt or continue owner"
                    } else {
                        "reconcile interrupted peer"
                    }
                    .into(),
                ),
                references: AssistantAssignmentReferencePatch {
                    receipt_id: Some(receipt.receipt_id.clone()),
                    ..Default::default()
                },
                observed_at: view.assignment.updated_at,
            },
        )?;
        Ok(())
    }

    pub fn create_assistant_assignment(&self, value: &AssistantAssignment) -> Result<bool> {
        validate_assignment(value)?;
        self.create(
            &object_path(
                &scope(&value.source.authority_id),
                "assistant-assignment",
                &value.assignment_id,
            )?,
            value,
        )
    }

    pub fn assistant_assignment(
        &self,
        authority_id: &AuthorityId,
        assignment_id: &str,
    ) -> Result<AssistantAssignmentView> {
        require_text("id", assignment_id)?;
        let origin: AssistantAssignment = self.read(&object_path(
            &scope(authority_id),
            "assistant-assignment",
            assignment_id,
        )?)?;
        if origin.assignment_id != assignment_id || origin.source.authority_id != *authority_id {
            bail!("assistant assignment identity mismatch");
        }
        self.fold_assistant_assignment(origin)
    }

    pub fn record_assistant_assignment_event(
        &self,
        authority_id: &AuthorityId,
        event: &AssistantAssignmentEvent,
    ) -> Result<bool> {
        validate_event(event)?;
        let origin: AssistantAssignment = self.read(&object_path(
            &scope(authority_id),
            "assistant-assignment",
            &event.assignment_id,
        )?)?;
        if origin.source.authority_id != *authority_id {
            bail!("assistant assignment authority mismatch");
        }

        if event.status.is_terminal() && !origin.status.is_terminal() {
            // First terminal wins permanently. A conflicting terminal cannot be
            // smuggled in as a later/out-of-order native lifecycle event.
            self.create(
                &object_path(
                    &scope(authority_id),
                    "assistant-terminal",
                    &event.assignment_id,
                )?,
                event,
            )?;
        }
        self.create(
            &object_path(&scope(authority_id), "assistant-event", &event.event_id)?,
            event,
        )
    }

    pub fn claim_assistant_assignment_command(
        &self,
        authority_id: &AuthorityId,
        command: &AssistantAssignmentCommand,
    ) -> Result<AssistantCommandClaim> {
        for (label, field) in [
            ("command id", command.command_id.as_str()),
            ("command assignment id", command.assignment_id.as_str()),
            ("command action", command.action.as_str()),
            ("command actor", command.actor_id.as_str()),
            ("command correlation id", command.correlation_id.as_str()),
        ] {
            require_text(label, field)?;
        }
        let view = self.assistant_assignment(authority_id, &command.assignment_id)?;
        if view.assignment.status.is_terminal() {
            bail!("terminal assistant assignment cannot accept a command");
        }
        let created = self.create(
            &object_path(
                &scope(authority_id),
                "assistant-command",
                &command.command_id,
            )?,
            command,
        )?;
        Ok(if created {
            AssistantCommandClaim::Claimed
        } else {
            AssistantCommandClaim::Existing
        })
    }

    pub fn list_assistant_assignments(
        &self,
        authority_id: &AuthorityId,
        filter: &AssistantAssignmentListFilter,
        limit: usize,
        after_assignment_id: Option<&str>,
    ) -> Result<AssistantAssignmentListPage> {
        if !(1..=MAX_PAGE_SIZE).contains(&limit) {
            bail!("invalid assistant assignment page size");
        }
        let entries = self.root.list_root_utf8()?;
        if entries.len() > MAX_LEDGER_ENTRIES {
            bail!("assistant assignment scan budget exhausted");
        }
        let mut origins = BTreeMap::new();
        for entry in entries {
            if !entry.name.starts_with("aa1-") {
                continue;
            }
            let value: AssistantAssignment = self.read(&StorePath::parse(&entry.name)?)?;
            if value.source.authority_id != *authority_id
                || after_assignment_id.is_some_and(|after| value.assignment_id.as_str() <= after)
                || filter
                    .owner_id
                    .as_ref()
                    .is_some_and(|owner| owner != &value.owner_id)
                || filter
                    .principal_id
                    .as_ref()
                    .is_some_and(|principal| principal != &value.principal_id)
                || filter.kind.is_some_and(|kind| kind != value.execution.kind)
            {
                continue;
            }
            if origins.insert(value.assignment_id.clone(), value).is_some() {
                bail!("duplicate assistant assignment identity");
            }
        }

        let mut assignments = Vec::new();
        let mut has_more = false;
        for origin in origins.into_values() {
            let view = self.fold_assistant_assignment(origin)?;
            if filter
                .terminal
                .is_some_and(|terminal| terminal != view.assignment.status.is_terminal())
            {
                continue;
            }
            if assignments.len() == limit {
                has_more = true;
                break;
            }
            assignments.push(view);
        }
        let next_cursor = has_more
            .then(|| {
                assignments
                    .last()
                    .map(|view| view.assignment.assignment_id.clone())
            })
            .flatten();
        Ok(AssistantAssignmentListPage {
            assignments,
            next_cursor,
        })
    }

    /// Returns one assignment's native observations in deterministic source order.
    pub fn assistant_assignment_events(
        &self,
        authority_id: &AuthorityId,
        assignment_id: &str,
        limit: usize,
        after_native_sequence: Option<u64>,
    ) -> Result<Vec<AssistantAssignmentEvent>> {
        if !(1..=MAX_PAGE_SIZE).contains(&limit) {
            bail!("invalid assistant assignment event page size");
        }
        let view = self.assistant_assignment(authority_id, assignment_id)?;
        let entries = self.root.list_root_utf8()?;
        if entries.len() > MAX_LEDGER_ENTRIES {
            bail!("assistant assignment scan budget exhausted");
        }
        let mut events = BTreeMap::new();
        for entry in entries {
            if !entry.name.starts_with("ae1-") {
                continue;
            }
            let event: AssistantAssignmentEvent = self.read(&StorePath::parse(&entry.name)?)?;
            if event.assignment_id == view.assignment.assignment_id
                && after_native_sequence.is_none_or(|after| event.native_sequence > after)
            {
                events.insert((event.native_sequence, event.event_id.clone()), event);
                if events.len() > limit {
                    events.pop_last();
                }
            }
        }
        Ok(events.into_values().collect())
    }

    fn fold_assistant_assignment(
        &self,
        mut origin: AssistantAssignment,
    ) -> Result<AssistantAssignmentView> {
        let entries = self.root.list_root_utf8()?;
        if entries.len() > MAX_LEDGER_ENTRIES {
            bail!("assistant assignment scan budget exhausted");
        }
        let mut events = BTreeMap::new();
        for entry in entries {
            if !entry.name.starts_with("ae1-") {
                continue;
            }
            let event: AssistantAssignmentEvent = self.read(&StorePath::parse(&entry.name)?)?;
            if event.assignment_id == origin.assignment_id {
                let key = (event.native_sequence, event.event_id.clone());
                if events.insert(key, event).is_some() {
                    bail!("duplicate assistant assignment event order");
                }
            }
        }

        let terminal = match self.read::<AssistantAssignmentEvent>(&object_path(
            &scope(&origin.source.authority_id),
            "assistant-terminal",
            &origin.assignment_id,
        )?) {
            Ok(event) => Some(event),
            Err(error)
                if error
                    .downcast_ref::<medousa_store::StoreRootError>()
                    .is_some_and(|error| error.is_not_found()) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        let terminal_event_id = terminal.as_ref().map(|event| event.event_id.clone());

        // Native sequence orders non-terminal observations. The separately
        // claimed first terminal always wins, regardless of delivery order.
        for event in events.values().filter(|event| !event.status.is_terminal()) {
            apply_assignment_event(&mut origin, event).map_err(anyhow::Error::msg)?;
        }
        if let Some(event) = &terminal {
            if event.assignment_id != origin.assignment_id || !event.status.is_terminal() {
                bail!("assistant terminal fence identity mismatch");
            }
            apply_assignment_event(&mut origin, event).map_err(anyhow::Error::msg)?;
        }
        Ok(AssistantAssignmentView {
            assignment: origin,
            event_count: events.len(),
            terminal_event_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use medousa_types::assistant_assignment::{
        ASSISTANT_ASSIGNMENT_SCHEMA_VERSION, AssistantAssignmentBudget,
        AssistantAssignmentReferences, AssistantAssignmentSource, AssistantExecutionBinding,
    };

    fn authority() -> AuthorityId {
        format!("auth_{}", "a".repeat(64)).parse().unwrap()
    }

    fn at(second: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 21, 12, 0, second)
            .single()
            .unwrap()
    }

    fn assignment(id: &str, kind: AssistantAssignmentKind) -> AssistantAssignment {
        AssistantAssignment {
            schema_version: ASSISTANT_ASSIGNMENT_SCHEMA_VERSION,
            assignment_id: id.into(),
            owner_id: "assistant:primary".into(),
            principal_id: "user:alice".into(),
            intent: format!("own {id}"),
            source: AssistantAssignmentSource {
                authority_id: authority(),
                session: None,
                coordination_channel_id: Some("project-review".into()),
            },
            execution: AssistantExecutionBinding {
                kind,
                native_id: format!("native-{id}"),
                runtime_id: Some("runtime-local".into()),
                execution_session: None,
                workshop_authority_id: authority(),
                forge_work_id: Some("work-1".into()),
            },
            budget: AssistantAssignmentBudget {
                max_attempts: Some(2),
                ..Default::default()
            },
            status: AssistantAssignmentStatus::Pending,
            references: AssistantAssignmentReferences {
                grant_id: Some("grant-1".into()),
                ..Default::default()
            },
            actor_id: "assistant:primary".into(),
            correlation_id: format!("correlation-{id}"),
            causation_id: Some("turn-1".into()),
            next_action: Some("wait for executor".into()),
            created_at: at(0),
            updated_at: at(0),
        }
    }

    fn event(
        assignment_id: &str,
        event_id: &str,
        sequence: u64,
        status: AssistantAssignmentStatus,
    ) -> AssistantAssignmentEvent {
        AssistantAssignmentEvent {
            event_id: event_id.into(),
            assignment_id: assignment_id.into(),
            native_sequence: sequence,
            status,
            actor_id: "runtime:local".into(),
            correlation_id: format!("correlation-{assignment_id}"),
            causation_id: Some("native-event".into()),
            detail: None,
            next_action: Some("inspect result".into()),
            references: AssistantAssignmentReferencePatch {
                receipt_id: status
                    .is_terminal()
                    .then(|| format!("receipt-{assignment_id}")),
                ..Default::default()
            },
            observed_at: at(sequence as u32),
        }
    }

    #[test]
    fn mixed_executor_query_survives_reopen_with_exact_provenance() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        for (id, kind) in [
            ("a-internal", AssistantAssignmentKind::InternalTurnWorker),
            ("b-external", AssistantAssignmentKind::ExternalPeer),
            ("c-job", AssistantAssignmentKind::Job),
            ("d-workflow", AssistantAssignmentKind::Workflow),
            ("e-scheduled", AssistantAssignmentKind::ScheduledOccurrence),
        ] {
            store
                .create_assistant_assignment(&assignment(id, kind))
                .unwrap();
        }
        store
            .record_assistant_assignment_event(
                &authority(),
                &event(
                    "b-external",
                    "external-running",
                    1,
                    AssistantAssignmentStatus::Running,
                ),
            )
            .unwrap();
        drop(store);

        let reopened = CoordinationStore::open(temp.path()).unwrap();
        let page = reopened
            .list_assistant_assignments(
                &authority(),
                &AssistantAssignmentListFilter {
                    principal_id: Some("user:alice".into()),
                    ..Default::default()
                },
                10,
                None,
            )
            .unwrap();
        assert_eq!(page.assignments.len(), 5);
        assert_eq!(
            page.assignments[1].assignment.status,
            AssistantAssignmentStatus::Running
        );
        assert_eq!(
            page.assignments
                .iter()
                .map(|view| view.assignment.execution.kind)
                .collect::<Vec<_>>(),
            vec![
                AssistantAssignmentKind::InternalTurnWorker,
                AssistantAssignmentKind::ExternalPeer,
                AssistantAssignmentKind::Job,
                AssistantAssignmentKind::Workflow,
                AssistantAssignmentKind::ScheduledOccurrence,
            ]
        );
        assert!(page.assignments.iter().all(|view| {
            view.assignment.execution.native_id.starts_with("native-")
                && view.assignment.execution.forge_work_id.as_deref() == Some("work-1")
                && view.assignment.references.grant_id.as_deref() == Some("grant-1")
        }));
    }

    #[test]
    fn duplicate_and_out_of_order_events_cannot_regress_first_terminal() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        store
            .create_assistant_assignment(&assignment("assignment-1", AssistantAssignmentKind::Job))
            .unwrap();
        let complete = event(
            "assignment-1",
            "completed",
            3,
            AssistantAssignmentStatus::Completed,
        );
        assert!(
            store
                .record_assistant_assignment_event(&authority(), &complete)
                .unwrap()
        );
        assert!(
            !store
                .record_assistant_assignment_event(&authority(), &complete)
                .unwrap()
        );
        store
            .record_assistant_assignment_event(
                &authority(),
                &event(
                    "assignment-1",
                    "late-running",
                    2,
                    AssistantAssignmentStatus::Running,
                ),
            )
            .unwrap();
        assert!(
            store
                .record_assistant_assignment_event(
                    &authority(),
                    &event(
                        "assignment-1",
                        "conflicting-failure",
                        4,
                        AssistantAssignmentStatus::Failed
                    ),
                )
                .is_err()
        );

        let view = store
            .assistant_assignment(&authority(), "assignment-1")
            .unwrap();
        assert_eq!(view.assignment.status, AssistantAssignmentStatus::Completed);
        assert_eq!(view.terminal_event_id.as_deref(), Some("completed"));
        assert_eq!(
            view.assignment.references.receipt_id.as_deref(),
            Some("receipt-assignment-1")
        );
    }

    #[test]
    fn commands_are_claimed_before_effect_and_exact_replay_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        store
            .create_assistant_assignment(&assignment(
                "assignment-1",
                AssistantAssignmentKind::InternalTurnWorker,
            ))
            .unwrap();
        let command = AssistantAssignmentCommand {
            command_id: "command-1".into(),
            assignment_id: "assignment-1".into(),
            action: "cancel".into(),
            actor_id: "assistant:primary".into(),
            correlation_id: "correlation-assignment-1".into(),
            causation_id: Some("turn-2".into()),
            claimed_at: at(1),
        };
        assert_eq!(
            store
                .claim_assistant_assignment_command(&authority(), &command)
                .unwrap(),
            AssistantCommandClaim::Claimed
        );
        assert_eq!(
            store
                .claim_assistant_assignment_command(&authority(), &command)
                .unwrap(),
            AssistantCommandClaim::Existing
        );
        let mut conflict = command;
        conflict.action = "steer".into();
        assert!(
            store
                .claim_assistant_assignment_command(&authority(), &conflict)
                .is_err()
        );
    }
}
