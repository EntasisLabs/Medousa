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
        budget: None,
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

fn next(store: &WorkGraphStore, mutation: WorkGraphMutation) -> WorkGraphReceipt {
    let revision = store
        .query(&domain("user:a"), WorkGraphQuery::default())
        .unwrap()
        .revision;
    apply(store, revision, mutation)
}

fn reject(store: &WorkGraphStore, mutation: WorkGraphMutation) {
    let revision = store
        .query(&domain("user:a"), WorkGraphQuery::default())
        .unwrap()
        .revision;
    assert!(
        store
            .apply(
                &domain("user:a"),
                command("rejected", revision, mutation),
                provenance()
            )
            .is_err()
    );
    assert_eq!(
        store
            .query(&domain("user:a"), WorkGraphQuery::default())
            .unwrap()
            .revision,
        revision
    );
}

fn limits(cost: u64, concurrent: u16) -> WorkBudgetLimits {
    WorkBudgetLimits {
        cost_microusd: cost,
        execution_count: 10,
        concurrent_executions: concurrent,
        deadline: chrono::Utc::now() + chrono::Duration::hours(1),
    }
}

fn activate(store: &WorkGraphStore, id: &str) {
    next(
        store,
        WorkGraphMutation::SetState {
            work_unit_id: id.into(),
            state: WorkUnitState::Active,
            reason: "Admitted execution scope".into(),
            evidence: vec![],
        },
    );
}

fn subscription(
    id: &str,
    scope_revision: u64,
    resources: Vec<ResourceRef>,
    after_revision: u64,
) -> WorkGraphMutation {
    WorkGraphMutation::Subscribe {
        input: WorkSubscriptionInput {
            subscription_id: id.into(),
            work_unit_id: "work".into(),
            expected_scope_revision: scope_revision,
            resources,
            event_kinds: vec![
                WorkEventKind::ResourceObserved,
                WorkEventKind::WorkStateChanged,
                WorkEventKind::WorkScopeChanged,
            ],
            after_revision,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        },
    }
}

fn inbox(store: &WorkGraphStore, id: &str, limit: usize) -> WorkEventsPage {
    store
        .events(
            &domain("user:a"),
            "adapter:test",
            WorkEventsQuery {
                subscription_id: id.into(),
                limit: Some(limit),
            },
        )
        .unwrap()
}

#[test]
fn durable_hooks_replay_raced_completion_without_a_chat_and_acknowledge_in_order() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let note = resource(ResourceKind::VaultNote, "note");
    next(
        &store,
        record(note.clone(), "note.md", ResourceResolution::Available),
    );
    let scope = next(
        &store,
        accept(
            "work",
            WorkScope {
                resources: vec![note.clone()],
                ..Default::default()
            },
        ),
    )
    .revision;
    activate(&store, "work"); // Completion/state can precede hook registration.
    next(
        &store,
        subscription("hook", scope, vec![note.clone()], scope),
    );
    let restarted = WorkGraphStore::open(dir.path()).unwrap();
    let first = inbox(&restarted, "hook", 1);
    assert_eq!(first.events[0].receipt.revision, 3);
    assert_eq!(inbox(&restarted, "hook", 1).events, first.events); // Reads never consume.
    next(
        &restarted,
        record(note, "note.md", ResourceResolution::Unavailable),
    );
    reject(
        &restarted,
        WorkGraphMutation::AcknowledgeEvent {
            subscription_id: "hook".into(),
            event_revision: 5,
            decision: "skip first".into(),
        },
    );
    let ack = command(
        "ack",
        5,
        WorkGraphMutation::AcknowledgeEvent {
            subscription_id: "hook".into(),
            event_revision: 3,
            decision: "Observed executor running".into(),
        },
    );
    restarted
        .apply(&domain("user:a"), ack.clone(), provenance())
        .unwrap();
    assert!(
        restarted
            .apply(&domain("user:a"), ack, provenance())
            .unwrap()
            .replayed
    );
    let pending = inbox(&WorkGraphStore::open(dir.path()).unwrap(), "hook", 1);
    assert_eq!(pending.events.len(), 1);
    assert_eq!(pending.events[0].receipt.revision, 5);
    assert!(
        store
            .events(
                &domain("user:a"),
                "other",
                WorkEventsQuery {
                    subscription_id: "hook".into(),
                    limit: None
                }
            )
            .is_err()
    );
    assert!(
        store
            .events(
                &domain("user:b"),
                "adapter:test",
                WorkEventsQuery {
                    subscription_id: "hook".into(),
                    limit: None
                }
            )
            .is_err()
    );
}

