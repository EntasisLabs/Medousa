//! A bounded, capability-confined registry of scope and intent. Native content,
//! execution, grants, scheduling, and delivery remain owned by their adapters.
//!
//! Call synchronous methods through the daemon's ForgeExecutionService. Each
//! mutation uses a cross-process writer lock, revision check, and atomic synced
//! snapshot; a command receipt and its effects are published together.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use chrono::Utc;
use fs2::FileExt;
use medousa_store::{
    FileTransaction, PersistenceError, PersistenceErrorKind, StorePath, StoreRoot, StoreRootError,
    TransactionFaults,
};
use medousa_types::work_unit::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

fn lock_exclusive_briefly(file: &std::fs::File) -> std::io::Result<()> {
    let started = std::time::Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    && started.elapsed() < std::time::Duration::from_millis(500) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(error) => return Err(error),
        }
    }
}

mod budget;
mod coordinator;
mod coordinator_wakes;
mod events;
mod provider_chains;
mod provider_dispatch;
mod providers;
pub use coordinator::{COORDINATOR_ACTOR, CoordinatorInbox, CoordinatorInboxPage};
pub use coordinator_wakes::{coordinator_session, coordinator_turn_id};
use medousa_types::work_coordinator::WorkCoordinatorWakeRecord;
use medousa_types::work_provider::{WorkProviderDispatchRecord, WorkProviderRecord};
pub use provider_chains::MAX_PROVIDER_CHAIN_STAGES;
pub use provider_dispatch::PROVIDER_DISPATCH_ACTOR;

#[cfg(test)]
mod tests;

pub const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
pub const MAX_COMMAND_BYTES: usize = 32 * 1024;
const MAX_RECORDS: usize = 4096;
const MAX_SCOPE_MEMBERS: usize = 128;
const SCHEMA_VERSION: u16 = 1;

type Result<T> = std::result::Result<T, PersistenceError>;

#[derive(Clone)]
pub struct WorkGraphStore {
    transaction: FileTransaction,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u16,
    domain: UserDomainRef,
    revision: u64,
    resources: BTreeMap<String, ResourceRecord>,
    relationships: BTreeMap<String, RelationshipRecord>,
    work_units: BTreeMap<String, WorkUnit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    reservations: BTreeMap<String, WorkBudgetReservation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    subscriptions: BTreeMap<String, WorkEventSubscription>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    provider_requests: BTreeMap<String, WorkProviderRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    provider_dispatches: BTreeMap<String, WorkProviderDispatchRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    coordinator_wakes: BTreeMap<String, WorkCoordinatorWakeRecord>,
    commands: BTreeMap<String, CommittedCommand>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommittedCommand {
    digest: String,
    receipt: WorkGraphReceipt,
    command: WorkGraphCommand,
    provenance: RecordProvenance,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    revision: u64,
    query_digest: String,
    after: String,
}

fn error(kind: PersistenceErrorKind, message: impl Into<String>) -> PersistenceError {
    PersistenceError::new(kind, message)
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    error(PersistenceErrorKind::PermanentIo, message)
}

fn digest(value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| error(PersistenceErrorKind::Serialization, e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(invalid(
            "identity must be 1–256 bytes, without surrounding whitespace or control characters",
        ));
    }
    Ok(())
}

fn text(value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        return Err(invalid(format!(
            "text must be nonempty and at most {max} bytes"
        )));
    }
    Ok(())
}

fn reference_key(reference: &ResourceRef) -> Result<String> {
    identifier(&reference.id)?;
    digest(&("medousa-resource-v1", reference))
}

fn work_reference(domain: &UserDomainRef, id: &str) -> ResourceRef {
    ResourceRef {
        authority_id: domain.authority_id.clone(),
        kind: ResourceKind::WorkUnit,
        id: id.to_string(),
    }
}

impl WorkGraphStore {
    pub fn open(root: &Path) -> Result<Self> {
        Ok(Self {
            transaction: FileTransaction::new(Arc::new(StoreRoot::open_or_create_nofollow(root)?)),
        })
    }

    /// Fault injection shares the same publication boundaries as production.
    pub fn with_faults(root: &Path, faults: Arc<dyn TransactionFaults>) -> Result<Self> {
        Ok(Self {
            transaction: FileTransaction::with_faults(
                Arc::new(StoreRoot::open_or_create_nofollow(root)?),
                faults,
            ),
        })
    }

    fn path(domain: &UserDomainRef, extension: &str) -> Result<StorePath> {
        identifier(&domain.user_id)?;
        StorePath::parse(&format!(
            "{}.{}",
            digest(&("medousa-work-domain-v1", domain))?,
            extension
        ))
        .map_err(PersistenceError::from)
    }

