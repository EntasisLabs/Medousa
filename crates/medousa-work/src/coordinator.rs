//! Runtime-owned intake over the retained journal. Registration is part of the
//! provider request transaction, before dispatch can claim an external effect.
use super::*;

pub const COORDINATOR_ACTOR: &str = "adapter:work-coordinator";
const PREFIX: &str = "work-intake:";

pub fn inbox_id(conversation: &str, request: &str) -> Result<String> {
    Ok(format!("{PREFIX}{}", digest(&(conversation, request))?))
}

#[derive(Debug, Clone)]
pub struct CoordinatorInbox {
    pub domain: UserDomainRef,
    pub subscription_id: String,
    pub conversation_id: String,
    pub request_id: String,
}

pub struct CoordinatorInboxPage {
    pub inboxes: Vec<CoordinatorInbox>,
    pub dispatches: Vec<(
        UserDomainRef,
        medousa_types::work_provider::WorkProviderDispatch,
    )>,
    pub wakes: Vec<(
        UserDomainRef,
        medousa_types::work_coordinator::WorkCoordinatorWakeRecord,
    )>,
    pub next_cursor: Option<String>,
}

impl Snapshot {
    pub(super) fn subscribe_provider_coordinator(
        &mut self,
        request: &medousa_types::work_provider::WorkProviderRequest,
    ) -> Result<()> {
        self.subscribe(
            WorkSubscriptionInput {
                subscription_id: inbox_id(&request.conversation_id, &request.request_id)?,
                work_unit_id: request.input.work_unit_id.clone(),
                expected_scope_revision: request.input.expected_scope_revision,
                resources: vec![],
                event_kinds: vec![
                    WorkEventKind::ProviderProgress,
                    WorkEventKind::ProviderCompleted,
                    WorkEventKind::ProviderFailed,
                ],
                after_revision: self.revision - 1,
                // Retain late callbacks for observation; this does not extend
                // execution or approval authority beyond the request deadline.
                expires_at: request.input.deadline + chrono::Duration::days(1),
            },
            &RecordProvenance {
                actor_id: COORDINATOR_ACTOR.into(),
                source: RecordSource::SystemEvent,
                evidence: vec![],
            },
        )
    }
}

