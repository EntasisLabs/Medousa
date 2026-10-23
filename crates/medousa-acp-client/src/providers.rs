//! Launch profiles for external agents. Native credentials and configuration
//! remain owned by each vendor; Medousa never substitutes a model API key.

use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::{AcpAgentConfig, AgentRuntimeKind, RuntimeAuthProbe, RuntimeAuthStatus};

pub const CLAUDE_ACP_PACKAGE: &str = "@agentclientprotocol/claude-agent-acp@0.94.0";
pub const PI_ACP_PACKAGE: &str = "pi-acp@0.0.34";

#[derive(Clone, Copy)]
struct Profile {
    kind: AgentRuntimeKind,
    name: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    bridge: Option<(&'static str, &'static str)>,
    bridge_requires_native: bool,
    override_prefix: &'static str,
    linux_only: bool,
    setup: &'static str,
    limitations: Option<&'static str>,
}

fn profile(kind: AgentRuntimeKind) -> Option<Profile> {
    Some(match kind {
        AgentRuntimeKind::Claude => Profile {
            kind,
            name: "Claude Code",
            command: "claude",
            args: &[],
            bridge: Some(("claude-agent-acp", CLAUDE_ACP_PACKAGE)),
            bridge_requires_native: false,
            override_prefix: "MEDOUSA_ACP_CLAUDE",
            linux_only: false,
            setup: "Install Claude Code and sign in using `claude auth login`; install claude-agent-acp, or Node.js 22+ and npx for the pinned adapter. The adapter uses the existing account and user/project/local settings.",
            limitations: None,
        },
        AgentRuntimeKind::Devin => Profile {
            kind,
            name: "Devin",
            command: "devin",
            args: &["acp"],
            bridge: None,
            bridge_requires_native: false,
            override_prefix: "MEDOUSA_ACP_DEVIN",
            linux_only: false,
            setup: "Install Devin CLI and complete its native sign-in before starting a session.",
            limitations: None,
        },
        AgentRuntimeKind::Pi => Profile {
            kind,
            name: "Pi",
            command: "pi",
            args: &[],
            bridge: Some(("pi-acp", PI_ACP_PACKAGE)),
            bridge_requires_native: true,
            override_prefix: "MEDOUSA_ACP_PI",
            linux_only: false,
            setup: "Install Pi v0.81.0+ and configure its native provider login; install pi-acp, or Node.js 22+ and npx to launch the pinned adapter.",
            limitations: Some(
                "Pi ACP supports selection/confirmation dialogs, but not extension input/editor dialogs, extension slash commands, or MCP forwarding; reasoning is combined with the answer stream.",
            ),
        },
        AgentRuntimeKind::MuseCode => Profile {
            kind,
            name: "Muse Code",
            command: "muse",
            args: &["serve"],
            bridge: None,
            bridge_requires_native: false,
            override_prefix: "MEDOUSA_ACP_MUSE_CODE",
            linux_only: false,
            setup: "Install Muse Code and complete its native sign-in; Medousa connects to `muse serve` on the workshop.",
            limitations: None,
        },
        AgentRuntimeKind::Hades => Profile {
            kind,
            name: "Hades",
            command: "hades",
            args: &["acp"],
            bridge: None,
            bridge_requires_native: false,
            override_prefix: "MEDOUSA_ACP_HADES",
            linux_only: true,
            setup: "Install and configure Hades on a Linux workshop; Medousa starts `hades acp` using that workshop's account and files.",
            limitations: Some(
                "Hades does not advertise session loading; a saved conversation is not a resumable Hades execution.",
            ),
        },
        _ => return None,
    })
}

/// Provider limitations that clients should not mistake for missing sign-in.
pub fn limitations(kind: AgentRuntimeKind) -> Option<&'static str> {
    profile(kind).and_then(|profile| profile.limitations)
}

struct Launch {
    config: AcpAgentConfig,
    available: bool,
    binary_present: bool,
    detail: String,
}