#[test]
fn hooks_are_exact_bounded_and_fenced_without_implicit_authority() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let note = resource(ResourceKind::VaultNote, "note");
    let unrelated = resource(ResourceKind::VaultNote, "unrelated");
    next(
        &store,
        record(note.clone(), "note.md", ResourceResolution::Available),
    );
    next(
        &store,
        record(unrelated.clone(), "other.md", ResourceResolution::Available),
    );
    let scope = next(
        &store,
        accept(
            "work",
            WorkScope {
                resources: vec![note.clone()],
                ..Default::default()
            },
        ),
    )
    .revision;
    reject(
        &store,
        subscription("bad", scope, vec![unrelated.clone()], scope),
    );
    reject(
        &store,
        subscription("bad", scope + 1, vec![note.clone()], scope),
    );
    next(
        &store,
        subscription("hook", scope, vec![note.clone()], scope),
    );
    next(
        &store,
        record(unrelated, "other.md", ResourceResolution::Unavailable),
    );
    assert!(inbox(&store, "hook", 32).events.is_empty());
    next(
        &store,
        WorkGraphMutation::SetScope {
            work_unit_id: "work".into(),
            scope: WorkScope::default(),
        },
    );
    assert_eq!(
        inbox(&store, "hook", 1).status,
        WorkSubscriptionStatus::ScopeChanged
    );
    assert_eq!(inbox(&store, "hook", 1).events.len(), 1);
    next(
        &store,
        WorkGraphMutation::StopSubscription {
            subscription_id: "hook".into(),
        },
    );
    next(
        &store,
        record(note, "note.md", ResourceResolution::Unavailable),
    );
    assert_eq!(inbox(&store, "hook", 32).events.len(), 1); // No post-stop intake.
    assert_eq!(
        inbox(&store, "hook", 32).status,
        WorkSubscriptionStatus::Stopped
    );
    assert_eq!(
        store.work_unit(&domain("user:a"), "work").unwrap().state,
        WorkUnitState::Accepted
    );
}

#[test]
fn hook_acknowledgment_publication_fault_keeps_event_or_commits_cursor_atomically() {
    for (point, published) in [
        (TransactionFaultPoint::BeforeRenamePublish, false),
        (TransactionFaultPoint::AfterRenamePublish, true),
    ] {
        let dir = tempdir();
        let store = WorkGraphStore::open(dir.path()).unwrap();
        let note = resource(ResourceKind::VaultNote, "note");
        next(
            &store,
            record(note.clone(), "note.md", ResourceResolution::Available),
        );
        let scope = next(
            &store,
            accept(
                "work",
                WorkScope {
                    resources: vec![note.clone()],
                    ..Default::default()
                },
            ),
        )
        .revision;
        next(
            &store,
            subscription("hook", scope, vec![note.clone()], scope),
        );
        next(
            &store,
            record(note, "note.md", ResourceResolution::Unavailable),
        );
        let faulty = WorkGraphStore::with_faults(
            dir.path(),
            Arc::new(FailOnce {
                point,
                fired: AtomicBool::new(false),
            }),
        )
        .unwrap();
        let ack = command(
            "ack",
            4,
            WorkGraphMutation::AcknowledgeEvent {
                subscription_id: "hook".into(),
                event_revision: 4,
                decision: "Observed unavailable resource".into(),
            },
        );
        assert!(
            faulty
                .apply(&domain("user:a"), ack.clone(), provenance())
                .is_err()
        );
        let reopened = WorkGraphStore::open(dir.path()).unwrap();
        assert_eq!(inbox(&reopened, "hook", 1).events.is_empty(), published);
        assert_eq!(
            reopened
                .apply(&domain("user:a"), ack, provenance())
                .unwrap()
                .replayed,
            published
        );
        assert!(inbox(&reopened, "hook", 1).events.is_empty());
    }
}

