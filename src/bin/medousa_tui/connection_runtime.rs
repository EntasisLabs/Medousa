//! Connection picker — switch workshop daemon URL + scoped workspace layout.
//!
//! Settings UI label is **Connection** (aligned with Home). Does not invent a
//! second workshop registry; reads Home's `workshops.json` when present.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use medousa::tui::workshop_connection::{
    self, ConnectionChoice, ConnectionSource, normalize_daemon_url, remember_daemon,
    workshop_scope_key,
};
use medousa::tui::workspace::{
    load_workspace_session_for, save_workspace_session_for, short_session_title,
};

use super::{EventOutcome, TuiState, UiMode};

pub(crate) fn resolve_scope_for_url(url: &str) -> (String, String) {
    let registry = workshop_connection::load_workshop_registry();
    let scope = workshop_scope_key(url, registry.as_ref());
    let label = workshop_connection::label_for_url(url, registry.as_ref());
    (scope, label)
}

pub(crate) fn open_connection_picker(state: &mut TuiState) {
    state.mode = UiMode::ConnectionPicker;
    state.connection_picker_selected = 0;
    state.connection_picker_query.clear();
    state.connection_picker_custom.clear();
    state.connection_picker_editing_custom = false;
    refresh_connection_picker(state);
}

pub(crate) fn refresh_connection_picker(state: &mut TuiState) {
    let mut choices = workshop_connection::connection_choices();
    let q = state.connection_picker_query.trim().to_ascii_lowercase();
    if !q.is_empty() {
        choices.retain(|c| {
            c.label.to_ascii_lowercase().contains(&q)
                || c.url.to_ascii_lowercase().contains(&q)
                || c.workshop_id
                    .as_deref()
                    .is_some_and(|id| id.to_ascii_lowercase().contains(&q))
        });
    }
    state.connection_picker_hits = choices;
    if state.connection_picker_selected >= state.connection_picker_hits.len() {
        state.connection_picker_selected = state.connection_picker_hits.len().saturating_sub(1);
    }
}

pub(crate) async fn apply_connection(
    state: &mut TuiState,
    url: &str,
    label: Option<&str>,
    workshop_id: Option<&str>,
) -> bool {
    let url = normalize_daemon_url(url);
    if url == normalize_daemon_url(&state.daemon_url) {
        // Still refresh label/scope metadata and remember.
        let (scope, resolved_label) = resolve_scope_for_url(&url);
        state.workshop_scope = scope;
        state.workshop_label = label.unwrap_or(&resolved_label).to_string();
        let _ = remember_daemon(
            &url,
            Some(state.workshop_label.as_str()),
            workshop_id.or(Some(state.workshop_scope.as_str())),
        );
        super::workspace_runtime::sync_mode_from_active_tab(state);
        super::push_obs(
            state,
            format!("◈ already on {} ({})", state.workshop_label, url),
        );
        return true;
    }

    // Checkpoint current workshop layout before switching.
    let _ = save_workspace_session_for(&state.workshop_scope, &state.workspace);

    // Drop workshop-local surface caches (authority is the next daemon).
    for sid in state.terminal_panes.keys().cloned().collect::<Vec<_>>() {
        super::terminal_runtime::detach_terminal(state, &sid);
    }
    state.note_buffers.clear();
    state.code_workspaces.clear();
    state.review_workspaces.clear();
    state.chat_lanes.clear();
    for (_, task) in state.session_tasks.drain() {
        task.abort();
    }
    state.turn_sessions.clear();
    if let Some(task) = state.active_request_task.take() {
        task.abort();
    }
    state.is_processing = false;

    let (scope, resolved_label) = resolve_scope_for_url(&url);
    let label = label.unwrap_or(&resolved_label).to_string();
    state.daemon_url = url.clone();
    state.workshop_scope = scope.clone();
    state.workshop_label = label.clone();
    let _ = remember_daemon(&url, Some(&label), workshop_id.or(Some(scope.as_str())));

    let session_id = state.session_id.clone();
    state.workspace = if let Some(mut shell) = load_workspace_session_for(&scope) {
        shell.sanitize();
        let title = short_session_title(&session_id);
        shell.rebind_focused_chat_session(&session_id, &title);
        shell
    } else {
        medousa::tui::workspace::WorkspaceShell::bootstrap(
            &session_id,
            &short_session_title(&session_id),
        )
    };

    // Reload chat history from the new daemon when possible.
    let session_for_history = state.session_id.clone();
    let history =
        super::history_services::load_history_daemon_first(state, &session_for_history).await;
    state.conversation = history;
    state.input_buffer.clear();
    state.conv_scroll = 0;
    state.auto_scroll = true;
    super::invalidate_markdown_cache(state);
    super::terminal_runtime::restore_terminal_tabs(state).await;
    super::workspace_runtime::sync_mode_from_active_tab(state);
    super::workspace_runtime::persist_workspace(state);

    super::push_obs(
        state,
        format!("✓ Connection → {label} ({url})  scope:{scope}"),
    );
    true
}

