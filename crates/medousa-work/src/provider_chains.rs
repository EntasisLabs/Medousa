//! Linear operator-admitted provider chains. A predecessor outcome is reference
//! evidence, never native review approval or authority to choose another target.
use super::*;
use medousa_types::{ExternalEventKind, work_provider::*};
use std::collections::BTreeSet;

pub const MAX_PROVIDER_CHAIN_STAGES: usize = 8;

impl WorkGraphStore {
    pub fn provider_chain_terminal(
        &self,
        domain: &UserDomainRef,
        dispatch: &WorkProviderDispatch,
    ) -> Result<Option<WorkProviderTerminalRef>> {
        self.load(domain)?.provider_chain_terminal(dispatch)
    }

    pub fn provider_terminal_event(
        &self,
        domain: &UserDomainRef,
        pin: &WorkProviderTerminalRef,
    ) -> Result<WorkProviderEvent> {
        Ok(self.load(domain)?.provider_terminal_event(pin)?.clone())
    }
}

impl Snapshot {
    pub(super) fn chain_source(
        &self,
        dispatch: &WorkProviderDispatch,
    ) -> Result<&WorkProviderRecord> {
        let source = dispatch
            .after_provider_completion
            .as_ref()
            .ok_or_else(|| invalid("provider chain source missing"))?;
        identifier(&source.request.conversation_id)?;
        identifier(&source.request.request_id)?;
        if source.target_digest.len() != 64
            || !source.target_digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("invalid provider source destination digest"));
        }
        let record = self.provider_record(&source.request)?;
        let request = &record.request;
        if (source.request.conversation_id == dispatch.conversation_id
            && source.request.request_id == dispatch.request_id)
            || !record.dispatch_claimed
            || dispatch.input.review_of.is_some()
            || request.reviewed.is_some()
            || request.input.work_unit_id != dispatch.input.work_unit_id
            || request.input.expected_scope_revision != dispatch.input.expected_scope_revision
            || request.scope_digest != dispatch.scope_digest
            || dispatch.input.deadline > request.input.deadline
            || digest(request)? != dispatch.source_request_digest
        {
            return Err(invalid(
                "provider chain differs from its exact admitted predecessor scope",
            ));
        }
        if self.chain_depth(request)? >= MAX_PROVIDER_CHAIN_STAGES {
            return Err(invalid("provider chain stage limit reached"));
        }
        Ok(record)
    }

    fn provider_record(&self, reference: &WorkProviderRequestRef) -> Result<&WorkProviderRecord> {
        self.provider_requests
            .values()
            .find(|record| {
                record.request.conversation_id == reference.conversation_id
                    && record.request.request_id == reference.request_id
            })
            .ok_or_else(|| invalid("provider predecessor is not in this owner domain"))
    }

    pub(super) fn chain_depth(&self, request: &WorkProviderRequest) -> Result<usize> {
        let mut depth = 1;
        let mut current = request;
        let mut visited =
            BTreeSet::from([(request.conversation_id.clone(), request.request_id.clone())]);
        while let Some(pin) = &current.predecessor {
            if !visited.insert((
                pin.request.conversation_id.clone(),
                pin.request.request_id.clone(),
            )) || depth >= MAX_PROVIDER_CHAIN_STAGES
            {
                return Err(invalid(
                    "provider chain is cyclic or exceeds its stage limit",
                ));
            }
            current = &self.provider_record(&pin.request)?.request;
            depth += 1;
        }
        Ok(depth)
    }

    pub(super) fn provider_terminal_event(
        &self,
        pin: &WorkProviderTerminalRef,
    ) -> Result<&WorkProviderEvent> {
        identifier(&pin.event_id)?;
        let record = self.provider_record(&pin.request)?;
        let event = record
            .events
            .iter()
            .find(|event| event.event_id == pin.event_id)
            .ok_or_else(|| invalid("exact provider predecessor terminal missing"))?;
        if !record.dispatch_claimed
            || event.kind != ExternalEventKind::Completed
            || event.qualification != WorkProviderQualification::OutcomeOnly
            || event.created_at > record.request.input.deadline
            || digest(event)? != pin.event_digest
        {
            return Err(invalid(
                "provider predecessor terminal pin differs from admitted completion",
            ));
        }
        Ok(event)
    }

    pub(super) fn provider_chain_terminal(
        &self,
        dispatch: &WorkProviderDispatch,
    ) -> Result<Option<WorkProviderTerminalRef>> {
        let record = self.chain_source(dispatch)?;
        let source = dispatch.after_provider_completion.as_ref().unwrap();
        if !self.provider_stage_is_current(
            &source.request.conversation_id,
            &source.request.request_id,
        )? || record.request.input.deadline <= Utc::now()
        {
            return Err(invalid("provider predecessor is expired or superseded"));
        }
        let Some(event) = record.events.iter().find(|event| {
            matches!(
                event.kind,
                ExternalEventKind::Completed | ExternalEventKind::Failed
            )
        }) else {
            return Ok(None);
        };
        let pin = WorkProviderTerminalRef {
            request: source.request.clone(),
            event_id: event.event_id.clone(),
            event_digest: digest(event)?,
        };
        self.provider_terminal_event(&pin)?;
        Ok(Some(pin))
    }

    pub(super) fn validate_predecessor(&self, request: &WorkProviderRequest) -> Result<()> {
        if let Some(pin) = &request.predecessor {
            let source = self.provider_record(&pin.request)?;
            self.provider_terminal_event(pin)?;
            if request.reviewed.is_some()
                || request.input.review_of.is_some()
                || request.input.work_unit_id != source.request.input.work_unit_id
                || request.input.expected_scope_revision
                    != source.request.input.expected_scope_revision
                || request.scope_digest != source.request.scope_digest
                || request.input.deadline > source.request.input.deadline
            {
                return Err(invalid("provider successor differs from predecessor scope"));
            }
            self.chain_depth(request)?;
        }
        Ok(())
    }
}
