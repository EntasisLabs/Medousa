//! Saved authority for one native or provider predecessor handoff.
use super::*;
use medousa_types::work_provider::*;

pub const PROVIDER_DISPATCH_ACTOR: &str = "adapter:provider-dispatch";

pub(super) fn dispatch_key(conversation: &str, request: &str) -> Result<String> {
    identifier(conversation)?;
    identifier(request)?;
    digest(&("provider-dispatch-v1", conversation, request))
}

impl WorkGraphStore {
    pub fn provider_dispatch(
        &self,
        domain: &UserDomainRef,
        conversation: &str,
        request: &str,
    ) -> Result<Option<WorkProviderDispatchRecord>> {
        Ok(self
            .load(domain)?
            .provider_dispatches
            .get(&dispatch_key(conversation, request)?)
            .cloned())
    }
}

impl Snapshot {
    pub(super) fn dispatch_pending(&self, record: &WorkProviderDispatchRecord) -> bool {
        record.closed_reason.is_none()
            && !self.provider_requests.values().any(|request| {
                request.request.conversation_id == record.dispatch.conversation_id
                    && request.request.request_id == record.dispatch.request_id
                    && request.dispatch_claimed
            })
    }

    pub(super) fn dispatch_has_future(&self, record: &WorkProviderDispatchRecord) -> bool {
        record.closed_reason.is_none()
            && record
                .dispatch
                .remaining_stages
                .first()
                .is_some_and(|stage| {
                    !self.provider_dispatches.contains_key(
                        &dispatch_key(&stage.input.conversation_id, &stage.input.request_id)
                            .unwrap_or_default(),
                    )
                })
    }

    pub(super) fn dispatch_recoverable(&self, record: &WorkProviderDispatchRecord) -> bool {
        self.dispatch_pending(record) || self.dispatch_has_future(record)
    }

