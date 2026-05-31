use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use crossterm::style::Stylize;
use medousa::session::{
    load_discord_bot_token, load_telegram_bot_token, load_tui_api_key, load_tui_defaults,
    save_discord_bot_token, save_telegram_bot_token, save_tui_api_key, save_tui_defaults,
};
use medousa::{
    ProductConfig, apply_adapter_env, format_i64_csv, format_u64_csv, load_product_config,
    migrate_from_onboard_profile, parse_i64_csv, parse_u64_csv, save_product_config,
};
use serde::{Deserialize, Serialize};

const DEFAULT_DAEMON_BIND: &str = "127.0.0.1:7419";
const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7419";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
const DEFAULT_OLLAMA_MODEL: &str = "llama3.2";
const DEFAULT_OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434/v1/";

#[path = "medousa/onboard_wizard/mod.rs"]
mod onboard_wizard;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OnboardProfile {
    daemon_url: Option<String>,
    daemon_backend: Option<String>,
    telegram_allow_user_ids: Option<String>,
}

#[derive(Debug, Clone)]
struct ComponentCommand {
    program: String,
    pre_args: Vec<String>,
}

fn main() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();

    if args.is_empty() {
        print_help();
        return Ok(());
    }

    match args[0].as_str() {
        "onboard" | "setup" | "init" => run_onboard(&args[1..]),
        "tui" => run_tui(&args[1..]),
        "daemon" => run_daemon(&args[1..]),
        "discord" => run_discord(&args[1..]),
        "telegram" => run_telegram(&args[1..]),
        "doctor" => run_doctor(&args[1..]),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(anyhow!(
            "unknown command '{}'. run 'medousa --help' for available commands",
            other
        )),
    }
}