    fn load(&self, domain: &UserDomainRef) -> Result<Snapshot> {
        let bytes = match self
            .transaction
            .root()
            .read_limited(&Self::path(domain, "json")?, MAX_SNAPSHOT_BYTES as u64)
        {
            Ok(bytes) => bytes,
            Err(StoreRootError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(Snapshot {
                    schema_version: SCHEMA_VERSION,
                    domain: domain.clone(),
                    revision: 0,
                    resources: BTreeMap::new(),
                    relationships: BTreeMap::new(),
                    work_units: BTreeMap::new(),
                    reservations: BTreeMap::new(),
                    subscriptions: BTreeMap::new(),
                    provider_requests: BTreeMap::new(),
                    provider_dispatches: BTreeMap::new(),
                    coordinator_wakes: BTreeMap::new(),
                    commands: BTreeMap::new(),
                });
            }
            Err(e) => return Err(e.into()),
        };
        let mut snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|e| {
            error(
                PersistenceErrorKind::Corruption,
                format!("work graph cannot be decoded: {e}"),
            )
        })?;
        if snapshot.schema_version != SCHEMA_VERSION || &snapshot.domain != domain {
            return Err(error(
                PersistenceErrorKind::Corruption,
                "work graph schema or domain mismatch",
            ));
        }
        // Additive metadata does not rewrite old commands or their digests.
        // Freeze legacy scope identity at its last stored unit revision.
        for unit in snapshot.work_units.values_mut() {
            if unit.scope_revision == 0 {
                unit.scope_revision = unit.revision;
                if let Some(origin) = &unit.origin
                    && !unit.conversations.contains(origin)
                {
                    unit.conversations.push(origin.clone());
                }
            }
        }
        snapshot
            .validate()
            .map_err(|e| error(PersistenceErrorKind::Corruption, e.to_string()))?;
        Ok(snapshot)
    }

    pub fn apply(
        &self,
        domain: &UserDomainRef,
        command: WorkGraphCommand,
        provenance: RecordProvenance,
    ) -> Result<WorkGraphReceipt> {
        self.apply_checked(domain, command, provenance, |_| Ok(()))
    }

    /// Check current adapter visibility only for a new effect. An exact replay
    /// remains a read of accepted intent even if its originating session has
    /// since been removed; the caller must still authorize domain access.
    pub fn apply_checked(
        &self,
        domain: &UserDomainRef,
        command: WorkGraphCommand,
        provenance: RecordProvenance,
        check: impl FnOnce(&WorkGraphMutation) -> Result<()>,
    ) -> Result<WorkGraphReceipt> {
        identifier(&command.command_id)?;
        identifier(&provenance.actor_id)?;
        if provenance.evidence.len() > MAX_SCOPE_MEMBERS {
            return Err(invalid("too many provenance references"));
        }
        for reference in &provenance.evidence {
            reference_key(reference)?;
        }
        let command_bytes = serde_json::to_vec(&(&command, &provenance))
            .map_err(|e| error(PersistenceErrorKind::Serialization, e.to_string()))?;
        if command_bytes.len() > MAX_COMMAND_BYTES {
            return Err(error(
                PersistenceErrorKind::Overloaded,
                "work graph command exceeds byte budget",
            ));
        }
        let request_digest = format!("{:x}", Sha256::digest(&command_bytes));
        let lock = self
            .transaction
            .root()
            .open_lock_file(&Self::path(domain, "lock")?)?;
        lock_exclusive_briefly(&lock).map_err(|e| {
            error(
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    PersistenceErrorKind::Overloaded
                } else {
                    PersistenceErrorKind::PermanentIo
                },
                format!("work graph writer unavailable: {e}"),
            )
        })?;
        let mut snapshot = self.load(domain)?;
        if let Some(committed) = snapshot.commands.get(&command.command_id) {
            if committed.digest != request_digest {
                return Err(error(
                    PersistenceErrorKind::Conflict,
                    "command identity already describes different intent",
                ));
            }
            let mut receipt = committed.receipt.clone();
            // A previous attempt may have published the synced file but lost
            // the parent-sync/receipt boundary. Complete that fence on replay.
            self.transaction
                .root()
                .sync_parent_of(&Self::path(domain, "json")?)?;
            receipt.replayed = true;
            return Ok(receipt);
        }
        if snapshot.revision != command.expected_revision {
            return Err(error(
                PersistenceErrorKind::Conflict,
                format!(
                    "expected revision {}, current revision {}",
                    command.expected_revision, snapshot.revision
                ),
            ));
        }
        check(&command.mutation)?;
        snapshot.revision = snapshot.revision.checked_add(1).ok_or_else(|| {
            error(
                PersistenceErrorKind::Overloaded,
                "work graph revision exhausted",
            )
        })?;
        snapshot.mutate(command.mutation.clone(), provenance.clone())?;
        let receipt = WorkGraphReceipt {
            command_id: command.command_id.clone(),
            revision: snapshot.revision,
            committed_at: Utc::now(),
            replayed: false,
        };
        snapshot.commands.insert(
            command.command_id.clone(),
            CommittedCommand {
                digest: request_digest,
                receipt: receipt.clone(),
                command,
                provenance,
            },
        );
        snapshot.validate()?;
        let bytes = serde_json::to_vec(&snapshot)
            .map_err(|e| error(PersistenceErrorKind::Serialization, e.to_string()))?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(error(
                PersistenceErrorKind::Overloaded,
                "domain registry is full; no records or replay receipts were discarded",
            ));
        }
        self.transaction
            .replace_snapshot_json(&Self::path(domain, "json")?, &snapshot)?;
        Ok(receipt)
    }

    pub fn work_unit(&self, domain: &UserDomainRef, id: &str) -> Result<WorkUnit> {
        self.inspect_work_unit(domain, id).map(|(unit, _, _)| unit)
    }

    pub fn peer_coordination_scope_digest(
        &self,
        domain: &UserDomainRef,
        id: &str,
    ) -> Result<String> {
        let snapshot = self.load(domain)?;
        let unit = snapshot
            .work_units
            .get(id)
            .ok_or_else(|| invalid("unknown work unit"))?;
        let resources = unit
            .scope
            .resources
            .iter()
            .map(|reference| {
                let record = snapshot
                    .resources
                    .get(&reference_key(reference)?)
                    .ok_or_else(|| invalid("scope resource missing"))?;
                Ok((
                    reference,
                    record.revision,
                    record.resolution,
                    &record.native_revision,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        digest(&(unit.scope_revision, &unit.scope, resources))
    }

    /// Registration holds this nonblocking writer fence through native index
    /// publication, so a model state commit cannot race controller ownership.
    pub fn peer_coordination_custody(
        &self,
        domain: &UserDomainRef,
        id: &str,
        scope_revision: u64,
        scope_digest: &str,
    ) -> Result<std::fs::File> {
        let lock = self
            .transaction
            .root()
            .open_lock_file(&Self::path(domain, "lock")?)?;
        lock.try_lock_exclusive().map_err(|e| {
            error(
                PersistenceErrorKind::Overloaded,
                format!("work registration custody unavailable: {e}"),
            )
        })?;
        self.admit_peer_coordination(domain, id, scope_revision)?;
        if self.peer_coordination_scope_digest(domain, id)? != scope_digest {
            return Err(invalid(
                "work resource revisions changed during registration",
            ));
        }
        Ok(lock)
    }

    /// Native peer execution currently supports standalone finite work. Unknown
    /// provider costs cannot be booked as zero or bypass a parent's budget.
    /// Called again at every native provider effect boundary.
    pub fn admit_peer_coordination(
        &self,
        domain: &UserDomainRef,
        id: &str,
        scope_revision: u64,
    ) -> Result<WorkUnit> {
        let snapshot = self.load(domain)?;
        let unit = snapshot
            .work_units
            .get(id)
            .ok_or_else(|| invalid("unknown work unit"))?;
        if unit.scope_revision != scope_revision
            || unit.kind != WorkUnitKind::Finite
            || !matches!(
                unit.state,
                WorkUnitState::Accepted | WorkUnitState::Active | WorkUnitState::Waiting
            )
            || !unit.scope.children.is_empty()
            || !unit.scope.depends_on.is_empty()
            || snapshot.budget_usage(id).concurrent_executions != 0
        {
            return Err(invalid(
                "work coordination scope or execution custody is not admissible",
            ));
        }
        let mut visited = std::collections::BTreeSet::new();
        let mut pending = vec![id.to_string()];
        while let Some(child) = pending.pop() {
            if !visited.insert(child.clone()) {
                continue;
            }
            if snapshot.work_units[&child].budget.is_some() {
                return Err(invalid(
                    "peer provider cost is unmetered; budgeted work requires a metered native adapter",
                ));
            }
            pending.extend(
                snapshot
                    .work_units
                    .iter()
                    .filter(|(_, parent)| parent.scope.children.contains(&child))
                    .map(|(id, _)| id.clone()),
            );
        }
        Ok(unit.clone())
    }

    /// Freshness and the saved record are read from one snapshot. This is a
    /// projection of recorded native facts, not an implicit native refresh.
    pub fn inspect_work_unit(
        &self,
        domain: &UserDomainRef,
        id: &str,
    ) -> Result<(WorkUnit, bool, WorkBudgetUsage)> {
        identifier(id)?;
        let snapshot = self.load(domain)?;
        let unit = snapshot
            .work_units
            .get(id)
            .ok_or_else(|| invalid("work unit is not recorded in this domain"))?;
        Ok((
            unit.clone(),
            snapshot.readiness_current(unit, Utc::now())?,
            snapshot.budget_usage(id),
        ))
    }

    pub fn query(&self, domain: &UserDomainRef, query: WorkGraphQuery) -> Result<WorkGraphPage> {
        let limit = query.limit.unwrap_or(20);
        if !(1..=100).contains(&limit) {
            return Err(invalid("page size must be between 1 and 100"));
        }
        if let Some(anchor) = &query.anchor {
            reference_key(anchor)?;
        }
        let snapshot = self.load(domain)?;
        let mut cursor_query = query.clone();
        cursor_query.cursor = None;
        let query_digest = digest(&(domain, cursor_query))?;
        let after = match &query.cursor {
            Some(value) => {
                if value.len() > 1024 {
                    return Err(invalid("cursor exceeds byte budget"));
                }
                let cursor: Cursor =
                    serde_json::from_str(value).map_err(|_| invalid("invalid graph cursor"))?;
                if cursor.revision != snapshot.revision || cursor.query_digest != query_digest {
                    return Err(error(
                        PersistenceErrorKind::Conflict,
                        "graph or query changed; restart pagination",
                    ));
                }
                Some(cursor.after)
            }
            None => None,
        };
        let mut rows: Vec<(String, WorkGraphItem)> = match query.collection {
            WorkGraphCollection::Resources => snapshot
                .resources
                .iter()
                .filter(|(_, r)| query.anchor.as_ref().is_none_or(|a| &r.reference == a))
                .map(|(k, r)| (k.clone(), WorkGraphItem::Resource(r.clone())))
                .collect(),
            WorkGraphCollection::Relationships => snapshot
                .relationships
                .iter()
                .filter(|(_, r)| {
                    query.anchor.as_ref().is_none_or(|a| match query.direction {
                        RelationshipDirection::Incoming => &r.to == a,
                        RelationshipDirection::Outgoing => &r.from == a,
                        RelationshipDirection::Both => &r.from == a || &r.to == a,
                    })
                })
                .map(|(k, r)| (k.clone(), WorkGraphItem::Relationship(r.clone())))
                .collect(),
            WorkGraphCollection::WorkUnits => snapshot
                .work_units
                .iter()
                .filter(|(_, unit)| {
                    query.anchor.as_ref().is_none_or(|a| {
                        unit.scope.resources.contains(a)
                            || &work_reference(domain, &unit.work_unit_id) == a
                            || (a.authority_id == domain.authority_id
                                && a.kind == ResourceKind::Session
                                && unit.conversations.iter().any(|session| {
                                    session.authority_id == a.authority_id
                                        && session.session_id.as_str() == a.id
                                }))
                            || (a.authority_id == domain.authority_id
                                && a.kind == ResourceKind::WorkUnit
                                && (unit.scope.children.contains(&a.id)
                                    || unit.scope.depends_on.contains(&a.id)))
                    })
                })
                .map(|(k, r)| (k.clone(), WorkGraphItem::WorkUnit(r.clone())))
                .collect(),
            WorkGraphCollection::Events => {
                if query.anchor.is_some() {
                    return Err(invalid(
                        "event history is domain-scoped; anchor is not supported",
                    ));
                }
                let mut events: Vec<_> = snapshot
                    .commands
                    .values()
                    .map(|c| {
                        (
                            format!("{:020}", c.receipt.revision),
                            WorkGraphItem::Event(WorkGraphEvent {
                                command: c.command.clone(),
                                provenance: c.provenance.clone(),
                                receipt: c.receipt.clone(),
                            }),
                        )
                    })
                    .collect();
                events.sort_by(|a, b| a.0.cmp(&b.0));
                events
            }
            WorkGraphCollection::BudgetReservations => snapshot
                .reservations
                .iter()
                .filter(|(_, reservation)| {
                    query.anchor.as_ref().is_none_or(|anchor| {
                        &reservation.execution == anchor
                            || (anchor.authority_id == domain.authority_id
                                && anchor.kind == ResourceKind::WorkUnit
                                && reservation.charged_units.contains(&anchor.id))
                    })
                })
                .map(|(id, reservation)| {
                    (
                        id.clone(),
                        WorkGraphItem::BudgetReservation(reservation.clone()),
                    )
                })
                .collect(),
            WorkGraphCollection::ProviderRequests => snapshot
                .provider_requests
                .iter()
                .filter(|(_, record)| {
                    query.anchor.as_ref().is_none_or(|anchor| {
                        anchor == &work_reference(domain, &record.request.input.work_unit_id)
                    })
                })
                .map(|(id, record)| (id.clone(), WorkGraphItem::ProviderRequest(record.clone())))
                .collect(),
            WorkGraphCollection::ProviderDispatches => snapshot
                .provider_dispatches
                .iter()
                .filter(|(_, record)| {
                    query.anchor.as_ref().is_none_or(|anchor| {
                        anchor == &work_reference(domain, &record.dispatch.input.work_unit_id)
                    })
                })
                .map(|(id, record)| (id.clone(), WorkGraphItem::ProviderDispatch(record.clone())))
                .collect(),
            WorkGraphCollection::CoordinatorWakes => snapshot
                .coordinator_wakes
                .iter()
                .filter(|(_, record)| {
                    query.anchor.as_ref().is_none_or(|anchor| {
                        anchor == &work_reference(domain, &record.wake.input.work_unit_id)
                    })
                })
                .map(|(id, record)| (id.clone(), WorkGraphItem::CoordinatorWake(record.clone())))
                .collect(),
            WorkGraphCollection::Subscriptions => snapshot
                .subscriptions
                .iter()
                .filter(|(_, subscription)| {
                    query.anchor.as_ref().is_none_or(|anchor| {
                        anchor == &work_reference(domain, &subscription.input.work_unit_id)
                            || subscription.input.resources.contains(anchor)
                    })
                })
                .map(|(id, record)| (id.clone(), WorkGraphItem::Subscription(record.clone())))
                .collect(),
        };
        rows.retain(|(k, _)| after.as_ref().is_none_or(|a| k > a));
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        let next_cursor = if has_more {
            rows.last()
                .map(|(k, _)| {
                    serde_json::to_string(&Cursor {
                        revision: snapshot.revision,
                        query_digest,
                        after: k.clone(),
                    })
                })
                .transpose()
                .map_err(|e| error(PersistenceErrorKind::Serialization, e.to_string()))?
        } else {
            None
        };
        Ok(WorkGraphPage {
            domain: domain.clone(),
            revision: snapshot.revision,
            items: rows.into_iter().map(|(_, r)| r).collect(),
            next_cursor,
            coverage: "current_workshop_domain_registry".into(),
        })
    }
}

impl Snapshot {
    fn require_resource(&self, reference: &ResourceRef, live: bool) -> Result<()> {
        let record = self
            .resources
            .get(&reference_key(reference)?)
            .ok_or_else(|| invalid("resource reference is not recorded in this domain"))?;
        if &record.reference != reference
            || (live && record.resolution == ResourceResolution::Tombstoned)
        {
            return Err(invalid("resource identity is unavailable or tombstoned"));
        }
        Ok(())
    }

    fn validate_scope(&self, id: &str, scope: &WorkScope, live: bool) -> Result<()> {
        if scope.resources.len()
            + scope.children.len()
            + scope.depends_on.len()
            + scope.readiness.len()
            > MAX_SCOPE_MEMBERS
        {
            return Err(invalid("work scope exceeds 128 explicit members"));
        }
        for (i, reference) in scope.resources.iter().enumerate() {
            if reference.kind == ResourceKind::WorkUnit || scope.resources[..i].contains(reference)
            {
                return Err(invalid(
                    "use children or depends_on for work units; scope members must be unique",
                ));
            }
            self.require_resource(reference, live)?;
        }
        for members in [&scope.children, &scope.depends_on] {
            for (i, member) in members.iter().enumerate() {
                identifier(member)?;
                if member == id
                    || members[..i].contains(member)
                    || !self.work_units.contains_key(member)
                {
                    return Err(invalid(
                        "work scope requires distinct, existing units and cannot include itself",
                    ));
                }
            }
        }
        for (i, requirement) in scope.readiness.iter().enumerate() {
            text(&requirement.condition, 4096)?;
            if !scope.children.contains(&requirement.work_unit_id)
                && !scope.depends_on.contains(&requirement.work_unit_id)
                || scope.readiness[..i]
                    .iter()
                    .any(|r| r.work_unit_id == requirement.work_unit_id)
                || self
                    .work_units
                    .get(&requirement.work_unit_id)
                    .is_none_or(|u| u.kind != WorkUnitKind::Maintenance)
            {
                return Err(invalid(
                    "readiness requires a unique explicit maintenance member",
                ));
            }
        }
        Ok(())
    }

    fn validate_session(&self, session: &medousa_types::SessionRef) -> Result<()> {
        if session.authority_id != self.domain.authority_id {
            return Err(invalid("conversation attachment requires local authority"));
        }
        Ok(())
    }

    fn readiness_current(&self, unit: &WorkUnit, now: chrono::DateTime<Utc>) -> Result<bool> {
        let Some(checkpoint) = &unit.readiness else {
            return Ok(false);
        };
        if unit.state != WorkUnitState::Active
            || checkpoint.scope_revision != unit.scope_revision
            || checkpoint.expires_at <= now
        {
            return Ok(false);
        }
        for (evidence, revision) in checkpoint
            .evidence
            .iter()
            .zip(&checkpoint.resource_revisions)
        {
            if self
                .resources
                .get(&reference_key(&evidence.reference)?)
                .is_none_or(|resource| {
                    resource.resolution != ResourceResolution::Available
                        || resource.revision != *revision
                        || resource.native_revision.as_ref() != Some(&evidence.native_revision)
                })
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn member_ready(
        &self,
        scope: &WorkScope,
        id: &str,
        now: chrono::DateTime<Utc>,
    ) -> Result<bool> {
        let unit = self
            .work_units
            .get(id)
            .ok_or_else(|| invalid("unknown component"))?;
        if unit.kind == WorkUnitKind::Finite {
            return Ok(unit.state == WorkUnitState::Satisfied);
        }
        let Some(requirement) = scope.readiness.iter().find(|r| r.work_unit_id == id) else {
            return Ok(false);
        };
        Ok(unit
            .readiness
            .as_ref()
            .is_some_and(|r| r.condition == requirement.condition)
            && self.readiness_current(unit, now)?)
    }

    fn validate_contact(&self, contact: &WorkContactPreference) -> Result<()> {
        match contact {
            WorkContactPreference::Participant { participant } => {
                self.require_resource(participant, false)
            }
            WorkContactPreference::Channel { channel } => self.require_resource(channel, false),
            _ => Ok(()),
        }
    }

    fn mutate(&mut self, mutation: WorkGraphMutation, provenance: RecordProvenance) -> Result<()> {
        let now = Utc::now();
        match mutation {
            WorkGraphMutation::RegisterCoordinatorWake { wake } => {
                self.register_coordinator_wake(*wake, &provenance)?
            }
            WorkGraphMutation::ClaimCoordinatorWake {
                conversation_id,
                request_id,
                attempt,
            } => {
                self.claim_coordinator_wake(&conversation_id, &request_id, attempt, &provenance)?
            }
            WorkGraphMutation::CompleteCoordinatorWake {
                conversation_id,
                request_id,
                decision,
            } => self.complete_coordinator_wake(
                &conversation_id,
                &request_id,
                decision,
                &provenance,
            )?,
            WorkGraphMutation::BlockCoordinatorWake {
                conversation_id,
                request_id,
                reason,
            } => {
                self.block_coordinator_wake(&conversation_id, &request_id, &reason, &provenance)?
            }
            WorkGraphMutation::RegisterProviderDispatch { dispatch } => {
                self.register_provider_dispatch(*dispatch, &provenance)?;
            }
            WorkGraphMutation::CloseProviderDispatch {
                conversation_id,
                request_id,
                reason,
            } => {
                self.close_provider_dispatch(&conversation_id, &request_id, &reason, &provenance)?;
            }
            WorkGraphMutation::RegisterProviderRequest { request } => {
                self.register_provider_request(*request, &provenance)?
            }
            WorkGraphMutation::ClaimProviderRequest {
                conversation_id,
                request_id,
            } => self.claim_provider_request(&conversation_id, &request_id, &provenance)?,
            WorkGraphMutation::RecordProviderEvent { event } => {
                self.record_provider_event(*event, &provenance)?
            }
            WorkGraphMutation::Subscribe { input } => self.subscribe(input, &provenance)?,
            WorkGraphMutation::AcknowledgeEvent {
                subscription_id,
                event_revision,
                decision,
            } => {
                self.acknowledge_event(&subscription_id, event_revision, &decision, &provenance)?;
            }
            WorkGraphMutation::AdvanceProviderStage {
                subscription_id,
                event_revision,
                state,
                reason,
            } => {
                self.advance_provider_stage(
                    &subscription_id,
                    event_revision,
                    state,
                    &reason,
                    &provenance,
                )?;
            }
            WorkGraphMutation::StopSubscription { subscription_id } => {
                let revision = self.revision;
                let subscription =
                    self.subscription_for_actor(&subscription_id, &provenance.actor_id)?;
                subscription.stopped_at_revision.get_or_insert(revision);
                subscription.revision = revision;
            }
            WorkGraphMutation::RecordResource {
                reference,
                locator,
                native_revision,
                resolution,
            } => {
                if reference.kind == ResourceKind::WorkUnit {
                    return Err(invalid("work unit resources are owned by work admission"));
                }
                if provenance.source != RecordSource::SystemEvent
                    && (resolution != ResourceResolution::Unresolved || native_revision.is_some())
                {
                    return Err(invalid(
                        "native resolution and revision require system provenance",
                    ));
                }
                if let Some(locator) = &locator {
                    text(locator, 4096)?;
                }
                if let Some(revision) = &native_revision {
                    text(revision, 1024)?;
                }
                let key = reference_key(&reference)?;
                if provenance.source != RecordSource::SystemEvent
                    && self
                        .resources
                        .get(&key)
                        .is_some_and(|r| r.provenance.source == RecordSource::SystemEvent)
                {
                    return Err(error(
                        PersistenceErrorKind::Conflict,
                        "model or user claims cannot overwrite adapter-owned resource facts",
                    ));
                }
                if self
                    .resources
                    .get(&key)
                    .is_some_and(|r| r.resolution == ResourceResolution::Tombstoned)
                {
                    return Err(error(
                        PersistenceErrorKind::Conflict,
                        "tombstoned identity cannot be reused",
                    ));
                }
                self.resources.insert(
                    key,
                    ResourceRecord {
                        reference,
                        locator,
                        native_revision,
                        resolution,
                        revision: self.revision,
                        provenance,
                        updated_at: now,
                    },
                );
            }
            WorkGraphMutation::PutRelationship {
                relationship_id,
                from,
                to,
                kind,
            } => {
                identifier(&relationship_id)?;
                self.require_resource(&from, true)?;
                self.require_resource(&to, true)?;
                self.relationships.insert(
                    relationship_id.clone(),
                    RelationshipRecord {
                        relationship_id,
                        from,
                        to,
                        kind,
                        revision: self.revision,
                        provenance,
                        updated_at: now,
                    },
                );
            }
            WorkGraphMutation::AcceptWork {
                work_unit_id,
                intent,
                kind,
                scope,
                completion_condition,
                contact,
                origin,
                budget,
            } => {
                identifier(&work_unit_id)?;
                text(&intent, 8192)?;
                text(&completion_condition, 4096)?;
                if self.work_units.contains_key(&work_unit_id) {
                    return Err(error(
                        PersistenceErrorKind::Conflict,
                        "work unit identity is already accepted",
                    ));
                }
                self.validate_scope(&work_unit_id, &scope, true)?;
                self.validate_contact(&contact)?;
                if let Some(origin) = &origin {
                    self.validate_session(origin)?;
                }
                let reference = work_reference(&self.domain, &work_unit_id);
                self.resources.insert(
                    reference_key(&reference)?,
                    ResourceRecord {
                        reference,
                        locator: None,
                        native_revision: None,
                        resolution: ResourceResolution::Available,
                        revision: self.revision,
                        provenance: provenance.clone(),
                        updated_at: now,
                    },
                );
                self.work_units.insert(
                    work_unit_id.clone(),
                    WorkUnit {
                        work_unit_id: work_unit_id.clone(),
                        intent,
                        kind,
                        scope,
                        scope_revision: self.revision,
                        readiness: None,
                        completion_condition,
                        state: WorkUnitState::Accepted,
                        state_reason: None,
                        state_evidence: vec![],
                        contact,
                        conversations: origin.iter().cloned().collect(),
                        budget,
                        origin,
                        revision: self.revision,
                        provenance,
                        created_at: now,
                        updated_at: now,
                    },
                );
                self.extend_budget_charges(&work_unit_id)?;
            }
            WorkGraphMutation::SetScope {
                work_unit_id,
                scope,
            } => {
                self.validate_scope(&work_unit_id, &scope, true)?;
                let revision = self.revision;
                let unit = self.active_unit(&work_unit_id)?;
                unit.scope = scope;
                unit.scope_revision = revision;
                unit.readiness = None;
                unit.revision = revision;
                unit.updated_at = now;
                unit.provenance = provenance;
                self.extend_budget_charges(&work_unit_id)?;
            }
            WorkGraphMutation::SetState {
                work_unit_id,
                state,
                reason,
                evidence,
            } => {
                text(&reason, 4096)?;
                if evidence.len() > MAX_SCOPE_MEMBERS {
                    return Err(invalid("too many state evidence references"));
                }
                for reference in &evidence {
                    self.require_resource(reference, true)?;
                }
                let current = self
                    .work_units
                    .get(&work_unit_id)
                    .ok_or_else(|| invalid("unknown work unit"))?;
                if state == WorkUnitState::Accepted {
                    return Err(invalid("accepted state is established by admission"));
                }
                if state == WorkUnitState::Satisfied {
                    if provenance.source != RecordSource::SystemEvent
                        && (self
                            .provider_requests
                            .values()
                            .any(|record| record.request.input.work_unit_id == work_unit_id)
                            || self
                                .provider_dispatches
                                .values()
                                .any(|record| record.dispatch.input.work_unit_id == work_unit_id))
                    {
                        return Err(invalid(
                            "provider work requires native outcome qualification; a model state claim cannot satisfy it",
                        ));
                    }
                    if self.budget_usage(&work_unit_id).concurrent_executions != 0 {
                        return Err(invalid("satisfaction requires settled execution custody"));
                    }
                    if current.kind == WorkUnitKind::Maintenance {
                        return Err(invalid(
                            "maintenance responsibility cannot terminate as satisfied",
                        ));
                    }
                    if evidence.is_empty() {
                        return Err(invalid("satisfaction requires evidence"));
                    }
                    for id in current
                        .scope
                        .children
                        .iter()
                        .chain(&current.scope.depends_on)
                    {
                        if !self.member_ready(&current.scope, id, now)? {
                            return Err(invalid(
                                "satisfaction requires satisfied finite members or current explicit maintenance checkpoints",
                            ));
                        }
                    }
                    for reference in &evidence {
                        if self.resources[&reference_key(reference)?].resolution
                            != ResourceResolution::Available
                        {
                            return Err(invalid(
                                "satisfaction evidence must be resolved by a native adapter",
                            ));
                        }
                        if reference.kind == ResourceKind::WorkUnit
                            && (reference.authority_id != self.domain.authority_id
                                || reference.id == work_unit_id
                                || self
                                    .work_units
                                    .get(&reference.id)
                                    .is_none_or(|u| u.state != WorkUnitState::Satisfied))
                        {
                            return Err(invalid(
                                "work unit evidence must identify a satisfied component",
                            ));
                        }
                    }
                }
                let revision = self.revision;
                let unit = self.active_unit(&work_unit_id)?;
                unit.state = state;
                unit.readiness = None;
                unit.state_reason = Some(reason);
                unit.state_evidence = evidence;
                unit.revision = revision;
                unit.updated_at = now;
                unit.provenance = provenance;
            }
            WorkGraphMutation::SetContact {
                work_unit_id,
                contact,
            } => {
                self.validate_contact(&contact)?;
                let unit = self
                    .work_units
                    .get_mut(&work_unit_id)
                    .ok_or_else(|| invalid("unknown work unit"))?;
                unit.contact = contact;
                unit.revision = self.revision;
                unit.updated_at = now;
                unit.provenance = provenance;
            }
            WorkGraphMutation::AttachConversation {
                work_unit_id,
                session,
            } => {
                self.validate_session(&session)?;
                let unit = self
                    .work_units
                    .get_mut(&work_unit_id)
                    .ok_or_else(|| invalid("unknown work unit"))?;
                if !unit.conversations.contains(&session) {
                    if unit.conversations.len() >= MAX_SCOPE_MEMBERS {
                        return Err(invalid("too many attached conversations"));
                    }
                    unit.conversations.push(session);
                }
                unit.revision = self.revision;
                unit.updated_at = now;
                unit.provenance = provenance;
            }
            WorkGraphMutation::RecordReadiness {
                work_unit_id,
                expected_scope_revision,
                condition,
                evidence,
                valid_for_seconds,
            } => {
                text(&condition, 4096)?;
                if !(1..=86400).contains(&valid_for_seconds)
                    || evidence.is_empty()
                    || evidence.len() > MAX_SCOPE_MEMBERS
                {
                    return Err(invalid(
                        "readiness requires evidence and a lifetime of 1–86400 seconds",
                    ));
                }
                let unit = self
                    .work_units
                    .get(&work_unit_id)
                    .ok_or_else(|| invalid("unknown work unit"))?;
                if unit.kind != WorkUnitKind::Maintenance
                    || unit.state != WorkUnitState::Active
                    || unit.scope_revision != expected_scope_revision
                {
                    return Err(error(
                        PersistenceErrorKind::Conflict,
                        "readiness requires an active maintenance unit at the exact scope revision",
                    ));
                }
                let mut resource_revisions = Vec::with_capacity(evidence.len());
                for (i, proof) in evidence.iter().enumerate() {
                    text(&proof.native_revision, 1024)?;
                    if !unit.scope.resources.contains(&proof.reference)
                        || proof.reference.authority_id != self.domain.authority_id
                        || evidence[..i].iter().any(|e| e.reference == proof.reference)
                    {
                        return Err(invalid(
                            "readiness evidence must be distinct local resources in the saved scope",
                        ));
                    }
                    let resource = self
                        .resources
                        .get(&reference_key(&proof.reference)?)
                        .ok_or_else(|| invalid("unrecorded readiness resource"))?;
                    if resource.resolution != ResourceResolution::Available
                        || resource.provenance.source != RecordSource::SystemEvent
                        || resource.native_revision.as_ref() != Some(&proof.native_revision)
                    {
                        return Err(invalid(
                            "readiness evidence requires the current adapter-owned native revision",
                        ));
                    }
                    resource_revisions.push(resource.revision);
                }
                let revision = self.revision;
                let unit = self.active_unit(&work_unit_id)?;
                unit.readiness = Some(WorkReadiness {
                    condition,
                    scope_revision: expected_scope_revision,
                    evidence,
                    resource_revisions,
                    observed_at: now,
                    expires_at: now + chrono::Duration::seconds(i64::from(valid_for_seconds)),
                });
                unit.revision = revision;
                unit.updated_at = now;
                unit.provenance = provenance;
            }
            WorkGraphMutation::SetBudget {
                work_unit_id,
                limits,
            } => {
                self.check_budget_limits(&limits, &self.budget_usage(&work_unit_id))?;
                let revision = self.revision;
                let unit = self.active_unit(&work_unit_id)?;
                unit.budget = Some(limits);
                unit.revision = revision;
                unit.updated_at = now;
                unit.provenance = provenance;
            }
            WorkGraphMutation::ReserveBudget {
                reservation_id,
                work_unit_id,
                execution,
                reserved_cost_microusd,
            } => {
                self.reserve_budget(
                    reservation_id,
                    work_unit_id,
                    execution,
                    reserved_cost_microusd,
                    provenance,
                    now,
                )?;
            }
            WorkGraphMutation::SettleBudget {
                reservation_id,
                disposition,
                actual_cost_microusd,
            } => {
                self.settle_budget(
                    &reservation_id,
                    disposition,
                    actual_cost_microusd,
                    provenance,
                )?;
            }
        }
        Ok(())
    }

    fn active_unit(&mut self, id: &str) -> Result<&mut WorkUnit> {
        let unit = self
            .work_units
            .get_mut(id)
            .ok_or_else(|| invalid("unknown work unit"))?;
        if unit.state.is_terminal() {
            return Err(error(
                PersistenceErrorKind::Conflict,
                "terminal work unit cannot be changed or implicitly reopened",
            ));
        }
        Ok(unit)
    }

    fn validate(&self) -> Result<()> {
        if self.commands.len() as u64 != self.revision {
            return Err(invalid(
                "work graph revision does not match committed history",
            ));
        }
        if [
            self.resources.len(),
            self.relationships.len(),
            self.work_units.len(),
            self.reservations.len(),
            self.subscriptions.len(),
            self.provider_requests.len(),
            self.provider_dispatches.len(),
            self.coordinator_wakes.len(),
            self.commands.len(),
        ]
        .iter()
        .any(|n| *n > MAX_RECORDS)
        {
            return Err(error(
                PersistenceErrorKind::Overloaded,
                "domain record capacity reached",
            ));
        }
        for (key, resource) in &self.resources {
            if key != &reference_key(&resource.reference)?
                || resource.revision == 0
                || resource.revision > self.revision
            {
                return Err(invalid("resource index or revision mismatch"));
            }
        }
        for (key, relationship) in &self.relationships {
            if key != &relationship.relationship_id
                || relationship.revision == 0
                || relationship.revision > self.revision
            {
                return Err(invalid("relationship index or revision mismatch"));
            }
            self.require_resource(&relationship.from, false)?;
            self.require_resource(&relationship.to, false)?;
        }
        for (id, unit) in &self.work_units {
            if id != &unit.work_unit_id
                || unit.revision == 0
                || unit.revision > self.revision
                || unit.scope_revision == 0
                || unit.scope_revision > unit.revision
            {
                return Err(invalid("work unit index or revision mismatch"));
            }
            self.require_resource(&work_reference(&self.domain, id), true)?;
            self.validate_scope(id, &unit.scope, false)?;
            self.validate_contact(&unit.contact)?;
            if unit.conversations.len() > MAX_SCOPE_MEMBERS {
                return Err(invalid("too many attached conversations"));
            }
            for (i, session) in unit.conversations.iter().enumerate() {
                self.validate_session(session)?;
                if unit.conversations[..i].contains(session) {
                    return Err(invalid("duplicate conversation attachment"));
                }
            }
            if let Some(origin) = &unit.origin
                && !unit.conversations.contains(origin)
            {
                return Err(invalid(
                    "work origin is missing its conversation attachment",
                ));
            }
            if let Some(checkpoint) = &unit.readiness {
                text(&checkpoint.condition, 4096)?;
                if unit.kind != WorkUnitKind::Maintenance
                    || unit.state != WorkUnitState::Active
                    || checkpoint.scope_revision != unit.scope_revision
                    || checkpoint.evidence.is_empty()
                    || checkpoint.evidence.len() > MAX_SCOPE_MEMBERS
                    || checkpoint.evidence.len() != checkpoint.resource_revisions.len()
                    || checkpoint.expires_at <= checkpoint.observed_at
                    || checkpoint.expires_at - checkpoint.observed_at > chrono::Duration::days(1)
                {
                    return Err(invalid("invalid saved readiness checkpoint"));
                }
                for (i, (proof, revision)) in checkpoint
                    .evidence
                    .iter()
                    .zip(&checkpoint.resource_revisions)
                    .enumerate()
                {
                    text(&proof.native_revision, 1024)?;
                    self.require_resource(&proof.reference, false)?;
                    if !unit.scope.resources.contains(&proof.reference)
                        || proof.reference.authority_id != self.domain.authority_id
                        || checkpoint.evidence[..i]
                            .iter()
                            .any(|e| e.reference == proof.reference)
                        || *revision == 0
                        || *revision > unit.revision
                    {
                        return Err(invalid("invalid saved readiness evidence"));
                    }
                }
            }
        }
        // Kahn's algorithm bounds work composition/dependency evaluation. The
        // ordinary semantic graph may contain cycles without this restriction.
        let mut incoming: BTreeMap<&str, usize> =
            self.work_units.keys().map(|id| (id.as_str(), 0)).collect();
        for unit in self.work_units.values() {
            for id in unit.scope.children.iter().chain(&unit.scope.depends_on) {
                *incoming
                    .get_mut(id.as_str())
                    .ok_or_else(|| invalid("missing dependency"))? += 1;
            }
        }
        let mut ready: Vec<&str> = incoming
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut visited = 0;
        while let Some(id) = ready.pop() {
            visited += 1;
            let unit = &self.work_units[id];
            for child in unit.scope.children.iter().chain(&unit.scope.depends_on) {
                let count = incoming
                    .get_mut(child.as_str())
                    .ok_or_else(|| invalid("missing dependency"))?;
                *count -= 1;
                if *count == 0 {
                    ready.push(child);
                }
            }
        }
        if visited != self.work_units.len() {
            return Err(invalid("work composition or dependency cycle"));
        }
        for (id, command) in &self.commands {
            if id != &command.receipt.command_id
                || id != &command.command.command_id
                || command.receipt.revision == 0
                || command.receipt.revision > self.revision
                || command.command.expected_revision.checked_add(1)
                    != Some(command.receipt.revision)
                || command.receipt.replayed
                || command.digest != digest(&(&command.command, &command.provenance))?
            {
                return Err(invalid("invalid durable command receipt"));
            }
        }
        self.validate_reservations()?;
        self.validate_subscriptions()?;
        self.validate_provider_records()?;
        self.validate_provider_dispatches()?;
        self.validate_coordinator_wakes()?;
        Ok(())
    }
}
