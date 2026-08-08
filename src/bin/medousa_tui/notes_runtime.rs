//! Library-lite notes: vault list/search/read/write through the daemon SDK.

use medousa::daemon_api::{VaultNotesQuery, VaultPutQuery, VaultSearchQuery, VaultWriteRequest};
use medousa::tui::editor_buffer::TextBuffer;

use super::daemon_commands::daemon_client;
use super::{EventOutcome, NoteBuffer, NotesPickerHit, TuiState, UiMode};

fn note_title_from_path(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".md")
        .to_string()
}

pub(crate) async fn refresh_notes_picker(state: &mut TuiState) {
    let client = daemon_client(&state.daemon_url);
    let query = state.notes_picker_query.trim();
    let result = if query.is_empty() {
        client
            .vault()
            .list_notes(&VaultNotesQuery {
                prefix: None,
                limit: Some(80),
                tags: None,
                tag_prefix: None,
            })
            .await
            .map(|resp| {
                resp.notes
                    .into_iter()
                    .map(|note| NotesPickerHit {
                        title: if note.title.trim().is_empty() {
                            note_title_from_path(&note.path)
                        } else {
                            note.title
                        },
                        path: note.path,
                        snippet: String::new(),
                    })
                    .collect::<Vec<_>>()
            })
    } else {
        client
            .vault()
            .search(&VaultSearchQuery {
                q: Some(query.to_string()),
                limit: Some(40),
                tags: None,
            })
            .await
            .map(|resp| {
                resp.hits
                    .into_iter()
                    .map(|hit| {
                        let path = hit.note.path.clone();
                        NotesPickerHit {
                            title: note_title_from_path(&path),
                            path,
                            snippet: hit.snippet.unwrap_or_default(),
                        }
                    })
                    .collect::<Vec<_>>()
            })
    };

    match result {
        Ok(hits) => {
            state.notes_picker_hits = hits;
            if state.notes_picker_selected >= state.notes_picker_hits.len() {
                state.notes_picker_selected = state.notes_picker_hits.len().saturating_sub(1);
            }
        }
        Err(err) => {
            state.notes_picker_hits.clear();
            super::push_obs(state, format!("⚠ vault list/search failed: {err}"));
        }
    }
}

pub(crate) async fn open_notes_picker(state: &mut TuiState) {
    state.mode = UiMode::NotesPicker;
    state.notes_picker_query.clear();
    state.notes_picker_selected = 0;
    state.notes_picker_scroll = 0;
    refresh_notes_picker(state).await;
}

pub(crate) async fn open_note_path(state: &mut TuiState, path: &str) -> bool {
    if let Some(existing) = state.note_buffers.get(path) {
        let title = existing.title.clone();
        if state.workspace.open_notes_tab_in_active(path, &title) {
            state.mode = UiMode::Notes;
            super::workspace_runtime::persist_workspace(state);
            return true;
        }
        return false;
    }

    let client = daemon_client(&state.daemon_url);
    match client.vault().get_note(path).await {
        Ok(resp) => {
            let title = note_title_from_path(&resp.note.path);
            let buffer = NoteBuffer {
                path: resp.note.path.clone(),
                title: title.clone(),
                buffer: TextBuffer::from_text(resp.content),
                content_hash: resp.note.content_hash,
                dirty: false,
                status: "loaded".to_string(),
                scroll: 0,
                preferred_col: None,
            };
            state.note_buffers.insert(resp.note.path.clone(), buffer);
            if state.workspace.open_notes_tab_in_active(&resp.note.path, &title) {
                state.mode = UiMode::Notes;
                super::workspace_runtime::persist_workspace(state);
                super::push_obs(state, format!("✓ opened note {}", resp.note.path));
                true
            } else {
                super::push_obs(state, "⚠ tab cap reached".to_string());
                false
            }
        }
        Err(err) => {
            super::push_obs(state, format!("⚠ vault read failed: {err}"));
            false
        }
    }
}