fn run_onboard(args: &[String]) -> Result<()> {
    let started_at = Instant::now();
    let non_interactive = has_flag(args, "--yes");
    let advanced_mode = true;

    let explicit_no_daemon = has_flag(args, "--no-daemon");
    let explicit_daemon = has_flag(args, "--daemon");
    let explicit_no_tui = has_flag(args, "--no-tui");
    let explicit_tui = has_flag(args, "--tui");

    let existing_api_key = load_tui_api_key();
    let existing_discord_token = load_discord_bot_token();
    let existing_telegram_token = load_telegram_bot_token();
    let mut profile = load_onboard_profile();
    let mut defaults = load_tui_defaults();
    let mut product_config = load_product_config();
    migrate_from_onboard_profile(
        &mut product_config,
        profile.telegram_allow_user_ids.as_deref(),
    );

    let initial_telegram_allow_user_ids = if product_config.telegram.allowed_user_ids.is_empty() {
        profile.telegram_allow_user_ids.clone()
    } else {
        Some(format_u64_csv(&product_config.telegram.allowed_user_ids))
    };

    let ollama_detected = detect_local_ollama();
    let detected_provider = if ollama_detected { "ollama" } else { "openai" };

    let initial_provider = find_arg_value(args, "--provider")
        .map(normalize_provider)
        .filter(|value| !value.is_empty())
        .or_else(|| defaults.provider.clone())
        .unwrap_or_else(|| detected_provider.to_string());

    let initial_model = find_arg_value(args, "--model")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| defaults.model.clone())
        .unwrap_or_else(|| default_model_for_provider(&initial_provider).to_string());

    let initial_backend = find_arg_value(args, "--backend")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| profile.daemon_backend.clone())
        .or_else(|| defaults.backend.clone())
        .unwrap_or_else(|| "in-memory".to_string());

    let initial_daemon_url = find_arg_value(args, "--daemon-url")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| profile.daemon_url.clone())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string());

    let mut initial_base_url = find_arg_value(args, "--base-url")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| defaults.base_url.clone());

    let initial_base_url_empty = initial_base_url
        .as_deref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true);
    if initial_provider.eq_ignore_ascii_case("ollama") && initial_base_url_empty {
        initial_base_url = default_base_url_for_provider(&initial_provider);
    }

    let initial_api_key = find_arg_value(args, "--api-key")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    let mut selected = if non_interactive {
        let provider = if initial_provider.eq_ignore_ascii_case("custom") {
            "openai".to_string()
        } else {
            initial_provider.clone()
        };

        onboard_wizard::WizardOutput {
            provider,
            model: initial_model,
            base_url: initial_base_url,
            api_key: initial_api_key,
            backend: initial_backend,
            daemon_url: initial_daemon_url,
            daemon_bind: product_config.daemon.bind.clone(),
            start_daemon: true,
            launch_tui: true,
            configure_discord: false,
            discord_token: None,
            discord_command_prefix: product_config.discord.command_prefix.clone(),
            discord_heartbeat_nudges_enabled: product_config.discord.heartbeat_nudges_enabled,
            discord_heartbeat_channel_ids: if product_config.discord.heartbeat_channel_ids.is_empty()
            {
                None
            } else {
                Some(format_u64_csv(
                    &product_config.discord.heartbeat_channel_ids,
                ))
            },
            start_discord: false,
            configure_telegram: false,
            telegram_token: None,
            telegram_allow_user_ids: initial_telegram_allow_user_ids.clone(),
            telegram_heartbeat_nudges_enabled: product_config.telegram.heartbeat_nudges_enabled,
            telegram_heartbeat_chat_ids: if product_config.telegram.heartbeat_chat_ids.is_empty() {
                None
            } else {
                Some(format_i64_csv(&product_config.telegram.heartbeat_chat_ids))
            },
            start_telegram: false,
            tui_response_depth_mode: product_config.tui.response_depth_mode.clone(),
        }
    } else {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(anyhow!(
                "interactive setup requires a TTY. Use '--yes' for non-interactive mode"
            ));
        }

        let surreal_kv_default_path = medousa_data_dir()
            .join("runtime.surrealkv")
            .to_string_lossy()
            .to_string();
        let bootstrap = onboard_wizard::WizardBootstrap {
            ollama_detected,
            advanced_mode,
            existing_api_key: existing_api_key.is_some(),
            existing_discord_token: existing_discord_token.is_some(),
            existing_telegram_token: existing_telegram_token.is_some(),
            initial_telegram_allow_user_ids,
            initial_daemon_bind: product_config.daemon.bind.clone(),
            initial_discord_command_prefix: product_config.discord.command_prefix.clone(),
            initial_discord_heartbeat_nudges: product_config.discord.heartbeat_nudges_enabled,
            initial_discord_heartbeat_channel_ids: format_u64_csv(
                &product_config.discord.heartbeat_channel_ids,
            ),
            initial_telegram_heartbeat_nudges: product_config.telegram.heartbeat_nudges_enabled,
            initial_telegram_heartbeat_chat_ids: format_i64_csv(
                &product_config.telegram.heartbeat_chat_ids,
            ),
            initial_tui_response_depth: product_config.tui.response_depth_mode.clone(),
            initial_provider,
            initial_model,
            initial_base_url,
            initial_api_key,
            initial_backend,
            initial_daemon_url,
            default_openai_model: DEFAULT_OPENAI_MODEL.to_string(),
            default_ollama_model: DEFAULT_OLLAMA_MODEL.to_string(),
            default_ollama_base_url: default_base_url_for_provider("ollama")
                .unwrap_or_else(|| DEFAULT_OLLAMA_BASE_URL.to_string()),
            surreal_kv_default_path,
            force_daemon: explicit_daemon,
            force_no_daemon: explicit_no_daemon,
            force_tui: explicit_tui,
            force_no_tui: explicit_no_tui,
        };

        match onboard_wizard::run(bootstrap)? {
            Some(output) => output,
            None => {
                println!("{}", "Setup canceled. No changes were saved.".yellow());
                return Ok(());
            }
        }
    };

    if explicit_no_daemon {
        selected.start_daemon = false;
    }
    if explicit_daemon {
        selected.start_daemon = true;
    }
    if explicit_no_tui {
        selected.launch_tui = false;
    }
    if explicit_tui {
        selected.launch_tui = true;
    }

    selected.provider = normalize_provider(&selected.provider);
    if selected.provider.eq_ignore_ascii_case("custom") || selected.provider.trim().is_empty() {
        selected.provider = "openai".to_string();
    }

    if selected.model.trim().is_empty() {
        selected.model = default_model_for_provider(&selected.provider).to_string();
    } else {
        selected.model = selected.model.trim().to_string();
    }

    let base_url_empty = selected
        .base_url
        .as_deref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true);
    if selected.provider.eq_ignore_ascii_case("ollama") && base_url_empty {
        selected.base_url = default_base_url_for_provider(&selected.provider);
    } else if !selected.provider.eq_ignore_ascii_case("ollama") && base_url_empty {
        selected.base_url = None;
    }

    if let Some(api_key) = selected.api_key.as_deref()
        && api_key.trim().is_empty()
    {
        selected.api_key = None;
    }
    if selected.provider.eq_ignore_ascii_case("ollama") {
        selected.api_key = None;
    }

    if selected.backend.trim().is_empty() {
        selected.backend = "in-memory".to_string();
    } else {
        selected.backend = selected.backend.trim().to_string();
    }

    if selected.daemon_url.trim().is_empty() {
        selected.daemon_url = DEFAULT_DAEMON_URL.to_string();
    } else {
        selected.daemon_url = selected.daemon_url.trim().to_string();
    }

    let daemon_bind = if selected.daemon_bind.trim().is_empty() {
        daemon_bind_from_url(&selected.daemon_url).unwrap_or_else(|| DEFAULT_DAEMON_BIND.to_string())
    } else {
        selected.daemon_bind.trim().to_string()
    };

    product_config = product_config_from_wizard(&selected);
    save_product_config(&product_config)?;
    apply_adapter_env(&product_config);
    defaults.response_depth_mode = Some(selected.tui_response_depth_mode.clone());

    if (selected.launch_tui || selected.start_discord || selected.start_telegram)
        && !selected.start_daemon
        && !explicit_no_daemon
        && !is_bind_reachable(&daemon_bind)
    {
        println!(
            "{}",
            "[info] Chat and adapters need daemon access. Auto-enabling daemon start.".yellow()
        );
        selected.start_daemon = true;
    }

    defaults.provider = Some(selected.provider.clone());
    defaults.model = Some(selected.model.clone());
    defaults.base_url = selected.base_url.clone();
    defaults.backend = Some(selected.backend.clone());
    save_tui_defaults(&defaults);

    if let Some(api_key) = selected.api_key.as_deref() {
        save_tui_api_key(Some(api_key));
    }

    if selected.configure_discord {
        if let Some(token) = selected.discord_token.as_deref() {
            save_discord_bot_token(Some(token));
            println!("{}", "[ok] Saved Discord bot token.".green());
        } else if existing_discord_token.is_some() {
            println!("{}", "[ok] Keeping existing Discord bot token.".green());
        }
    }

    if selected.configure_telegram {
        if let Some(token) = selected.telegram_token.as_deref() {
            save_telegram_bot_token(Some(token));
            println!("{}", "[ok] Saved Telegram bot token.".green());
        } else if existing_telegram_token.is_some() {
            println!("{}", "[ok] Keeping existing Telegram bot token.".green());
        }
    }

    profile.daemon_url = Some(selected.daemon_url.clone());
    profile.daemon_backend = Some(selected.backend.clone());
    save_onboard_profile(&profile)?;

    println!("{}", "[ok] Saved defaults, product config, and startup profile.".green());

    if selected.start_daemon {
        if is_bind_reachable(&daemon_bind) {
            println!(
                "{}",
                format!("[ok] Daemon already running at {}", selected.daemon_url).green()
            );
        } else {
            start_daemon_background(&selected.backend, &daemon_bind)?;
            if wait_for_bind_reachable(&daemon_bind, Duration::from_secs(4)) {
                println!(
                    "{}",
                    format!("[ok] Daemon started at {}", selected.daemon_url).green()
                );
            } else {
                println!(
                    "{}",
                    format!(
                        "[warn] Daemon start requested, but it is not reachable yet. Check {}",
                        daemon_log_path().display()
                    )
                    .yellow()
                );
            }
        }
    } else {
        println!("{}", "[ok] Daemon start skipped.".green());
    }

    if selected.start_discord {
        if let Some(token) = load_discord_bot_token() {
            start_discord_background(&selected.daemon_url, &token)?;
            println!("{}", "[ok] Discord adapter launch requested.".green());
        } else {
            println!(
                "{}",
                "[warn] Discord adapter start requested, but no token is configured.".yellow()
            );
        }
    }

    if selected.start_telegram {
        if let Some(token) = load_telegram_bot_token() {
            start_telegram_background(&selected.daemon_url, &token)?;
            println!("{}", "[ok] Telegram adapter launch requested.".green());
        } else {
            println!(
                "{}",
                "[warn] Telegram adapter start requested, but no token is configured.".yellow()
            );
        }
    }

    let elapsed = started_at.elapsed().as_secs_f32();
    println!(
        "{}",
        format!(
            "[ok] Setup complete in {:.1}s. Provider={} Model={}",
            elapsed, selected.provider, selected.model
        )
        .green()
    );

    if selected.launch_tui {
        println!("{}", "Launching Medousa chat...".magenta().bold());
        launch_tui_process(&selected.daemon_url, &[], None)?;
    } else {
        println!("{}", "Next command: medousa tui".blue());
    }

    Ok(())
}

