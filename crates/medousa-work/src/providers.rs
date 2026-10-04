//! Native provider evidence. Model intent and transport acknowledgments cannot
//! publish these records or turn them into work satisfaction.
use super::*;
use medousa_types::{ExternalEventKind, work_provider::*};

fn key(conversation: &str, request: &str) -> Result<String> {
    identifier(conversation)?;
    identifier(request)?;
    digest(&("provider-work-v1", conversation, request))
}

impl WorkGraphStore {
    pub fn provider_request(
        &self,
        domain: &UserDomainRef,
        conversation: &str,
        request: &str,
    ) -> Result<Option<WorkProviderRecord>> {
        Ok(self
            .load(domain)?
            .provider_requests
            .get(&key(conversation, request)?)
            .cloned())
    }

    pub fn require_provider_idle(&self, domain: &UserDomainRef, work_unit_id: &str) -> Result<()> {
        let snapshot = self.load(domain)?;
        if snapshot.provider_dispatches.values().any(|record| {
            record.dispatch.input.work_unit_id == work_unit_id && snapshot.dispatch_pending(record)
        }) {
            return Err(invalid("work has saved provider handoff custody"));
        }
        if snapshot.provider_requests.values().any(|record| {
            record.request.input.work_unit_id == work_unit_id
                && record.dispatch_claimed
                && !record.events.iter().any(|event| {
                    matches!(
                        event.kind,
                        ExternalEventKind::Completed | ExternalEventKind::Failed
                    )
                })
        }) {
            return Err(invalid(
                "provider dispatch custody is unresolved; inspect without replacement",
            ));
        }
        Ok(())
    }

    /// Native adapters retain one command identity before effects. Reconciliation
    /// retries the original CAS/digest, never a new event or a provider send.
    pub fn apply_native_command(
        &self,
        domain: &UserDomainRef,
        command_id: String,
        mutation: WorkGraphMutation,
        provenance: RecordProvenance,
    ) -> Result<WorkGraphReceipt> {
        self.apply_native_command_checked(domain, command_id, mutation, provenance, |_| Ok(()))
    }

    /// Cross-ledger admission runs under the same work transaction as custody.
    pub fn apply_native_command_checked(
        &self,
        domain: &UserDomainRef,
        command_id: String,
        mutation: WorkGraphMutation,
        provenance: RecordProvenance,
        check: impl Fn(&WorkGraphMutation) -> Result<()>,
    ) -> Result<WorkGraphReceipt> {
        if provenance.source != RecordSource::SystemEvent {
            return Err(invalid("native command requires system provenance"));
        }
        for _ in 0..3 {
            let snapshot = self.load(domain)?;
            let command = if let Some(existing) = snapshot.commands.get(&command_id) {
                if existing.command.mutation != mutation || existing.provenance != provenance {
                    return Err(invalid(
                        "native event identity describes different evidence",
                    ));
                }
                existing.command.clone()
            } else {
                WorkGraphCommand {
                    command_id: command_id.clone(),
                    expected_revision: snapshot.revision,
                    mutation: mutation.clone(),
                }
            };
            match self.apply_checked(domain, command, provenance.clone(), &check) {
                Err(error) if error.kind == PersistenceErrorKind::Conflict => continue,
                other => return other,
            }
        }
        Err(error(
            PersistenceErrorKind::Conflict,
            "native event publication conflicted; retained source must retry",
        ))
    }
}

impl Snapshot {
    pub(super) fn register_provider_request(
        &mut self,
        request: WorkProviderRequest,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        if provenance.source != RecordSource::SystemEvent {
            return Err(invalid(
                "provider request association requires a native adapter",
            ));
        }
        self.validate_provider_request(&request)?;
        let unit = &self.work_units[&request.input.work_unit_id];
        if unit.scope_revision != request.input.expected_scope_revision
            || unit.state.is_terminal()
            || unit.state == WorkUnitState::Paused
        {
            return Err(invalid(
                "provider request requires the current admitted work scope",
            ));
        }
        let now = Utc::now();
        if request.input.deadline <= now || request.input.deadline - now > chrono::Duration::days(1)
        {
            return Err(invalid("provider request deadline must be within 24 hours"));
        }
        let id = key(&request.conversation_id, &request.request_id)?;
        if self.provider_requests.contains_key(&id) {
            return Err(invalid(
                "provider request already associated; replay the original command",
            ));
        }
        self.subscribe_provider_coordinator(&request)?;
        self.provider_requests.insert(
            id,
            WorkProviderRecord {
                request,
                events: vec![],
                dispatch_claimed: false,
                revision: self.revision,
            },
        );
        Ok(())
    }

