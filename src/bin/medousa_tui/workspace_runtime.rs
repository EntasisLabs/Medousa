//! TUI window-manager runtime: chat lane stash + prefix keymap + persistence.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

use medousa::session::ConversationTurn;
use medousa::tui::workspace::{
    FocusDir, SplitDirection, WorkspaceShell, load_workspace_session, save_workspace_session,
    short_session_title,
};

use super::{EventOutcome, TuiState, UiMode};

/// Cached chat view for a session that is not currently focused.
#[derive(Debug, Clone, Default)]
pub(crate) struct ChatLane {
    pub conversation: Vec<ConversationTurn>,
    pub input_buffer: String,
    pub conv_scroll: u16,
    pub conv_max_scroll: u16,
    pub auto_scroll: bool,
    pub session_display_name: Option<String>,
}

pub(crate) fn bootstrap_workspace_from_disk(session_id: &str) -> WorkspaceShell {
    if let Some(mut shell) = load_workspace_session() {
        shell.sanitize();
        let title = short_session_title(session_id);
        shell.rebind_focused_chat_session(session_id, &title);
        shell
    } else {
        WorkspaceShell::bootstrap(session_id, &short_session_title(session_id))
    }
}

pub(crate) fn persist_workspace(state: &TuiState) {
    let _ = save_workspace_session(&state.workspace);
}

fn stash_focused_lane(state: &mut TuiState) {
    let session_id = state.session_id.clone();
    let lane = ChatLane {
        conversation: state.conversation.clone(),
        input_buffer: state.input_buffer.clone(),
        conv_scroll: state.conv_scroll,
        conv_max_scroll: state.conv_max_scroll,
        auto_scroll: state.auto_scroll,
        session_display_name: state.session_display_name.clone(),
    };
    state.chat_lanes.insert(session_id, lane);
}

fn restore_lane_into_focus(
    state: &mut TuiState,
    session_id: &str,
    conversation: Vec<ConversationTurn>,
) {
    let lane = state.chat_lanes.remove(session_id).unwrap_or_default();
    state.session_id = session_id.to_string();
    state.session_display_name = lane
        .session_display_name
        .or_else(|| medousa::session::get_session_display_name(session_id));
    state.conversation = if lane.conversation.is_empty() {
        conversation
    } else {
        lane.conversation
    };
    state.input_buffer = lane.input_buffer;
    state.conv_scroll = lane.conv_scroll;
    state.conv_max_scroll = lane.conv_max_scroll;
    state.auto_scroll = lane.auto_scroll;
    state.active_agent_stream_turn = None;
    state.pending_agent_chunk_delta.clear();
    state.pending_agent_chunk_count = 0;
    super::invalidate_markdown_cache(state);
}

fn guard_idle(state: &mut TuiState) -> bool {
    if state.is_processing {
        super::push_obs(
            state,
            "⚠ finish or stop the active turn before changing panes (Ctrl+G)".to_string(),
        );
        false
    } else {
        true
    }
}

pub(crate) async fn focus_group(state: &mut TuiState, group_id: &str) {
    if !guard_idle(state) {
        return;
    }
    let current_group = state.workspace.layout().active_group_id.clone();
    if current_group == group_id {
        return;
    }
    let Some(target_session) = state
        .workspace
        .group_active_tab(group_id)
        .and_then(|t| t.chat_session_id().map(str::to_string))
    else {
        return;
    };

    stash_focused_lane(state);
    state.workspace.layout_mut().active_group_id = group_id.to_string();

    let history = if state.chat_lanes.contains_key(&target_session) {
        Vec::new()
    } else {
        super::history_services::load_history_daemon_first(state, &target_session).await
    };
    restore_lane_into_focus(state, &target_session, history);
    medousa::session::save_last_session_id(&state.session_id);
    persist_workspace(state);
}

