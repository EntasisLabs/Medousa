use super::*;
use crate::AgentRuntimeKind;
use tempfile::TempDir;

fn config(mode: &str) -> (TempDir, AcpAgentConfig) {
    let temp = tempfile::tempdir().unwrap();
    let config = AcpAgentConfig {
        kind: AgentRuntimeKind::MuseCode,
        command: "python3".into(),
        args: vec![
            format!("{}/src/muse/fixtures/server.py", env!("CARGO_MANIFEST_DIR")),
            mode.into(),
        ],
        cwd: Some(temp.path().canonicalize().unwrap().display().to_string()),
    };
    (temp, config)
}

async fn events(client: &MuseClient, id: &AcpSessionId) -> Vec<AcpEvent> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut result = Vec::new();
        loop {
            if let Some(event) = client.next_event(id).await.unwrap() {
                let done = matches!(event, AcpEvent::Done);
                result.push(event);
                if done {
                    break;
                }
            }
        }
        result
    })
    .await
    .expect("MSP fixture must terminate the turn")
}

fn requests(temp: &TempDir) -> Vec<Value> {
    std::fs::read_to_string(temp.path().join("msp-requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn msp_handshake_stream_tools_and_model_configuration() {
    let (temp, config) = config("normal");
    let client = MuseClient::new();
    let (id, wire, resumed) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    assert!(id.0.starts_with("muse-code-"));
    assert!(!resumed);
    assert_eq!(client.wire_session_id(&id).await, wire);
    assert!(client.session_capabilities(&id).await.resume);
    let options = client.session_config_options(&id).await;
    assert_eq!(options[0]["currentValue"], "muse-1");
    assert_eq!(options[1]["options"][1]["value"], "high");
    assert!(
        client
            .set_session_config_option(&id, "model", json!("invented"))
            .await
            .is_err()
    );
    client
        .set_session_config_option(&id, "model", json!("muse-2"))
        .await
        .unwrap();
    client
        .set_session_config_option(&id, "reasoning_effort", json!("ultra"))
        .await
        .unwrap();
    assert!(
        client
            .set_session_config_option(&id, "reasoning_effort", json!("high"))
            .await
            .is_err()
    );
    client.prompt(&id, "hello").await.unwrap();
    let observed = events(&client, &id).await;
    assert!(
        observed
            .iter()
            .any(|event| matches!(event, AcpEvent::MessageDelta { text } if text == "🌈"))
    );
    assert!(
        observed
            .iter()
            .any(|event| matches!(event, AcpEvent::MessageDone { text } if text == "Hello 🌈"))
    );
    assert!(observed.iter().any(|event| matches!(event, AcpEvent::ToolCall { input, .. } if input["status"] == "completed" && input["output"] == "yes" && input["arguments"]["command"] == "echo yes")));
    assert!(
        !observed
            .iter()
            .any(|event| matches!(event, AcpEvent::Error { .. }))
    );
    client.release(&id).await.unwrap();
    let sent = requests(&temp);
    assert_eq!(sent[0]["method"], "initialize");
    assert_eq!(sent[1]["method"], "initialized");
    let turn = sent
        .iter()
        .find(|frame| frame["method"] == "turn/start")
        .unwrap();
    assert_eq!(turn["params"]["reasoningEffort"], "ultra");
}

#[tokio::test]
async fn msp_permission_receipt_explicit_choice_and_duplicate_notification() {
    let (temp, config) = config("normal");
    let client = MuseClient::new();
    let (id, _, _) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    client.prompt(&id, "hold").await.unwrap();
    let permission = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(AcpEvent::PermissionRequest { id, options, .. }) =
                client.next_event(&id).await.unwrap()
            {
                break (id, options);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(permission.1.len(), 3);
    assert_eq!(permission.1[1].kind, "allow_always");
    assert!(
        client
            .respond_permission_choice(&id, &permission.0, Some("invented"))
            .await
            .is_err()
    );
    client
        .respond_permission_choice(&id, &permission.0, Some("session"))
        .await
        .unwrap();
    assert!(
        !events(&client, &id)
            .await
            .iter()
            .any(|event| matches!(event, AcpEvent::PermissionRequest { .. }))
    );
    client.release(&id).await.unwrap();
    let sent = requests(&temp);
    assert!(
        sent.iter()
            .any(|frame| frame["id"] == "present-1" && frame["result"] == json!({}))
    );
    assert!(
        sent.iter().any(|frame| frame["method"] == "approval/decide"
            && frame["params"]["choiceId"] == "session")
    );
}

#[tokio::test]
async fn msp_boolean_permission_never_grants_session_scope() {
    let (temp, config) = config("normal");
    let client = MuseClient::new();
    let (id, _, _) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    client.prompt(&id, "hold").await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(AcpEvent::PermissionRequest { id: request, .. }) =
                client.next_event(&id).await.unwrap()
            {
                client
                    .respond_permission(&id, &request, true)
                    .await
                    .unwrap();
                break;
            }
        }
    })
    .await
    .unwrap();
    events(&client, &id).await;
    client.release(&id).await.unwrap();
    assert!(
        requests(&temp)
            .iter()
            .any(|frame| frame["method"] == "approval/decide"
                && frame["params"]["choiceId"] == "once")
    );
}