fn run_tui(args: &[String]) -> Result<()> {
    let profile = load_onboard_profile();
    let defaults = load_tui_defaults();
    let product_config = load_product_config();
    apply_adapter_env(&product_config);
    let daemon_bind = resolve_daemon_bind(args, &profile, &product_config);
    let daemon_url = find_arg_value(args, "--daemon-url")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| profile.daemon_url.clone())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string());
    let backend = profile
        .daemon_backend
        .clone()
        .or(defaults.backend)
        .unwrap_or_else(|| "in-memory".to_string());

    let daemon_already_running = is_bind_reachable(&daemon_bind);

    if !has_flag(args, "--no-daemon") && !daemon_already_running {
        start_daemon_background(&backend, &daemon_bind)?;
        thread::sleep(Duration::from_millis(300));
    }

    // When the daemon is already running, it owns the database lock.
    // The TUI only needs an in-memory runtime for local tool execution;
    // all persistence goes through the daemon's HTTP API.
    if daemon_already_running {
        launch_tui_process(&daemon_url, args, Some("in-memory"))
    } else {
        launch_tui_process(&daemon_url, args, None)
    }
}

fn run_daemon(args: &[String]) -> Result<()> {
    let profile = load_onboard_profile();
    let defaults = load_tui_defaults();
    let product_config = load_product_config();
    let bind = resolve_daemon_bind(args, &profile, &product_config);
    let backend = find_arg_value(args, "--backend")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| profile.daemon_backend.clone())
        .or(defaults.backend)
        .unwrap_or_else(|| "in-memory".to_string());

    let mut passthrough = drop_flag_value_pair(args, "--backend");
    passthrough = drop_flag_value_pair(&passthrough, "--bind");

    let daemon = resolve_component_command("medousa_daemon")?;
    let mut command = Command::new(&daemon.program);
    command.args(&daemon.pre_args);
    command.arg("--backend").arg(backend);
    command.arg("--bind").arg(bind);
    command.args(&passthrough);

    let status = command
        .status()
        .context("failed to launch medousa_daemon")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("medousa_daemon exited with status {status}"))
    }
}