fn reserve(id: &str, unit: &str, execution: &str, cost: u64) -> WorkGraphMutation {
    WorkGraphMutation::ReserveBudget {
        reservation_id: id.into(),
        work_unit_id: unit.into(),
        execution: resource(ResourceKind::Assignment, execution),
        reserved_cost_microusd: cost,
    }
}

#[test]
fn aggregate_budget_counts_shared_executions_once_and_keeps_removed_scope_charges() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    next(&store, accept("shared", WorkScope::default()));
    for id in ["left", "right"] {
        next(
            &store,
            accept(
                id,
                WorkScope {
                    children: vec!["shared".into()],
                    ..Default::default()
                },
            ),
        );
    }
    next(
        &store,
        accept(
            "root",
            WorkScope {
                children: vec!["left".into(), "right".into()],
                ..Default::default()
            },
        ),
    );
    for id in ["shared", "left", "right", "root"] {
        next(
            &store,
            WorkGraphMutation::SetBudget {
                work_unit_id: id.into(),
                limits: limits(100, 1),
            },
        );
        activate(&store, id);
    }
    for id in ["execution-a", "execution-b"] {
        next(
            &store,
            record(
                resource(ResourceKind::Assignment, id),
                "assignment",
                ResourceResolution::Available,
            ),
        );
    }
    let receipt = next(&store, reserve("hold-a", "shared", "execution-a", 50));
    let usage = store
        .inspect_work_unit(&domain("user:a"), "root")
        .unwrap()
        .2;
    assert_eq!(usage.cost_microusd, 50); // Both paths share one reservation.
    assert_eq!(usage.concurrent_executions, 1);
    assert_eq!(usage.execution_count, 1);
    reject(&store, reserve("hold-b", "shared", "execution-b", 1));
    reject(
        &store,
        reserve("duplicate-execution", "shared", "execution-a", 1),
    );
    reject(
        &store,
        WorkGraphMutation::SetState {
            work_unit_id: "root".into(),
            state: WorkUnitState::Satisfied,
            reason: "Cannot finish uncertain custody".into(),
            evidence: vec![resource(ResourceKind::Assignment, "execution-a")],
        },
    );
    let settle = next(
        &store,
        WorkGraphMutation::SettleBudget {
            reservation_id: "hold-a".into(),
            disposition: WorkBudgetDisposition::Completed,
            actual_cost_microusd: 40,
        },
    );
    let restarted = WorkGraphStore::open(dir.path()).unwrap();
    assert_eq!(
        restarted
            .inspect_work_unit(&domain("user:a"), "root")
            .unwrap()
            .2
            .cost_microusd,
        40
    );
    let replay = command(
        &receipt.command_id,
        receipt.revision - 1,
        reserve("hold-a", "shared", "execution-a", 50),
    );
    assert!(
        restarted
            .apply(&domain("user:a"), replay, provenance())
            .unwrap()
            .replayed
    );
    next(
        &restarted,
        WorkGraphMutation::SetScope {
            work_unit_id: "root".into(),
            scope: WorkScope::default(),
        },
    );
    assert_eq!(
        restarted
            .inspect_work_unit(&domain("user:a"), "root")
            .unwrap()
            .2
            .cost_microusd,
        40
    );
    next(&restarted, accept("new-parent", WorkScope::default()));
    next(
        &restarted,
        WorkGraphMutation::SetBudget {
            work_unit_id: "new-parent".into(),
            limits: limits(30, 1),
        },
    );
    let attach = WorkGraphMutation::SetScope {
        work_unit_id: "new-parent".into(),
        scope: WorkScope {
            children: vec!["shared".into()],
            ..Default::default()
        },
    };
    reject(&restarted, attach.clone());
    assert!(
        restarted
            .work_unit(&domain("user:a"), "new-parent")
            .unwrap()
            .scope
            .children
            .is_empty()
    );
    next(
        &restarted,
        WorkGraphMutation::SetBudget {
            work_unit_id: "new-parent".into(),
            limits: limits(100, 1),
        },
    );
    next(&restarted, attach);
    assert_eq!(
        restarted
            .inspect_work_unit(&domain("user:a"), "new-parent")
            .unwrap()
            .2
            .cost_microusd,
        40
    );
    reject(&restarted, reserve("hold-b", "shared", "execution-b", 61));
    next(&restarted, reserve("hold-b", "shared", "execution-b", 60));
    assert_eq!(
        restarted
            .inspect_work_unit(&domain("user:a"), "root")
            .unwrap()
            .2
            .cost_microusd,
        40
    );
    next(
        &restarted,
        WorkGraphMutation::SettleBudget {
            reservation_id: "hold-b".into(),
            disposition: WorkBudgetDisposition::Completed,
            actual_cost_microusd: 120,
        },
    );
    let usage = restarted
        .inspect_work_unit(&domain("user:a"), "shared")
        .unwrap()
        .2;
    assert_eq!(usage.cost_microusd, 160); // Preserve an executor's overrun.
    assert_eq!(usage.execution_count, 2);
    assert_eq!(usage.concurrent_executions, 0);
    let original_settle = command(
        &settle.command_id,
        settle.revision - 1,
        WorkGraphMutation::SettleBudget {
            reservation_id: "hold-a".into(),
            disposition: WorkBudgetDisposition::Completed,
            actual_cost_microusd: 40,
        },
    );
    assert!(
        restarted
            .apply(&domain("user:a"), original_settle, provenance())
            .unwrap()
            .replayed
    );
    reject(
        &restarted,
        WorkGraphMutation::SettleBudget {
            reservation_id: "hold-a".into(),
            disposition: WorkBudgetDisposition::NotStarted,
            actual_cost_microusd: 0,
        },
    );
    next(
        &restarted,
        record(
            resource(ResourceKind::Assignment, "execution-c"),
            "assignment",
            ResourceResolution::Available,
        ),
    );
    reject(&restarted, reserve("hold-c", "shared", "execution-c", 0));
    assert_eq!(
        restarted
            .query(
                &domain("user:a"),
                WorkGraphQuery {
                    collection: WorkGraphCollection::BudgetReservations,
                    anchor: Some(resource(ResourceKind::WorkUnit, "root")),
                    ..Default::default()
                }
            )
            .unwrap()
            .items
            .len(),
        1
    );
}

