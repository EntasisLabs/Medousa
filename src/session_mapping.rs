use crate::daemon_api::{
    IngestAttachment, IngestRequest, InteractiveTurnRequest, TurnSurfaceContext,
};
use crate::session::load_history;
use crate::stage_routing::StageRoutingMatrix;

/// What the ingester should do after parsing a request.
#[derive(Debug, Clone, PartialEq)]
pub enum IngestAction {
    Reply,
    EnqueueAsk { prompt: String },
    CancelActiveJob,
    Regenerate,
    ListHistory,
    ResumeSession { target_session_id: String },
    ConfigureModel { args: Vec<String> },
    ConfigureDepth { mode: Option<String> },
    SetDisplayName { label: Option<String> },
    QueryHealth,
    QueryHeartbeat,
}

/// Rich result of processing an ingest request.
#[derive(Debug, Clone, PartialEq)]
pub struct IngestOutcome {
    pub session_id: String,
    pub is_new_session: bool,
    pub reply: String,
    pub action: IngestAction,
}

/// Per-session runtime preferences managed by the ingester.
#[derive(Debug, Clone, PartialEq)]
pub struct IngestSessionRuntimeConfig {
    pub draft_provider: String,
    pub draft_model: String,
    pub response_depth_mode: String,
}

impl IngestSessionRuntimeConfig {
    /// Load channel ingest defaults from wizard/TUI saved settings (`tui_defaults.json`).
    pub fn from_saved_defaults() -> Self {
        let defaults = crate::session::load_tui_defaults();
        let product = crate::load_product_config();
        let response_depth_mode = defaults
            .response_depth_mode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(product.tui.response_depth_mode.as_str())
            .to_string();

        Self {
            draft_provider: crate::resolve_llm_provider(defaults.provider.as_deref()),
            draft_model: crate::resolve_llm_model(defaults.model.as_deref()),
            response_depth_mode,
        }
    }
}

impl Default for IngestSessionRuntimeConfig {
    fn default() -> Self {
        Self::from_saved_defaults()
    }
}

/// Tracks the active streaming job for a channel+user mapping.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveIngestJob {
    pub job_id: String,
    pub stream_id: String,
    pub channel: String,
    pub user_id: String,
    pub channel_id: String,
    pub session_id: String,
}

/// Sub-commands that the ingester recognizes within a text body.
#[derive(Debug, Clone, PartialEq)]
enum IngestCommand {
    New,
    Help,
    Stop,
    Regen,
    History { target: Option<String> },
    Model { args: Vec<String> },
    Depth { mode: Option<String> },
    Name { label: Option<String> },
    Health,
    Heartbeat,
    Ask { prompt: String },
}

fn split_slash_command(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let token = trimmed.split_whitespace().next().unwrap_or("");
    let normalized = token.split('@').next().unwrap_or(token).to_ascii_lowercase();
    let args = trimmed
        .strip_prefix(token)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    Some((normalized, args))
}

fn parse_model_args(args: &str) -> Vec<String> {
    args.split_whitespace()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Parse an ingester text body into a command and its arguments.
fn parse_ingest_command(text: &str) -> IngestCommand {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return IngestCommand::Help;
    }

    let Some((command, args)) = split_slash_command(trimmed) else {
        return IngestCommand::Ask {
            prompt: trimmed.to_string(),
        };
    };

    match command.as_str() {
        "/new" => IngestCommand::New,
        "/help" | "/start" => IngestCommand::Help,
        "/stop" => IngestCommand::Stop,
        "/regen" => IngestCommand::Regen,
        "/history" => IngestCommand::History {
            target: if args.is_empty() {
                None
            } else {
                Some(args.to_string())
            },
        },
        "/model" => IngestCommand::Model {
            args: parse_model_args(&args),
        },
        "/depth" => IngestCommand::Depth {
            mode: if args.is_empty() {
                None
            } else {
                Some(args.to_string())
            },
        },
        "/name" => IngestCommand::Name {
            label: if args.is_empty() {
                None
            } else {
                Some(args.to_string())
            },
        },
        "/health" => IngestCommand::Health,
        "/heartbeat" => IngestCommand::Heartbeat,
        "/ask" => {
            if args.is_empty() {
                IngestCommand::Help
            } else {
                IngestCommand::Ask {
                    prompt: args.to_string(),
                }
            }
        }
        _ if args.is_empty() => IngestCommand::Help,
        _ => IngestCommand::Ask { prompt: args },
    }
}

