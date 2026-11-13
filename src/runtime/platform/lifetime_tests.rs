//! Real daemon constructor/platform lifetime regressions. Run in a hermetic,
//! isolated lib-test process; constructor registration touches process globals.
use super::*;
use std::ffi::OsString;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Clone, Copy)]
enum Action {
    Continue,
    Fail,
    Pause,
}

struct Registered {
    agent: Arc<MedousaAgentRuntime>,
    sender: mpsc::Sender<RuntimeEvent>,
}

struct Hook {
    action: Action,
    reached: Mutex<Option<oneshot::Sender<Registered>>>,
}

tokio::task_local! {
    static HOOK: Hook;
}

pub(super) async fn after_handler_registration(
    agent: &Arc<MedousaAgentRuntime>,
    sender: mpsc::Sender<RuntimeEvent>,
) -> Result<()> {
    let action = HOOK.try_with(|hook| {
        if let Some(reached) = hook.reached.lock().unwrap().take() {
            assert!(
                reached
                    .send(Registered {
                        agent: agent.clone(),
                        sender
                    })
                    .is_ok()
            );
        }
        hook.action
    });
    match action {
        Ok(Action::Fail) => {
            anyhow::bail!("injected late constructor failure after handler registration")
        }
        Ok(Action::Pause) => std::future::pending::<Result<()>>().await,
        _ => Ok(()),
    }
}

struct NoRemoteWork;

#[async_trait::async_trait]
impl crate::delegated_task::DelegatedTaskTransport for NoRemoteWork {
    async fn submit_or_observe(
        &self,
        _target: &crate::delegation::DelegationTarget,
        _request: crate::delegated_task::DelegatedTaskRequest,
    ) -> std::result::Result<
        crate::delegated_task::DelegatedTaskObservation,
        crate::delegated_task::DelegatedTaskError,
    > {
        panic!("lifetime tests must not execute remote work")
    }
}