#[test]
fn cancellation_does_not_refund_custody_and_only_native_not_started_settlement_releases_it() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    next(&store, accept("work", WorkScope::default()));
    activate(&store, "work");
    next(
        &store,
        record(
            resource(ResourceKind::Assignment, "execution"),
            "assignment",
            ResourceResolution::Available,
        ),
    );
    reject(&store, reserve("hold", "work", "execution", 1)); // No implicit budget.
    let mut expired = limits(10, 1);
    expired.deadline = chrono::Utc::now() - chrono::Duration::seconds(1);
    next(
        &store,
        WorkGraphMutation::SetBudget {
            work_unit_id: "work".into(),
            limits: expired,
        },
    );
    reject(&store, reserve("hold", "work", "execution", 1));
    next(
        &store,
        WorkGraphMutation::SetBudget {
            work_unit_id: "work".into(),
            limits: limits(10, 1),
        },
    );
    let model = RecordProvenance {
        source: RecordSource::ModelInferred,
        ..provenance()
    };
    let revision = store
        .query(&domain("user:a"), WorkGraphQuery::default())
        .unwrap()
        .revision;
    assert!(
        store
            .apply(
                &domain("user:a"),
                command(
                    "model-hold",
                    revision,
                    reserve("hold", "work", "execution", 10)
                ),
                model.clone()
            )
            .is_err()
    );
    next(&store, reserve("hold", "work", "execution", 10));
    reject(
        &store,
        WorkGraphMutation::SetBudget {
            work_unit_id: "work".into(),
            limits: limits(9, 1),
        },
    );
    next(
        &store,
        WorkGraphMutation::SetState {
            work_unit_id: "work".into(),
            state: WorkUnitState::Cancelled,
            reason: "User withdrew intent".into(),
            evidence: vec![],
        },
    );
    assert_eq!(
        store
            .inspect_work_unit(&domain("user:a"), "work")
            .unwrap()
            .2
            .cost_microusd,
        10
    );
    let release = WorkGraphMutation::SettleBudget {
        reservation_id: "hold".into(),
        disposition: WorkBudgetDisposition::NotStarted,
        actual_cost_microusd: 0,
    };
    let revision = store
        .query(&domain("user:a"), WorkGraphQuery::default())
        .unwrap()
        .revision;
    assert!(
        store
            .apply(
                &domain("user:a"),
                command("model-release", revision, release.clone()),
                model
            )
            .is_err()
    );
    reject(
        &store,
        WorkGraphMutation::SettleBudget {
            reservation_id: "hold".into(),
            disposition: WorkBudgetDisposition::NotStarted,
            actual_cost_microusd: 1,
        },
    );
    next(&store, release);
    assert_eq!(
        store
            .inspect_work_unit(&domain("user:a"), "work")
            .unwrap()
            .2,
        WorkBudgetUsage::default()
    );
    assert_eq!(
        store.work_unit(&domain("user:a"), "work").unwrap().state,
        WorkUnitState::Cancelled
    );
}

