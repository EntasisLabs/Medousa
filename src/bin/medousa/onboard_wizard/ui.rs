use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap},
};
use ratatui_image::{StatefulImage, protocol::StatefulProtocol};


use super::model::{BackendChoice, ProviderChoice, WizardState, WizardStep};

pub(crate) fn render(
    frame: &mut Frame,
    state: &WizardState,
    logo_protocol: Option<&mut StatefulProtocol>,
    logo_notice: Option<&str>,
) {
    let area = centered_rect(88, 88, frame.area());
    frame.render_widget(Clear, area);

    let shell = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(vec![Span::styled(
            " Medousa ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )]));
    frame.render_widget(shell.clone(), area);

    let inner = shell.inner(area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(inner);

    let (step_index, step_total) = state.step_position();
    let progress = if step_total == 0 {
        0.0
    } else {
        step_index as f64 / step_total as f64
    };

    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            state.step_title(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(title, sections[0]);

    let gauge = Gauge::default()
        .ratio(progress)
        .gauge_style(
            Style::default()
                .fg(Color::Magenta)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .label(format!("{}%", (progress * 100.0).round() as i32));
    frame.render_widget(gauge, sections[1]);

    if state.step == WizardStep::Welcome {
        render_welcome_body(frame, sections[2], state, logo_protocol, logo_notice);
    } else {
        let body = Paragraph::new(body_text(state))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Setup "),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(body, sections[2]);
    }

    let footer = Paragraph::new(footer_text(state)).wrap(Wrap { trim: false });
    frame.render_widget(footer, sections[3]);
}

fn render_welcome_body(
    frame: &mut Frame,
    area: Rect,
    state: &WizardState,
    logo_protocol: Option<&mut StatefulProtocol>,
    logo_notice: Option<&str>,
) {
    let shell = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Setup ");
    frame.render_widget(shell.clone(), area);

    let inner = shell.inner(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(inner);

    let logo_shell = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Medousa ");
    frame.render_widget(logo_shell.clone(), columns[0]);

    let logo_inner = logo_shell.inner(columns[0]);
    if let Some(protocol) = logo_protocol {
        frame.render_stateful_widget(StatefulImage::new(), logo_inner, protocol);
    } else {
        let fallback = Paragraph::new(Line::from(
            logo_notice.unwrap_or("Logo unavailable on this terminal."),
        ))
        .wrap(Wrap { trim: false });
        frame.render_widget(fallback, logo_inner);
    }

    let body = Paragraph::new(body_text(state)).wrap(Wrap { trim: false });
    frame.render_widget(body, columns[1]);
}

fn body_text(state: &WizardState) -> Text<'static> {
    let mut lines = Vec::new();

    match state.step {
        WizardStep::Welcome => {
            lines.push(Line::from("From zero to first chat in seconds."));
            lines.push(Line::from(""));
            lines.push(Line::from(if state.bootstrap.ollama_detected {
                "Local Ollama detected."
            } else {
                "Ready for any OpenAI-compatible provider."
            }));
            lines.push(Line::from(""));
            lines.push(Line::from("Press Enter to start."));
        }
        WizardStep::Provider => {
            lines.push(Line::from("Choose a provider:"));
            lines.push(Line::from(""));
            lines.push(option_line(
                state.provider_choice == ProviderChoice::Ollama,
                "Ollama",
                if state.bootstrap.ollama_detected {
                    "local, no API key"
                } else {
                    "install locally if preferred"
                },
            ));
            lines.push(option_line(
                state.provider_choice == ProviderChoice::OpenAi,
                "OpenAI-compatible API",
                "hosted API keys",
            ));
            lines.push(option_line(
                state.provider_choice == ProviderChoice::Custom,
                "Custom provider id",
                "advanced",
            ));
        }
        WizardStep::CustomProvider => {
            lines.push(Line::from("Your provider identifier:"));
            lines.push(Line::from(""));
            lines.push(input_line("Provider id", &state.custom_provider));
        }
        WizardStep::Model => {
            lines.push(Line::from("Choose a model:"));
            lines.push(Line::from(""));
            lines.push(input_line("Model", &state.model));
        }
        WizardStep::BaseUrl => {
            lines.push(Line::from("Endpoint override:"));
            lines.push(Line::from(""));
            lines.push(input_line("Base URL", &state.base_url));
            if state.provider_choice == ProviderChoice::Ollama {
                lines.push(Line::from("Leave blank for Ollama default."));
            } else {

                lines.push(Line::from("Leave blank for provider default."));
            }
        }
        WizardStep::ApiKey => {
            lines.push(Line::from("Add API key:"));
            lines.push(Line::from(""));
            lines.push(input_line("API key", &mask_secret(&state.api_key)));
            lines.push(Line::from("Leave empty to configure later."));
        }
        WizardStep::Backend => {
            lines.push(Line::from("Select storage backend:"));
            lines.push(Line::from(""));
            lines.push(option_line(
                matches!(state.backend_choice, BackendChoice::InMemory),
                "in-memory",
                "fastest local iteration, no persistence",
            ));
            lines.push(option_line(
                matches!(state.backend_choice, BackendChoice::SurrealMem),
                "surreal-mem",
                "durable in-memory session store",
            ));
            lines.push(option_line(
                matches!(state.backend_choice, BackendChoice::SurrealKv { .. }),
                "surreal-kv",
                "persistent local file-based store",
            ));
            lines.push(option_line(
                matches!(state.backend_choice, BackendChoice::SurrealWs { .. }),
                "surreal-ws",
                "remote SurrealDB server over WebSocket",
            ));
        }
        WizardStep::BackendSurrealKvPath => {
            lines.push(Line::from("SurrealKV database path:"));
            lines.push(Line::from(""));
            let default_path = &state.bootstrap.surreal_kv_default_path;
            let display = if state.backend_config_input.trim().is_empty() {
                format!("(default: {})", default_path)
            } else {
                state.backend_config_input.clone()
            };
            lines.push(input_line("Path", &display));
            lines.push(Line::from("Leave empty for default."));
        }
        WizardStep::BackendSurrealWsEndpoint => {
            lines.push(Line::from("SurrealDB WebSocket endpoint:"));
            lines.push(Line::from(""));
            lines.push(input_line("Endpoint", &state.backend_config_input));
            lines.push(Line::from("Example: ws://127.0.0.1:8000/rpc"));
        }
        WizardStep::DaemonUrl => {
            lines.push(Line::from("Set runtime URL:"));
            lines.push(Line::from(""));
            lines.push(input_line("Daemon URL", &state.daemon_url));
        }
        WizardStep::DaemonBind => {
            lines.push(Line::from("Daemon listen address:"));
            lines.push(Line::from(""));
            lines.push(input_line("Bind", &state.daemon_bind));
            lines.push(Line::from("Example: 127.0.0.1:7419"));
        }
        WizardStep::LaunchDaemon => {
            lines.push(Line::from("Start runtime in background?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Start daemon", state.start_daemon));
            lines.push(Line::from(""));
        }
        WizardStep::McpGateway => {
            lines.push(Line::from("Install MCP gateway config?"));
            lines.push(Line::from(""));
            lines.push(toggle_line(
                "Write ~/.config/medousa/mcp-gateway.toml",
                state.configure_mcp_gateway,
            ));
            lines.push(Line::from(if state.bootstrap.existing_mcp_gateway_config {
                "Existing config detected — skip to keep current file."
            } else {
                "Creates a starter config with mock MCP servers."
            }));
            lines.push(Line::from(
                "Optional env: MEDOUSA_MCP_POLICY_TOKEN, MEDOUSA_MCP_TURN_TOKEN_SECRET",
            ));
            lines.push(Line::from(""));
        }
        WizardStep::LaunchMcpGateway => {
            lines.push(Line::from("Start MCP gateway in background?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Start medousa_mcp_gateway", state.start_mcp_gateway));
            lines.push(Line::from("Works even if you skipped config install (uses existing or starter file)."));
            lines.push(Line::from("Gateway listens on 127.0.0.1:7420 — see docs/mcp-gateway-setup.md"));
            lines.push(Line::from(""));
        }
        WizardStep::LaunchChat => {
            lines.push(Line::from("Open chat after setup?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Launch chat", state.launch_tui));
            lines.push(Line::from(""));
        }
        WizardStep::Discord => {
            lines.push(Line::from("Configure Discord?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Configure Discord", state.configure_discord));
            lines.push(Line::from(if state.bootstrap.existing_discord_token {
                "Stored token detected."
            } else {
                "No token stored."
            }));
            lines.push(Line::from(""));
        }
        WizardStep::DiscordToken => {
            lines.push(Line::from("Add Discord bot token:"));
            lines.push(Line::from(""));
            lines.push(input_line("Discord token", &mask_secret(&state.discord_token)));
            lines.push(Line::from(if state.bootstrap.existing_discord_token {
                "Keep existing Discord token."
            } else {
                "Required when enabled."
            }));
        }
        WizardStep::DiscordPrefix => {
            lines.push(Line::from("Command prefix for Discord messages:"));
            lines.push(Line::from(""));
            lines.push(input_line("Prefix", &state.discord_command_prefix));
            lines.push(Line::from("Messages starting with this prefix route to Medousa."));
        }
        WizardStep::DiscordHeartbeat => {
            lines.push(Line::from("Optional heartbeat nudges for Discord:"));
            lines.push(Line::from(""));
            lines.push(toggle_line(
                "Enable heartbeat nudges",
                state.discord_heartbeat_nudges_enabled,
            ));
            lines.push(Line::from(""));
            lines.push(input_line(
                "Channel ids",
                if state.discord_heartbeat_channel_ids.trim().is_empty() {
                    "(all channels when enabled)"
                } else {
                    state.discord_heartbeat_channel_ids.trim()
                },
            ));
            lines.push(Line::from("Comma-separated channel ids. Space toggles nudges."));
        }
        WizardStep::LaunchDiscord => {
            lines.push(Line::from("Start Discord in background?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Start Discord", state.start_discord));
            lines.push(Line::from(""));
        }
        WizardStep::Telegram => {
            lines.push(Line::from("Configure Telegram?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Configure Telegram", state.configure_telegram));
            lines.push(Line::from(if state.bootstrap.existing_telegram_token {
                "Stored token detected."
            } else {
                "No token stored."
            }));
            lines.push(Line::from(""));
        }
        WizardStep::TelegramToken => {
            lines.push(Line::from("Add Telegram bot token:"));
            lines.push(Line::from(""));
            lines.push(input_line("Telegram token", &mask_secret(&state.telegram_token)));
            lines.push(Line::from(if state.bootstrap.existing_telegram_token {
                "Keep existing Telegram token."
            } else {
                "Required when enabled."
            }));
        }
        WizardStep::TelegramAllowUserIds => {
            lines.push(Line::from(
                "Sender lock — only accept from these user ids:",
            ));
            lines.push(Line::from(""));
            lines.push(input_line("Allowed user ids", &state.telegram_allow_user_ids));
            lines.push(Line::from("Comma-separated numeric ids, e.g. 123456789,987654321."));
            lines.push(Line::from("Leave blank to allow all Telegram users."));
        }
        WizardStep::TelegramHeartbeat => {
            lines.push(Line::from("Optional heartbeat nudges for Telegram:"));
            lines.push(Line::from(""));
            lines.push(toggle_line(
                "Enable heartbeat nudges",
                state.telegram_heartbeat_nudges_enabled,
            ));
            lines.push(Line::from(""));
            lines.push(input_line(
                "Chat ids",
                if state.telegram_heartbeat_chat_ids.trim().is_empty() {
                    "(all chats when enabled)"
                } else {
                    state.telegram_heartbeat_chat_ids.trim()
                },
            ));
            lines.push(Line::from("Comma-separated chat ids. Space toggles nudges."));
        }
        WizardStep::LaunchTelegram => {
            lines.push(Line::from("Start Telegram in background?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Start Telegram", state.start_telegram));
            lines.push(Line::from(""));
        }
        WizardStep::Slack => {
            lines.push(Line::from("Configure Slack (Socket Mode)?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Configure Slack", state.configure_slack));
            lines.push(Line::from(
                "Requires xoxb- bot token and xapp- app token with connections:write.",
            ));
            lines.push(Line::from(if state.bootstrap.existing_slack_bot_token
                && state.bootstrap.existing_slack_app_token
            {
                "Stored Slack tokens detected."
            } else {
                "No Slack tokens stored."
            }));
            lines.push(Line::from(""));
        }
        WizardStep::SlackBotToken => {
            lines.push(Line::from("Slack bot token (xoxb-…):"));
            lines.push(Line::from(""));
            lines.push(input_line("Bot token", &mask_secret(&state.slack_bot_token)));
            lines.push(Line::from(if state.bootstrap.existing_slack_bot_token {
                "Keep existing bot token when left blank."
            } else {
                "Required when Slack is enabled."
            }));
        }
        WizardStep::SlackAppToken => {
            lines.push(Line::from("Slack app token for Socket Mode (xapp-…):"));
            lines.push(Line::from(""));
            lines.push(input_line("App token", &mask_secret(&state.slack_app_token)));
            lines.push(Line::from(if state.bootstrap.existing_slack_app_token {
                "Keep existing app token when left blank."
            } else {
                "Required when Slack is enabled."
            }));
        }
        WizardStep::SlackAllowUserIds => {
            lines.push(Line::from("Optional Slack sender allowlist:"));
            lines.push(Line::from(""));
            lines.push(input_line("Allowed user ids", &state.slack_allow_user_ids));
            lines.push(Line::from("Comma-separated Slack user ids (U…). Blank = all users."));
        }
        WizardStep::LaunchSlack => {
            lines.push(Line::from("Start Slack adapter in background?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Start Slack", state.start_slack));
            lines.push(Line::from(""));
        }
        WizardStep::WhatsApp => {
            lines.push(Line::from("Configure WhatsApp (whatsapp-rust)?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Configure WhatsApp", state.configure_whatsapp));
            lines.push(Line::from(
                "Unofficial WhatsApp Web client — review Meta ToS. First run shows QR pairing.",
            ));
            lines.push(Line::from("Session persists in ~/.local/share/medousa/whatsapp/session.db"));
            lines.push(Line::from(""));
        }
        WizardStep::WhatsAppDeliverBind => {
            lines.push(Line::from("Local deliver endpoint bind (daemon outbox push):"));
            lines.push(Line::from(""));
            lines.push(input_line("Deliver bind", &state.whatsapp_deliver_bind));
            lines.push(Line::from("Default 127.0.0.1:7422 — POST /v1/deliver for outbound messages."));
        }
        WizardStep::WhatsAppAllowUserIds => {
            lines.push(Line::from("Optional WhatsApp sender allowlist:"));
            lines.push(Line::from(""));
            lines.push(input_line("Allowed senders", &state.whatsapp_allow_user_ids));
            lines.push(Line::from(
                "Comma-separated JIDs or suffixes. Blank = all senders.",
            ));
        }
        WizardStep::LaunchWhatsApp => {
            lines.push(Line::from("Start WhatsApp adapter in background?"));
            lines.push(Line::from(""));
            lines.push(toggle_line("Start WhatsApp", state.start_whatsapp));
            lines.push(Line::from("Scan QR in terminal log if first pairing."));
            lines.push(Line::from(""));
        }
        WizardStep::TuiResponseDepth => {
            lines.push(Line::from("Default response depth for chat:"));
            lines.push(Line::from(""));
            lines.push(option_line(
                state.tui_response_depth_mode == "concise",
                "concise",
                "shorter answers, faster turns",
            ));
            lines.push(option_line(
                state.tui_response_depth_mode == "standard",
                "standard",
                "balanced detail",
            ));
            lines.push(option_line(
                state.tui_response_depth_mode == "deep",
                "deep",
                "thorough reasoning and context",
            ));
        }
        WizardStep::Confirm => {
            lines.push(Line::from("Review setup choices:"));
            lines.push(Line::from(""));
            lines.push(summary_line("Provider", &state.provider_id()));
            lines.push(summary_line("Model", &state.model));
            lines.push(summary_line(
                "Base URL",
                if state.base_url.trim().is_empty() {
                    "(default)"
                } else {
                    state.base_url.trim()
                },
            ));
            lines.push(summary_line(
                "API key",
                if state.api_key.trim().is_empty() {
                    "(not set now)"
                } else {
                    "(configured)"
                },
            ));
            lines.push(summary_line("Backend", &state.backend_choice.as_backend_id()));
            if state.bootstrap.advanced_mode {
                lines.push(summary_line("Daemon URL", &state.daemon_url));
            }
            lines.push(summary_line(
                "Daemon bind",
                if state.daemon_bind.trim().is_empty() {
                    state.bootstrap.initial_daemon_bind.as_str()
                } else {
                    state.daemon_bind.trim()
                },
            ));
            lines.push(summary_line(
                "Start daemon",
                if state.start_daemon { "yes" } else { "no" },
            ));
            lines.push(summary_line(
                "MCP gateway config",
                if state.configure_mcp_gateway { "yes" } else { "no" },
            ));
            lines.push(summary_line(
                "Start MCP gateway",
                if state.configure_mcp_gateway && state.start_mcp_gateway {
                    "yes"
                } else {
                    "no"
                },
            ));
            lines.push(summary_line(
                "Launch chat",
                if state.launch_tui { "yes" } else { "no" },
            ));
            lines.push(summary_line(
                "Discord setup",
                if state.configure_discord { "yes" } else { "no" },
            ));
            if state.configure_discord {
                lines.push(summary_line(
                    "Discord prefix",
                    if state.discord_command_prefix.trim().is_empty() {
                        state.bootstrap.initial_discord_command_prefix.as_str()
                    } else {
                        state.discord_command_prefix.trim()
                    },
                ));
                lines.push(summary_line(
                    "Discord heartbeat",
                    if state.discord_heartbeat_nudges_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    },
                ));
            }
            lines.push(summary_line(
                "Start Discord",
                if state.configure_discord && state.start_discord {
                    "yes"
                } else {
                    "no"
                },
            ));
            lines.push(summary_line(
                "Telegram setup",
                if state.configure_telegram { "yes" } else { "no" },
            ));
            lines.push(summary_line(
                "Telegram allowed users",
                if !state.configure_telegram || state.telegram_allow_user_ids.trim().is_empty() {
                    "(all users)"
                } else {
                    state.telegram_allow_user_ids.trim()
                },
            ));
            if state.configure_telegram {
                lines.push(summary_line(
                    "Telegram heartbeat",
                    if state.telegram_heartbeat_nudges_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    },
                ));
            }
            lines.push(summary_line(
                "Start Telegram",
                if state.configure_telegram && state.start_telegram {
                    "yes"
                } else {
                    "no"
                },
            ));
            lines.push(summary_line(
                "Slack setup",
                if state.configure_slack { "yes" } else { "no" },
            ));
            if state.configure_slack {
                lines.push(summary_line(
                    "Slack allowed users",
                    if state.slack_allow_user_ids.trim().is_empty() {
                        "(all users)"
                    } else {
                        state.slack_allow_user_ids.trim()
                    },
                ));
            }
            lines.push(summary_line(
                "Start Slack",
                if state.configure_slack && state.start_slack {
                    "yes"
                } else {
                    "no"
                },
            ));
            lines.push(summary_line(
                "WhatsApp setup",
                if state.configure_whatsapp { "yes" } else { "no" },
            ));
            if state.configure_whatsapp {
                lines.push(summary_line(
                    "WhatsApp deliver bind",
                    if state.whatsapp_deliver_bind.trim().is_empty() {
                        state.bootstrap.initial_whatsapp_deliver_bind.as_str()
                    } else {
                        state.whatsapp_deliver_bind.trim()
                    },
                ));
                lines.push(summary_line(
                    "WhatsApp allowed senders",
                    if state.whatsapp_allow_user_ids.trim().is_empty() {
                        "(all senders)"
                    } else {
                        state.whatsapp_allow_user_ids.trim()
                    },
                ));
            }
            lines.push(summary_line(
                "Start WhatsApp",
                if state.configure_whatsapp && state.start_whatsapp {
                    "yes"
                } else {
                    "no"
                },
            ));
            lines.push(summary_line(
                "Response depth",
                state.tui_response_depth_mode.as_str(),
            ));
            lines.push(Line::from(""));
            lines.push(Line::from("Press Enter to apply and finish."));
        }
    }

    if let Some(message) = state.status_message.as_deref() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            format!("Warning: {}", message),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )]));
    }

    Text::from(lines)
}

