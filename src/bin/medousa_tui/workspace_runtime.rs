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

/// Soft cap aligned with Home chatStreamPool / max panes.
pub(crate) const MAX_LIVE_STREAMS: usize = 4;

/// Cached chat view for a session that is not currently focused.
#[derive(Debug, Default)]
pub(crate) struct ChatLane {
    pub conversation: Vec<ConversationTurn>,
    pub input_buffer: String,
    pub conv_scroll: u16,
    pub conv_max_scroll: u16,
    pub auto_scroll: bool,
    pub session_display_name: Option<String>,
    pub is_processing: bool,
    pub open_stream_turn_id: Option<u64>,
    pub active_agent_stream_turn: Option<usize>,
    pub pending_agent_chunk_delta: String,
    pub pending_agent_chunk_count: u64,
    pub turn_parts: medousa::turn_parts::TurnPartsAccumulator,
    pub in_thinking_tag: bool,
    pub stream_tag_tail: String,
    pub received_native_reasoning: bool,
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

pub(crate) fn live_stream_count(state: &TuiState) -> usize {
    let focused = usize::from(state.active_request_task.is_some() || state.is_processing);
    let background = state.session_tasks.len();
    // Avoid double-counting if focused task was also left in session_tasks.
    if state.active_request_task.is_some()
        && state.session_tasks.contains_key(&state.session_id)
    {
        background
    } else {
        focused + background
    }
}

pub(crate) fn register_stream_turn(state: &mut TuiState, turn_id: u64) {
    state
        .turn_sessions
        .insert(turn_id, state.session_id.clone());
}

pub(crate) fn clear_stream_turn(state: &mut TuiState, turn_id: u64) {
    if let Some(session) = state.turn_sessions.remove(&turn_id) {
        state.session_tasks.remove(&session);
        if session == state.session_id {
            state.active_request_task = None;
        }
        if let Some(lane) = state.chat_lanes.get_mut(&session) {
            lane.is_processing = false;
            lane.open_stream_turn_id = None;
            lane.active_agent_stream_turn = None;
            lane.pending_agent_chunk_delta.clear();
            lane.pending_agent_chunk_count = 0;
        }
    }
}

fn stash_focused_lane(state: &mut TuiState) {
    let session_id = state.session_id.clone();
    if let Some(task) = state.active_request_task.take() {
        state.session_tasks.insert(session_id.clone(), task);
    }
    let lane = ChatLane {
        conversation: state.conversation.clone(),
        input_buffer: state.input_buffer.clone(),
        conv_scroll: state.conv_scroll,
        conv_max_scroll: state.conv_max_scroll,
        auto_scroll: state.auto_scroll,
        session_display_name: state.session_display_name.clone(),
        is_processing: state.is_processing,
        open_stream_turn_id: state.open_stream_turn_id,
        active_agent_stream_turn: state.active_agent_stream_turn,
        pending_agent_chunk_delta: std::mem::take(&mut state.pending_agent_chunk_delta),
        pending_agent_chunk_count: std::mem::take(&mut state.pending_agent_chunk_count),
        turn_parts: std::mem::take(&mut state.turn_parts),
        in_thinking_tag: state.in_thinking_tag,
        stream_tag_tail: std::mem::take(&mut state.stream_tag_tail),
        received_native_reasoning: state.received_native_reasoning,
    };
    state.chat_lanes.insert(session_id, lane);
    state.is_processing = false;
    state.open_stream_turn_id = None;
    state.active_agent_stream_turn = None;
    state.in_thinking_tag = false;
    state.received_native_reasoning = false;
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
    state.is_processing = lane.is_processing;
    state.open_stream_turn_id = lane.open_stream_turn_id;
    state.active_agent_stream_turn = lane.active_agent_stream_turn;
    state.pending_agent_chunk_delta = lane.pending_agent_chunk_delta;
    state.pending_agent_chunk_count = lane.pending_agent_chunk_count;
    state.turn_parts = lane.turn_parts;
    state.in_thinking_tag = lane.in_thinking_tag;
    state.stream_tag_tail = lane.stream_tag_tail;
    state.received_native_reasoning = lane.received_native_reasoning;
    state.active_request_task = state.session_tasks.remove(session_id);
    if state.active_request_task.is_some() {
        state.is_processing = true;
    }
    super::invalidate_markdown_cache(state);
}

/// Temporarily focus `session_id` for stream event application. Returns prior session id.
pub(crate) fn swap_to_session(state: &mut TuiState, session_id: &str) -> String {
    let previous = state.session_id.clone();
    if previous == session_id {
        return previous;
    }
    stash_focused_lane(state);
    let history = Vec::new();
    restore_lane_into_focus(state, session_id, history);
    previous
}

pub(crate) async fn focus_group(state: &mut TuiState, group_id: &str) {
    let current_group = state.workspace.layout().active_group_id.clone();
    if current_group == group_id {
        return;
    }
    let target_tab_kind = state
        .workspace
        .group_active_tab(group_id)
        .map(|t| t.kind());
    let target_session = state
        .workspace
        .group_active_tab(group_id)
        .and_then(|t| t.chat_session_id().map(str::to_string));

    stash_focused_lane(state);
    state.workspace.layout_mut().active_group_id = group_id.to_string();

    if let Some(target_session) = target_session {
        let history = if state.chat_lanes.contains_key(&target_session)
            || state.session_tasks.contains_key(&target_session)
        {
            Vec::new()
        } else {
            super::history_services::load_history_daemon_first(state, &target_session).await
        };
        restore_lane_into_focus(state, &target_session, history);
        medousa::session::save_last_session_id(&state.session_id);
    } else if matches!(
        target_tab_kind,
        Some(
            medousa::tui::workspace::ShellTabKind::Notes
                | medousa::tui::workspace::ShellTabKind::Code
                | medousa::tui::workspace::ShellTabKind::Review
                | medousa::tui::workspace::ShellTabKind::Terminal
        )
    ) {
        // Non-chat pane: chat lane stays stashed; mode follows tab kind.
    }
    sync_mode_from_active_tab(state);
    persist_workspace(state);
}

pub(crate) async fn split_active(
    state: &mut TuiState,
    direction: SplitDirection,
) -> EventOutcome {
    // Home rule: splitting a terminal pane creates a new shell session, not a chat.
    if state.workspace.active_tab().map(|t| t.kind())
        == Some(medousa::tui::workspace::ShellTabKind::Terminal)
    {
        return super::terminal_runtime::split_with_new_terminal(state, direction).await;
    }

    let new_session = Uuid::new_v4().simple().to_string();
    stash_focused_lane(state);
    if !state.workspace.split_active(direction, &new_session) {
        let current = state.session_id.clone();
        restore_lane_into_focus(state, &current, Vec::new());
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
    if state.workspace.pane_count() <= 1 {
        super::push_obs(state, "⚠ cannot close the last pane".to_string());
        return EventOutcome::Continue;
    }
    let closing_session = state.session_id.clone();
    if state.is_processing || state.active_request_task.is_some() {
        super::stop_active_generation(state);
    }
    stash_focused_lane(state);
    if !state.workspace.close_active_pane() {
        restore_lane_into_focus(state, &closing_session, Vec::new());
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
    for (sid, task) in state.session_tasks.drain().collect::<Vec<_>>() {
        if live_sessions.contains(&sid) {
            state.session_tasks.insert(sid, task);
        } else {
            task.abort();
        }
    }
    state
        .turn_sessions
        .retain(|_, sid| live_sessions.contains(sid));
    super::terminal_runtime::detach_orphaned_terminals(state);
    sync_mode_from_active_tab(state);
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

pub(crate) fn lane_is_processing(state: &TuiState, session_id: &str) -> bool {
    if session_id == state.session_id {
        state.is_processing
    } else {
        state
            .chat_lanes
            .get(session_id)
            .map(|l| l.is_processing)
            .unwrap_or_else(|| state.session_tasks.contains_key(session_id))
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
            let session = Uuid::new_v4().simple().to_string();
            stash_focused_lane(state);
            let title = short_session_title(&session);
            if state.workspace.open_chat_tab_in_active(&session, &title) {
                restore_lane_into_focus(state, &session, Vec::new());
                medousa::session::save_last_session_id(&state.session_id);
                persist_workspace(state);
            } else {
                restore_lane_into_focus(state, &state.session_id.clone(), Vec::new());
            }
            EventOutcome::Continue
        }
        KeyCode::Char('o') => {
            super::notes_runtime::open_notes_picker(state).await;
            EventOutcome::Continue
        }
        KeyCode::Char('f') => {
            super::forge_runtime::open_forge_picker(
                state,
                super::forge_runtime::ForgePickerTarget::Code,
            )
            .await;
            EventOutcome::Continue
        }
        KeyCode::Char('r') => {
            super::forge_runtime::open_forge_picker(
                state,
                super::forge_runtime::ForgePickerTarget::Review,
            )
            .await;
            EventOutcome::Continue
        }
        KeyCode::Char('t') => {
            let _ = super::terminal_runtime::open_new_terminal(state).await;
            EventOutcome::Continue
        }
        KeyCode::Char('T') => {
            super::terminal_runtime::open_terminal_picker(state).await;
            EventOutcome::Continue
        }
        KeyCode::Char('n') => {
            if state.workspace.cycle_tab(true) {
                if let Some(sid) = state
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
                }
                sync_mode_from_active_tab(state);
                persist_workspace(state);
            }
            EventOutcome::Continue
        }
        KeyCode::Char('p') => {
            if state.workspace.cycle_tab(false) {
                if let Some(sid) = state
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
                }
                sync_mode_from_active_tab(state);
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
                "panes: Ctrl+; then % \" h j k l z x c o f r t T n p 1-4".to_string(),
            );
            EventOutcome::Continue
        }
        KeyCode::Esc => EventOutcome::Continue,
        _ => {
            super::push_obs(
                state,
                "prefix: % \" h/j/k/l z x c o f r t/T n/p 1-4 ?".to_string(),
            );
            EventOutcome::Continue
        }
    }
}

async fn switch_desktop(state: &mut TuiState, index: usize) -> EventOutcome {
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
    if !matches!(
        state.mode,
        UiMode::Chat | UiMode::Notes | UiMode::Code | UiMode::Review | UiMode::Terminal
    ) {
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

pub(crate) fn sync_mode_from_active_tab(state: &mut TuiState) {
    match state.workspace.active_tab().map(|t| t.kind()) {
        Some(medousa::tui::workspace::ShellTabKind::Notes) => state.mode = UiMode::Notes,
        Some(medousa::tui::workspace::ShellTabKind::Code) => state.mode = UiMode::Code,
        Some(medousa::tui::workspace::ShellTabKind::Review) => state.mode = UiMode::Review,
        Some(medousa::tui::workspace::ShellTabKind::Terminal) => state.mode = UiMode::Terminal,
        Some(medousa::tui::workspace::ShellTabKind::Chat) => {
            if matches!(
                state.mode,
                UiMode::Notes | UiMode::Code | UiMode::Review | UiMode::Terminal
            ) {
                state.mode = UiMode::Chat;
            }
        }
        _ => {}
    }
}
