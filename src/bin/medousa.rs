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
    load_discord_bot_token, load_slack_app_token, load_slack_bot_token, load_telegram_bot_token,
    load_tui_api_key, load_tui_defaults, save_discord_bot_token, save_slack_app_token,
    save_slack_bot_token, save_telegram_bot_token, save_tui_api_key, save_tui_defaults,
};
use medousa::{
    ProductConfig, apply_adapter_env, apply_daemon_env, clear_stale_surrealkv_lock, format_i64_csv,
    format_u64_csv, load_product_config, migrate_from_onboard_profile, parse_backend,
    parse_i64_csv, parse_u64_csv, save_product_config, surrealkv_lock_path,
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
        "slack" => run_slack(&args[1..]),
        "whatsapp" => run_whatsapp(&args[1..]),
        "doctor" => run_doctor(&args[1..]),
        "identity-export" => run_identity_export(&args[1..]),
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
    let existing_slack_bot_token = load_slack_bot_token();
    let existing_slack_app_token = load_slack_app_token();
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
            configure_slack: false,
            slack_bot_token: None,
            slack_app_token: None,
            slack_allow_user_ids: if product_config.slack.allowed_user_ids.is_empty() {
                None
            } else {
                Some(product_config.slack.allowed_user_ids.join(","))
            },
            start_slack: false,
            configure_whatsapp: false,
            whatsapp_deliver_bind: product_config.whatsapp.deliver_bind.clone(),
            whatsapp_allow_user_ids: if product_config.whatsapp.allowed_user_ids.is_empty() {
                None
            } else {
                Some(product_config.whatsapp.allowed_user_ids.join(","))
            },
            start_whatsapp: false,
            configure_mcp_gateway: true,
            start_mcp_gateway: true,
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
            existing_slack_bot_token: existing_slack_bot_token.is_some(),
            existing_slack_app_token: existing_slack_app_token.is_some(),
            existing_mcp_gateway_config: medousa::mcp_gateway::gateway_config_path().exists(),
            initial_telegram_allow_user_ids,
            initial_slack_allow_user_ids: if product_config.slack.allowed_user_ids.is_empty() {
                String::new()
            } else {
                product_config.slack.allowed_user_ids.join(",")
            },
            initial_whatsapp_deliver_bind: product_config.whatsapp.deliver_bind.clone(),
            initial_whatsapp_allow_user_ids: if product_config.whatsapp.allowed_user_ids.is_empty()
            {
                String::new()
            } else {
                product_config.whatsapp.allowed_user_ids.join(",")
            },
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

    if (selected.launch_tui
        || selected.start_discord
        || selected.start_telegram
        || selected.start_slack
        || selected.start_whatsapp)
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

    if selected.configure_slack {
        if let Some(token) = selected.slack_bot_token.as_deref() {
            save_slack_bot_token(Some(token));
            println!("{}", "[ok] Saved Slack bot token.".green());
        } else if existing_slack_bot_token.is_some() {
            println!("{}", "[ok] Keeping existing Slack bot token.".green());
        }
        if let Some(token) = selected.slack_app_token.as_deref() {
            save_slack_app_token(Some(token));
            println!("{}", "[ok] Saved Slack app token.".green());
        } else if existing_slack_app_token.is_some() {
            println!("{}", "[ok] Keeping existing Slack app token.".green());
        }
    }

    if selected.configure_whatsapp {
        println!(
            "{}",
            "[ok] WhatsApp deliver bind saved in product_config (session db: ~/.local/share/medousa/whatsapp/session.db)."
                .green()
        );
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
            ensure_daemon_running(&selected.backend, &daemon_bind)?;
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

    if selected.configure_mcp_gateway {
        match install_default_mcp_gateway_config() {
            Ok(path) => {
                println!(
                    "{}",
                    format!("[ok] MCP gateway config ready at {}", path.display()).green()
                );
            }
            Err(error) => {
                println!(
                    "{}",
                    format!("[warn] MCP gateway config install failed: {error:#}").yellow()
                );
            }
        }
    }

    if selected.start_mcp_gateway {
        let mcp_bind = medousa::DEFAULT_MCP_GATEWAY_BIND;
        if is_bind_reachable(mcp_bind) {
            println!(
                "{}",
                format!("[ok] MCP gateway already running at {}", medousa::DEFAULT_MCP_GATEWAY_URL)
                    .green()
            );
        } else {
            start_mcp_gateway_background()?;
            if wait_for_bind_reachable(mcp_bind, Duration::from_secs(4)) {
                println!(
                    "{}",
                    format!(
                        "[ok] MCP gateway started at {}",
                        medousa::DEFAULT_MCP_GATEWAY_URL
                    )
                    .green()
                );
            } else {
                println!(
                    "{}",
                    format!(
                        "[warn] MCP gateway start requested, but it is not reachable yet. Check {}",
                        mcp_gateway_log_path().display()
                    )
                    .yellow()
                );
            }
        }
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

    if selected.start_slack {
        let bot_token = load_slack_bot_token();
        let app_token = load_slack_app_token();
        match (bot_token, app_token) {
            (Some(bot_token), Some(app_token)) => {
                start_slack_background(&selected.daemon_url, &bot_token, &app_token)?;
                println!("{}", "[ok] Slack adapter launch requested.".green());
            }
            _ => {
                println!(
                    "{}",
                    "[warn] Slack adapter start requested, but bot/app tokens are missing.".yellow()
                );
            }
        }
    }

    if selected.start_whatsapp {
        start_whatsapp_background(
            &selected.daemon_url,
            &selected.whatsapp_deliver_bind,
            product_config.whatsapp.session_db_path.as_deref(),
        )?;
        println!("{}", "[ok] WhatsApp adapter launch requested.".green());
        println!(
            "{}",
            format!(
                "Scan QR on first pairing — log: {}",
                whatsapp_log_path().display()
            )
            .yellow()
        );
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
        launch_tui_process(&selected.daemon_url, &[], Some("in-memory"))?;
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
        ensure_daemon_running(&backend, &daemon_bind)?;
    }

    // When the daemon owns persistence, the TUI uses in-memory locally and talks to the daemon API.
    let daemon_hosts_persistence = is_bind_reachable(&daemon_bind);
    if daemon_hosts_persistence {
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
    apply_daemon_env(&product_config);
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

fn run_slack(args: &[String]) -> Result<()> {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        let adapter = resolve_component_command("medousa_slack")?;
        let mut command = Command::new(&adapter.program);
        command.args(&adapter.pre_args);
        command.args(args);
        let status = command
            .status()
            .context("failed to launch medousa_slack")?;
        return if status.success() {
            Ok(())
        } else {
            Err(anyhow!("medousa_slack exited with status {status}"))
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

    let bot_token = find_arg_value(args, "--bot-token")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(load_slack_bot_token)
        .ok_or_else(|| {
            anyhow!(
                "slack bot token missing. pass --bot-token or set MEDOUSA_SLACK_BOT_TOKEN"
            )
        })?;
    let app_token = find_arg_value(args, "--app-token")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(load_slack_app_token)
        .ok_or_else(|| {
            anyhow!(
                "slack app token missing. pass --app-token or set MEDOUSA_SLACK_APP_TOKEN"
            )
        })?;

    let mut passthrough = drop_flag_value_pair(args, "--daemon-url");
    passthrough = drop_flag_value_pair(&passthrough, "--bot-token");
    passthrough = drop_flag_value_pair(&passthrough, "--app-token");

    let adapter = resolve_component_command("medousa_slack")?;
    let mut command = Command::new(&adapter.program);
    command.args(&adapter.pre_args);
    command.arg("--daemon-url").arg(daemon_url);
    command.arg("--bot-token").arg(bot_token);
    command.arg("--app-token").arg(app_token);
    command.args(&passthrough);

    let status = command
        .status()
        .context("failed to launch medousa_slack")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("medousa_slack exited with status {status}"))
    }
}

fn run_whatsapp(args: &[String]) -> Result<()> {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        let adapter = resolve_component_command("medousa_whatsapp")?;
        let mut command = Command::new(&adapter.program);
        command.args(&adapter.pre_args);
        command.args(args);
        let status = command
            .status()
            .context("failed to launch medousa_whatsapp")?;
        return if status.success() {
            Ok(())
        } else {
            Err(anyhow!("medousa_whatsapp exited with status {status}"))
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

    let deliver_bind = find_arg_value(args, "--deliver-bind")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| product_config.whatsapp.deliver_bind.clone());

    let mut passthrough = drop_flag_value_pair(args, "--daemon-url");
    passthrough = drop_flag_value_pair(&passthrough, "--deliver-bind");
    if let Some(path) = product_config
        .whatsapp
        .session_db_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        passthrough.push("--session-db".to_string());
        passthrough.push(path.trim().to_string());
    }

    let adapter = resolve_component_command("medousa_whatsapp")?;
    let mut command = Command::new(&adapter.program);
    command.args(&adapter.pre_args);
    command.arg("--daemon-url").arg(daemon_url);
    command.arg("--deliver-bind").arg(deliver_bind);
    command.args(&passthrough);

    let status = command
        .status()
        .context("failed to launch medousa_whatsapp")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("medousa_whatsapp exited with status {status}"))
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
    let backend_name = profile
        .daemon_backend
        .clone()
        .or_else(|| defaults.backend.clone())
        .unwrap_or_else(|| "in-memory".to_string());
    println!(
        "provider={} model={} base_url={} backend={}",
        defaults.provider.unwrap_or_else(|| "(unset)".to_string()),
        defaults.model.unwrap_or_else(|| "(unset)".to_string()),
        defaults.base_url.unwrap_or_else(|| "(unset)".to_string()),
        backend_name,
    );
    println!("daemon_url={} bind={} reachable={}", daemon_url, daemon_bind, daemon_reachable);
    let backend = parse_backend(Some(&backend_name));
    let daemon_process_running = is_medousa_daemon_process_running();
    println!("daemon_process_running={}", daemon_process_running);
    if let Some(lock_path) = surrealkv_lock_path(&backend) {
        println!(
            "surrealkv_lock={} exists={}",
            lock_path.display(),
            lock_path.exists()
        );
        if lock_path.exists() && !daemon_process_running && !daemon_reachable {
            println!(
                "surrealkv_lock_hint=stale lock with no daemon — run: rm {}",
                lock_path.display()
            );
        }
    }
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
        "discord_token={} telegram_token={} slack_bot_token={} slack_app_token={}",
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
        if load_slack_bot_token().is_some() {
            "configured"
        } else {
            "missing"
        },
        if load_slack_app_token().is_some() {
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
        "slack_allow_user_ids={}",
        if product_config.slack.allowed_user_ids.is_empty() {
            "(all users)".to_string()
        } else {
            product_config.slack.allowed_user_ids.join(",")
        }
    );
    println!(
        "whatsapp_deliver_bind={} whatsapp_allow_user_ids={}",
        product_config.whatsapp.deliver_bind,
        if product_config.whatsapp.allowed_user_ids.is_empty() {
            "(all users)".to_string()
        } else {
            product_config.whatsapp.allowed_user_ids.join(",")
        }
    );
    let whatsapp_session_db = product_config
        .whatsapp
        .session_db_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            medousa_data_dir()
                .join("whatsapp")
                .join("session.db")
                .display()
                .to_string()
        });
    println!(
        "whatsapp_session_db={} exists={}",
        whatsapp_session_db,
        Path::new(&whatsapp_session_db).exists()
    );
    println!(
        "deliver_webhook_token={}",
        if product_config
            .daemon
            .deliver_webhook_token
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            "configured"
        } else {
            "missing"
        }
    );
    println!(
        "ollama_detected={}",
        if detect_local_ollama() { "yes" } else { "no" }
    );

    let mcp_gateway_url = medousa::resolve_mcp_gateway_url(None);
    let mcp_gateway_bind = mcp_gateway_url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let mcp_gateway_reachable = is_bind_reachable(mcp_gateway_bind);
    let policy_token_configured = medousa::mcp_gateway::resolve_mcp_policy_token().is_some();
    let turn_token_configured = env::var("MEDOUSA_MCP_TURN_TOKEN_SECRET")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    println!(
        "mcp_gateway_url={} reachable={} auth={} policy_token={} turn_token_secret={}",
        mcp_gateway_url,
        mcp_gateway_reachable,
        if medousa::gateway_auth_configured() {
            "configured"
        } else {
            "open"
        },
        if policy_token_configured {
            "configured"
        } else {
            "open"
        },
        if turn_token_configured {
            "configured"
        } else {
            "open"
        }
    );
    if daemon_reachable && !mcp_gateway_reachable {
        println!(
            "{}",
            format!(
                "[warn] Daemon is up but MCP gateway is not reachable at {mcp_gateway_url}. cognition.mcp.* tools will fail until medousa_mcp_gateway is running."
            )
            .yellow()
        );
    }
    if mcp_gateway_reachable {
        if let Ok(health) = fetch_mcp_gateway_health(&mcp_gateway_url) {
            println!(
                "mcp_gateway_status={} invokes_enabled={} registered_servers={} connected_servers={} catalog_entries={}",
                health.status,
                health.invokes_enabled,
                health.registered_servers,
                health.connected_servers,
                health.catalog_entries
            );
            if !health.invokes_enabled {
                println!(
                    "{}",
                    "[warn] MCP gateway invokes are disabled — cognition.mcp.invoke will be rejected."
                        .yellow()
                );
            }
            if health.registered_servers > 0 && health.connected_servers == 0 {
                println!(
                    "{}",
                    "[warn] MCP gateway has registered servers but none are connected. Check ~/.config/medousa/mcp-gateway.toml or enable use_mock on servers."
                        .yellow()
                );
            }
            if health.registered_servers == 0 {
                println!(
                    "{}",
                    "[hint] No MCP servers registered. Run medousa setup to install mcp-gateway.toml or add [[servers]] entries."
                        .blue()
                );
            }
        }
        if daemon_reachable {
            if let Ok(capabilities) = fetch_capabilities(&daemon_url) {
                println!(
                    "capability_catalog_count={}",
                    capabilities.capabilities.len()
                );
            }
        }
    } else if medousa::gateway_auth_configured() && !policy_token_configured {
        println!(
            "{}",
            "[hint] MEDOUSA_MCP_GATEWAY_TOKEN is set but MEDOUSA_MCP_POLICY_TOKEN is unset — gateway→daemon policy calls may fail if policy auth is required."
                .blue()
        );
    }

    if daemon_reachable {
        if let Ok(health) = fetch_daemon_health(&daemon_url) {
            println!(
                "agent_runtime_version={} tool_registry_count={} last_agent_turn_latency_ms={:?} last_agent_turn_at={:?}",
                health.agent_runtime_version,
                health.tool_registry_count,
                health.last_agent_turn_latency_ms,
                health.last_agent_turn_at_utc,
            );
        }
        if let Ok(delivery) = fetch_delivery_health(&daemon_url) {
            println!(
                "delivery_endpoint={} seeded={} target={} webhook_auth={} pending={} last_delivery={:?} last_latency_ms={:?}",
                delivery.endpoint_id,
                delivery.endpoint_seeded,
                delivery.endpoint_target,
                if delivery.deliver_webhook_auth_configured {
                    "configured"
                } else {
                    "open"
                },
                delivery.pending_job_deliveries,
                delivery.last_delivery_at_utc,
                delivery.last_delivery_latency_ms,
            );
        } else {
            println!("delivery_status=unavailable (daemon did not return /v1/delivery/status)");
        }
    }

    if !daemon_reachable {
        println!("next: medousa setup or medousa tui");
    }

    let heartbeat_agent_turn = medousa::agent_runtime::heartbeat_agent_turn_enabled();
    println!(
        "heartbeat_agent_turn={}",
        if heartbeat_agent_turn {
            "enabled"
        } else {
            "disabled (set MEDOUSA_HEARTBEAT_AGENT_TURN_ENABLED=1)"
        }
    );
    let heartbeat_policy = medousa::agent_runtime::heartbeat_policy_doc_path();
    println!(
        "heartbeat_policy_doc={} exists={}",
        heartbeat_policy.display(),
        heartbeat_policy.is_file()
    );

    let identity_export_dir = medousa::identity_markdown::identity_markdown_export_dir();
    println!(
        "identity_export_dir={} exists={}",
        identity_export_dir.display(),
        identity_export_dir.is_dir()
    );
    if !identity_export_dir.is_dir() {
        println!(
            "{}",
            "[hint] Run `medousa identity-export` to write SOUL.md, USER.md, and IDENTITY.md from identity memory."
                .blue()
        );
    }

    println!(
        "{}",
        "[hint] For DOM/browser automation, register a browser MCP server (Playwright, Puppeteer, etc.) in ~/.config/medousa/mcp-gateway.toml — Medousa uses MCP BYOB, not native browser."
            .blue()
    );

    Ok(())
}

