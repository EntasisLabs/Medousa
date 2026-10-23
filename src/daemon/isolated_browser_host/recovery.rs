//! Reclaim only Chromium processes orphaned by a previous daemon, never another
//! live daemon's browser or the user's regular browser profile.
use super::*;
use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

fn execution() -> &'static ForgeExecutionService {
    static SERVICE: OnceLock<ForgeExecutionService> = OnceLock::new();
    SERVICE.get_or_init(ForgeExecutionService::new)
}

pub(super) async fn release_orphaned_profile(
    profile: &Path,
    binary: &Path,
) -> Result<(), IsolatedBrowserError> {
    let profile = profile.to_owned();
    let binary = binary.to_owned();
    execution()
        .run(ExecutionClass::WorkEnvironment, 64 * 1024, move || {
            Ok(release_orphan(&profile, &binary))
        })
        .await
        .map_err(|e| IsolatedBrowserError::Unavailable(e.to_string()))??;
    Ok(())
}

fn release_orphan(profile: &Path, binary: &Path) -> Result<(), IsolatedBrowserError> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::Always)
            .with_exe(UpdateKind::Always)
            .with_user(UpdateKind::Always),
    );
    let own_user = system
        .process(sysinfo::Pid::from_u32(std::process::id()))
        .and_then(|p| p.user_id());
    let profile_arg = format!("--user-data-dir={}", profile.display());
    let mut killed = Vec::new();
    for process in system.processes().values() {
        let args = process.cmd();
        if process.exe() != Some(binary)
            || own_user.is_none()
            || process.user_id() != own_user
            || !args.iter().any(|a| a == profile_arg.as_str())
            || !args.iter().any(|a| a == "--remote-debugging-port=0")
            || !args
                .iter()
                .any(|a| a == "--remote-debugging-address=127.0.0.1")
            || args
                .iter()
                .any(|a| a.to_string_lossy().starts_with("--type="))
        {
            continue;
        }
        // On Unix an abandoned daemon child is reparented to init. Elsewhere
        // the recorded parent must no longer exist. Refuse live-owner recovery.
        let orphan = process
            .parent()
            .is_none_or(|parent| parent.as_u32() == 1 || system.process(parent).is_none());
        if !orphan {
            return Err(IsolatedBrowserError::Conflict(
                "this browser profile is still owned by a running process; stop its workshop before restarting it".into(),
            ));
        }
        let identity = (process.pid(), process.start_time());
        if !process.kill() {
            return Err(IsolatedBrowserError::Unavailable(
                "could not release the browser left by the previous workshop".into(),
            ));
        }
        killed.push(identity);
    }
    for _ in 0..40 {
        system.refresh_processes(ProcessesToUpdate::All, true);
        if killed.iter().all(|(pid, started)| {
            system
                .process(*pid)
                .is_none_or(|p| p.start_time() != *started)
        }) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(IsolatedBrowserError::Unavailable(
        "the previous workshop browser is still shutting down; try again".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires installed Chromium; intentionally exits a child test process"]
    async fn chromium_profile_recovers_after_daemon_exit() {
        const CHILD_ROOT: &str = "MEDOUSA_TEST_ORPHAN_BROWSER_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let host = IsolatedBrowserHost::open(root.into()).await.unwrap();
            host.create(
                "restart-test",
                "restart-workshop",
                CreateIsolatedBrowserWorldRequest {
                    display_name: None,
                    profile: None,
                    initial_url: None,
                    headless: true,
                    view_attached: false,
                },
            )
            .await
            .unwrap();
            // Simulate abrupt daemon exit: static hosts and child handles never drop.
            std::process::exit(0);
        }
        let root = std::env::temp_dir().join(format!("medousa-browser-restart-{}", Uuid::new_v4()));
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "daemon::isolated_browser_host::recovery::tests::chromium_profile_recovers_after_daemon_exit", "--ignored", "--nocapture"])
            .env(CHILD_ROOT, &root).status().await.unwrap();
        assert!(status.success());
        let host = IsolatedBrowserHost::open(root.clone()).await.unwrap();
        let world = host.list("restart-test").await.pop().unwrap();
        assert_eq!(world.run_state, IsolatedBrowserRunState::Stopped);
        assert!(!world.view_attached);
        // Takeover resumes atomically under user control, never returning a
        // half-recovered agent-owned browser or deleting profile contents.
        let running = host
            .lifecycle(
                "restart-test",
                &world.world_id,
                IsolatedBrowserLifecycleAction::Takeover,
            )
            .await
            .unwrap();
        assert_eq!(running.run_state, IsolatedBrowserRunState::Running);
        assert_eq!(running.control, BrowserControl::User);
        assert!(!running.agent_operable());
        host.observe("restart-test", &world.world_id, None, 32)
            .await
            .unwrap();
        // A second host must not terminate a browser with a live owner.
        let profile = host.profile_directory(&running);
        let binary = resolve_browser_binary().unwrap();
        assert!(matches!(
            release_orphaned_profile(&profile, &binary).await,
            Err(IsolatedBrowserError::Conflict(_))
        ));
        assert_eq!(
            host.get("restart-test", &world.world_id)
                .await
                .unwrap()
                .run_state,
            IsolatedBrowserRunState::Running
        );
        host.lifecycle(
            "restart-test",
            &world.world_id,
            IsolatedBrowserLifecycleAction::Stop,
        )
        .await
        .unwrap();
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