fn run_discord(args: &[String]) -> Result<()> {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        let adapter = resolve_component_command("medousa_discord")?;
        let mut command = Command::new(&adapter.program);
        command.args(&adapter.pre_args);
        command.args(args);
        let status = command
            .status()
            .context("failed to launch medousa_discord")?;
        return if status.success() {
            Ok(())
        } else {
            Err(anyhow!("medousa_discord exited with status {status}"))
        };
    }

    let profile = load_onboard_profile();
    let product_config = load_product_config();
    apply_adapter_env(&product_config);
    let daemon_url = find_arg_value(args, "--daemon-url")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or(profile.daemon_url)
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string());

    let token = find_arg_value(args, "--token")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(load_discord_bot_token)
        .ok_or_else(|| {
            anyhow!(
                "discord token missing. run medousa setup and enable Discord, or pass --token"
            )
        })?;

    let mut passthrough = drop_flag_value_pair(args, "--daemon-url");
    passthrough = drop_flag_value_pair(&passthrough, "--token");

    let adapter = resolve_component_command("medousa_discord")?;
    let mut command = Command::new(&adapter.program);
    command.args(&adapter.pre_args);
    command.arg("--daemon-url").arg(daemon_url);
    command.arg("--token").arg(token);
    command.args(&passthrough);

    let status = command
        .status()
        .context("failed to launch medousa_discord")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("medousa_discord exited with status {status}"))
    }
}