fn run_identity_export(args: &[String]) -> Result<()> {
    let user_id = find_arg_value(args, "--user-id").map(str::to_string);
    let dir = find_arg_value(args, "--dir")
        .map(PathBuf::from)
        .unwrap_or_else(medousa::identity_markdown::identity_markdown_export_dir);

    let store = medousa::identity_memory::build_seeded_identity_memory_store()
        .context("build identity memory store")?;
    let rt = tokio::runtime::Runtime::new().context("start tokio runtime")?;
    let written = rt
        .block_on(medousa::identity_markdown::write_identity_markdown_export(
            store.as_ref(),
            user_id.as_deref(),
            &dir,
        ))
        .context("export identity markdown")?;

    println!("identity markdown exported to {}", written.display());
    println!("  SOUL.md");
    println!("  USER.md");
    println!("  IDENTITY.md");
    Ok(())
}

fn fetch_mcp_gateway_health(gateway_url: &str) -> Result<medousa::McpGatewayHealthResponse> {
    let gateway_url = gateway_url.trim_end_matches('/');
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(format!("{gateway_url}/health"))
        .send()?
        .error_for_status()?;
    Ok(response.json()?)
}

fn fetch_capabilities(daemon_url: &str) -> Result<medousa::CapabilityListResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(format!("{daemon_url}/v1/capabilities"))
        .send()?
        .error_for_status()?;
    Ok(response.json()?)
}

