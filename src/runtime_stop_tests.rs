use super::*;
use serde_json::json;

fn record(id: &str, session: &str, owner: &str) -> TurnWorkRecord {
    serde_json::from_value(json!({
        "work_id": id, "session_id": session, "identity_user_id": owner,
        "intent": "research", "task_prompt": "test", "status": "running",
        "tool_names": [], "user_ack": "", "provider": "test", "model": "test",
        "response_depth_mode": "normal", "created_at": Utc::now(), "updated_at": Utc::now(),
        "stasis_job_id": format!("job-{id}")
    }))
    .unwrap()
}

fn request(scope: StopScope) -> RuntimeStop {
    RuntimeStop {
        request_key: "test-stop".into(),
        scope,
        project_id: (scope == StopScope::Project).then(|| "project-a".into()),
    }
}

#[test]
fn batch_stop_scope_and_owner_boundaries() {
    let mut own = record("own", "chat-a", "owner-a");
    own.parent_code_work_id = Some("project-a".into());
    let mut other_chat = own.clone();
    other_chat.session_id = "chat-b".into();
    let foreign = record("foreign", "chat-a", "owner-b");
    assert!(matches_worker(
        &request(StopScope::Chat),
        &own,
        "chat-a",
        "owner-a",
        false
    ));
    assert!(!matches_worker(
        &request(StopScope::Chat),
        &other_chat,
        "chat-a",
        "owner-a",
        false
    ));
    assert!(!matches_worker(
        &request(StopScope::Chat),
        &foreign,
        "chat-a",
        "owner-a",
        false
    ));
    assert!(matches_worker(
        &request(StopScope::Chat),
        &foreign,
        "chat-a",
        "owner-a",
        true
    ));
    assert!(matches_worker(
        &request(StopScope::Project),
        &other_chat,
        "chat-a",
        "owner-a",
        true
    ));
    assert!(!matches_worker(
        &request(StopScope::Project),
        &foreign,
        "chat-a",
        "owner-a",
        true
    ));
    assert!(matches_worker(
        &request(StopScope::Workshop),
        &foreign,
        "chat-a",
        "owner-a",
        true
    ));
}

#[test]
fn batch_stop_excludes_caller_not_its_children() {
    let caller = record("caller", "chat-a", "owner-a");
    for turn in [
        "caller",
        "work-caller",
        "caller-synthesis",
        "caller-host-resume",
    ] {
        assert!(excludes_caller(&caller, turn));
    }
    let mut child = record("child", "chat-a", "owner-a");
    child.parent_turn_correlation_id = Some("caller".into());
    let (workers, excluded) = select_workers(
        vec![caller, child],
        &request(StopScope::Chat),
        "chat-a",
        "owner-a",
        true,
        "caller",
    );
    assert_eq!(
        workers
            .iter()
            .map(|w| w.work_id.as_str())
            .collect::<Vec<_>>(),
        ["child"]
    );
    assert_eq!(excluded, BTreeSet::from(["job-caller".into()]));
}

#[test]
fn batch_stop_host_resume_excludes_only_its_exact_cohort() {
    let mut primary = record("primary", "chat-a", "owner-a");
    primary.parent_stream_turn_id = 1;
    let mut sibling = record("sibling", "chat-a", "owner-a");
    sibling.parent_stream_turn_id = 1;
    let mut unrelated = record("unrelated", "chat-a", "owner-a");
    unrelated.parent_stream_turn_id = 2;
    // Legacy records may all lack parent correlation; stream still separates them.
    let (workers, excluded) = select_workers(
        vec![primary, sibling, unrelated],
        &request(StopScope::Chat),
        "chat-a",
        "owner-a",
        true,
        "primary-host-resume",
    );
    assert_eq!(
        workers
            .iter()
            .map(|w| w.work_id.as_str())
            .collect::<Vec<_>>(),
        ["unrelated"]
    );
    assert_eq!(excluded.len(), 2);
}

#[test]
fn batch_stop_includes_undelivered_results_not_delivered_history() {
    let mut pending_intake = record("intake", "chat-a", "owner-a");
    pending_intake.status = TurnWorkStatus::Completed;
    let mut delivered = pending_intake.clone();
    delivered.work_id = "delivered".into();
    delivered.synthesis_delivered = true;
    let mut archived = pending_intake.clone();
    archived.work_id = "archived".into();
    archived.archived = true;
    let (workers, _) = select_workers(
        vec![pending_intake, delivered, archived],
        &request(StopScope::Chat),
        "chat-a",
        "owner-a",
        true,
        "interactive",
    );
    assert_eq!(
        workers
            .iter()
            .map(|w| w.work_id.as_str())
            .collect::<Vec<_>>(),
        ["intake"]
    );
}

