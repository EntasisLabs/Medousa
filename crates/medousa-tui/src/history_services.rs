use medousa_tui::SessionAppendTurnRequest;
use medousa_tui::session::{ConversationTurn, SessionHistorySummary};

use super::daemon_commands::{
    daemon_append_session_turn, daemon_list_history_sessions, daemon_load_session_history,
};
use super::{TuiState, push_obs_alert};

pub(crate) async fn list_history_sessions_daemon_first(
    state: &mut TuiState,
    limit: usize,
) -> Vec<SessionHistorySummary> {
    match daemon_list_history_sessions(&state.daemon_url, limit).await {
        Ok(response) => response.sessions,
        Err(daemon_err) => {
            push_obs_alert(
                state,
                format!(
                    "◈ history unavailable: daemon_error={}",
                    truncate_error(&daemon_err.to_string(), 140)
                ),
            );
            Vec::new()
        }
    }
}

pub(crate) async fn load_history_daemon_first(
    state: &mut TuiState,
    session_id: &str,
) -> Vec<ConversationTurn> {
    match daemon_load_session_history(&state.daemon_url, session_id).await {
        Ok(response) => response.turns.into_iter().map(|entry| entry.turn).collect(),
        Err(daemon_err) => {
            push_obs_alert(
                state,
                format!(
                    "◈ session unavailable: daemon_error={}",
                    truncate_error(&daemon_err.to_string(), 140)
                ),
            );
            Vec::new()
        }
    }
}

pub(crate) async fn append_turn_daemon_first(
    state: &mut TuiState,
    session_id: &str,
    turn: &ConversationTurn,
) {
    let request = SessionAppendTurnRequest { turn: turn.clone() };
    if let Err(error) = daemon_append_session_turn(&state.daemon_url, session_id, &request).await {
        push_obs_alert(
            state,
            format!(
                "◈ session append daemon error={}",
                truncate_error(&error.to_string(), 140)
            ),
        );
    }
}

fn truncate_error(value: &str, max_chars: usize) -> String {
    let out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        format!("{out}...")
    } else {
        out
    }
}
