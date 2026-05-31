use crate::daemon_api::IngestRequest;
use crate::session::load_history;

/// Sub-commands that the ingester recognizes within a text body.
#[derive(Debug, Clone, PartialEq)]
enum IngestCommand {
    New,
    Help,
    Ask { prompt: String },
}

/// Parse an ingester text body into a command and its arguments.
fn parse_ingest_command(text: &str) -> IngestCommand {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return IngestCommand::Help;
    }

    // Split on first whitespace to get the potential command token
    let first_token = trimmed.split_whitespace().next().unwrap_or("").trim();

    match first_token {
        "/new" => IngestCommand::New,
        "/help" => IngestCommand::Help,
        "/start" => IngestCommand::Help, // Telegram convention
        cmd if cmd.starts_with('/') => {
            // Some other slash command — treat remaining text as ask
            let rest = trimmed
                .strip_prefix(cmd)
                .map(|s| s.trim())
                .unwrap_or("")
                .to_string();
            if rest.is_empty() {
                // Unsupported command, treat as help
                IngestCommand::Help
            } else {
                IngestCommand::Ask { prompt: rest }
            }
        }
        _ => IngestCommand::Ask {
            prompt: trimmed.to_string(),
        },
    }
}

/// Rich result of processing an ingest request.
pub struct IngestOutcome {
    pub session_id: String,
    pub is_new_session: bool,
    pub reply: String,
    pub job_id: Option<String>,
    pub should_enqueue: bool,
    pub prompt: Option<String>,
}

/// Process an ingest request: resolve session, parse command, return outcome.
///
/// This is the core logic that the daemon's `/v1/ingest` handler calls.
/// It is kept pure (no I/O beyond what's passed in) so it can be unit tested.
pub fn process_ingest(
    request: &IngestRequest,
    _session_mapping_key: &str,
    existing_session_id: Option<String>,
) -> IngestOutcome {
    let command = parse_ingest_command(&request.text);

    match command {
        IngestCommand::New => {
            // Reset session: generate new session_id, clear history
            let session_id = uuid::Uuid::new_v4().simple().to_string();
            IngestOutcome {
                session_id,
                is_new_session: true,
                reply: format!(
                    "✓ new session started for {}:{}",
                    request.channel, request.channel_id
                ),
                job_id: None,
                should_enqueue: false,
                prompt: None,
            }
        }

        IngestCommand::Help => {
            let session_id = existing_session_id.unwrap_or_else(|| {
                uuid::Uuid::new_v4().simple().to_string()
            });
            let commands = [
                "/new - Start a new conversation session",
                "/help - Show this help message",
                "/ask <prompt> - Send a prompt to Medousa",
                "",
                "Plain text messages are treated as /ask.",
                "Session history is preserved until /new is sent.",
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
                job_id: None,
                should_enqueue: false,
                prompt: None,
            }
        }

        IngestCommand::Ask { prompt } => {
            let is_new = existing_session_id.is_none();
            let session_id = existing_session_id.unwrap_or_else(|| {
                uuid::Uuid::new_v4().simple().to_string()
            });

            let reply = format!(
                "queued ask for session {} ({}:{})",
                &session_id[..8],
                request.channel,
                request.channel_id
            );

            IngestOutcome {
                session_id,
                is_new_session: is_new,
                reply,
                job_id: None, // filled in by caller after enqueue
                should_enqueue: true,
                prompt: Some(prompt),
            }
        }
    }
}

/// Build an `EnqueueAskRequest` from an ingest outcome + session context.
pub fn build_enqueue_ask_request(
    channel: &str,
    user_id: &str,
    channel_id: &str,
    _session_id: &str,
    prompt: String,
) -> crate::daemon_api::EnqueueAskRequest {
    crate::daemon_api::EnqueueAskRequest {
        prompt,
        policy_profile: Some("interactive".to_string()),
        model_hint: None,
        max_turns: Some(1),
        identity_user_id: Some(format!("{}:user:{}", channel, user_id)),
        identity_persona_id: None,
        identity_channel_id: Some(format!("{}:chat:{}", channel, channel_id)),
    }
}

