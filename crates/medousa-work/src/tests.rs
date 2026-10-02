use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{MAX_COMMAND_BYTES, WorkGraphStore};
use medousa_store::{
    PersistenceError, PersistenceErrorKind, TransactionFaultPoint, TransactionFaults,
};
use medousa_types::{AuthorityId, work_unit::*};

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

fn domain(user: &str) -> UserDomainRef {
    UserDomainRef {
        authority_id: AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap(),
        user_id: user.into(),
    }
}

fn resource(kind: ResourceKind, id: &str) -> ResourceRef {
    ResourceRef {
        authority_id: domain("user:a").authority_id,
        kind,
        id: id.into(),
    }
}

fn provenance() -> RecordProvenance {
    RecordProvenance {
        actor_id: "adapter:test".into(),
        source: RecordSource::SystemEvent,
        evidence: vec![],
    }
}

fn command(id: &str, revision: u64, mutation: WorkGraphMutation) -> WorkGraphCommand {
    WorkGraphCommand {
        command_id: id.into(),
        expected_revision: revision,
        mutation,
    }
}

fn record(
    reference: ResourceRef,
    locator: &str,
    resolution: ResourceResolution,
) -> WorkGraphMutation {
    WorkGraphMutation::RecordResource {
        reference,
        locator: Some(locator.into()),
        native_revision: None,
        resolution,
    }
}

fn accept(id: &str, scope: WorkScope) -> WorkGraphMutation {
    WorkGraphMutation::AcceptWork {
        work_unit_id: id.into(),
        intent: "Keep the launch ready".into(),
        kind: WorkUnitKind::Finite,
        scope,
        completion_condition: "Reviewed change and current documentation".into(),
        contact: WorkContactPreference::ReturnToOrigin,
        origin: None,
    }
}

fn apply(store: &WorkGraphStore, revision: u64, mutation: WorkGraphMutation) -> WorkGraphReceipt {
    store
        .apply(
            &domain("user:a"),
            command(&format!("command-{revision}"), revision, mutation),
            provenance(),
        )
        .unwrap()
}

fn query(collection: WorkGraphCollection) -> WorkGraphQuery {
    WorkGraphQuery {
        collection,
        ..Default::default()
    }
}

#[test]
fn work_survives_sessions_and_restart_and_does_not_merge_by_title() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let request = command("accept-first", 0, accept("launch-1", WorkScope::default()));
    let original = store
        .apply(&domain("user:a"), request.clone(), provenance())
        .unwrap();
    drop(store);
    let restarted = WorkGraphStore::open(dir.path()).unwrap();
    let unit = restarted.work_unit(&domain("user:a"), "launch-1").unwrap();
    assert!(unit.origin.is_none());
    assert_eq!(unit.state, WorkUnitState::Accepted);
    apply(&restarted, 1, accept("launch-2", WorkScope::default()));
    let replay = restarted
        .apply(&domain("user:a"), request.clone(), provenance())
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.revision, original.revision);
    assert_eq!(replay.committed_at, original.committed_at);
    let mut changed = request;
    changed.expected_revision = 2;
    assert_eq!(
        restarted
            .apply(&domain("user:a"), changed, provenance())
            .unwrap_err()
            .kind,
        PersistenceErrorKind::Conflict
    );
    assert_eq!(
        restarted
            .query(&domain("user:a"), query(WorkGraphCollection::WorkUnits))
            .unwrap()
            .items
            .len(),
        2
    );
    let history = restarted
        .query(&domain("user:a"), query(WorkGraphCollection::Events))
        .unwrap();
    assert_eq!(history.items.len(), 2);
    assert!(
        matches!(&history.items[0], WorkGraphItem::Event(event) if event.command.command_id == "accept-first" && event.provenance == provenance())
    );
}