    pub(super) fn claim_provider_request(
        &mut self,
        conversation_id: &str,
        request_id: &str,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        if provenance.source != RecordSource::SystemEvent {
            return Err(invalid(
                "provider dispatch custody requires a native adapter",
            ));
        }
        let id = key(conversation_id, request_id)?;
        let request = &self
            .provider_requests
            .get(&id)
            .ok_or_else(|| invalid("unknown provider request"))?
            .request;
        self.validate_provider_dispatch(request)?;
        let record = self
            .provider_requests
            .get_mut(&id)
            .ok_or_else(|| invalid("unknown provider request"))?;
        if record.dispatch_claimed {
            return Err(invalid(
                "provider dispatch is already claimed; never replace uncertain custody",
            ));
        }
        record.dispatch_claimed = true;
        record.revision = self.revision;
        Ok(())
    }

    pub(super) fn record_provider_event(
        &mut self,
        event: WorkProviderEvent,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        if provenance.source != RecordSource::SystemEvent {
            return Err(invalid(
                "provider evidence requires an authenticated native adapter",
            ));
        }
        if provenance.actor_id != event.actor_id {
            return Err(invalid(
                "provider event actor must match authenticated provenance",
            ));
        }
        if self.provider_requests.values().any(|record| {
            record.request.conversation_id == event.conversation_id
                && record.request.request_id != event.request_id
                && record
                    .events
                    .iter()
                    .any(|old| old.event_id == event.event_id)
        }) {
            return Err(invalid(
                "provider event identity is already associated with another request",
            ));
        }
        let record = self
            .provider_requests
            .get(&key(&event.conversation_id, &event.request_id)?)
            .ok_or_else(|| invalid("provider event has no exact work request"))?;
        if !record.dispatch_claimed {
            return Err(invalid("provider request has no durable dispatch claim"));
        }
        self.validate_provider_event(&record.request, &event)?;
        if event.request_sequence != record.events.len() as u64 + 1 {
            return Err(invalid("provider request event sequence is not next"));
        }
        if record.events.len() >= 64 {
            return Err(error(
                PersistenceErrorKind::Overloaded,
                "provider work event capacity reached; source retained for reconciliation",
            ));
        }
        if record.events.iter().any(|old| {
            old.event_id == event.event_id || old.request_sequence == event.request_sequence
        }) {
            return Err(invalid(
                "provider event identity already committed; replay the original command",
            ));
        }
        if matches!(
            event.kind,
            ExternalEventKind::Completed | ExternalEventKind::Failed
        ) && record.events.iter().any(|old| {
            matches!(
                old.kind,
                ExternalEventKind::Completed | ExternalEventKind::Failed
            )
        }) {
            return Err(invalid("provider terminal evidence is immutable"));
        }
        let record = self
            .provider_requests
            .get_mut(&key(&event.conversation_id, &event.request_id)?)
            .unwrap();
        record.events.push(event);
        record.revision = self.revision;
        Ok(())
    }

