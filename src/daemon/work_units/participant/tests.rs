use super::*;
use crate::request_principal::TransportClass;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use tower::ServiceExt;

pub(crate) async fn fixture() -> (tempfile::TempDir, Arc<WorkUnitHost>) {
    crate::workshop_authority::initialize(
        &medousa_types::secrets::InstallationId::parse(
            crate::workshop_authority::TEST_INSTALLATION_ID,
        )
        .unwrap(),
    )
    .unwrap();
    let execution = Arc::new(ForgeExecutionService::new());
    let (dir, store, forge) = execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, || {
            Ok((|| -> Result<_> {
                let dir = tempfile::tempdir()?;
                let root = dir.path().canonicalize()?;
                let store = WorkGraphStore::open(&root)?;
                let forge = medousa_forge::forge::Forge::open(root.join("forge"))?;
                Ok((dir, store, forge))
            })())
        })
        .await
        .unwrap()
        .unwrap();
    (
        dir,
        Arc::new(WorkUnitHost {
            store: Arc::new(store),
            forge: Arc::new(forge),
            execution,
        }),
    )
}

fn principal(owner: &str, work: bool) -> RequestPrincipal {
    RequestPrincipal::external_agent(
        Arc::from("external-agent:credential-1"),
        owner.into(),
        work,
        TransportClass::Direct,
    )
}

pub(crate) async fn request(
    host: Arc<WorkUnitHost>,
    principal: RequestPrincipal,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = surface_with_host(Some(host))
        .into_router()
        .layer(Extension(principal))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), MAX_SNAPSHOT_BYTES + 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

pub(crate) fn accept() -> Value {
    json!({"action":"work.record","command":{"command_id":"accept-unit","expected_revision":0,"mutation":{
        "operation":"accept_work","work_unit_id":"unit","intent":"Ship the requested change","kind":"finite",
        "scope":{"resources":[],"children":[],"depends_on":[]},"completion_condition":"Exact review approves", "origin":null
    }}})
}