/// Merge attachment payloads into the ask prompt.
pub fn merge_attachments_into_prompt(prompt: &str, attachments: &[IngestAttachment]) -> String {
    if attachments.is_empty() {
        return prompt.to_string();
    }

    let mut merged = prompt.to_string();
    for attachment in attachments {
        if attachment.content.trim().is_empty() {
            continue;
        }
        merged.push_str("\n\n[attachment:");
        merged.push_str(attachment.kind.trim());
        merged.push_str("] ");
        merged.push_str(attachment.content.trim());
    }
    merged
}

/// Process an ingest request: resolve session, parse command, return outcome.
pub fn process_ingest(
    request: &IngestRequest,
    _session_mapping_key: &str,
    existing_session_id: Option<String>,
) -> IngestOutcome {
    let command = parse_ingest_command(&request.text);

    match command {
        IngestCommand::New => {
            let session_id = uuid::Uuid::new_v4().simple().to_string();
            IngestOutcome {
                session_id,
                is_new_session: true,
                reply: format!(
                    "✓ new session started for {}:{}",
                    request.channel, request.channel_id
                ),
                action: IngestAction::Reply,
            }
        }

        IngestCommand::Help => {
            let session_id = existing_session_id
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
            let commands = [
                "/new - Start a new conversation session",
                "/help - Show this help message",
                "/history - List recent sessions for this channel/user",
                "/history <id> - Resume a prior session",
                "/model - Show current model routing",
                "/model <model> - Set model (or provider:model)",
                "/depth - Show response depth mode",
                "/depth <mode> - Set depth (concise|standard|deep)",
                "/name - Show this session's display name",
                "/name <label> - Set a global display name for this session",
                "/stop - Cancel the active ask job",
                "/regen - Regenerate the last response",
                "/health - Daemon health check",
                "/heartbeat - Daemon heartbeat status",
                "",
                "Plain text messages are treated as asks.",
            ]
            .join("\n");

            let reply = format!(
                "Medousa ingester online.\n\nAvailable commands:\n{commands}\n\nChannel: {} | Channel ID: {}",
                request.channel, request.channel_id
            );

            IngestOutcome {
                session_id,
                is_new_session: false,
                reply,
                action: IngestAction::Reply,
            }
        }

        IngestCommand::Stop => IngestOutcome {
            session_id: existing_session_id.unwrap_or_else(|| {
                uuid::Uuid::new_v4().simple().to_string()
            }),
            is_new_session: false,
            reply: "stopping active job…".to_string(),
            action: IngestAction::CancelActiveJob,
        },

        IngestCommand::Regen => IngestOutcome {
            session_id: existing_session_id.unwrap_or_else(|| {
                uuid::Uuid::new_v4().simple().to_string()
            }),
            is_new_session: false,
            reply: "regenerating last response…".to_string(),
            action: IngestAction::Regenerate,
        },

        IngestCommand::History { target } => {
            let session_id = existing_session_id
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
            if let Some(target_raw) = target {
                let target_session_id = crate::session::resolve_history_resume_target(&target_raw)
                    .unwrap_or(target_raw);
                let label = crate::session::format_session_history_label(
                    &target_session_id,
                    crate::session::get_session_display_name(&target_session_id).as_deref(),
                );
                IngestOutcome {
                    session_id: target_session_id.clone(),
                    is_new_session: false,
                    reply: format!("resumed session {label}"),
                    action: IngestAction::ResumeSession {
                        target_session_id,
                    },
                }
            } else {
                IngestOutcome {
                    session_id,
                    is_new_session: false,
                    reply: "loading session history…".to_string(),
                    action: IngestAction::ListHistory,
                }
            }
        }

        IngestCommand::Model { args } => IngestOutcome {
            session_id: existing_session_id.unwrap_or_else(|| {
                uuid::Uuid::new_v4().simple().to_string()
            }),
            is_new_session: false,
            reply: "updating model routing…".to_string(),
            action: IngestAction::ConfigureModel { args },
        },

        IngestCommand::Depth { mode } => IngestOutcome {
            session_id: existing_session_id.unwrap_or_else(|| {
                uuid::Uuid::new_v4().simple().to_string()
            }),
            is_new_session: false,
            reply: "updating response depth…".to_string(),
            action: IngestAction::ConfigureDepth { mode },
        },

        IngestCommand::Name { label } => {
            let session_id = existing_session_id
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
            let reply = match label {
                None => {
                    let current = crate::session::get_session_display_name(&session_id);
                    match current {
                        Some(name) => format!(
                            "session name: {} ({})",
                            name,
                            &session_id[..session_id.len().min(8)]
                        ),
                        None => format!(
                            "no display name set for session {}",
                            &session_id[..session_id.len().min(8)]
                        ),
                    }
                }
                Some(ref raw) => {
                    match crate::session::set_session_display_name(&session_id, raw) {
                        Ok(()) => {
                            let name = crate::session::get_session_display_name(&session_id)
                                .unwrap_or_else(|| raw.clone());
                            format!("✓ session name set to \"{name}\" (global)")
                        }
                        Err(err) => format!("⚠ could not set session name: {err}"),
                    }
                }
            };
            IngestOutcome {
                session_id,
                is_new_session: false,
                reply,
                action: IngestAction::SetDisplayName { label },
            }
        }

        IngestCommand::Health => IngestOutcome {
            session_id: existing_session_id.unwrap_or_else(|| {
                uuid::Uuid::new_v4().simple().to_string()
            }),
            is_new_session: false,
            reply: "checking daemon health…".to_string(),
            action: IngestAction::QueryHealth,
        },

        IngestCommand::Heartbeat => IngestOutcome {
            session_id: existing_session_id.unwrap_or_else(|| {
                uuid::Uuid::new_v4().simple().to_string()
            }),
            is_new_session: false,
            reply: "checking daemon heartbeat…".to_string(),
            action: IngestAction::QueryHeartbeat,
        },

        IngestCommand::Ask { prompt } => {
            let is_new = existing_session_id.is_none();
            let session_id = existing_session_id
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
            let merged_prompt = merge_attachments_into_prompt(&prompt, &request.attachments);
            let session_prefix = session_id[..8.min(session_id.len())].to_string();

            IngestOutcome {
                session_id,
                is_new_session: is_new,
                reply: format!(
                    "queued ask for session {} ({}:{})",
                    session_prefix,
                    request.channel,
                    request.channel_id
                ),
                action: IngestAction::EnqueueAsk {
                    prompt: merged_prompt,
                },
            }
        }
    }
}