impl WorkGraphStore {
    /// A bounded rotating scan, independent of the selected profile or source
    /// chat. Invalid snapshots are retained and skipped, never overwritten.
    pub fn coordinator_inboxes(
        &self,
        authority: &medousa_types::AuthorityId,
        limit: usize,
        after: Option<&str>,
    ) -> Result<CoordinatorInboxPage> {
        if !(1..=4).contains(&limit) {
            return Err(invalid("invalid coordinator recovery page"));
        }
        let mut entries = self.transaction.root().list_root_utf8()?;
        if entries.len() > 32768 {
            return Err(invalid("coordinator recovery scan budget exhausted"));
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut inboxes = vec![];
        let mut dispatches = vec![];
        let mut wakes = vec![];
        let mut cursor = None;
        let mut scanned = 0;
        for entry in entries {
            let end = format!("{}~", entry.name);
            if !entry.name.ends_with(".json") || after.is_some_and(|a| end.as_str() <= a) {
                continue;
            }
            if scanned == 32 {
                return Ok(CoordinatorInboxPage {
                    inboxes,
                    dispatches,
                    wakes,
                    next_cursor: cursor,
                });
            }
            scanned += 1;
            let snapshot = (|| -> Result<Snapshot> {
                #[derive(Deserialize)]
                struct Header {
                    domain: UserDomainRef,
                }
                let bytes = self
                    .transaction
                    .root()
                    .read_limited(&StorePath::parse(&entry.name)?, MAX_SNAPSHOT_BYTES as u64)?;
                let header: Header =
                    serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
                if Self::path(&header.domain, "json")?.to_string() != entry.name {
                    return Err(invalid("coordinator domain identity mismatch"));
                }
                drop(bytes);
                self.load(&header.domain)
            })();
            let snapshot = match snapshot {
                Ok(snapshot) => snapshot,
                Err(_) => {
                    cursor = Some(end);
                    continue;
                }
            };
            if &snapshot.domain.authority_id == authority {
                let mut records = BTreeMap::new();
                for record in snapshot.provider_requests.values() {
                    records.insert(
                        inbox_id(&record.request.conversation_id, &record.request.request_id)?,
                        record,
                    );
                }
                for (id, record) in records {
                    let position = format!("{}#{id}", entry.name);
                    if after.is_some_and(|a| position.as_str() <= a) {
                        continue;
                    }
                    let Some(subscription) = snapshot.subscriptions.get(&id) else {
                        continue;
                    };
                    if subscription.recipient_actor_id != COORDINATOR_ACTOR
                        || snapshot.pending_events(subscription).is_empty()
                    {
                        continue;
                    }
                    inboxes.push(CoordinatorInbox {
                        domain: snapshot.domain.clone(),
                        subscription_id: id,
                        conversation_id: record.request.conversation_id.clone(),
                        request_id: record.request.request_id.clone(),
                    });
                    cursor = Some(position);
                    if inboxes.len() + dispatches.len() + wakes.len() == limit {
                        return Ok(CoordinatorInboxPage {
                            inboxes,
                            dispatches,
                            wakes,
                            next_cursor: cursor,
                        });
                    }
                }
                for (id, record) in &snapshot.provider_dispatches {
                    let position = format!("{}${id}", entry.name);
                    if after.is_some_and(|a| position.as_str() <= a)
                        || !snapshot.dispatch_pending(record)
                    {
                        continue;
                    }
                    dispatches.push((snapshot.domain.clone(), record.dispatch.clone()));
                    cursor = Some(position);
                    if inboxes.len() + dispatches.len() + wakes.len() == limit {
                        return Ok(CoordinatorInboxPage {
                            inboxes,
                            dispatches,
                            wakes,
                            next_cursor: cursor,
                        });
                    }
                }
                for (id, record) in &snapshot.coordinator_wakes {
                    let position = format!("{}%{id}", entry.name);
                    if after.is_some_and(|a| position.as_str() <= a)
                        || !snapshot.wake_pending(record)
                    {
                        continue;
                    }
                    wakes.push((snapshot.domain.clone(), record.clone()));
                    cursor = Some(position);
                    if inboxes.len() + dispatches.len() + wakes.len() == limit {
                        return Ok(CoordinatorInboxPage {
                            inboxes,
                            dispatches,
                            wakes,
                            next_cursor: cursor,
                        });
                    }
                }
            }
            cursor = Some(end);
        }
        Ok(CoordinatorInboxPage {
            inboxes,
            dispatches,
            wakes,
            next_cursor: None,
        })
    }
}

impl WorkGraphStore {
    /// Only the newest claimed stage may project state. An older terminal must
    /// not overwrite a newer stage admitted before intake caught up.
    pub fn provider_stage_is_current(
        &self,
        domain: &UserDomainRef,
        conversation: &str,
        request: &str,
    ) -> Result<bool> {
        self.load(domain)?
            .provider_stage_is_current(conversation, request)
    }
}

impl Snapshot {
    pub(super) fn provider_stage_is_current(
        &self,
        conversation: &str,
        request: &str,
    ) -> Result<bool> {
        let record = self
            .provider_requests
            .values()
            .find(|record| {
                record.request.conversation_id == conversation
                    && record.request.request_id == request
            })
            .ok_or_else(|| invalid("unknown provider stage"))?;
        let latest =
            self.commands
                .values()
                .filter_map(|command| {
                    let WorkGraphMutation::ClaimProviderRequest {
                        conversation_id,
                        request_id,
                    } = &command.command.mutation
                    else {
                        return None;
                    };
                    let stage = self.provider_requests.values().find(|stage| {
                        stage.request.conversation_id == *conversation_id
                            && stage.request.request_id == *request_id
                    })?;
                    (stage.request.input.work_unit_id == record.request.input.work_unit_id)
                        .then_some((command.receipt.revision, conversation_id, request_id))
                })
                .max_by_key(|(revision, _, _)| *revision);
        Ok(latest.is_some_and(|(_, c, r)| c == conversation && r == request))
    }