#[test]
fn domains_and_authorities_are_isolated_without_path_aliases() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    for user in ["user:a", "user:A", "../other/user"] {
        store
            .apply(
                &domain(user),
                command("same-key", 0, accept("same-id", WorkScope::default())),
                provenance(),
            )
            .unwrap();
    }
    assert_eq!(
        store
            .query(
                &domain("user:missing"),
                query(WorkGraphCollection::WorkUnits)
            )
            .unwrap()
            .revision,
        0
    );
    let mut foreign = domain("user:a");
    foreign.authority_id = AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
    assert!(store.work_unit(&foreign, "same-id").is_err());
    assert_eq!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|e| e == "json"))
            .count(),
        3
    );
}

#[test]
fn shared_links_have_inverse_queries_and_never_expand_scope() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let note = resource(ResourceKind::VaultNote, "note-id");
    let project = resource(ResourceKind::Project, "project-id");
    let folder = resource(ResourceKind::VaultFolder, "folder-id");
    for (i, reference) in [note.clone(), project.clone(), folder.clone()]
        .into_iter()
        .enumerate()
    {
        apply(
            &store,
            i as u64,
            record(reference, "locator", ResourceResolution::Available),
        );
    }
    for (i, (from, to)) in [
        (note.clone(), project.clone()),
        (note.clone(), folder.clone()),
        (project.clone(), note.clone()),
    ]
    .into_iter()
    .enumerate()
    {
        apply(
            &store,
            3 + i as u64,
            WorkGraphMutation::PutRelationship {
                relationship_id: format!("edge-{i}"),
                from,
                to,
                kind: ResourceRelationshipKind::Informs,
            },
        );
    }
    let page = store
        .query(
            &domain("user:a"),
            WorkGraphQuery {
                collection: WorkGraphCollection::Relationships,
                anchor: Some(note.clone()),
                direction: RelationshipDirection::Outgoing,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(page.items.len(), 2);
    let inverse = store
        .query(
            &domain("user:a"),
            WorkGraphQuery {
                collection: WorkGraphCollection::Relationships,
                anchor: Some(note.clone()),
                direction: RelationshipDirection::Incoming,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(inverse.items.len(), 1);
    apply(
        &store,
        6,
        accept(
            "scoped",
            WorkScope {
                resources: vec![project.clone()],
                ..Default::default()
            },
        ),
    );
    assert_eq!(
        store
            .work_unit(&domain("user:a"), "scoped")
            .unwrap()
            .scope
            .resources,
        vec![project]
    );
    let note_work = store
        .query(
            &domain("user:a"),
            WorkGraphQuery {
                collection: WorkGraphCollection::WorkUnits,
                anchor: Some(note),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(note_work.items.is_empty());
}

#[test]
fn rename_preserves_identity_and_deleted_path_reuse_creates_distinct_resource() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let note = resource(ResourceKind::VaultNote, "old-identity");
    let project = resource(ResourceKind::Project, "project");
    apply(
        &store,
        0,
        record(note.clone(), "old.md", ResourceResolution::Available),
    );
    apply(
        &store,
        1,
        record(project.clone(), "repo", ResourceResolution::Available),
    );
    apply(
        &store,
        2,
        WorkGraphMutation::PutRelationship {
            relationship_id: "link".into(),
            from: note.clone(),
            to: project,
            kind: ResourceRelationshipKind::Supports,
        },
    );
    apply(
        &store,
        3,
        record(note.clone(), "renamed.md", ResourceResolution::Available),
    );
    apply(
        &store,
        4,
        record(note.clone(), "renamed.md", ResourceResolution::Tombstoned),
    );
    let err = store
        .apply(
            &domain("user:a"),
            command(
                "resurrect",
                5,
                record(note.clone(), "renamed.md", ResourceResolution::Available),
            ),
            provenance(),
        )
        .unwrap_err();
    assert_eq!(err.kind, PersistenceErrorKind::Conflict);
    let replacement = resource(ResourceKind::VaultNote, "new-identity");
    apply(
        &store,
        5,
        record(replacement, "renamed.md", ResourceResolution::Available),
    );
    let links = store
        .query(&domain("user:a"), query(WorkGraphCollection::Relationships))
        .unwrap();
    assert!(matches!(&links.items[0], WorkGraphItem::Relationship(link) if link.from == note));
    let old = store
        .query(
            &domain("user:a"),
            WorkGraphQuery {
                anchor: Some(note),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        matches!(&old.items[0], WorkGraphItem::Resource(record) if record.resolution == ResourceResolution::Tombstoned && record.locator.as_deref() == Some("renamed.md"))
    );
}

#[test]
fn work_dag_rejects_cycles_and_parent_cancellation_keeps_shared_child_active() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    apply(&store, 0, accept("shared", WorkScope::default()));
    for (revision, id) in [(1, "parent-a"), (2, "parent-b")] {
        apply(
            &store,
            revision,
            accept(
                id,
                WorkScope {
                    children: vec!["shared".into()],
                    ..Default::default()
                },
            ),
        );
    }
    let err = store
        .apply(
            &domain("user:a"),
            command(
                "cycle",
                3,
                WorkGraphMutation::SetScope {
                    work_unit_id: "shared".into(),
                    scope: WorkScope {
                        depends_on: vec!["parent-a".into()],
                        ..Default::default()
                    },
                },
            ),
            provenance(),
        )
        .unwrap_err();
    assert!(err.to_string().contains("cycle"));
    assert_eq!(
        store
            .query(&domain("user:a"), query(WorkGraphCollection::WorkUnits))
            .unwrap()
            .revision,
        3
    );
    apply(
        &store,
        3,
        WorkGraphMutation::SetState {
            work_unit_id: "parent-a".into(),
            state: WorkUnitState::Cancelled,
            reason: "User withdrew this responsibility".into(),
            evidence: vec![],
        },
    );
    assert_eq!(
        store.work_unit(&domain("user:a"), "shared").unwrap().state,
        WorkUnitState::Accepted
    );
    assert_eq!(
        store
            .work_unit(&domain("user:a"), "parent-b")
            .unwrap()
            .state,
        WorkUnitState::Accepted
    );
    apply(
        &store,
        4,
        WorkGraphMutation::SetContact {
            work_unit_id: "parent-b".into(),
            contact: WorkContactPreference::Silent,
        },
    );
    let parent = store.work_unit(&domain("user:a"), "parent-b").unwrap();
    assert_eq!(parent.state, WorkUnitState::Accepted);
    assert_eq!(parent.contact, WorkContactPreference::Silent);
}

#[test]
fn satisfaction_requires_resolved_evidence_and_children_and_maintenance_stays_open() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let evidence = resource(ResourceKind::Artifact, "evidence");
    apply(
        &store,
        0,
        record(evidence.clone(), "artifact", ResourceResolution::Unresolved),
    );
    apply(&store, 1, accept("child", WorkScope::default()));
    apply(
        &store,
        2,
        accept(
            "parent",
            WorkScope {
                children: vec!["child".into()],
                ..Default::default()
            },
        ),
    );
    let satisfy = |id: &str| WorkGraphMutation::SetState {
        work_unit_id: id.into(),
        state: WorkUnitState::Satisfied,
        reason: "Verified result".into(),
        evidence: vec![evidence.clone()],
    };
    assert!(
        store
            .apply(
                &domain("user:a"),
                command("unresolved", 3, satisfy("child")),
                provenance()
            )
            .is_err()
    );
    apply(
        &store,
        3,
        record(evidence.clone(), "artifact", ResourceResolution::Available),
    );
    assert!(
        store
            .apply(
                &domain("user:a"),
                command("unfinished-child", 4, satisfy("parent")),
                provenance()
            )
            .is_err()
    );
    apply(&store, 4, satisfy("child"));
    apply(&store, 5, satisfy("parent"));
    assert!(
        store
            .apply(
                &domain("user:a"),
                command(
                    "reopen",
                    6,
                    WorkGraphMutation::SetState {
                        work_unit_id: "child".into(),
                        state: WorkUnitState::Active,
                        reason: "Implicit reopen".into(),
                        evidence: vec![]
                    }
                ),
                provenance()
            )
            .is_err()
    );
    let mut maintenance = accept("maintenance", WorkScope::default());
    if let WorkGraphMutation::AcceptWork { kind, .. } = &mut maintenance {
        *kind = WorkUnitKind::Maintenance;
    }
    apply(&store, 6, maintenance);
    assert!(
        store
            .apply(
                &domain("user:a"),
                command("maintenance-done", 7, satisfy("maintenance")),
                provenance()
            )
            .is_err()
    );
}

#[test]
fn cursor_is_bound_to_revision_owner_and_query_and_limits_are_enforced() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    apply(&store, 0, accept("a", WorkScope::default()));
    apply(&store, 1, accept("b", WorkScope::default()));
    let mut request = WorkGraphQuery {
        collection: WorkGraphCollection::WorkUnits,
        limit: Some(1),
        ..Default::default()
    };
    let first = store.query(&domain("user:a"), request.clone()).unwrap();
    request.cursor = first.next_cursor;
    assert!(request.cursor.is_some());
    let next = store.query(&domain("user:a"), request.clone()).unwrap();
    assert!(matches!(&next.items[0], WorkGraphItem::WorkUnit(unit) if unit.work_unit_id == "b"));
    store
        .apply(
            &domain("user:b"),
            command("other-a", 0, accept("a", WorkScope::default())),
            provenance(),
        )
        .unwrap();
    store
        .apply(
            &domain("user:b"),
            command("other-b", 1, accept("b", WorkScope::default())),
            provenance(),
        )
        .unwrap();
    assert!(store.query(&domain("user:b"), request.clone()).is_err());
    let mut wrong = request.clone();
    wrong.collection = WorkGraphCollection::Resources;
    assert!(store.query(&domain("user:a"), wrong).is_err());
    apply(&store, 2, accept("c", WorkScope::default()));
    assert_eq!(
        store.query(&domain("user:a"), request).unwrap_err().kind,
        PersistenceErrorKind::Conflict
    );
    for limit in [0, 101] {
        assert!(
            store
                .query(
                    &domain("user:a"),
                    WorkGraphQuery {
                        limit: Some(limit),
                        ..Default::default()
                    }
                )
                .is_err()
        );
    }
}

#[test]
fn independent_store_handles_cannot_lose_writes_or_wait_on_busy_locks() {
    use fs2::FileExt;
    let dir = tempdir();
    let first = WorkGraphStore::open(dir.path()).unwrap();
    let second = WorkGraphStore::open(dir.path()).unwrap();
    apply(&first, 0, accept("first", WorkScope::default()));
    assert_eq!(
        second
            .apply(
                &domain("user:a"),
                command("stale", 0, accept("second", WorkScope::default())),
                provenance()
            )
            .unwrap_err()
            .kind,
        PersistenceErrorKind::Conflict
    );
    let lock_path = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.path().extension().is_some_and(|e| e == "lock"))
        .unwrap()
        .path();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    lock.lock_exclusive().unwrap();
    assert_eq!(
        second
            .apply(
                &domain("user:a"),
                command("busy", 1, accept("second", WorkScope::default())),
                provenance()
            )
            .unwrap_err()
            .kind,
        PersistenceErrorKind::Overloaded
    );
    drop(lock);
    apply(&second, 1, accept("second", WorkScope::default()));
    assert_eq!(
        first
            .query(&domain("user:a"), query(WorkGraphCollection::WorkUnits))
            .unwrap()
            .items
            .len(),
        2
    );
}

struct FailOnce {
    point: TransactionFaultPoint,
    fired: AtomicBool,
}

impl TransactionFaults for FailOnce {
    fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
        if point == self.point && !self.fired.swap(true, Ordering::SeqCst) {
            return Err(PersistenceError::new(
                PersistenceErrorKind::RetryableIo,
                "injected publication failure",
            ));
        }
        Ok(())
    }
}

