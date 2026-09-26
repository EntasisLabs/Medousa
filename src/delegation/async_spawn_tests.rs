use std::future::Future;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use stasis::application::runtime::in_memory_runtime::InMemoryRuntime;
use stasis::domain::runtime::job::{BackoffPolicy, JobState, NewJob};
use stasis::ports::outbound::agent::TurnWaitStore;
use stasis::prelude::RuntimeComposition;
use tokio::sync::Notify;

use crate::delegated_task::{
    DelegatedTaskError, DelegatedTaskObservation, DelegatedTaskRequest, DelegatedTaskTransport,
    WorkerParentSpec, WorkerToolRequest,
};
use crate::delegation::{
    AuthorizedDelegationTarget, DELEGATION_JOB_TYPE, DelegationTarget, PendingDelegationSpawn,
    install_delegation_runtime,
};
use crate::delegation_tools::PendingRemoteWorker;
use crate::runtime_composition_ext::{RuntimeCompositionExt, process_once};
use crate::turn_continuation::TurnContinuationScope;
use crate::workshop_api::WorkshopPlacementRequest;
use crate::workshop_contract::{ExecutionTargetCandidate, ExecutionTargetSelection};

use super::tests::{MemorySessionStore, request_for, terminal_observation};

struct BlockingDiscoveryTransport {
    entered: Notify,
    release: Notify,
    returned: Notify,
    submissions: AtomicUsize,
}

#[async_trait::async_trait]
impl DelegatedTaskTransport for BlockingDiscoveryTransport {
    async fn authorized_targets(
        &self,
    ) -> Result<Vec<AuthorizedDelegationTarget>, DelegatedTaskError> {
        self.entered.notify_one();
        self.release.notified().await;
        self.returned.notify_one();
        Ok(vec![authorized_target("remote-daemon", true)])
    }

    async fn submit_or_observe(
        &self,
        _target: &DelegationTarget,
        _request: DelegatedTaskRequest,
    ) -> Result<DelegatedTaskObservation, DelegatedTaskError> {
        self.submissions.fetch_add(1, Ordering::SeqCst);
        Err(DelegatedTaskError::transport(
            "unexpected dispatch while discovery is blocked",
        ))
    }
}

#[tokio::test]
async fn remote_spawn_acknowledges_local_queue_while_target_discovery_is_blocked() {
    let runtime = Arc::new(RuntimeComposition::InMemory(InMemoryRuntime::new()));
    let seed_request = request_for("delegation-job-queue-fast", "delegation-turn-queue-fast");
    let session_store = Arc::new(MemorySessionStore::default());
    session_store.seed_parent_receipt(&seed_request);
    let host = Arc::new(BlockingDiscoveryTransport {
        entered: Notify::new(),
        release: Notify::new(),
        returned: Notify::new(),
        submissions: AtomicUsize::new(0),
    });
    let authority = seed_request.source_execution.authority_id.clone();
    let service = install_delegation_runtime(runtime, authority, session_store, host.clone())
        .expect("install delegation runtime");
    let placement = WorkshopPlacementRequest {
        parent_runtime_id: "runtime-source".into(),
        requested: ExecutionTargetSelection::Exact {
            runtime_id: "remote-daemon".into(),
        },
        agent_selected: false,
        ingress_default: false,
    };
    let entered = host.entered.notified();

    let queued = tokio::time::timeout(
        Duration::from_secs(2),
        with_source_turn_context(service.enqueue_spawn(pending_worker(), placement)),
    )
    .await
    .expect("local admission does not await remote discovery")
    .expect("enqueue spawn");

    assert_eq!(queued["worker_queued"], true);
    assert_eq!(queued["worker_spawned"], false);
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .expect("background driver entered discovery");
    assert_eq!(host.submissions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancelling_queued_spawn_during_discovery_prevents_late_submit() {
    let runtime = Arc::new(RuntimeComposition::InMemory(InMemoryRuntime::new()));
    let seed_request = request_for(
        "delegation-job-cancel-discovery",
        "delegation-turn-cancel-discovery",
    );
    let session_store = Arc::new(MemorySessionStore::default());
    session_store.seed_parent_receipt(&seed_request);
    let host = Arc::new(BlockingDiscoveryTransport {
        entered: Notify::new(),
        release: Notify::new(),
        returned: Notify::new(),
        submissions: AtomicUsize::new(0),
    });
    let service = install_delegation_runtime(
        runtime,
        seed_request.source_execution.authority_id.clone(),
        session_store,
        host.clone(),
    )
    .expect("install delegation runtime");
    let placement = placement_request(false);
    let entered = host.entered.notified();
    let queued = with_source_turn_context(service.enqueue_spawn(pending_worker(), placement))
        .await
        .expect("enqueue spawn");
    let work_id = queued["work_id"].as_str().expect("queued work id");
    let returned = host.returned.notified();
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .expect("background discovery started");

    let cancelled = with_source_turn_context(service.cancel(work_id))
        .await
        .expect("cancel queued spawn");
    assert_eq!(cancelled["status"], "cancelled");
    host.release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), returned)
        .await
        .expect("discovery released");
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let running = service
                .active_drivers
                .lock()
                .expect("drivers")
                .contains(queued["stasis_job_id"].as_str().expect("job id"));
            if !running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancelled discovery driver finishes without dispatch");
    assert_eq!(host.submissions.load(Ordering::SeqCst), 0);
}

