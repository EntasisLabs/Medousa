//! All config lives in temp directories. Bootstrap uses fake effects only.
use crate::{
    connection::*, paths::resolve_data_dir_with, workshop_registry::*, workshop_runtime::*,
};
use std::{collections::VecDeque, path::PathBuf};

fn save(root: &std::path::Path, registry: &WorkshopRegistry) {
    std::fs::write(
        root.join("workshops.json"),
        serde_json::to_vec(registry).unwrap(),
    )
    .unwrap();
}
#[test]
fn root_precedence_env_redirect_default_no_scanning() {
    let default = PathBuf::from("/fixture/default");
    assert_eq!(
        resolve_data_dir_with(default.clone(), Some(" /fixture/env "), |_| panic!(
            "env must skip redirect"
        )),
        PathBuf::from("/fixture/env")
    );
    assert_eq!(
        resolve_data_dir_with(default.clone(), Some("  "), |path| {
            assert_eq!(path, default.join("data_dir"));
            Some(" /fixture/redirect\n".into())
        }),
        PathBuf::from("/fixture/redirect")
    );
    assert_eq!(
        resolve_data_dir_with(default.clone(), None, |_| Some(" \n".into())),
        default
    );
    assert_eq!(
        resolve_data_dir_with(default.clone(), None, |_| None),
        default
    );
}
#[test]
fn no_registry_uses_personal_7419_without_writes_or_legacy_secret_reads() {
    let temp = tempfile::tempdir().unwrap();
    // These malformed legacy/credential files must not be parsed or migrated.
    std::fs::write(temp.path().join("pairing_credentials.json"), "not json").unwrap();
    std::fs::write(
        temp.path().join("home_daemon_url.txt"),
        "https://remote.invalid",
    )
    .unwrap();
    let c = resolve_local(temp.path(), Overrides::default()).unwrap();
    assert_eq!(c.origin, "http://127.0.0.1:7419/");
    assert_eq!(c.data_dir, temp.path());
    assert!(c.allow_bootstrap);
    assert!(!temp.path().join("workshops.json").exists());
    assert!(!temp.path().join("credentials").exists());
}
#[test]
fn active_local_bind_and_root_win_over_stale_url_and_personal_data() {
    let temp = tempfile::tempdir().unwrap();
    let mut registry = default_registry();
    registry.workshops[0].url = "http://remote.invalid".into();
    registry.workshops[0].data_dir = Some("/ignored/personal".into());
    registry.workshops[0].bind = Some("0.0.0.0:7450".into());
    save(temp.path(), &registry);
    let c = resolve_local(temp.path(), Overrides::default()).unwrap();
    assert_eq!(c.origin, "http://127.0.0.1:7450/");
    assert_eq!(c.data_dir, temp.path());
    let mut other = default_personal_workshop();
    other.id = "secondary".into();
    other.bind = Some("[::1]:7460".into());
    other.data_dir = Some(" /fixture/secondary ".into());
    registry.active_workshop_id = other.id.clone();
    registry.workshops.push(other);
    save(temp.path(), &registry);
    let c = resolve_local(temp.path(), Overrides::default()).unwrap();
    assert_eq!(c.origin, "http://[::1]:7460/");
    assert_eq!(c.data_dir, PathBuf::from("/fixture/secondary"));
    // Registry stays untouched: no user config write during discovery.
    let after: WorkshopRegistry =
        serde_json::from_slice(&std::fs::read(temp.path().join("workshops.json")).unwrap())
            .unwrap();
    assert_eq!(
        after.workshops[0].data_dir.as_deref(),
        Some("/ignored/personal")
    );
}
#[test]
fn explicit_overrides_and_missing_bind() {
    let temp = tempfile::tempdir().unwrap();
    let mut registry = default_registry();
    registry.workshops[0].bind = None;
    save(temp.path(), &registry);
    assert_eq!(
        resolve_local(temp.path(), Overrides::default())
            .unwrap()
            .origin,
        "http://127.0.0.1:7419/"
    );
    let c = resolve_local(
        temp.path(),
        Overrides {
            daemon: Some("http://localhost:9999".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(c.origin, "http://localhost:9999/");
    assert_eq!(c.data_dir, temp.path());
    assert!(!c.allow_bootstrap);
    // Explicit engine root bypasses the app registry, including malformed config.
    std::fs::write(temp.path().join("workshops.json"), "bad config").unwrap();
    let c = resolve_local(
        temp.path(),
        Overrides {
            data_dir: Some("/fixture/explicit".into()),
            daemon: None,
        },
    )
    .unwrap();
    assert_eq!(c.data_dir, PathBuf::from("/fixture/explicit"));
    assert_eq!(c.origin, "http://127.0.0.1:7419/");
}
#[test]
fn invalid_config_remote_registry_and_nonlocal_origins_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("workshops.json"),
        "secret-looking malformed input",
    )
    .unwrap();
    let error = resolve_local(temp.path(), Overrides::default())
        .err()
        .unwrap();
    assert!(!error.contains("secret-looking"));
    let mut registry = default_registry();
    registry.version = 99;
    save(temp.path(), &registry);
    assert!(resolve_local(temp.path(), Overrides::default()).is_err());
    registry.version = 1;
    registry.workshops[0].kind = "portal".into();
    save(temp.path(), &registry);
    assert!(
        resolve_local(temp.path(), Overrides::default())
            .err()
            .unwrap()
            .contains("remote")
    );
    registry.workshops[0].kind = "local".into();
    registry.workshops[0].bind = Some("example.invalid:7419".into());
    save(temp.path(), &registry);
    assert!(resolve_local(temp.path(), Overrides::default()).is_err());
    for origin in [
        "not-url",
        "file:///tmp/x",
        "http://example.invalid",
        "http://127.0.0.1.evil.invalid",
        "http://user:synthetic@localhost",
        "http://localhost/path",
        "http://localhost?token=synthetic",
        "http://localhost#x",
    ] {
        let result = resolve_local(
            temp.path(),
            Overrides {
                data_dir: Some(temp.path().into()),
                daemon: Some(origin.into()),
            },
        );
        assert!(result.is_err(), "accepted unsafe origin");
        assert!(!result.err().unwrap().contains("synthetic"));
    }
}
#[test]
fn peer_active_falls_back_to_personal_and_paired_never_uses_local_auth() {
    let temp = tempfile::tempdir().unwrap();
    let mut registry = default_registry();
    let mut peer = default_personal_workshop();
    peer.id = "peer".into();
    peer.kind = "peer".into();
    registry.active_workshop_id = peer.id.clone();
    registry.workshops.push(peer);
    save(temp.path(), &registry);
    assert_eq!(
        resolve_local(temp.path(), Overrides::default())
            .unwrap()
            .workshop
            .id,
        "personal"
    );
    registry.workshops[1].kind = "paired".into();
    save(temp.path(), &registry);
    assert!(resolve_local(temp.path(), Overrides::default()).is_err());
}

struct Fake {
    healthy: bool,
    reachable: bool,
    waits: VecDeque<bool>,
    fail_spawn: bool,
    calls: Vec<&'static str>,
}
impl EngineEffects for Fake {
    async fn healthy(&mut self) -> bool {
        self.calls.push("health");
        self.healthy
    }
    fn reachable(&mut self) -> bool {
        self.calls.push("reachable");
        self.reachable
    }
    async fn wait_healthy(&mut self) -> Result<bool, String> {
        self.calls.push("wait");
        Ok(self.waits.pop_front().expect("unexpected wait"))
    }
    async fn stop_and_pause(&mut self) {
        self.calls.push("stop+pause");
    }
    fn spawn(&mut self) -> Result<(u32, PathBuf), String> {
        self.calls.push("spawn-core-only");
        if self.fail_spawn {
            Err("Fixture binary unavailable".into())
        } else {
            Ok((42, "fixture.log".into()))
        }
    }
}
#[test]
fn readiness_reuses_waits_bootstraps_and_reports_unavailable_without_real_effects() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for (healthy, reachable, waits, fail_spawn, expected_calls, ready) in [
        (true, false, vec![], false, vec!["health"], true),
        (
            false,
            true,
            vec![true],
            false,
            vec!["health", "reachable", "wait"],
            true,
        ),
        (
            false,
            false,
            vec![true],
            false,
            vec!["health", "reachable", "spawn-core-only", "wait"],
            true,
        ),
        (
            false,
            true,
            vec![false, true],
            false,
            vec![
                "health",
                "reachable",
                "wait",
                "stop+pause",
                "spawn-core-only",
                "wait",
            ],
            true,
        ),
        (
            false,
            false,
            vec![false],
            false,
            vec!["health", "reachable", "spawn-core-only", "wait"],
            false,
        ),
        (
            false,
            false,
            vec![],
            true,
            vec!["health", "reachable", "spawn-core-only"],
            false,
        ),
    ] {
        let mut fake = Fake {
            healthy,
            reachable,
            waits: waits.into(),
            fail_spawn,
            calls: vec![],
        };
        let result = rt.block_on(ensure_engine_ready_with(&mut fake));
        assert_eq!(fake.calls, expected_calls);
        match result {
            Ok(EngineReadiness::Running { .. }) => assert!(ready),
            Ok(EngineReadiness::Started { healthy, .. }) => assert_eq!(healthy, ready),
            Err(_) => assert!(fail_spawn),
        }
    }
}

