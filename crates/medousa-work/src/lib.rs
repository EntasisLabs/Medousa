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
                    commands: BTreeMap::new(),
                });
            }
            Err(e) => return Err(e.into()),
        };
        let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|e| {
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
        lock.try_lock_exclusive().map_err(|e| {
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
        identifier(id)?;
        self.load(domain)?
            .work_units
            .remove(id)
            .ok_or_else(|| invalid("work unit is not recorded in this domain"))
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
        if scope.resources.len() + scope.children.len() + scope.depends_on.len() > MAX_SCOPE_MEMBERS
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
        Ok(())
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
                        work_unit_id,
                        intent,
                        kind,
                        scope,
                        completion_condition,
                        state: WorkUnitState::Accepted,
                        state_reason: None,
                        state_evidence: vec![],
                        contact,
                        origin,
                        revision: self.revision,
                        provenance,
                        created_at: now,
                        updated_at: now,
                    },
                );
            }
            WorkGraphMutation::SetScope {
                work_unit_id,
                scope,
            } => {
                self.validate_scope(&work_unit_id, &scope, true)?;
                let revision = self.revision;
                let unit = self.active_unit(&work_unit_id)?;
                unit.scope = scope;
                unit.revision = revision;
                unit.updated_at = now;
                unit.provenance = provenance;
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
                    if current.kind == WorkUnitKind::Maintenance {
                        return Err(invalid(
                            "maintenance responsibility cannot terminate as satisfied",
                        ));
                    }
                    if evidence.is_empty()
                        || current
                            .scope
                            .children
                            .iter()
                            .chain(&current.scope.depends_on)
                            .any(|id| {
                                self.work_units
                                    .get(id)
                                    .is_none_or(|u| u.state != WorkUnitState::Satisfied)
                            })
                    {
                        return Err(invalid(
                            "satisfaction requires evidence and satisfied component/dependency units",
                        ));
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
            if id != &unit.work_unit_id || unit.revision == 0 || unit.revision > self.revision {
                return Err(invalid("work unit index or revision mismatch"));
            }
            self.require_resource(&work_reference(&self.domain, id), true)?;
            self.validate_scope(id, &unit.scope, false)?;
            self.validate_contact(&unit.contact)?;
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
        Ok(())
    }
}
