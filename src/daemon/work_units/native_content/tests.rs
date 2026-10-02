use super::*;
use crate::{environment_store::EnvironmentHub, feed_store::FeedStore};
use medousa_types::{environment_default::default_environment_spec, feed::FeedEvent};

struct Fixture {
    dir: tempfile::TempDir,
    host: WorkUnitHost,
    domain: UserDomainRef,
    environment: EnvironmentHub,
    feeds: FeedStore,
}
async fn fixture() -> Fixture {
    crate::workshop_authority::initialize(
        &medousa_types::secrets::InstallationId::parse(
            crate::workshop_authority::TEST_INSTALLATION_ID,
        )
        .unwrap(),
    )
    .unwrap();
    let execution = Arc::new(ForgeExecutionService::new());
    let (dir, graph, forge) = execution
        .run(ExecutionClass::Observation, 8 * 1024 * 1024, || {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            Ok((
                dir,
                WorkGraphStore::open(&root.join("graph")).unwrap(),
                medousa_forge::forge::Forge::open(root.join("forge")).unwrap(),
            ))
        })
        .await
        .unwrap();
    let root = dir.path().canonicalize().unwrap();
    Fixture {
        dir,
        host: WorkUnitHost {
            store: Arc::new(graph),
            execution,
            forge: Arc::new(forge),
        },
        domain: UserDomainRef {
            authority_id: crate::workshop_authority::current().unwrap().clone(),
            user_id: "user:test".into(),
        },
        environment: EnvironmentHub::new_at(root.join("environment")),
        feeds: FeedStore::new_in(root.join("feeds")),
    }
}
async fn observe(fx: &Fixture, target: ContentResolveTarget) -> serde_json::Value {
    fx.host
        .resolve_content_in(
            &fx.domain,
            WorkContentResolveInput { target },
            &fx.environment,
            &fx.feeds,
        )
        .await
        .unwrap()
}
fn resource(value: &serde_json::Value) -> ResourceRecord {
    serde_json::from_value(value["resource"].clone()).unwrap()
}
fn event(feed: &str, n: usize) -> FeedEvent {
    FeedEvent {
        id: String::new(),
        feed_id: feed.into(),
        emitted_at_utc: chrono::Utc::now(),
        source: "agent".into(),
        summary: format!("private summary {n}"),
        refs: vec![],
        payload: Some(serde_json::json!({"private":"feed body"})),
    }
}
async fn put_component(fx: &Fixture, feeds: Vec<String>) {
    let mut spec = medousa_types::environment_default::writing_studio_demo_spec(&fx.domain.user_id);
    spec.components[0].id = "dashboard".into();
    spec.components[0].config =
        serde_json::json!({"artifactId":"friendly-alias","private":"component config body"});
    spec.components[0].feeds = feeds;
    // Existing native API; this fixture has no concurrent observer and performs disk I/O on its admitted lane.
    let hub = fx.environment.clone();
    let handle = tokio::runtime::Handle::current();
    fx.host
        .execution
        .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
            Ok(handle.block_on(hub.put(spec, "test")))
        })
        .await
        .unwrap()
        .unwrap();
}
async fn mutate(fx: &Fixture, mutation: WorkGraphMutation) {
    let graph = fx.host.store.clone();
    let domain = fx.domain.clone();
    fx.host
        .execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            let revision = graph
                .query(&domain, WorkGraphQuery::default())
                .unwrap()
                .revision;
            Ok(graph.apply(
                &domain,
                WorkGraphCommand {
                    command_id: format!("test-{revision}"),
                    expected_revision: revision,
                    mutation,
                },
                RecordProvenance {
                    actor_id: domain.user_id.clone(),
                    source: RecordSource::UserDirect,
                    evidence: vec![],
                },
            ))
        })
        .await
        .unwrap()
        .unwrap();
}
async fn pin(fx: &Fixture, r: &ResourceRecord) {
    mutate(
        fx,
        WorkGraphMutation::AcceptWork {
            work_unit_id: "maintenance".into(),
            intent: "Keep current".into(),
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
    )
    .await;
    mutate(
        fx,
        WorkGraphMutation::SetState {
            work_unit_id: "maintenance".into(),
            state: WorkUnitState::Active,
            reason: "Maintain".into(),
            evidence: vec![],
        },
    )
    .await;
    let graph = fx.host.store.clone();
    let domain = fx.domain.clone();
    let scope = fx
        .host
        .execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok(graph
                .work_unit(&domain, "maintenance")
                .unwrap()
                .scope_revision)
        })
        .await
        .unwrap();
    mutate(
        fx,
        WorkGraphMutation::RecordReadiness {
            work_unit_id: "maintenance".into(),
            condition: "Current".into(),
            expected_scope_revision: scope,
            evidence: vec![WorkRevisionEvidence {
                reference: r.reference.clone(),
                native_revision: r.native_revision.clone().unwrap(),
            }],
            valid_for_seconds: 3600,
        },
    )
    .await;
}
async fn ready(fx: &Fixture) -> bool {
    let graph = fx.host.store.clone();
    let domain = fx.domain.clone();
    fx.host
        .execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok(graph.inspect_work_unit(&domain, "maintenance").unwrap().1)
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn components_return_exact_feed_bindings_invalidate_readiness_and_survive_restart() {
    let mut fx = fixture().await;
    put_component(&fx, vec!["digest".into()]).await;
    let first = observe(
        &fx,
        ContentResolveTarget::Component {
            component_id: "dashboard".into(),
        },
    )
    .await;
    assert_eq!(first["bindings"]["artifact"]["resolution"], "unresolved");
    assert!(!first.to_string().contains("component config body"));
    let r = resource(&first);
    assert_eq!(r.reference.kind, ResourceKind::Component);
    assert_eq!(
        first["bindings"]["feeds"][0],
        serde_json::to_value(
            ContentLocator::Feed {
                feed_id: "digest".into()
            }
            .reference(&fx.domain)
            .unwrap()
        )
        .unwrap()
    );
    pin(&fx, &r).await;
    assert!(ready(&fx).await);
    put_component(&fx, vec![]).await;
    let changed = observe(
        &fx,
        ContentResolveTarget::Reference {
            reference: r.reference.clone(),
        },
    )
    .await;
    assert_eq!(resource(&changed).reference, r.reference);
    assert_ne!(resource(&changed).native_revision, r.native_revision);
    assert!(!ready(&fx).await);
    let root = fx.dir.path().canonicalize().unwrap();
    fx.environment = EnvironmentHub::new_at(root.join("environment"));
    let restarted = observe(
        &fx,
        ContentResolveTarget::Reference {
            reference: r.reference.clone(),
        },
    )
    .await;
    assert_eq!(
        resource(&changed).native_revision,
        resource(&restarted).native_revision
    );
    assert_eq!(restarted["receipt"]["replayed"], true);
    let hub = fx.environment.clone();
    let mut spec = default_environment_spec(&fx.domain.user_id);
    spec.components.clear();
    let handle = tokio::runtime::Handle::current();
    fx.host
        .execution
        .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
            Ok(handle.block_on(hub.put(spec, "test")))
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        observe(
            &fx,
            ContentResolveTarget::Reference {
                reference: r.reference
            }
        )
        .await["resource"]["resolution"],
        "unavailable"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_stream_revisions_ignore_read_cursor_but_track_events_and_reopen() {
    let mut fx = fixture().await;
    let empty = observe(
        &fx,
        ContentResolveTarget::Feed {
            feed_id: "digest".into(),
        },
    )
    .await;
    assert_eq!(empty["resource"]["resolution"], "unavailable");
    fx.feeds
        .append(&fx.domain.user_id, "digest", event("digest", 1))
        .await
        .unwrap();
    let first = observe(
        &fx,
        ContentResolveTarget::Feed {
            feed_id: "digest".into(),
        },
    )
    .await;
    let r = resource(&first);
    assert!(!first.to_string().contains("private"));
    pin(&fx, &r).await;
    fx.feeds
        .set_read_cursor(&fx.domain.user_id, "digest", 0)
        .await
        .unwrap();
    let same = observe(
        &fx,
        ContentResolveTarget::Reference {
            reference: r.reference.clone(),
        },
    )
    .await;
    assert_eq!(same["receipt"]["replayed"], true);
    assert!(ready(&fx).await);
    fx.feeds = FeedStore::new_in(fx.dir.path().canonicalize().unwrap().join("feeds"));
    assert_eq!(
        observe(
            &fx,
            ContentResolveTarget::Reference {
                reference: r.reference.clone()
            }
        )
        .await["receipt"]["replayed"],
        true
    );
    fx.feeds
        .append(&fx.domain.user_id, "digest", event("digest", 2))
        .await
        .unwrap();
    let changed = observe(
        &fx,
        ContentResolveTarget::Reference {
            reference: r.reference,
        },
    )
    .await;
    assert_ne!(resource(&changed).native_revision, r.native_revision);
    assert!(!ready(&fx).await);
}

async fn create_artifact(
    fx: &Fixture,
    session: String,
    html: &str,
) -> crate::artifact_store::ArtifactRecord {
    let owner = fx.domain.user_id.clone();
    let html = html.to_string();
    fx.host
        .execution
        .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
            crate::session_catalog::ensure_named_session_for_profile(&session, None, &owner)
                .unwrap();
            Ok(crate::artifact_store::persist_ui_artifact(
                &session,
                &html,
                "Dashboard",
                "inline",
                None,
            )
            .unwrap())
        })
        .await
        .unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn artifact_references_include_full_source_session_and_never_follow_alias_or_prefix() {
    let fx = fixture().await;
    let prefix = uuid::Uuid::new_v4().simple().to_string();
    let a = create_artifact(&fx, format!("{prefix}-a"), "<p>private HTML</p>").await;
    let b = create_artifact(&fx, format!("{prefix}-b"), "<p>private HTML</p>").await;
    assert_eq!(a.artifact_id, b.artifact_id); // Native legacy short-session collision.
    let first = observe(
        &fx,
        ContentResolveTarget::Artifact {
            session_id: a.session_id.clone(),
            artifact_id: a.artifact_id.clone(),
        },
    )
    .await;
    let turn = super::super::tests::turn("user:test", "another-content-chat");
    let admitted = fx
        .host
        .resolve_content(
            &turn,
            WorkContentResolveInput {
                target: ContentResolveTarget::Artifact {
                    session_id: a.session_id.clone(),
                    artifact_id: a.artifact_id.clone(),
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(admitted["receipt"]["replayed"], true);
    let second = observe(
        &fx,
        ContentResolveTarget::Artifact {
            session_id: b.session_id.clone(),
            artifact_id: b.artifact_id.clone(),
        },
    )
    .await;
    assert_ne!(resource(&first).reference, resource(&second).reference);
    assert!(!first.to_string().contains("private HTML"));
    let partial = observe(
        &fx,
        ContentResolveTarget::Artifact {
            session_id: a.session_id.clone(),
            artifact_id: a.artifact_id[..a.artifact_id.len() - 4].into(),
        },
    )
    .await;
    assert_eq!(partial["resource"]["resolution"], "unavailable");
    let mut other = fx.domain.clone();
    other.user_id = "user:other".into();
    assert!(
        fx.host
            .resolve_content_in(
                &other,
                WorkContentResolveInput {
                    target: ContentResolveTarget::Artifact {
                        session_id: a.session_id,
                        artifact_id: a.artifact_id
                    }
                },
                &fx.environment,
                &fx.feeds
            )
            .await
            .is_err()
    );
    assert!(
        fx.host
            .resolve_content_in(
                &other,
                WorkContentResolveInput {
                    target: ContentResolveTarget::Reference {
                        reference: resource(&first).reference
                    }
                },
                &fx.environment,
                &fx.feeds
            )
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn artifact_loss_refreshes_exact_reference_and_graph_relationships_persist() {
    let mut fx = fixture().await;
    let a = create_artifact(
        &fx,
        format!("content-{}", uuid::Uuid::new_v4()),
        "<p>content</p>",
    )
    .await;
    let artifact = resource(
        &observe(
            &fx,
            ContentResolveTarget::Artifact {
                session_id: a.session_id.clone(),
                artifact_id: a.artifact_id,
            },
        )
        .await,
    );
    put_component(&fx, vec!["digest".into()]).await;
    let component = resource(
        &observe(
            &fx,
            ContentResolveTarget::Component {
                component_id: "dashboard".into(),
            },
        )
        .await,
    );
    fx.feeds
        .append(&fx.domain.user_id, "digest", event("digest", 1))
        .await
        .unwrap();
    let feed = resource(
        &observe(
            &fx,
            ContentResolveTarget::Feed {
                feed_id: "digest".into(),
            },
        )
        .await,
    );
    for (id, from, to) in [
        (
            "render",
            component.reference.clone(),
            artifact.reference.clone(),
        ),
        ("updates", feed.reference, component.reference.clone()),
    ] {
        mutate(
            &fx,
            WorkGraphMutation::PutRelationship {
                relationship_id: id.into(),
                from,
                to,
                kind: ResourceRelationshipKind::Informs,
            },
        )
        .await;
    }
    pin(&fx, &artifact).await;
    let session = a.session_id;
    fx.host
        .execution
        .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
            crate::artifact_store::delete_artifacts_for_session(&session).unwrap();
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        observe(
            &fx,
            ContentResolveTarget::Reference {
                reference: artifact.reference
            }
        )
        .await["resource"]["resolution"],
        "unavailable"
    );
    assert!(!ready(&fx).await);
    let root = fx.dir.path().canonicalize().unwrap().join("graph");
    fx.host.store = Arc::new(
        fx.host
            .execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(WorkGraphStore::open(&root).unwrap())
            })
            .await
            .unwrap(),
    );
    let graph = fx.host.store.clone();
    let domain = fx.domain.clone();
    let links = fx
        .host
        .execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok(graph
                .query(
                    &domain,
                    WorkGraphQuery {
                        collection: WorkGraphCollection::Relationships,
                        anchor: Some(component.reference),
                        ..Default::default()
                    },
                )
                .unwrap())
        })
        .await
        .unwrap();
    assert_eq!(links.items.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn absent_component_observation_never_creates_default_environment_and_owner_is_frozen() {
    let fx = fixture().await;
    let missing = observe(
        &fx,
        ContentResolveTarget::Component {
            component_id: "dashboard".into(),
        },
    )
    .await;
    assert_eq!(missing["resource"]["resolution"], "unavailable");
    let path = fx.dir.path().canonicalize().unwrap().join("environment");
    let exists = fx
        .host
        .execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok(path.exists())
        })
        .await
        .unwrap();
    assert!(!exists);
    put_component(&fx, vec![]).await;
    let mut foreign = fx.domain.clone();
    foreign.user_id = "user:other".into();
    let other = fx
        .host
        .resolve_content_in(
            &foreign,
            WorkContentResolveInput {
                target: ContentResolveTarget::Component {
                    component_id: "dashboard".into(),
                },
            },
            &fx.environment,
            &fx.feeds,
        )
        .await
        .unwrap();
    assert_eq!(other["resource"]["resolution"], "unavailable");
    assert_ne!(resource(&other).reference, resource(&missing).reference);
    let turn = super::super::tests::turn("user:test", "content-turn");
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
    assert!(
        fx.host
            .resolve_content(
                &read_only,
                WorkContentResolveInput {
                    target: ContentResolveTarget::Feed {
                        feed_id: "digest".into()
                    }
                }
            )
            .await
            .is_err()
    );
    let mut reference = resource(&other).reference;
    reference.authority_id =
        medousa_types::AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
    assert!(
        fx.host
            .resolve_content_in(
                &fx.domain,
                WorkContentResolveInput {
                    target: ContentResolveTarget::Reference { reference }
                },
                &fx.environment,
                &fx.feeds
            )
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_publication_holds_append_custody_until_callback_finishes() {
    let fx = fixture().await;
    fx.feeds
        .append(&fx.domain.user_id, "digest", event("digest", 1))
        .await
        .unwrap();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let feeds = fx.feeds.clone();
    let owner = fx.domain.user_id.clone();
    let observer = tokio::spawn(async move {
        feeds
            .observe_metadata(&owner, "digest", |available, _| async move {
                assert!(available);
                entered_tx.send(()).unwrap();
                release_rx.await.unwrap();
                Ok(())
            })
            .await
            .unwrap();
    });
    entered_rx.await.unwrap();
    let feeds = fx.feeds.clone();
    let owner = fx.domain.user_id.clone();
    let mut writer = tokio::spawn(async move {
        feeds
            .append(&owner, "digest", event("digest", 2))
            .await
            .unwrap()
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), &mut writer)
            .await
            .is_err()
    );
    release_tx.send(()).unwrap();
    observer.await.unwrap();
    writer.await.unwrap();
    assert_eq!(fx.feeds.event_count(&fx.domain.user_id, "digest").await, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn environment_publication_holds_native_put_custody() {
    let fx = fixture().await;
    put_component(&fx, vec![]).await;
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let hub = fx.environment.clone();
    let execution = fx.host.execution.clone();
    let owner = fx.domain.user_id.clone();
    let observer = tokio::spawn(async move {
        hub.observe_component(&owner, "dashboard", &execution, |component| async move {
            assert!(component.is_some());
            entered_tx.send(()).unwrap();
            release_rx.await.unwrap();
            Ok(())
        })
        .await
        .unwrap();
    });
    entered_rx.await.unwrap();
    let hub = fx.environment.clone();
    let execution = fx.host.execution.clone();
    let spec = default_environment_spec(&fx.domain.user_id);
    let handle = tokio::runtime::Handle::current();
    let mut writer = tokio::spawn(async move {
        execution
            .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                Ok(handle.block_on(hub.put(spec, "test")))
            })
            .await
            .unwrap()
            .unwrap()
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), &mut writer)
            .await
            .is_err()
    );
    release_tx.send(()).unwrap();
    observer.await.unwrap();
    writer.await.unwrap();
    assert_eq!(
        observe(
            &fx,
            ContentResolveTarget::Component {
                component_id: "dashboard".into()
            }
        )
        .await["resource"]["resolution"],
        "unavailable"
    );
}

struct FailOnce(std::sync::atomic::AtomicBool);
impl medousa_store::TransactionFaults for FailOnce {
    fn check(
        &self,
        point: medousa_store::TransactionFaultPoint,
    ) -> std::result::Result<(), medousa_store::PersistenceError> {
        if point == medousa_store::TransactionFaultPoint::AfterSnapshotPublish
            && self.0.swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(medousa_store::PersistenceError::new(
                medousa_store::PersistenceErrorKind::RetryableIo,
                "injected content graph interruption",
            ));
        }
        Ok(())
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn interrupted_content_projection_replays_after_restart_without_native_effects() {
    let mut fx = fixture().await;
    fx.feeds
        .append(&fx.domain.user_id, "digest", event("digest", 1))
        .await
        .unwrap();
    let root = fx.dir.path().canonicalize().unwrap().join("graph");
    let open_root = root.clone();
    fx.host.store = Arc::new(
        fx.host
            .execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(WorkGraphStore::with_faults(
                    &open_root,
                    Arc::new(FailOnce(std::sync::atomic::AtomicBool::new(true))),
                )
                .unwrap())
            })
            .await
            .unwrap(),
    );
    let target = || WorkContentResolveInput {
        target: ContentResolveTarget::Feed {
            feed_id: "digest".into(),
        },
    };
    assert!(
        fx.host
            .resolve_content_in(&fx.domain, target(), &fx.environment, &fx.feeds)
            .await
            .is_err()
    );
    fx.host.store = Arc::new(
        fx.host
            .execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(WorkGraphStore::open(&root).unwrap())
            })
            .await
            .unwrap(),
    );
    fx.feeds = FeedStore::new_in(fx.dir.path().canonicalize().unwrap().join("feeds"));
    let recovered = fx
        .host
        .resolve_content_in(&fx.domain, target(), &fx.environment, &fx.feeds)
        .await
        .unwrap();
    assert_eq!(recovered["receipt"]["replayed"], true);
    assert_eq!(recovered["file_effects_replayed"], false);
    assert_eq!(fx.feeds.event_count(&fx.domain.user_id, "digest").await, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_native_feed_identity_fails_without_publishing_unavailability() {
    let fx = fixture().await;
    fx.feeds
        .append(&fx.domain.user_id, "digest", event("foreign-feed", 1))
        .await
        .unwrap();
    assert!(
        fx.host
            .resolve_content_in(
                &fx.domain,
                WorkContentResolveInput {
                    target: ContentResolveTarget::Feed {
                        feed_id: "digest".into()
                    }
                },
                &fx.environment,
                &fx.feeds
            )
            .await
            .is_err()
    );
    let graph = fx.host.store.clone();
    let domain = fx.domain.clone();
    assert_eq!(
        fx.host
            .execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || Ok(
                graph
                    .query(&domain, WorkGraphQuery::default())
                    .unwrap()
                    .revision
            ))
            .await
            .unwrap(),
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn corrupt_or_oversized_component_specs_fail_even_with_a_warm_native_cache() {
    let fx = fixture().await;
    put_component(&fx, vec![]).await;
    let observed = observe(
        &fx,
        ContentResolveTarget::Component {
            component_id: "dashboard".into(),
        },
    )
    .await;
    let r = resource(&observed);
    for bytes in [b"invalid-json".to_vec(), vec![b' '; 4 * 1024 * 1024 + 1]] {
        let root_path = fx.dir.path().canonicalize().unwrap().join("environment");
        let profile =
            medousa_types::authority_id::EnvironmentProfileId::parse(&fx.domain.user_id).unwrap();
        fx.host
            .execution
            .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                let root = crate::store_root::StoreRoot::open_nofollow(&root_path).unwrap();
                let path = crate::store_root::StorePath::parse(&format!(
                    "{}.json",
                    profile.storage_key().as_str()
                ))
                .unwrap();
                root.atomic_write(&path, &bytes).unwrap();
                Ok(())
            })
            .await
            .unwrap();
        assert!(
            fx.host
                .resolve_content_in(
                    &fx.domain,
                    WorkContentResolveInput {
                        target: ContentResolveTarget::Reference {
                            reference: r.reference.clone()
                        }
                    },
                    &fx.environment,
                    &fx.feeds
                )
                .await
                .is_err()
        );
        let graph = fx.host.store.clone();
        let domain = fx.domain.clone();
        let reference = r.reference.clone();
        let page = fx
            .host
            .execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(graph
                    .query(
                        &domain,
                        WorkGraphQuery {
                            anchor: Some(reference),
                            ..Default::default()
                        },
                    )
                    .unwrap())
            })
            .await
            .unwrap();
        assert_eq!(page.revision, observed["graph_revision"].as_u64().unwrap());
    }
}
