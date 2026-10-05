use super::process::{OUTPUT_LIMIT, argv, drain, validate};
use super::*;
use tempfile::TempDir;

fn config() -> TargetConfig {
    TargetConfig {
        name: "Lab".into(),
        host: "lab.local".into(),
        port: 22,
        username: "ops".into(),
        identity_file: None,
        agent_access: true,
    }
}
fn keys() -> Vec<String> {
    use base64::Engine;
    vec![format!(
        "ssh-ed25519 {}",
        base64::engine::general_purpose::STANDARD.encode([1; 48])
    )]
}
async fn host(root: &TempDir) -> Arc<SshHost> {
    SshHost::open(
        Arc::new(ForgeExecutionService::new()),
        root.path().into(),
        super::super::shell_session_host::ShellSessionHost::new(),
    )
    .await
    .unwrap()
}
async fn save(host: &SshHost) -> String {
    host.save(
        "user:alice".into(),
        SaveTarget {
            config: config(),
            host_keys: keys(),
        },
    )
    .await
    .unwrap()["target_id"]
        .as_str()
        .unwrap()
        .into()
}

#[tokio::test]
async fn keyscan_banner_comments_are_excluded_from_keys_saved_by_home() {
    let key = &keys()[0];
    let scan = format!(
        "# lab.local:22 SSH-2.0-OpenSSH_10.0\n\nlab.local {key}\n  # lab.local:22 SSH-2.0-OpenSSH_10.0\r\n[lab.local]:22 {key}\r\n"
    );
    let inspected_keys = process::parse_keyscan(&scan).unwrap();
    assert_eq!(inspected_keys, keys());
    // Exercise the JSON round trip used by inspection -> Home -> save.
    let inspection = serde_json::json!({"host_keys": inspected_keys});
    let save_input: SaveTarget = serde_json::from_value(serde_json::json!({
        "config": config(), "host_keys": inspection["host_keys"]
    }))
    .unwrap();
    let root = TempDir::new().unwrap();
    let host = host(&root).await;
    let saved = host.save("user:alice".into(), save_input).await.unwrap();
    let target = host
        .access("user:alice", saved["target_id"].as_str().unwrap(), true)
        .await
        .unwrap();
    assert_eq!(target.host_keys, keys());
}

#[test]
fn keyscan_comments_do_not_hide_missing_or_invalid_host_keys() {
    for scan in [
        "# lab.local:22 SSH-2.0-OpenSSH_10.0\n\n",
        "lab.local ssh-ed25519",
        "lab.local unsupported-key AAAA",
        "lab.local ssh-ed25519 invalid!",
    ] {
        assert!(process::parse_keyscan(scan).is_err(), "accepted {scan:?}");
    }
}

#[test]
fn endpoint_options_and_relative_credentials_are_rejected() {
    for hostname in [
        "-oProxyCommand=evil",
        "ops@lab",
        "lab;echo bad",
        "lab\nother",
        "lab/else",
    ] {
        assert!(
            validate(&TargetConfig {
                host: hostname.into(),
                ..config()
            })
            .is_err()
        );
    }
    assert!(
        validate(&TargetConfig {
            identity_file: Some("../key".into()),
            ..config()
        })
        .is_err()
    );
    assert!(
        validate(&TargetConfig {
            host: "2001:db8::1".into(),
            ..config()
        })
        .is_ok()
    );
    assert!(process::normalize_keys(&[format!("lab {}", keys()[0])]).is_err());
}

#[test]
fn commands_and_identity_stay_separate_from_ssh_options() {
    let target = Target {
        id: "ssh-target".into(),
        owner: "alice".into(),
        config: config(),
        host_keys: keys(),
    };
    let command = "printf '%s' '$HOME'; docker ps";
    let args = argv(
        &target,
        std::path::Path::new("/workshop/known_hosts"),
        Some(command),
    );
    assert_eq!(args.last().unwrap(), command);
    assert!(args.iter().any(|a| a == "StrictHostKeyChecking=yes"));
    assert!(args.iter().any(|a| a == "ForwardAgent=no"));
    assert!(args.iter().any(|a| a == "HostKeyAlias=ssh-target"));
    assert!(!target.summary().to_string().contains("host_keys"));
    assert!(!target.summary().to_string().contains("identity_file"));
}

