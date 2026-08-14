//! Terminal panes — attach to workshop `medousa-session` via daemon `/v1/sessions/shell*`.
//!
//! Splits create **new** shell sessions (Home rule). VT parse uses pure-Rust `vte`
//! into [`medousa::tui::vt_grid::VtGrid`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures_util::{SinkExt, StreamExt};
use medousa::tui::vt_grid::VtGrid;
use medousa::tui::workspace::short_terminal_title;
use medousa_sdk::transport::decode;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::Message;
use vte::Parser;

use super::daemon_commands::daemon_client;
use super::{EventOutcome, TuiState, UiMode};

pub(crate) enum OutboundFrame {
    Stdin(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

#[derive(Debug, Clone)]
pub(crate) enum TerminalUiEvent {
    Dirty { session_id: String },
    Status {
        session_id: String,
        connected: bool,
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ShellSessionSummary {
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub root_kind: String,
    #[serde(default)]
    pub work_id: Option<String>,
}

pub(crate) struct TerminalPane {
    pub session_id: String,
    pub work_id: Option<String>,
    #[allow(dead_code)]
    pub title: String,
    pub cols: u16,
    pub rows: u16,
    pub grid: Arc<Mutex<VtGrid>>,
    pub parser: Arc<Mutex<Parser>>,
    pub status: String,
    pub connected: bool,
    /// Local pager offset into scrollback (0 = live). PageUp/PageDown.
    pub view_offset: u16,
    pub stdin_tx: Option<mpsc::UnboundedSender<OutboundFrame>>,
    pub attach_task: Option<tokio::task::JoinHandle<()>>,
}

impl TerminalPane {
    fn new(session_id: String, work_id: Option<String>, title: String, cols: u16, rows: u16) -> Self {
        Self {
            session_id,
            work_id,
            title,
            cols,
            rows,
            grid: Arc::new(Mutex::new(VtGrid::new(cols, rows))),
            parser: Arc::new(Mutex::new(Parser::new())),
            status: "connecting…".to_string(),
            connected: false,
            view_offset: 0,
            stdin_tx: None,
            attach_task: None,
        }
    }
}

fn ws_url_for(daemon_url: &str, path: &str) -> String {
    let base = daemon_url.trim_end_matches('/').replacen("http", "ws", 1);
    format!("{base}{path}")
}

async fn shell_get<T: serde::de::DeserializeOwned>(
    daemon_url: &str,
    path: &str,
) -> Result<T, String> {
    let client = daemon_client(daemon_url).map_err(|error| error.to_string())?;
    let value = client
        .transport()
        .get_json(client.base_url(), path)
        .await
        .map_err(|e| e.to_string())?;
    decode(value).await.map_err(|e| e.to_string())
}

async fn shell_post<T: serde::de::DeserializeOwned>(
    daemon_url: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<T, String> {
    let client = daemon_client(daemon_url).map_err(|error| error.to_string())?;
    let value = client
        .transport()
        .post_json(client.base_url(), path, body)
        .await
        .map_err(|e| e.to_string())?;
    decode(value).await.map_err(|e| e.to_string())
}

pub(crate) fn empty_terminal_panes() -> HashMap<String, TerminalPane> {
    HashMap::new()
}

pub(crate) fn handle_terminal_event(event: TerminalUiEvent, state: &mut TuiState) {
    match event {
        TerminalUiEvent::Dirty { session_id } => {
            // Stick to the live bottom when new output arrives.
            if let Some(pane) = state.terminal_panes.get_mut(&session_id) {
                pane.view_offset = 0;
            }
        }
        TerminalUiEvent::Status {
            session_id,
            connected,
            message,
        } => {
            if let Some(pane) = state.terminal_panes.get_mut(&session_id) {
                pane.connected = connected;
                pane.status = if connected {
                    "connected".to_string()
                } else {
                    message.unwrap_or_else(|| "disconnected".to_string())
                };
            }
        }
    }
}

fn inherit_work_id(state: &TuiState) -> Option<String> {
    state
        .workspace
        .active_tab()
        .and_then(|t| t.forge_work_id().map(str::to_string))
        .or_else(|| {
            state
                .workspace
                .layout()
                .tabs
                .iter()
                .find_map(|t| t.code_work_id().map(str::to_string))
        })
}

pub(crate) async fn open_new_terminal(state: &mut TuiState) -> bool {
    let work_id = inherit_work_id(state);
    create_and_open_terminal(state, work_id.as_deref(), None).await
}

pub(crate) async fn attach_or_create_terminal(
    state: &mut TuiState,
    session_id: Option<&str>,
    work_id: Option<&str>,
) -> bool {
    if let Some(sid) = session_id.map(str::trim).filter(|s| !s.is_empty()) {
        return attach_existing_terminal(state, sid, work_id).await;
    }
    create_and_open_terminal(state, work_id.or(inherit_work_id(state).as_deref()), None).await
}

async fn create_and_open_terminal(
    state: &mut TuiState,
    work_id: Option<&str>,
    title_override: Option<&str>,
) -> bool {
    #[derive(Deserialize)]
    struct CreateResp {
        ok: bool,
        session_id: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        work_id: Option<String>,
    }

    let cols = 80u16;
    let rows = 24u16;
    let body = json!({
        "work_id": work_id,
        "cols": cols,
        "rows": rows,
    });
    let created = match shell_post::<CreateResp>(&state.daemon_url, "/v1/sessions/shell", body).await
    {
        Ok(resp) if resp.ok => resp,
        Ok(resp) => {
            super::push_obs(
                state,
                format!(
                    "⚠ shell create failed: {}",
                    if resp.message.is_empty() {
                        "unknown"
                    } else {
                        &resp.message
                    }
                ),
            );
            return false;
        }
        Err(err) => {
            super::push_obs(state, format!("⚠ shell create failed: {err}"));
            return false;
        }
    };

    let title = title_override
        .map(str::to_string)
        .unwrap_or_else(|| short_terminal_title(&created.session_id));
    let bound_work = created.work_id.or_else(|| work_id.map(str::to_string));
    insert_and_attach(state, created.session_id, bound_work, title, cols, rows).await
}

async fn attach_existing_terminal(
    state: &mut TuiState,
    session_id: &str,
    work_id: Option<&str>,
) -> bool {
    if state.terminal_panes.contains_key(session_id) {
        if state.workspace.open_terminal_tab_in_active(
            session_id,
            work_id.or_else(|| {
                state
                    .terminal_panes
                    .get(session_id)
                    .and_then(|p| p.work_id.as_deref())
            }),
            &short_terminal_title(session_id),
        ) {
            state.mode = UiMode::Terminal;
            super::workspace_runtime::persist_workspace(state);
            return true;
        }
        return false;
    }
    let title = short_terminal_title(session_id);
    insert_and_attach(
        state,
        session_id.to_string(),
        work_id.map(str::to_string),
        title,
        80,
        24,
    )
    .await
}

async fn insert_and_attach(
    state: &mut TuiState,
    session_id: String,
    work_id: Option<String>,
    title: String,
    cols: u16,
    rows: u16,
) -> bool {
    if !state
        .workspace
        .open_terminal_tab_in_active(&session_id, work_id.as_deref(), &title)
    {
        super::push_obs(state, "⚠ tab cap reached".to_string());
        return false;
    }

    let mut pane = TerminalPane::new(session_id.clone(), work_id, title.clone(), cols, rows);
    spawn_attach(state, &mut pane);
    state.terminal_panes.insert(session_id.clone(), pane);
    state.mode = UiMode::Terminal;
    super::workspace_runtime::persist_workspace(state);
    super::push_obs(state, format!("✓ terminal {title}"));
    true
}

/// Create a new shell session and place it in a newly split pane.
pub(crate) async fn split_with_new_terminal(
    state: &mut TuiState,
    direction: medousa::tui::workspace::SplitDirection,
) -> EventOutcome {
    #[derive(Deserialize)]
    struct CreateResp {
        ok: bool,
        session_id: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        work_id: Option<String>,
    }

    let work_id = inherit_work_id(state);
    let cols = 80u16;
    let rows = 24u16;
    let body = json!({
        "work_id": work_id,
        "cols": cols,
        "rows": rows,
    });
    let created = match shell_post::<CreateResp>(&state.daemon_url, "/v1/sessions/shell", body).await
    {
        Ok(resp) if resp.ok => resp,
        Ok(resp) => {
            super::push_obs(
                state,
                format!(
                    "⚠ shell create failed: {}",
                    if resp.message.is_empty() {
                        "unknown"
                    } else {
                        &resp.message
                    }
                ),
            );
            return EventOutcome::Continue;
        }
        Err(err) => {
            super::push_obs(state, format!("⚠ shell create failed: {err}"));
            return EventOutcome::Continue;
        }
    };

    let title = short_terminal_title(&created.session_id);
    let bound_work = created.work_id.or(work_id);
    let tab = medousa::tui::workspace::new_terminal_tab(
        created.session_id.clone(),
        bound_work.clone(),
        title.clone(),
    );
    if !state.workspace.split_active_with_tab(direction, tab) {
        super::push_obs(state, "⚠ pane cap reached (max 4)".to_string());
        return EventOutcome::Continue;
    }

    let mut pane = TerminalPane::new(created.session_id.clone(), bound_work, title.clone(), cols, rows);
    spawn_attach(state, &mut pane);
    state.terminal_panes.insert(created.session_id, pane);
    state.mode = UiMode::Terminal;
    super::workspace_runtime::persist_workspace(state);
    super::push_obs(
        state,
        format!(
            "✓ terminal split → {} panes ({title})",
            state.workspace.pane_count()
        ),
    );
    EventOutcome::Continue
}

struct AttachConfig {
    daemon_url: String,
    session_id: String,
    cols: u16,
    rows: u16,
    grid: Arc<Mutex<VtGrid>>,
    parser: Arc<Mutex<Parser>>,
    event_tx: mpsc::Sender<TerminalUiEvent>,
}

fn spawn_attach(state: &TuiState, pane: &mut TerminalPane) {
    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    pane.stdin_tx = Some(stdin_tx);
    let cfg = AttachConfig {
        daemon_url: state.daemon_url.clone(),
        session_id: pane.session_id.clone(),
        cols: pane.cols,
        rows: pane.rows,
        grid: Arc::clone(&pane.grid),
        parser: Arc::clone(&pane.parser),
        event_tx: state.terminal_event_tx.clone(),
    };
    pane.attach_task = Some(tokio::spawn(run_attach(cfg, stdin_rx)));
}

async fn run_attach(
    cfg: AttachConfig,
    mut stdin_rx: mpsc::UnboundedReceiver<OutboundFrame>,
) {
    let AttachConfig {
        daemon_url,
        session_id,
        cols,
        rows,
        grid,
        parser,
        event_tx,
    } = cfg;
    let url = ws_url_for(
        &daemon_url,
        &format!(
            "/v1/sessions/shell/{}",
            urlencoding::encode(&session_id)
        ),
    );
    let Ok(mut request) = url.as_str().into_client_request() else {
        let _ = event_tx
            .send(TerminalUiEvent::Status {
                session_id,
                connected: false,
                message: Some("invalid ws URL".to_string()),
            })
            .await;
        return;
    };
    let authorization = medousa::local_daemon_auth::authorization_header(
        &url,
        medousa_local_credential::TUI_LOCAL_NAME,
    );
    let Ok(authorization) = authorization else {
        let _ = event_tx
            .send(TerminalUiEvent::Status {
                session_id,
                connected: false,
                message: Some("ws authentication failed".to_string()),
            })
            .await;
        return;
    };
    if let Some(value) = authorization {
        request.headers_mut().insert(AUTHORIZATION, value);
    }
    let Ok((websocket, _)) = tokio_tungstenite::connect_async(request).await else {
        let _ = event_tx
            .send(TerminalUiEvent::Status {
                session_id,
                connected: false,
                message: Some("ws connect failed".to_string()),
            })
            .await;
        return;
    };

    let (mut ws_tx, mut ws_rx) = websocket.split();
    let resize = json!({
        "type": "resize",
        "cols": cols.max(2),
        "rows": rows.max(1),
    });
    if ws_tx
        .send(Message::Text(resize.to_string().into()))
        .await
        .is_err()
    {
        let _ = event_tx
            .send(TerminalUiEvent::Status {
                session_id,
                connected: false,
                message: Some("initial resize failed".to_string()),
            })
            .await;
        return;
    }
    let _ = event_tx
        .send(TerminalUiEvent::Status {
            session_id: session_id.clone(),
            connected: true,
            message: None,
        })
        .await;

    let mut disconnect_message: Option<String> = None;
    loop {
        tokio::select! {
            command = stdin_rx.recv() => {
                let Some(command) = command else { break; };
                let frame = match command {
                    OutboundFrame::Stdin(bytes) => json!({
                        "type": "stdin",
                        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                    }),
                    OutboundFrame::Resize { cols, rows } => json!({
                        "type": "resize",
                        "cols": cols.max(2),
                        "rows": rows.max(1),
                    }),
                };
                if ws_tx.send(Message::Text(frame.to_string().into())).await.is_err() {
                    disconnect_message = Some("ws send failed".to_string());
                    break;
                }
            }
            message = ws_rx.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                            match value.get("type").and_then(|t| t.as_str()) {
                                Some("stdout") => {
                                    if let Some(data) = value.get("data").and_then(|d| d.as_str())
                                        && let Ok(bytes) = base64::engine::general_purpose::STANDARD
                                            .decode(data)
                                    {
                                        feed_grid(&grid, &parser, &bytes);
                                        let _ = event_tx
                                            .send(TerminalUiEvent::Dirty {
                                                session_id: session_id.clone(),
                                            })
                                            .await;
                                    }
                                }
                                Some("error") => {
                                    disconnect_message = Some(
                                        value
                                            .get("message")
                                            .and_then(|m| m.as_str())
                                            .unwrap_or("terminal protocol error")
                                            .to_string(),
                                    );
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        feed_grid(&grid, &parser, &bytes);
                        let _ = event_tx
                            .send(TerminalUiEvent::Dirty {
                                session_id: session_id.clone(),
                            })
                            .await;
                    }
                    Some(Ok(Message::Close(frame))) => {
                        disconnect_message = frame.map(|f| f.reason.to_string());
                        break;
                    }
                    Some(Err(err)) => {
                        disconnect_message = Some(err.to_string());
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
        }
    }

    let _ = event_tx
        .send(TerminalUiEvent::Status {
            session_id,
            connected: false,
            message: disconnect_message,
        })
        .await;
}

fn feed_grid(grid: &Arc<Mutex<VtGrid>>, parser: &Arc<Mutex<Parser>>, bytes: &[u8]) {
    let Ok(mut grid) = grid.lock() else {
        return;
    };
    let Ok(mut parser) = parser.lock() else {
        return;
    };
    grid.feed_bytes(&mut parser, bytes);
}

pub(crate) fn detach_terminal(state: &mut TuiState, session_id: &str) {
    if let Some(mut pane) = state.terminal_panes.remove(session_id) {
        pane.stdin_tx.take();
        if let Some(task) = pane.attach_task.take() {
            task.abort();
        }
    }
}

pub(crate) fn detach_orphaned_terminals(state: &mut TuiState) {
    let live: std::collections::HashSet<String> = state
        .workspace
        .desktops
        .iter()
        .flat_map(|d| d.layout.tabs.iter())
        .filter_map(|t| t.terminal_session_id().map(str::to_string))
        .collect();
    let orphaned: Vec<String> = state
        .terminal_panes
        .keys()
        .filter(|id| !live.contains(*id))
        .cloned()
        .collect();
    for id in orphaned {
        detach_terminal(state, &id);
    }
}

/// Re-attach any Terminal tabs restored from the workspace session file.
pub(crate) async fn restore_terminal_tabs(state: &mut TuiState) {
    let targets: Vec<(String, Option<String>, String)> = state
        .workspace
        .desktops
        .iter()
        .flat_map(|d| d.layout.tabs.iter())
        .filter_map(|t| match t {
            medousa::tui::workspace::ShellTab::Terminal {
                session_id,
                work_id,
                title,
                ..
            } => Some((session_id.clone(), work_id.clone(), title.clone())),
            _ => None,
        })
        .collect();
    for (session_id, work_id, title) in targets {
        if state.terminal_panes.contains_key(&session_id) {
            continue;
        }
        let mut pane = TerminalPane::new(session_id.clone(), work_id, title, 80, 24);
        spawn_attach(state, &mut pane);
        state.terminal_panes.insert(session_id, pane);
    }
}

pub(crate) fn ensure_geometry(state: &mut TuiState, session_id: &str, cols: u16, rows: u16) {
    let cols = cols.max(2);
    let rows = rows.max(1);
    let Some(pane) = state.terminal_panes.get_mut(session_id) else {
        return;
    };
    if pane.cols == cols && pane.rows == rows {
        return;
    }
    pane.cols = cols;
    pane.rows = rows;
    if let Ok(mut grid) = pane.grid.lock() {
        grid.resize(cols, rows);
    }
    if let Some(tx) = &pane.stdin_tx {
        let _ = tx.send(OutboundFrame::Resize { cols, rows });
    }
}

pub(crate) async fn open_terminal_picker(state: &mut TuiState) {
    state.mode = UiMode::TerminalPicker;
    state.terminal_picker_selected = 0;
    state.terminal_picker_query.clear();
    refresh_terminal_picker(state).await;
}

pub(crate) async fn refresh_terminal_picker(state: &mut TuiState) {
    #[derive(Deserialize)]
    struct ListResp {
        #[serde(default)]
        sessions: Vec<ShellSessionSummary>,
    }
    match shell_get::<ListResp>(&state.daemon_url, "/v1/sessions/shell").await {
        Ok(resp) => {
            let mut sessions = resp.sessions;
            let q = state.terminal_picker_query.trim().to_ascii_lowercase();
            if !q.is_empty() {
                sessions.retain(|s| {
                    s.session_id.to_ascii_lowercase().contains(&q)
                        || s.cwd.to_ascii_lowercase().contains(&q)
                        || s.work_id
                            .as_deref()
                            .is_some_and(|w| w.to_ascii_lowercase().contains(&q))
                });
            }
            state.terminal_picker_hits = sessions;
            if state.terminal_picker_selected >= state.terminal_picker_hits.len() {
                state.terminal_picker_selected =
                    state.terminal_picker_hits.len().saturating_sub(1);
            }
        }
        Err(err) => {
            state.terminal_picker_hits.clear();
            super::push_obs(state, format!("⚠ shell list failed: {err}"));
        }
    }
}

pub(crate) async fn handle_terminal_picker_key(
    key: KeyEvent,
    state: &mut TuiState,
) -> EventOutcome {
    match key.code {
        KeyCode::Esc => {
            super::workspace_runtime::sync_mode_from_active_tab(state);
            if matches!(state.mode, UiMode::TerminalPicker) {
                state.mode = UiMode::Chat;
            }
            EventOutcome::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.terminal_picker_selected = state.terminal_picker_selected.saturating_sub(1);
            EventOutcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !state.terminal_picker_hits.is_empty() {
                state.terminal_picker_selected = (state.terminal_picker_selected + 1)
                    .min(state.terminal_picker_hits.len() - 1);
            }
            EventOutcome::Continue
        }
        KeyCode::Enter => {
            if let Some(hit) = state
                .terminal_picker_hits
                .get(state.terminal_picker_selected)
                .cloned()
            {
                let _ = attach_existing_terminal(
                    state,
                    &hit.session_id,
                    hit.work_id.as_deref(),
                )
                .await;
            } else {
                let _ = open_new_terminal(state).await;
            }
            EventOutcome::Continue
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let _ = open_new_terminal(state).await;
            EventOutcome::Continue
        }
        KeyCode::Backspace => {
            state.terminal_picker_query.pop();
            refresh_terminal_picker(state).await;
            EventOutcome::Continue
        }
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            state.terminal_picker_query.push(c);
            refresh_terminal_picker(state).await;
            EventOutcome::Continue
        }
        _ => EventOutcome::Continue,
    }
}

fn encode_key(key: KeyEvent) -> Option<Vec<u8>> {
    match key.code {
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let lower = c.to_ascii_lowercase();
            if lower.is_ascii_lowercase() {
                Some(vec![(lower as u8) - b'a' + 1])
            } else if c == ' ' {
                Some(vec![0])
            } else {
                None
            }
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            Some(c.encode_utf8(&mut buf).as_bytes().to_vec())
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        _ => None,
    }
}

pub(crate) async fn handle_terminal_key(key: KeyEvent, state: &mut TuiState) -> EventOutcome {
    // Ctrl+Q still quits the TUI; Ctrl+C goes to the PTY.
    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return EventOutcome::Break;
    }

    let Some(session_id) = state
        .workspace
        .active_tab()
        .and_then(|t| t.terminal_session_id().map(str::to_string))
    else {
        state.mode = UiMode::Chat;
        return EventOutcome::Continue;
    };

    if key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && key.modifiers.contains(KeyModifiers::SHIFT)
    {
        // Ctrl+Shift+C → interrupt via HTTP signal (extra escape hatch).
        let _ = shell_post::<serde_json::Value>(
            &state.daemon_url,
            &format!("/v1/sessions/shell/{session_id}/signal"),
            json!({ "signal": "interrupt" }),
        )
        .await;
        return EventOutcome::Continue;
    }

    // Local scrollback pager — do not forward PageUp/PageDown/Esc-while-scrolled to PTY.
    if matches!(
        key.code,
        KeyCode::PageUp | KeyCode::PageDown | KeyCode::Esc
    ) {
        let Some(pane) = state.terminal_panes.get_mut(&session_id) else {
            return EventOutcome::Continue;
        };
        let sb_len = pane
            .grid
            .lock()
            .map(|g| g.scrollback_len())
            .unwrap_or(0) as u16;
        let page = pane.rows.max(1);
        match key.code {
            KeyCode::PageUp => {
                pane.view_offset = pane.view_offset.saturating_add(page).min(sb_len);
                return EventOutcome::Continue;
            }
            KeyCode::PageDown => {
                pane.view_offset = pane.view_offset.saturating_sub(page);
                return EventOutcome::Continue;
            }
            KeyCode::Esc if pane.view_offset > 0 => {
                pane.view_offset = 0;
                return EventOutcome::Continue;
            }
            _ => {}
        }
    }

    // Typing while scrolled jumps back to live and forwards the key.
    if let Some(pane) = state.terminal_panes.get_mut(&session_id) {
        pane.view_offset = 0;
    }

    let Some(bytes) = encode_key(key) else {
        return EventOutcome::Continue;
    };
    if let Some(pane) = state.terminal_panes.get(&session_id)
        && let Some(tx) = &pane.stdin_tx
    {
        let _ = tx.send(OutboundFrame::Stdin(bytes));
    }
    EventOutcome::Continue
}
