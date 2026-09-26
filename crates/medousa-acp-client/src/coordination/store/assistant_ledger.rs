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

fn is_missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<medousa_store::StoreRootError>()
        .is_some_and(|error| error.is_not_found())
}

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
    /// Projection retries retain the first observation time. Only immutable
    /// origin fields may replay; a different owner or execution is a conflict.
    pub fn ensure_assistant_assignment(&self, value: &AssistantAssignment) -> Result<bool> {
        let mut candidate = value.clone();
        let path = object_path(
            &scope(&value.source.authority_id),
            "assistant-assignment",
            &value.assignment_id,
        )?;
        match self.read::<AssistantAssignment>(&path) {
            Ok(existing) => {
                candidate.created_at = existing.created_at;
                candidate.updated_at = existing.updated_at;
                self.create_assistant_assignment(&candidate)
            }
            Err(error) if is_missing(&error) => match self.create_assistant_assignment(value) {
                Ok(created) => Ok(created),
                Err(error) => {
                    // Another observer may have published the same origin.
                    let existing: AssistantAssignment = self.read(&path).map_err(|_| error)?;
                    candidate.created_at = existing.created_at;
                    candidate.updated_at = existing.updated_at;
                    self.create_assistant_assignment(&candidate)
                }
            },
            Err(error) => Err(error),
        }
    }

    /// A snapshot observation has a stable id, but its wall-clock read time may
    /// differ on replay. Preserve the original observation and reject changes
    /// to evidence under an already-used event id.
    pub fn observe_assistant_assignment_event(
        &self,
        authority_id: &AuthorityId,
        event: &AssistantAssignmentEvent,
    ) -> Result<bool> {
        let path = object_path(&scope(authority_id), "assistant-event", &event.event_id)?;
        let mut candidate = event.clone();
        match self.read::<AssistantAssignmentEvent>(&path) {
            Ok(existing) => candidate.observed_at = existing.observed_at,
            Err(error) if is_missing(&error) => {}
            Err(error) => return Err(error),
        }
        match self.record_assistant_assignment_event(authority_id, &candidate) {
            Ok(created) => Ok(created),
            Err(error) => {
                // Two snapshot readers can race the first observation.
                let existing: AssistantAssignmentEvent = self.read(&path).map_err(|_| error)?;
                candidate.observed_at = existing.observed_at;
                self.record_assistant_assignment_event(authority_id, &candidate)
            }
        }
    }

    pub(crate) fn project_external_request(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<()> {
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
        self.ensure_assistant_assignment(&origin)?;
        Ok(())
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
        if origin.source.authority_id != *authority_id
            || origin.assignment_id != event.assignment_id
        {
            bail!("assistant assignment authority mismatch");
        }

        // Claim the event identity before the terminal fence: a conflicting
        // event id must never publish a terminal that the caller rejected.
        let created = self.create(
            &object_path(&scope(authority_id), "assistant-event", &event.event_id)?,
            event,
        )?;
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
        Ok(created)
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
        let path = object_path(
            &scope(authority_id),
            "assistant-command",
            &command.command_id,
        )?;
        match self.read::<AssistantAssignmentCommand>(&path) {
            Ok(existing) if existing == *command => return Ok(AssistantCommandClaim::Existing),
            Ok(_) => bail!("conflicting assistant assignment command"),
            Err(error) if is_missing(&error) => {}
            Err(error) => return Err(error),
        }
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
            validate_assignment(&value)?;
            if entry.name
                != object_path(
                    &scope(&value.source.authority_id),
                    "assistant-assignment",
                    &value.assignment_id,
                )?
                .file_name()
            {
                bail!("assistant assignment storage identity mismatch");
            }
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
        self.assistant_assignment_events_after(
            authority_id,
            assignment_id,
            limit,
            after_native_sequence.map(|sequence| (sequence, "\u{10ffff}")),
        )
    }

    /// Compound cursor preserves events sharing a native sequence at page edges.
    pub fn assistant_assignment_events_after(
        &self,
        authority_id: &AuthorityId,
        assignment_id: &str,
        limit: usize,
        after: Option<(u64, &str)>,
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
            if entry.name
                == object_path(&scope(authority_id), "assistant-event", &event.event_id)?
                    .file_name()
                && event.assignment_id == view.assignment.assignment_id
                && after
                    .is_none_or(|after| (event.native_sequence, event.event_id.as_str()) > after)
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
        validate_assignment(&origin)?;
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
            if entry.name
                == object_path(
                    &scope(&origin.source.authority_id),
                    "assistant-event",
                    &event.event_id,
                )?
                .file_name()
                && event.assignment_id == origin.assignment_id
            {
                validate_event(&event)?;
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
            if event.assignment_id != origin.assignment_id
                || !event.status.is_terminal()
                || events.get(&(event.native_sequence, event.event_id.clone())) != Some(event)
            {
                bail!("assistant terminal fence identity mismatch");
            }
            apply_assignment_event(&mut origin, event).map_err(anyhow::Error::msg)?;
        } else if events.values().any(|event| event.status.is_terminal()) {
            // The process may have stopped between creating the observation
            // and its terminal fence. Native snapshot replay can finish that
            // write; do not claim a completed or still-running execution here.
            origin.status = AssistantAssignmentStatus::Unknown;
            origin.next_action = Some("reconcile native terminal observation".into());
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
    fn incomplete_terminal_projection_is_unknown_until_native_replay() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        let origin = assignment("crash", AssistantAssignmentKind::Job);
        store.create_assistant_assignment(&origin).unwrap();
        let terminal = event("crash", "terminal", 2, AssistantAssignmentStatus::Completed);
        store
            .create(
                &object_path(&scope(&authority()), "assistant-event", "terminal").unwrap(),
                &terminal,
            )
            .unwrap();
        drop(store);
        let store = CoordinationStore::open(temp.path()).unwrap();
        let unresolved = store.assistant_assignment(&authority(), "crash").unwrap();
        assert_eq!(
            unresolved.assignment.status,
            AssistantAssignmentStatus::Unknown
        );
        assert!(unresolved.terminal_event_id.is_none());
        assert!(
            !store
                .record_assistant_assignment_event(&authority(), &terminal)
                .unwrap()
        );
        let repaired = store.assistant_assignment(&authority(), "crash").unwrap();
        assert_eq!(
            repaired.assignment.status,
            AssistantAssignmentStatus::Completed
        );
        assert_eq!(repaired.terminal_event_id.as_deref(), Some("terminal"));
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
    #[test]
    fn projection_replays_keep_first_timestamp_but_reject_changed_identity() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        let mut origin = assignment("retry", AssistantAssignmentKind::Job);
        assert!(store.ensure_assistant_assignment(&origin).unwrap());
        origin.created_at = at(3);
        origin.updated_at = at(3);
        assert!(!store.ensure_assistant_assignment(&origin).unwrap());
        origin.principal_id = "user:mallory".into();
        assert!(store.ensure_assistant_assignment(&origin).is_err());
        let mut observation = event("retry", "running", 1, AssistantAssignmentStatus::Running);
        assert!(
            store
                .observe_assistant_assignment_event(&authority(), &observation)
                .unwrap()
        );
        observation.observed_at = at(4);
        assert!(
            !store
                .observe_assistant_assignment_event(&authority(), &observation)
                .unwrap()
        );
        observation.status = AssistantAssignmentStatus::Completed;
        assert!(
            store
                .observe_assistant_assignment_event(&authority(), &observation)
                .is_err()
        );
        assert_eq!(
            store
                .assistant_assignment(&authority(), "retry")
                .unwrap()
                .assignment
                .status,
            AssistantAssignmentStatus::Running
        );
    }

    #[test]
    fn matching_native_ids_do_not_mix_workshop_events() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        let a = assignment("same-native-id", AssistantAssignmentKind::Job);
        let mut b = a.clone();
        b.source.authority_id = format!("auth_{}", "b".repeat(64)).parse().unwrap();
        b.execution.workshop_authority_id = b.source.authority_id.clone();
        store.create_assistant_assignment(&a).unwrap();
        store.create_assistant_assignment(&b).unwrap();
        store
            .record_assistant_assignment_event(
                &b.source.authority_id,
                &event(
                    &b.assignment_id,
                    "other-workshop-running",
                    1,
                    AssistantAssignmentStatus::Running,
                ),
            )
            .unwrap();
        let view = store
            .assistant_assignment(&a.source.authority_id, &a.assignment_id)
            .unwrap();
        assert_eq!(view.assignment.status, AssistantAssignmentStatus::Pending);
        assert_eq!(view.event_count, 0);
        assert!(
            store
                .assistant_assignment_events(&a.source.authority_id, &a.assignment_id, 10, None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn event_cursor_retains_observations_with_equal_sequence() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        store
            .create_assistant_assignment(&assignment("a", AssistantAssignmentKind::Job))
            .unwrap();
        for id in ["first", "second"] {
            store
                .record_assistant_assignment_event(
                    &authority(),
                    &event("a", id, 1, AssistantAssignmentStatus::Running),
                )
                .unwrap();
        }
        let page = store
            .assistant_assignment_events_after(&authority(), "a", 1, None)
            .unwrap();
        assert_eq!(page[0].event_id, "first");
        let next = store
            .assistant_assignment_events_after(
                &authority(),
                "a",
                1,
                Some((page[0].native_sequence, &page[0].event_id)),
            )
            .unwrap();
        assert_eq!(next[0].event_id, "second");
    }

    #[test]
    fn exact_command_replay_remains_safe_after_completion() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        store
            .create_assistant_assignment(&assignment("a", AssistantAssignmentKind::Job))
            .unwrap();
        let command = AssistantAssignmentCommand {
            command_id: "cancel-a".into(),
            assignment_id: "a".into(),
            action: "cancel".into(),
            actor_id: "user:alice".into(),
            correlation_id: "corr".into(),
            causation_id: None,
            claimed_at: at(1),
        };
        store
            .claim_assistant_assignment_command(&authority(), &command)
            .unwrap();
        store
            .record_assistant_assignment_event(
                &authority(),
                &event("a", "cancelled", 2, AssistantAssignmentStatus::Cancelled),
            )
            .unwrap();
        assert_eq!(
            store
                .claim_assistant_assignment_command(&authority(), &command)
                .unwrap(),
            AssistantCommandClaim::Existing
        );
        let mut fresh = command;
        fresh.command_id = "other".into();
        assert!(
            store
                .claim_assistant_assignment_command(&authority(), &fresh)
                .is_err()
        );
    }
}