fn run_telegram(args: &[String]) -> Result<()> {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        let adapter = resolve_component_command("medousa_telegram")?;
        let mut command = Command::new(&adapter.program);
        command.args(&adapter.pre_args);
        command.args(args);
        let status = command
            .status()
            .context("failed to launch medousa_telegram")?;
        return if status.success() {
            Ok(())
        } else {
            Err(anyhow!("medousa_telegram exited with status {status}"))
        };
    }

    let profile = load_onboard_profile();
    let product_config = load_product_config();
    apply_adapter_env(&product_config);
    let daemon_url = find_arg_value(args, "--daemon-url")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or(profile.daemon_url.clone())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string());

    let token = find_arg_value(args, "--token")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(load_telegram_bot_token)
        .ok_or_else(|| {
            anyhow!(
                "telegram token missing. run medousa setup and enable Telegram, or pass --token"
            )
        })?;

    let mut passthrough = drop_flag_value_pair(args, "--daemon-url");
    passthrough = drop_flag_value_pair(&passthrough, "--token");
    passthrough = drop_flag_value_pair(&passthrough, "--allow-user-ids");

    let adapter = resolve_component_command("medousa_telegram")?;
    let mut command = Command::new(&adapter.program);
    command.args(&adapter.pre_args);
    command.arg("--daemon-url").arg(daemon_url);
    command.arg("--token").arg(token);
    command.args(&passthrough);

    let status = command
        .status()
        .context("failed to launch medousa_telegram")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("medousa_telegram exited with status {status}"))
    }
}