fn resolve_launch(
    profile: Profile,
    command_override: Option<String>,
    args_override: Option<Vec<String>>,
    platform: &str,
    resolve: impl Fn(&str) -> Option<PathBuf>,
) -> Launch {
    let command_override = command_override.filter(|value| !value.trim().is_empty());
    let native = resolve(profile.command);
    let overridden = command_override.is_some();
    let (command, default_args, ready, detail) = if let Some(command) = command_override {
        let resolved = resolve(&command);
        let ready = resolved.is_some();
        (
            resolved
                .map(|path| path.display().to_string())
                .unwrap_or(command),
            profile.args.iter().map(|arg| (*arg).to_string()).collect(),
            ready,
            if ready {
                "Configured command is available."
            } else {
                "Configured command was not found."
            }
            .to_string(),
        )
    } else if profile.linux_only && platform != "linux" {
        (profile.command.to_string(), profile.args.iter().map(|arg| (*arg).to_string()).collect(), false,
            "Hades currently requires a Linux workshop. A configured command override can launch a compatible wrapper.".to_string())
    } else if let Some((bridge, package)) = profile.bridge {
        if let Some(installed) = resolve(bridge) {
            (
                installed.display().to_string(),
                Vec::new(),
                !profile.bridge_requires_native || native.is_some(),
                if profile.bridge_requires_native && native.is_none() {
                    profile.setup.to_string()
                } else {
                    "Installed ACP adapter is available.".to_string()
                },
            )
        } else if let (Some(_), Some(npx), Some(_)) = (&native, resolve("npx"), resolve("node")) {
            (
                npx.display().to_string(),
                vec!["-y".into(), package.into()],
                true,
                format!(
                    "Native CLI is available; starts the pinned ACP adapter {package} with npx."
                ),
            )
        } else {
            (
                bridge.to_string(),
                Vec::new(),
                false,
                profile.setup.to_string(),
            )
        }
    } else if let Some(native) = &native {
        (
            native.display().to_string(),
            profile.args.iter().map(|arg| (*arg).to_string()).collect(),
            true,
            "Native CLI is available.".to_string(),
        )
    } else {
        (
            profile.command.to_string(),
            profile.args.iter().map(|arg| (*arg).to_string()).collect(),
            false,
            profile.setup.to_string(),
        )
    };

    let detail = match profile.limitations {
        Some(limitations) => format!("{detail} {limitations}"),
        None => detail,
    };
    Launch {
        config: AcpAgentConfig {
            kind: profile.kind,
            command,
            args: args_override.unwrap_or(default_args),
            cwd: None,
        },
        available: ready,
        binary_present: native.is_some() || (ready && (overridden || profile.bridge.is_some())),
        detail,
    }
}

fn configured_launch(kind: AgentRuntimeKind) -> Result<Launch> {
    let Some(profile) = profile(kind) else {
        bail!(
            "{} does not use an expanded external-agent profile",
            kind.as_str()
        );
    };
    let command = std::env::var(format!("{}_COMMAND", profile.override_prefix)).ok();
    let args_key = format!("{}_ARGS", profile.override_prefix);
    let args = std::env::var(&args_key)
        .ok()
        .map(|_| crate::env_args(&args_key, &[]));
    Ok(resolve_launch(
        profile,
        command,
        args,
        std::env::consts::OS,
        crate::resolve_command_path,
    ))
}

pub fn runtime_config(kind: AgentRuntimeKind) -> Result<AcpAgentConfig> {
    Ok(configured_launch(kind)?.config)
}

pub fn availability(kind: AgentRuntimeKind) -> (bool, Option<String>, Option<String>) {
    match configured_launch(kind) {
        Ok(launch) => (
            launch.available,
            Some(launch.config.command),
            Some(launch.detail),
        ),
        Err(error) => (false, None, Some(error.to_string())),
    }
}