#[test]
fn publication_failure_never_splits_intent_effects_from_receipt() {
    for (point, was_published) in [
        (TransactionFaultPoint::BeforeRenamePublish, false),
        (TransactionFaultPoint::AfterRenamePublish, true),
    ] {
        let dir = tempdir();
        let store = WorkGraphStore::with_faults(
            dir.path(),
            Arc::new(FailOnce {
                point,
                fired: AtomicBool::new(false),
            }),
        )
        .unwrap();
        let request = command("faulted", 0, accept("work", WorkScope::default()));
        assert!(
            store
                .apply(&domain("user:a"), request.clone(), provenance())
                .is_err()
        );
        let restarted = WorkGraphStore::open(dir.path()).unwrap();
        let page = restarted
            .query(&domain("user:a"), query(WorkGraphCollection::WorkUnits))
            .unwrap();
        assert_eq!(page.items.len(), usize::from(was_published));
        assert_eq!(
            restarted
                .query(&domain("user:a"), query(WorkGraphCollection::Events))
                .unwrap()
                .items
                .len(),
            usize::from(was_published)
        );
        let retry = restarted
            .apply(&domain("user:a"), request, provenance())
            .unwrap();
        assert_eq!(retry.replayed, was_published);
        assert_eq!(
            restarted
                .query(&domain("user:a"), query(WorkGraphCollection::WorkUnits))
                .unwrap()
                .revision,
            1
        );
    }
}