/// Load session history for context building. Returns formatted history string.
pub fn load_session_context(session_id: &str, max_turns: usize) -> String {
    let turns = load_history(session_id);
    let total = turns.len();

    // Take the last N turns as hot context
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
        // Truncate each turn to 400 chars for context budget
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
    fn test_parse_ingest_command_plain_text_is_ask() {
        let cmd = parse_ingest_command("hello world");
        assert_eq!(cmd, IngestCommand::Ask { prompt: "hello world".to_string() });
    }

    #[test]
    fn test_parse_ingest_command_new() {
        let cmd = parse_ingest_command("/new");
        assert_eq!(cmd, IngestCommand::New);
    }

    #[test]
    fn test_parse_ingest_command_help() {
        let cmd = parse_ingest_command("/help");
        assert_eq!(cmd, IngestCommand::Help);
    }

    #[test]
    fn test_parse_ingest_command_start() {
        let cmd = parse_ingest_command("/start");
        assert_eq!(cmd, IngestCommand::Help);
    }

    #[test]
    fn test_parse_ingest_command_unknown_command_treated_as_help() {
        let cmd = parse_ingest_command("/unsupported");
        assert_eq!(cmd, IngestCommand::Help);
    }

    #[test]
    fn test_parse_ingest_command_empty_treated_as_help() {
        let cmd = parse_ingest_command("");
        assert_eq!(cmd, IngestCommand::Help);
    }

    #[test]
    fn test_parse_ingest_command_ask_after_slash() {
        let cmd = parse_ingest_command("/ask what is the weather");
        assert_eq!(cmd, IngestCommand::Ask { prompt: "what is the weather".to_string() });
    }

    #[test]
    fn test_process_ingest_new_creates_new_session() {
        let request = IngestRequest {
            channel: "telegram".to_string(),
            user_id: "telegram:user:123".to_string(),
            channel_id: "telegram:chat:456".to_string(),
            text: "/new".to_string(),
        };
        let key = "telegram:telegram:chat:456:telegram:user:123";
        let outcome = process_ingest(&request, key, Some("existing-session".to_string()));
        assert!(outcome.is_new_session);
        assert_ne!(outcome.session_id, "existing-session");
        assert!(!outcome.should_enqueue);
        assert!(outcome.reply.contains("new session"));
    }

    #[test]
    fn test_process_ingest_help_returns_help_text() {
        let request = IngestRequest {
            channel: "discord".to_string(),
            user_id: "discord:user:789".to_string(),
            channel_id: "discord:channel:012".to_string(),
            text: "/help".to_string(),
        };
        let key = "discord:discord:channel:012:discord:user:789";
        let outcome = process_ingest(&request, key, Some("my-session".to_string()));
        assert!(!outcome.is_new_session);
        assert!(!outcome.should_enqueue);
        assert!(outcome.reply.contains("Available commands"));
        assert_eq!(outcome.session_id, "my-session");
    }

    #[test]
    fn test_process_ingest_ask_enqueues_with_existing_session() {
        let request = IngestRequest {
            channel: "telegram".to_string(),
            user_id: "telegram:user:111".to_string(),
            channel_id: "telegram:chat:222".to_string(),
            text: "what is rust".to_string(),
        };
        let key = "telegram:telegram:chat:222:telegram:user:111";
        let outcome = process_ingest(&request, key, Some("session-abc".to_string()));
        assert!(!outcome.is_new_session);
        assert!(outcome.should_enqueue);
        assert_eq!(outcome.session_id, "session-abc");
        assert_eq!(outcome.prompt, Some("what is rust".to_string()));
    }

    #[test]
    fn test_process_ingest_ask_without_existing_session_marks_new() {
        let request = IngestRequest {
            channel: "telegram".to_string(),
            user_id: "telegram:user:333".to_string(),
            channel_id: "telegram:chat:444".to_string(),
            text: "hello".to_string(),
        };
        let key = "telegram:telegram:chat:444:telegram:user:333";
        let outcome = process_ingest(&request, key, None);
        assert!(outcome.is_new_session);
        assert!(outcome.should_enqueue);
        assert!(outcome.prompt == Some("hello".to_string()));
    }

    #[test]
    fn test_build_enqueue_ask_request() {
        let req = build_enqueue_ask_request(
            "telegram",
            "user:555",
            "chat:666",
            "session-xyz",
            "test prompt".to_string(),
        );
        assert_eq!(req.prompt, "test prompt");
        assert_eq!(req.identity_user_id, Some("telegram:user:user:555".to_string()));
        assert_eq!(req.identity_channel_id, Some("telegram:chat:chat:666".to_string()));
        assert_eq!(req.max_turns, Some(1));
    }

    #[test]
    fn test_load_session_context_empty() {
        // With no history, should return empty
        let ctx = load_session_context("nonexistent-session", 10);
        assert!(ctx.is_empty());
    }
}
