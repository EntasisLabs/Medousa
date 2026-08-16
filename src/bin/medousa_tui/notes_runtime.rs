//! Library-lite notes: vault list/search/read/write through the daemon SDK.

use medousa::daemon_api::{
    VaultBacklinksQuery, VaultNotesQuery, VaultPutQuery, VaultSearchQuery, VaultWriteRequest,
};
use medousa::tui::editor_buffer::TextBuffer;

use super::daemon_commands::daemon_client;
use super::{EventOutcome, NoteBuffer, NotesFocus, NotesPickerHit, TuiState, UiMode};

fn note_title_from_path(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".md")
        .to_string()
}

async fn fetch_vault_tree(daemon_url: &str) -> Vec<String> {
    let Ok(client) = daemon_client(daemon_url) else {
        return Vec::new();
    };
    match client
        .vault()
        .list_notes(&VaultNotesQuery {
            prefix: None,
            limit: Some(200),
            tags: None,
            tag_prefix: None,
            cursor: None,
            generation: None,
        })
        .await
    {
        Ok(resp) => {
            let mut paths: Vec<String> = resp.notes.into_iter().map(|n| n.path).collect();
            paths.sort();
            paths
        }
        Err(_) => Vec::new(),
    }
}

fn select_tree_path(tree: &[String], path: &str) -> usize {
    tree.iter().position(|p| p == path).unwrap_or(0)
}

fn apply_links(note: &mut NoteBuffer, backlinks: Vec<String>, wikilinks_out: Vec<String>) {
    note.backlinks = backlinks;
    note.wikilinks_out = wikilinks_out;
    note.links_selected = 0;
    note.links_scroll = 0;
}

fn link_targets(note: &NoteBuffer) -> Vec<String> {
    let mut targets = note.backlinks.clone();
    for out in &note.wikilinks_out {
        if !targets.iter().any(|t| t == out) {
            targets.push(out.clone());
        }
    }
    targets
}

fn observed_daemon_client(state: &mut TuiState) -> Option<medousa_sdk::MedousaClient> {
    match daemon_client(&state.daemon_url) {
        Ok(client) => Some(client),
        Err(error) => {
            super::push_obs(state, format!("⚠ daemon authentication failed: {error}"));
            None
        }
    }
}

