//! Subprocess-level tests for the shared ACP transport (not vendor mocks).
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

use crate::{
    AcpAgentConfig, AcpClient, AcpEvent, AcpSessionId, AgentRuntimeKind, ExternalAcpClient,
};

fn fixture(mode: &str) -> (TempDir, AcpAgentConfig) {
    let directory = tempfile::tempdir().unwrap();
    let config = AcpAgentConfig {
        kind: AgentRuntimeKind::Cursor,
        command: "python3".into(),
        args: vec![
            format!("{}/fixtures/acp_server.py", env!("CARGO_MANIFEST_DIR")),
            mode.into(),
        ],
        cwd: Some(
            directory
                .path()
                .canonicalize()
                .unwrap()
                .display()
                .to_string(),
        ),
    };
    (directory, config)
}

async fn collect(client: &ExternalAcpClient, session: &AcpSessionId) -> Vec<AcpEvent> {
    tokio::time::timeout(Duration::from_secs(4), async {
        let mut events = Vec::new();
        loop {
            if let Some(event) = client.next_event(session).await.unwrap() {
                let done = matches!(event, AcpEvent::Done | AcpEvent::Error { .. });
                events.push(event);
                if done {
                    return events;
                }
            }
        }
    })
    .await
    .expect("fixture prompt must finish")
}

async fn permission(client: &ExternalAcpClient, session: &AcpSessionId) -> String {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match client.next_event(session).await.unwrap() {
                Some(AcpEvent::PermissionRequest { id, options, .. }) => {
                    assert_eq!(options.len(), 3);
                    assert_eq!(options[1].option_id, "allow-second");
                    return id;
                }
                Some(AcpEvent::Done) => {
                    panic!("an opposite-direction request id is not prompt completion")
                }
                Some(AcpEvent::Error { message }) => panic!("fixture failed: {message}"),
                _ => {}
            }
        }
    })
    .await
    .expect("fixture must request permission")
}

fn requests(directory: &TempDir) -> Vec<Value> {
    std::fs::read_to_string(directory.path().join("acp-requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn acp_handshake_resume_config_and_tool_lifecycle_are_preserved() {
    let (_directory, config) = fixture("normal");
    let client = ExternalAcpClient::new();
    let (session, wire, resumed) = client
        .create_or_resume_session(&config, Some("retained-session"))
        .await
        .unwrap();
    assert_eq!(wire.as_deref(), Some("retained-session"));
    assert!(resumed);
    let capabilities = client.session_capabilities(&session).await;
    assert!(capabilities.resume && capabilities.fork && capabilities.images);
    assert!(!capabilities.audio);
    let options = client.session_config_options(&session).await;
    assert_eq!(options[0]["currentValue"], "fixture-one");
    let updated = client
        .set_session_config_option(&session, "model", json!("fixture-two"))
        .await
        .unwrap();
    assert_eq!(updated[0]["currentValue"], "fixture-two");
    client.prompt(&session, "hello").await.unwrap();
    let events = collect(&client, &session).await;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AcpEvent::ReasoningDelta { text } if text == "thinking"))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AcpEvent::MessageDelta { text } if text == "Hello 🌈"))
    );
    assert!(events.iter().any(|event| matches!(event, AcpEvent::ToolCall { input, .. } if input["status"] == "completed" && input["rawOutput"] == "read result")));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AcpEvent::Done))
            .count(),
        1
    );
    client.cancel(&session).await.unwrap();
}