pub(crate) async fn split_active(
    state: &mut TuiState,
    direction: SplitDirection,
) -> EventOutcome {
    if !guard_idle(state) {
        return EventOutcome::Continue;
    }
    let new_session = Uuid::new_v4().simple().to_string();
    stash_focused_lane(state);
    if !state.workspace.split_active(direction, &new_session) {
        let current = state.session_id.clone();
        let hist = state
            .chat_lanes
            .remove(&current)
            .map(|l| l.conversation)
            .unwrap_or_default();
        restore_lane_into_focus(state, &current, hist);
        super::push_obs(state, "⚠ pane cap reached (max 4)".to_string());
        return EventOutcome::Continue;
    }
    restore_lane_into_focus(state, &new_session, Vec::new());
    medousa::session::save_last_session_id(&state.session_id);
    persist_workspace(state);
    super::push_obs(
        state,
        format!("✓ split → {} panes", state.workspace.pane_count()),
    );
    EventOutcome::Continue
}

pub(crate) async fn close_active_pane(state: &mut TuiState) -> EventOutcome {
    if !guard_idle(state) {
        return EventOutcome::Continue;
    }
    if state.workspace.pane_count() <= 1 {
        super::push_obs(state, "⚠ cannot close the last pane".to_string());
        return EventOutcome::Continue;
    }
    let closing_session = state.session_id.clone();
    stash_focused_lane(state);
    if !state.workspace.close_active_pane() {
        let hist = state
            .chat_lanes
            .remove(&closing_session)
            .map(|l| l.conversation)
            .unwrap_or_default();
        restore_lane_into_focus(state, &closing_session, hist);
        return EventOutcome::Continue;
    }
    let target_session = state
        .workspace
        .focused_chat_session_id()
        .unwrap_or(closing_session.as_str())
        .to_string();
    let history = if state.chat_lanes.contains_key(&target_session) {
        Vec::new()
    } else {
        super::history_services::load_history_daemon_first(state, &target_session).await
    };
    restore_lane_into_focus(state, &target_session, history);
    let live_sessions: std::collections::HashSet<String> = state
        .workspace
        .layout()
        .tabs
        .iter()
        .filter_map(|t| t.chat_session_id().map(str::to_string))
        .collect();
    state.chat_lanes.retain(|sid, _| live_sessions.contains(sid));
    medousa::session::save_last_session_id(&state.session_id);
    persist_workspace(state);
    EventOutcome::Continue
}

pub(crate) fn lane_conversation<'a>(
    state: &'a TuiState,
    session_id: &str,
) -> Option<&'a [ConversationTurn]> {
    if session_id == state.session_id {
        Some(state.conversation.as_slice())
    } else {
        state
            .chat_lanes
            .get(session_id)
            .map(|l| l.conversation.as_slice())
    }
}