pub(crate) async fn refresh_notes_picker(state: &mut TuiState) {
    let Some(client) = observed_daemon_client(state) else {
        return;
    };
    let query = state.notes_picker_query.trim();
    let result = if query.is_empty() {
        client
            .vault()
            .list_notes(&VaultNotesQuery {
                prefix: None,
                limit: Some(80),
                tags: None,
                tag_prefix: None,
                cursor: None,
                generation: None,
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
            refresh_active_note_sidebars(state).await;
            return true;
        }
        return false;
    }

    let Some(client) = observed_daemon_client(state) else {
        return false;
    };
    let tree = fetch_vault_tree(&state.daemon_url).await;
    match client.vault().get_note(path).await {
        Ok(resp) => {
            let title = note_title_from_path(&resp.note.path);
            let tree_selected = select_tree_path(&tree, &resp.note.path);
            let buffer = NoteBuffer {
                path: resp.note.path.clone(),
                title: title.clone(),
                buffer: TextBuffer::from_text(resp.content),
                content_hash: resp.note.content_hash,
                dirty: false,
                conflict: false,
                status: "loaded".to_string(),
                scroll: 0,
                preferred_col: None,
                tree,
                tree_selected,
                tree_scroll: tree_selected.saturating_sub(2) as u16,
                backlinks: resp.note.backlinks,
                wikilinks_out: resp.note.wikilinks_out,
                links_selected: 0,
                links_scroll: 0,
                focus: NotesFocus::Buffer,
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
    let Some(client) = observed_daemon_client(state) else {
        return false;
    };
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
            let tree = fetch_vault_tree(&state.daemon_url).await;
            let tree_selected = select_tree_path(&tree, &resp.note.path);
            state.note_buffers.insert(
                resp.note.path.clone(),
                NoteBuffer {
                    path: resp.note.path.clone(),
                    title: title.clone(),
                    buffer: TextBuffer::from_text(body),
                    content_hash: resp.note.content_hash,
                    dirty: false,
                    conflict: false,
                    status: "created".to_string(),
                    scroll: 0,
                    preferred_col: None,
                    tree,
                    tree_selected,
                    tree_scroll: tree_selected.saturating_sub(2) as u16,
                    backlinks: resp.note.backlinks,
                    wikilinks_out: resp.note.wikilinks_out,
                    links_selected: 0,
                    links_scroll: 0,
                    focus: NotesFocus::Buffer,
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

fn is_vault_conflict_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("412")
        || lower.contains("precondition")
        || lower.contains("if-match")
        || lower.contains("content_hash mismatch")
        || lower.contains("content-hash mismatch")
}

async fn put_active_note(state: &mut TuiState, force: bool) {
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
    if !note.dirty && !force {
        super::push_obs(state, "note unchanged".to_string());
        return;
    }
    let content = note.buffer.as_text().to_string();
    let if_match = if force {
        None
    } else {
        Some(note.content_hash.clone())
    };
    let Some(client) = observed_daemon_client(state) else {
        return;
    };
    match client
        .vault()
        .update_note(
            &path,
            &content,
            &VaultPutQuery {
                session_id: Some(state.session_id.clone()),
                auto_workshop_tags: None,
            },
            if_match.as_deref(),
        )
        .await
    {
        Ok(resp) => {
            if let Some(note) = state.note_buffers.get_mut(&path) {
                note.content_hash = resp.note.content_hash;
                note.dirty = false;
                note.conflict = false;
                note.status = if force {
                    "overwrote (kept mine)".to_string()
                } else {
                    "saved".to_string()
                };
                apply_links(note, resp.note.backlinks, resp.note.wikilinks_out);
            }
            // Tree may include a newly created path after first save of a draft.
            refresh_active_note_sidebars(state).await;
            super::push_obs(
                state,
                if force {
                    format!("✓ overwrote {path} (kept mine)")
                } else {
                    format!("✓ saved {path}")
                },
            );
        }
        Err(err) => {
            let conflict = is_vault_conflict_error(&err.to_string());
            if let Some(note) = state.note_buffers.get_mut(&path) {
                note.conflict = conflict;
                note.status = if conflict {
                    "conflict — Ctrl+R reload · Ctrl+Y keep mine".to_string()
                } else {
                    format!("save failed: {err}")
                };
            }
            if conflict {
                super::push_obs(
                    state,
                    format!(
                        "⚠ vault conflict on {path} (If-Match / content_hash). Ctrl+R reload · Ctrl+Y keep mine"
                    ),
                );
            } else {
                super::push_obs(state, format!("⚠ vault save failed: {err}"));
            }
        }
    }
}

pub(crate) async fn save_active_note(state: &mut TuiState) {
    put_active_note(state, false).await;
}

pub(crate) async fn overwrite_active_note(state: &mut TuiState) {
    put_active_note(state, true).await;
}

pub(crate) async fn refresh_active_note_sidebars(state: &mut TuiState) {
    let Some(path) = state
        .workspace
        .active_tab()
        .and_then(|t| t.notes_path().map(str::to_string))
    else {
        return;
    };
    let tree = fetch_vault_tree(&state.daemon_url).await;
    let Some(client) = observed_daemon_client(state) else {
        return;
    };
    let (backlinks, wikilinks_out) = match client.vault().get_note(&path).await {
        Ok(resp) => (resp.note.backlinks, resp.note.wikilinks_out),
        Err(_) => {
            // Fallback: backlinks endpoint only.
            let bl = client
                .vault()
                .backlinks(&VaultBacklinksQuery {
                    path: Some(path.clone()),
                })
                .await
                .map(|r| r.backlinks)
                .unwrap_or_default();
            (bl, Vec::new())
        }
    };
    if let Some(note) = state.note_buffers.get_mut(&path) {
        note.tree = tree;
        note.tree_selected = select_tree_path(&note.tree, &path);
        apply_links(note, backlinks, wikilinks_out);
    }
}

pub(crate) async fn reload_active_note(state: &mut TuiState) {
    let Some(path) = state
        .workspace
        .active_tab()
        .and_then(|t| t.notes_path().map(str::to_string))
    else {
        super::push_obs(state, "⚠ no note tab focused".to_string());
        return;
    };
    let Some(client) = observed_daemon_client(state) else {
        return;
    };
    match client.vault().get_note(&path).await {
        Ok(resp) => {
            if let Some(note) = state.note_buffers.get_mut(&path) {
                note.buffer = TextBuffer::from_text(resp.content);
                note.content_hash = resp.note.content_hash;
                note.dirty = false;
                note.conflict = false;
                note.status = "reloaded".to_string();
                note.preferred_col = None;
                note.scroll = 0;
                apply_links(note, resp.note.backlinks, resp.note.wikilinks_out);
            }
            refresh_active_note_sidebars(state).await;
            super::push_obs(state, format!("✓ reloaded {path} from vault"));
        }
        Err(err) => {
            super::push_obs(state, format!("⚠ vault reload failed: {err}"));
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
    if key.code == KeyCode::Char('r') && key.modifiers.contains(KeyModifiers::CONTROL) {
        reload_active_note(state).await;
        return EventOutcome::Continue;
    }
    if key.code == KeyCode::Char('y') && key.modifiers.contains(KeyModifiers::CONTROL) {
        // Keep mine — force save without If-Match (Home conflict bar parity).
        overwrite_active_note(state).await;
        return EventOutcome::Continue;
    }
    if key.code == KeyCode::Tab {
        if let Some(note) = focused_note_mut(state) {
            note.focus = match note.focus {
                NotesFocus::Tree => NotesFocus::Buffer,
                NotesFocus::Buffer => NotesFocus::Backlinks,
                NotesFocus::Backlinks => NotesFocus::Tree,
            };
        }
        return EventOutcome::Continue;
    }
    if key.code == KeyCode::Esc {
        if let Some(note) = focused_note_mut(state) {
            match note.focus {
                NotesFocus::Buffer => {
                    note.focus = NotesFocus::Tree;
                    return EventOutcome::Continue;
                }
                NotesFocus::Backlinks => {
                    note.focus = NotesFocus::Buffer;
                    return EventOutcome::Continue;
                }
                NotesFocus::Tree => {}
            }
        }
        state.mode = UiMode::Chat;
        return EventOutcome::Continue;
    }

    let focus = focused_note_mut(state).map(|n| n.focus);
    let Some(focus) = focus else {
        return EventOutcome::Continue;
    };

    match focus {
        NotesFocus::Tree => {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if let Some(note) = focused_note_mut(state) {
                        note.tree_selected = note.tree_selected.saturating_sub(1);
                        note.tree_scroll = note.tree_scroll.min(note.tree_selected as u16);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if let Some(note) = focused_note_mut(state)
                        && !note.tree.is_empty()
                    {
                        note.tree_selected = (note.tree_selected + 1)
                            .min(note.tree.len().saturating_sub(1));
                        if note.tree_selected as u16 >= note.tree_scroll.saturating_add(8) {
                            note.tree_scroll = note.tree_scroll.saturating_add(1);
                        }
                    }
                }
                KeyCode::Enter => {
                    let path = focused_note_mut(state).and_then(|note| {
                        note.tree.get(note.tree_selected).cloned()
                    });
                    if let Some(path) = path {
                        let _ = open_note_path(state, &path).await;
                        if let Some(note) = focused_note_mut(state) {
                            note.focus = NotesFocus::Buffer;
                        }
                    }
                }
                _ => {}
            }
        }
        NotesFocus::Backlinks => {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if let Some(note) = focused_note_mut(state) {
                        note.links_selected = note.links_selected.saturating_sub(1);
                        note.links_scroll = note.links_scroll.min(note.links_selected as u16);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if let Some(note) = focused_note_mut(state) {
                        let len = link_targets(note).len();
                        if len > 0 {
                            note.links_selected =
                                (note.links_selected + 1).min(len.saturating_sub(1));
                            if note.links_selected as u16 >= note.links_scroll.saturating_add(8) {
                                note.links_scroll = note.links_scroll.saturating_add(1);
                            }
                        }
                    }
                }
                KeyCode::Enter => {
                    let path = focused_note_mut(state).and_then(|note| {
                        link_targets(note).get(note.links_selected).cloned()
                    });
                    if let Some(path) = path {
                        let _ = open_note_path(state, &path).await;
                        if let Some(note) = focused_note_mut(state) {
                            note.focus = NotesFocus::Buffer;
                        }
                    }
                }
                _ => {}
            }
        }
        NotesFocus::Buffer => {
            let Some(note) = focused_note_mut(state) else {
                return EventOutcome::Continue;
            };
            match key.code {
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    note.buffer.insert_char(c);
                    note.dirty = true;
                    note.conflict = false;
                    note.preferred_col = None;
                }
                KeyCode::Enter => {
                    note.buffer.insert_newline();
                    note.dirty = true;
                    note.conflict = false;
                    note.preferred_col = None;
                }
                KeyCode::Backspace => {
                    note.buffer.backspace();
                    note.dirty = true;
                    note.conflict = false;
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
        }
    }
    EventOutcome::Continue
}