    fn validate_provider_request(&self, request: &WorkProviderRequest) -> Result<()> {
        key(&request.conversation_id, &request.request_id)?;
        text(&request.scope_digest, 64)?;
        if request.scope_digest.len() != 64
            || !request.scope_digest.bytes().all(|b| b.is_ascii_hexdigit())
            || request.instruction_digest.len() != 64
            || !request
                .instruction_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("provider request instruction digest is invalid"));
        }
        let unit = self
            .work_units
            .get(&request.input.work_unit_id)
            .ok_or_else(|| invalid("unknown provider work unit"))?;
        text(&request.completion_condition, 4096)?;
        if request.completion_condition != unit.completion_condition {
            return Err(invalid(
                "provider completion condition must match saved work",
            ));
        }
        if request.input.expected_scope_revision == 0
            || request.input.expected_scope_revision > unit.revision
        {
            return Err(invalid("invalid provider scope revision"));
        }
        match (&request.input.review_of, &request.reviewed) {
            (None, None) => {}
            (Some(source), Some(input))
                if source.channel.authority_id == self.domain.authority_id
                    && source.executor_assignment_id == input.executor_assignment_id
                    && input.work_unit_id == request.input.work_unit_id =>
            {
                identifier(&input.coordination_id)?;
                identifier(&input.executor_receipt_id)?;
                identifier(&input.forge_work_id)?;
                text(&input.branch, 1024)?;
                if input.head_oid.len() != 40
                    || !input.head_oid.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(invalid("invalid provider review revision"));
                }
            }
            _ => {
                return Err(invalid(
                    "provider review must pin the exact admitted native source",
                ));
            }
        }
        Ok(())
    }

    fn validate_provider_event(
        &self,
        request: &WorkProviderRequest,
        event: &WorkProviderEvent,
    ) -> Result<()> {
        identifier(&event.event_id)?;
        identifier(&event.actor_id)?;
        if event.conversation_id != request.conversation_id
            || event.request_id != request.request_id
            || event.request_sequence == 0
            || event.text.len() > 16 * 1024
            || event.text.contains('\0')
            || !matches!(
                event.kind,
                ExternalEventKind::Progress
                    | ExternalEventKind::Question
                    | ExternalEventKind::Completed
                    | ExternalEventKind::Failed
            )
        {
            return Err(invalid("invalid correlated provider event"));
        }
        match (&event.review_decision, event.qualification) {
            (
                Some(decision),
                WorkProviderQualification::ReviewApproved
                | WorkProviderQualification::ChangesRequested,
            ) if event.kind == ExternalEventKind::Completed
                && request.reviewed.as_ref() == Some(&decision.reviewed)
                && serde_json::from_str::<medousa_types::work_coordination::WorkReviewDecision>(
                    &event.text,
                )
                .ok()
                .as_ref() == Some(decision)
                && !decision.summary.trim().is_empty()
                && decision.summary.len() <= 4096
                && event.text.len() <= 8192
                && ((decision.verdict
                    == medousa_types::work_coordination::WorkReviewVerdict::Approved)
                    == (event.qualification == WorkProviderQualification::ReviewApproved)) => {}
            (None, qualification)
                if !matches!(
                    qualification,
                    WorkProviderQualification::ReviewApproved
                        | WorkProviderQualification::ChangesRequested
                ) => {}
            _ => {
                return Err(invalid(
                    "provider review qualification lacks the exact verdict envelope",
                ));
            }
        }
        Ok(())
    }

    fn validate_provider_dispatch(&self, request: &WorkProviderRequest) -> Result<()> {
        self.validate_dispatch_claim(request)?;
        let unit = &self.work_units[&request.input.work_unit_id];
        if self.provider_requests.values().any(|record| {
            record.request.input.work_unit_id == unit.work_unit_id
                && record.dispatch_claimed
                && !record.events.iter().any(|event| {
                    matches!(
                        event.kind,
                        ExternalEventKind::Completed | ExternalEventKind::Failed
                    )
                })
        }) {
            return Err(invalid(
                "provider dispatch custody is unresolved; never replace it",
            ));
        }
        if unit.scope_revision != request.input.expected_scope_revision
            || unit.kind != WorkUnitKind::Finite
            || !matches!(
                unit.state,
                WorkUnitState::Accepted | WorkUnitState::Active | WorkUnitState::Waiting
            )
            || !unit.scope.children.is_empty()
            || !unit.scope.depends_on.is_empty()
            || self.budget_usage(&unit.work_unit_id).concurrent_executions != 0
            || request.input.deadline <= Utc::now()
        {
            return Err(invalid("provider dispatch scope is no longer admitted"));
        }
        let resources = unit
            .scope
            .resources
            .iter()
            .map(|reference| {
                let record = self
                    .resources
                    .get(&reference_key(reference)?)
                    .ok_or_else(|| invalid("provider scope resource missing"))?;
                Ok((
                    reference,
                    record.revision,
                    record.resolution,
                    &record.native_revision,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        if digest(&(unit.scope_revision, &unit.scope, resources))? != request.scope_digest {
            return Err(invalid("provider scope resource versions changed"));
        }
        let mut visited = std::collections::BTreeSet::new();
        let mut pending = vec![unit.work_unit_id.clone()];
        while let Some(id) = pending.pop() {
            if !visited.insert(id.clone()) {
                continue;
            }
            if self.work_units[&id].budget.is_some() {
                return Err(invalid(
                    "provider cost is unmetered; budgeted work requires a metered adapter",
                ));
            }
            pending.extend(
                self.work_units
                    .iter()
                    .filter(|(_, parent)| parent.scope.children.contains(&id))
                    .map(|(id, _)| id.clone()),
            );
        }
        Ok(())
    }

    pub(super) fn validate_provider_records(&self) -> Result<()> {
        for (id, record) in &self.provider_requests {
            self.validate_provider_request(&record.request)?;
            if id != &key(&record.request.conversation_id, &record.request.request_id)?
                || record.revision == 0
                || record.revision > self.revision
                || record.events.len() > 64
            {
                return Err(invalid("invalid provider work record"));
            }
            let mut terminals = 0;
            for (i, event) in record.events.iter().enumerate() {
                self.validate_provider_event(&record.request, event)?;
                if !record.dispatch_claimed || event.request_sequence != i as u64 + 1 {
                    return Err(invalid("provider event lacks ordered dispatch custody"));
                }
                if record.events[..i].iter().any(|old| {
                    old.event_id == event.event_id || old.request_sequence == event.request_sequence
                }) {
                    return Err(invalid("duplicate provider work evidence"));
                }
                terminals += usize::from(matches!(
                    event.kind,
                    ExternalEventKind::Completed | ExternalEventKind::Failed
                ));
            }
            if terminals > 1 {
                return Err(invalid("provider terminal evidence is immutable"));
            }
        }
        Ok(())
    }
}