struct RetryAfterCheckpointTransport {
    target: DelegationTarget,
    agent_selectable: bool,
    discoveries: AtomicUsize,
    submissions: AtomicUsize,
    submitted_targets: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl DelegatedTaskTransport for RetryAfterCheckpointTransport {
    async fn authorized_targets(
        &self,
    ) -> Result<Vec<AuthorizedDelegationTarget>, DelegatedTaskError> {
        self.discoveries.fetch_add(1, Ordering::SeqCst);
        Ok(vec![AuthorizedDelegationTarget {
            target: self.target.clone(),
            candidate: authorized_target(&self.target.peer_device_id, self.agent_selectable)
                .candidate,
            policy_revision: 7,
        }])
    }

    async fn submit_or_observe(
        &self,
        target: &DelegationTarget,
        request: DelegatedTaskRequest,
    ) -> Result<DelegatedTaskObservation, DelegatedTaskError> {
        self.submitted_targets
            .lock()
            .expect("submitted target lock")
            .push(target.peer_device_id.clone());
        if self.submissions.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(DelegatedTaskError::transport("temporary peer outage"));
        }
        let mut observation = terminal_observation(&request);
        if let Some(worker) = request.worker.as_ref() {
            let digest = crate::delegated_task::worker_spec_digest(worker).expect("worker digest");
            let route = crate::delegated_task::WorkerRouteProvenance {
                provider: worker.parent.provider.clone(),
                model: worker.parent.model.clone(),
                response_depth_mode: worker.parent.response_depth_mode.clone(),
                stage_role: worker.stage_role.clone(),
                model_hint: worker.model_hint.clone(),
            };
            observation.worker_spec_digest = Some(digest.clone());
            observation.worker_route = Some(route.clone());
            let result = observation.result.as_mut().expect("terminal result");
            result.worker_spec_digest = Some(digest);
            result.worker_route = Some(route);
        }
        Ok(observation)
    }
}

