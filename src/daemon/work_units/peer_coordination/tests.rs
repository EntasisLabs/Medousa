//! The real controller driver, native journal and Forge observations with a
//! fake provider. Even test filesystem/Git work is admitted off Tokio workers.
use super::*;
use crate::daemon::coordination::work::{
    WorkCoordinationPort, WorkCoordinationProgress, advance_work_coordination,
};
use async_trait::async_trait;
use medousa_acp_client::coordination::store::{AssignmentClaim, proposals::proposal_identity};
use medousa_acp_client::coordination::{
    ExternalPeerExecutionPort, PeerAssignmentAuthority, PeerDispatchJournal,
    dispatch_external_peer_assignment,
};
use medousa_forge::{
    git::{CheckpointAuthor, GitEngine},
    model::{ActorKind, ActorRef, WorkspaceMode},
};
use medousa_types::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct TestPort {
    _temp: Arc<tempfile::TempDir>,
    native: Arc<CoordinationStore>,
    host: Arc<WorkUnitHost>,
    starts: Arc<AtomicUsize>,
    uncertain_start: AtomicBool,
    lost_publication: AtomicBool,
    changes_requested: AtomicBool,
}

impl TestPort {
    async fn io<T: Send + 'static>(
        &self,
        f: impl FnOnce(&CoordinationStore, &WorkUnitHost, &Forge) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let native = self.native.clone();
        let host = self.host.clone();
        let forge = host.forge.clone();
        self.host
            .execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(f(&native, &host, &forge))
            })
            .await?
    }

    async fn fixture() -> (Self, WorkCoordinationPlan) {
        let execution = Arc::new(ForgeExecutionService::new());
        let exec = execution.clone();
        execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(Self::fixture_sync(exec, true, ExternalPeerRuntime::Codex))
            })
            .await
            .unwrap()
            .unwrap()
    }

    fn fixture_sync(
        execution: Arc<ForgeExecutionService>,
        approved: bool,
        runtime: ExternalPeerRuntime,
    ) -> Result<(Self, WorkCoordinationPlan)> {
        Self::fixture_sync_controlled(execution, approved, runtime, true)
    }

    fn fixture_sync_controlled(
        execution: Arc<ForgeExecutionService>,
        approved: bool,
        runtime: ExternalPeerRuntime,
        controlled: bool,
    ) -> Result<(Self, WorkCoordinationPlan)> {
        let temp = Arc::new(tempfile::tempdir()?);
        let root = temp.path().canonicalize()?;
        let repo = root.join("repo");
        std::fs::create_dir(&repo)?;
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.name", "test"],
            vec!["config", "user.email", "test@example.com"],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(&repo)
                    .output()?
                    .status
                    .success()
            );
        }
        std::fs::write(repo.join("initial.txt"), "initial")?;
        assert!(
            std::process::Command::new("git")
                .args(["add", "."])
                .current_dir(&repo)
                .output()?
                .status
                .success()
        );
        GitEngine::detect()?.commit_checkpoint(&repo, "initial", &CheckpointAuthor::default())?;
        let forge = Arc::new(Forge::open(root.join("forge"))?);
        let actor = ActorRef {
            kind: ActorKind::User,
            id: "owner".into(),
        };
        let item = forge.register_with_workspace_mode(
            "work",
            "brief",
            &repo,
            "main",
            "owner",
            WorkspaceMode::AttachedCheckout,
            &actor,
        )?;
        let item = forge.provision(&item.id, &actor)?;
        let authority =
            crate::workshop_authority::initialize(&medousa_types::secrets::InstallationId::parse(
                crate::workshop_authority::TEST_INSTALLATION_ID,
            )?)
            .map_err(anyhow::Error::msg)?
            .clone();
        let domain = UserDomainRef {
            authority_id: authority.clone(),
            user_id: "owner".into(),
        };
        let store = Arc::new(WorkGraphStore::open(&root.join("graph"))?);
        store.apply(
            &domain,
            WorkGraphCommand {
                command_id: "accept".into(),
                expected_revision: 0,
                mutation: WorkGraphMutation::AcceptWork {
                    work_unit_id: "unit".into(),
                    intent: "implement and review".into(),
                    kind: WorkUnitKind::Finite,
                    scope: WorkScope::default(),
                    completion_condition: "reviewed implementation".into(),
                    contact: WorkContactPreference::Silent,
                    origin: None,
                    budget: None,
                },
            },
            RecordProvenance {
                actor_id: "owner".into(),
                source: RecordSource::UserDirect,
                evidence: vec![],
            },
        )?;
        let host = Arc::new(WorkUnitHost {
            store,
            forge: forge.clone(),
            execution,
        });
        let native = Arc::new(CoordinationStore::open(&root.join("coordination"))?);
        let channel = CoordinationChannelRef {
            authority_id: authority.clone(),
            channel_id: "channel".into(),
        };
        let session = SessionRef {
            authority_id: authority.clone(),
            session_id: SessionId::parse("ses_owner")?,
        };
        native.create_channel(&CoordinationChannelRecord {
            channel: channel.clone(),
            owner_principal_id: "owner".into(),
            member_principal_ids: vec!["owner".into()],
            attached_sessions: vec![session.clone()],
        })?;
        let expiry = chrono::Utc::now() + chrono::Duration::hours(2);
        let mut ids = vec![];
        for id in ["executor", "reviewer"] {
            let request = ExternalPeerAssignmentRequest {
                assignment_id: id.into(),
                idempotency_key: format!("command-{id}"),
                owner_principal_id: "owner".into(),
                owner_session: session.clone(),
                channel: channel.clone(),
                target: ExternalPeerTarget {
                    authority_id: authority.clone(),
                    execution_runtime_id: "runtime".into(),
                    runtime,
                },
                context: ContextManifest {
                    manifest_id: ContextManifestId::parse(format!(
                        "ctx_{}",
                        if id == "executor" {
                            "a".repeat(32)
                        } else {
                            "b".repeat(32)
                        }
                    ))?,
                    sources: vec![ResolvedConversationRange {
                        selection: ConversationRangeSelection {
                            session: session.clone(),
                            after_entry_seq: None,
                            through_entry_seq: 1,
                        },
                        selection_digest: "test-digest".into(),
                    }],
                    created_by: "owner".into(),
                    created_at: chrono::Utc::now(),
                },
                execution_session: SessionRef {
                    authority_id: authority.clone(),
                    session_id: SessionId::parse(format!("ses_{id}"))?,
                },
                instructions: format!("do {id} {WORK_REVIEW_CONTRACT}"),
                execution_grant_id: format!("grant-{id}"),
                forge_work_id: item.id.to_string(),
                existing_agent_session_id: None,
            };
            let mut proposal = PeerAssignmentProposal {
                proposal_id: String::new(),
                request: request.clone(),
                expires_at: expiry,
                continue_owner: false,
            };
            proposal.proposal_id = proposal_identity(&proposal)?;
            native.record_proposal(&proposal)?;
            if approved {
                native.decide_proposal(
                    &channel,
                    &PeerProposalDecision {
                        proposal_id: proposal.proposal_id.clone(),
                        owner_principal_id: "owner".into(),
                        approved: true,
                    },
                )?;
                native.approve_assignment(&ExternalPeerAssignmentGrant {
                    request,
                    expires_at: expiry,
                })?;
            }
            ids.push(proposal.proposal_id);
        }
        let input = WorkCoordinationInput {
            coordination_id: "coord".into(),
            work_unit_id: "unit".into(),
            expected_scope_revision: 1,
            channel,
            executor_proposal_id: ids[0].clone(),
            reviewer_proposal_id: ids[1].clone(),
            deadline: chrono::Utc::now() + chrono::Duration::hours(1),
        };
        let plan = WorkCoordinationPlan {
            scope_digest: host.peer_scope_digest(&domain, &input)?,
            domain,
            input,
            executor_assignment_id: "executor".into(),
            reviewer_assignment_id: "reviewer".into(),
            forge_work_id: item.id.to_string(),
        };
        if controlled {
            host.register_peer_plan(&native, &forge, &plan)?;
        }
        Ok((
            Self {
                _temp: temp,
                native,
                host,
                starts: Arc::new(AtomicUsize::new(0)),
                uncertain_start: AtomicBool::new(false),
                lost_publication: AtomicBool::new(false),
                changes_requested: AtomicBool::new(false),
            },
            plan,
        ))
    }

    async fn reopen(self) -> Self {
        let temp = self._temp.clone();
        let starts = self.starts.clone();
        let execution = self.host.execution.clone();
        let exec = execution.clone();
        execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                let root = temp.path().canonicalize().unwrap();
                Ok(Self {
                    _temp: temp,
                    starts,
                    host: Arc::new(WorkUnitHost {
                        store: Arc::new(WorkGraphStore::open(&root.join("graph")).unwrap()),
                        forge: Arc::new(Forge::open(root.join("forge")).unwrap()),
                        execution: exec,
                    }),
                    native: Arc::new(CoordinationStore::open(&root.join("coordination")).unwrap()),
                    uncertain_start: AtomicBool::new(false),
                    lost_publication: AtomicBool::new(false),
                    changes_requested: AtomicBool::new(false),
                })
            })
            .await
            .unwrap()
    }

    async fn mutate(&self, plan: &WorkCoordinationPlan, mutation: WorkGraphMutation) {
        let plan = plan.clone();
        self.io(move |_, host, _| {
            let revision = host
                .store
                .query(&plan.domain, WorkGraphQuery::default())?
                .revision;
            host.store.apply(
                &plan.domain,
                WorkGraphCommand {
                    command_id: format!("test-{revision}"),
                    expected_revision: revision,
                    mutation,
                },
                RecordProvenance {
                    actor_id: "owner".into(),
                    source: RecordSource::UserDirect,
                    evidence: vec![],
                },
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }

    async fn state(&self, plan: &WorkCoordinationPlan) -> WorkUnitState {
        let plan = plan.clone();
        self.io(move |_, host, _| {
            Ok(host
                .store
                .work_unit(&plan.domain, &plan.input.work_unit_id)?
                .state)
        })
        .await
        .unwrap()
    }
}

#[async_trait]
impl PeerAssignmentAuthority for TestPort {
    async fn authorize(&self, request: &ExternalPeerAssignmentRequest) -> Result<()> {
        let request = request.clone();
        self.io(move |native, host, forge| {
            native.require_assignment_grant(&request, chrono::Utc::now())?;
            let plan = native.work_plan_for_assignment(&request)?.unwrap();
            host.peer_stage_context(native, forge, &plan, &request)?;
            Ok(())
        })
        .await
    }
}
#[async_trait]
impl PeerDispatchJournal for TestPort {
    async fn claim(&self, request: &ExternalPeerAssignmentRequest) -> Result<AssignmentClaim> {
        let request = request.clone();
        self.io(move |native, host, forge| {
            native.claim_assignment_checked(&request, || {
                let plan = native.work_plan_for_assignment(&request)?.unwrap();
                host.peer_stage_context(native, forge, &plan, &request)?;
                Ok(())
            })
        })
        .await
    }
    async fn binding(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<Option<ExternalPeerAssignmentBinding>> {
        let request = request.clone();
        self.io(move |native, _, _| {
            native.peer_if_recorded(&request.channel, &request.assignment_id)
        })
        .await
    }
    async fn record(&self, binding: &ExternalPeerAssignmentBinding) -> Result<()> {
        let binding = binding.clone();
        self.io(move |native, _, _| native.record_peer(&binding).map(|_| ()))
            .await
    }
}
#[async_trait]
impl ExternalPeerExecutionPort for TestPort {
    async fn discover(&self) -> Result<Vec<ExternalPeerCandidate>> {
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        Ok([ExternalPeerRuntime::Medousa, ExternalPeerRuntime::Codex]
            .into_iter()
            .map(|runtime| ExternalPeerCandidate {
                target: ExternalPeerTarget {
                    authority_id: authority.clone(),
                    execution_runtime_id: "runtime".into(),
                    runtime,
                },
                availability: PeerAvailability::Ready,
            })
            .collect())
    }
    async fn assign(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<ExternalPeerAssignmentBinding> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        if self.uncertain_start.swap(false, Ordering::SeqCst) {
            bail!("lost custody after provider effect");
        }
        let request = request.clone();
        let changes = self.changes_requested.load(Ordering::SeqCst);
        self.io(move |native, host, forge| {
            let plan = native.work_plan_for_assignment(&request)?.unwrap();
            host.peer_stage_context(native, forge, &plan, &request)?;
            let binding = ExternalPeerAssignmentBinding {
                assignment_id: request.assignment_id.clone(),
                owner_principal_id: request.owner_principal_id.clone(),
                channel: request.channel.clone(),
                target: request.target.clone(),
                execution_session: request.execution_session.clone(),
                agent_session_id: format!("native-{}", request.assignment_id),
            };
            native.record_peer(&binding)?;
            let result = if request.assignment_id == plan.executor_assignment_id {
                let item = item(forge, &plan)?;
                let repo = &item.workspace_environment().unwrap().worktree;
                std::fs::write(repo.join("implemented.txt"), "implementation")?;
                assert!(
                    std::process::Command::new("git")
                        .args(["add", "."])
                        .current_dir(repo)
                        .output()?
                        .status
                        .success()
                );
                forge
                    .git()
                    .commit_checkpoint(repo, "implemented", &CheckpointAuthor::default())?;
                "implemented and tested".into()
            } else {
                let input = native.work_review_input(&plan)?.unwrap();
                serde_json::to_string(&WorkReviewDecision {
                    reviewed: input,
                    verdict: if changes {
                        WorkReviewVerdict::ChangesRequested
                    } else {
                        WorkReviewVerdict::Approved
                    },
                    summary: "native revision reviewed".into(),
                })?
            };
            // Complete before assign returns: observation was registered first.
            native.observe_receipt_once(&ExternalPeerAssignmentReceipt {
                receipt_id: peer_terminal_receipt_id(&binding),
                binding: binding.clone(),
                outcome: PeerAssignmentOutcome::Completed,
                result,
            })?;
            Ok(binding)
        })
        .await
    }
}

#[async_trait]
impl WorkCoordinationPort for TestPort {
    async fn ready(&self, plan: &WorkCoordinationPlan, executor: bool) -> Result<bool> {
        let plan = plan.clone();
        self.io(move |native, _, _| {
            let proposal = native.proposal(
                &plan.input.channel,
                if executor {
                    &plan.input.executor_proposal_id
                } else {
                    &plan.input.reviewer_proposal_id
                },
            )?;
            Ok(native
                .proposal_decision(&proposal)?
                .is_some_and(|decision| decision.approved))
        })
        .await
    }
    async fn result(&self, plan: &WorkCoordinationPlan) -> Result<Option<WorkCoordinationResult>> {
        let plan = plan.clone();
        self.io(move |native, _, _| native.work_coordination_result(&plan))
            .await
    }
    async fn admit(&self, plan: &WorkCoordinationPlan) -> Result<()> {
        let plan = plan.clone();
        self.io(move |_, host, forge| host.admit_peer_plan(forge, &plan).map(|_| ()))
            .await
    }
    async fn receipt(
        &self,
        plan: &WorkCoordinationPlan,
        executor: bool,
    ) -> Result<Option<ExternalPeerAssignmentReceipt>> {
        let plan = plan.clone();
        self.io(move |native, _, _| {
            native.receipt_if_recorded(
                &plan.input.channel,
                if executor {
                    &plan.executor_assignment_id
                } else {
                    &plan.reviewer_assignment_id
                },
            )
        })
        .await
    }
    async fn pin(&self, plan: &WorkCoordinationPlan) -> Result<WorkReviewInput> {
        let plan = plan.clone();
        self.io(move |native, host, forge| {
            if let Some(input) = native.work_review_input(&plan)? {
                Ok(input)
            } else {
                host.pin_peer_output(native, forge, &plan)
            }
        })
        .await
    }
    async fn revision_current(
        &self,
        plan: &WorkCoordinationPlan,
        input: &WorkReviewInput,
    ) -> Result<bool> {
        let plan = plan.clone();
        let input = input.clone();
        self.io(move |_, host, forge| host.peer_revision_current(forge, &plan, &input))
            .await
    }
    async fn dispatch_stage(&self, plan: &WorkCoordinationPlan, executor: bool) -> Result<()> {
        let plan = plan.clone();
        let request = self
            .io(move |native, _, _| {
                Ok(native
                    .require_approved_proposal(
                        &plan.input.channel,
                        if executor {
                            &plan.input.executor_proposal_id
                        } else {
                            &plan.input.reviewer_proposal_id
                        },
                        &plan.domain.user_id,
                    )?
                    .request)
            })
            .await?;
        dispatch_external_peer_assignment(self, self, self, &request)
            .await
            .map(|_| ())
    }
    async fn close(
        &self,
        plan: &WorkCoordinationPlan,
        result: WorkCoordinationResult,
    ) -> Result<WorkCoordinationProgress> {
        let plan = plan.clone();
        let lost = self.lost_publication.swap(false, Ordering::SeqCst);
        self.io(move |native, host, forge| {
            native.record_work_coordination_result(&plan, &result)?;
            if lost {
                bail!("crash after native review result before work publication");
            }
            host.project_peer_result(native, forge, &plan, &result)?;
            Ok(WorkCoordinationProgress::Closed)
        })
        .await
    }
}

#[tokio::test]
async fn fast_completion_restart_and_duplicate_wakes_execute_and_review_once() {
    let (port, plan) = TestPort::fixture().await;
    assert!(matches!(
        advance_work_coordination(&port, &plan).await.unwrap(),
        WorkCoordinationProgress::ExecutorRunning
    ));
    let port = port.reopen().await;
    assert!(matches!(
        advance_work_coordination(&port, &plan).await.unwrap(),
        WorkCoordinationProgress::ReviewerRunning
    ));
    port.lost_publication.store(true, Ordering::SeqCst);
    assert!(advance_work_coordination(&port, &plan).await.is_err());
    let port = port.reopen().await;
    for _ in 0..3 {
        assert!(matches!(
            advance_work_coordination(&port, &plan).await.unwrap(),
            WorkCoordinationProgress::Closed
        ));
    }
    assert_eq!(port.starts.load(Ordering::SeqCst), 2);
    assert_eq!(port.state(&plan).await, WorkUnitState::Satisfied);
    let saved = plan.clone();
    port.io(move |native, host, _| {
        assert!(
            native
                .local_work_plans(&saved.domain.authority_id, "runtime", 4, None)?
                .is_empty()
        );
        assert_eq!(
            host.store.work_unit(&saved.domain, "unit")?.contact,
            WorkContactPreference::Silent
        );
        let resources = host.store.query(&saved.domain, WorkGraphQuery::default())?;
        let mut assignment_ids = resources
            .items
            .into_iter()
            .filter_map(|item| match item {
                WorkGraphItem::Resource(record)
                    if record.reference.kind == ResourceKind::Assignment =>
                {
                    Some(record.reference.id)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assignment_ids.sort();
        assert_eq!(
            assignment_ids,
            vec![saved.executor_assignment_id, saved.reviewer_assignment_id]
        );
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn uncertain_launch_survives_restart_without_relaunching_or_starting_review() {
    let (port, plan) = TestPort::fixture().await;
    port.uncertain_start.store(true, Ordering::SeqCst);
    assert!(advance_work_coordination(&port, &plan).await.is_err());
    let port = port.reopen().await;
    for _ in 0..3 {
        assert!(advance_work_coordination(&port, &plan).await.is_err());
    }
    assert_eq!(port.starts.load(Ordering::SeqCst), 1);
    assert_eq!(port.state(&plan).await, WorkUnitState::Accepted);
}

#[tokio::test]
async fn review_changes_request_preserves_work_and_cannot_become_satisfaction_on_replay() {
    let (port, plan) = TestPort::fixture().await;
    port.changes_requested.store(true, Ordering::SeqCst);
    advance_work_coordination(&port, &plan).await.unwrap();
    advance_work_coordination(&port, &plan).await.unwrap();
    advance_work_coordination(&port, &plan).await.unwrap();
    assert_eq!(port.state(&plan).await, WorkUnitState::NeedsAttention);
    let port = port.reopen().await;
    advance_work_coordination(&port, &plan).await.unwrap();
    assert_eq!(port.starts.load(Ordering::SeqCst), 2);
    assert_eq!(port.state(&plan).await, WorkUnitState::NeedsAttention);
}

#[tokio::test]
async fn changing_the_checkout_after_review_does_not_satisfy_work() {
    let (port, plan) = TestPort::fixture().await;
    advance_work_coordination(&port, &plan).await.unwrap();
    advance_work_coordination(&port, &plan).await.unwrap();
    let saved = plan.clone();
    port.io(move |_, _, forge| {
        let item = item(forge, &saved)?;
        let repo = &item.workspace_environment().unwrap().worktree;
        std::fs::write(repo.join("changed.txt"), "changed")?;
        assert!(
            std::process::Command::new("git")
                .args(["add", "."])
                .current_dir(repo)
                .output()?
                .status
                .success()
        );
        forge
            .git()
            .commit_checkpoint(repo, "changed", &CheckpointAuthor::default())?;
        Ok(())
    })
    .await
    .unwrap();
    advance_work_coordination(&port, &plan).await.unwrap();
    assert_eq!(port.state(&plan).await, WorkUnitState::NeedsAttention);
}

#[tokio::test]
async fn paused_cancelled_and_superseded_work_block_even_direct_native_dispatch() {
    for state in [WorkUnitState::Paused, WorkUnitState::Cancelled] {
        let (port, plan) = TestPort::fixture().await;
        port.mutate(
            &plan,
            WorkGraphMutation::SetState {
                work_unit_id: "unit".into(),
                state,
                reason: "user intent".into(),
                evidence: vec![],
            },
        )
        .await;
        assert!(advance_work_coordination(&port, &plan).await.is_err());
        assert!(port.dispatch_stage(&plan, true).await.is_err());
        assert_eq!(port.starts.load(Ordering::SeqCst), 0);
    }
    let (port, plan) = TestPort::fixture().await;
    port.mutate(
        &plan,
        WorkGraphMutation::SetScope {
            work_unit_id: "unit".into(),
            scope: WorkScope::default(),
        },
    )
    .await;
    assert!(port.dispatch_stage(&plan, true).await.is_err());
    assert_eq!(port.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn revoked_review_grant_stops_the_handoff_after_executor_completion() {
    let (port, plan) = TestPort::fixture().await;
    advance_work_coordination(&port, &plan).await.unwrap();
    let saved = plan.clone();
    port.io(move |native, _, _| {
        let request = native
            .proposal(&saved.input.channel, &saved.input.reviewer_proposal_id)?
            .request;
        native.revoke_assignment_grant(&request.channel, &request.execution_grant_id)?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(advance_work_coordination(&port, &plan).await.is_err());
    assert_eq!(port.starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn registration_precedes_approval_and_each_stage_waits_for_its_own_grant() {
    let execution = Arc::new(ForgeExecutionService::new());
    let exec = execution.clone();
    let (port, plan) = execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok(TestPort::fixture_sync(
                exec,
                false,
                ExternalPeerRuntime::Codex,
            ))
        })
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        advance_work_coordination(&port, &plan).await.unwrap(),
        WorkCoordinationProgress::AwaitingApproval
    ));
    assert_eq!(port.starts.load(Ordering::SeqCst), 0);
    for executor in [true, false] {
        let saved = plan.clone();
        port.io(move |native, _, _| {
            let id = if executor {
                &saved.input.executor_proposal_id
            } else {
                &saved.input.reviewer_proposal_id
            };
            let proposal = native.proposal(&saved.input.channel, id)?;
            native.decide_proposal(
                &saved.input.channel,
                &PeerProposalDecision {
                    proposal_id: id.clone(),
                    owner_principal_id: "owner".into(),
                    approved: true,
                },
            )?;
            native.approve_assignment(&ExternalPeerAssignmentGrant {
                request: proposal.request,
                expires_at: proposal.expires_at,
            })?;
            Ok(())
        })
        .await
        .unwrap();
        advance_work_coordination(&port, &plan).await.unwrap();
        if executor {
            assert!(matches!(
                advance_work_coordination(&port, &plan).await.unwrap(),
                WorkCoordinationProgress::AwaitingApproval
            ));
            assert_eq!(port.starts.load(Ordering::SeqCst), 1);
        }
    }
    advance_work_coordination(&port, &plan).await.unwrap();
    assert_eq!(port.state(&plan).await, WorkUnitState::Satisfied);
    assert_eq!(port.starts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn model_satisfaction_cannot_bypass_registered_review_after_rescoping() {
    let (port, plan) = TestPort::fixture().await;
    let saved = plan.clone();
    port.io(move |native, _, _| {
        let mutation = WorkGraphMutation::SetState {
            work_unit_id: "unit".into(),
            state: WorkUnitState::Satisfied,
            reason: "model says done".into(),
            evidence: vec![],
        };
        assert!(validate_peer_qualification(Some(native), &saved.domain, &mutation).is_err());
        assert!(validate_peer_qualification(None, &saved.domain, &mutation).is_err());
        Ok(())
    })
    .await
    .unwrap();
    port.mutate(
        &plan,
        WorkGraphMutation::SetScope {
            work_unit_id: "unit".into(),
            scope: WorkScope::default(),
        },
    )
    .await;
    let saved = plan.clone();
    port.io(move |native, _, _| {
        assert!(native.work_is_controlled(&saved.domain, "unit")?);
        let mutation = WorkGraphMutation::SetState {
            work_unit_id: "unit".into(),
            state: WorkUnitState::Satisfied,
            reason: "new scope".into(),
            evidence: vec![],
        };
        assert!(validate_peer_qualification(Some(native), &saved.domain, &mutation).is_err());
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn native_medousa_executor_and_reviewer_follow_the_same_durable_controller() {
    let execution = Arc::new(ForgeExecutionService::new());
    let exec = execution.clone();
    let (port, plan) = execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok(TestPort::fixture_sync(
                exec,
                true,
                ExternalPeerRuntime::Medousa,
            ))
        })
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        advance_work_coordination(&port, &plan).await.unwrap(),
        WorkCoordinationProgress::ExecutorRunning
    ));
    let port = port.reopen().await;
    assert!(matches!(
        advance_work_coordination(&port, &plan).await.unwrap(),
        WorkCoordinationProgress::ReviewerRunning
    ));
    assert!(matches!(
        advance_work_coordination(&port, &plan).await.unwrap(),
        WorkCoordinationProgress::Closed
    ));
    assert_eq!(port.starts.load(Ordering::SeqCst), 2);
    assert_eq!(port.state(&plan).await, WorkUnitState::Satisfied);
}

#[tokio::test]
async fn provider_review_requires_exact_completed_source_visibility_and_current_checkout() {
    use crate::daemon::work_units::provider_events::{
        review_revision, source_request_with_visibility,
    };
    use medousa_types::work_provider::WorkProviderReviewSource;
    let (port, plan) = TestPort::fixture().await;
    let copy = plan.clone();
    port.io(move |native, _, _| {
        let source = WorkProviderReviewSource {
            channel: copy.input.channel.clone(),
            executor_assignment_id: copy.executor_assignment_id.clone(),
        };
        assert!(
            source_request_with_visibility(native, &copy.domain, &source, |_, _| true).is_err()
        );
        Ok(())
    })
    .await
    .unwrap();
    advance_work_coordination(&port, &plan).await.unwrap();
    let pin = port.pin(&plan).await.unwrap();
    let copy = plan.clone();
    port.io(move |native, _, forge| {
        let source = WorkProviderReviewSource {
            channel: copy.input.channel.clone(),
            executor_assignment_id: copy.executor_assignment_id.clone(),
        };
        assert!(source_request_with_visibility(native, &copy.domain, &source, |_, _| true).is_ok());
        assert!(
            source_request_with_visibility(native, &copy.domain, &source, |_, _| false).is_err()
        );
        let mut foreign = copy.domain.clone();
        foreign.user_id = "other".into();
        assert!(source_request_with_visibility(native, &foreign, &source, |_, _| true).is_err());
        let mut missing = source.clone();
        missing.executor_assignment_id = "unknown".into();
        assert!(
            source_request_with_visibility(native, &copy.domain, &missing, |_, _| true).is_err()
        );
        assert!(review_revision(forge, &copy.domain.user_id, &pin)?);
        assert!(!review_revision(forge, "other", &pin)?);
        let item = item(forge, &copy)?;
        let root = &item.workspace_environment().unwrap().worktree;
        std::fs::write(root.join("changed.txt"), "changed after pin")?;
        assert!(!review_revision(forge, &copy.domain.user_id, &pin)?);
        assert!(
            std::process::Command::new("git")
                .args(["add", "."])
                .current_dir(root)
                .output()?
                .status
                .success()
        );
        forge
            .git()
            .commit_checkpoint(root, "changed", &CheckpointAuthor::default())?;
        assert!(!review_revision(forge, &copy.domain.user_id, &pin)?);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn provider_stage_controller_recovers_exact_review_without_an_open_source_chat() {
    use medousa_types::work_provider::*;
    for scenario in [
        "approved",
        "changes",
        "invalid",
        "dirty",
        "invisible",
        "scope",
        "paused",
        "busy",
    ] {
        let execution = Arc::new(ForgeExecutionService::new());
        let exec = execution.clone();
        let (port, plan) = execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(TestPort::fixture_sync_controlled(
                    exec,
                    true,
                    ExternalPeerRuntime::Codex,
                    false,
                ))
            })
            .await
            .unwrap()
            .unwrap();
        let copy = plan.clone();
        port.io(move |native, host, forge| {
            let request = native
                .proposal(&copy.input.channel, &copy.input.executor_proposal_id)?
                .request;
            native.claim_assignment_checked(&request, || Ok(()))?;
            let binding = ExternalPeerAssignmentBinding {
                assignment_id: request.assignment_id.clone(),
                owner_principal_id: request.owner_principal_id.clone(),
                channel: request.channel.clone(),
                target: request.target.clone(),
                execution_session: request.execution_session.clone(),
                agent_session_id: "native-executor".into(),
            };
            native.record_peer(&binding)?;
            let receipt_id = peer_terminal_receipt_id(&binding);
            native.observe_receipt_once(&ExternalPeerAssignmentReceipt {
                receipt_id: receipt_id.clone(),
                binding,
                outcome: PeerAssignmentOutcome::Completed,
                result: "implemented and tested".into(),
            })?;
            let pin = checkout(forge, &copy, receipt_id)?;
            let request = WorkProviderRequest {
                conversation_id: "muse".into(),
                request_id: "review-request".into(),
                provider: ExternalProvider::Muse,
                input: WorkProviderRequestInput {
                    work_unit_id: copy.input.work_unit_id.clone(),
                    expected_scope_revision: copy.input.expected_scope_revision,
                    deadline: copy.input.deadline,
                    review_of: Some(WorkProviderReviewSource {
                        channel: copy.input.channel.clone(),
                        executor_assignment_id: copy.executor_assignment_id.clone(),
                    }),
                },
                instruction_digest: "a".repeat(64),
                scope_digest: copy.scope_digest.clone(),
                completion_condition: "reviewed implementation".into(),
                reviewed: Some(pin.clone()),
                predecessor: None,
            };
            let proof = RecordProvenance {
                actor_id: "external-agent:test".into(),
                source: RecordSource::SystemEvent,
                evidence: vec![],
            };
            host.store.apply_native_command(
                &copy.domain,
                "provider-request".into(),
                WorkGraphMutation::RegisterProviderRequest {
                    request: Box::new(request),
                },
                proof.clone(),
            )?;
            host.store.apply_native_command(
                &copy.domain,
                "provider-claim".into(),
                WorkGraphMutation::ClaimProviderRequest {
                    conversation_id: "muse".into(),
                    request_id: "review-request".into(),
                },
                proof.clone(),
            )?;
            let decision = WorkReviewDecision {
                reviewed: pin,
                verdict: if scenario == "changes" {
                    WorkReviewVerdict::ChangesRequested
                } else {
                    WorkReviewVerdict::Approved
                },
                summary: "exact native revision reviewed".into(),
            };
            let event = WorkProviderEvent {
                conversation_id: "muse".into(),
                request_id: "review-request".into(),
                event_id: "provider:done".into(),
                actor_id: proof.actor_id.clone(),
                request_sequence: 1,
                kind: ExternalEventKind::Completed,
                text: if scenario == "invalid" {
                    "looks fine".into()
                } else {
                    serde_json::to_string(&decision)?
                },
                created_at: chrono::Utc::now(),
                qualification: if scenario == "invalid" {
                    WorkProviderQualification::InvalidReview
                } else if scenario == "changes" {
                    WorkProviderQualification::ChangesRequested
                } else {
                    WorkProviderQualification::ReviewApproved
                },
                review_decision: (scenario != "invalid").then_some(decision),
            };
            host.store.apply_native_command(
                &copy.domain,
                "provider-result".into(),
                WorkGraphMutation::RecordProviderEvent {
                    event: Box::new(event),
                },
                proof,
            )?;
            if scenario == "dirty" {
                std::fs::write(
                    item(forge, &copy)?
                        .workspace_environment()
                        .unwrap()
                        .worktree
                        .join("changed.txt"),
                    "changed after callback",
                )?;
            }
            if scenario == "scope" || scenario == "paused" {
                let revision = host
                    .store
                    .query(&copy.domain, WorkGraphQuery::default())?
                    .revision;
                host.store.apply(
                    &copy.domain,
                    WorkGraphCommand {
                        command_id: "steering".into(),
                        expected_revision: revision,
                        mutation: if scenario == "scope" {
                            WorkGraphMutation::SetScope {
                                work_unit_id: copy.input.work_unit_id.clone(),
                                scope: WorkScope::default(),
                            }
                        } else {
                            WorkGraphMutation::SetState {
                                work_unit_id: copy.input.work_unit_id.clone(),
                                state: WorkUnitState::Paused,
                                reason: "user paused work".into(),
                                evidence: vec![],
                            }
                        },
                    },
                    RecordProvenance {
                        actor_id: "owner".into(),
                        source: RecordSource::UserDirect,
                        evidence: vec![],
                    },
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
        // Reopen both native ledgers and Forge. No source-chat turn exists.
        let port = port.reopen().await;
        let copy = plan.clone();
        port.io(move |native, host, forge| {
            let inbox = host
                .store
                .coordinator_inboxes(&copy.domain.authority_id, 4, None)?
                .inboxes
                .remove(0);
            if scenario == "busy" {
                let _lease = forge
                    .store()
                    .try_lock_item(&WorkId::parse_storage(&copy.forge_work_id).unwrap())?
                    .unwrap();
                assert!(
                    host.consume_provider_inbox_with(&inbox, Some(native), |_, _| true)
                        .is_err()
                );
                assert_eq!(
                    host.store
                        .work_unit(&copy.domain, &copy.input.work_unit_id)?
                        .state,
                    WorkUnitState::Accepted
                );
                assert_eq!(
                    host.store
                        .coordinator_inboxes(&copy.domain.authority_id, 4, None)?
                        .inboxes
                        .len(),
                    1
                );
            }
            host.consume_provider_inbox_with(&inbox, Some(native), |_, _| scenario != "invisible")?;
            if scenario == "paused" {
                assert_eq!(
                    host.store
                        .work_unit(&copy.domain, &copy.input.work_unit_id)?
                        .state,
                    WorkUnitState::Paused
                );
                assert_eq!(
                    host.store
                        .coordinator_inboxes(&copy.domain.authority_id, 4, None)?
                        .inboxes
                        .len(),
                    1
                );
                let revision = host
                    .store
                    .query(&copy.domain, WorkGraphQuery::default())?
                    .revision;
                host.store.apply(
                    &copy.domain,
                    WorkGraphCommand {
                        command_id: "resume".into(),
                        expected_revision: revision,
                        mutation: WorkGraphMutation::SetState {
                            work_unit_id: copy.input.work_unit_id.clone(),
                            state: WorkUnitState::Active,
                            reason: "resume".into(),
                            evidence: vec![],
                        },
                    },
                    RecordProvenance {
                        actor_id: "owner".into(),
                        source: RecordSource::UserDirect,
                        evidence: vec![],
                    },
                )?;
                host.consume_provider_inbox_with(&inbox, Some(native), |_, _| true)?;
            }
            let expected = match scenario {
                "approved" | "paused" | "busy" => WorkUnitState::Satisfied,
                "scope" => WorkUnitState::Accepted,
                _ => WorkUnitState::NeedsAttention,
            };
            let unit = host
                .store
                .work_unit(&copy.domain, &copy.input.work_unit_id)?;
            assert_eq!(unit.state, expected, "{scenario}");
            assert_eq!(unit.contact, WorkContactPreference::Silent);
            assert!(unit.origin.is_none());
            assert!(
                host.store
                    .coordinator_inboxes(&copy.domain.authority_id, 4, None)?
                    .inboxes
                    .is_empty()
            );
            let revision = host
                .store
                .query(&copy.domain, WorkGraphQuery::default())?
                .revision;
            host.consume_provider_inbox_with(&inbox, Some(native), |_, _| true)?;
            assert_eq!(
                host.store
                    .query(&copy.domain, WorkGraphQuery::default())?
                    .revision,
                revision
            );
            assert!(!native.work_is_controlled(&copy.domain, &copy.input.work_unit_id)?);
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(port.starts.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn provider_handoff_waits_for_exact_executor_and_never_replaces_claimed_send() {
    use medousa_types::work_provider::*;
    for scenario in [
        "completed",
        "failed",
        "paused",
        "cancelled",
        "scope",
        "invisible",
        "unapproved",
        "controlled",
    ] {
        let execution = Arc::new(ForgeExecutionService::new());
        let exec = execution.clone();
        let (port, plan) = execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(TestPort::fixture_sync_controlled(
                    exec,
                    scenario != "unapproved",
                    ExternalPeerRuntime::Codex,
                    scenario == "controlled",
                ))
            })
            .await
            .unwrap()
            .unwrap();
        let copy = plan.clone();
        let dispatch = port
            .io(move |native, host, _| {
                let mut dispatch = WorkProviderDispatch {
                    conversation_id: "provider-chat".into(),
                    request_id: "review-after-executor".into(),
                    provider: ExternalProvider::Muse,
                    input: WorkProviderRequestInput {
                        work_unit_id: copy.input.work_unit_id.clone(),
                        expected_scope_revision: copy.input.expected_scope_revision,
                        deadline: copy.input.deadline,
                        review_of: Some(WorkProviderReviewSource {
                            channel: copy.input.channel.clone(),
                            executor_assignment_id: copy.executor_assignment_id.clone(),
                        }),
                    },
                    instructions: "Review the exact completed native executor revision".into(),
                    target_digest: "a".repeat(64),
                    scope_digest: String::new(),
                    source_request_digest: String::new(),
                    coordinator_wake: false,
                    after_provider_completion: None,
                };
                let admission = host.admit_provider_dispatch_with(
                    native,
                    &copy.domain,
                    &mut dispatch,
                    |_, _| true,
                );
                if matches!(scenario, "unapproved" | "controlled") {
                    assert!(admission.is_err());
                    assert!(
                        host.store
                            .provider_dispatch(
                                &copy.domain,
                                &dispatch.conversation_id,
                                &dispatch.request_id
                            )?
                            .is_none()
                    );
                    return Ok(None);
                }
                admission?;
                assert!(!host.provider_dispatch_ready_with(
                    native,
                    &copy.domain,
                    &dispatch,
                    |_, _| true
                )?);
                assert!(
                    host.store
                        .require_provider_idle(&copy.domain, &copy.input.work_unit_id)
                        .is_err()
                );
                let request = native
                    .proposal(&copy.input.channel, &copy.input.executor_proposal_id)?
                    .request;
                native.claim_assignment_checked(&request, || Ok(()))?;
                let binding = ExternalPeerAssignmentBinding {
                    assignment_id: request.assignment_id.clone(),
                    owner_principal_id: request.owner_principal_id.clone(),
                    channel: request.channel.clone(),
                    target: request.target.clone(),
                    execution_session: request.execution_session.clone(),
                    agent_session_id: "native-executor".into(),
                };
                native.record_peer(&binding)?;
                native.observe_receipt_once(&ExternalPeerAssignmentReceipt {
                    receipt_id: peer_terminal_receipt_id(&binding),
                    binding,
                    outcome: if scenario == "failed" {
                        PeerAssignmentOutcome::Failed
                    } else {
                        PeerAssignmentOutcome::Completed
                    },
                    result: "native terminal".into(),
                })?;
                if matches!(scenario, "paused" | "cancelled") {
                    host.store.apply_native_command(
                        &copy.domain,
                        "state-change".into(),
                        WorkGraphMutation::SetState {
                            work_unit_id: copy.input.work_unit_id.clone(),
                            state: if scenario == "paused" {
                                WorkUnitState::Paused
                            } else {
                                WorkUnitState::Cancelled
                            },
                            reason: "user changed state".into(),
                            evidence: vec![],
                        },
                        provider_handoff_provenance(),
                    )?;
                } else if scenario == "scope" {
                    host.store.apply_native_command(
                        &copy.domain,
                        "scope-change".into(),
                        WorkGraphMutation::SetScope {
                            work_unit_id: copy.input.work_unit_id.clone(),
                            scope: WorkScope::default(),
                        },
                        provider_handoff_provenance(),
                    )?;
                }
                Ok(Some(dispatch))
            })
            .await
            .unwrap();
        let Some(dispatch) = dispatch else {
            continue;
        };
        let port = port.reopen().await;
        let copy = plan.clone();
        let temp = port._temp.clone();
        port.io(move |native, host, forge| {
            let ready =
                host.provider_dispatch_ready_with(native, &copy.domain, &dispatch, |_, _| {
                    scenario != "invisible"
                });
            if scenario == "invisible" {
                assert!(ready.is_err());
                return Ok(());
            }
            assert_eq!(ready?, scenario == "completed");
            if scenario == "paused" {
                assert!(
                    host.store
                        .provider_dispatch(
                            &copy.domain,
                            &dispatch.conversation_id,
                            &dispatch.request_id
                        )?
                        .unwrap()
                        .closed_reason
                        .is_none()
                );
            } else if scenario != "completed" {
                assert!(
                    host.store
                        .provider_dispatch(
                            &copy.domain,
                            &dispatch.conversation_id,
                            &dispatch.request_id
                        )?
                        .unwrap()
                        .closed_reason
                        .is_some()
                );
            } else {
                // Real Forge/Git derives the pin only after the saved executor
                // receipt; its durable claim owns the send across restart.
                let receipt = native.receipt(&copy.input.channel, &copy.executor_assignment_id)?;
                let pin = checkout(forge, &copy, receipt.receipt_id)?;
                let request = WorkProviderRequest {
                    conversation_id: dispatch.conversation_id.clone(),
                    request_id: dispatch.request_id.clone(),
                    provider: dispatch.provider,
                    input: dispatch.input.clone(),
                    instruction_digest: format!(
                        "{:x}",
                        sha2::Sha256::digest(dispatch.instructions.as_bytes())
                    ),
                    scope_digest: dispatch.scope_digest.clone(),
                    completion_condition: host
                        .store
                        .work_unit(&copy.domain, &copy.input.work_unit_id)?
                        .completion_condition,
                    reviewed: Some(pin),
                    predecessor: None,
                };
                host.store.apply_native_command(
                    &copy.domain,
                    "prepare-review".into(),
                    WorkGraphMutation::RegisterProviderRequest {
                        request: Box::new(request),
                    },
                    provider_handoff_provenance(),
                )?;
                host.store.apply_native_command(
                    &copy.domain,
                    "claim-review".into(),
                    WorkGraphMutation::ClaimProviderRequest {
                        conversation_id: dispatch.conversation_id.clone(),
                        request_id: dispatch.request_id.clone(),
                    },
                    provider_handoff_provenance(),
                )?;
                assert!(!host.provider_dispatch_ready_with(
                    native,
                    &copy.domain,
                    &dispatch,
                    |_, _| true
                )?);
                let reopened = WorkGraphStore::open(&temp.path().canonicalize()?.join("graph"))?;
                assert!(
                    reopened
                        .provider_request(
                            &copy.domain,
                            &dispatch.conversation_id,
                            &dispatch.request_id
                        )?
                        .unwrap()
                        .dispatch_claimed
                );
                assert!(
                    reopened
                        .coordinator_inboxes(&copy.domain.authority_id, 2, None)?
                        .dispatches
                        .is_empty()
                );
            }
            assert!(
                host.store
                    .work_unit(&copy.domain, &copy.input.work_unit_id)?
                    .origin
                    .is_none()
            );
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(port.starts.load(Ordering::SeqCst), 0);
    }
}

fn provider_handoff_provenance() -> RecordProvenance {
    RecordProvenance {
        actor_id: "adapter:provider-work".into(),
        source: RecordSource::SystemEvent,
        evidence: vec![],
    }
}