fn run_doctor(_args: &[String]) -> Result<()> {
    let defaults = load_tui_defaults();
    let profile = load_onboard_profile();
    let mut product_config = load_product_config();
    migrate_from_onboard_profile(
        &mut product_config,
        profile.telegram_allow_user_ids.as_deref(),
    );
    let daemon_url = profile
        .daemon_url
        .clone()
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string());
    let daemon_bind = resolve_daemon_bind(&[], &profile, &product_config);
    let daemon_reachable = is_bind_reachable(&daemon_bind);

    println!("medousa doctor");
    println!(
        "provider={} model={} base_url={} backend={}",
        defaults.provider.unwrap_or_else(|| "(unset)".to_string()),
        defaults.model.unwrap_or_else(|| "(unset)".to_string()),
        defaults.base_url.unwrap_or_else(|| "(unset)".to_string()),
        defaults.backend.unwrap_or_else(|| "(unset)".to_string()),
    );
    println!("daemon_url={} bind={} reachable={}", daemon_url, daemon_bind, daemon_reachable);
    println!(
        "response_depth={}",
        product_config.tui.response_depth_mode
    );
    println!(
        "api_key={}",
        if load_tui_api_key().is_some() {
            "configured"
        } else {
            "missing"
        }
    );
    println!(
        "discord_token={} telegram_token={}",
        if load_discord_bot_token().is_some() {
            "configured"
        } else {
            "missing"
        },
        if load_telegram_bot_token().is_some() {
            "configured"
        } else {
            "missing"
        },
    );
    println!(
        "discord_prefix={} discord_heartbeat={}",
        product_config.discord.command_prefix,
        if product_config.discord.heartbeat_nudges_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!(
        "telegram_allow_user_ids={}",
        if product_config.telegram.allowed_user_ids.is_empty() {
            "(all users)".to_string()
        } else {
            format_u64_csv(&product_config.telegram.allowed_user_ids)
        }
    );
    println!(
        "telegram_heartbeat={}",
        if product_config.telegram.heartbeat_nudges_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!(
        "ollama_detected={}",
        if detect_local_ollama() { "yes" } else { "no" }
    );

    if !daemon_reachable {
        println!("next: medousa setup or medousa tui");
    }

    Ok(())
}

fn product_config_from_wizard(selected: &onboard_wizard::WizardOutput) -> ProductConfig {
    let mut config = load_product_config();
    config.daemon.bind = if selected.daemon_bind.trim().is_empty() {
        DEFAULT_DAEMON_BIND.to_string()
    } else {
        selected.daemon_bind.trim().to_string()
    };
    config.telegram.allowed_user_ids = parse_u64_csv(
        selected.telegram_allow_user_ids.as_deref().unwrap_or(""),
    )
    .unwrap_or_default();
    config.telegram.heartbeat_nudges_enabled = selected.telegram_heartbeat_nudges_enabled;
    config.telegram.heartbeat_chat_ids = parse_i64_csv(
        selected.telegram_heartbeat_chat_ids.as_deref().unwrap_or(""),
    );
    config.discord.command_prefix = if selected.discord_command_prefix.trim().is_empty() {
        "!".to_string()
    } else {
        selected.discord_command_prefix.trim().to_string()
    };
    config.discord.heartbeat_nudges_enabled = selected.discord_heartbeat_nudges_enabled;
    config.discord.heartbeat_channel_ids = parse_u64_csv(
        selected.discord_heartbeat_channel_ids.as_deref().unwrap_or(""),
    )
    .unwrap_or_default();
    config.tui.response_depth_mode = selected.tui_response_depth_mode.clone();
    config
}

fn resolve_daemon_bind(args: &[String], profile: &OnboardProfile, product: &ProductConfig) -> String {
    find_arg_value(args, "--bind")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            if product.daemon.bind.trim().is_empty() {
                profile
                    .daemon_url
                    .as_deref()
                    .and_then(daemon_bind_from_url)
                    .unwrap_or_else(|| DEFAULT_DAEMON_BIND.to_string())
            } else {
                product.daemon.bind.clone()
            }
        })
}

fn normalize_provider(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "openai".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn default_model_for_provider(provider: &str) -> &'static str {
    if provider.eq_ignore_ascii_case("ollama") {
        DEFAULT_OLLAMA_MODEL
    } else {
        DEFAULT_OPENAI_MODEL
    }
}

fn default_base_url_for_provider(provider: &str) -> Option<String> {
    if !provider.eq_ignore_ascii_case("ollama") {
        return None;
    }

    env::var("MEDOUSA_OLLAMA_BASE_URL")
        .ok()
        .or_else(|| env::var("STASIS_OLLAMA_BASE_URL").ok())
        .or_else(|| {
            env::var("OLLAMA_HOST").ok().map(|host| {
                let trimmed = host.trim().trim_end_matches('/');
                if trimmed.ends_with("/v1") {
                    trimmed.to_string()
                } else {
                    format!("{trimmed}/v1/")
                }
            })
        })
        .or_else(|| Some(DEFAULT_OLLAMA_BASE_URL.to_string()))
}

