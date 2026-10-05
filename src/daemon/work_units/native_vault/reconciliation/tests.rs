use super::super::tests::{fixture, mutate, native, write};
use super::*;
use crate::vault::{
    contracts::MutationPrecondition,
    mutation::{WriteMutation, commit_write},
    relocate::relocate_move,
};
use medousa_store::{
    FileTransaction, PersistenceError, PersistenceErrorKind, TransactionFaultPoint,
    TransactionFaults,
};

fn observed(
    store: &WorkGraphStore,
    domain: &UserDomainRef,
    owner: &Arc<VaultIndexOwner>,
    path: &str,
) -> ResourceRecord {
    native(
        &super::super::resolve(
            store,
            domain,
            owner,
            VaultResolveTarget::Note { path: path.into() },
        )
        .unwrap(),
    )
}
fn readiness(store: &WorkGraphStore, domain: &UserDomainRef, resource: &ResourceRecord) {
    mutate(
        store,
        domain,
        WorkGraphMutation::AcceptWork {
            work_unit_id: "maintenance".into(),
            intent: "Keep note current".into(),
            kind: WorkUnitKind::Maintenance,
            scope: WorkScope {
                resources: vec![resource.reference.clone()],
                ..Default::default()
            },
            completion_condition: "Current".into(),
            contact: WorkContactPreference::Silent,
            origin: None,
            budget: None,
        },
    );
    let scope_revision = store
        .work_unit(domain, "maintenance")
        .unwrap()
        .scope_revision;
    mutate(
        store,
        domain,
        WorkGraphMutation::SetState {
            work_unit_id: "maintenance".into(),
            state: WorkUnitState::Active,
            reason: "Maintaining accepted scope".into(),
            evidence: vec![],
        },
    );
    mutate(
        store,
        domain,
        WorkGraphMutation::RecordReadiness {
            work_unit_id: "maintenance".into(),
            expected_scope_revision: scope_revision,
            condition: "Current".into(),
            evidence: vec![WorkRevisionEvidence {
                reference: resource.reference.clone(),
                native_revision: resource.native_revision.clone().unwrap(),
            }],
            valid_for_seconds: 3600,
        },
    );
    assert!(store.inspect_work_unit(domain, "maintenance").unwrap().1);
}