#[tokio::test]
async fn msp_resume_preserves_identity_and_refuses_missing_session() {
    let (_temp, config) = config("normal");
    let client = MuseClient::new();
    let wire = uuid::Uuid::now_v7().to_string();
    let (id, token, resumed) = client
        .create_or_resume_session(&config, Some(&wire))
        .await
        .unwrap();
    assert!(resumed);
    assert_eq!(token.as_deref(), Some(wire.as_str()));
    client.release(&id).await.unwrap();
    let (_temp, config) = self::config("resume-fail");
    let error = client
        .create_or_resume_session(&config, Some(&wire))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("sessionNotFound"));
}

#[tokio::test]
async fn msp_paused_session_does_not_block_another_session_or_cancel() {
    let (_a, a) = config("normal");
    let (_b, b) = config("normal");
    let client = MuseClient::new();
    let (a_id, _, _) = client.create_or_resume_session(&a, None).await.unwrap();
    let (b_id, _, _) = client.create_or_resume_session(&b, None).await.unwrap();
    client.prompt(&a_id, "stall").await.unwrap();
    assert!(client.prompt(&a_id, "overlap").await.is_err());
    client.prompt(&b_id, "hello").await.unwrap();
    assert!(matches!(
        events(&client, &b_id).await.last(),
        Some(AcpEvent::Done)
    ));
    client.cancel(&a_id).await.unwrap();
    assert!(client.next_event(&a_id).await.is_err());
    assert!(client.prompt(&a_id, "after stop").await.is_err());
    client.release(&a_id).await.unwrap();
    client.release(&b_id).await.unwrap();
}

#[tokio::test]
async fn msp_reports_malformed_frames_and_provider_failures() {
    let (_temp, config) = config("normal");
    let client = MuseClient::new();
    let (id, _, _) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    client.prompt(&id, "fail").await.unwrap();
    assert!(
        events(&client, &id)
            .await
            .iter()
            .any(|event| matches!(event, AcpEvent::Error { message } if message.contains("quota")))
    );
    client.prompt(&id, "malformed").await.unwrap();
    assert!(events(&client, &id).await.iter().any(
        |event| matches!(event, AcpEvent::Error { message } if message.contains("malformed JSON"))
    ));
    client.release(&id).await.unwrap();
}

#[tokio::test]
async fn msp_initialization_failure_and_bounded_stderr_drain() {
    let client = MuseClient::new();
    for mode in ["exit", "version"] {
        let (_temp, config) = config(mode);
        assert!(
            client
                .create_or_resume_session(&config, None)
                .await
                .is_err()
        );
    }
    let (_temp, config) = config("stderr");
    let (id, _, _) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    let session = client.session(&id).await.unwrap();
    assert!(session.stderr.lock().await.len() <= STDERR_LIMIT);
    client.release(&id).await.unwrap();
    let (_temp, config) = self::config("ephemeral");
    let (id, _, _) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    assert!(!client.session_capabilities(&id).await.resume);
    client.release(&id).await.unwrap();
}

#[tokio::test]
async fn msp_failed_turn_does_not_leave_completion_for_the_next_prompt() {
    let (_temp, config) = config("normal");
    let client = MuseClient::new();
    let (id, _, _) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    client.prompt(&id, "fail").await.unwrap();
    // Match the daemon: Error ends consumption without reading the paired Done.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(AcpEvent::Error { .. }) = client.next_event(&id).await.unwrap() {
                break;
            }
        }
    })
    .await
    .unwrap();
    client.prompt(&id, "hello").await.unwrap();
    let retry = events(&client, &id).await;
    assert!(
        retry
            .iter()
            .any(|event| matches!(event, AcpEvent::MessageDone { text } if text == "Hello 🌈"))
    );
    assert!(!matches!(retry.first(), Some(AcpEvent::Done)));
    client.release(&id).await.unwrap();
}

#[tokio::test]
async fn msp_cancel_during_approval_releases_process_and_session_handle() {
    let (_temp, config) = config("normal");
    let client = MuseClient::new();
    let (id, _, _) = client
        .create_or_resume_session(&config, None)
        .await
        .unwrap();
    client.prompt(&id, "hold").await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(AcpEvent::PermissionRequest { .. }) = client.next_event(&id).await.unwrap()
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    let process = client.session(&id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), client.cancel(&id))
        .await
        .unwrap()
        .unwrap();
    assert!(client.sessions.lock().await.is_empty());
    assert!(process.child.lock().await.try_wait().unwrap().is_some());
    assert!(
        client
            .respond_permission_choice(&id, "approval-1:0", Some("once"))
            .await
            .is_err()
    );
}