    fn validate_dispatch(&self, dispatch: &WorkProviderDispatch) -> Result<()> {
        dispatch_key(&dispatch.conversation_id, &dispatch.request_id)?;
        text(&dispatch.instructions, 12 * 1024)?;
        for value in [
            &dispatch.scope_digest,
            &dispatch.target_digest,
            &dispatch.source_request_digest,
        ] {
            if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(invalid("invalid provider dispatch digest"));
            }
        }
        let unit = self
            .work_units
            .get(&dispatch.input.work_unit_id)
            .ok_or_else(|| invalid("unknown provider dispatch work"))?;
        if dispatch.input.expected_scope_revision == 0
            || dispatch.input.expected_scope_revision > unit.revision
        {
            return Err(invalid("invalid provider dispatch scope generation"));
        }
        if dispatch.remaining_stages.len() > MAX_PROVIDER_CHAIN_STAGES - 2
            || (!dispatch.remaining_stages.is_empty()
                && dispatch.after_provider_completion.is_none())
        {
            return Err(invalid(
                "provider plan requires a bounded provider predecessor",
            ));
        }
        let mut identities = std::collections::BTreeSet::from([(
            dispatch.conversation_id.clone(),
            dispatch.request_id.clone(),
        )]);
        if let Some(source) = &dispatch.after_provider_completion {
            identities.insert((
                source.request.conversation_id.clone(),
                source.request.request_id.clone(),
            ));
        }
        for stage in &dispatch.remaining_stages {
            dispatch_key(&stage.input.conversation_id, &stage.input.request_id)?;
            text(&stage.input.text, 12 * 1024)?;
            if !identities.insert((
                stage.input.conversation_id.clone(),
                stage.input.request_id.clone(),
            )) || stage.target_digest.len() != 64
                || !stage.target_digest.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(invalid(
                    "provider plan contains duplicate identities or invalid destination pins",
                ));
            }
        }
        if dispatch.after_provider_completion.is_some() {
            self.chain_source(dispatch)?;
        } else {
            let source = dispatch.input.review_of.as_ref().ok_or_else(|| {
                invalid("scheduled provider dispatch requires an exact executor source")
            })?;
            identifier(&source.executor_assignment_id)?;
            identifier(&source.channel.channel_id)?;
            if source.channel.authority_id != self.domain.authority_id {
                return Err(invalid(
                    "provider dispatch source belongs to another authority",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn register_provider_dispatch(
        &mut self,
        dispatch: WorkProviderDispatch,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        if provenance.source != RecordSource::SystemEvent
            || provenance.actor_id != PROVIDER_DISPATCH_ACTOR
        {
            return Err(invalid(
                "provider dispatch admission requires native operator custody",
            ));
        }
        self.validate_dispatch(&dispatch)?;
        let unit = &self.work_units[&dispatch.input.work_unit_id];
        let now = Utc::now();
        if unit.scope_revision != dispatch.input.expected_scope_revision
            || !matches!(
                unit.state,
                WorkUnitState::Accepted | WorkUnitState::Active | WorkUnitState::Waiting
            )
            || dispatch.input.deadline <= now
            || dispatch.input.deadline - now > chrono::Duration::days(1)
        {
            return Err(invalid(
                "provider dispatch work or deadline is not admitted",
            ));
        }
        if self.provider_dispatches.values().any(|record| {
            record.dispatch.input.work_unit_id == unit.work_unit_id
                && (self.dispatch_pending(record)
                    || (self.dispatch_has_future(record)
                        && self
                            .next_provider_dispatch(&record.dispatch)
                            .ok()
                            .flatten()
                            .as_ref()
                            != Some(&dispatch)))
        }) || self.provider_requests.values().any(|record| {
            record.request.input.work_unit_id == unit.work_unit_id
                && record.dispatch_claimed
                && !dispatch
                    .after_provider_completion
                    .as_ref()
                    .is_some_and(|source| {
                        source.request.conversation_id == record.request.conversation_id
                            && source.request.request_id == record.request.request_id
                    })
                && !record.events.iter().any(|event| {
                    matches!(
                        event.kind,
                        medousa_types::ExternalEventKind::Completed
                            | medousa_types::ExternalEventKind::Failed
                    )
                })
        }) {
            return Err(invalid(
                "work already has pending provider dispatch custody",
            ));
        }
        if let Some(source) = &dispatch.after_provider_completion {
            if !self.provider_stage_is_current(
                &source.request.conversation_id,
                &source.request.request_id,
            )? {
                return Err(invalid("provider chain predecessor is not current"));
            }
            // Pending is allowed; failed/stale completed evidence is not.
            self.provider_chain_terminal(&dispatch)?;
        }
        let proposed_ids = std::iter::once((&dispatch.conversation_id, &dispatch.request_id))
            .chain(
                dispatch
                    .remaining_stages
                    .iter()
                    .map(|stage| (&stage.input.conversation_id, &stage.input.request_id)),
            )
            .collect::<std::collections::BTreeSet<_>>();
        for record in self.provider_dispatches.values() {
            // Successors repeat the identical frozen suffix from their ancestor.
            let inherited = dispatch
                .after_provider_completion
                .as_ref()
                .is_some_and(|source| {
                    source.request.conversation_id == record.dispatch.conversation_id
                        && source.request.request_id == record.dispatch.request_id
                });
            if !inherited
                && record.dispatch.remaining_stages.iter().any(|stage| {
                    proposed_ids.contains(&(&stage.input.conversation_id, &stage.input.request_id))
                })
            {
                // Earlier ancestors also retain the suffix; prove membership by
                // following immutable predecessor request pins, not labels.
                let mut source = dispatch
                    .after_provider_completion
                    .as_ref()
                    .map(|source| source.request.clone());
                let mut ancestor = false;
                for _ in 0..MAX_PROVIDER_CHAIN_STAGES {
                    let Some(reference) = source else {
                        break;
                    };
                    if reference.conversation_id == record.dispatch.conversation_id
                        && reference.request_id == record.dispatch.request_id
                    {
                        ancestor = true;
                        break;
                    }
                    source = self
                        .provider_requests
                        .values()
                        .find(|request| {
                            request.request.conversation_id == reference.conversation_id
                                && request.request.request_id == reference.request_id
                        })
                        .and_then(|request| {
                            request
                                .request
                                .predecessor
                                .as_ref()
                                .map(|pin| pin.request.clone())
                        });
                }
                if !ancestor {
                    return Err(invalid(
                        "provider stage identity is reserved by another plan",
                    ));
                }
            }
        }
        for stage in &dispatch.remaining_stages {
            if self.provider_dispatches.contains_key(&dispatch_key(
                &stage.input.conversation_id,
                &stage.input.request_id,
            )?) || self.provider_requests.values().any(|record| {
                record.request.conversation_id == stage.input.conversation_id
                    && record.request.request_id == stage.input.request_id
            }) {
                return Err(invalid("future provider stage identity already used"));
            }
        }
        let key = dispatch_key(&dispatch.conversation_id, &dispatch.request_id)?;
        if self.provider_dispatches.contains_key(&key)
            || self.provider_requests.values().any(|record| {
                record.request.conversation_id == dispatch.conversation_id
                    && record.request.request_id == dispatch.request_id
            })
        {
            return Err(invalid("provider dispatch identity already used"));
        }
        self.provider_dispatches.insert(
            key,
            WorkProviderDispatchRecord {
                dispatch,
                closed_reason: None,
                revision: self.revision,
            },
        );
        Ok(())
    }

    pub(super) fn close_provider_dispatch(
        &mut self,
        conversation: &str,
        request: &str,
        reason: &str,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        if provenance.source != RecordSource::SystemEvent
            || provenance.actor_id != PROVIDER_DISPATCH_ACTOR
        {
            return Err(invalid("provider dispatch closure requires native custody"));
        }
        text(reason, 4096)?;
        let key = dispatch_key(conversation, request)?;
        let record = self
            .provider_dispatches
            .get(&key)
            .ok_or_else(|| invalid("unknown provider dispatch"))?;
        if !self.dispatch_recoverable(record) {
            return Err(invalid("provider dispatch is already closed or claimed"));
        }
        let record = self.provider_dispatches.get_mut(&key).unwrap();
        record.closed_reason = Some(reason.into());
        record.revision = self.revision;
        Ok(())
    }

    pub(super) fn validate_dispatch_claim(&self, request: &WorkProviderRequest) -> Result<()> {
        for record in self.provider_dispatches.values().filter(|record| {
            record.dispatch.input.work_unit_id == request.input.work_unit_id
                && self.dispatch_recoverable(record)
        }) {
            let dispatch = &record.dispatch;
            if dispatch.conversation_id != request.conversation_id
                || dispatch.request_id != request.request_id
                || dispatch.input != request.input
                || dispatch.provider != request.provider
                || dispatch.scope_digest != request.scope_digest
                || format!("{:x}", Sha256::digest(dispatch.instructions.as_bytes()))
                    != request.instruction_digest
            {
                return Err(invalid(
                    "provider request cannot replace saved dispatch authority",
                ));
            }
            let expected = if dispatch.after_provider_completion.is_some() {
                Some(
                    self.provider_chain_terminal(dispatch)?
                        .ok_or_else(|| invalid("provider chain predecessor has not completed"))?,
                )
            } else {
                None
            };
            if request.predecessor != expected {
                return Err(invalid("provider successor lacks its exact terminal pin"));
            }
        }
        if request.predecessor.is_some()
            && !self.provider_dispatches.values().any(|record| {
                record.dispatch.conversation_id == request.conversation_id
                    && record.dispatch.request_id == request.request_id
                    && record.dispatch.after_provider_completion.is_some()
                    && record.closed_reason.is_none()
            })
        {
            return Err(invalid("provider chain has no native dispatch admission"));
        }
        if let Some(record) = self.provider_dispatches.get(&dispatch_key(
            &request.conversation_id,
            &request.request_id,
        )?) && record.closed_reason.is_some()
        {
            return Err(invalid("provider dispatch authority is closed"));
        }
        Ok(())
    }

    pub(super) fn validate_provider_dispatches(&self) -> Result<()> {
        for (key, record) in &self.provider_dispatches {
            self.validate_dispatch(&record.dispatch)?;
            if *key
                != dispatch_key(
                    &record.dispatch.conversation_id,
                    &record.dispatch.request_id,
                )?
                || record.revision == 0
                || record.revision > self.revision
            {
                return Err(invalid("invalid retained provider dispatch"));
            }
            if let Some(reason) = &record.closed_reason {
                text(reason, 4096)?;
            }
        }
        Ok(())
    }
}