#[test]
fn real_external_move_preserves_links_retires_replaced_locator_and_replays_current_facts() {
    let (dir, owner, graph, domain) = fixture();
    write(&owner, "source.md", "source");
    write(&owner, "destination.md", "other object");
    let source = observed(&graph, &domain, &owner, "source.md");
    let replaced = observed(&graph, &domain, &owner, "destination.md");
    readiness(&graph, &domain, &replaced);
    for id in ["project-a", "project-b"] {
        let project = ResourceRef {
            authority_id: domain.authority_id.clone(),
            kind: ResourceKind::Project,
            id: id.into(),
        };
        mutate(
            &graph,
            &domain,
            WorkGraphMutation::RecordResource {
                reference: project.clone(),
                locator: None,
                native_revision: None,
                resolution: ResourceResolution::Unresolved,
            },
        );
        mutate(
            &graph,
            &domain,
            WorkGraphMutation::PutRelationship {
                relationship_id: id.into(),
                from: source.reference.clone(),
                to: project,
                kind: ResourceRelationshipKind::Informs,
            },
        );
    }
    std::fs::rename(
        dir.path().join("vault/source.md"),
        dir.path().join("vault/destination.md"),
    )
    .unwrap();
    let input = WorkNativeReconcileInput {
        root_id: "personal".into(),
        command: VaultReconcileCommand::AdoptMove {
            command_id: "adopt-source".into(),
            reference: source.reference.clone(),
            expected_native_revision: source.native_revision.clone().unwrap(),
            path: "destination.md".into(),
        },
    };
    let adopted = reconcile(&graph, &domain, &owner, input.clone()).unwrap();
    assert_eq!(
        adopted["reconciliation"]["disposition"],
        "external_move_adopted"
    );
    assert_eq!(adopted["resources"].as_array().unwrap().len(), 2);
    let first: ResourceRecord = serde_json::from_value(adopted["resources"][0].clone()).unwrap();
    assert_eq!(first.reference, source.reference);
    assert!(first.locator.unwrap().contains("destination.md"));
    let second: ResourceRecord = serde_json::from_value(adopted["resources"][1].clone()).unwrap();
    assert_eq!(second.reference, replaced.reference);
    assert_eq!(second.resolution, ResourceResolution::Unavailable);
    assert!(!graph.inspect_work_unit(&domain, "maintenance").unwrap().1);
    relocate_move(&owner, "destination.md", "final.md").unwrap();
    let restarted = crate::vault::owner::VaultIndexOwner::new(
        crate::vault::contracts::VaultRootId::new("restarted"),
        owner.files.clone(),
    );
    let graph = WorkGraphStore::open(&dir.path().canonicalize().unwrap().join("graph")).unwrap();
    let replay = reconcile(&graph, &domain, &restarted, input.clone()).unwrap();
    assert_eq!(replay["reconciliation"]["replayed"], true);
    assert!(
        replay["resources"][0]["locator"]
            .as_str()
            .unwrap()
            .contains("final.md")
    );
    let links = graph
        .query(
            &domain,
            WorkGraphQuery {
                collection: WorkGraphCollection::Relationships,
                anchor: Some(source.reference),
                direction: RelationshipDirection::Outgoing,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(links.items.len(), 2);
    let other = UserDomainRef {
        user_id: "user:b".into(),
        ..domain.clone()
    };
    assert!(reconcile(&graph, &other, &restarted, input).is_err());
    assert!(
        graph
            .query(&other, WorkGraphQuery::default())
            .unwrap()
            .items
            .is_empty()
    );
}

#[test]
fn uncertain_publication_is_quarantined_not_completed_and_invalidates_old_readiness() {
    struct Fail(std::path::PathBuf, std::sync::atomic::AtomicBool);
    impl TransactionFaults for Fail {
        fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
            if point == TransactionFaultPoint::AfterSnapshotPublish
                && std::fs::read(&self.0).ok().as_deref() == Some(b"new")
                && !self.1.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                return Err(PersistenceError::new(
                    PersistenceErrorKind::RetryableIo,
                    "lost publication witness",
                ));
            }
            Ok(())
        }
    }
    let (dir, owner, graph, domain) = fixture();
    write(&owner, "note.md", "old");
    let original = observed(&graph, &domain, &owner, "note.md");
    readiness(&graph, &domain, &original);
    owner.set_transaction(FileTransaction::with_faults(
        owner.files.clone(),
        Arc::new(Fail(
            dir.path().join("vault/note.md"),
            std::sync::atomic::AtomicBool::new(false),
        )),
    ));
    assert!(
        commit_write(
            &owner,
            WriteMutation {
                path: "note.md".into(),
                content: "new".into(),
                precondition: MutationPrecondition::Unconditional,
                expected_version: None
            }
        )
        .is_err()
    );
    owner.set_transaction(FileTransaction::new(owner.files.clone()));
    let inspection = reconcile(
        &graph,
        &domain,
        &owner,
        WorkNativeReconcileInput {
            root_id: "personal".into(),
            command: VaultReconcileCommand::Inspect {},
        },
    )
    .unwrap();
    let pending = &inspection["pending_journals"][0];
    assert_eq!(pending["has_publication_witness"], false);
    assert!(inspection["recovery_error"].is_string());
    let input = WorkNativeReconcileInput {
        root_id: "personal".into(),
        command: VaultReconcileCommand::QuarantineJournal {
            command_id: "quarantine-note".into(),
            operation_id: pending["operation_id"].as_str().unwrap().into(),
            expected_intent_digest: pending["intent_digest"].as_str().unwrap().into(),
        },
    };
    let quarantined = reconcile(&graph, &domain, &owner, input.clone()).unwrap();
    assert_eq!(
        quarantined["reconciliation"]["disposition"],
        "journal_quarantined"
    );
    assert_eq!(quarantined["native_outcome"], "unresolved");
    assert_eq!(quarantined["file_effects_replayed"], false);
    assert_eq!(quarantined["resources"][0]["resolution"], "unavailable");
    assert!(!graph.inspect_work_unit(&domain, "maintenance").unwrap().1);
    assert_eq!(
        std::fs::read(dir.path().join("vault/note.md")).unwrap(),
        b"new"
    );
    let replacement = observed(&graph, &domain, &owner, "note.md");
    assert_ne!(replacement.reference, original.reference);
    let before = graph
        .query(&domain, WorkGraphQuery::default())
        .unwrap()
        .revision;
    assert_eq!(
        reconcile(&graph, &domain, &owner, input).unwrap()["reconciliation"]["replayed"],
        true
    );
    assert_eq!(
        graph
            .query(&domain, WorkGraphQuery::default())
            .unwrap()
            .revision,
        before
    );
    write(&owner, "unblocked.md", "next");
}

#[test]
fn move_commands_cannot_override_kind_authority_root_or_request_evidence() {
    let (_dir, owner, graph, domain) = fixture();
    write(&owner, "note.md", "body");
    let resource = observed(&graph, &domain, &owner, "note.md");
    for change in ["authority", "kind", "root"] {
        let mut reference = resource.reference.clone();
        match change {
            "authority" => {
                reference.authority_id =
                    medousa_types::AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap()
            }
            "kind" => reference.kind = ResourceKind::VaultFolder,
            "root" => reference.id = format!("vault:{}:user:{}", "f".repeat(32), "a".repeat(32)),
            _ => unreachable!(),
        }
        assert!(
            reconcile(
                &graph,
                &domain,
                &owner,
                WorkNativeReconcileInput {
                    root_id: "personal".into(),
                    command: VaultReconcileCommand::AdoptMove {
                        command_id: change.into(),
                        reference,
                        expected_native_revision: resource.native_revision.clone().unwrap(),
                        path: "new.md".into()
                    }
                }
            )
            .is_err()
        );
    }
    assert!(serde_json::from_value::<WorkNativeReconcileInput>(serde_json::json!({"root_id":"personal","command":{"operation":"quarantine_journal","command_id":"fake","operation_id":"unknown","expected_intent_digest":"unknown","allow_hash_match":true}})).is_err());
    assert!(
        serde_json::from_value::<WorkNativeReconcileInput>(
            serde_json::json!({"command":{"operation":"inspect"}})
        )
        .is_err()
    );
}