    pub(super) fn advance_provider_stage(
        &mut self,
        id: &str,
        revision: u64,
        state: Option<WorkUnitState>,
        reason: &str,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        use medousa_types::{ExternalEventKind, work_provider::WorkProviderQualification};
        if provenance.source != RecordSource::SystemEvent
            || provenance.actor_id != COORDINATOR_ACTOR
        {
            return Err(invalid(
                "provider stage decisions require native runtime custody",
            ));
        }
        let subscription = self
            .subscriptions
            .get(id)
            .filter(|s| s.recipient_actor_id == COORDINATOR_ACTOR)
            .ok_or_else(|| invalid("runtime provider inbox missing"))?;
        let pending = self.pending_events(subscription);
        let command = pending
            .first()
            .filter(|c| c.receipt.revision == revision)
            .ok_or_else(|| {
                invalid("provider stage decision must consume the next pending event")
            })?;
        let WorkGraphMutation::RecordProviderEvent { event } = &command.command.mutation else {
            return Err(invalid("provider stage has no callback"));
        };
        let event = event.clone();
        let record = self
            .provider_requests
            .values()
            .find(|record| {
                record.request.conversation_id == event.conversation_id
                    && record.request.request_id == event.request_id
            })
            .ok_or_else(|| invalid("provider stage request missing"))?;
        let request = record.request.clone();
        if let Some(state) = state {
            if inbox_id(&event.conversation_id, &event.request_id)? != id
                || !self.provider_stage_is_current(&event.conversation_id, &event.request_id)?
            {
                return Err(invalid(
                    "superseded or unrelated provider stage cannot project work",
                ));
            }
            let unit = &self.work_units[&request.input.work_unit_id];
            if unit.kind != WorkUnitKind::Finite
                || unit.scope_revision != request.input.expected_scope_revision
                || !matches!(
                    unit.state,
                    WorkUnitState::Accepted | WorkUnitState::Active | WorkUnitState::Waiting
                )
                || !unit.scope.children.is_empty()
                || !unit.scope.depends_on.is_empty()
            {
                return Err(invalid(
                    "provider stage requires current standalone active work",
                ));
            }
            let allowed = match state {
                WorkUnitState::Satisfied => {
                    event.kind == ExternalEventKind::Completed
                        && event.qualification == WorkProviderQualification::ReviewApproved
                        && request.input.deadline > Utc::now()
                }
                WorkUnitState::Waiting => {
                    event.kind == ExternalEventKind::Completed
                        && event.qualification == WorkProviderQualification::OutcomeOnly
                        && request.reviewed.is_none()
                }
                WorkUnitState::NeedsAttention => matches!(
                    event.kind,
                    ExternalEventKind::Question
                        | ExternalEventKind::Failed
                        | ExternalEventKind::Completed
                ),
                _ => false,
            };
            if !allowed {
                return Err(invalid(
                    "provider evidence cannot justify this stage transition",
                ));
            }
            let reference = ResourceRef {
                authority_id: self.domain.authority_id.clone(),
                kind: ResourceKind::Assignment,
                id: format!(
                    "provider-request:{}",
                    digest(&(&request.conversation_id, &request.request_id))?
                ),
            };
            self.mutate(WorkGraphMutation::RecordResource {
                reference: reference.clone(), native_revision: Some(digest(&(&request, &event))?), resolution: ResourceResolution::Available,
                locator: Some(serde_json::json!({"source":"provider_work_stage", "conversation_id":request.conversation_id, "request_id":request.request_id, "provider":request.provider, "event_id":event.event_id, "kind":event.kind, "qualification":event.qualification, "reviewed":request.reviewed}).to_string()),
            }, provenance.clone())?;
            let mut proof = provenance.clone();
            proof.evidence = vec![reference.clone()];
            self.mutate(
                WorkGraphMutation::SetState {
                    work_unit_id: request.input.work_unit_id.clone(),
                    state,
                    reason: reason.into(),
                    evidence: vec![reference],
                },
                proof,
            )?;
        }
        self.acknowledge_event(id, revision, reason, provenance)
    }
}