#[tokio::test]
async fn targets_survive_restart_and_require_the_saved_owner_grant() {
    let root = TempDir::new().unwrap();
    let first = host(&root).await;
    let id = save(&first).await;
    assert!(first.access("user:bob", &id, true).await.is_err());
    first.set_access("user:alice", &id, false).await.unwrap();
    assert!(first.access("user:alice", &id, true).await.is_err());
    assert!(first.access("user:alice", &id, false).await.is_ok());
    assert_eq!(
        first.targets("user:alice", true).await["targets"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let next = host(&root).await;
    assert_eq!(
        next.targets("user:alice", false).await["targets"][0]["name"],
        "Lab"
    );
    next.remove("user:alice", &id).await.unwrap();
    assert!(next.access("user:alice", &id, false).await.is_err());
}

#[tokio::test]
async fn retries_reuse_receipts_and_restart_never_replays_remote_work() {
    let root = TempDir::new().unwrap();
    let first = host(&root).await;
    let id = save(&first).await;
    let (receipt, fresh) = first
        .reserve("user:alice", &id, "deploy-1", Some("deploy".into()), None)
        .await
        .unwrap();
    assert!(fresh);
    assert!(
        !first
            .reserve("user:alice", &id, "deploy-1", Some("deploy".into()), None)
            .await
            .unwrap()
            .1
    );
    assert!(
        first
            .reserve(
                "user:alice",
                &id,
                "deploy-1",
                Some("deploy-again".into()),
                None
            )
            .await
            .is_err()
    );
    assert!(
        first
            .status("user:bob", &receipt.execution_id)
            .await
            .is_err()
    );
    assert!(
        first
            .status("user:alice", "../../connections")
            .await
            .is_err()
    );
    let next = host(&root).await;
    let (recovered, fresh) = next
        .reserve("user:alice", &id, "deploy-1", Some("deploy".into()), None)
        .await
        .unwrap();
    assert!(!fresh);
    assert_eq!(recovered.status, "unknown");
    assert_eq!(recovered.execution_id, receipt.execution_id);
    assert_eq!(
        next.status("user:alice", &receipt.execution_id)
            .await
            .unwrap()["remote_outcome_unknown"],
        true
    );
}

#[tokio::test]
async fn output_is_bounded_but_the_pipe_is_fully_drained() {
    let (mut writer, reader) = tokio::io::duplex(8192);
    let task = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        writer
            .write_all(&vec![b'x'; OUTPUT_LIMIT * 3])
            .await
            .unwrap();
    });
    let (output, truncated) = drain(reader).await.unwrap();
    task.await.unwrap();
    assert!(truncated);
    assert_eq!(output.len(), OUTPUT_LIMIT);
}

#[test]
fn agent_schemas_cannot_supply_credentials_or_grants() {
    let mut input = serde_json::json!({"target_id": "saved", "request_key": "one", "command": "uptime", "timeout_ms": null});
    for field in [
        "host",
        "owner",
        "identity_file",
        "agent_access",
        "host_keys",
    ] {
        input[field] = "forged".into();
        assert!(serde_json::from_value::<RunInput>(input.clone()).is_err());
        input.as_object_mut().unwrap().remove(field);
    }
    assert!(
        owner(
            &RequestPrincipal::anonymous(crate::request_principal::TransportClass::Loopback),
            Some("user:alice")
        )
        .is_err()
    );
}

#[cfg(unix)]
async fn fake_host(root: &TempDir, script: &str) -> Arc<SshHost> {
    use std::os::unix::fs::PermissionsExt;
    let program = root.path().join("fake-ssh");
    std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut host = host(root).await;
    Arc::get_mut(&mut host).unwrap().ssh_program = program;
    host
}

#[cfg(unix)]
#[tokio::test]
async fn command_completion_is_persisted_and_a_retry_does_not_execute_twice() {
    let root = TempDir::new().unwrap();
    let count = root.path().join("count");
    let first = fake_host(
        &root,
        &format!(
            "printf x >> '{}'; printf 'remote output'; printf 'diagnostic' >&2; exit 0",
            count.display()
        ),
    )
    .await;
    let id = save(&first).await;
    let input = RunInput {
        target_id: id,
        request_key: "run-once".into(),
        command: "uptime".into(),
        timeout_ms: None,
    };
    let started = first.run("user:alice", input.clone(), true).await.unwrap();
    let execution_id = started["execution_id"].as_str().unwrap();
    let finished = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let receipt = first.status("user:alice", execution_id).await.unwrap();
            if receipt["status"] == "succeeded" {
                break receipt;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(finished["stdout"], "remote output");
    assert_eq!(finished["stderr"], "diagnostic");
    assert_eq!(finished["exit_code"], 0);
    let reopened = host(&root).await;
    let replay = reopened.run("user:alice", input, true).await.unwrap();
    assert_eq!(replay, finished);
    assert_eq!(std::fs::read_to_string(count).unwrap(), "x");
}

#[cfg(unix)]
#[tokio::test]
async fn ssh_transport_failure_is_unknown_and_an_exit_failure_is_failed() {
    for (code, expected) in [(255, "unknown"), (7, "failed")] {
        let root = TempDir::new().unwrap();
        let first = fake_host(&root, &format!("exit {code}")).await;
        let id = save(&first).await;
        let (job, _) = first
            .reserve(
                "user:alice",
                &id,
                "failure",
                Some("remote-operation".into()),
                None,
            )
            .await
            .unwrap();
        first
            .execute(
                first.access("user:alice", &id, true).await.unwrap(),
                job.clone(),
            )
            .await
            .unwrap();
        assert_eq!(
            first.status("user:alice", &job.execution_id).await.unwrap()["status"],
            expected
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn remote_timeout_never_claims_the_operation_was_cancelled() {
    let root = TempDir::new().unwrap();
    let first = fake_host(&root, "exec sleep 5").await;
    let id = save(&first).await;
    let (job, _) = first
        .reserve(
            "user:alice",
            &id,
            "slow",
            Some("slow remote operation".into()),
            Some(10),
        )
        .await
        .unwrap();
    first
        .execute(
            first.access("user:alice", &id, true).await.unwrap(),
            job.clone(),
        )
        .await
        .unwrap();
    let receipt = first.status("user:alice", &job.execution_id).await.unwrap();
    assert_eq!(receipt["status"], "unknown");
    assert_eq!(receipt["remote_outcome_unknown"], true);
}

#[test]
fn setup_routes_require_native_operator_access() {
    for entry in surface().inventory().entries() {
        assert_eq!(entry.required_capability, Some("admin.runtime"));
        assert_eq!(
            entry.browser_policy,
            crate::daemon::route_policy::BrowserPolicy::NativeOnly
        );
        assert!(!entry.bootstrap_public);
    }
}
