//! Tauri host adapter for the renderer-agnostic chat application core.
//!
//! The webview never owns reconciliation. Commands mutate [`ChatCore`] and
//! return canonical [`ChatUpdate`] snapshots.

use std::sync::Mutex;

use medousa_app_core::{ChatCommand, ChatCore, ChatCoreError, ChatUpdate};
use tauri::State;

#[derive(Default)]
pub struct AppChatState {
    core: Mutex<ChatCore>,
}

fn map_error(error: ChatCoreError) -> String {
    error.to_string()
}

/// Dispatch one application chat command and return the resulting update.
#[tauri::command]
pub fn app_chat_dispatch(
    state: State<'_, AppChatState>,
    command: ChatCommand,
) -> Result<ChatUpdate, String> {
    let mut core = state.core.lock().map_err(|error| error.to_string())?;
    core.dispatch(command).map_err(map_error)
}

/// Convenience: open (or focus) a session and return its snapshot.
#[tauri::command]
pub fn app_chat_open_session(
    state: State<'_, AppChatState>,
    session_id: String,
    authority_id: String,
) -> Result<ChatUpdate, String> {
    app_chat_dispatch(
        state,
        ChatCommand::OpenSession {
            session_id,
            authority_id,
        },
    )
}

/// Convenience: read the current snapshot without mutating state.
#[tauri::command]
pub fn app_chat_snapshot(
    state: State<'_, AppChatState>,
    session_id: String,
) -> Result<ChatUpdate, String> {
    app_chat_dispatch(state, ChatCommand::Snapshot { session_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_app_core::contract::BeginTurnRequest;

    #[test]
    fn dispatch_begin_turn_round_trips_through_host_state() {
        let state = AppChatState::default();
        let mut core = state.core.lock().unwrap();
        let update = core
            .dispatch(ChatCommand::OpenSession {
                session_id: "ses_test".into(),
                authority_id: "auth_test".into(),
            })
            .unwrap();
        assert!(matches!(update, ChatUpdate::Snapshot { .. }));
        let update = core
            .dispatch(ChatCommand::BeginTurn {
                session_id: "ses_test".into(),
                request: BeginTurnRequest {
                    turn_id: "turn".into(),
                    user_item_id: "u".into(),
                    assistant_item_id: "a".into(),
                    user_text: "hi".into(),
                    created_at: None,
                    lane: None,
                    ask_job_id: None,
                    work_id: None,
                    status_line: None,
                    speaker_profile_id: None,
                    media_attachments: None,
                    mode: None,
                    workspace_card_id: None,
                    reusable_stream: false,
                },
                expected_revision: None,
            })
            .unwrap();
        let ChatUpdate::Snapshot { snapshot } = update else {
            panic!("expected snapshot");
        };
        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.items[0].content, "hi");
    }
}