fn footer_text(state: &WizardState) -> Text<'static> {
    let mut lines = vec![Line::from(vec![
        Span::styled("Enter", Style::default().fg(Color::Green)),
        Span::raw(" →   "),
        Span::styled("←", Style::default().fg(Color::Yellow)),
        Span::raw(" Back   "),
        Span::styled("Esc", Style::default().fg(Color::LightRed)),
        Span::raw(" Cancel"),
    ])];

    match state.step {
        WizardStep::Provider | WizardStep::Backend | WizardStep::TuiResponseDepth => {
            lines.push(Line::from("↑↓ to change selection."));
        }
        WizardStep::LaunchDaemon
        | WizardStep::LaunchChat
        | WizardStep::McpGateway
        | WizardStep::LaunchMcpGateway
        | WizardStep::Discord
        | WizardStep::LaunchDiscord
        | WizardStep::Telegram
        | WizardStep::LaunchTelegram
        | WizardStep::Slack
        | WizardStep::LaunchSlack
        | WizardStep::WhatsApp
        | WizardStep::LaunchWhatsApp
        | WizardStep::DiscordHeartbeat
        | WizardStep::TelegramHeartbeat => {
            lines.push(Line::from("Space to toggle. Type channel/chat ids when enabled."));
        }
        WizardStep::ApiKey
        | WizardStep::DiscordToken
        | WizardStep::TelegramToken
        | WizardStep::SlackBotToken
        | WizardStep::SlackAppToken => {
            lines.push(Line::from("Hidden input."));
        }
        WizardStep::DaemonBind
        | WizardStep::DiscordPrefix
        | WizardStep::WhatsAppDeliverBind
        | WizardStep::SlackAllowUserIds
        | WizardStep::WhatsAppAllowUserIds
        | WizardStep::TelegramAllowUserIds
        | WizardStep::BackendSurrealKvPath
        | WizardStep::BackendSurrealWsEndpoint => {
            lines.push(Line::from("Type a value, then Enter to confirm."));
        }
        _ => {}
    }

    Text::from(lines)
}

fn option_line(selected: bool, label: &str, detail: &str) -> Line<'static> {
    let marker = if selected { "▸" } else { " " };
    let marker_style = if selected {
        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Line::from(vec![
        Span::styled(format!("{} ", marker), marker_style),
        Span::styled(
            label.to_string(),
            if selected {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ),
        Span::raw(format!("  ({})", detail)),
    ])
}

fn input_line(label: &str, value: &str) -> Line<'static> {
    let display = if value.trim().is_empty() {
        "(empty)".to_string()
    } else {
        value.to_string()
    };
    Line::from(vec![
        Span::styled(
            format!("{}: ", label),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(display),
    ])
}

fn toggle_line(label: &str, enabled: bool) -> Line<'static> {
    let check = if enabled { "✓" } else { "○" };
    Line::from(vec![
        Span::styled(
            check.to_string(),
            if enabled {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
        Span::raw(format!(" {}", label)),
    ])
}

fn summary_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{}: ", label),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

fn mask_secret(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }
    let count = raw.chars().count().min(36);
    let mut masked = "*".repeat(count);
    if raw.chars().count() > 36 {
        masked.push_str("...");
    }
    masked
}

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}