#[tokio::test]
async fn participant_hooks_are_credential_bound_and_acknowledgments_do_not_launch_work() {
    let (_dir, host) = fixture().await;
    assert_eq!(
        request(
            host.clone(),
            principal("owner", true),
            "/v1/work/mutate",
            accept()
        )
        .await
        .0,
        StatusCode::OK
    );
    let subscribe = json!({"action":"work.record","command":{"command_id":"hook","expected_revision":1,
        "mutation":{"operation":"subscribe","input":{"subscription_id":"hook","work_unit_id":"unit",
        "expected_scope_revision":1,"resources":[],"event_kinds":["work_state_changed"],"after_revision":1,
        "expires_at":chrono::Utc::now() + chrono::Duration::hours(1)}}}});
    assert_eq!(
        request(
            host.clone(),
            principal("owner", false),
            "/v1/work/mutate",
            subscribe.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            host.clone(),
            principal("owner", true),
            "/v1/work/mutate",
            subscribe
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(request(host.clone(), principal("owner", true), "/v1/work/mutate", json!({"action":"work.record","command":{
        "command_id":"state","expected_revision":2,"mutation":{"operation":"set_state","work_unit_id":"unit",
        "state":"active","reason":"Admitted work","evidence":[]}}})).await.0, StatusCode::OK);
    let query = json!({"action":"work.events","query":{"subscription_id":"hook","limit":1}});
    let (status, page) = request(
        host.clone(),
        principal("owner", true),
        "/v1/work/query",
        query.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["result"]["events"][0]["receipt"]["revision"], 3);
    let other = RequestPrincipal::external_agent(
        Arc::from("external-agent:other"),
        "owner".into(),
        true,
        TransportClass::Direct,
    );
    assert_eq!(
        request(host.clone(), other, "/v1/work/query", query.clone())
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(request(host.clone(), principal("owner", true), "/v1/work/mutate", json!({"action":"work.record","command":{
        "command_id":"ack","expected_revision":3,"mutation":{"operation":"acknowledge_event","subscription_id":"hook",
        "event_revision":3,"decision":"Observed the accepted work"}}})).await.0, StatusCode::OK);
    assert!(
        request(host, principal("owner", true), "/v1/work/query", query)
            .await
            .1["result"]["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn participant_intent_reopens_without_a_chat_and_preserves_credential_provenance() {
    let (dir, host) = fixture().await;
    assert_eq!(
        request(
            host.clone(),
            principal("owner", true),
            "/v1/work/mutate",
            accept()
        )
        .await
        .0,
        StatusCode::OK
    );
    let replay = request(
        host.clone(),
        principal("owner", true),
        "/v1/work/mutate",
        accept(),
    )
    .await;
    assert_eq!(replay.0, StatusCode::OK);
    assert_eq!(replay.1["result"]["replayed"], true);
    let execution = host.execution.clone();
    let root = host
        .execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, {
            let path = dir.path().to_path_buf();
            move || Ok(path.canonicalize()?)
        })
        .await
        .unwrap();
    let store = execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok(WorkGraphStore::open(&root))
        })
        .await
        .unwrap()
        .unwrap();
    let reopened = Arc::new(WorkUnitHost {
        store: Arc::new(store),
        forge: host.forge.clone(),
        execution,
    });
    let get = request(
        reopened.clone(),
        principal("owner", false),
        "/v1/work/query",
        json!({"action":"work.get","work_unit_id":"unit"}),
    )
    .await;
    assert_eq!(get.0, StatusCode::OK);
    assert_eq!(get.1["result"]["state"], "accepted");
    assert_eq!(
        get.1["result"]["provenance"]["actor_id"],
        "external-agent:credential-1"
    );
    assert_eq!(get.1["result"]["provenance"]["source"], "model_inferred");
    assert_eq!(
        request(
            reopened,
            principal("other", true),
            "/v1/work/query",
            json!({"action":"work.get","work_unit_id":"unit"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn reads_writes_and_source_chat_continuations_have_distinct_admission() {
    let (_dir, host) = fixture().await;
    assert_eq!(
        request(
            host.clone(),
            principal("owner", false),
            "/v1/work/query",
            json!({"action":"work.graph","query":{}})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            host.clone(),
            principal("owner", false),
            "/v1/work/mutate",
            accept()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            host.clone(),
            principal("owner", false),
            "/v1/work/query",
            json!({"action":"peer.discover","session_id":"ses_source"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(request(host.clone(), principal("owner", true), "/v1/work/mutate", json!({"action":"peer.propose","session_id":"ses_source","intent":{
        "request_key":"proposal","runtime":"codex","instructions":"Implement","after_entry_seq":0,"through_entry_seq":1,"continue_owner":true
    }})).await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        request(
            host,
            RequestPrincipal::local_app(Arc::from("local"), TransportClass::Loopback),
            "/v1/work/query",
            json!({"action":"work.graph","query":{}})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn participants_cannot_supply_native_evidence_budget_custody_or_owner() {
    let (_dir, host) = fixture().await;
    let authority = crate::workshop_authority::current().unwrap().clone();
    let mutations = [
        json!({"operation":"record_resource","reference":{"authority_id":authority,"kind":"vault_note","id":"note"},"resolution":"available","native_revision":"invented"}),
        json!({"operation":"reserve_budget","reservation_id":"fake","work_unit_id":"unit","execution":{"authority_id":authority,"kind":"assignment","id":"fake"},"reserved_cost_microusd":0}),
    ];
    for (i, mutation) in mutations.into_iter().enumerate() {
        assert_eq!(request(host.clone(), principal("owner", true), "/v1/work/mutate", json!({"action":"work.record","command":{"command_id":format!("fake-{i}"),"expected_revision":0,"mutation":mutation}})).await.0, StatusCode::CONFLICT);
    }
    assert_eq!(
        request(
            host,
            principal("owner", true),
            "/v1/work/query",
            json!({"action":"work.graph","query":{},"owner_id":"other"})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn provider_adapter_retains_exact_results_and_never_resends_claimed_work() {
    use medousa_types::{
        ExternalEventKind, ExternalProvider, ExternalProviderEventRequest, work_provider::*,
    };
    let (dir, host) = fixture().await;
    assert_eq!(
        request(
            host.clone(),
            principal("owner", true),
            "/v1/work/mutate",
            accept()
        )
        .await
        .0,
        StatusCode::OK
    );
    let domain = UserDomainRef {
        authority_id: crate::workshop_authority::current().unwrap().clone(),
        user_id: "owner".into(),
    };
    let input = WorkProviderRequestInput {
        work_unit_id: "unit".into(),
        expected_scope_revision: 1,
        deadline: chrono::Utc::now() + chrono::Duration::hours(1),
        review_of: None,
    };
    let prepared = host
        .prepare_provider_request(
            domain.clone(),
            "conversation".into(),
            ExternalProvider::Dots,
            "request".into(),
            input.clone(),
            "Implement the change".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        host.prepare_provider_request(
            domain.clone(),
            "conversation".into(),
            ExternalProvider::Dots,
            "request".into(),
            input.clone(),
            "Implement the change".into()
        )
        .await
        .unwrap(),
        prepared
    );
    assert!(
        host.prepare_provider_request(
            domain.clone(),
            "conversation".into(),
            ExternalProvider::Dots,
            "request".into(),
            input.clone(),
            "Different change".into()
        )
        .await
        .is_err()
    );
    let event = ExternalProviderEventRequest {
        event_id: "done".into(),
        request_id: "request".into(),
        kind: ExternalEventKind::Completed,
        text: "Implemented".into(),
        reaction: None,
    };
    assert!(
        host.record_provider_outcome(
            domain.clone(),
            "conversation".into(),
            "external-agent:test".into(),
            event.clone()
        )
        .await
        .is_err()
    );
    // A model credential cannot manufacture native dispatch or result custody.
    for mutation in [
        json!({"operation":"register_provider_request", "request":prepared}),
        json!({"operation":"claim_provider_request", "conversation_id":"conversation", "request_id":"request"}),
    ] {
        assert_eq!(
            request(
                host.clone(),
                principal("owner", true),
                "/v1/work/mutate",
                json!({"action":"work.record", "command":{
            "command_id":"fake-native", "expected_revision":2, "mutation":mutation}})
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
    }
    host.claim_provider_request(domain.clone(), prepared.clone())
        .await
        .unwrap();
    let reopened_root = dir.path().canonicalize().unwrap();
    let execution = host.execution.clone();
    let exec = execution.clone();
    let reopened = execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok((|| -> Result<_> {
                Ok(WorkUnitHost {
                    store: Arc::new(WorkGraphStore::open(&reopened_root)?),
                    forge: Arc::new(medousa_forge::forge::Forge::open(
                        reopened_root.join("forge"),
                    )?),
                    execution: exec,
                })
            })())
        })
        .await
        .unwrap()
        .unwrap();
    assert!(
        reopened
            .claim_provider_request(domain.clone(), prepared)
            .await
            .is_err()
    );
    assert!(
        reopened
            .prepare_provider_request(
                domain.clone(),
                "conversation".into(),
                ExternalProvider::Dots,
                "request".into(),
                input,
                "Implement the change".into()
            )
            .await
            .is_err()
    );
    assert!(
        reopened
            .record_provider_outcome(
                domain.clone(),
                "other".into(),
                "external-agent:test".into(),
                event.clone()
            )
            .await
            .is_err()
    );
    reopened
        .record_provider_outcome(
            domain.clone(),
            "conversation".into(),
            "external-agent:test".into(),
            event.clone(),
        )
        .await
        .unwrap();
    let original = reopened
        .provider_request(domain.clone(), "conversation".into(), "request".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        original.events[0].qualification,
        WorkProviderQualification::OutcomeOnly
    );
    // Credential rotation permits exact reconciliation without replacing the original actor/time.
    reopened
        .record_provider_outcome(
            domain.clone(),
            "conversation".into(),
            "external-agent:rotated".into(),
            event.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        reopened
            .provider_request(domain.clone(), "conversation".into(), "request".into())
            .await
            .unwrap()
            .unwrap(),
        original
    );
    let mut changed = event.clone();
    changed.text = "Different result".into();
    assert!(
        reopened
            .record_provider_outcome(
                domain.clone(),
                "conversation".into(),
                "external-agent:test".into(),
                changed
            )
            .await
            .is_err()
    );
    let mut replacement = event;
    replacement.event_id = "another-done".into();
    assert!(
        reopened
            .record_provider_outcome(
                domain,
                "conversation".into(),
                "external-agent:test".into(),
                replacement
            )
            .await
            .is_err()
    );
}