// Hold the existing suite environment lock throughout each current-thread test.
// The scoped data/vault roots also apply to the polled constructor and tool.
struct Fixture {
    _data: crate::paths::TestDataDirGuard,
    _directory: tempfile::TempDir,
    previous_env: Vec<(&'static str, Option<OsString>)>,
    _lock: crate::test_env::EnvLock,
}

// Resolve only our existing trusted temporary directory, before appending vault.
// macOS temp paths commonly contain /var -> /private/var; production vault roots
// intentionally reject symlink ancestors. Keep TempDir alive for cleanup.
fn canonical_fixture_root(directory: &tempfile::TempDir) -> std::path::PathBuf {
    directory
        .path()
        .canonicalize()
        .expect("canonical fixture root")
}

#[cfg(unix)]
#[test]
fn fixture_root_resolves_symlink_ancestor_and_retains_owner() {
    let parent = tempfile::tempdir().unwrap();
    let canonical_parent = parent.path().canonicalize().unwrap();
    let target = canonical_parent.join("target");
    std::fs::create_dir(&target).unwrap();
    let alias = canonical_parent.join("alias");
    std::os::unix::fs::symlink(&target, &alias).unwrap();
    let directory = tempfile::tempdir_in(&alias).unwrap();
    assert!(directory.path().starts_with(&alias));
    let root = canonical_fixture_root(&directory);
    assert!(root.starts_with(&target));
    assert_eq!(root.canonicalize().unwrap(), root);
    assert!(
        root.exists(),
        "canonicalization must retain the TempDir owner"
    );
    drop(directory);
    assert!(
        !root.exists(),
        "TempDir must still remove the canonical directory"
    );
}

impl Fixture {
    fn new() -> Self {
        let lock = crate::test_env::lock();
        let directory = tempfile::tempdir().unwrap();
        let root = canonical_fixture_root(&directory);
        let data = crate::paths::scoped_test_data_dir(&root);
        crate::vault::roots::set_test_vault_root_override(Some(root.join("vault")));
        let mut previous_env = Vec::new();
        for (key, value) in [
            ("MEDOUSA_TEST_HERMETIC", Some("1")),
            // Port zero is never a live host gateway. No remote MCP catalog is needed.
            ("MEDOUSA_MCP_GATEWAY_URL", Some("http://127.0.0.1:0")),
            // Constructor writes this; restore it even on assertion failure.
            ("STASIS_OTEL_ENABLED", None),
        ] {
            previous_env.push((key, std::env::var_os(key)));
            if let Some(value) = value {
                // SAFETY: suite environment lock held until Fixture::drop.
                unsafe { std::env::set_var(key, value) };
            }
        }
        let installation = medousa_types::secrets::InstallationId::parse(
            crate::workshop_authority::TEST_INSTALLATION_ID,
        )
        .unwrap();
        crate::workshop_authority::initialize(&installation).unwrap();
        Self {
            _data: data,
            _directory: directory,
            previous_env,
            _lock: lock,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        crate::vault::roots::set_test_vault_root_override(None);
        for (key, previous) in &self.previous_env {
            // SAFETY: suite environment lock remains held.
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

fn construction(
    action: Action,
) -> (
    impl std::future::Future<Output = Result<Arc<MedousaPlatformRuntime>>>,
    oneshot::Receiver<Registered>,
) {
    let (reached, receiver) = oneshot::channel();
    let mut config = PlatformBuildConfig::daemon_defaults("http://127.0.0.1:0/delivery");
    config.provider = Some("ollama".into());
    config.model = Some("lifetime-test-no-inference".into());
    config.base_url = Some("http://127.0.0.1:0".into());
    let future = HOOK.scope(
        Hook {
            action,
            reached: Mutex::new(Some(reached)),
        },
        build_daemon_platform(RuntimeBackend::InMemory, config, Arc::new(NoRemoteWork)),
    );
    (future, receiver)
}

async fn calendar_operation(agent: &MedousaAgentRuntime) -> serde_json::Value {
    // This is the actual registered public tool, not a manufactured send-only probe.
    // Its CalendarList producer emits ToolInvoked with try_send and must not fail
    // the operation when the observation receiver has disappeared.
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        agent.tool_registry.invoke_tool(
            crate::public_api::COGNITION_CALENDAR_QUERY,
            serde_json::json!({"action": "calendar.list", "path": "calendar/lifetime.ics",
            "from": "2026-01-01T00:00:00Z", "to": "2026-01-02T00:00:00Z"}),
        ),
    )
    .await
    .expect("calendar operation must not block on observations")
    .expect("calendar operation must succeed despite lost observations");
    assert_eq!(output["calendar_path"], "calendar/lifetime.ics");
    assert_eq!(output["events"], serde_json::json!([]));
    output
}

async fn closed_with_live_agent(registered: &Registered) {
    tokio::time::timeout(Duration::from_secs(5), registered.sender.closed())
        .await
        .expect("drain abort must eventually close the receiver with live agent Arcs");
    // Receiver abortion is asynchronous, not a synchronous Drop guarantee.
    assert!(
        registered
            .sender
            .send(RuntimeEvent::UiNotice("late observation".into()))
            .await
            .is_err()
    );
    calendar_operation(&registered.agent).await;
}

#[tokio::test]
#[ignore = "process-global constructor state; run scripts/ci/test-platform-lifetime.sh"]
async fn retained_agent_survives_final_platform_drop_without_observation_failure() {
    let _fixture = Fixture::new();
    let (future, reached) = construction(Action::Continue);
    let platform = tokio::time::timeout(Duration::from_secs(30), future)
        .await
        .expect("real constructor timed out")
        .expect("real constructor failed");
    let registered = reached.await.unwrap();
    let agent = platform.agent_handle();
    assert!(Arc::ptr_eq(&agent, &registered.agent));
    let other_platform_owner = platform.clone();
    drop(platform);
    assert!(
        !registered.sender.is_closed(),
        "nonfinal platform drop must not abort drain"
    );
    let before = calendar_operation(&agent).await;
    drop(other_platform_owner); // Final actual platform owner; agent remains live.
    closed_with_live_agent(&registered).await;
    assert_eq!(calendar_operation(&agent).await, before);
}

#[tokio::test]
#[ignore = "process-global constructor state; run scripts/ci/test-platform-lifetime.sh"]
async fn late_actual_constructor_failure_closes_observations_after_handler_registration() {
    let _fixture = Fixture::new();
    let (future, reached) = construction(Action::Fail);
    let result = tokio::time::timeout(Duration::from_secs(30), future)
        .await
        .expect("real constructor timed out");
    let error = result
        .err()
        .expect("injected late failure must be returned");
    assert!(
        error
            .to_string()
            .contains("injected late constructor failure")
    );
    let registered = reached
        .await
        .expect("must reach real handler registration first");
    closed_with_live_agent(&registered).await;
}

#[tokio::test]
#[ignore = "process-global constructor state; run scripts/ci/test-platform-lifetime.sh"]
async fn polled_actual_constructor_cancellation_closes_observations_after_handler_registration() {
    let _fixture = Fixture::new();
    let (future, reached) = construction(Action::Pause);
    let registered = {
        tokio::pin!(future);
        tokio::select! {
            result = &mut future => panic!("paused constructor unexpectedly completed: {}", result.is_ok()),
            reached = tokio::time::timeout(Duration::from_secs(30), reached) =>
                reached.expect("constructor never reached real handler registration").unwrap(),
        }
        // Dropping this POLLED constructor cancels it, with its agent retained by
        // actual registered handlers and by Registered. No unpolled synthetic future.
    };
    closed_with_live_agent(&registered).await;
}
