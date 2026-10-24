//! Subprocess tests of the native Codex transport through the shared client.
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

use crate::{
    AcpAgentConfig, AcpClient, AcpEvent, AcpSessionId, AgentRuntimeKind, ExternalAcpClient,
};

#[tokio::test]
async fn codex_native_running_resume_is_rejected_explicitly() {
    let (_dir, config) = fixture("active-resume");
    let client = ExternalAcpClient::new();
    let error = client
        .create_or_resume_session(&config, Some("saved-thread"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("active turn"), "{error}");
}

fn fixture(mode: &str) -> (TempDir, AcpAgentConfig) {
    let dir = tempfile::tempdir().unwrap();
    let config = AcpAgentConfig {
        kind: AgentRuntimeKind::Codex,
        command: "python3".into(),
        args: vec![
            format!("{}/fixtures/codex_server.py", env!("CARGO_MANIFEST_DIR")),
            mode.into(),
        ],
        cwd: Some(dir.path().canonicalize().unwrap().display().to_string()),
    };
    (dir, config)
}

async fn collect(client: &ExternalAcpClient, id: &AcpSessionId) -> Vec<AcpEvent> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut events = Vec::new();
        loop {
            if let Some(event) = client.next_event(id).await.unwrap() {
                let terminal = matches!(event, AcpEvent::Done | AcpEvent::Error { .. });
                events.push(event);
                if terminal {
                    return events;
                }
            }
        }
    })
    .await
    .expect("native peer must complete")
}

fn requests(dir: &TempDir) -> Vec<Value> {
    std::fs::read_to_string(dir.path().join("codex-requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn codex_native_resume_models_tools_and_followups() {
    let (dir, config) = fixture("normal");
    let client = ExternalAcpClient::new();
    let (id, wire, resumed) = client
        .create_or_resume_session(&config, Some("saved-thread"))
        .await
        .unwrap();
    assert!(id.0.starts_with("codex-native-"));
    assert_eq!(wire.as_deref(), Some("saved-thread"));
    assert!(resumed && client.session_capabilities(&id).await.resume);
    assert!(
        client.next_event(&id).await.unwrap().is_none(),
        "resume must not replay historical text"
    );
    let options = client.session_config_options(&id).await;
    assert_eq!(options[0]["options"].as_array().unwrap().len(), 2);
    client
        .set_session_config_option(&id, "model", json!("fixture-two"))
        .await
        .unwrap();
    client
        .set_session_config_option(&id, "reasoning_effort", json!("low"))
        .await
        .unwrap();
    for number in 1..=2 {
        client.prompt(&id, "hello").await.unwrap();
        let events = collect(&client, &id).await;
        assert!(matches!(events.last(), Some(AcpEvent::Done)));
        let text: String = events
            .iter()
            .filter_map(|event| match event {
                AcpEvent::MessageDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            text,
            format!("answer {number}"),
            "completed text must not duplicate deltas"
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, AcpEvent::ToolCall { .. }))
                .count(),
            2
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AcpEvent::ReasoningDelta { .. }))
        );
    }
    client.cancel(&id).await.unwrap();
    let requests = requests(&dir);
    let turns: Vec<_> = requests
        .iter()
        .filter(|r| r["method"] == "turn/start")
        .collect();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0]["params"]["model"], "fixture-two");
    assert_eq!(turns[0]["params"]["effort"], "low");
    assert!(!requests.iter().any(|r| {
        r["method"]
            .as_str()
            .is_some_and(|m| m.starts_with("account/login"))
    }));
}

#[tokio::test]
async fn codex_native_rpc_ack_is_not_turn_completion() {
    let (_dir, config) = fixture("delayed");
    let client = ExternalAcpClient::new();
    let (id, _, _) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    client.prompt(&id, "hello").await.unwrap();
    assert!(client.next_event(&id).await.unwrap().is_none());
    assert!(matches!(
        collect(&client, &id).await.last(),
        Some(AcpEvent::Done)
    ));
    client.cancel(&id).await.unwrap();
}

#[tokio::test]
async fn codex_native_approval_ids_choices_and_independent_sessions() {
    for mode in ["approval-number", "approval-string"] {
        let (dir, config) = fixture(mode);
        let client = ExternalAcpClient::new();
        let (id, _, _) = client
            .create_or_resume_session(&config, None)
            .await
            .unwrap();
        client.prompt(&id, "hello").await.unwrap();
        let (request_id, choice) = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match client.next_event(&id).await.unwrap() {
                    Some(AcpEvent::PermissionRequest { id, options, .. }) => {
                        assert_eq!(options.len(), 3);
                        return (id, options[1].option_id.clone());
                    }
                    Some(AcpEvent::Done | AcpEvent::Error { .. }) => {
                        panic!("approval must precede completion")
                    }
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        assert!(
            client
                .respond_permission_choice(&id, &request_id, Some("unoffered"))
                .await
                .is_err()
        );
        let (_other_dir, other_config) = fixture("normal");
        let (other, _, _) = client
            .create_or_resume_session(&other_config, None)
            .await
            .unwrap();
        client.prompt(&other, "independent").await.unwrap();
        assert!(matches!(
            collect(&client, &other).await.last(),
            Some(AcpEvent::Done)
        ));
        client
            .respond_permission_choice(&id, &request_id, Some(&choice))
            .await
            .unwrap();
        assert!(matches!(
            collect(&client, &id).await.last(),
            Some(AcpEvent::Done)
        ));
        client.cancel(&id).await.unwrap();
        client.cancel(&other).await.unwrap();
        let reply = requests(&dir)
            .into_iter()
            .find(|r| r.get("method").is_none())
            .unwrap();
        assert_eq!(reply["id"].is_number(), mode == "approval-number");
        assert_eq!(reply["result"]["decision"], "acceptForSession");
    }
}

#[tokio::test]
async fn codex_native_failures_never_become_success() {
    for mode in ["failed", "crash", "unsupported", "malformed", "oversize"] {
        let (_dir, config) = fixture(mode);
        let client = ExternalAcpClient::new();
        let (id, _, _) = client
            .create_or_resume_session(&config, None)
            .await
            .unwrap();
        client.prompt(&id, "hello").await.unwrap();
        let events = collect(&client, &id).await;
        assert!(
            matches!(events.last(), Some(AcpEvent::Error { .. })),
            "{mode}: {events:?}"
        );
        assert!(!events.iter().any(|event| matches!(event, AcpEvent::Done)));
        let _ = client.cancel(&id).await;
    }
}

#[tokio::test]
async fn codex_native_signed_out_or_api_key_never_starts_login_or_thread() {
    for mode in ["signed-out", "api-key"] {
        let (dir, config) = fixture(mode);
        let client = ExternalAcpClient::new();
        let error = client
            .create_or_resume_session(&config, None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("saved ChatGPT login"));
        assert!(!requests(&dir).iter().any(|r| {
            r["method"] == "thread/start"
                || r["method"]
                    .as_str()
                    .is_some_and(|m| m.starts_with("account/login"))
        }));
    }
}

#[tokio::test]
async fn codex_native_cancel_interrupts_and_reaps_active_turn() {
    let (dir, config) = fixture("hold");
    let client = ExternalAcpClient::new();
    let (id, _, _) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    client.prompt(&id, "hello").await.unwrap();
    tokio::time::timeout(Duration::from_secs(4), client.cancel(&id))
        .await
        .unwrap()
        .unwrap();
    assert!(
        requests(&dir)
            .iter()
            .any(|r| r["method"] == "turn/interrupt")
    );
}