pub(crate) async fn create_note(state: &mut TuiState, path: &str, content: &str) -> bool {
    let client = daemon_client(&state.daemon_url);
    let request = VaultWriteRequest {
        path: Some(path.to_string()),
        content: content.to_string(),
        session_id: Some(state.session_id.clone()),
        semantic_tags: None,
        auto_workshop_tags: false,
    };
    match client.vault().create_note(&request).await {
        Ok(resp) => {
            let title = note_title_from_path(&resp.note.path);
            let body = resp.content.unwrap_or_else(|| content.to_string());
            state.note_buffers.insert(
                resp.note.path.clone(),
                NoteBuffer {
                    path: resp.note.path.clone(),
                    title: title.clone(),
                    buffer: TextBuffer::from_text(body),
                    content_hash: resp.note.content_hash,
                    dirty: false,
                    status: "created".to_string(),
                    scroll: 0,
                    preferred_col: None,
                },
            );
            let _ = state
                .workspace
                .open_notes_tab_in_active(&resp.note.path, &title);
            state.mode = UiMode::Notes;
            super::workspace_runtime::persist_workspace(state);
            super::push_obs(state, format!("✓ created note {}", resp.note.path));
            true
        }
        Err(err) => {
            super::push_obs(state, format!("⚠ vault create failed: {err}"));
            false
        }
    }
}

pub(crate) async fn save_active_note(state: &mut TuiState) {
    let Some(path) = state
        .workspace
        .active_tab()
        .and_then(|t| t.notes_path().map(str::to_string))
    else {
        super::push_obs(state, "⚠ no note tab focused".to_string());
        return;
    };
    let Some(note) = state.note_buffers.get(&path) else {
        super::push_obs(state, "⚠ note buffer missing".to_string());
        return;
    };
    if !note.dirty {
        super::push_obs(state, "note unchanged".to_string());
        return;
    }
    let content = note.buffer.as_text().to_string();
    let if_match = note.content_hash.clone();
    let client = daemon_client(&state.daemon_url);
    match client
        .vault()
        .update_note(
            &path,
            &content,
            &VaultPutQuery {
                session_id: Some(state.session_id.clone()),
                auto_workshop_tags: None,
            },
            Some(&if_match),
        )
        .await
    {
        Ok(resp) => {
            if let Some(note) = state.note_buffers.get_mut(&path) {
                note.content_hash = resp.note.content_hash;
                note.dirty = false;
                note.status = "saved".to_string();
            }
            super::push_obs(state, format!("✓ saved {path}"));
        }
        Err(err) => {
            if let Some(note) = state.note_buffers.get_mut(&path) {
                note.status = format!("save failed: {err}");
            }
            super::push_obs(state, format!("⚠ vault save failed: {err}"));
        }
    }
}

pub(crate) fn focused_note_mut(state: &mut TuiState) -> Option<&mut NoteBuffer> {
    let path = state
        .workspace
        .active_tab()
        .and_then(|t| t.notes_path().map(str::to_string))?;
    state.note_buffers.get_mut(&path)
}

pub(crate) async fn ask_about_active_note(state: &mut TuiState) {
    let Some(path) = state
        .workspace
        .active_tab()
        .and_then(|t| t.notes_path().map(str::to_string))
    else {
        super::push_obs(state, "⚠ open a note first".to_string());
        return;
    };
    let snippet = state
        .note_buffers
        .get(&path)
        .map(|n| {
            let text = n.buffer.as_text();
            if text.len() > 600 {
                format!("{}…", &text[..600])
            } else {
                text.to_string()
            }
        })
        .unwrap_or_default();
    // Prefer an existing chat tab in this pane; else open a fresh chat session.
    let chat_session = uuid::Uuid::new_v4().simple().to_string();
    let title = format!("Ask · {}", note_title_from_path(&path));
    if state
        .workspace
        .open_chat_tab_in_active(&chat_session, &title)
    {
        state.session_id = chat_session;
        state.session_display_name = Some(title);
        state.conversation.clear();
        state.input_buffer = format!(
            "About note `{path}`:\n\n```md\n{snippet}\n```\n\n"
        );
        state.mode = UiMode::Chat;
        super::workspace_runtime::persist_workspace(state);
        super::push_obs(state, "✓ ask-about-note composer ready".to_string());
    }
}