#[tokio::test]
async fn process_once_checkpoint_pins_target_across_fresh_handler_resume() {
    let first_runtime = Arc::new(RuntimeComposition::InMemory(InMemoryRuntime::new()));
    let (job, request) = pending_job("delegation-job-checkpoint-reopen");
    let session_store = Arc::new(MemorySessionStore::default());
    session_store.seed_parent_receipt(&request);
    let target = DelegationTarget {
        route_ref: "route-exact".into(),
        peer_device_id: "remote-daemon".into(),
        label: Some("Workshop".into()),
    };
    let first_host = Arc::new(RetryAfterCheckpointTransport {
        target: target.clone(),
        agent_selectable: true,
        discoveries: AtomicUsize::new(0),
        submissions: AtomicUsize::new(0),
        submitted_targets: Mutex::new(Vec::new()),
    });
    install_delegation_runtime(
        first_runtime.clone(),
        request.source_execution.authority_id.clone(),
        session_store.clone(),
        first_host.clone(),
    )
    .expect("install first delegation handler");
    first_runtime
        .enqueue_job(job.clone())
        .await
        .expect("enqueue pending delegation job");

    assert_eq!(
        process_once(first_runtime.as_ref(), "async-spawn-first")
            .await
            .expect("first Stasis pass")
            .as_deref(),
        Some(job.id.as_str())
    );
    let checkpointed_job = first_runtime
        .get_job(&job.id)
        .await
        .expect("load checkpointed job")
        .expect("checkpointed job exists");
    assert!(
        checkpointed_job.progress_json.is_some(),
        "{:?}",
        checkpointed_job.last_error
    );
    assert_eq!(first_host.discoveries.load(Ordering::SeqCst), 1);
    assert_eq!(first_host.submissions.load(Ordering::SeqCst), 1);

    let second_runtime = Arc::new(RuntimeComposition::InMemory(InMemoryRuntime::new()));
    let second_host = Arc::new(RetryAfterCheckpointTransport {
        target,
        agent_selectable: true,
        discoveries: AtomicUsize::new(0),
        // The peer has already processed the first exchange before reconnect.
        submissions: AtomicUsize::new(1),
        submitted_targets: Mutex::new(Vec::new()),
    });
    install_delegation_runtime(
        second_runtime.clone(),
        request.source_execution.authority_id.clone(),
        session_store,
        second_host.clone(),
    )
    .expect("install fresh delegation handler");
    let mut reopened_job = checkpointed_job;
    reopened_job.state = JobState::Enqueued;
    reopened_job.scheduled_at = Utc::now();
    reopened_job.lease_owner = None;
    reopened_job.lease_expires_at = None;
    reopened_job.heartbeat_at = None;
    second_runtime
        .save_job(reopened_job)
        .await
        .expect("restore checkpointed job");

    assert_eq!(
        process_once(second_runtime.as_ref(), "async-spawn-reopened")
            .await
            .expect("resumed Stasis pass")
            .as_deref(),
        Some(job.id.as_str())
    );
    assert_eq!(second_host.discoveries.load(Ordering::SeqCst), 0);
    assert_eq!(second_host.submissions.load(Ordering::SeqCst), 2);
    assert_eq!(
        second_runtime
            .get_job(&job.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        JobState::Succeeded
    );
    assert_eq!(
        *second_host
            .submitted_targets
            .lock()
            .expect("submitted target lock"),
        vec!["remote-daemon"]
    );
}

#[tokio::test]
async fn unauthorized_agent_candidate_is_never_submitted() {
    let runtime = Arc::new(RuntimeComposition::InMemory(InMemoryRuntime::new()));
    let (job, request) = pending_job("delegation-job-denied-agent-target");
    let session_store = Arc::new(MemorySessionStore::default());
    session_store.seed_parent_receipt(&request);
    let host = Arc::new(RetryAfterCheckpointTransport {
        target: DelegationTarget {
            route_ref: "route-denied".into(),
            peer_device_id: "remote-daemon".into(),
            label: None,
        },
        agent_selectable: false,
        discoveries: AtomicUsize::new(0),
        submissions: AtomicUsize::new(0),
        submitted_targets: Mutex::new(Vec::new()),
    });
    install_delegation_runtime(
        runtime.clone(),
        request.source_execution.authority_id.clone(),
        session_store,
        host.clone(),
    )
    .expect("install delegation handler");
    let mut denied_job: PendingDelegationSpawn =
        serde_json::from_str(&job.payload_ref).expect("pending job payload");
    denied_job.placement.agent_selected = true;
    let denied_job = NewJob {
        payload_ref: serde_json::to_string(&denied_job).expect("denied pending payload"),
        ..job
    };
    runtime
        .enqueue_job(denied_job.clone())
        .await
        .expect("enqueue denied job");

    process_once(runtime.as_ref(), "async-spawn-denied")
        .await
        .expect("denied discovery pass");

    assert_eq!(host.discoveries.load(Ordering::SeqCst), 1);
    assert_eq!(host.submissions.load(Ordering::SeqCst), 0);
    let stored = runtime
        .get_job(&denied_job.id)
        .await
        .expect("read denied job")
        .expect("denied job exists");
    assert!(stored.progress_json.is_none());
}

#[tokio::test]
async fn stalled_transport_attempt_is_bounded_by_explicit_task_deadline() {
    let runtime = Arc::new(RuntimeComposition::InMemory(InMemoryRuntime::new()));
    let (mut job, request) = pending_job("delegation-job-stalled-transport");
    let mut pending: PendingDelegationSpawn =
        serde_json::from_str(&job.payload_ref).expect("pending job payload");
    pending.deadline_at = Some(Utc::now() + chrono::Duration::milliseconds(1_500));
    pending.lifetime_policy_version = 1;
    job.payload_ref = serde_json::to_string(&pending).expect("deadline payload");
    let session_store = Arc::new(MemorySessionStore::default());
    session_store.seed_parent_receipt(&request);
    let host = Arc::new(StallingSubmitTransport {
        target: DelegationTarget {
            route_ref: "route-stalled".into(),
            peer_device_id: "remote-daemon".into(),
            label: None,
        },
        submissions: AtomicUsize::new(0),
    });
    install_delegation_runtime(
        runtime.clone(),
        request.source_execution.authority_id.clone(),
        session_store,
        host.clone(),
    )
    .expect("install delegation handler");
    runtime.enqueue_job(job).await.expect("enqueue stalled job");

    tokio::time::timeout(
        Duration::from_secs(4),
        process_once(runtime.as_ref(), "async-spawn-stalled"),
    )
    .await
    .expect("transport call is bounded by spawn deadline")
    .expect("Stasis pass completes as deferred");
    assert_eq!(host.submissions.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn expired_legacy_spawn_deadline_does_not_block_useful_work() {
    let runtime = Arc::new(RuntimeComposition::InMemory(InMemoryRuntime::new()));
    let (mut job, request) = pending_job("delegation-job-expired-before-discovery");
    let mut pending: PendingDelegationSpawn =
        serde_json::from_str(&job.payload_ref).expect("pending job payload");
    pending.deadline_at = Some(Utc::now() - chrono::Duration::seconds(1));
    pending.lifetime_policy_version = 0;
    pending.grant.payload["deadline_at"] =
        serde_json::json!(Utc::now() - chrono::Duration::seconds(1));
    job.payload_ref = serde_json::to_string(&pending).expect("legacy expired payload");
    let session_store = Arc::new(MemorySessionStore::default());
    session_store.seed_parent_receipt(&request);
    let host = Arc::new(RetryAfterCheckpointTransport {
        target: DelegationTarget {
            route_ref: "route-legacy-expired".into(),
            peer_device_id: "remote-daemon".into(),
            label: None,
        },
        agent_selectable: true,
        discoveries: AtomicUsize::new(0),
        submissions: AtomicUsize::new(0),
        submitted_targets: Mutex::new(Vec::new()),
    });
    install_delegation_runtime(
        runtime.clone(),
        request.source_execution.authority_id.clone(),
        session_store,
        host.clone(),
    )
    .expect("install delegation handler");
    runtime
        .enqueue_job(job.clone())
        .await
        .expect("enqueue legacy job");

    process_once(runtime.as_ref(), "async-spawn-expired")
        .await
        .expect("legacy job pass");
    let wait = super::RuntimeDelegationWaitStore::new(runtime.as_ref())
        .get(request.grant.turn_id.as_deref().expect("turn id"))
        .await
        .expect("load legacy wait")
        .expect("legacy wait exists");

    assert_eq!(
        wait.status,
        stasis::domain::agent::turn_wait::TurnWaitStatus::Pending
    );
    assert_eq!(host.discoveries.load(Ordering::SeqCst), 1);
    assert_eq!(host.submissions.load(Ordering::SeqCst), 1);

    tokio::time::sleep(Duration::from_millis(1_100)).await;
    process_once(runtime.as_ref(), "async-spawn-legacy-reconcile")
        .await
        .expect("legacy request resumes from its durable checkpoint");
    let completed = super::RuntimeDelegationWaitStore::new(runtime.as_ref())
        .get(request.grant.turn_id.as_deref().expect("turn id"))
        .await
        .expect("load completed legacy wait")
        .expect("completed legacy wait exists");
    assert_eq!(
        completed.status,
        stasis::domain::agent::turn_wait::TurnWaitStatus::Completed
    );
    assert_eq!(host.submissions.load(Ordering::SeqCst), 2);
}

fn pending_job(job_id: &str) -> (NewJob, DelegatedTaskRequest) {
    let turn_id = format!(
        "delegation-turn-{}",
        job_id.trim_start_matches("delegation-job-")
    );
    let request = request_for(job_id, &turn_id);
    let mut spawn = pending_worker();
    spawn.task = request.grant.payload["user_prompt"]
        .as_str()
        .unwrap()
        .to_string();
    spawn.parent.turn_correlation_id = request.grant.correlation_id.clone();
    let pending = PendingDelegationSpawn {
        work_id: format!("work-{job_id}"),
        spawn,
        placement: placement_request(false),
        grant: request.grant.clone(),
        source_execution: request.source_execution.clone(),
        context: request.context.clone(),
        deadline_at: None,
        lifetime_policy_version: 1,
    };
    (
        NewJob {
            id: job_id.into(),
            queue: "default".into(),
            job_type: DELEGATION_JOB_TYPE.into(),
            payload_ref: serde_json::to_string(&pending).expect("pending payload json"),
            priority: 100,
            max_attempts: 3,
            idempotency_key: format!("delegation:test:{job_id}"),
            correlation_id: request.grant.correlation_id.clone(),
            causation_id: request.grant.causation_id.clone(),
            trace_id: request.grant.correlation_id.clone(),
            input_provenance: None,
            placement: stasis::domain::runtime::placement::PlacementConstraints::unrestricted(),
            scheduled_at: Utc::now(),
            backoff_policy: BackoffPolicy::default(),
        },
        request,
    )
}

fn pending_worker() -> PendingRemoteWorker {
    PendingRemoteWorker {
        schema_version: crate::delegated_task::WORKER_SPAWN_SPEC_SCHEMA_VERSION,
        intent: "research".into(),
        task: "Inspect the sources".into(),
        user_ack: "I will inspect the sources".into(),
        manuscript_ids: Vec::new(),
        manuscript: None,
        stage_role: None,
        model_hint: None,
        parent: WorkerParentSpec {
            stream_turn_id: 0,
            turn_correlation_id: "source-turn-correlation".into(),
            agent_mode: Some("general".into()),
            original_user_prompt: "Compare these claims".into(),
            provider: "provider-a".into(),
            model: "model-a".into(),
            response_depth_mode: "normal".into(),
            code_work_id: None,
            bot: None,
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
        },
        code_project: None,
        code_project_setup: None,
        world_ids: Vec::new(),
        expected_world_runtime_id: None,
        max_tool_rounds: 4,
        tools: WorkerToolRequest {
            names: vec!["cognition_turn".into()],
        },
    }
}

fn placement_request(agent_selected: bool) -> WorkshopPlacementRequest {
    WorkshopPlacementRequest {
        parent_runtime_id: "runtime-source".into(),
        requested: ExecutionTargetSelection::Exact {
            runtime_id: "remote-daemon".into(),
        },
        agent_selected,
        ingress_default: false,
    }
}

fn authorized_target(runtime_id: &str, agent_selectable: bool) -> AuthorizedDelegationTarget {
    let mut candidate = ExecutionTargetCandidate::local(
        runtime_id,
        stasis::domain::runtime::placement::WorkerCapabilities {
            node_id: Some(runtime_id.into()),
            ..Default::default()
        },
    );
    candidate.agent_selectable = agent_selectable;
    AuthorizedDelegationTarget {
        target: DelegationTarget {
            route_ref: format!("route-{runtime_id}"),
            peer_device_id: runtime_id.into(),
            label: None,
        },
        candidate,
        policy_revision: 1,
    }
}

struct StallingSubmitTransport {
    target: DelegationTarget,
    submissions: AtomicUsize,
}

#[async_trait::async_trait]
impl DelegatedTaskTransport for StallingSubmitTransport {
    async fn authorized_targets(
        &self,
    ) -> Result<Vec<AuthorizedDelegationTarget>, DelegatedTaskError> {
        Ok(vec![authorized_target(&self.target.peer_device_id, true)])
    }

    async fn submit_or_observe(
        &self,
        _target: &DelegationTarget,
        _request: DelegatedTaskRequest,
    ) -> Result<DelegatedTaskObservation, DelegatedTaskError> {
        self.submissions.fetch_add(1, Ordering::SeqCst);
        std::future::pending().await
    }
}

async fn with_source_turn_context<F: Future>(future: F) -> F::Output {
    let scope = TurnContinuationScope {
        turn_correlation_id: "source-turn-correlation".into(),
        session_id: "ses_source".into(),
        identity_user_id: Some("profile-source".into()),
        original_prompt: "Compare these claims".into(),
        delivery_target: None,
        provider: "provider-a".into(),
        model: "model-a".into(),
        response_depth_mode: "normal".into(),
        supports_ui_artifacts: false,
        supports_liquid_markdown: false,
        supports_browser_host: false,
        browser_driver_id: None,
        selected_worlds: Vec::new(),
        channel_surface: Some("test".into()),
    };
    let context = crate::agent_runtime::execution_context::TurnExecutionContext::from_scope(
        "source-turn",
        crate::request_principal::RequestPrincipal::continuation("profile-source"),
        tokio_util::sync::CancellationToken::new(),
        std::time::Instant::now() + Duration::from_secs(30),
        scope,
    )
    .expect("source turn context");
    crate::agent_runtime::execution_context::with_turn_execution_context(Arc::new(context), future)
        .await
}
