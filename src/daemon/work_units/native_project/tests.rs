use super::*;
use medousa_forge::{
    git::{CheckpointAuthor, GitEngine},
    model::{ActorKind, ActorRef, WorkspaceMode},
};
use medousa_store::{PersistenceError, TransactionFaultPoint, TransactionFaults};
use std::sync::atomic::{AtomicBool, Ordering};

struct Fixture {
    dir: tempfile::TempDir,
    forge: Forge,
    graph: WorkGraphStore,
    domain: UserDomainRef,
    repo: PathBuf,
}
fn actor() -> ActorRef {
    ActorRef {
        kind: ActorKind::User,
        id: "user:test".into(),
    }
}
fn init_repo(path: &std::path::Path) {
    std::fs::create_dir_all(path).unwrap();
    let out = std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(path)
        .output()
        .unwrap();
    assert!(out.status.success());
    std::fs::write(path.join("hello.txt"), "hello\n").unwrap();
    let out = std::process::Command::new("git")
        .args(["add", "hello.txt"])
        .current_dir(path)
        .output()
        .unwrap();
    assert!(out.status.success());
    GitEngine::detect()
        .unwrap()
        .commit_checkpoint(path, "initial", &CheckpointAuthor::default())
        .unwrap();
}
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().canonicalize().unwrap().join("repo");
    init_repo(&repo);
    let forge = Forge::open(dir.path().canonicalize().unwrap().join("forge")).unwrap();
    let graph = WorkGraphStore::open(&dir.path().canonicalize().unwrap().join("graph")).unwrap();
    let domain = UserDomainRef {
        authority_id: medousa_types::AuthorityId::parse(format!("auth_{}", "a".repeat(64)))
            .unwrap(),
        user_id: "user:test".into(),
    };
    Fixture {
        dir,
        forge,
        graph,
        domain,
        repo,
    }
}
fn register(fx: &Fixture, repo: &std::path::Path, title: &str, provision: bool) -> WorkItem {
    let item = fx
        .forge
        .register_with_workspace_mode(
            title,
            "brief",
            repo,
            "main",
            "user:test",
            WorkspaceMode::AttachedCheckout,
            &actor(),
        )
        .unwrap();
    if provision {
        fx.forge.provision(&item.id, &actor()).unwrap()
    } else {
        item
    }
}
fn observe(fx: &Fixture, item: &WorkItem, target: ProjectResolveTarget) -> serde_json::Value {
    resolve(
        &fx.graph,
        &fx.domain,
        &fx.forge,
        WorkProjectResolveInput {
            work_id: item.id.to_string(),
            target,
        },
    )
    .unwrap()
}
fn resource(value: &serde_json::Value) -> ResourceRecord {
    serde_json::from_value(value["resource"].clone()).unwrap()
}
fn mutate(fx: &Fixture, mutation: WorkGraphMutation) {
    let revision = fx
        .graph
        .query(&fx.domain, WorkGraphQuery::default())
        .unwrap()
        .revision;
    fx.graph
        .apply(
            &fx.domain,
            WorkGraphCommand {
                command_id: format!("test-{revision}"),
                expected_revision: revision,
                mutation,
            },
            RecordProvenance {
                actor_id: "user:test".into(),
                source: RecordSource::UserDirect,
                evidence: vec![],
            },
        )
        .unwrap();
}
fn pin(fx: &Fixture, r: &ResourceRecord) {
    mutate(
        fx,
        WorkGraphMutation::AcceptWork {
            work_unit_id: "maintenance".into(),
            intent: "Keep metadata current".into(),
            kind: WorkUnitKind::Maintenance,
            scope: WorkScope {
                resources: vec![r.reference.clone()],
                ..Default::default()
            },
            completion_condition: "Current".into(),
            contact: WorkContactPreference::Silent,
            origin: None,
            budget: None,
        },
    );
    mutate(
        fx,
        WorkGraphMutation::SetState {
            work_unit_id: "maintenance".into(),
            state: WorkUnitState::Active,
            reason: "Maintain".into(),
            evidence: vec![],
        },
    );
    mutate(
        fx,
        WorkGraphMutation::RecordReadiness {
            work_unit_id: "maintenance".into(),
            condition: "Current".into(),
            expected_scope_revision: fx
                .graph
                .work_unit(&fx.domain, "maintenance")
                .unwrap()
                .scope_revision,
            evidence: vec![WorkRevisionEvidence {
                reference: r.reference.clone(),
                native_revision: r.native_revision.clone().unwrap(),
            }],
            valid_for_seconds: 3600,
        },
    );
}
#[test]
fn repository_groups_and_work_threads_have_distinct_durable_identities_and_real_links() {
    let fx = fixture();
    let first = register(&fx, &fx.repo, "first", false);
    let second = register(&fx, &fx.repo, "second", false);
    let other_repo = fx.dir.path().canonicalize().unwrap().join("other");
    init_repo(&other_repo);
    let other = register(&fx, &other_repo, "other", false);
    let p = resource(&observe(&fx, &first, ProjectResolveTarget::Project {}));
    assert_eq!(
        p.reference,
        resource(&observe(&fx, &second, ProjectResolveTarget::Project {})).reference
    );
    let p2 = resource(&observe(&fx, &other, ProjectResolveTarget::Project {}));
    assert_ne!(p.reference, p2.reference);
    let w1 = resource(&observe(&fx, &first, ProjectResolveTarget::ForgeWork {}));
    let w2 = resource(&observe(&fx, &second, ProjectResolveTarget::ForgeWork {}));
    assert_ne!(w1.reference, w2.reference);
    assert_ne!(w1.reference.kind, p.reference.kind);
    let vault_path = fx.dir.path().canonicalize().unwrap().join("vault");
    let owner = VaultIndexOwner::new(
        VaultRootId::new("test-vault"),
        Arc::new(StoreRoot::open_or_create_nofollow(&vault_path).unwrap()),
    );
    crate::vault::mutation::commit_write(
        &owner,
        crate::vault::mutation::WriteMutation {
            path: "design.md".into(),
            content: "shared design".into(),
            precondition: crate::vault::contracts::MutationPrecondition::CreateOnly,
            expected_version: None,
        },
    )
    .unwrap();
    let note = resource(
        &native_vault::resolve(
            &fx.graph,
            &fx.domain,
            &owner,
            VaultResolveTarget::Note {
                path: "design.md".into(),
            },
        )
        .unwrap(),
    );
    for (n, project) in [&p, &p2].iter().enumerate() {
        mutate(
            &fx,
            WorkGraphMutation::PutRelationship {
                relationship_id: format!("informs-{n}"),
                from: note.reference.clone(),
                to: project.reference.clone(),
                kind: ResourceRelationshipKind::Informs,
            },
        );
    }
    let mut folders = Vec::new();
    for (n, path) in ["test-notes", "release-notes"].into_iter().enumerate() {
        owner
            .files
            .create_dir_all(&VaultPath::parse(path).unwrap())
            .unwrap();
        let folder = resource(
            &native_vault::resolve(
                &fx.graph,
                &fx.domain,
                &owner,
                VaultResolveTarget::Folder { path: path.into() },
            )
            .unwrap(),
        );
        mutate(
            &fx,
            WorkGraphMutation::PutRelationship {
                relationship_id: format!("produces-{n}"),
                from: p.reference.clone(),
                to: folder.reference.clone(),
                kind: ResourceRelationshipKind::Produces,
            },
        );
        folders.push(folder.reference);
    }
    let linked = fx
        .graph
        .query(
            &fx.domain,
            WorkGraphQuery {
                collection: WorkGraphCollection::Relationships,
                anchor: Some(note.reference.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        linked
            .items
            .iter()
            .filter(|i| matches!(i, WorkGraphItem::Relationship(_)))
            .count()
            == 2
    );
    let forge = Forge::open(fx.dir.path().canonicalize().unwrap().join("forge")).unwrap();
    let graph = WorkGraphStore::open(&fx.dir.path().canonicalize().unwrap().join("graph")).unwrap();
    let before = graph
        .query(&fx.domain, WorkGraphQuery::default())
        .unwrap()
        .revision;
    let replay = resolve(
        &graph,
        &fx.domain,
        &forge,
        WorkProjectResolveInput {
            work_id: first.id.to_string(),
            target: ProjectResolveTarget::Reference {
                reference: p.reference.clone(),
            },
        },
    )
    .unwrap();
    assert_eq!(resource(&replay).reference, p.reference);
    assert_eq!(replay["graph_revision"], before);
    assert_eq!(replay["receipt"]["replayed"], true);
    assert_eq!(forge.load(&first.id).unwrap(), first);
    assert_eq!(forge.load(&second.id).unwrap(), second);
    assert_eq!(
        graph
            .query(
                &fx.domain,
                WorkGraphQuery {
                    collection: WorkGraphCollection::Relationships,
                    anchor: Some(p.reference),
                    ..Default::default()
                }
            )
            .unwrap()
            .items
            .iter()
            .filter(|i| matches!(i, WorkGraphItem::Relationship(_)))
            .count(),
        3
    );
    for folder in folders {
        let inverse = graph
            .query(
                &fx.domain,
                WorkGraphQuery {
                    collection: WorkGraphCollection::Relationships,
                    anchor: Some(folder),
                    direction: RelationshipDirection::Incoming,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(inverse.items.len(), 1);
    }
}
#[test]
fn work_lifecycle_invalidates_readiness_without_launching_or_completing_units() {
    let fx = fixture();
    let item = register(&fx, &fx.repo, "discardable", false);
    let work = resource(&observe(&fx, &item, ProjectResolveTarget::ForgeWork {}));
    let revision = fx
        .graph
        .query(&fx.domain, WorkGraphQuery::default())
        .unwrap()
        .revision;
    assert!(
        fx.graph
            .apply(
                &fx.domain,
                WorkGraphCommand {
                    command_id: "model-downgrade".into(),
                    expected_revision: revision,
                    mutation: WorkGraphMutation::RecordResource {
                        reference: work.reference.clone(),
                        locator: None,
                        native_revision: None,
                        resolution: ResourceResolution::Unresolved
                    }
                },
                RecordProvenance {
                    actor_id: fx.domain.user_id.clone(),
                    source: RecordSource::ModelInferred,
                    evidence: vec![]
                }
            )
            .is_err()
    );
    let refreshed = resource(&observe(
        &fx,
        &item,
        ProjectResolveTarget::Reference {
            reference: work.reference.clone(),
        },
    ));
    assert_eq!(refreshed.native_revision, work.native_revision);
    assert_eq!(refreshed.resolution, ResourceResolution::Available);
    assert_eq!(refreshed.provenance.source, RecordSource::SystemEvent);
    pin(&fx, &refreshed);
    assert!(
        fx.graph
            .inspect_work_unit(&fx.domain, "maintenance")
            .unwrap()
            .1
    );
    fx.forge.discard(&item.id, &actor()).unwrap();
    let changed = resource(&observe(
        &fx,
        &item,
        ProjectResolveTarget::Reference {
            reference: work.reference.clone(),
        },
    ));
    assert_eq!(changed.reference, work.reference);
    assert_ne!(changed.native_revision, work.native_revision);
    assert!(
        !fx.graph
            .inspect_work_unit(&fx.domain, "maintenance")
            .unwrap()
            .1
    );
    assert_eq!(
        fx.graph.work_unit(&fx.domain, "maintenance").unwrap().state,
        WorkUnitState::Active
    );
}
#[test]
fn missing_and_replaced_repositories_keep_old_links_and_never_inherit_identity() {
    let fx = fixture();
    let item = register(&fx, &fx.repo, "attached", true);
    let original = resource(&observe(&fx, &item, ProjectResolveTarget::Project {}));
    pin(&fx, &original);
    std::fs::rename(fx.repo.join(".git"), fx.repo.join(".git-retained")).unwrap();
    let missing = resource(&observe(
        &fx,
        &item,
        ProjectResolveTarget::Reference {
            reference: original.reference.clone(),
        },
    ));
    assert_eq!(missing.resolution, ResourceResolution::Unavailable);
    assert!(
        !fx.graph
            .inspect_work_unit(&fx.domain, "maintenance")
            .unwrap()
            .1
    );
    init_repo(&fx.repo);
    let replaced = observe(&fx, &item, ProjectResolveTarget::Project {});
    let new = resource(&replaced);
    assert_ne!(new.reference, original.reference);
    let old = resource(&observe(
        &fx,
        &item,
        ProjectResolveTarget::Reference {
            reference: original.reference.clone(),
        },
    ));
    assert_eq!(old.resolution, ResourceResolution::Unavailable);
    assert_eq!(replaced["affected_resources"].as_array().unwrap().len(), 2);
    std::fs::remove_dir_all(fx.repo.join(".git")).unwrap();
    std::fs::rename(fx.repo.join(".git-retained"), fx.repo.join(".git")).unwrap();
    let restored = resource(&observe(&fx, &item, ProjectResolveTarget::Project {}));
    assert_eq!(restored.reference, original.reference);
    assert_eq!(restored.resolution, ResourceResolution::Available);
}
#[test]
fn exact_overlay_uses_pinned_work_environment_and_rejects_aliases_and_replacement() {
    let fx = fixture();
    let item = register(&fx, &fx.repo, "overlay", true);
    let generation = item.workspace_environment().unwrap().generation;
    let root = fx.repo.join(".medousa/vault");
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("notes/design.md"), "project design").unwrap();
    let target = || ProjectResolveTarget::Overlay {
        environment_generation: generation,
        environment_branch: item.workspace_environment().unwrap().branch.clone(),
        target: VaultResolveTarget::Note {
            path: "notes/design.md".into(),
        },
    };
    let note = resource(&observe(&fx, &item, target()));
    assert!(note.reference.id.contains(":project:"));
    pin(&fx, &note);
    let folder = resource(&observe(
        &fx,
        &item,
        ProjectResolveTarget::Overlay {
            environment_generation: generation,
            environment_branch: item.workspace_environment().unwrap().branch.clone(),
            target: VaultResolveTarget::Folder {
                path: "notes".into(),
            },
        },
    ));
    assert_ne!(folder.reference, note.reference);
    let before = fx
        .graph
        .query(&fx.domain, WorkGraphQuery::default())
        .unwrap()
        .revision;
    assert!(
        resolve(
            &fx.graph,
            &fx.domain,
            &fx.forge,
            WorkProjectResolveInput {
                work_id: item.id.to_string(),
                target: ProjectResolveTarget::Overlay {
                    environment_generation: generation + 1,
                    environment_branch: item.workspace_environment().unwrap().branch.clone(),
                    target: VaultResolveTarget::Note {
                        path: "notes/design.md".into()
                    }
                }
            }
        )
        .is_err()
    );
    assert!(
        resolve(
            &fx.graph,
            &fx.domain,
            &fx.forge,
            WorkProjectResolveInput {
                work_id: item.id.to_string(),
                target: ProjectResolveTarget::Overlay {
                    environment_generation: generation,
                    environment_branch: "wrong-branch".into(),
                    target: VaultResolveTarget::Note {
                        path: "notes/design.md".into()
                    }
                }
            }
        )
        .is_err()
    );
    let mut wrong = note.reference.clone();
    wrong.id = wrong.id.replace(":project:", ":user:");
    assert!(
        resolve(
            &fx.graph,
            &fx.domain,
            &fx.forge,
            WorkProjectResolveInput {
                work_id: item.id.to_string(),
                target: ProjectResolveTarget::Overlay {
                    environment_generation: generation,
                    environment_branch: item.workspace_environment().unwrap().branch.clone(),
                    target: VaultResolveTarget::Reference { reference: wrong }
                }
            }
        )
        .is_err()
    );
    assert_eq!(
        fx.graph
            .query(&fx.domain, WorkGraphQuery::default())
            .unwrap()
            .revision,
        before
    );
    std::fs::write(root.join("notes/replacement.md"), "project design").unwrap();
    std::fs::rename(
        root.join("notes/replacement.md"),
        root.join("notes/design.md"),
    )
    .unwrap();
    let old = resource(&observe(
        &fx,
        &item,
        ProjectResolveTarget::Overlay {
            environment_generation: generation,
            environment_branch: item.workspace_environment().unwrap().branch.clone(),
            target: VaultResolveTarget::Reference {
                reference: note.reference.clone(),
            },
        },
    ));
    assert_eq!(old.resolution, ResourceResolution::Unavailable);
    assert!(
        !fx.graph
            .inspect_work_unit(&fx.domain, "maintenance")
            .unwrap()
            .1
    );
    assert_ne!(
        resource(&observe(&fx, &item, target())).reference,
        note.reference
    );
    #[cfg(unix)]
    {
        let outside = fx.dir.path().canonicalize().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.md"), "secret").unwrap();
        std::os::unix::fs::symlink(outside, root.join("escape")).unwrap();
        assert!(
            resolve(
                &fx.graph,
                &fx.domain,
                &fx.forge,
                WorkProjectResolveInput {
                    work_id: item.id.to_string(),
                    target: ProjectResolveTarget::Overlay {
                        environment_generation: generation,
                        environment_branch: item.workspace_environment().unwrap().branch.clone(),
                        target: VaultResolveTarget::Note {
                            path: "escape/secret.md".into()
                        }
                    }
                }
            )
            .is_err()
        );
    }
    assert_eq!(fx.forge.load(&item.id).unwrap(), item);
}
struct FailOnce(AtomicBool);
impl TransactionFaults for FailOnce {
    fn check(&self, point: TransactionFaultPoint) -> std::result::Result<(), PersistenceError> {
        if point == TransactionFaultPoint::AfterSnapshotPublish
            && self.0.swap(false, Ordering::SeqCst)
        {
            Err(PersistenceError::new(
                medousa_store::PersistenceErrorKind::RetryableIo,
                "injected graph publication",
            ))
        } else {
            Ok(())
        }
    }
}
#[test]
fn interrupted_graph_publication_reopens_with_stable_native_identity_and_exact_replay() {
    let fx = fixture();
    let item = register(&fx, &fx.repo, "repair", false);
    let graph = WorkGraphStore::with_faults(
        &fx.dir.path().canonicalize().unwrap().join("fault-graph"),
        Arc::new(FailOnce(AtomicBool::new(true))),
    )
    .unwrap();
    assert!(
        resolve(
            &graph,
            &fx.domain,
            &fx.forge,
            WorkProjectResolveInput {
                work_id: item.id.to_string(),
                target: ProjectResolveTarget::Project {}
            }
        )
        .is_err()
    );
    let saved = graph.query(&fx.domain, WorkGraphQuery::default()).unwrap();
    let original = match &saved.items[0] {
        WorkGraphItem::Resource(r) => r,
        _ => panic!("resource"),
    };
    let graph =
        WorkGraphStore::open(&fx.dir.path().canonicalize().unwrap().join("fault-graph")).unwrap();
    let forge = Forge::open(fx.dir.path().canonicalize().unwrap().join("forge")).unwrap();
    let replay = resolve(
        &graph,
        &fx.domain,
        &forge,
        WorkProjectResolveInput {
            work_id: item.id.to_string(),
            target: ProjectResolveTarget::Project {},
        },
    )
    .unwrap();
    assert_eq!(resource(&replay).reference, original.reference);
    assert_eq!(replay["graph_revision"], saved.revision);
    assert_eq!(replay["receipt"]["replayed"], true);
}
#[test]
fn foreign_owners_authorities_work_ids_and_missing_overlays_cannot_change_graph() {
    let fx = fixture();
    let item = register(&fx, &fx.repo, "owned", true);
    let p = resource(&observe(&fx, &item, ProjectResolveTarget::Project {}));
    let before = fx
        .graph
        .query(&fx.domain, WorkGraphQuery::default())
        .unwrap()
        .revision;
    let foreign = UserDomainRef {
        user_id: "user:other".into(),
        ..fx.domain.clone()
    };
    assert!(
        resolve(
            &fx.graph,
            &foreign,
            &fx.forge,
            WorkProjectResolveInput {
                work_id: item.id.to_string(),
                target: ProjectResolveTarget::Project {}
            }
        )
        .is_err()
    );
    assert_eq!(
        fx.graph
            .query(&foreign, WorkGraphQuery::default())
            .unwrap()
            .revision,
        0
    );
    for id in ["../escape", "", "work-missing"] {
        assert!(
            resolve(
                &fx.graph,
                &fx.domain,
                &fx.forge,
                WorkProjectResolveInput {
                    work_id: id.into(),
                    target: ProjectResolveTarget::ForgeWork {}
                }
            )
            .is_err()
        );
    }
    let mut reference = p.reference;
    reference.authority_id =
        medousa_types::AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
    assert!(
        resolve(
            &fx.graph,
            &fx.domain,
            &fx.forge,
            WorkProjectResolveInput {
                work_id: item.id.to_string(),
                target: ProjectResolveTarget::Reference { reference }
            }
        )
        .is_err()
    );
    assert!(
        resolve(
            &fx.graph,
            &fx.domain,
            &fx.forge,
            WorkProjectResolveInput {
                work_id: item.id.to_string(),
                target: ProjectResolveTarget::Overlay {
                    environment_generation: item.workspace_environment().unwrap().generation,
                    environment_branch: item.workspace_environment().unwrap().branch.clone(),
                    target: VaultResolveTarget::Note {
                        path: "missing.md".into()
                    }
                }
            }
        )
        .is_err()
    );
    assert!(!fx.repo.join(".medousa/vault").exists());
    assert_eq!(
        fx.graph
            .query(&fx.domain, WorkGraphQuery::default())
            .unwrap()
            .revision,
        before
    );
    let held = fx.forge.store().lock_item(&item.id).unwrap();
    assert!(
        resolve(
            &fx.graph,
            &fx.domain,
            &fx.forge,
            WorkProjectResolveInput {
                work_id: item.id.to_string(),
                target: ProjectResolveTarget::ForgeWork {}
            }
        )
        .is_err()
    );
    drop(held);
}

#[tokio::test]
async fn host_admits_project_observation_and_rejects_read_only_or_foreign_owners() {
    crate::workshop_authority::initialize(
        &medousa_types::secrets::InstallationId::parse(
            crate::workshop_authority::TEST_INSTALLATION_ID,
        )
        .unwrap(),
    )
    .unwrap();
    let execution = Arc::new(ForgeExecutionService::new());
    let (fx, item) = execution
        .run(ExecutionClass::Observation, 8 * 1024 * 1024, || {
            let fx = fixture();
            let item = register(&fx, &fx.repo, "host", false);
            Ok((fx, item))
        })
        .await
        .unwrap();
    let host = WorkUnitHost {
        store: Arc::new(fx.graph),
        execution,
        forge: Arc::new(fx.forge),
    };
    let turn = super::super::tests::turn("user:test", "project-host");
    let input = || WorkProjectResolveInput {
        work_id: item.id.to_string(),
        target: ProjectResolveTarget::Project {},
    };
    let observed = host.resolve_project(&turn, input()).await.unwrap();
    assert_eq!(observed["resource"]["resolution"], "available");
    assert_eq!(
        host.resolve_project(&turn, input()).await.unwrap()["receipt"]["replayed"],
        true
    );
    let foreign = super::super::tests::turn("user:other", "project-other");
    assert!(host.resolve_project(&foreign, input()).await.is_err());
    let read_only = TurnExecutionContext::from_scope(
        "read-only",
        RequestPrincipal::external_agent(
            Arc::from("external:test"),
            "user:test".into(),
            false,
            crate::request_principal::TransportClass::Direct,
        ),
        tokio_util::sync::CancellationToken::new(),
        None,
        turn.legacy_scope().clone(),
    )
    .unwrap();
    assert!(host.resolve_project(&read_only, input()).await.is_err());
}
