//! Home-side Terminal: VT parse on a dedicated thread + workshop WS bridge.
//!
//! Each Terminal tab owns a parser/grid on its own std::thread. WS bytes from
//! the workshop session host are fed in via `terminal_feed`; snapshot rows are
//! pushed back to the webview as `terminal-frame` events. Key input is encoded
//! and sent to the workshop as session stdin frames.
//!
//! libghostty-vt is the target parser (its sys crate builds the Ghostty VT
//! archive via Zig); until a Zig toolchain is available, `vte` drives the same
//! dedicated-thread cell-grid model so the UI/WS plumbing is identical.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tungstenite::{Message, stream::MaybeTlsStream, WebSocket};
use vte::{Params, Perform};

use crate::daemon::sdk::client;
use crate::daemon::DaemonState;

const GRID_COLS: usize = 120;
const GRID_ROWS: usize = 40;

static NEXT_ATTACH_ID: AtomicU64 = AtomicU64::new(1);

pub type TerminalRegistry = Arc<Mutex<HashMap<u64, Arc<TerminalHandle>>>>;

pub struct TerminalHandle {
    pub attach_id: u64,
    pub input: std::sync::mpsc::Sender<TerminalCmd>,
    /// Shared render buffer so the UI can poll instead of relying only on events.
    pub lines: Arc<Mutex<Vec<String>>>,
    /// Stdin writer for the workshop session (blocking ws writer thread).
    pub stdin: std::sync::mpsc::Sender<Vec<u8>>,
    /// Session id this attach is bound to (for resize/signal via daemon HTTP).
    pub session_id: String,
}