/// A launchable CLI or credentials file is not proof that a vendor login is
/// valid. Keep authentication unknown until the vendor verifies it at startup.
pub fn auth_probe(kind: AgentRuntimeKind) -> RuntimeAuthProbe {
    match (profile(kind), configured_launch(kind)) {
        (Some(profile), Ok(launch)) => RuntimeAuthProbe {
            status: RuntimeAuthStatus::Unknown,
            binary_present: launch.binary_present,
            detail: Some(if launch.available {
                format!(
                    "{} uses its native sign-in; credentials have not been verified. {}",
                    profile.name, profile.setup
                )
            } else {
                launch.detail
            }),
        },
        (_, Err(error)) => RuntimeAuthProbe {
            status: RuntimeAuthStatus::Unknown,
            binary_present: false,
            detail: Some(error.to_string()),
        },
        _ => unreachable!("configured launch requires a provider profile"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(kind: AgentRuntimeKind, installed: &[&str], platform: &str) -> Launch {
        resolve_launch(profile(kind).unwrap(), None, None, platform, |name| {
            installed
                .contains(&name)
                .then(|| PathBuf::from("/vendor/bin").join(name))
        })
    }

    #[test]
    fn runtime_ids_roundtrip_without_changing_existing_ids() {
        for (kind, wire) in [
            (AgentRuntimeKind::Claude, "claude"),
            (AgentRuntimeKind::Devin, "devin"),
            (AgentRuntimeKind::Pi, "pi"),
            (AgentRuntimeKind::MuseCode, "muse_code"),
            (AgentRuntimeKind::Hades, "hades"),
            (AgentRuntimeKind::Codex, "codex"),
            (AgentRuntimeKind::Hermes, "hermes"),
            (AgentRuntimeKind::Cursor, "cursor"),
        ] {
            assert_eq!(AgentRuntimeKind::parse(wire), Some(kind));
            assert_eq!(kind.as_str(), wire);
            assert_eq!(serde_json::to_value(kind).unwrap(), serde_json::json!(wire));
            assert_eq!(
                serde_json::from_value::<AgentRuntimeKind>(serde_json::json!(wire)).unwrap(),
                kind
            );
        }
    }

    #[test]
    fn installed_bridges_are_preferred_over_npx() {
        for (kind, native, bridge) in [
            (AgentRuntimeKind::Claude, "claude", "claude-agent-acp"),
            (AgentRuntimeKind::Pi, "pi", "pi-acp"),
        ] {
            let launch = launch(kind, &[native, bridge, "npx", "node"], "macos");
            assert!(launch.available);
            assert!(launch.config.command.ends_with(bridge));
            assert!(launch.config.args.is_empty());
        }
    }

    #[test]
    fn npx_fallback_is_pinned_and_requires_native_cli_and_node() {
        for (kind, native, package) in [
            (AgentRuntimeKind::Claude, "claude", CLAUDE_ACP_PACKAGE),
            (AgentRuntimeKind::Pi, "pi", PI_ACP_PACKAGE),
        ] {
            let ready = launch(kind, &[native, "npx", "node"], "linux");
            assert!(ready.available);
            assert_eq!(ready.config.args, ["-y", package]);
            for installed in [
                vec!["npx", "node"],
                vec![native, "npx"],
                vec![native, "node"],
            ] {
                assert!(!launch(kind, &installed, "linux").available);
            }
        }
    }

    #[test]
    fn pi_bridge_cannot_run_without_the_native_pi_cli() {
        let installed = launch(AgentRuntimeKind::Pi, &["pi-acp", "node", "npx"], "linux");
        assert!(!installed.available);
        assert!(!installed.binary_present);
        assert!(installed.detail.contains("Install Pi"));
    }

    #[test]
    fn direct_protocol_commands_keep_native_args() {
        for (kind, binary, arg) in [
            (AgentRuntimeKind::Devin, "devin", "acp"),
            (AgentRuntimeKind::Hades, "hades", "acp"),
            (AgentRuntimeKind::MuseCode, "muse", "serve"),
        ] {
            let launch = launch(kind, &[binary], "linux");
            assert!(launch.available);
            assert_eq!(launch.config.args, [arg]);
        }
    }

    #[test]
    fn hades_platform_limit_is_explicit_and_override_can_use_a_wrapper() {
        let unsupported = launch(AgentRuntimeKind::Hades, &["hades"], "macos");
        assert!(!unsupported.available);
        assert!(unsupported.detail.contains("Linux workshop"));
        let custom = resolve_launch(
            profile(AgentRuntimeKind::Hades).unwrap(),
            Some("wrapper".into()),
            Some(vec!["hades".into(), "acp".into()]),
            "macos",
            |name| (name == "wrapper").then(|| PathBuf::from("/wrapper")),
        );
        assert!(custom.available);
        assert_eq!(custom.config.command, "/wrapper");
        assert_eq!(custom.config.args, ["hades", "acp"]);
    }

    #[test]
    fn pi_limitations_are_visible_and_no_native_login_is_claimed_verified() {
        let pi = launch(AgentRuntimeKind::Pi, &["pi", "pi-acp"], "linux");
        assert!(pi.detail.contains("input/editor"));
        assert!(pi.detail.contains("MCP forwarding"));
        assert_eq!(
            auth_probe(AgentRuntimeKind::Pi).status,
            RuntimeAuthStatus::Unknown
        );
    }
}