pub(crate) async fn handle_connection_picker_key(
    key: KeyEvent,
    state: &mut TuiState,
) -> EventOutcome {
    if state.connection_picker_editing_custom {
        match key.code {
            KeyCode::Esc => {
                state.connection_picker_editing_custom = false;
                state.connection_picker_custom.clear();
                EventOutcome::Continue
            }
            KeyCode::Enter => {
                let url = state.connection_picker_custom.trim().to_string();
                state.connection_picker_editing_custom = false;
                if url.is_empty() {
                    return EventOutcome::Continue;
                }
                let _ = apply_connection(state, &url, None, None).await;
                EventOutcome::Continue
            }
            KeyCode::Backspace => {
                state.connection_picker_custom.pop();
                EventOutcome::Continue
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                state.connection_picker_custom.push(c);
                EventOutcome::Continue
            }
            _ => EventOutcome::Continue,
        }
    } else {
        match key.code {
            KeyCode::Esc => {
                super::workspace_runtime::sync_mode_from_active_tab(state);
                if matches!(state.mode, UiMode::ConnectionPicker) {
                    state.mode = UiMode::Chat;
                }
                EventOutcome::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.connection_picker_selected =
                    state.connection_picker_selected.saturating_sub(1);
                EventOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !state.connection_picker_hits.is_empty() {
                    state.connection_picker_selected = (state.connection_picker_selected + 1)
                        .min(state.connection_picker_hits.len() - 1);
                }
                EventOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(hit) = state
                    .connection_picker_hits
                    .get(state.connection_picker_selected)
                    .cloned()
                {
                    let _ = apply_connection(
                        state,
                        &hit.url,
                        Some(&hit.label),
                        hit.workshop_id.as_deref(),
                    )
                    .await;
                }
                EventOutcome::Continue
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.connection_picker_editing_custom = true;
                state.connection_picker_custom.clear();
                EventOutcome::Continue
            }
            KeyCode::Char('u') => {
                state.connection_picker_editing_custom = true;
                state.connection_picker_custom.clear();
                EventOutcome::Continue
            }
            KeyCode::Backspace => {
                state.connection_picker_query.pop();
                refresh_connection_picker(state);
                EventOutcome::Continue
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                state.connection_picker_query.push(c);
                refresh_connection_picker(state);
                EventOutcome::Continue
            }
            _ => EventOutcome::Continue,
        }
    }
}

pub(crate) fn choice_subtitle(choice: &ConnectionChoice) -> String {
    format!(
        "{} · {}",
        workshop_connection::source_label(choice.source),
        choice.url
    )
}

#[allow(dead_code)]
pub(crate) fn source_glyph(source: ConnectionSource) -> &'static str {
    match source {
        ConnectionSource::Local => "⌂",
        ConnectionSource::HomeRegistry => "◈",
        ConnectionSource::Recent => "↻",
        ConnectionSource::Custom => "✎",
    }
}