/// Extract the last user prompt from a session transcript for /regen.
pub fn last_user_prompt_for_regen(session_id: &str) -> Option<String> {
    load_history(session_id)
        .into_iter()
        .rev()
        .find(|turn| turn.role == "user")
        .map(|turn| turn.content)
        .filter(|content| !content.trim().is_empty())
}

/// Build an interactive turn request for centralized ingest (agent runtime path).
pub fn build_interactive_turn_request_for_ingest(
    session_id: &str,
    prompt: String,
    provider: &str,
    model: &str,
    response_depth_mode: &str,
    ingest: Option<&IngestRequest>,
) -> InteractiveTurnRequest {
    let defaults = crate::session::load_tui_defaults();
    let surface = ingest.map(|request| {
        TurnSurfaceContext::from_ingest(
            &request.channel,
            &request.channel_id,
            &request.user_id,
        )
    }).or_else(|| {
        Some(TurnSurfaceContext {
            channel_surface: Some("api".to_string()),
            channel_id: None,
            user_id: None,
        })
    });
    InteractiveTurnRequest {
        session_id: session_id.to_string(),
        prompt,
        persist_user_turn: true,
        response_depth_mode: response_depth_mode.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        stage_routing: StageRoutingMatrix::default_for(provider, model),
        surface,
        max_tool_rounds: Some(defaults.max_tool_rounds.unwrap_or(10)),
        retry_runtime_max_rounds: Some(
            defaults
                .retry_runtime_max_rounds
                .unwrap_or(crate::agent_runtime::turn_orchestrator::DEFAULT_RETRY_RUNTIME_MAX_ROUNDS),
        ),
        manuscript_id: None,
        scheduled_tool_allowlist: None,
    }
}

