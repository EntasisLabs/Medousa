//! Exact native observations, published after vault journal/identity repair.

use super::*;
use crate::vault::{
    identity::{VaultIdentityTransaction, VaultResourceKind},
    owner::VaultIndexOwner,
    path::VaultPath,
};

mod reconciliation;
pub use reconciliation::WorkNativeReconcileInput;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkNativeResolveInput {
    /// Configured vault root ID; never defaults to today's selected vault.
    pub root_id: String,
    pub target: VaultResolveTarget,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VaultResolveTarget {
    Note { path: String },
    Folder { path: String },
    Reference { reference: ResourceRef },
}

pub(super) fn resolve(
    store: &WorkGraphStore,
    domain: &UserDomainRef,
    owner: &Arc<VaultIndexOwner>,
    target: VaultResolveTarget,
) -> Result<serde_json::Value> {
    if let VaultResolveTarget::Reference { reference } = &target
        && (reference.authority_id != domain.authority_id
            || !matches!(
                reference.kind,
                ResourceKind::VaultNote | ResourceKind::VaultFolder
            ))
    {
        bail!("native vault resolution requires a local vault reference");
    }
    crate::vault::mutation::recover_all_pending_writes(owner)?;
    let mut identities = VaultIdentityTransaction::begin(owner, false)?;
    let record = match target {
        VaultResolveTarget::Note { path } => {
            identities.observe_path(&VaultPath::parse(&path)?, VaultResourceKind::Note)?
        }
        VaultResolveTarget::Folder { path } => {
            identities.observe_path(&VaultPath::parse(&path)?, VaultResourceKind::Folder)?
        }
        VaultResolveTarget::Reference { reference } => {
            let parts: Vec<_> = reference.id.split(':').collect();
            if parts.len() != 4
                || parts[0] != "vault"
                || parts[1] != identities.vault_id()
                || parts[2] != "user"
            {
                bail!(
                    "vault reference does not belong to the selected physical root and user source"
                );
            }
            let record = identities.observe_id(parts[3])?;
            if !matches!(
                (reference.kind, record.kind),
                (ResourceKind::VaultNote, VaultResourceKind::Note)
                    | (ResourceKind::VaultFolder, VaultResourceKind::Folder)
            ) {
                bail!("vault reference kind does not match its native identity");
            }
            record
        }
    };
    project(store, domain, identities.vault_id(), record)
}

fn project(
    store: &WorkGraphStore,
    domain: &UserDomainRef,
    vault_id: &str,
    record: crate::vault::identity::VaultResourceIdentity,
) -> Result<serde_json::Value> {
    let reference = ResourceRef {
        authority_id: domain.authority_id.clone(),
        kind: match record.kind {
            VaultResourceKind::Note => ResourceKind::VaultNote,
            VaultResourceKind::Folder => ResourceKind::VaultFolder,
        },
        id: format!("vault:{vault_id}:user:{}", record.resource_id),
    };
    let locator = Some(
        serde_json::json!({"vault_id":vault_id,"source":"user","path":record.path}).to_string(),
    );
    let native_revision = Some(format!(
        "vault-observation-v1:{vault_id}:{}",
        record.revision
    ));
    let resolution_detail = match record.resolution {
        ResourceResolution::Unavailable => Some(
            "Resource is missing, replaced, or exceeds the bounded native observation limit; identity was retained.",
        ),
        ResourceResolution::Tombstoned => {
            Some("Native deletion is retained; this identity will not be reused.")
        }
        _ => None,
    };
    let mutation = WorkGraphMutation::RecordResource {
        reference: reference.clone(),
        locator: locator.clone(),
        native_revision: native_revision.clone(),
        resolution: record.resolution,
    };
    let command_id = {
        use sha2::{Digest, Sha256};
        format!(
            "vault-observe:{:x}",
            Sha256::digest(serde_json::to_vec(&(domain, &mutation))?)
        )
    };
    // Keep native custody locked through graph publication. Unrelated graph
    // writers may advance CAS; bounded retries never reuse an older observation.
    for _ in 0..3 {
        let page = store.query(
            domain,
            WorkGraphQuery {
                anchor: Some(reference.clone()),
                ..Default::default()
            },
        )?;
        if let Some(WorkGraphItem::Resource(existing)) = page.items.first()
            && existing.locator == locator
            && existing.native_revision == native_revision
            && existing.resolution == record.resolution
            && existing.provenance.source == RecordSource::SystemEvent
            && existing.provenance.actor_id == "adapter:vault"
        {
            // Reconstruct the original command, not a new observation. Its
            // exact replay finishes a possibly interrupted parent sync fence.
            let receipt = store.apply(
                domain,
                WorkGraphCommand {
                    command_id: command_id.clone(),
                    expected_revision: existing.revision.saturating_sub(1),
                    mutation: mutation.clone(),
                },
                existing.provenance.clone(),
            )?;
            return bounded_response(
                serde_json::json!({"resource":existing,"graph_revision":page.revision,"receipt":receipt,"coverage":"user_vault_exact_resource","resolution_detail":resolution_detail}),
            );
        }
        let command = WorkGraphCommand {
            command_id: command_id.clone(),
            expected_revision: page.revision,
            mutation: mutation.clone(),
        };
        match store.apply(
            domain,
            command,
            RecordProvenance {
                actor_id: "adapter:vault".into(),
                source: RecordSource::SystemEvent,
                evidence: vec![],
            },
        ) {
            Ok(receipt) => {
                let page = store.query(
                    domain,
                    WorkGraphQuery {
                        anchor: Some(reference.clone()),
                        ..Default::default()
                    },
                )?;
                return bounded_response(
                    serde_json::json!({"resource": match page.items.first() { Some(WorkGraphItem::Resource(resource)) => serde_json::to_value(resource)?, _ => bail!("native graph observation is missing") }, "graph_revision":page.revision,"receipt":receipt,"coverage":"user_vault_exact_resource","resolution_detail":resolution_detail}),
                );
            }
            Err(error) if error.kind == medousa_store::PersistenceErrorKind::Conflict => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("native graph observation conflicted with concurrent registry changes; retry resolution")
}

impl WorkUnitHost {
    pub async fn resolve_native(
        &self,
        turn: &TurnExecutionContext,
        input: WorkNativeResolveInput,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, true)?;
        if input.root_id.is_empty() || input.root_id.len() > 256 {
            bail!("explicit vault root ID is required");
        }
        let store = self.store.clone();
        self.execution
            .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                Ok((|| {
                    let path = crate::vault::roots::vault_root_by_id(&input.root_id)?;
                    let owner = crate::vault::owner::ensure_owner_for_root(path)?;
                    resolve(&store, &domain, &owner, input.target)
                })())
            })
            .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{
        contracts::{MutationPrecondition, VaultRootId},
        mutation::{WriteMutation, commit_write},
        relocate::{relocate_delete, relocate_move},
    };
    use medousa_store::StoreRoot;

    pub(super) fn fixture() -> (
        tempfile::TempDir,
        Arc<VaultIndexOwner>,
        WorkGraphStore,
        UserDomainRef,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let files = Arc::new(StoreRoot::open_or_create_nofollow(&root.join("vault")).unwrap());
        let owner = VaultIndexOwner::new(VaultRootId::new("test"), files);
        let graph = WorkGraphStore::open(&root.join("graph")).unwrap();
        let domain = UserDomainRef {
            authority_id: medousa_types::AuthorityId::parse(format!("auth_{}", "a".repeat(64)))
                .unwrap(),
            user_id: "user:a".into(),
        };
        (dir, owner, graph, domain)
    }
    pub(super) fn write(owner: &Arc<VaultIndexOwner>, path: &str, body: &str) {
        commit_write(
            owner,
            WriteMutation {
                path: path.into(),
                content: body.into(),
                precondition: MutationPrecondition::Unconditional,
                expected_version: None,
            },
        )
        .unwrap();
    }
    pub(super) fn native(value: &serde_json::Value) -> ResourceRecord {
        serde_json::from_value(value["resource"].clone()).unwrap()
    }
    pub(super) fn mutate(
        store: &WorkGraphStore,
        domain: &UserDomainRef,
        mutation: WorkGraphMutation,
    ) {
        let revision = store
            .query(domain, WorkGraphQuery::default())
            .unwrap()
            .revision;
        store
            .apply(
                domain,
                WorkGraphCommand {
                    command_id: format!("model-{revision}"),
                    expected_revision: revision,
                    mutation,
                },
                RecordProvenance {
                    actor_id: domain.user_id.clone(),
                    source: RecordSource::ModelInferred,
                    evidence: vec![],
                },
            )
            .unwrap();
    }

    #[test]
    fn real_note_links_keep_identity_and_inverse_queries_after_native_move_and_restart() {
        let (dir, owner, graph, domain) = fixture();
        write(&owner, "note.md", "# Design\n");
        owner
            .files
            .create_dir_all(&crate::vault::path::VaultPath::parse("notes").unwrap())
            .unwrap();
        let note = native(
            &resolve(
                &graph,
                &domain,
                &owner,
                VaultResolveTarget::Note {
                    path: "note.md".into(),
                },
            )
            .unwrap(),
        );
        let again = resolve(
            &graph,
            &domain,
            &owner,
            VaultResolveTarget::Reference {
                reference: note.reference.clone(),
            },
        )
        .unwrap();
        assert_eq!(again["graph_revision"], 1); // No history/wake inflation on reread.
        let folder = native(
            &resolve(
                &graph,
                &domain,
                &owner,
                VaultResolveTarget::Folder {
                    path: "notes".into(),
                },
            )
            .unwrap(),
        );
        for id in ["project-a", "project-b"] {
            let project = ResourceRef {
                kind: ResourceKind::Project,
                id: id.into(),
                authority_id: domain.authority_id.clone(),
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
                    relationship_id: format!("note-{id}"),
                    from: note.reference.clone(),
                    to: project.clone(),
                    kind: ResourceRelationshipKind::Informs,
                },
            );
            mutate(
                &graph,
                &domain,
                WorkGraphMutation::PutRelationship {
                    relationship_id: format!("folder-{id}"),
                    from: project,
                    to: folder.reference.clone(),
                    kind: ResourceRelationshipKind::Produces,
                },
            );
        }
        relocate_move(&owner, "note.md", "renamed.md").unwrap();
        let restarted_owner = VaultIndexOwner::new(
            VaultRootId::new("another-owner-instance"),
            owner.files.clone(),
        );
        let restarted_graph =
            WorkGraphStore::open(&dir.path().canonicalize().unwrap().join("graph")).unwrap();
        let moved = native(
            &resolve(
                &restarted_graph,
                &domain,
                &restarted_owner,
                VaultResolveTarget::Reference {
                    reference: note.reference.clone(),
                },
            )
            .unwrap(),
        );
        assert_eq!(moved.reference, note.reference);
        assert!(moved.locator.unwrap().contains("renamed.md"));
        let links = restarted_graph
            .query(
                &domain,
                WorkGraphQuery {
                    collection: WorkGraphCollection::Relationships,
                    anchor: Some(note.reference.clone()),
                    direction: RelationshipDirection::Outgoing,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(links.items.len(), 2);
        let inverse = restarted_graph
            .query(
                &domain,
                WorkGraphQuery {
                    collection: WorkGraphCollection::Relationships,
                    anchor: Some(folder.reference),
                    direction: RelationshipDirection::Incoming,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(inverse.items.len(), 2);
        relocate_delete(&restarted_owner, "renamed.md").unwrap();
        let deleted = native(
            &resolve(
                &restarted_graph,
                &domain,
                &restarted_owner,
                VaultResolveTarget::Reference {
                    reference: note.reference.clone(),
                },
            )
            .unwrap(),
        );
        assert_eq!(deleted.resolution, ResourceResolution::Tombstoned);
        write(&restarted_owner, "renamed.md", "# Design\n");
        let new = native(
            &resolve(
                &restarted_graph,
                &domain,
                &restarted_owner,
                VaultResolveTarget::Note {
                    path: "renamed.md".into(),
                },
            )
            .unwrap(),
        );
        assert_ne!(new.reference, note.reference);
        let other = UserDomainRef {
            user_id: "user:b".into(),
            ..domain.clone()
        };
        resolve(
            &restarted_graph,
            &other,
            &restarted_owner,
            VaultResolveTarget::Note {
                path: "renamed.md".into(),
            },
        )
        .unwrap();
        assert!(
            restarted_graph
                .query(
                    &other,
                    WorkGraphQuery {
                        collection: WorkGraphCollection::Relationships,
                        ..Default::default()
                    }
                )
                .unwrap()
                .items
                .is_empty()
        );
    }

    #[test]
    fn actual_native_observations_invalidate_saved_maintenance_readiness() {
        let (_dir, owner, graph, domain) = fixture();
        write(&owner, "docs.md", "version one");
        let note = native(
            &resolve(
                &graph,
                &domain,
                &owner,
                VaultResolveTarget::Note {
                    path: "docs.md".into(),
                },
            )
            .unwrap(),
        );
        mutate(
            &graph,
            &domain,
            WorkGraphMutation::AcceptWork {
                work_unit_id: "docs".into(),
                intent: "Maintain release docs".into(),
                kind: WorkUnitKind::Maintenance,
                scope: WorkScope {
                    resources: vec![note.reference.clone()],
                    ..Default::default()
                },
                completion_condition: "Documentation current".into(),
                contact: WorkContactPreference::Silent,
                origin: None,
                budget: None,
            },
        );
        let scope_revision = graph.work_unit(&domain, "docs").unwrap().scope_revision;
        mutate(
            &graph,
            &domain,
            WorkGraphMutation::SetState {
                work_unit_id: "docs".into(),
                state: WorkUnitState::Active,
                reason: "Maintaining accepted docs".into(),
                evidence: vec![],
            },
        );
        mutate(
            &graph,
            &domain,
            WorkGraphMutation::RecordReadiness {
                work_unit_id: "docs".into(),
                expected_scope_revision: scope_revision,
                condition: "Documentation current".into(),
                evidence: vec![WorkRevisionEvidence {
                    reference: note.reference.clone(),
                    native_revision: note.native_revision.unwrap(),
                }],
                valid_for_seconds: 3600,
            },
        );
        assert!(graph.inspect_work_unit(&domain, "docs").unwrap().1);
        write(&owner, "docs.md", "version two");
        resolve(
            &graph,
            &domain,
            &owner,
            VaultResolveTarget::Reference {
                reference: note.reference.clone(),
            },
        )
        .unwrap();
        assert!(!graph.inspect_work_unit(&domain, "docs").unwrap().1);
        assert_eq!(
            graph.work_unit(&domain, "docs").unwrap().state,
            WorkUnitState::Active
        );
    }

    #[test]
    fn uncertain_graph_publication_replays_its_receipt_without_a_new_observation() {
        use medousa_store::{
            PersistenceError, PersistenceErrorKind, TransactionFaultPoint, TransactionFaults,
        };
        struct FailOnce(std::sync::atomic::AtomicBool);
        impl TransactionFaults for FailOnce {
            fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
                if point == TransactionFaultPoint::AfterSnapshotPublish
                    && !self.0.swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    return Err(PersistenceError::new(
                        PersistenceErrorKind::RetryableIo,
                        "graph publication fence interrupted",
                    ));
                }
                Ok(())
            }
        }
        let (dir, owner, _, domain) = fixture();
        write(&owner, "note.md", "body");
        let graph = WorkGraphStore::with_faults(
            &dir.path().canonicalize().unwrap().join("faulty-graph"),
            Arc::new(FailOnce(std::sync::atomic::AtomicBool::new(false))),
        )
        .unwrap();
        let target = || VaultResolveTarget::Note {
            path: "note.md".into(),
        };
        assert!(resolve(&graph, &domain, &owner, target()).is_err());
        let replayed = resolve(&graph, &domain, &owner, target()).unwrap();
        assert_eq!(replayed["receipt"]["replayed"], true);
        assert_eq!(replayed["graph_revision"], 1);
        assert_eq!(
            resolve(&graph, &domain, &owner, target()).unwrap()["graph_revision"],
            1
        );
    }

    #[test]
    fn oversized_native_note_loses_availability_without_history_inflation() {
        let (_dir, owner, graph, domain) = fixture();
        write(&owner, "large.md", "small");
        let original = native(
            &resolve(
                &graph,
                &domain,
                &owner,
                VaultResolveTarget::Note {
                    path: "large.md".into(),
                },
            )
            .unwrap(),
        );
        write(
            &owner,
            "large.md",
            &"x".repeat(crate::vault::identity::MAX_OBSERVATION_NOTE_BYTES as usize + 1),
        );
        let target = || VaultResolveTarget::Reference {
            reference: original.reference.clone(),
        };
        let observed = resolve(&graph, &domain, &owner, target()).unwrap();
        let unavailable = native(&observed);
        assert_eq!(unavailable.reference, original.reference);
        assert_eq!(unavailable.resolution, ResourceResolution::Unavailable);
        assert_ne!(unavailable.native_revision, original.native_revision);
        assert!(
            observed["resolution_detail"]
                .as_str()
                .unwrap()
                .contains("bounded")
        );
        let repeated = resolve(&graph, &domain, &owner, target()).unwrap();
        assert_eq!(repeated["graph_revision"], observed["graph_revision"]);
        write(&owner, "large.md", "small again");
        let recovered = native(&resolve(&graph, &domain, &owner, target()).unwrap());
        assert_eq!(recovered.reference, original.reference);
        assert_eq!(recovered.resolution, ResourceResolution::Available);
    }

    #[test]
    fn resolution_cannot_retarget_foreign_authority_kind_root_or_hidden_path() {
        let (_dir, owner, graph, domain) = fixture();
        write(&owner, "note.md", "body");
        let record = native(
            &resolve(
                &graph,
                &domain,
                &owner,
                VaultResolveTarget::Note {
                    path: "note.md".into(),
                },
            )
            .unwrap(),
        );
        let mut foreign = record.reference.clone();
        foreign.authority_id =
            medousa_types::AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
        assert!(
            resolve(
                &graph,
                &domain,
                &owner,
                VaultResolveTarget::Reference { reference: foreign }
            )
            .is_err()
        );
        let mut wrong_kind = record.reference.clone();
        wrong_kind.kind = ResourceKind::VaultFolder;
        assert!(
            resolve(
                &graph,
                &domain,
                &owner,
                VaultResolveTarget::Reference {
                    reference: wrong_kind
                }
            )
            .is_err()
        );
        let (_other_dir, other, _, _) = fixture();
        assert!(
            resolve(
                &graph,
                &domain,
                &other,
                VaultResolveTarget::Reference {
                    reference: record.reference
                }
            )
            .is_err()
        );
        assert!(
            resolve(
                &graph,
                &domain,
                &owner,
                VaultResolveTarget::Note {
                    path: ".medousa/vault/resource-identities.json".into()
                }
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<WorkNativeResolveInput>(
                serde_json::json!({"target":{"kind":"note","path":"note.md"}})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<WorkNativeResolveInput>(serde_json::json!({"root_id":"personal","owner_id":"other","target":{"kind":"note","path":"note.md"}})).is_err());
    }
}