#[test]
fn batch_stop_ledger_roundtrip_freezes_membership() {
    let directory = tempfile::tempdir().unwrap();
    let root = Arc::new(
        StoreRoot::open_or_create_nofollow(&directory.path().canonicalize().unwrap()).unwrap(),
    );
    let path = StorePath::parse("batch.json").unwrap();
    let input = request(StopScope::Chat);
    let (workers, _) = select_workers(
        vec![record("first", "chat-a", "owner-a")],
        &input,
        "chat-a",
        "owner-a",
        true,
        "interactive",
    );
    let batch = StopBatch {
        input,
        owner: "owner-a".into(),
        session_id: "chat-a".into(),
        created_at: Utc::now(),
        workers,
        jobs: vec!["job-first".into()],
    };
    crate::persistence::FileTransaction::new(root.clone())
        .replace_snapshot_json(&path, &batch)
        .unwrap();
    let recovered: StopBatch = serde_json::from_slice(&root.read(&path).unwrap()).unwrap();
    assert_eq!(recovered.input, batch.input);
    assert_eq!(recovered.workers.len(), 1);
    assert_eq!(recovered.workers[0].work_id, "first");
    assert_eq!(recovered.jobs, ["job-first"]);
    assert_eq!(recovered.created_at, batch.created_at);
}

#[test]
fn batch_stop_is_advertised_typed_and_rejects_authority_overrides() {
    use super::super::{RuntimeMutateAction, runtime_type_schemas};
    let input = json!({"action":"runtime.stop", "request_key":"key", "scope":"chat"});
    serde_json::from_value::<RuntimeMutateAction>(input.clone()).unwrap();
    let catalog = runtime_type_schemas();
    let entry = catalog
        .iter()
        .find(|entry| entry.name == "runtime.stop")
        .unwrap();
    assert_eq!(entry.parameters["additionalProperties"], false);
    let advertised = serde_json::to_value(schemars::schema_for!(RuntimeMutateAction)).unwrap();
    assert!(
        advertised["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action == "runtime.stop")
    );
    for field in ["session_id", "owner_id", "include_caller", "principal"] {
        let mut invalid = input.clone();
        invalid[field] = json!("override");
        assert!(serde_json::from_value::<RuntimeMutateAction>(invalid).is_err());
    }
}

#[tokio::test]
async fn batch_stop_live_followup_registry_does_not_retain_finished_contexts() {
    use crate::agent_runtime::execution_context::TurnExecutionContext;
    use crate::request_principal::RequestPrincipal;
    use crate::turn_continuation::{TurnContinuationScope, live_resumes, register_live_resume};
    let execution = Arc::new(
        TurnExecutionContext::from_scope(
            "resume-turn-batch-stop-test",
            RequestPrincipal::anonymous(crate::request_principal::TransportClass::Loopback),
            tokio_util::sync::CancellationToken::new(),
            None,
            TurnContinuationScope {
                turn_correlation_id: "parent-batch-stop-test".into(),
                session_id: "ses_batch_stop_test".into(),
                identity_user_id: None,
                original_prompt: String::new(),
                delivery_target: None,
                provider: "test".into(),
                model: "test".into(),
                response_depth_mode: "standard".into(),
                supports_ui_artifacts: false,
                supports_liquid_markdown: false,
                supports_browser_host: false,
                browser_driver_id: None,
                selected_worlds: Vec::new(),
                channel_surface: None,
            },
        )
        .unwrap(),
    );
    let id = "batch-stop-live-followup-test";
    register_live_resume(id.into(), &execution);
    let live = live_resumes();
    assert_eq!(live[id].turn_id(), "resume-turn-batch-stop-test");
    live[id].cancellation().cancel();
    assert!(execution.cancellation().is_cancelled());
    drop(live);
    drop(execution);
    assert!(!live_resumes().contains_key(id));
}

#[tokio::test]
async fn batch_stop_requires_admitted_execution() {
    use stasis::application::runtime::in_memory_runtime::InMemoryRuntime;
    let runtime = Arc::new(RuntimeComposition::InMemory(InMemoryRuntime::new()));
    assert!(
        execute(request(StopScope::Workshop), runtime)
            .await
            .unwrap_err()
            .to_string()
            .contains("admitted owner turn")
    );
}
