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
                expires_at: request.input.deadline,
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
                    if inboxes.len() == limit {
                        return Ok(CoordinatorInboxPage {
                            inboxes,
                            next_cursor: cursor,
                        });
                    }
                }
            }
            cursor = Some(end);
        }
        Ok(CoordinatorInboxPage {
            inboxes,
            next_cursor: None,
        })
    }
}