#[tokio::test]
async fn acp_load_hydration_cannot_become_the_next_prompt_answer() {
    let (directory, config) = fixture("load-history");
    let client = ExternalAcpClient::new();
    let (session, wire, resumed) = client
        .create_or_resume_session(&config, Some("retained-history"))
        .await
        .unwrap();
    assert!(resumed);
    assert_eq!(wire.as_deref(), Some("retained-history"));
    // Controls and actionable permission requests survive transcript hydration.
    assert_eq!(
        client.session_config_options(&session).await[0]["currentValue"],
        "fixture-two"
    );
    let pending = permission(&client, &session).await;
    assert_eq!(pending, "hydration-choice");
    client
        .respond_permission_choice(&session, &pending, Some("allow-second"))
        .await
        .unwrap();
    assert!(client.next_event(&session).await.unwrap().is_none());
    client.prompt(&session, "a new question").await.unwrap();
    let observed = collect(&client, &session).await;
    let text: String = observed
        .iter()
        .filter_map(|event| match event {
            AcpEvent::MessageDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        text, "Hello 🌈",
        "neither hydrated history nor current user echoes are assistant output"
    );
    assert!(
        !observed
            .iter()
            .any(|event| matches!(event, AcpEvent::ToolCall { id, .. } if id == "historical-tool"))
    );
    assert!(
        !observed.iter().any(
            |event| matches!(event, AcpEvent::ReasoningDelta { text } if text.contains("OLD"))
        )
    );
    client.cancel(&session).await.unwrap();
    assert!(
        requests(&directory)
            .iter()
            .any(|frame| frame["method"] == "session/load")
    );
}

#[tokio::test]
async fn acp_exact_permission_choice_and_numeric_string_ids_survive_collisions() {
    let (directory, config) = fixture("normal");
    let client = ExternalAcpClient::new();
    let session = client.create_session(&config).await.unwrap();
    client.prompt(&session, "permissions").await.unwrap();
    let first = permission(&client, &session).await;
    assert!(
        client
            .respond_permission_choice(&session, &first, Some("invented"))
            .await
            .is_err()
    );
    client
        .respond_permission_choice(&session, &first, Some("allow-second"))
        .await
        .unwrap();
    let second = permission(&client, &session).await;
    client
        .respond_permission_choice(&session, &second, Some("allow-second"))
        .await
        .unwrap();
    let events = collect(&client, &session).await;
    assert!(events.iter().any(
        |event| matches!(event, AcpEvent::MessageDelta { text } if text == "permission granted")
    ));
    client.cancel(&session).await.unwrap();
    let sent = requests(&directory);
    let replies: Vec<_> = sent
        .iter()
        .filter(|frame| frame.get("method").is_none() && frame.get("result").is_some())
        .collect();
    assert_eq!(replies.len(), 2);
    assert!(replies[0]["id"].is_number());
    assert!(replies[1]["id"].is_string());
    for reply in replies {
        assert_eq!(reply["result"]["outcome"]["optionId"], "allow-second");
    }
}

#[tokio::test]
async fn acp_explicit_false_capabilities_do_not_authorize_resume() {
    let (directory, config) = fixture("false-caps");
    let client = ExternalAcpClient::new();
    let (session, _, resumed) = client
        .create_or_resume_session(&config, Some("unsupported-resume"))
        .await
        .unwrap();
    assert!(!resumed);
    let caps = client.session_capabilities(&session).await;
    assert!(!caps.resume && !caps.fork && !caps.images && !caps.audio);
    client.cancel(&session).await.unwrap();
    assert!(
        !requests(&directory)
            .iter()
            .any(|frame| frame["method"] == "session/resume" || frame["method"] == "session/load")
    );
}

#[tokio::test]
async fn acp_quiet_poll_is_not_completion() {
    let (_directory, config) = fixture("normal");
    let client = ExternalAcpClient::new();
    let session = client.create_session(&config).await.unwrap();
    client.prompt(&session, "quiet").await.unwrap();
    assert!(client.next_event(&session).await.unwrap().is_none());
    let events = collect(&client, &session).await;
    assert!(
        events.iter().any(
            |event| matches!(event, AcpEvent::MessageDelta { text } if text == "still working")
        )
    );
    client.cancel(&session).await.unwrap();
}

#[tokio::test]
async fn acp_process_eof_and_prompt_failure_are_errors_not_success() {
    let client = ExternalAcpClient::new();
    for text in ["eof", "crash"] {
        let (_directory, config) = fixture("normal");
        let session = client.create_session(&config).await.unwrap();
        client.prompt(&session, text).await.unwrap();
        let event = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(event) = client.next_event(&session).await.unwrap() {
                    return event;
                }
            }
        })
        .await
        .unwrap();
        assert!(
            matches!(event, AcpEvent::Error { message } if !message.contains("do-not-expose-vendor-secret"))
        );
        client.cancel(&session).await.unwrap();
    }
    let (_directory, config) = fixture("normal");
    let session = client.create_session(&config).await.unwrap();
    client.prompt(&session, "prompt-error").await.unwrap();
    let events = collect(&client, &session).await;
    assert!(
        matches!(events.first(), Some(AcpEvent::Error { message }) if message == "fixture provider error")
    );
    assert_eq!(
        events.len(),
        1,
        "an error itself is terminal; no stale Done may survive"
    );
    client.cancel(&session).await.unwrap();
}

#[tokio::test]
async fn acp_stderr_saturation_does_not_block_initialization() {
    let (_directory, config) = fixture("stderr");
    let client = ExternalAcpClient::new();
    let session = tokio::time::timeout(Duration::from_secs(4), client.create_session(&config))
        .await
        .unwrap()
        .unwrap();
    client.prompt(&session, "hello").await.unwrap();
    collect(&client, &session).await;
    client.cancel(&session).await.unwrap();
}

#[tokio::test]
async fn acp_slow_session_does_not_hold_the_global_process_map() {
    let (slow_directory, slow_config) = fixture("slow-config");
    let (_fast_directory, fast_config) = fixture("normal");
    let client = Arc::new(ExternalAcpClient::new());
    let slow = client.create_session(&slow_config).await.unwrap();
    let fast = client.create_session(&fast_config).await.unwrap();
    let worker = {
        let client = client.clone();
        let slow = slow.clone();
        tokio::spawn(async move {
            client
                .set_session_config_option(&slow, "model", json!("fixture-two"))
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        while !slow_directory.path().join("config-waiting").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_millis(350), async {
        client.prompt(&fast, "hello").await.unwrap();
        collect(&client, &fast).await;
    })
    .await
    .expect("another session must remain usable while the first awaits a reply");
    worker.await.unwrap().unwrap();
    client.cancel(&slow).await.unwrap();
    client.cancel(&fast).await.unwrap();
}
