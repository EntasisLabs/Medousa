use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use crossterm::style::Stylize;
use medousa::session::{
    load_discord_bot_token, load_slack_app_token, load_slack_bot_token, load_telegram_bot_token,
    load_surreal_password, load_tui_api_key, load_tui_defaults, save_discord_bot_token,
    save_slack_app_token, save_slack_bot_token, save_surreal_password, save_telegram_bot_token,
    save_tui_api_key, save_tui_defaults,
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
        "start" => run_start(&args[1..]),
        "tui" => run_tui(&args[1..]),
        "daemon" => run_daemon(&args[1..]),
        "discord" => run_discord(&args[1..]),
        "telegram" => run_telegram(&args[1..]),
        "slack" => run_slack(&args[1..]),
        "whatsapp" => run_whatsapp(&args[1..]),
        "doctor" => run_doctor(&args[1..]),
        "identity-export" => run_identity_export(&args[1..]),
        "identity-remember" => run_identity_remember(&args[1..]),
        "manuscript-list" => run_manuscript_list(),
        "manuscript-validate" => run_manuscript_validate(&args[1..]),
        "manuscript-install" => run_manuscript_install(&args[1..]),
        "skill-import" => run_skill_import(&args[1..]),
        "openshell-probe" => run_openshell_probe(&args[1..]),
        "workspace" => run_workspace(&args[1..]),
        "vault" => run_vault(&args[1..]),
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
            stasis_otel_enabled: defaults.stasis_otel_enabled.unwrap_or(false),
            surreal_endpoint: defaults
                .surreal_endpoint
                .clone()
                .unwrap_or_else(|| "ws://127.0.0.1:8000/rpc".to_string()),
            surreal_username: defaults.surreal_username.clone().unwrap_or_default(),
            surreal_password: defaults
                .surreal_password
                .clone()
                .or_else(|| load_surreal_password())
                .unwrap_or_default(),
            surreal_namespace: defaults
                .surreal_namespace
                .clone()
                .unwrap_or_else(|| "medousa".to_string()),
            surreal_database: defaults
                .surreal_database
                .clone()
                .unwrap_or_else(|| "runtime".to_string()),
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
        let _initial_surreal_endpoint = defaults
            .surreal_endpoint
            .clone()
            .or_else(|| product_config.surreal.endpoint.clone())
            .unwrap_or_else(|| "ws://127.0.0.1:8000/rpc".to_string());
        let initial_surreal_username = defaults
            .surreal_username
            .clone()
            .or_else(|| product_config.surreal.username.clone())
            .unwrap_or_default();
        let initial_surreal_password = defaults
            .surreal_password
            .clone()
            .or_else(|| product_config.surreal.password.clone())
            .or_else(|| load_surreal_password())
            .unwrap_or_default();
        let initial_surreal_namespace = defaults
            .surreal_namespace
            .clone()
            .or_else(|| product_config.surreal.namespace.clone())
            .unwrap_or_else(|| "medousa".to_string());
        let initial_surreal_database = defaults
            .surreal_database
            .clone()
            .or_else(|| product_config.surreal.database.clone())
            .unwrap_or_else(|| "runtime".to_string());

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
            initial_stasis_otel_enabled: defaults.stasis_otel_enabled.unwrap_or(false),
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
            initial_surreal_username,
            initial_surreal_password,
            initial_surreal_namespace,
            initial_surreal_database,
            default_surreal_namespace: "medousa".to_string(),
            default_surreal_database: "runtime".to_string(),
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
    defaults.stasis_otel_enabled = Some(selected.stasis_otel_enabled);

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
    defaults.surreal_endpoint = if selected.surreal_endpoint.trim().is_empty() {
        None
    } else {
        Some(selected.surreal_endpoint.trim().to_string())
    };
    defaults.surreal_username = if selected.surreal_username.trim().is_empty() {
        None
    } else {
        Some(selected.surreal_username.trim().to_string())
    };
    defaults.surreal_namespace = if selected.surreal_namespace.trim().is_empty() {
        None
    } else {
        Some(selected.surreal_namespace.trim().to_string())
    };
    defaults.surreal_database = if selected.surreal_database.trim().is_empty() {
        None
    } else {
        Some(selected.surreal_database.trim().to_string())
    };
    defaults.surreal_password = None;
    save_tui_defaults(&defaults);
    medousa::runtime::stasis_otel::apply_stasis_otel_user_preference(selected.stasis_otel_enabled);

    if selected.surreal_password.trim().is_empty() {
        save_surreal_password(None);
    } else {
        save_surreal_password(Some(selected.surreal_password.trim()));
    }

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
    medousa::sync_profile_daemon_backend(&mut profile.daemon_backend, &product_config, &defaults);
    save_onboard_profile(&profile)?;

    println!("{}", "[ok] Saved defaults, product config, and startup profile.".green());

    let daemon_launch_backend = medousa::resolve_daemon_launch_backend(
        None,
        profile.daemon_backend.as_deref(),
        &product_config,
        &defaults,
    );

    if selected.start_daemon {
        let plan = DaemonLaunchPlan {
            bind: daemon_bind.clone(),
            health_url: selected.daemon_url.clone(),
            mobile_url: None,
        };
        if daemon_http_healthy(&plan.health_url) {
            print_daemon_ready_messages(&plan, true);
        } else if is_bind_reachable(&plan.bind) {
            println!(
                "{}",
                "[warn] Port is open but the daemon API is not healthy (dashboard and delivery will not work)."
                    .yellow()
            );
            println!(
                "{}",
                "       Restart with: medousa start daemon-restart".blue()
            );
        } else {
            ensure_daemon_running(&daemon_launch_backend, &plan)?;
            if wait_for_daemon_healthy(&plan.health_url, Duration::from_secs(20)) {
                print_daemon_ready_messages(&plan, false);
            } else {
                println!(
                    "{}",
                    format!(
                        "[warn] Daemon start requested, but /health did not respond in time. Check {}",
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
        match medousa::install_starter_gateway_config_if_missing() {
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
        if !medousa::mcp_gateway::gateway_config_path().exists() {
            match medousa::install_starter_gateway_config_if_missing() {
                Ok(path) => println!(
                    "{}",
                    format!(
                        "[ok] MCP gateway config created at {} (required for start)",
                        path.display()
                    )
                    .green()
                ),
                Err(error) => println!(
                    "{}",
                    format!("[warn] MCP gateway config missing and install failed: {error:#}")
                        .yellow()
                ),
            }
        }
        if is_bind_reachable(mcp_bind) {
            println!(
                "{}",
                format!("[ok] MCP gateway already running at {}", medousa::DEFAULT_MCP_GATEWAY_URL)
                    .green()
            );
        } else {
            start_mcp_gateway_background()?;
            if wait_for_bind_reachable(mcp_bind, Duration::from_secs(8)) {
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
    let plan = resolve_daemon_launch_plan(args, &profile, &product_config);
    let backend = medousa::resolve_daemon_launch_backend(
        find_arg_value(args, "--backend"),
        profile.daemon_backend.as_deref(),
        &product_config,
        &defaults,
    );

    let daemon_already_healthy = daemon_http_healthy(&plan.health_url);

    if !has_flag(args, "--no-daemon") && !daemon_already_healthy {
        ensure_daemon_running(&backend, &plan)?;
    }

    // When the daemon owns persistence, the TUI uses in-memory locally and talks to the daemon API.
    let daemon_hosts_persistence = daemon_http_healthy(&plan.health_url);
    if daemon_hosts_persistence {
        launch_tui_process(&plan.health_url, args, Some("in-memory"))
    } else {
        launch_tui_process(&plan.health_url, args, None)
    }
}

fn run_daemon(args: &[String]) -> Result<()> {
    let profile = load_onboard_profile();
    let defaults = load_tui_defaults();
    let product_config = load_product_config();
    let plan = resolve_daemon_launch_plan(args, &profile, &product_config);
    let backend = medousa::resolve_daemon_launch_backend(
        find_arg_value(args, "--backend"),
        profile.daemon_backend.as_deref(),
        &product_config,
        &defaults,
    );

    let mut passthrough = drop_flag_value_pair(args, "--backend");
    passthrough = drop_flag_value_pair(&passthrough, "--bind");
    passthrough.retain(|arg| arg != "--public");

    let daemon = resolve_component_command("medousa_daemon")?;
    let mut command = Command::new(&daemon.program);
    command.args(&daemon.pre_args);
    command.arg("--backend").arg(backend);
    command.arg("--bind").arg(&plan.bind);
    if let Some(mobile_url) = &plan.mobile_url {
        command.env("MEDOUSA_DAEMON_PUBLIC_URL", mobile_url);
    }
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
    let daemon_tcp_reachable = is_bind_reachable(&daemon_bind);
    let daemon_http = probe_daemon_http(&daemon_url);

    println!("medousa doctor");
    let backend_name = profile
        .daemon_backend
        .clone()
        .or_else(|| defaults.backend.clone())
        .unwrap_or_else(|| "in-memory".to_string());
    println!(
        "provider={} model={} base_url={} backend={}",
        defaults
            .provider
            .as_deref()
            .unwrap_or("(unset)"),
        defaults.model.as_deref().unwrap_or("(unset)"),
        defaults.base_url.as_deref().unwrap_or("(unset)"),
        backend_name,
    );
    let launch_backend = medousa::resolve_daemon_launch_backend(
        None,
        profile.daemon_backend.as_deref(),
        &product_config,
        &defaults,
    );
    let surreal_settings =
        medousa::resolve_surreal_connection_settings(&product_config, &defaults);
    println!(
        "daemon_url={} bind={} tcp_reachable={} http_health={}",
        daemon_url, daemon_bind, daemon_tcp_reachable, daemon_http.label
    );
    println!("daemon_launch_backend={launch_backend}");
    println!("{}", medousa::runtime::stasis_otel::stasis_otel_status_line());
    println!(
        "daemon_otel_note=surreal-ws daemon path does not attach OTLP today; hang is usually Surreal connect or schema bootstrap (watch stderr for medousa-daemon: connecting… lines)"
    );
    if let Some(endpoint) = surreal_settings.endpoint.as_deref() {
        println!("surreal_endpoint_effective={endpoint}");
    }
    if let Some(endpoint) = product_config.surreal.endpoint.as_deref() {
        println!("surreal_endpoint_product_config={endpoint}");
    }
    if profile.daemon_backend.as_deref() != Some(launch_backend.as_str()) {
        if let Some(stale) = profile.daemon_backend.as_deref() {
            println!("surreal_endpoint_onboard_profile_stale={stale}");
        }
    }
    for (label, key) in [
        ("env", "MEDOUSA_SURREAL_ENDPOINT"),
        ("env", "STASIS_SURREAL_ENDPOINT"),
    ] {
        if let Ok(value) = env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                println!("{label}_{key}={trimmed}");
            }
        }
    }
    if !daemon_http.detail.is_empty() {
        println!("daemon_http_detail={}", daemon_http.detail);
    }
    let backend = parse_backend(Some(&backend_name));
    let daemon_process_running = is_medousa_daemon_process_running();
    println!("daemon_process_running={}", daemon_process_running);
    if daemon_tcp_reachable && !daemon_http.healthy {
        println!(
            "daemon_recovery=Run: medousa start daemon-restart  (stops wedged medousa_daemon and starts a fresh one)"
        );
        println!("dashboard_url={} (available when http_health=ok)", daemon_dashboard_url(&daemon_url));
    } else if daemon_http.healthy {
        println!("dashboard_url={}", daemon_dashboard_url(&daemon_url));
    }
    println!(
        "medousa_home={} (Tauri workshop UI — cd apps/medousa-home && npm run tauri dev)",
        if daemon_http.healthy {
            "ready"
        } else {
            "needs daemon http_health=ok"
        }
    );
    if let Some(lock_path) = surrealkv_lock_path(&backend) {
        println!(
            "surrealkv_lock={} exists={}",
            lock_path.display(),
            lock_path.exists()
        );
        if lock_path.exists() && !daemon_process_running && !daemon_tcp_reachable {
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
    println!(
        "telegram_heartbeat_chat_ids={}",
        if product_config.telegram.heartbeat_chat_ids.is_empty() {
            "(none — use explicit delivery.telegram_chat_id)".to_string()
        } else {
            medousa::format_i64_csv(&product_config.telegram.heartbeat_chat_ids)
        }
    );
    println!(
        "recurring_delivery=Phase 2: use cognition_runtime_recurring_doctor or cognition_runtime_recurring_list in TUI/agent; delivery modes: explicit | current_channel | linked_channel | product_default"
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
    if daemon_http.healthy && !mcp_gateway_reachable {
        println!(
            "{}",
            format!(
                "[warn] Daemon is up but MCP gateway is not reachable at {mcp_gateway_url}. Run: medousa start mcp-gateway (see docs/mcp-gateway-setup.md)"
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
                    "[hint] No MCP servers registered. See docs/mcp-gateway-setup.md or run: medousa setup / medousa start mcp-gateway"
                        .blue()
                );
            }
        }
        if daemon_http.healthy {
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

    if daemon_http.healthy {
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
        if let Ok(continuations) = fetch_continuation_health(&daemon_url) {
            println!(
                "continuation_pending={} dead_letter_pending={} resumed={} consumed={} total={} last_resume_at={:?} last_resume_job_id={:?}",
                continuations.pending_count,
                continuations.dead_letter_pending_count,
                continuations.resumed_count,
                continuations.consumed_count,
                continuations.total_count,
                continuations.last_resume_at_utc,
                continuations.last_resume_child_job_id,
            );
        } else {
            println!("continuation_status=unavailable (daemon did not return /v1/continuations/status)");
        }
    }

    if !daemon_http.healthy {
        if daemon_tcp_reachable {
            println!("next: medousa start daemon-restart");
        } else {
            println!("next: medousa start daemon  (or medousa setup)");
        }
    }

    let _ = medousa::install_starter_openshell_policies_if_missing();
    let openshell = medousa::collect_openshell_doctor_report();
    println!(
        "openshell_gateway_url={} tcp_reachable={} readyz={} cli={} active_gateway={}",
        openshell.gateway_url,
        openshell.gateway_reachable,
        openshell.readyz_ok,
        if openshell.cli_installed {
            openshell
                .cli_version
                .as_deref()
                .unwrap_or("installed")
        } else {
            "missing"
        },
        openshell
            .active_gateway_name
            .as_deref()
            .unwrap_or("(unset)")
    );
    println!(
        "openshell_gateway_bin={} sandbox_bin={} podman_socket={} podman_active={}",
        openshell
            .gateway_binary
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "(not found)".to_string()),
        openshell
            .sandbox_binary
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "(not found)".to_string()),
        openshell.podman_socket.display(),
        openshell.podman_socket_active
    );
    println!(
        "openshell_policies_dir={} policy_count={}",
        openshell.policy_templates_dir.display(),
        openshell.policy_template_count
    );
    if !openshell.cli_installed {
        println!(
            "{}",
            "[hint] Install OpenShell CLI: uv tool install openshell — see docs/openshell-handoff-setup.md"
                .blue()
        );
    } else if !openshell.gateway_reachable {
        println!(
            "{}",
            "[warn] OpenShell gateway is not reachable. Start: openshell-gateway --disable-tls --port 8080 --bind-address 127.0.0.1 --drivers docker (or podman). See docs/openshell-handoff-setup.md"
                .yellow()
        );
    } else if !openshell.readyz_ok {
        println!(
            "{}",
            "[warn] OpenShell gateway TCP is open but /readyz failed — gateway may still be starting or compute driver is wedged."
                .yellow()
        );
    } else if !openshell.podman_socket_active {
        println!(
            "{}",
            "[hint] Podman user socket inactive — use --drivers docker if Docker is available, or enable podman.socket."
                .blue()
        );
    }
    if openshell.policy_template_count == 0 {
        println!(
            "{}",
            "[hint] No OpenShell policy templates in ~/.config/medousa/openshell-policies/ — copy from config/openshell-policies/ or re-run doctor."
                .blue()
        );
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
    println!("  PEOPLE.md");
    println!("  IDENTITY.md");
    Ok(())
}

fn run_identity_remember(args: &[String]) -> Result<()> {
    let kind = find_arg_value(args, "--kind")
        .ok_or_else(|| anyhow!("missing --kind preference|person|note"))?;
    let subject = find_arg_value(args, "--subject")
        .ok_or_else(|| anyhow!("missing --subject"))?;
    let statement = find_arg_value(args, "--statement")
        .ok_or_else(|| anyhow!("missing --statement"))?;
    let source_raw = find_arg_value(args, "--source").unwrap_or("user_direct");
    let user_id = find_arg_value(args, "--user-id")
        .map(str::to_string)
        .unwrap_or_else(|| medousa::identity_memory::resolve_identity_user_id(None));

    let store = medousa::identity_memory::build_seeded_medousa_identity_store()
        .context("build identity memory store")?;
    let writer = medousa::cognitive_identity_writer::CognitiveIdentityWriter::new(store, None);
    let source = medousa::identity_write_policy::parse_update_source(Some(source_raw))
        .map_err(anyhow::Error::msg)?;

    let rt = tokio::runtime::Runtime::new().context("start tokio runtime")?;
    let result = rt.block_on(async {
        match kind {
            "preference" => {
                writer
                    .remember_preference(
                        &user_id,
                        subject,
                        serde_json::Value::String(statement.to_string()),
                        source,
                        1.0,
                        "cli identity-remember",
                    )
                    .await
            }
            "person" => {
                let attrs = find_arg_value(args, "--attributes")
                    .map(|raw| {
                        raw.split(',')
                            .map(str::trim)
                            .filter(|segment| !segment.is_empty())
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                writer
                    .remember_contact(
                        &user_id,
                        subject,
                        statement,
                        &attrs,
                        &[],
                        source,
                        1.0,
                        "cli identity-remember",
                    )
                    .await
            }
            "note" => {
                writer
                    .remember_note(&user_id, subject, statement, source, 1.0, "cli identity-remember")
                    .await
            }
            other => Err(stasis::domain::errors::StasisError::PortFailure(format!(
                "unsupported --kind '{other}'"
            ))),
        }
    })?;

    println!(
        "identity remember kind={kind} subject={subject} committed={} requires_confirmation={} proposals={}",
        result.committed,
        result.requires_confirmation,
        result.proposal_ids.len()
    );
    if let Some(preview) = result.digest_preview.as_deref() {
        println!("digest_preview:\n{preview}");
    }
    Ok(())
}

fn run_manuscript_list() -> Result<()> {
    let entries = medousa::identity_manuscript::list_manuscripts()?;
    if entries.is_empty() {
        println!("no manuscripts found");
        println!("  project: {}", medousa::identity_manuscript::project_manuscripts_dir().display());
        println!("  user: {}", medousa::identity_manuscript::user_manuscripts_dir().display());
        return Ok(());
    }
    for entry in entries {
        println!(
            "{} ({}) scope={:?} path={}",
            entry.id,
            entry.name,
            entry.scope,
            entry.path.display()
        );
        if let Some(description) = entry.description.as_deref() {
            println!("  {description}");
        }
    }
    Ok(())
}

fn run_manuscript_install(args: &[String]) -> Result<()> {
    let source = args
        .first()
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow!("usage: medousa manuscript-install <path-to.yaml> [--project]"))?;
    let scope = if has_flag(args, "--project") {
        medousa::identity_manuscript::ManuscriptScope::Project
    } else {
        medousa::identity_manuscript::ManuscriptScope::User
    };
    let installed = medousa::identity_manuscript::install_manuscript(&source, scope)?;
    let file = medousa::identity_manuscript::load_manuscript_file(&installed)?;
    let context = medousa::identity_manuscript::build_manuscript_context(&file.metadata.id)?;
    println!(
        "manuscript installed id={} path={}",
        context.id,
        installed.display()
    );
    Ok(())
}

fn run_manuscript_validate(args: &[String]) -> Result<()> {
    let id = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| anyhow!("usage: medousa manuscript-validate <id>"))?;
    let (file, path) = medousa::identity_manuscript::load_manuscript(id)?;
    medousa::identity_manuscript::validate_manuscript(&file, &path)?;
    let context = medousa::identity_manuscript::build_manuscript_context(id)?;
    println!("manuscript valid id={} name={}", context.id, context.name);
    println!("source={}", context.source_path.display());
    if let Some(intent) = context.worker_intent.as_deref() {
        println!("worker_intent={intent}");
    }
    if context.openshell_enabled {
        println!(
            "openshell=enabled policy_template={} sandbox_from={} allow_scheduled={}",
            context
                .openshell_policy_template
                .as_deref()
                .unwrap_or("(unset)"),
            context
                .openshell_sandbox_from
                .as_deref()
                .unwrap_or("base"),
            context.openshell_allow_scheduled
        );
    }
    Ok(())
}

fn run_openshell_probe(args: &[String]) -> Result<()> {
    let _ = medousa::install_starter_openshell_policies_if_missing();
    let sandbox_from = find_arg_value(args, "--from")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("medousa-openshell-sandbox:local");
    let policy = find_arg_value(args, "--policy");
    let skip_grapheme = has_flag(args, "--skip-grapheme");

    if !skip_grapheme {
        println!("openshell-probe h6 grapheme --version (sandbox_from={sandbox_from})");
        let receipt = medousa::openshell_sandbox_run::probe_grapheme_in_sandbox(
            &sandbox_from,
            policy.as_deref(),
        )
        .map_err(anyhow::Error::msg)?;
        println!("h6_ok sandbox={} exit={:?}", receipt.sandbox_name, receipt.exit_code);
        let version_line = receipt
            .stdout
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("(empty stdout)");
        println!("grapheme_version={version_line}");
    }

    let manuscript_id = find_arg_value(args, "--manuscript-id").or_else(|| {
        args.iter()
            .find(|arg| !arg.starts_with("--"))
            .map(|value| value.as_str())
    });
    let script = find_arg_value(args, "--script").unwrap_or("scripts/echo.sh");

    if let Some(manuscript_id) = manuscript_id {
        println!(
            "openshell-probe h7 skill script={script} manuscript={manuscript_id}"
        );
        let receipt = medousa::openshell_sandbox_run::probe_skill_script_in_sandbox(
            manuscript_id,
            script,
            &sandbox_from,
            policy.as_deref(),
        )
        .map_err(anyhow::Error::msg)?;
        println!("h7_ok sandbox={} exit={:?}", receipt.sandbox_name, receipt.exit_code);
        println!("stdout={}", receipt.stdout.trim());
        return Ok(());
    }

    println!(
        "h6 complete. For H7 import fixture then probe:\n  \
         medousa skill-import config/openshell-sandbox/fixtures/echo-skill --project\n  \
         medousa openshell-probe echo-skill --script scripts/echo.sh"
    );
    Ok(())
}

fn run_skill_import(args: &[String]) -> Result<()> {
    let _ = medousa::install_starter_openshell_policies_if_missing();
    let scope = if has_flag(args, "--project") {
        medousa::identity_manuscript::ManuscriptScope::Project
    } else {
        medousa::identity_manuscript::ManuscriptScope::User
    };
    let force = has_flag(args, "--force");
    let extends = if has_flag(args, "--no-extends") {
        None
    } else {
        Some(
            find_arg_value(args, "--extends")
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("base-researcher"),
        )
    };

    let mut roots = Vec::new();
    if has_flag(args, "--from-hermes") {
        roots.extend(medousa::skill_import::preset_skill_roots(
            medousa::skill_import::SkillImportPreset::Hermes,
        ));
    }
    if has_flag(args, "--from-openclaw") {
        roots.extend(medousa::skill_import::preset_skill_roots(
            medousa::skill_import::SkillImportPreset::OpenClaw,
        ));
    }
    if has_flag(args, "--from-cursor") {
        roots.extend(medousa::skill_import::preset_skill_roots(
            medousa::skill_import::SkillImportPreset::Cursor,
        ));
        let project_cursor = medousa::skill_import::project_cursor_skills_dir();
        if project_cursor.is_dir() {
            roots.push(project_cursor);
        }
    }

    let results = if !roots.is_empty() {
        medousa::skill_import::import_skills_from_roots(&roots, scope, force, extends)?
    } else {
        let source = args
            .iter()
            .find(|arg| !arg.starts_with("--") && !arg.starts_with('-'))
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                anyhow!(
                    "usage: medousa skill-import <path> [--project] [--force] [--extends <id>|--no-extends]\n       medousa skill-import --from-hermes|--from-openclaw|--from-cursor [--project] [--force]"
                )
            })?;
        medousa::skill_import::import_skills_at_path(&source, scope, force, extends)?
    };

    if results.is_empty() {
        println!("no skills imported");
        return Ok(());
    }

    println!("imported {} skill(s) as specialties:", results.len());
    for result in results {
        println!(
            "  {} ({}) -> {}",
            result.id,
            result.name,
            result.yaml_path.display()
        );
    }
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

fn fetch_continuation_health(daemon_url: &str) -> Result<medousa::ContinuationStatusResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(format!("{daemon_url}/v1/continuations/status"))
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
    config.surreal.endpoint = if selected.surreal_endpoint.trim().is_empty() {
        None
    } else {
        Some(selected.surreal_endpoint.trim().to_string())
    };
    config.surreal.username = if selected.surreal_username.trim().is_empty() {
        None
    } else {
        Some(selected.surreal_username.trim().to_string())
    };
    config.surreal.password = if selected.surreal_password.trim().is_empty() {
        None
    } else {
        Some(selected.surreal_password.trim().to_string())
    };
    config.surreal.namespace = if selected.surreal_namespace.trim().is_empty() {
        None
    } else {
        Some(selected.surreal_namespace.trim().to_string())
    };
    config.surreal.database = if selected.surreal_database.trim().is_empty() {
        None
    } else {
        Some(selected.surreal_database.trim().to_string())
    };
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

struct DaemonLaunchPlan {
    bind: String,
    health_url: String,
    mobile_url: Option<String>,
}

fn resolve_daemon_launch_plan(
    args: &[String],
    profile: &OnboardProfile,
    product: &ProductConfig,
) -> DaemonLaunchPlan {
    let explicit_daemon_url = find_arg_value(args, "--daemon-url")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    if has_flag(args, "--public") {
        let fallback_bind = resolve_daemon_bind(args, profile, product);
        let fallback_port = medousa::daemon_api::parse_daemon_bind_port(&fallback_bind);
        let bind = medousa::daemon_api::resolve_public_daemon_bind(
            find_arg_value(args, "--bind"),
            fallback_port,
        );
        let health_url = explicit_daemon_url
            .clone()
            .unwrap_or_else(|| medousa::daemon_api::resolve_local_daemon_health_url(&bind));
        let mobile_url = medousa::daemon_api::resolve_mobile_client_daemon_url(&bind);
        return DaemonLaunchPlan {
            bind,
            health_url,
            mobile_url,
        };
    }

    let bind = resolve_daemon_bind(args, profile, product);
    let health_url = explicit_daemon_url
        .or_else(|| profile.daemon_url.clone())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string());
    DaemonLaunchPlan {
        bind,
        health_url,
        mobile_url: None,
    }
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

struct DaemonHttpProbe {
    healthy: bool,
    label: &'static str,
    detail: String,
}

fn daemon_dashboard_url(daemon_url: &str) -> String {
    format!("{}/dashboard", daemon_url.trim_end_matches('/'))
}

fn daemon_http_healthy(daemon_url: &str) -> bool {
    probe_daemon_http(daemon_url).healthy
}

fn probe_daemon_http(daemon_url: &str) -> DaemonHttpProbe {
    let daemon_url = daemon_url.trim_end_matches('/');
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            return DaemonHttpProbe {
                healthy: false,
                label: "client_error",
                detail: err.to_string(),
            };
        }
    };
    match client.get(format!("{daemon_url}/health")).send() {
        Ok(response) if response.status().is_success() => DaemonHttpProbe {
            healthy: true,
            label: "ok",
            detail: String::new(),
        },
        Ok(response) => DaemonHttpProbe {
            healthy: false,
            label: "http_error",
            detail: format!("GET /health returned {}", response.status()),
        },
        Err(err) if err.is_timeout() => DaemonHttpProbe {
            healthy: false,
            label: "timeout",
            detail: "GET /health timed out (daemon likely wedged on Surreal or startup)".to_string(),
        },
        Err(err) => DaemonHttpProbe {
            healthy: false,
            label: "unreachable",
            detail: err.to_string(),
        },
    }
}

fn print_daemon_ready_messages(plan: &DaemonLaunchPlan, already_running: bool) {
    let daemon_url = &plan.health_url;
    if already_running {
        println!(
            "{}",
            format!("[ok] Daemon healthy at {daemon_url}").green()
        );
    } else {
        println!(
            "{}",
            format!("[ok] Daemon started at {daemon_url}").green()
        );
    }
    println!(
        "{}",
        format!(
            "[ok] Stasis dashboard at {}",
            daemon_dashboard_url(daemon_url)
        )
        .green()
    );
    if let Some(mobile_url) = &plan.mobile_url {
        println!(
            "{}",
            format!("[ok] Mobile / LAN clients: {mobile_url}").green()
        );
        println!(
            "{}",
            "[info] Point Medousa Home → Settings → Connection at that URL on iPhone."
                .blue()
        );
    } else if plan.bind.starts_with("0.0.0.0:") || plan.bind.starts_with("[::]:") {
        println!(
            "{}",
            "[warn] Public bind but no LAN IP detected — chat stream URLs may fail from phones until MEDOUSA_DAEMON_PUBLIC_URL is set."
                .yellow()
        );
    }
}

fn wait_for_daemon_healthy(daemon_url: &str, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if daemon_http_healthy(daemon_url) {
            return true;
        }
        thread::sleep(Duration::from_millis(200));
    }
    false
}

#[cfg(unix)]
fn stop_medousa_daemon_process() {
    let _ = Command::new("pkill")
        .args(["-x", "medousa_daemon"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    thread::sleep(Duration::from_millis(400));
}

#[cfg(not(unix))]
fn stop_medousa_daemon_process() {}

fn wait_for_bind_closed(bind: &str, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if !is_bind_reachable(bind) {
            return true;
        }
        thread::sleep(Duration::from_millis(120));
    }
    false
}

fn restart_daemon_service(backend: &str, plan: &DaemonLaunchPlan) -> Result<()> {
    println!("{}", "[info] Stopping medousa_daemon…".yellow());
    stop_medousa_daemon_process();
    if is_bind_reachable(&plan.bind) && !wait_for_bind_closed(&plan.bind, Duration::from_secs(8)) {
        return Err(anyhow!(
            "port {} still in use after stop — check {}",
            plan.bind,
            daemon_log_path().display()
        ));
    }
    ensure_daemon_running(backend, plan)?;
    if wait_for_daemon_healthy(&plan.health_url, Duration::from_secs(30)) {
        print_daemon_ready_messages(plan, false);
        Ok(())
    } else {
        Err(anyhow!(
            "medousa_daemon restarted but /health is not ready — check {}",
            daemon_log_path().display()
        ))
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

fn ensure_daemon_running(backend: &str, plan: &DaemonLaunchPlan) -> Result<()> {
    if daemon_http_healthy(&plan.health_url) {
        return Ok(());
    }

    if is_bind_reachable(&plan.bind) {
        return Err(anyhow!(
            "port {} is open but {}/health is not responding. \
             The daemon is wedged or the wrong process owns the port. \
             Run: medousa start daemon-restart",
            plan.bind,
            plan.health_url.trim_end_matches('/')
        ));
    }

    if is_medousa_daemon_process_running() {
        if wait_for_bind_reachable(&plan.bind, Duration::from_secs(15)) {
            if daemon_http_healthy(&plan.health_url) {
                return Ok(());
            }
        }
        return Err(anyhow!(
            "medousa_daemon is running but not healthy at {} — check {}",
            plan.health_url,
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

    start_daemon_background(backend, plan)?;

    if wait_for_bind_reachable(&plan.bind, Duration::from_secs(15)) {
        return Ok(());
    }

    Err(anyhow!(
        "medousa_daemon failed to start or is not reachable at {} — check {}",
        plan.bind,
        daemon_log_path().display()
    ))
}

fn start_daemon_background(backend: &str, plan: &DaemonLaunchPlan) -> Result<()> {
    let daemon = resolve_component_command("medousa_daemon")?;
    let log = medousa::service_launch::BackgroundLog::new(daemon_log_path());
    let mut command = Command::new(&daemon.program);
    command.args(&daemon.pre_args);
    command.arg("--backend").arg(backend);
    command.arg("--bind").arg(&plan.bind);
    if let Some(mobile_url) = &plan.mobile_url {
        command.env("MEDOUSA_DAEMON_PUBLIC_URL", mobile_url);
    }
    let product_config = load_product_config();
    apply_daemon_env(&product_config);
    medousa::runtime::stasis_otel::prepare_stasis_otel_from_tui_defaults();
    println!(
        "{}",
        format!("[info] medousa_daemon --backend {backend} --bind {}", plan.bind).blue()
    );
    if let Some(mobile_url) = &plan.mobile_url {
        println!(
            "{}",
            format!("[info] Public stream URL base: {mobile_url}").blue()
        );
    }
    println!("{}", medousa::runtime::stasis_otel::stasis_otel_status_line());
    let pid = medousa::service_launch::spawn_command_background(command, &log)
        .with_context(|| format!("failed to spawn medousa_daemon using {}", daemon.program))?;
    println!(
        "daemon launch requested pid={} log={}",
        pid,
        log.path.display()
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
    let log = medousa::service_launch::BackgroundLog::new(log_path);
    let mut command = Command::new(&adapter.program);
    command.args(&adapter.pre_args);
    command.arg("--daemon-url").arg(daemon_url);
    if let Some(token) = token {
        command.arg("--token").arg(token);
    }
    command.args(extra_args);
    let pid = medousa::service_launch::spawn_command_background(command, &log)
        .with_context(|| format!("failed to spawn {binary_name} using {}", adapter.program))?;
    println!(
        "{} launch requested pid={} log={}",
        label,
        pid,
        log.path.display()
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

fn start_mcp_gateway_background() -> Result<()> {
    let _ = medousa::install_starter_gateway_config_if_missing();
    let gateway = resolve_component_command("medousa_mcp_gateway")?;
    let log = medousa::service_launch::BackgroundLog::new(mcp_gateway_log_path());
    let mut command = Command::new(&gateway.program);
    command.args(&gateway.pre_args);
    let pid = medousa::service_launch::spawn_command_background(command, &log)
        .with_context(|| {
            format!(
                "failed to spawn medousa_mcp_gateway using {}",
                gateway.program
            )
        })?;
    println!(
        "mcp gateway launch requested pid={} log={}",
        pid,
        log.path.display()
    );
    Ok(())
}

fn run_start(args: &[String]) -> Result<()> {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        print_start_help();
        return Ok(());
    }

    let service = args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("missing service name. run 'medousa start --help'"))?;

    let profile = load_onboard_profile();
    let product_config = load_product_config();
    let plan = resolve_daemon_launch_plan(args, &profile, &product_config);
    let defaults = load_tui_defaults();
    let backend = medousa::resolve_daemon_launch_backend(
        find_arg_value(args, "--backend"),
        profile.daemon_backend.as_deref(),
        &product_config,
        &defaults,
    );

    match service.as_str() {
        "daemon" => start_daemon_service(&backend, &plan),
        "daemon-restart" | "restart-daemon" => restart_daemon_service(&backend, &plan),
        "mcp-gateway" | "mcp_gateway" | "mcp" => start_mcp_gateway_service(),
        "discord" => start_discord_service(&plan.health_url),
        "telegram" => start_telegram_service(&plan.health_url),
        "slack" => start_slack_service(&plan.health_url),
        "whatsapp" => start_whatsapp_service(&plan.health_url, &product_config),
        "all" => {
            start_daemon_service(&backend, &plan)?;
            start_mcp_gateway_service()?;
            start_discord_service(&plan.health_url).ok();
            start_telegram_service(&plan.health_url).ok();
            start_slack_service(&plan.health_url).ok();
            start_whatsapp_service(&plan.health_url, &product_config).ok();
            Ok(())
        }
        other => Err(anyhow!(
            "unknown service '{other}'. try: daemon, mcp-gateway, discord, telegram, slack, whatsapp, all"
        )),
    }
}

fn start_daemon_service(backend: &str, plan: &DaemonLaunchPlan) -> Result<()> {
    if daemon_http_healthy(&plan.health_url) {
        print_daemon_ready_messages(plan, true);
        return Ok(());
    }
    ensure_daemon_running(backend, plan)?;
    if wait_for_daemon_healthy(&plan.health_url, Duration::from_secs(30)) {
        print_daemon_ready_messages(plan, false);
    } else {
        println!(
            "{}",
            format!(
                "[warn] Daemon process started but /health not ready — try: medousa start daemon-restart (log: {})",
                daemon_log_path().display()
            )
            .yellow()
        );
    }
    Ok(())
}

fn start_mcp_gateway_service() -> Result<()> {
    let mcp_bind = medousa::DEFAULT_MCP_GATEWAY_BIND;
    if is_bind_reachable(mcp_bind) {
        println!(
            "{}",
            format!(
                "[ok] MCP gateway already running at {}",
                medousa::DEFAULT_MCP_GATEWAY_URL
            )
            .green()
        );
        return Ok(());
    }
    start_mcp_gateway_background()?;
    if wait_for_bind_reachable(mcp_bind, Duration::from_secs(10)) {
        println!(
            "{}",
            format!(
                "[ok] MCP gateway running at {}",
                medousa::DEFAULT_MCP_GATEWAY_URL
            )
            .green()
        );
    } else {
        println!(
            "{}",
            format!(
                "[warn] MCP gateway started but not reachable yet. Check {}",
                mcp_gateway_log_path().display()
            )
            .yellow()
        );
    }
    Ok(())
}

fn start_discord_service(daemon_url: &str) -> Result<()> {
    let token = load_discord_bot_token()
        .ok_or_else(|| anyhow!("discord token missing — run medousa setup"))?;
    start_discord_background(daemon_url, &token)?;
    println!("{}", "[ok] Discord adapter started in background.".green());
    Ok(())
}

fn start_telegram_service(daemon_url: &str) -> Result<()> {
    let token = load_telegram_bot_token()
        .ok_or_else(|| anyhow!("telegram token missing — run medousa setup"))?;
    start_telegram_background(daemon_url, &token)?;
    println!("{}", "[ok] Telegram adapter started in background.".green());
    Ok(())
}

fn start_slack_service(daemon_url: &str) -> Result<()> {
    let bot = load_slack_bot_token()
        .ok_or_else(|| anyhow!("slack bot token missing — run medousa setup"))?;
    let app = load_slack_app_token()
        .ok_or_else(|| anyhow!("slack app token missing — run medousa setup"))?;
    start_slack_background(daemon_url, &bot, &app)?;
    println!("{}", "[ok] Slack adapter started in background.".green());
    Ok(())
}

fn start_whatsapp_service(daemon_url: &str, product_config: &ProductConfig) -> Result<()> {
    start_whatsapp_background(
        daemon_url,
        &product_config.whatsapp.deliver_bind,
        None,
    )?;
    println!("{}", "[ok] WhatsApp adapter started in background.".green());
    Ok(())
}

fn print_start_help() {
    println!("medousa start — launch Medousa services in the background");
    println!();
    println!("USAGE:");
    println!("  medousa start <service> [--backend <name>] [--bind <host:port>] [--public] [--daemon-url <url>]");
    println!();
    println!("FLAGS:");
    println!("  --public      Bind daemon to 0.0.0.0 and print LAN URL for phones (mobile dev)");
    println!();
    println!("SERVICES:");
    println!("  daemon        Background medousa_daemon (engine)");
    println!("  daemon-restart  Stop wedged daemon and start fresh (same as restart-daemon)");
    println!("  mcp-gateway   Background medousa_mcp_gateway (MCP broker)");
    println!("  discord       Background Discord adapter (needs bot token)");
    println!("  telegram      Background Telegram adapter (needs bot token)");
    println!("  slack         Background Slack adapter (needs bot + app tokens)");
    println!("  whatsapp      Background WhatsApp adapter");
    println!("  all           daemon + mcp-gateway + any configured adapters");
    println!();
    println!("Logs: ~/.local/share/medousa/logs/<service>.log");
    println!("MCP config: ~/.config/medousa/mcp-gateway.toml (see docs/mcp-gateway-setup.md)");
    println!();
    println!("EXAMPLES:");
    println!("  medousa start daemon --backend surreal-mem");
    println!("  medousa start daemon --public          # iPhone / LAN mobile dev");
    println!("  medousa start mcp-gateway");
    println!("  medousa start all");
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

fn run_workspace(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("snapshot");
    let daemon_url = find_arg_value(args, "--daemon-url")
        .map(str::to_string)
        .or_else(|| std::env::var("MEDOUSA_DAEMON_URL").ok())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string());
    let daemon_url = daemon_url.trim_end_matches('/').to_string();

    if !daemon_http_healthy(&daemon_url) {
        return Err(anyhow!(
            "daemon not healthy at {daemon_url} — run: medousa start daemon"
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to build HTTP client")?;

    let (method, path, body) = match sub {
        "cancel" => {
            let card_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: medousa workspace cancel <card-id>"))?;
            ("POST".to_string(), format!("/v1/workspace/cards/{card_id}/cancel"), None)
        }
        "retry" => {
            let card_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: medousa workspace retry <card-id>"))?;
            ("POST".to_string(), format!("/v1/workspace/cards/{card_id}/retry"), None)
        }
        "link-vault" => {
            let card_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: medousa workspace link-vault <card-id> --path <vault-path>"))?;
            let vault_path = find_arg_value(args, "--path")
                .ok_or_else(|| anyhow!("usage: medousa workspace link-vault <card-id> --path <vault-path>"))?;
            let payload = serde_json::json!({ "vault_path": vault_path });
            (
                "POST".to_string(),
                format!("/v1/workspace/cards/{card_id}/link-vault"),
                Some(payload.to_string()),
            )
        }
        "cards" => {
            let mut query = Vec::new();
            if let Some(limit) = find_arg_value(args, "--limit") {
                query.push(format!("limit={limit}"));
            }
            if has_flag(args, "--include-terminal") {
                query.push("include_terminal=true".to_string());
            }
            if let Some(session) = find_arg_value(args, "--session") {
                query.push(format!("session_id={session}"));
            }
            if let Some(column) = find_arg_value(args, "--column") {
                query.push(format!("column={column}"));
            }
            let suffix = if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.join("&"))
            };
            (
                "GET".to_string(),
                format!("/v1/workspace/cards{suffix}"),
                None,
            )
        }
        "feed" => {
            let mut query = Vec::new();
            if let Some(limit) = find_arg_value(args, "--limit") {
                query.push(format!("limit={limit}"));
            }
            if let Some(since) = find_arg_value(args, "--since-id") {
                query.push(format!("since_id={since}"));
            }
            if let Some(card) = find_arg_value(args, "--card") {
                query.push(format!("card_id={card}"));
            }
            let suffix = if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.join("&"))
            };
            (
                "GET".to_string(),
                format!("/v1/workspace/feed{suffix}"),
                None,
            )
        }
        "card" => {
            let card_id = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: medousa workspace card <card-id>"))?;
            (
                "GET".to_string(),
                format!("/v1/workspace/cards/{card_id}"),
                None,
            )
        }
        "stream" => {
            return run_workspace_stream(&daemon_url, args);
        }
        "snapshot" | _ => {
            let mut query = Vec::new();
            if let Some(limit) = find_arg_value(args, "--feed-tail") {
                query.push(format!("feed_tail_limit={limit}"));
            }
            let suffix = if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.join("&"))
            };
            (
                "GET".to_string(),
                format!("/v1/workspace/snapshot{suffix}"),
                None,
            )
        }
    };

    let url = format!("{daemon_url}{path}");
    let response = if method == "POST" {
        let mut request = client.post(&url);
        if let Some(payload) = body {
            request = request
                .header("content-type", "application/json")
                .body(payload);
        }
        request
            .send()
            .with_context(|| format!("POST {url} failed"))?
    } else {
        client
            .get(&url)
            .send()
            .with_context(|| format!("GET {url} failed"))?
    };
    let status = response.status();
    let body = response.text().context("failed to read workspace response")?;
    if !status.is_success() {
        return Err(anyhow!("workspace request failed ({status}): {body}"));
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&body).context("workspace response was not valid JSON")?;
    println!("{}", serde_json::to_string_pretty(&parsed)?);
    Ok(())
}

fn run_workspace_stream(daemon_url: &str, args: &[String]) -> Result<()> {
    let mut query = Vec::new();
    if let Some(revision) = find_arg_value(args, "--since-revision") {
        query.push(format!("since_revision={revision}"));
    }
    if let Some(session) = find_arg_value(args, "--session") {
        query.push(format!("session_id={session}"));
    }
    if let Some(limit) = find_arg_value(args, "--feed-tail") {
        query.push(format!("feed_tail_limit={limit}"));
    }
    let suffix = if query.is_empty() {
        String::new()
    } else {
        format!("?{}", query.join("&"))
    };
    let url = format!("{daemon_url}/v1/workspace/stream{suffix}");
    let max_events = find_arg_value(args, "--max-events")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .context("failed to build HTTP client")?;

    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("GET {url} failed"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(anyhow!("workspace stream failed ({status}): {body}"));
    }

    let mut buf = String::new();
    let mut event_count = 0usize;
    let mut response = response;
    let mut chunk_buf = [0u8; 8192];
    loop {
        let n = response
            .read(&mut chunk_buf)
            .context("workspace stream read failed")?;
        if n == 0 {
            break;
        }
        buf.push_str(&String::from_utf8_lossy(&chunk_buf[..n]));

        while let Some(idx) = buf.find("\n\n") {
            let frame = buf[..idx].to_string();
            buf = buf[idx + 2..].to_string();
            let Some(data) = parse_workspace_sse_data(&frame) else {
                continue;
            };
            println!("{data}");
            event_count += 1;
            if max_events > 0 && event_count >= max_events {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn run_vault(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let daemon_url = find_arg_value(args, "--daemon-url")
        .map(str::to_string)
        .or_else(|| std::env::var("MEDOUSA_DAEMON_URL").ok())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string());
    let daemon_url = daemon_url.trim_end_matches('/').to_string();

    if !daemon_http_healthy(&daemon_url) {
        return Err(anyhow!(
            "daemon not healthy at {daemon_url} — run: medousa start daemon"
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")?;

    let (method, path, body) = match sub {
        "list" => {
            let mut query = Vec::new();
            if let Some(prefix) = find_arg_value(args, "--prefix") {
                query.push(format!("prefix={prefix}"));
            }
            if let Some(limit) = find_arg_value(args, "--limit") {
                query.push(format!("limit={limit}"));
            }
            let suffix = if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.join("&"))
            };
            (
                "GET".to_string(),
                format!("/v1/vault/notes{suffix}"),
                None,
            )
        }
        "read" => {
            let note_path = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: medousa vault read <path>"))?;
            (
                "GET".to_string(),
                format!("/v1/vault/notes/{note_path}"),
                None,
            )
        }
        "write" => {
            let note_path = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: medousa vault write <path> [--content <text>|--stdin]"))?;
            let content = if has_flag(args, "--stdin") {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                find_arg_value(args, "--content")
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("usage: medousa vault write <path> --content <markdown>"))?
            };
            let payload = serde_json::json!({ "path": note_path, "content": content });
            (
                "POST".to_string(),
                "/v1/vault/notes".to_string(),
                Some(payload.to_string()),
            )
        }
        "search" => {
            let query = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: medousa vault search <query>"))?;
            let encoded = urlencoding_encode(query);
            let mut suffix = format!("?q={encoded}");
            if let Some(limit) = find_arg_value(args, "--limit") {
                suffix.push_str(&format!("&limit={limit}"));
            }
            (
                "GET".to_string(),
                format!("/v1/vault/search{suffix}"),
                None,
            )
        }
        "delete" => {
            let note_path = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: medousa vault delete <path>"))?;
            (
                "DELETE".to_string(),
                format!("/v1/vault/notes/{note_path}"),
                None,
            )
        }
        "backlinks" => {
            let note_path = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: medousa vault backlinks <path>"))?;
            let encoded = urlencoding_encode(note_path);
            (
                "GET".to_string(),
                format!("/v1/vault/backlinks?path={encoded}"),
                None,
            )
        }
        other => return Err(anyhow!("unknown vault subcommand: {other}")),
    };

    let url = format!("{daemon_url}{path}");
    let response = match method.as_str() {
        "POST" => {
            let mut request = client.post(&url);
            if let Some(payload) = body {
                request = request
                    .header("content-type", "application/json")
                    .body(payload);
            }
            request.send().with_context(|| format!("POST {url} failed"))?
        }
        "DELETE" => client
            .delete(&url)
            .send()
            .with_context(|| format!("DELETE {url} failed"))?,
        _ => client
            .get(&url)
            .send()
            .with_context(|| format!("GET {url} failed"))?,
    };

    let status = response.status();
    let body = response.text().context("failed to read vault response")?;
    if !status.is_success() {
        return Err(anyhow!("vault request failed ({status}): {body}"));
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&body).context("vault response was not valid JSON")?;
    println!("{}", serde_json::to_string_pretty(&parsed)?);
    Ok(())
}

fn urlencoding_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn parse_workspace_sse_data(frame: &str) -> Option<String> {
    let mut data_lines = Vec::new();
    for line in frame.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start());
        }
    }
    if data_lines.is_empty() {
        return None;
    }
    let joined = data_lines.join("\n");
    let parsed: serde_json::Value = serde_json::from_str(&joined).ok()?;
    serde_json::to_string_pretty(&parsed).ok()
}

fn print_help() {
    println!("Medousa launcher");
    println!();
    println!("USAGE:");
    println!("  medousa onboard|setup|init [--yes] [--advanced] [--provider <name>] [--model <name>] [--base-url <url>] [--api-key <key>] [--backend <name>] [--daemon-url <url>] [--no-daemon] [--no-tui]");
    println!("  medousa start <service>   Background daemon, mcp-gateway, adapters (see: medousa start --help)");
    println!("  medousa tui [--daemon-url <url>] [--no-daemon] [-- <medousa_tui args>]");
    println!("  medousa daemon [--backend <name>] [--bind <host:port>] [--public] [-- <medousa_daemon args>]  (foreground)");
    println!("  medousa discord [--daemon-url <url>] [--token <token>] [-- <medousa_discord args>]");
    println!("  medousa telegram [--daemon-url <url>] [--token <token>] [-- <medousa_telegram args>]");
    println!("    Telegram allowlist is configured in medousa setup (product_config.json).");
    println!("  medousa slack [--daemon-url <url>] [--bot-token <xoxb-…>] [--app-token <xapp-…>]");
    println!("  medousa whatsapp [--daemon-url <url>] [--deliver-bind <host:port>] [--session-db <path>]");
    println!("    WhatsApp uses a local deliver endpoint; session persists in ~/.local/share/medousa/whatsapp/session.db by default.");
    println!("  medousa doctor");
    println!("  medousa identity-export [--user-id <id>] [--dir <path>]");
    println!("  medousa identity-remember --kind <preference|person|note> --subject <key|name> --statement <text> [--source user_direct] [--attributes a,b]");
    println!("  medousa manuscript-list");
    println!("  medousa manuscript-validate <id>");
    println!("  medousa manuscript-install <path-to.yaml> [--project]");
    println!("  medousa skill-import <path> [--project] [--force] [--extends <id>|--no-extends]");
    println!("  medousa skill-import --from-hermes|--from-openclaw|--from-cursor [--project] [--force]");
    println!("  medousa openshell-probe [<manuscript-id>] [--script scripts/echo.sh] [--from medousa-openshell-sandbox:local] [--policy skill-sandbox] [--skip-grapheme]");
    println!("  medousa workspace [snapshot|cards|feed|stream|card <id>|cancel <id>|retry <id>|link-vault <id> --path <vault-path>] [--daemon-url <url>] [--limit N] [--include-terminal] [--column backlog|in_flight|wrapping_up|done|blocked] [--since-revision N] [--max-events N]");
    println!("  medousa vault [list|read <path>|write <path> --content <md>|--stdin|search <q>|delete <path>|backlinks <path>] [--daemon-url <url>] [--prefix <dir/>] [--limit N]");
    println!();
    println!("EXAMPLES:");
    println!("  medousa onboard");
    println!("  medousa setup");
    println!("  medousa start daemon && medousa start mcp-gateway");
    println!("  medousa onboard --yes --provider ollama --model llama3.2 --no-daemon");
    println!("  medousa tui");
    println!("  medousa discord");
    println!("  medousa telegram");
    println!("  medousa slack");
    println!("  medousa whatsapp");
    println!("  medousa doctor");
}