pub(crate) async fn handle_notes_picker_key(
    key: crossterm::event::KeyEvent,
    state: &mut TuiState,
) -> EventOutcome {
    use crossterm::event::KeyCode;
    match key.code {
        KeyCode::Esc => {
            state.mode = UiMode::Chat;
            EventOutcome::Continue
        }
        KeyCode::Up => {
            state.notes_picker_selected = state.notes_picker_selected.saturating_sub(1);
            EventOutcome::Continue
        }
        KeyCode::Down => {
            if !state.notes_picker_hits.is_empty() {
                state.notes_picker_selected = (state.notes_picker_selected + 1)
                    .min(state.notes_picker_hits.len().saturating_sub(1));
            }
            EventOutcome::Continue
        }
        KeyCode::Enter => {
            if let Some(hit) = state.notes_picker_hits.get(state.notes_picker_selected).cloned()
            {
                let _ = open_note_path(state, &hit.path).await;
            }
            EventOutcome::Continue
        }
        KeyCode::Backspace => {
            state.notes_picker_query.pop();
            refresh_notes_picker(state).await;
            EventOutcome::Continue
        }
        KeyCode::Char(c) => {
            state.notes_picker_query.push(c);
            refresh_notes_picker(state).await;
            EventOutcome::Continue
        }
        _ => EventOutcome::Continue,
    }
}

pub(crate) async fn handle_notes_key(
    key: crossterm::event::KeyEvent,
    state: &mut TuiState,
) -> EventOutcome {
    use crossterm::event::{KeyCode, KeyModifiers};

    if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
        save_active_note(state).await;
        return EventOutcome::Continue;
    }
    if key.code == KeyCode::Char('a') && key.modifiers.contains(KeyModifiers::CONTROL) {
        ask_about_active_note(state).await;
        return EventOutcome::Continue;
    }
    if key.code == KeyCode::Esc {
        state.mode = UiMode::Chat;
        return EventOutcome::Continue;
    }

    let Some(note) = focused_note_mut(state) else {
        return EventOutcome::Continue;
    };

    match key.code {
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            note.buffer.insert_char(c);
            note.dirty = true;
            note.preferred_col = None;
        }
        KeyCode::Enter => {
            note.buffer.insert_newline();
            note.dirty = true;
            note.preferred_col = None;
        }
        KeyCode::Backspace => {
            note.buffer.backspace();
            note.dirty = true;
            note.preferred_col = None;
        }
        KeyCode::Left => {
            note.buffer.move_left();
            note.preferred_col = None;
        }
        KeyCode::Right => {
            note.buffer.move_right();
            note.preferred_col = None;
        }
        KeyCode::Up => {
            let col = note.preferred_col.unwrap_or_else(|| note.buffer.line_col().1);
            note.preferred_col = Some(col);
            note.buffer.move_up(col);
            note.scroll = note.scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            let col = note.preferred_col.unwrap_or_else(|| note.buffer.line_col().1);
            note.preferred_col = Some(col);
            note.buffer.move_down(col);
            note.scroll = note.scroll.saturating_add(1);
        }
        KeyCode::Home => note.buffer.move_line_start(),
        KeyCode::End => note.buffer.move_line_end(),
        KeyCode::PageUp => note.scroll = note.scroll.saturating_sub(10),
        KeyCode::PageDown => note.scroll = note.scroll.saturating_add(10),
        _ => {}
    }
    EventOutcome::Continue
}