#[test]
fn exact_conversations_join_existing_work_and_legacy_commands_still_replay() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let origin = medousa_types::SessionRef {
        authority_id: domain("user:a").authority_id,
        session_id: medousa_types::SessionId::parse("origin-session").unwrap(),
    };
    let mut mutation = accept("work", WorkScope::default());
    if let WorkGraphMutation::AcceptWork { origin: field, .. } = &mut mutation {
        *field = Some(origin.clone());
    }
    let request = command("legacy-accept", 0, mutation);
    store
        .apply(&domain("user:a"), request.clone(), provenance())
        .unwrap();
    // Model an on-disk foundation snapshot; adding fields must not invalidate
    // its immutable command digest or change its accepted identity.
    let path = dir.path().join(
        super::WorkGraphStore::path(&domain("user:a"), "json")
            .unwrap()
            .to_string(),
    );
    let mut snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let unit = snapshot["work_units"]["work"].as_object_mut().unwrap();
    for field in ["scope_revision", "readiness", "conversations", "budget"] {
        unit.remove(field);
    }
    std::fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let restarted = WorkGraphStore::open(dir.path()).unwrap();
    assert!(
        restarted
            .apply(&domain("user:a"), request.clone(), provenance())
            .unwrap()
            .replayed
    );
    let later = medousa_types::SessionRef {
        session_id: medousa_types::SessionId::parse("later-session").unwrap(),
        ..origin.clone()
    };
    let join = command(
        "join",
        1,
        WorkGraphMutation::AttachConversation {
            work_unit_id: "work".into(),
            session: later.clone(),
        },
    );
    restarted
        .apply(&domain("user:a"), join.clone(), provenance())
        .unwrap();
    next(
        &restarted,
        WorkGraphMutation::SetContact {
            work_unit_id: "work".into(),
            contact: WorkContactPreference::Silent,
        },
    );
    assert!(
        restarted
            .apply(&domain("user:a"), join, provenance())
            .unwrap()
            .replayed
    );
    assert!(
        restarted
            .apply(&domain("user:a"), request, provenance())
            .unwrap()
            .replayed
    );
    let unit = restarted.work_unit(&domain("user:a"), "work").unwrap();
    assert_eq!(unit.conversations, vec![origin.clone(), later.clone()]);
    assert_eq!(unit.origin, Some(origin));
    assert_eq!(unit.scope_revision, 1);
    assert_eq!(unit.state, WorkUnitState::Accepted);
    let anchored = restarted
        .query(
            &domain("user:a"),
            WorkGraphQuery {
                collection: WorkGraphCollection::WorkUnits,
                anchor: Some(resource(ResourceKind::Session, later.session_id.as_str())),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(anchored.items.len(), 1);
    let mut foreign = later;
    foreign.authority_id = AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
    reject(
        &restarted,
        WorkGraphMutation::AttachConversation {
            work_unit_id: "work".into(),
            session: foreign,
        },
    );
}

#[test]
fn maintenance_checkpoint_supports_finite_parents_without_ending_shared_work() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let note = resource(ResourceKind::VaultNote, "note");
    let native = |version: &str| WorkGraphMutation::RecordResource {
        reference: note.clone(),
        locator: Some("release.md".into()),
        resolution: ResourceResolution::Available,
        native_revision: Some(version.into()),
    };
    next(&store, native("v1"));
    let scope = WorkScope {
        resources: vec![note.clone()],
        ..Default::default()
    };
    let mut maintenance = accept("docs", scope.clone());
    if let WorkGraphMutation::AcceptWork { kind, .. } = &mut maintenance {
        *kind = WorkUnitKind::Maintenance;
    }
    next(&store, maintenance);
    for id in ["release-a", "release-b"] {
        next(
            &store,
            accept(
                id,
                WorkScope {
                    children: vec!["docs".into()],
                    readiness: vec![WorkReadinessRequirement {
                        work_unit_id: "docs".into(),
                        condition: "Documentation current".into(),
                    }],
                    ..Default::default()
                },
            ),
        );
    }
    let state = |id: &str, state| WorkGraphMutation::SetState {
        work_unit_id: id.into(),
        state,
        reason: "Verified accepted release".into(),
        evidence: vec![note.clone()],
    };
    let ready = |scope_revision, version: &str| WorkGraphMutation::RecordReadiness {
        work_unit_id: "docs".into(),
        expected_scope_revision: scope_revision,
        condition: "Documentation current".into(),
        evidence: vec![WorkRevisionEvidence {
            reference: note.clone(),
            native_revision: version.into(),
        }],
        valid_for_seconds: 3600,
    };
    reject(&store, ready(2, "v1")); // Accepted is not active.
    next(&store, state("docs", WorkUnitState::Active));
    reject(&store, ready(99, "v1"));
    reject(&store, ready(2, "invented"));
    reject(&store, state("release-a", WorkUnitState::Satisfied));
    next(&store, ready(2, "v1"));
    assert!(
        store
            .inspect_work_unit(&domain("user:a"), "docs")
            .unwrap()
            .1
    );
    next(
        &store,
        WorkGraphMutation::SetContact {
            work_unit_id: "docs".into(),
            contact: WorkContactPreference::Silent,
        },
    );
    let restarted = WorkGraphStore::open(dir.path()).unwrap();
    assert!(
        restarted
            .inspect_work_unit(&domain("user:a"), "docs")
            .unwrap()
            .1
    );
    let snapshot = restarted.load(&domain("user:a")).unwrap();
    assert!(
        !snapshot
            .member_ready(
                &snapshot.work_units["release-a"].scope,
                "docs",
                chrono::Utc::now() + chrono::Duration::hours(2)
            )
            .unwrap()
    );
    next(&restarted, state("release-a", WorkUnitState::Satisfied));
    assert_eq!(
        restarted
            .work_unit(&domain("user:a"), "docs")
            .unwrap()
            .state,
        WorkUnitState::Active
    );
    next(&restarted, native("v2"));
    assert!(
        !restarted
            .inspect_work_unit(&domain("user:a"), "docs")
            .unwrap()
            .1
    );
    reject(&restarted, state("release-b", WorkUnitState::Satisfied));
    reject(&restarted, ready(2, "v1"));
    next(&restarted, ready(2, "v2"));
    next(
        &restarted,
        WorkGraphMutation::SetScope {
            work_unit_id: "docs".into(),
            scope,
        },
    );
    let changed = restarted.work_unit(&domain("user:a"), "docs").unwrap();
    assert!(changed.readiness.is_none());
    reject(&restarted, ready(2, "v2"));
    next(&restarted, ready(changed.scope_revision, "v2"));
    next(&restarted, state("docs", WorkUnitState::Paused));
    reject(&restarted, state("release-b", WorkUnitState::Satisfied));
    next(&restarted, state("docs", WorkUnitState::Active));
    assert!(
        restarted
            .work_unit(&domain("user:a"), "docs")
            .unwrap()
            .readiness
            .is_none()
    );
    next(&restarted, ready(changed.scope_revision, "v2"));
    next(&restarted, state("release-b", WorkUnitState::Satisfied));
    assert_eq!(
        restarted
            .work_unit(&domain("user:a"), "docs")
            .unwrap()
            .state,
        WorkUnitState::Active
    );
    assert_eq!(
        restarted
            .work_unit(&domain("user:a"), "release-a")
            .unwrap()
            .state,
        WorkUnitState::Satisfied
    );
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

#[test]
fn budget_publication_faults_and_cost_overflow_preserve_native_custody() {
    for (point, published) in [
        (TransactionFaultPoint::BeforeRenamePublish, false),
        (TransactionFaultPoint::AfterRenamePublish, true),
    ] {
        let dir = tempdir();
        let store = WorkGraphStore::open(dir.path()).unwrap();
        let mut unit = accept("work", WorkScope::default());
        if let WorkGraphMutation::AcceptWork { budget, .. } = &mut unit {
            *budget = Some(limits(u64::MAX, 2));
        }
        next(&store, unit);
        activate(&store, "work");
        for id in ["execution-a", "execution-b", "execution-c"] {
            next(
                &store,
                record(
                    resource(ResourceKind::Assignment, id),
                    "assignment",
                    ResourceResolution::Available,
                ),
            );
        }
        let faulted = WorkGraphStore::with_faults(
            dir.path(),
            Arc::new(FailOnce {
                point,
                fired: AtomicBool::new(false),
            }),
        )
        .unwrap();
        let request = command(
            "reserve-fault",
            5,
            reserve("hold-a", "work", "execution-a", 10),
        );
        assert!(
            faulted
                .apply(&domain("user:a"), request.clone(), provenance())
                .is_err()
        );
        let restarted = WorkGraphStore::open(dir.path()).unwrap();
        assert_eq!(
            restarted
                .inspect_work_unit(&domain("user:a"), "work")
                .unwrap()
                .2
                .cost_microusd,
            if published { 10 } else { 0 }
        );
        assert_eq!(
            restarted
                .apply(&domain("user:a"), request, provenance())
                .unwrap()
                .replayed,
            published
        );
        next(
            &restarted,
            WorkGraphMutation::SettleBudget {
                reservation_id: "hold-a".into(),
                disposition: WorkBudgetDisposition::Completed,
                actual_cost_microusd: u64::MAX,
            },
        );
        next(&restarted, reserve("hold-b", "work", "execution-b", 0));
        next(
            &restarted,
            WorkGraphMutation::SettleBudget {
                reservation_id: "hold-b".into(),
                disposition: WorkBudgetDisposition::Completed,
                actual_cost_microusd: u64::MAX,
            },
        );
        let usage = restarted
            .inspect_work_unit(&domain("user:a"), "work")
            .unwrap()
            .2;
        assert_eq!(usage.cost_microusd, u64::MAX);
        assert!(usage.cost_overflowed);
        reject(&restarted, reserve("hold-c", "work", "execution-c", 0));
        assert_eq!(
            WorkGraphStore::open(dir.path())
                .unwrap()
                .query(
                    &domain("user:a"),
                    query(WorkGraphCollection::BudgetReservations)
                )
                .unwrap()
                .items
                .len(),
            2
        );
    }
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

#[test]
fn native_peer_coordination_rejects_changed_paused_composite_and_unmetered_budget_scope() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let owner = domain("user:a");
    let accepted = next(&store, accept("child", WorkScope::default()));
    assert!(
        store
            .admit_peer_coordination(&owner, "child", accepted.revision)
            .is_ok()
    );
    assert!(
        store
            .admit_peer_coordination(&owner, "child", accepted.revision + 1)
            .is_err()
    );
    next(
        &store,
        WorkGraphMutation::SetState {
            work_unit_id: "child".into(),
            state: WorkUnitState::Paused,
            reason: "pause".into(),
            evidence: vec![],
        },
    );
    assert!(
        store
            .admit_peer_coordination(&owner, "child", accepted.revision)
            .is_err()
    );
    next(
        &store,
        WorkGraphMutation::SetState {
            work_unit_id: "child".into(),
            state: WorkUnitState::Active,
            reason: "resume".into(),
            evidence: vec![],
        },
    );
    let parent = next(
        &store,
        accept(
            "parent",
            WorkScope {
                children: vec!["child".into()],
                ..Default::default()
            },
        ),
    );
    assert!(
        store
            .admit_peer_coordination(&owner, "parent", parent.revision)
            .is_err()
    );
    next(
        &store,
        WorkGraphMutation::SetBudget {
            work_unit_id: "parent".into(),
            limits: WorkBudgetLimits {
                cost_microusd: 100,
                execution_count: 2,
                concurrent_executions: 1,
                deadline: chrono::Utc::now() + chrono::Duration::hours(1),
            },
        },
    );
    assert!(
        store
            .admit_peer_coordination(&owner, "child", accepted.revision)
            .is_err()
    );
}

#[test]
fn native_peer_scope_pin_tracks_resource_versions_without_coupling_contact_or_graph_activity() {
    let dir = tempdir();
    let store = WorkGraphStore::open(dir.path()).unwrap();
    let owner = domain("user:a");
    let reference = resource(ResourceKind::Project, "native-project");
    next(
        &store,
        record(reference.clone(), "project", ResourceResolution::Available),
    );
    let accepted = next(
        &store,
        accept(
            "work",
            WorkScope {
                resources: vec![reference.clone()],
                ..Default::default()
            },
        ),
    );
    let pin = store
        .peer_coordination_scope_digest(&owner, "work")
        .unwrap();
    next(
        &store,
        WorkGraphMutation::SetContact {
            work_unit_id: "work".into(),
            contact: WorkContactPreference::Silent,
        },
    );
    next(&store, accept("unrelated", WorkScope::default()));
    assert_eq!(
        store
            .peer_coordination_scope_digest(&owner, "work")
            .unwrap(),
        pin
    );
    next(
        &store,
        WorkGraphMutation::RecordResource {
            reference,
            locator: Some("project".into()),
            native_revision: Some("changed".into()),
            resolution: ResourceResolution::Available,
        },
    );
    assert_ne!(
        store
            .peer_coordination_scope_digest(&owner, "work")
            .unwrap(),
        pin
    );
    assert_eq!(
        store.work_unit(&owner, "work").unwrap().scope_revision,
        accepted.revision
    );
}