fn detect_local_ollama() -> bool {
    if let Ok(mut addrs) = "127.0.0.1:11434".to_socket_addrs()
        && let Some(addr) = addrs.next()
    {
        return TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
    }
    false
}

fn daemon_bind_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next()?.trim();
    if host_port.is_empty() {
        None
    } else if host_port.contains(':') {
        Some(host_port.to_string())
    } else {
        Some(format!("{}:80", host_port))
    }
}

fn is_bind_reachable(bind: &str) -> bool {
    if let Ok(mut addrs) = bind.to_socket_addrs()
        && let Some(addr) = addrs.next()
    {
        return TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
    }
    false
}

fn wait_for_bind_reachable(bind: &str, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if is_bind_reachable(bind) {
            return true;
        }
        thread::sleep(Duration::from_millis(120));
    }
    false
}

fn start_daemon_background(backend: &str, bind: &str) -> Result<()> {
    let daemon = resolve_component_command("medousa_daemon")?;
    let log_path = daemon_log_path();
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create daemon log directory {}", parent.display()))?;
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open daemon log file {}", log_path.display()))?;
    let log_file_err = log_file
        .try_clone()
        .context("failed to clone daemon log handle")?;

    let mut command = Command::new(&daemon.program);
    command.args(&daemon.pre_args);
    command.arg("--backend").arg(backend);
    command.arg("--bind").arg(bind);
    command.stdout(Stdio::from(log_file));
    command.stderr(Stdio::from(log_file_err));

    let child = command
        .spawn()
        .with_context(|| format!("failed to spawn medousa_daemon using {}", daemon.program))?;
    println!(
        "daemon launch requested pid={} log={}",
        child.id(),
        log_path.display()
    );
    Ok(())
}

fn start_discord_background(daemon_url: &str, token: &str) -> Result<()> {
    apply_adapter_env(&load_product_config());
    start_adapter_background(
        "medousa_discord",
        daemon_url,
        token,
        &[],
        discord_log_path(),
        "discord",
    )
}

fn start_telegram_background(daemon_url: &str, token: &str) -> Result<()> {
    apply_adapter_env(&load_product_config());
    start_adapter_background(
        "medousa_telegram",
        daemon_url,
        token,
        &[],
        telegram_log_path(),
        "telegram",
    )
}

fn start_adapter_background(
    binary_name: &str,
    daemon_url: &str,
    token: &str,
    extra_args: &[String],
    log_path: PathBuf,
    label: &str,
) -> Result<()> {
    let adapter = resolve_component_command(binary_name)?;
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {label} log directory {}", parent.display()))?;
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open {label} log file {}", log_path.display()))?;
    let log_file_err = log_file
        .try_clone()
        .with_context(|| format!("failed to clone {label} log handle"))?;

    let mut command = Command::new(&adapter.program);
    command.args(&adapter.pre_args);
    command.arg("--daemon-url").arg(daemon_url);
    command.arg("--token").arg(token);
    command.args(extra_args);
    command.stdout(Stdio::from(log_file));
    command.stderr(Stdio::from(log_file_err));

    let child = command
        .spawn()
        .with_context(|| format!("failed to spawn {binary_name} using {}", adapter.program))?;
    println!(
        "{} launch requested pid={} log={}",
        label,
        child.id(),
        log_path.display()
    );
    Ok(())
}

fn launch_tui_process(daemon_url: &str, args: &[String], force_backend: Option<&str>) -> Result<()> {
    let tui = resolve_component_command("medousa_tui")?;
    let mut command = Command::new(&tui.program);
    command.args(&tui.pre_args);

    let mut passthrough = drop_flag_value_pair(args, "--daemon-url");
    passthrough = drop_flag_value_pair(&passthrough, "--backend");
    passthrough.retain(|arg| arg != "--no-daemon");

    command.arg("--daemon-url").arg(daemon_url);

    if let Some(backend) = force_backend {
        command.arg("--backend").arg(backend);
    }

    command.args(&passthrough);

    let status = command
        .status()
        .with_context(|| format!("failed to launch medousa_tui using {}", tui.program))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("medousa_tui exited with status {status}"))
    }
}