fn fetch_daemon_health(daemon_url: &str) -> Result<medousa::HealthResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(format!("{daemon_url}/health"))
        .send()?
        .error_for_status()?;
    Ok(response.json()?)
}

fn fetch_delivery_health(daemon_url: &str) -> Result<medousa::DeliveryHealthResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(format!("{daemon_url}/v1/delivery/status"))
        .send()?
        .error_for_status()?;
    Ok(response.json()?)
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
    config.slack.allowed_user_ids = parse_string_csv(
        selected.slack_allow_user_ids.as_deref().unwrap_or(""),
    );
    config.whatsapp.deliver_bind = if selected.whatsapp_deliver_bind.trim().is_empty() {
        config.whatsapp.deliver_bind
    } else {
        selected.whatsapp_deliver_bind.trim().to_string()
    };
    config.whatsapp.allowed_user_ids = parse_string_csv(
        selected.whatsapp_allow_user_ids.as_deref().unwrap_or(""),
    );
    config.tui.response_depth_mode = selected.tui_response_depth_mode.clone();
    if config
        .daemon
        .deliver_webhook_token
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        config.daemon.deliver_webhook_token =
            Some(uuid::Uuid::new_v4().simple().to_string());
    }
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

fn is_medousa_daemon_process_running() -> bool {
    #[cfg(unix)]
    {
        Command::new("pgrep")
            .args(["-x", "medousa_daemon"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn ensure_daemon_running(backend: &str, bind: &str) -> Result<()> {
    if is_bind_reachable(bind) {
        return Ok(());
    }

    if is_medousa_daemon_process_running() {
        if wait_for_bind_reachable(bind, Duration::from_secs(15)) {
            return Ok(());
        }
        return Err(anyhow!(
            "medousa_daemon is running but not reachable at {bind} — check {}",
            daemon_log_path().display()
        ));
    }

    let parsed_backend = parse_backend(Some(backend));
    clear_stale_surrealkv_lock(&parsed_backend).with_context(|| {
        format!(
            "could not clear SurrealKV lock before starting daemon (backend={backend}). \
             If no daemon is running, remove the LOCK file under ~/.local/share/medousa/runtime.surrealkv/"
        )
    })?;

    start_daemon_background(backend, bind)?;

    if wait_for_bind_reachable(bind, Duration::from_secs(15)) {
        return Ok(());
    }

    Err(anyhow!(
        "medousa_daemon failed to start or is not reachable at {bind} — check {}",
        daemon_log_path().display()
    ))
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
    apply_daemon_env(&load_product_config());
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
        Some(token),
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
        Some(token),
        &[],
        telegram_log_path(),
        "telegram",
    )
}

fn start_slack_background(daemon_url: &str, bot_token: &str, app_token: &str) -> Result<()> {
    apply_adapter_env(&load_product_config());
    start_adapter_background(
        "medousa_slack",
        daemon_url,
        None,
        &[
            "--bot-token".to_string(),
            bot_token.to_string(),
            "--app-token".to_string(),
            app_token.to_string(),
        ],
        slack_log_path(),
        "slack",
    )
}

fn start_whatsapp_background(
    daemon_url: &str,
    deliver_bind: &str,
    session_db_path: Option<&str>,
) -> Result<()> {
    apply_adapter_env(&load_product_config());
    let mut extra_args = vec![
        "--deliver-bind".to_string(),
        deliver_bind.to_string(),
    ];
    if let Some(path) = session_db_path.filter(|value| !value.trim().is_empty()) {
        extra_args.push("--session-db".to_string());
        extra_args.push(path.trim().to_string());
    }
    start_adapter_background(
        "medousa_whatsapp",
        daemon_url,
        None,
        &extra_args,
        whatsapp_log_path(),
        "whatsapp",
    )
}

fn start_adapter_background(
    binary_name: &str,
    daemon_url: &str,
    token: Option<&str>,
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
    if let Some(token) = token {
        command.arg("--token").arg(token);
    }
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
        if binary_name == "medousa_whatsapp" {
            let manifest = Path::new("adapters/medousa-whatsapp/Cargo.toml");
            if manifest.exists() {
                return Ok(ComponentCommand {
                    program: "cargo".to_string(),
                    pre_args: vec![
                        "run".to_string(),
                        "--manifest-path".to_string(),
                        manifest.to_string_lossy().to_string(),
                        "--".to_string(),
                    ],
                });
            }
        }

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

fn mcp_gateway_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("mcp-gateway.log")
}

const DEFAULT_MCP_GATEWAY_EXAMPLE_TOML: &str = r#"# Medousa MCP Client gateway — starter config
# Docs: docs/internal/mcp-client-gateway-design.md

[gateway]
bind = "127.0.0.1:7420"
daemon_policy_url = "http://127.0.0.1:7419/v1/mcp/policy/evaluate"
use_mock_fallback = true

# Optional auth (set matching env vars on daemon + gateway):
# MEDOUSA_MCP_GATEWAY_TOKEN, MEDOUSA_MCP_GATEWAY_ADMIN_TOKEN, MEDOUSA_MCP_POLICY_TOKEN
# MEDOUSA_MCP_TURN_TOKEN_SECRET — required for cognition.mcp.invoke turn tokens

[[servers]]
id = "notion"
title = "Notion MCP"
enabled = true
transport = "stdio"
use_mock = true
allowed_lanes = ["interactive", "scheduled"]
allowed_effect_classes = ["external_read", "external_write", "external_side_effect"]

[[servers]]
id = "gmail"
title = "Gmail MCP"
enabled = true
transport = "stdio"
use_mock = true
allowed_lanes = ["interactive", "scheduled"]
allowed_effect_classes = ["external_read", "external_write", "external_side_effect"]
"#;

fn install_default_mcp_gateway_config() -> Result<PathBuf> {
    let path = medousa::mcp_gateway::gateway_config_path();
    if path.exists() {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create MCP gateway config directory {}",
                parent.display()
            )
        })?;
    }
    fs::write(&path, DEFAULT_MCP_GATEWAY_EXAMPLE_TOML).with_context(|| {
        format!(
            "failed to write MCP gateway config {}",
            path.display()
        )
    })?;
    Ok(path)
}