enum TerminalCmd {
    Feed(Vec<u8>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalSessionSummary {
    pub session_id: String,
    pub cwd: String,
    pub root_kind: String,
    pub work_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub available: bool,
    pub url: String,
    pub daemon_base_path: String,
    pub workspace_root: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct TerminalAttachResponse {
    pub attach_id: u64,
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalCreateInput {
    pub work_id: Option<String>,
    pub cwd: Option<String>,
    pub lease_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Minimal VT grid (vte Perform → cell grid snapshot)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct TerminalCell {
    g: String,
    bold: bool,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            g: " ".into(),
            bold: false,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
struct TerminalFrame {
    attach_id: u64,
    rows: Vec<Vec<TerminalCell>>,
    cursor_row: usize,
    cursor_col: usize,
}

struct Grid {
    cells: Vec<Vec<TerminalCell>>,
    cursor_row: usize,
    cursor_col: usize,
    bold: bool,
    saved: Option<(usize, usize)>,
}

impl Grid {
    fn new() -> Self {
        Self {
            cells: vec![vec![TerminalCell::default(); GRID_COLS]; GRID_ROWS],
            cursor_row: 0,
            cursor_col: 0,
            bold: false,
            saved: None,
        }
    }

    fn put(&mut self, ch: char) {
        if self.cursor_row < GRID_ROWS && self.cursor_col < GRID_COLS {
            self.cells[self.cursor_row][self.cursor_col] = TerminalCell {
                g: ch.to_string(),
                bold: self.bold,
            };
        }
        self.cursor_col += 1;
        if self.cursor_col >= GRID_COLS {
            self.cursor_col = 0;
            self.newline();
        }
    }

    fn newline(&mut self) {
        if self.cursor_row + 1 < GRID_ROWS {
            self.cursor_row += 1;
        } else {
            self.cells.remove(0);
            self.cells.push(vec![TerminalCell::default(); GRID_COLS]);
        }
    }

    fn clear_from_cursor(&mut self) {
        if self.cursor_row < GRID_ROWS {
            for c in self.cursor_col..GRID_COLS {
                self.cells[self.cursor_row][c] = TerminalCell::default();
            }
            for r in (self.cursor_row + 1)..GRID_ROWS {
                self.cells[r] = vec![TerminalCell::default(); GRID_COLS];
            }
        }
    }

    fn snapshot(&self, attach_id: u64) -> TerminalFrame {
        TerminalFrame {
            attach_id,
            rows: self.cells.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        }
    }
}

impl Perform for Grid {
    fn print(&mut self, c: char) {
        self.put(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => self.cursor_col = 0,
            0x08 => {
                self.cursor_col = self.cursor_col.saturating_sub(1);
            }
            b'\t' => {
                self.cursor_col = (self.cursor_col + 8) & !7;
                if self.cursor_col >= GRID_COLS {
                    self.cursor_col = GRID_COLS - 1;
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        let param = |i: usize, default: usize| -> usize {
            params
                .iter()
                .nth(i)
                .and_then(|p| p.first().copied())
                .map(|v| v as usize)
                .filter(|v| *v > 0)
                .unwrap_or(default)
        };
        match action {
            'A' => self.cursor_row = self.cursor_row.saturating_sub(param(0, 1)),
            'B' => self.cursor_row = (self.cursor_row + param(0, 1)).min(GRID_ROWS - 1),
            'C' => self.cursor_col = (self.cursor_col + param(0, 1)).min(GRID_COLS - 1),
            'D' => self.cursor_col = self.cursor_col.saturating_sub(param(0, 1)),
            'E' => {
                self.cursor_row = (self.cursor_row + param(0, 1)).min(GRID_ROWS - 1);
                self.cursor_col = 0;
            }
            'F' => {
                self.cursor_row = self.cursor_row.saturating_sub(param(0, 1));
                self.cursor_col = 0;
            }
            'G' => self.cursor_col = param(0, 1).saturating_sub(1).min(GRID_COLS - 1),
            'H' | 'f' => {
                self.cursor_row = param(0, 1).saturating_sub(1).min(GRID_ROWS - 1);
                self.cursor_col = param(1, 1).saturating_sub(1).min(GRID_COLS - 1);
            }
            'J' => self.clear_from_cursor(),
            'K' => {
                if self.cursor_row < GRID_ROWS {
                    for c in self.cursor_col..GRID_COLS {
                        self.cells[self.cursor_row][c] = TerminalCell::default();
                    }
                }
            }
            'm' => {
                self.bold = params
                    .iter()
                    .any(|p| p.first().copied() == Some(1));
                if params.iter().any(|p| p.first().copied() == Some(0))
                    || params.is_empty()
                {
                    self.bold = false;
                }
            }
            's' => self.saved = Some((self.cursor_row, self.cursor_col)),
            'u' => {
                if let Some((r, c)) = self.saved {
                    self.cursor_row = r.min(GRID_ROWS - 1);
                    self.cursor_col = c.min(GRID_COLS - 1);
                }
            }
            _ => {}
        }
    }
}

fn flatten_rows(frame: &TerminalFrame) -> Vec<String> {
    frame
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|c| c.g.as_str())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

fn spawn_vt_thread(
    attach_id: u64,
    app: AppHandle,
    lines: Arc<Mutex<Vec<String>>>,
    rx: std::sync::mpsc::Receiver<TerminalCmd>,
) {
    std::thread::spawn(move || {
        let mut parser = vte::Parser::new();
        let mut grid = Grid::new();
        while let Ok(cmd) = rx.recv() {
            match cmd {
                TerminalCmd::Feed(bytes) => {
                    parser.advance(&mut grid, &bytes);
                    let frame = grid.snapshot(attach_id);
                    if let Ok(mut guard) = lines.lock() {
                        *guard = flatten_rows(&frame);
                    }
                    let _ = app.emit("terminal-frame", frame);
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Workshop WS bridge
// ---------------------------------------------------------------------------

fn ws_url_for(daemon_url: &str, path: &str) -> String {
    let base = daemon_url.trim_end_matches('/').replacen("http", "ws", 1);
    format!("{base}{path}")
}

fn ws_connect(
    daemon_url: &str,
    session_id: &str,
) -> Result<WebSocket<MaybeTlsStream<std::net::TcpStream>>, String> {
    let url = ws_url_for(
        daemon_url,
        &format!("/v1/sessions/shell/{}", urlencoding::encode(session_id)),
    );
    let (ws, _) = tungstenite::connect(&url).map_err(|e| e.to_string())?;
    Ok(ws)
}

async fn daemon_get<T: serde::de::DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    client(state).http().get(path).await.map_err(|e| e.to_string())
}

async fn daemon_post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    state: &State<'_, DaemonState>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    client(state)
        .http()
        .post(path, body)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_info(state: State<'_, DaemonState>) -> Result<TerminalInfo, String> {
    daemon_get(&state, "/v1/shell-sessions").await
}

#[tauri::command]
pub async fn terminal_sessions(
    state: State<'_, DaemonState>,
) -> Result<Vec<TerminalSessionSummary>, String> {
    let value: serde_json::Value = daemon_get(&state, "/v1/sessions/shell").await?;
    Ok(value
        .get("sessions")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn terminal_interrupt(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    daemon_post(
        &state,
        &format!("/v1/sessions/shell/{session_id}/signal"),
        &serde_json::json!({ "signal": "interrupt" }),
    )
    .await
}

#[tauri::command]
pub async fn terminal_create(
    state: State<'_, DaemonState>,
    input: TerminalCreateInput,
) -> Result<serde_json::Value, String> {
    daemon_post(
        &state,
        "/v1/sessions/shell",
        &serde_json::json!({
            "work_id": input.work_id,
            "cwd": input.cwd,
            "lease_id": input.lease_id,
        }),
    )
    .await
}

#[tauri::command]
pub async fn terminal_attach(
    app: AppHandle,
    state: State<'_, DaemonState>,
    registry: State<'_, TerminalRegistry>,
    session_id: String,
) -> Result<TerminalAttachResponse, String> {
    let attach_id = NEXT_ATTACH_ID.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = std::sync::mpsc::channel::<TerminalCmd>();
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    spawn_vt_thread(attach_id, app.clone(), Arc::clone(&lines), rx);

    let daemon_url = state
        .daemon_url
        .lock()
        .map_err(|_| "daemon url lock")?
        .clone();
    let sid = session_id.clone();
    let ws = tauri::async_runtime::spawn_blocking(move || ws_connect(&daemon_url, &sid))
        .await
        .map_err(|e| e.to_string())??;

    let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let ws = std::sync::Arc::new(std::sync::Mutex::new(ws));
    let ws_writer = std::sync::Arc::clone(&ws);
    std::thread::spawn(move || {
        while let Ok(bytes) = stdin_rx.recv() {
            let frame = serde_json::json!({
                "type": "stdin",
                "data": base64::engine::general_purpose::STANDARD.encode(&bytes)
            })
            .to_string();
            let Ok(mut guard) = ws_writer.lock() else {
                break;
            };
            if guard.send(Message::Text(frame.into())).is_err() {
                break;
            }
        }
    });

    let feed_tx = tx.clone();
    let ws_reader = std::sync::Arc::clone(&ws);
    std::thread::spawn(move || loop {
        let msg = {
            let Ok(mut guard) = ws_reader.lock() else {
                break;
            };
            guard.read()
        };
        match msg {
            Ok(Message::Text(text)) => {
                let decoded = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .filter(|v| v.get("type").and_then(|t| t.as_str()) == Some("stdout"))
                    .and_then(|v| {
                        let data = v.get("data").and_then(|d| d.as_str())?;
                        base64::engine::general_purpose::STANDARD.decode(data).ok()
                    });
                if let Some(bytes) = decoded {
                    if feed_tx.send(TerminalCmd::Feed(bytes)).is_err() {
                        break;
                    }
                }
            }
            Ok(Message::Binary(bytes)) => {
                if feed_tx.send(TerminalCmd::Feed(bytes.to_vec())).is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    });

    registry
        .lock()
        .map_err(|_| "terminal registry lock")?
        .insert(
            attach_id,
            Arc::new(TerminalHandle {
                attach_id,
                input: tx,
                lines,
                stdin: stdin_tx,
                session_id: session_id.clone(),
            }),
        );

    Ok(TerminalAttachResponse {
        attach_id,
        session_id,
    })
}

#[tauri::command]
pub async fn terminal_feed(
    registry: State<'_, TerminalRegistry>,
    attach_id: u64,
    data: String,
) -> Result<(), String> {
    let handles = registry.lock().map_err(|_| "terminal registry lock")?;
    let Some(handle) = handles.get(&attach_id) else {
        return Err("unknown attach_id".into());
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| e.to_string())?;
    handle
        .input
        .send(TerminalCmd::Feed(bytes))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_key(
    registry: State<'_, TerminalRegistry>,
    attach_id: u64,
    key: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> Result<(), String> {
    let bytes = encode_key(&key, ctrl, alt, shift)
        .ok_or_else(|| format!("unsupported key: {key}"))?;
    send_stdin(&registry, attach_id, &bytes).await
}

#[tauri::command]
pub async fn terminal_resize(
    registry: State<'_, TerminalRegistry>,
    attach_id: u64,
    _cols: u16,
    _rows: u16,
) -> Result<(), String> {
    // v1: fixed grid. Resize plumbing to the workshop lands when
    // medousa-session exposes PTY master resize; keep the command so the UI
    // can call it today without breaking.
    let handles = registry.lock().map_err(|_| "terminal registry lock")?;
    let Some(_handle) = handles.get(&attach_id) else {
        return Err("unknown attach_id".into());
    };
    Ok(())
}

#[tauri::command]
pub async fn terminal_detach(
    registry: State<'_, TerminalRegistry>,
    attach_id: u64,
) -> Result<(), String> {
    registry
        .lock()
        .map_err(|_| "terminal registry lock")?
        .remove(&attach_id);
    Ok(())
}

#[tauri::command]
pub async fn terminal_snapshot(
    registry: State<'_, TerminalRegistry>,
    attach_id: u64,
) -> Result<Vec<String>, String> {
    let lines = {
        let handles = registry.lock().map_err(|_| "terminal registry lock")?;
        handles
            .get(&attach_id)
            .map(|h| Arc::clone(&h.lines))
            .ok_or_else(|| "unknown attach_id".to_string())?
    };
    let guard = lines.lock().map_err(|_| "lines lock")?;
    Ok(guard.clone())
}

async fn send_stdin(
    registry: &State<'_, TerminalRegistry>,
    attach_id: u64,
    bytes: &[u8],
) -> Result<(), String> {
    let handles = registry.lock().map_err(|_| "terminal registry lock")?;
    let Some(handle) = handles.get(&attach_id) else {
        return Err("unknown attach_id".into());
    };
    handle.stdin.send(bytes.to_vec()).map_err(|e| e.to_string())
}

fn encode_key(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<Vec<u8>> {
    if key.len() == 1 {
        let ch = key.chars().next()?;
        let mut bytes = if ctrl {
            if ch.is_ascii_alphabetic() {
                vec![(ch.to_ascii_lowercase() as u8) & 0x1f]
            } else {
                return None;
            }
        } else {
            let mut s = String::new();
            if alt {
                s.push('\x1b');
            }
            s.push(if shift { ch.to_ascii_uppercase() } else { ch });
            s.into_bytes()
        };
        if alt && ctrl {
            bytes.insert(0, 0x1b);
        }
        return Some(bytes);
    }
    match key {
        "enter" => Some(b"\r".to_vec()),
        "backspace" => Some(b"\x7f".to_vec()),
        "tab" => Some(b"\t".to_vec()),
        "escape" => Some(b"\x1b".to_vec()),
        "up" => Some(b"\x1b[A".to_vec()),
        "down" => Some(b"\x1b[B".to_vec()),
        "right" => Some(b"\x1b[C".to_vec()),
        "left" => Some(b"\x1b[D".to_vec()),
        "home" => Some(b"\x1b[H".to_vec()),
        "end" => Some(b"\x1b[F".to_vec()),
        "delete" => Some(b"\x1b[3~".to_vec()),
        "pageup" => Some(b"\x1b[5~".to_vec()),
        "pagedown" => Some(b"\x1b[6~".to_vec()),
        _ => None,
    }
}