fn resolve_component_command(binary_name: &str) -> Result<ComponentCommand> {
    let env_key = format!("MEDOUSA_{}_BIN", binary_name.to_ascii_uppercase());
    if let Some(explicit) = env::var_os(&env_key) {
        let explicit_path = PathBuf::from(explicit);
        if explicit_path.exists() {
            return Ok(ComponentCommand {
                program: explicit_path.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }

    if let Ok(current_exe) = env::current_exe() {
        let sibling = current_exe.with_file_name(binary_name);
        if sibling.exists() {
            return Ok(ComponentCommand {
                program: sibling.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }

    if find_command_in_path(binary_name).is_some() {
        return Ok(ComponentCommand {
            program: binary_name.to_string(),
            pre_args: Vec::new(),
        });
    }

    if find_command_in_path("cargo").is_some() && Path::new("Cargo.toml").exists() {
        return Ok(ComponentCommand {
            program: "cargo".to_string(),
            pre_args: vec![
                "run".to_string(),
                "-p".to_string(),
                "medousa".to_string(),
                "--bin".to_string(),
                binary_name.to_string(),
                "--".to_string(),
            ],
        });
    }

    Err(anyhow!(
        "could not resolve '{}' binary. install package binaries or run from repo root",
        binary_name
    ))
}

fn find_command_in_path(command: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|path| path.join(command))
        .find(|candidate| candidate.exists())
}

fn drop_flag_value_pair(args: &[String], flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;

    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == flag {
            skip_next = true;
            continue;
        }
        out.push(arg.clone());
    }

    out
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn find_arg_value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    let idx = args.iter().position(|arg| arg == key)?;
    args.get(idx + 1).map(|value| value.as_str())
}

fn medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

fn onboard_profile_path() -> PathBuf {
    medousa_data_dir().join("onboard_profile.json")
}

fn daemon_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("daemon.log")
}

fn discord_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("discord.log")
}

fn telegram_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("telegram.log")
}

fn load_onboard_profile() -> OnboardProfile {
    let path = onboard_profile_path();
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<OnboardProfile>(&raw).ok())
        .unwrap_or_default()
}

fn save_onboard_profile(profile: &OnboardProfile) -> Result<()> {
    let path = onboard_profile_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create onboard profile dir {}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(profile).context("failed to encode onboard profile")?;
    fs::write(&path, raw)
        .with_context(|| format!("failed to write onboard profile {}", path.display()))?;
    Ok(())
}

fn print_help() {
    println!("Medousa launcher");
    println!();
    println!("USAGE:");
    println!("  medousa onboard|setup|init [--yes] [--advanced] [--provider <name>] [--model <name>] [--base-url <url>] [--api-key <key>] [--backend <name>] [--daemon-url <url>] [--no-daemon] [--no-tui]");
    println!("  medousa tui [--daemon-url <url>] [--no-daemon] [-- <medousa_tui args>]");
    println!("  medousa daemon [--backend <name>] [--bind <host:port>] [-- <medousa_daemon args>]");
    println!("  medousa discord [--daemon-url <url>] [--token <token>] [-- <medousa_discord args>]");
    println!("  medousa telegram [--daemon-url <url>] [--token <token>] [-- <medousa_telegram args>]");
    println!("    Telegram allowlist is configured in medousa setup (product_config.json).");
    println!("  medousa doctor");
    println!();
    println!("EXAMPLES:");
    println!("  medousa onboard");
    println!("  medousa setup");
    println!("  medousa onboard --yes --provider ollama --model llama3.2 --no-daemon");
    println!("  medousa tui");
    println!("  medousa discord");
    println!("  medousa telegram");
    println!("  medousa doctor");
}