/// Load session history for context building. Returns formatted history string.
pub fn load_session_context(session_id: &str, max_turns: usize) -> String {
    let turns = load_history(session_id);
    let total = turns.len();

    let hot_start = total.saturating_sub(max_turns);
    let context_turns = &turns[hot_start..];

    if context_turns.is_empty() {
        return String::new();
    }

    let mut lines = Vec::with_capacity(context_turns.len());
    for turn in context_turns {
        let role = match turn.role.as_str() {
            "user" => "user",
            "assistant" | "agent" => "assistant",
            _ => continue,
        };
        let content: String = turn.content.chars().take(400).collect();
        if content.trim().is_empty() {
            continue;
        }
        lines.push(format!("{}: {}", role, content));
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!(
            "[MEDOUSA_SESSION_HISTORY session_id={} turns={}]\n{}",
            session_id,
            total,
            lines.join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stop_regen_history_model_depth() {
        assert_eq!(parse_ingest_command("/stop"), IngestCommand::Stop);
        assert_eq!(parse_ingest_command("/regen"), IngestCommand::Regen);
        assert_eq!(
            parse_ingest_command("/history"),
            IngestCommand::History { target: None }
        );
        assert_eq!(
            parse_ingest_command("/history abc123"),
            IngestCommand::History {
                target: Some("abc123".to_string())
            }
        );
        assert_eq!(
            parse_ingest_command("/model gpt-4o-mini"),
            IngestCommand::Model {
                args: vec!["gpt-4o-mini".to_string()]
            }
        );
        assert_eq!(
            parse_ingest_command("/depth concise"),
            IngestCommand::Depth {
                mode: Some("concise".to_string())
            }
        );
        assert_eq!(parse_ingest_command("/health"), IngestCommand::Health);
        assert_eq!(parse_ingest_command("/heartbeat"), IngestCommand::Heartbeat);
        assert_eq!(
            parse_ingest_command("/name"),
            IngestCommand::Name { label: None }
        );
        assert_eq!(
            parse_ingest_command("/name research sprint"),
            IngestCommand::Name {
                label: Some("research sprint".to_string())
            }
        );
    }

    #[test]
    fn test_process_ingest_stop_action() {
        let request = IngestRequest {
            channel: "telegram".to_string(),
            user_id: "telegram:user:1".to_string(),
            channel_id: "telegram:chat:2".to_string(),
            text: "/stop".to_string(),
            attachments: Vec::new(),
        };
        let outcome = process_ingest(&request, "key", Some("session-1".to_string()));
        assert_eq!(outcome.action, IngestAction::CancelActiveJob);
    }

    #[test]
    fn test_merge_attachments_into_prompt() {
        let merged = merge_attachments_into_prompt(
            "summarize this",
            &[IngestAttachment {
                kind: "text".to_string(),
                content: "hello attachment".to_string(),
            }],
        );
        assert!(merged.contains("summarize this"));
        assert!(merged.contains("hello attachment"));
    }
}