#[test]
fn replay_does_not_readmit_effects_after_the_origin_becomes_unavailable() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let request = command("accepted", 0, accept("work", WorkScope::default()));
    store
        .apply_checked(&domain("user:a"), request.clone(), provenance(), |_| Ok(()))
        .unwrap();
    let replay = store
        .apply_checked(&domain("user:a"), request, provenance(), |_| {
            panic!("an accepted replay must not revalidate or readmit effects")
        })
        .unwrap();
    assert!(replay.replayed);
    let rejected = store.apply_checked(
        &domain("user:a"),
        command("new-effect", 1, accept("other", WorkScope::default())),
        provenance(),
        |_| {
            Err(PersistenceError::new(
                PersistenceErrorKind::PermanentIo,
                "origin is no longer visible",
            ))
        },
    );
    assert!(rejected.is_err());
    assert_eq!(
        store
            .query(&domain("user:a"), query(WorkGraphCollection::Events))
            .unwrap()
            .items
            .len(),
        1
    );
}

#[test]
fn corrupt_or_future_snapshot_and_oversized_intent_fail_without_reset() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    apply(&store, 0, accept("work", WorkScope::default()));
    let path = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.path().extension().is_some_and(|e| e == "json"))
        .unwrap()
        .path();
    let original = std::fs::read(&path).unwrap();
    let mut snapshot: serde_json::Value = serde_json::from_slice(&original).unwrap();
    snapshot["schema_version"] = 999.into();
    std::fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(
        store
            .query(&domain("user:a"), query(WorkGraphCollection::WorkUnits))
            .unwrap_err()
            .kind,
        PersistenceErrorKind::Corruption
    );
    assert!(
        store
            .apply(
                &domain("user:a"),
                command("cannot-reset", 0, accept("new", WorkScope::default())),
                provenance()
            )
            .is_err()
    );
    std::fs::write(&path, original).unwrap();
    let mut oversized = accept("oversized", WorkScope::default());
    if let WorkGraphMutation::AcceptWork { intent, .. } = &mut oversized {
        *intent = "x".repeat(MAX_COMMAND_BYTES + 1);
    }
    assert_eq!(
        store
            .apply(
                &domain("user:a"),
                command("too-large", 1, oversized),
                provenance()
            )
            .unwrap_err()
            .kind,
        PersistenceErrorKind::Overloaded
    );
    assert_eq!(
        store
            .query(&domain("user:a"), query(WorkGraphCollection::WorkUnits))
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn native_facts_cannot_be_downgraded_by_model_claims() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let reference = resource(ResourceKind::Project, "project-1");
    apply(
        &store,
        0,
        record(
            reference.clone(),
            "authoritative-locator",
            ResourceResolution::Available,
        ),
    );
    let model = RecordProvenance {
        actor_id: "user:a".into(),
        source: RecordSource::ModelInferred,
        evidence: vec![],
    };
    assert_eq!(
        store
            .apply(
                &domain("user:a"),
                command(
                    "downgrade",
                    1,
                    record(
                        reference.clone(),
                        "invented-locator",
                        ResourceResolution::Unresolved
                    )
                ),
                model.clone()
            )
            .unwrap_err()
            .kind,
        PersistenceErrorKind::Conflict
    );
    let fabricated = resource(ResourceKind::Artifact, "fabricated");
    assert!(
        store
            .apply(
                &domain("user:a"),
                command(
                    "fabricate",
                    1,
                    record(fabricated, "locator", ResourceResolution::Available)
                ),
                model
            )
            .is_err()
    );
    let page = store
        .query(
            &domain("user:a"),
            WorkGraphQuery {
                anchor: Some(reference),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(page.revision, 1);
    assert!(
        matches!(&page.items[0], WorkGraphItem::Resource(record) if record.resolution == ResourceResolution::Available && record.locator.as_deref() == Some("authoritative-locator"))
    );
}

#[test]
fn full_registry_keeps_history_and_replay_receipts_without_evicting_work() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let first = command("first", 0, accept("first", WorkScope::default()));
    store
        .apply(&domain("user:a"), first.clone(), provenance())
        .unwrap();
    let mut revision = 1;
    loop {
        let mut mutation = accept(&format!("large-{revision}"), WorkScope::default());
        if let WorkGraphMutation::AcceptWork {
            intent,
            completion_condition,
            ..
        } = &mut mutation
        {
            *intent = "i".repeat(8192);
            *completion_condition = "c".repeat(4096);
        }
        let result = store.apply(
            &domain("user:a"),
            command(&format!("large-{revision}"), revision, mutation),
            provenance(),
        );
        match result {
            Ok(_) => revision += 1,
            Err(error) => {
                assert_eq!(error.kind, PersistenceErrorKind::Overloaded);
                break;
            }
        }
        assert!(
            revision < 100,
            "snapshot byte limit must bound accepted work"
        );
    }
    assert!(
        store
            .apply(&domain("user:a"), first, provenance())
            .unwrap()
            .replayed
    );
    assert_eq!(
        store.work_unit(&domain("user:a"), "first").unwrap().state,
        WorkUnitState::Accepted
    );
    let events = store
        .query(
            &domain("user:a"),
            WorkGraphQuery {
                collection: WorkGraphCollection::Events,
                limit: Some(100),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(events.revision, revision);
    assert_eq!(events.items.len() as u64, revision);
}

#[cfg(unix)]
#[test]
fn registry_rejects_symlink_substitution() {
    let dir = tempdir();
    let outside = tempdir();
    let root_link = outside.path().join("registry");
    std::os::unix::fs::symlink(dir.path(), &root_link).unwrap();
    assert!(WorkGraphStore::open(&root_link).is_err());
    let store = WorkGraphStore::open(dir.path()).unwrap();
    apply(&store, 0, accept("work", WorkScope::default()));
    let path = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.path().extension().is_some_and(|e| e == "json"))
        .unwrap()
        .path();
    let stolen = outside.path().join("state.json");
    std::fs::rename(&path, &stolen).unwrap();
    std::os::unix::fs::symlink(&stolen, &path).unwrap();
    assert!(
        store
            .query(&domain("user:a"), query(WorkGraphCollection::WorkUnits))
            .is_err()
    );
    assert!(
        store
            .apply(
                &domain("user:a"),
                command("unsafe", 1, accept("other", WorkScope::default())),
                provenance()
            )
            .is_err()
    );
}