fn start_mcp_gateway_background() -> Result<()> {
    let gateway = resolve_component_command("medousa_mcp_gateway")?;
    let log_path = mcp_gateway_log_path();
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create MCP gateway log directory {}",
                parent.display()
            )
        })?;
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open MCP gateway log file {}", log_path.display()))?;
    let log_file_err = log_file
        .try_clone()
        .context("failed to clone MCP gateway log handle")?;

    let mut command = Command::new(&gateway.program);
    command.args(&gateway.pre_args);
    command.stdout(Stdio::from(log_file));
    command.stderr(Stdio::from(log_file_err));

    let child = command.spawn().with_context(|| {
        format!(
            "failed to spawn medousa_mcp_gateway using {}",
            gateway.program
        )
    })?;
    println!(
        "mcp gateway launch requested pid={} log={}",
        child.id(),
        log_path.display()
    );
    Ok(())
}

fn discord_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("discord.log")
}

fn telegram_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("telegram.log")
}

fn slack_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("slack.log")
}

fn whatsapp_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("whatsapp.log")
}

fn parse_string_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
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
    println!("  medousa slack [--daemon-url <url>] [--bot-token <xoxb-…>] [--app-token <xapp-…>]");
    println!("  medousa whatsapp [--daemon-url <url>] [--deliver-bind <host:port>] [--session-db <path>]");
    println!("    WhatsApp uses a local deliver endpoint; session persists in ~/.local/share/medousa/whatsapp/session.db by default.");
    println!("  medousa doctor");
    println!("  medousa identity-export [--user-id <id>] [--dir <path>]");
    println!();
    println!("EXAMPLES:");
    println!("  medousa onboard");
    println!("  medousa setup");
    println!("  medousa onboard --yes --provider ollama --model llama3.2 --no-daemon");
    println!("  medousa tui");
    println!("  medousa discord");
    println!("  medousa telegram");
    println!("  medousa slack");
    println!("  medousa whatsapp");
    println!("  medousa doctor");
}