#[test]
fn overlays_first_readable_wins_and_existing_env_is_never_overridden() {
    use crate::paths::{apply_env_lines, load_env_overlay_with};
    let temp = tempfile::tempdir().unwrap();
    let explicit = temp.path().join("explicit.env");
    let config = temp.path().join("config.env");
    let data = temp.path().join("data.env");
    for path in [&explicit, &config, &data] {
        std::fs::write(path, "fixture").unwrap();
    }
    let mut applied = vec![];
    let chosen = load_env_overlay_with(
        vec![explicit.clone(), config.clone(), data.clone()],
        |path| {
            applied.push(path.to_path_buf());
            Ok(())
        },
    );
    assert_eq!(chosen, Some(explicit.clone()));
    assert_eq!(applied, vec![explicit.clone()]);
    let chosen = load_env_overlay_with(vec![explicit.clone(), config.clone(), data], |path| {
        if path == explicit {
            Err(std::io::Error::other("fixture unreadable"))
        } else {
            Ok(())
        }
    });
    assert_eq!(chosen, Some(config));
    let mut env =
        std::collections::HashMap::from([("EXISTING".to_string(), "process".to_string())]);
    apply_env_lines(
        "# fixture\nEXISTING=overlay\nNEW='first'\nNEW=second\nINVALID\n =empty\n",
        |key, value| {
            env.entry(key.to_string())
                .or_insert_with(|| value.to_string());
        },
    );
    assert_eq!(env["EXISTING"], "process");
    assert_eq!(env["NEW"], "first");
    assert_eq!(env.len(), 2);
}

#[test]
fn forged_or_mismatched_bootstrap_plans_fail_before_any_effect() {
    let temp = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut c = resolve_local(temp.path(), Overrides::default()).unwrap();
    c.origin = "http://remote.invalid/".into();
    assert!(rt.block_on(c.ensure_ready()).is_err());
    c.origin = "http://127.0.0.1:9999/".into();
    assert_eq!(
        rt.block_on(c.ensure_ready()).unwrap_err(),
        "Local connection/bootstrap binding mismatch"
    );
    c.origin = "http://127.0.0.1:7419/".into();
    c.data_dir = temp.path().join("mismatch");
    assert_eq!(
        rt.block_on(c.ensure_ready()).unwrap_err(),
        "Local connection/bootstrap binding mismatch"
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