pub(crate) async fn handle_prefix_key(key: KeyEvent, state: &mut TuiState) -> EventOutcome {
    state.prefix_active = false;
    match key.code {
        KeyCode::Char('%') => split_active(state, SplitDirection::Right).await,
        KeyCode::Char('"') => split_active(state, SplitDirection::Down).await,
        KeyCode::Char('h') => {
            let id = state.workspace.layout().active_group_id.clone();
            if let Some(next) = medousa::tui::workspace::neighbor_in_direction(
                &state.workspace.layout().split_root,
                &id,
                FocusDir::Left,
            ) {
                focus_group(state, &next).await;
            }
            EventOutcome::Continue
        }
        KeyCode::Char('l') => {
            let id = state.workspace.layout().active_group_id.clone();
            if let Some(next) = medousa::tui::workspace::neighbor_in_direction(
                &state.workspace.layout().split_root,
                &id,
                FocusDir::Right,
            ) {
                focus_group(state, &next).await;
            }
            EventOutcome::Continue
        }
        KeyCode::Char('k') => {
            let id = state.workspace.layout().active_group_id.clone();
            if let Some(next) = medousa::tui::workspace::neighbor_in_direction(
                &state.workspace.layout().split_root,
                &id,
                FocusDir::Up,
            ) {
                focus_group(state, &next).await;
            }
            EventOutcome::Continue
        }
        KeyCode::Char('j') => {
            let id = state.workspace.layout().active_group_id.clone();
            if let Some(next) = medousa::tui::workspace::neighbor_in_direction(
                &state.workspace.layout().split_root,
                &id,
                FocusDir::Down,
            ) {
                focus_group(state, &next).await;
            }
            EventOutcome::Continue
        }
        KeyCode::Char('z') => {
            state.workspace.toggle_zoom();
            persist_workspace(state);
            EventOutcome::Continue
        }
        KeyCode::Char('x') => close_active_pane(state).await,
        KeyCode::Char('c') => {
            if !guard_idle(state) {
                return EventOutcome::Continue;
            }
            let session = Uuid::new_v4().simple().to_string();
            stash_focused_lane(state);
            let title = short_session_title(&session);
            if state.workspace.open_chat_tab_in_active(&session, &title) {
                restore_lane_into_focus(state, &session, Vec::new());
                medousa::session::save_last_session_id(&state.session_id);
                persist_workspace(state);
            }
            EventOutcome::Continue
        }
        KeyCode::Char('n') => {
            if !guard_idle(state) {
                return EventOutcome::Continue;
            }
            if state.workspace.cycle_tab(true)
                && let Some(sid) = state
                    .workspace
                    .focused_chat_session_id()
                    .map(str::to_string)
            {
                stash_focused_lane(state);
                let history = if state.chat_lanes.contains_key(&sid) {
                    Vec::new()
                } else {
                    super::history_services::load_history_daemon_first(state, &sid).await
                };
                restore_lane_into_focus(state, &sid, history);
                persist_workspace(state);
            }
            EventOutcome::Continue
        }
        KeyCode::Char('p') => {
            if !guard_idle(state) {
                return EventOutcome::Continue;
            }
            if state.workspace.cycle_tab(false)
                && let Some(sid) = state
                    .workspace
                    .focused_chat_session_id()
                    .map(str::to_string)
            {
                stash_focused_lane(state);
                let history = if state.chat_lanes.contains_key(&sid) {
                    Vec::new()
                } else {
                    super::history_services::load_history_daemon_first(state, &sid).await
                };
                restore_lane_into_focus(state, &sid, history);
                persist_workspace(state);
            }
            EventOutcome::Continue
        }
        KeyCode::Char('1') => switch_desktop(state, 0).await,
        KeyCode::Char('2') => switch_desktop(state, 1).await,
        KeyCode::Char('3') => switch_desktop(state, 2).await,
        KeyCode::Char('4') => switch_desktop(state, 3).await,
        KeyCode::Char('?') => {
            super::push_obs(
                state,
                "panes: Ctrl+; then % \" h j k l z x c n p 1-4".to_string(),
            );
            EventOutcome::Continue
        }
        KeyCode::Esc => EventOutcome::Continue,
        _ => {
            super::push_obs(
                state,
                "prefix: % \" h/j/k/l z x c n/p 1-4 ?".to_string(),
            );
            EventOutcome::Continue
        }
    }
}

async fn switch_desktop(state: &mut TuiState, index: usize) -> EventOutcome {
    if !guard_idle(state) {
        return EventOutcome::Continue;
    }
    if index >= state.workspace.desktops.len() {
        return EventOutcome::Continue;
    }
    let current = state.workspace.active_desktop_id.clone();
    let next_id = state.workspace.desktops[index].id.clone();
    if next_id == current {
        return EventOutcome::Continue;
    }
    stash_focused_lane(state);
    state.workspace.active_desktop_id = next_id;
    let target_session = state
        .workspace
        .focused_chat_session_id()
        .map(str::to_string)
        .unwrap_or_else(|| state.session_id.clone());
    let history = if state.chat_lanes.contains_key(&target_session) {
        Vec::new()
    } else {
        super::history_services::load_history_daemon_first(state, &target_session).await
    };
    restore_lane_into_focus(state, &target_session, history);
    medousa::session::save_last_session_id(&state.session_id);
    persist_workspace(state);
    EventOutcome::Continue
}

pub(crate) fn handle_prefix_trigger(key: KeyEvent, state: &mut TuiState) -> bool {
    if state.mode != UiMode::Chat {
        return false;
    }
    if key.code == KeyCode::Char(';') && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.prefix_active = true;
        return true;
    }
    false
}

pub(crate) fn empty_chat_lanes() -> HashMap<String, ChatLane> {
    HashMap::new()
}

/// Sync focused session id back onto the active chat tab (e.g. after `/new`).
pub(crate) fn rebind_focused_session(state: &mut TuiState) {
    let title = state
        .session_display_name
        .clone()
        .unwrap_or_else(|| short_session_title(&state.session_id));
    state
        .workspace
        .rebind_focused_chat_session(&state.session_id, &title);
    persist_workspace(state);
}
