//! Native Muse Session Protocol (MSP v1) over `muse serve` stdio.
//!
//! Protocol reference: https://meta-models.github.io/muse-code-sdk/next/guides/msp-wire/
//! This adapter keeps Muse's credential/configuration stores and sandbox policy.

use std::collections::{HashMap, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, Notify, oneshot};

use crate::{AcpAgentConfig, AcpEvent, AcpPermissionOption, AcpSessionId};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const FRAME_LIMIT: usize = 10 * 1024 * 1024;
const STDERR_LIMIT: usize = 8192;

type Reply = oneshot::Sender<std::result::Result<Value, String>>;

#[derive(Default)]
struct View {
    session_id: String,
    active_turn: Option<String>,
    items: HashMap<String, Value>,
    approvals: HashMap<String, Value>,
    events: VecDeque<AcpEvent>,
    models: Vec<Value>,
    model: Option<String>,
    effort: Option<String>,
    durable: bool,
    terminal_delivered: bool,
    ready: bool,
    early: VecDeque<(String, Value)>,
}

struct Session {
    input: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    pending: Mutex<HashMap<u64, Reply>>,
    view: Mutex<View>,
    changed: Notify,
    closed: AtomicBool,
    next_request: AtomicU64,
    stderr: Mutex<VecDeque<u8>>,
    cwd: String,
}

impl Session {
    async fn write(&self, value: &Value) -> Result<()> {
        let mut input = self.input.lock().await;
        let input = input.as_mut().context("Muse Code connection is closed")?;
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        tokio::time::timeout(REQUEST_TIMEOUT, async {
            input.write_all(&bytes).await?;
            input.flush().await
        })
        .await
        .context("Muse Code write timed out")??;
        Ok(())
    }

    async fn request(&self, method: &str, mut params: Value, command: bool) -> Result<Value> {
        if self.closed.load(Ordering::Acquire) {
            bail!("Muse Code connection is closed");
        }
        if command && params.get("commandId").is_none() {
            params["commandId"] = json!(uuid::Uuid::now_v7().to_string());
        }
        let command_id = params.get("commandId").cloned();
        let id = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (send, recv) = oneshot::channel();
        self.pending.lock().await.insert(id, send);
        if let Err(error) = self
            .write(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await
        {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        let result = match tokio::time::timeout(REQUEST_TIMEOUT, recv).await {
            Ok(Ok(Ok(result))) => result,
            Ok(Ok(Err(error))) => bail!("Muse {method}: {error}"),
            Ok(Err(_)) => bail!("Muse {method}: connection closed before reply"),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                bail!("Muse {method} timed out");
            }
        };
        if let (Some(expected), Some(actual)) = (command_id, result.get("commandId"))
            && expected != *actual
        {
            bail!("Muse {method}: response commandId mismatch");
        }
        Ok(result)
    }

    async fn command(&self, method: &str, mut params: Value) -> Result<Value> {
        params["sessionId"] = json!(self.view.lock().await.session_id);
        self.request(method, params, true).await
    }

    async fn fail(&self, message: String) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        for (_, reply) in self.pending.lock().await.drain() {
            let _ = reply.send(Err(message.clone()));
        }
        let mut view = self.view.lock().await;
        if !view.terminal_delivered {
            view.events.push_back(AcpEvent::Error { message });
            view.events.push_back(AcpEvent::Done);
            view.terminal_delivered = true;
        }
        view.active_turn = None;
        self.changed.notify_waiters();
    }

    async fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        for (_, reply) in self.pending.lock().await.drain() {
            let _ = reply.send(Err("Muse Code connection closed".into()));
        }
        self.input.lock().await.take();
        let mut child = self.child.lock().await;
        if tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .is_err()
        {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
    }

    async fn receive(&self, value: Value) -> Result<()> {
        if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            bail!("Muse Code sent an invalid JSON-RPC envelope");
        }
        if let Some(method) = value.get("method").and_then(Value::as_str) {
            let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
            if let Some(id) = value.get("id") {
                if method == "approval/request" {
                    self.notification("approval/requested", params).await?;
                    self.write(&json!({"jsonrpc":"2.0","id":id,"result":{}}))
                        .await?;
                } else {
                    // User questions cannot safely be reduced to Allow/Deny.
                    self.write(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Medousa cannot present this request type"}})).await?;
                    if method == "userInput/request" {
                        self.fail("Muse Code requested structured user input, which this client does not support yet. Stop the turn and refine the prompt.".into()).await;
                    }
                }
            } else {
                self.notification(method, params).await?;
            }
        } else if let Some(id) = value.get("id").and_then(Value::as_u64) {
            if let Some(reply) = self.pending.lock().await.remove(&id) {
                let result = if let Some(error) = value.get("error") {
                    Err(format!(
                        "{}{}",
                        error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("request failed"),
                        error
                            .pointer("/data/kind")
                            .and_then(Value::as_str)
                            .map(|kind| format!(" ({kind})"))
                            .unwrap_or_default()
                    ))
                } else {
                    value
                        .get("result")
                        .cloned()
                        .ok_or_else(|| "missing result".into())
                };
                let _ = reply.send(result);
            }
        } else {
            bail!("Muse Code sent an uncorrelated response");
        }
        Ok(())
    }

    async fn notification(&self, method: &str, params: Value) -> Result<()> {
        let mut view = self.view.lock().await;
        if let Some(session_id) = params.get("sessionId").and_then(Value::as_str)
            && session_id != view.session_id
        {
            return Ok(());
        }
        if !view.ready {
            if view.early.len() >= 1024 {
                bail!("Muse Code sent too many events before session initialization");
            }
            view.early.push_back((method.into(), params));
            return Ok(());
        }
        if method == "view/gap" || method == "view/truncated" {
            bail!("Muse Code delivery gap; resume the session to recover");
        }
        if method == "userInput/requested" {
            bail!(
                "Muse Code requested structured user input, which this client does not support yet. Stop the turn and refine the prompt."
            );
        }
        if method == "approval/requested" || method == "approval/updated" {
            let approval_id = params
                .get("approvalId")
                .and_then(Value::as_str)
                .context("Muse approval missing approvalId")?;
            let requirement = params
                .get("currentRequirementId")
                .context("Muse approval missing currentRequirementId")?;
            let local_id = format!(
                "{approval_id}:{}",
                requirement
                    .get("sourceIndex")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
            );
            let options = approval_options(&params);
            if options.is_empty() {
                bail!("Muse Code approval has no supported choices");
            }
            let duplicate = view.approvals.get(&local_id) == Some(&params);
            view.approvals
                .retain(|_, old| old.get("approvalId") != params.get("approvalId"));
            view.approvals.insert(local_id.clone(), params.clone());
            if !duplicate {
                let subject = params.get("subject").cloned().unwrap_or(Value::Null);
                let summary = format!(
                    "{}: {}",
                    params
                        .get("toolName")
                        .and_then(Value::as_str)
                        .unwrap_or("Muse Code permission"),
                    subject
                        .get("command")
                        .or_else(|| subject.get("path"))
                        .or_else(|| subject.get("host"))
                        .and_then(Value::as_str)
                        .unwrap_or("Allow this action?")
                );
                view.events.push_back(AcpEvent::PermissionRequest {
                    id: local_id,
                    summary,
                    options,
                });
            }
        } else if method == "approval/resolved" {
            view.approvals
                .retain(|_, old| old.get("approvalId") != params.get("approvalId"));
        } else if method.starts_with("item/") {
            if let Some(turn) = params.get("turnId").and_then(Value::as_str)
                && view.active_turn.as_deref() != Some(turn)
            {
                return Ok(());
            }
            if method == "item/delta" {
                let id = params.get("itemId").and_then(Value::as_str).unwrap_or("");
                let delta = params.get("delta").and_then(Value::as_str).unwrap_or("");
                let field = params
                    .get("field")
                    .and_then(Value::as_str)
                    .unwrap_or("text");
                if let Some(item) = view.items.get_mut(id) {
                    let kind = item
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let field = if field == "output" {
                        "visibleOutput"
                    } else {
                        field
                    };
                    let mut text = item
                        .get(field)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    text.push_str(delta);
                    item[field] = json!(text);
                    if field == "text" && kind == "agentMessage" {
                        view.events
                            .push_back(AcpEvent::MessageDelta { text: delta.into() });
                    } else if kind == "reasoning" {
                        view.events
                            .push_back(AcpEvent::ReasoningDelta { text: delta.into() });
                    } else if kind == "toolCall" {
                        let event = tool_event(item);
                        view.events.push_back(event);
                    }
                }
            } else if let Some(item) = params.get("item") {
                if let Some(turn) = item.get("turnId").and_then(Value::as_str)
                    && view.active_turn.as_deref() != Some(turn)
                {
                    return Ok(());
                }
                let id = item
                    .get("itemId")
                    .and_then(Value::as_str)
                    .context("Muse item missing itemId")?;
                if let Some(old) = view.items.get(id)
                    && old.get("revision").and_then(Value::as_i64).unwrap_or(0)
                        >= item.get("revision").and_then(Value::as_i64).unwrap_or(0)
                {
                    return Ok(());
                }
                let previous = view.items.insert(id.into(), item.clone());
                match item.get("kind").and_then(Value::as_str) {
                    Some("agentMessage") => {
                        let text = item.get("text").and_then(Value::as_str).unwrap_or("");
                        if item.get("status").and_then(Value::as_str) == Some("completed") {
                            view.events
                                .push_back(AcpEvent::MessageDone { text: text.into() });
                        } else {
                            let before = previous
                                .as_ref()
                                .and_then(|item| item.get("text"))
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            if let Some(delta) = text.strip_prefix(before)
                                && !delta.is_empty()
                            {
                                view.events
                                    .push_back(AcpEvent::MessageDelta { text: delta.into() });
                            }
                        }
                    }
                    Some("reasoning") => {
                        let text = item
                            .get("text")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .unwrap_or_else(|| {
                                item.get("summary")
                                    .and_then(Value::as_array)
                                    .map(|parts| {
                                        parts
                                            .iter()
                                            .filter_map(Value::as_str)
                                            .collect::<Vec<_>>()
                                            .join("\n")
                                    })
                                    .unwrap_or_default()
                            });
                        if previous.is_none() && !text.is_empty() {
                            view.events.push_back(AcpEvent::ReasoningDelta { text });
                        }
                    }
                    Some("toolCall" | "userShell" | "subagent" | "workflow") => {
                        view.events.push_back(tool_event(item))
                    }
                    _ => {}
                }
            }
        } else if method == "turn/completed" {
            if params.get("turnId").and_then(Value::as_str) != view.active_turn.as_deref() {
                return Ok(());
            }
            let terminal = params
                .get("terminal")
                .and_then(Value::as_str)
                .unwrap_or("failed");
            if terminal != "completed" && terminal != "cancelled" {
                let message = params
                    .pointer("/error/message")
                    .or_else(|| params.get("reason"))
                    .and_then(Value::as_str)
                    .unwrap_or("Muse Code turn failed");
                view.events.push_back(AcpEvent::Error {
                    message: message.into(),
                });
            }
            view.active_turn = None;
            view.approvals.clear();
            view.terminal_delivered = true;
            view.events.push_back(AcpEvent::Done);
        }
        self.changed.notify_waiters();
        Ok(())
    }
}

fn tool_event(item: &Value) -> AcpEvent {
    // Preserve MSP status/output alongside arguments for the shared tool renderer.
    let mut input = item.clone();
    input["status"] = item.get("status").cloned().unwrap_or(json!("inProgress"));
    input["output"] = item.get("visibleOutput").cloned().unwrap_or(Value::Null);
    input["arguments"] = item
        .get("args")
        .and_then(Value::as_str)
        .map(|args| serde_json::from_str(args).unwrap_or(json!(args)))
        .unwrap_or(Value::Null);
    AcpEvent::ToolCall {
        id: item
            .get("itemId")
            .and_then(Value::as_str)
            .unwrap_or("muse-tool")
            .into(),
        name: item
            .get("tool")
            .or_else(|| item.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("Muse Code tool")
            .into(),
        input,
    }
}

fn approval_options(approval: &Value) -> Vec<AcpPermissionOption> {
    approval
        .get("availableChoices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|choice| {
            let decision = choice.get("decision")?.as_str()?;
            let scope = choice
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("once");
            let kind = match decision {
                "approved" if scope == "once" => "allow_once",
                "approvedForSession" | "approvedPolicyAmendment"
                    if scope == "session" || scope == "localPersistent" =>
                {
                    "allow_always"
                }
                "denied" | "deniedPolicyAmendment" | "abort" => "reject_once",
                _ => return None,
            };
            Some(AcpPermissionOption {
                option_id: choice.get("choiceId")?.as_str()?.into(),
                name: choice.get("label")?.as_str()?.into(),
                kind: kind.into(),
            })
        })
        .collect()
}

/// Independent process/reader per session; a paused turn cannot lock other sessions.
#[derive(Default)]
pub struct MuseClient {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl MuseClient {
    pub fn new() -> Self {
        Self::default()
    }

    async fn session(&self, id: &AcpSessionId) -> Result<Arc<Session>> {
        self.sessions
            .lock()
            .await
            .get(&id.0)
            .cloned()
            .context("unknown Muse Code session")
    }

    pub async fn create_or_resume_session(
        &self,
        config: &AcpAgentConfig,
        resume: Option<&str>,
    ) -> Result<(AcpSessionId, Option<String>, bool)> {
        let cwd = config
            .cwd
            .clone()
            .unwrap_or(std::env::current_dir()?.display().to_string());
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .current_dir(&cwd)
            .env("MUSE_NO_AUTO_UPDATE", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        // A saved Muse login must not accidentally be replaced by an unrelated API key.
        command.env_remove("META_API_KEY");
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command
            .spawn()
            .context("could not launch Muse Code (muse serve)")?;
        let input = child.stdin.take().context("Muse stdin unavailable")?;
        let stdout = child.stdout.take().context("Muse stdout unavailable")?;
        let mut stderr = child.stderr.take().context("Muse stderr unavailable")?;
        let session = Arc::new(Session {
            input: Mutex::new(Some(input)),
            child: Mutex::new(child),
            pending: Mutex::new(HashMap::new()),
            view: Mutex::new(View::default()),
            changed: Notify::new(),
            closed: AtomicBool::new(false),
            next_request: AtomicU64::new(1),
            stderr: Mutex::new(VecDeque::new()),
            cwd,
        });
        let weak = Arc::downgrade(&session);
        tokio::spawn(async move {
            let mut buffer = [0u8; 1024];
            while let Ok(count) = stderr.read(&mut buffer).await {
                if count == 0 {
                    break;
                }
                let Some(session) = weak.upgrade() else {
                    break;
                };
                let mut tail = session.stderr.lock().await;
                tail.extend(&buffer[..count]);
                while tail.len() > STDERR_LIMIT {
                    tail.pop_front();
                }
            }
        });
        let weak = Arc::downgrade(&session);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                // `take` prevents an untrusted unterminated line allocating without bound.
                let mut frame = Vec::new();
                let read = (&mut reader)
                    .take((FRAME_LIMIT + 1) as u64)
                    .read_until(b'\n', &mut frame)
                    .await;
                let Some(session) = weak.upgrade() else {
                    break;
                };
                let error = match read {
                    Ok(0) => Some("Muse Code process exited or closed stdout".to_owned()),
                    Ok(_) if frame.len() > FRAME_LIMIT => {
                        Some("Muse Code frame exceeded 10 MiB".to_owned())
                    }
                    Ok(_) => match serde_json::from_slice(&frame) {
                        Ok(value) => session
                            .receive(value)
                            .await
                            .err()
                            .map(|error| error.to_string()),
                        Err(_) => Some("Muse Code sent malformed JSON".to_owned()),
                    },
                    Err(error) => Some(format!("Muse Code stdout failed: {error}")),
                };
                if let Some(error) = error {
                    session.fail(error).await;
                    session.shutdown().await;
                    break;
                }
                if session.closed.load(Ordering::Acquire) {
                    session.shutdown().await;
                    break;
                }
            }
        });
        let startup = async {
            let initialized = session.request("initialize", json!({"clientInfo":{"name":"medousa","title":"Medousa","version":env!("CARGO_PKG_VERSION")}}), false).await?;
            if initialized.pointer("/schema/version").and_then(Value::as_u64) != Some(1) { bail!("Muse Code returned an unsupported MSP envelope version"); }
            if initialized.pointer("/schema/fingerprint").and_then(Value::as_str).is_none() { bail!("Muse Code initialize missing schema fingerprint"); }
            session.view.lock().await.durable = initialized.get("sessionDurability").and_then(Value::as_str) != Some("ephemeral");
            session.write(&json!({"jsonrpc":"2.0","method":"initialized"})).await?;
            let resume = resume.filter(|id| !id.trim().is_empty());
            let requested = resume.map(str::to_owned).unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
            session.view.lock().await.session_id = requested.clone();
            let params = if resume.is_some() { json!({"sessionId":requested,"excludeItems":true,"permissionProfile":":ask-me"}) } else { json!({"sessionId":requested,"workspaceRoot":session.cwd,"approvalMode":"promptUnmatched","permissionProfile":":ask-me"}) };
            let result = session.request(if resume.is_some() { "session/resume" } else { "session/start" }, params, true).await?;
            let wire = result.pointer("/session/sessionId").and_then(Value::as_str).context("Muse Code session reply missing sessionId")?;
            if wire != requested { bail!("Muse Code returned an unexpected session identity"); }
            let early = { let mut view = session.view.lock().await; view.model = result.pointer("/session/modelId").and_then(Value::as_str).map(str::to_owned); view.active_turn = result.pointer("/session/activeTurnId").and_then(Value::as_str).map(str::to_owned); view.ready = true; std::mem::take(&mut view.early) };
            for (method, params) in early { session.notification(&method, params).await?; }
            if resume.is_some() { session.command("session/setApprovalMode", json!({"mode":"promptUnmatched"})).await?; }
            let catalog = session.request("model/list", json!({"sessionId":wire}), false).await?;
            session.view.lock().await.models = catalog.get("models").and_then(Value::as_array).cloned().context("Muse model/list missing models")?;
            Ok::<_, anyhow::Error>((wire.to_owned(), resume.is_some()))
        }.await;
        match startup {
            Ok((wire, resumed)) => {
                let id = AcpSessionId(format!("muse-code-{}", uuid::Uuid::now_v7()));
                self.sessions.lock().await.insert(id.0.clone(), session);
                Ok((id, Some(wire), resumed))
            }
            Err(error) => {
                session.shutdown().await;
                Err(error)
            }
        }
    }

    pub async fn wire_session_id(&self, id: &AcpSessionId) -> Option<String> {
        Some(
            self.session(id)
                .await
                .ok()?
                .view
                .lock()
                .await
                .session_id
                .clone(),
        )
    }

    pub async fn session_capabilities(
        &self,
        id: &AcpSessionId,
    ) -> medousa_types::AgentRuntimeCapabilities {
        let resume = match self.session(id).await {
            Ok(session) => session.view.lock().await.durable,
            Err(_) => false,
        };
        medousa_types::AgentRuntimeCapabilities {
            resume,
            ..Default::default()
        }
    }

    pub async fn prompt(&self, id: &AcpSessionId, text: &str) -> Result<()> {
        let session = self.session(id).await?;
        let turn_id = uuid::Uuid::now_v7().to_string();
        let effort = {
            let mut view = session.view.lock().await;
            if view.active_turn.is_some() {
                bail!("Muse Code already has an active turn");
            }
            // The daemon stops consuming on Error. Its paired terminal sentinel
            // belongs to the old turn and must never finish this new prompt.
            view.events
                .retain(|event| !matches!(event, AcpEvent::Done | AcpEvent::Error { .. }));
            view.active_turn = Some(turn_id.clone());
            view.items.clear();
            view.terminal_delivered = false;
            view.effort.clone()
        };
        let mut params = json!({"commandId":turn_id,"input":[{"type":"text","text":text}],"displayText":text,"workspaceRoots":[session.cwd],"ifBusy":"queue"});
        if let Some(effort) = effort {
            params["reasoningEffort"] = json!(effort);
        }
        match session.command("turn/start", params).await {
            Ok(result) if result.get("turnId").and_then(Value::as_str) == Some(&turn_id) => Ok(()),
            Ok(_) => {
                session
                    .fail("Muse Code admitted an unexpected turn identity".into())
                    .await;
                session.shutdown().await;
                bail!("Muse Code admitted an unexpected turn identity")
            }
            Err(error) => {
                // An unacknowledged admission may already be running. Close its
                // owner rather than risk launching a second concurrent turn.
                session
                    .fail(format!("Muse Code prompt admission failed: {error}"))
                    .await;
                session.shutdown().await;
                Err(error)
            }
        }
    }

    pub async fn cancel(&self, id: &AcpSessionId) -> Result<()> {
        let session = self
            .sessions
            .lock()
            .await
            .remove(&id.0)
            .context("unknown Muse Code session")?;
        let active = session.view.lock().await.active_turn.clone();
        if let Some(turn_id) = active {
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                session.command("turn/interrupt", json!({"turnId":turn_id})),
            )
            .await;
            session.shutdown().await;
            result.context("Muse Code interrupt timed out; process was stopped")??;
        } else {
            session.shutdown().await;
        }
        Ok(())
    }

    pub async fn release(&self, id: &AcpSessionId) -> Result<()> {
        let session = self.sessions.lock().await.remove(&id.0);
        if let Some(session) = session {
            session.shutdown().await;
        }
        Ok(())
    }

    pub async fn next_event(&self, id: &AcpSessionId) -> Result<Option<AcpEvent>> {
        let session = self.session(id).await?;
        let notified = session.changed.notified();
        if let Some(event) = session.view.lock().await.events.pop_front() {
            return Ok(Some(event));
        }
        let _ = tokio::time::timeout(Duration::from_millis(200), notified).await;
        let event = session.view.lock().await.events.pop_front();
        Ok(event)
    }

    pub async fn respond_permission(
        &self,
        id: &AcpSessionId,
        request_id: &str,
        approved: bool,
    ) -> Result<()> {
        let session = self.session(id).await?;
        let approval = session
            .view
            .lock()
            .await
            .approvals
            .get(request_id)
            .cloned()
            .context("Muse Code approval expired or changed")?;
        let kind = if approved {
            "allow_once"
        } else {
            "reject_once"
        };
        let choice = approval_options(&approval)
            .into_iter()
            .find(|option| option.kind == kind)
            .context("Muse Code did not offer this approval decision")?;
        self.respond_permission_choice(id, request_id, Some(&choice.option_id))
            .await
    }

    pub async fn respond_permission_choice(
        &self,
        id: &AcpSessionId,
        request_id: &str,
        option_id: Option<&str>,
    ) -> Result<()> {
        let session = self.session(id).await?;
        let approval = session
            .view
            .lock()
            .await
            .approvals
            .get(request_id)
            .cloned()
            .context("Muse Code approval expired or changed")?;
        let choices = approval_options(&approval);
        let option_id = option_id
            .or_else(|| {
                choices
                    .iter()
                    .find(|option| option.kind == "reject_once")
                    .map(|option| option.option_id.as_str())
            })
            .context("Muse Code did not offer a rejection choice")?;
        if !choices.iter().any(|choice| choice.option_id == option_id) {
            bail!("Muse Code did not offer this approval choice");
        }
        session.command("approval/decide", json!({"approvalId":approval["approvalId"],"requirementId":approval["currentRequirementId"],"choiceId":option_id})).await?;
        Ok(())
    }

    pub async fn session_config_options(&self, id: &AcpSessionId) -> Vec<Value> {
        match self.session(id).await {
            Ok(session) => config_options(&*session.view.lock().await),
            Err(_) => Vec::new(),
        }
    }

    pub async fn set_session_config_option(
        &self,
        id: &AcpSessionId,
        config_id: &str,
        value: Value,
    ) -> Result<Vec<Value>> {
        let session = self.session(id).await?;
        let choices = config_options(&*session.view.lock().await);
        let option = choices
            .iter()
            .find(|option| option["id"] == config_id)
            .context("unsupported Muse Code setting")?;
        if !option["options"]
            .as_array()
            .is_some_and(|choices| choices.iter().any(|choice| choice["value"] == value))
        {
            bail!("Muse Code did not advertise this setting value");
        }
        if config_id == "model" {
            let provider = {
                let view = session.view.lock().await;
                view.models
                    .iter()
                    .find(|model| model["modelId"] == value)
                    .and_then(|model| model.get("providerId"))
                    .cloned()
                    .unwrap_or(json!("meta"))
            };
            session
                .command(
                    "session/setModel",
                    json!({"model":{"modelId":value,"providerId":provider}}),
                )
                .await?;
            let mut view = session.view.lock().await;
            view.model = value.as_str().map(str::to_owned);
            view.effort = None;
        } else if config_id == "reasoning_effort" {
            session.view.lock().await.effort = value.as_str().map(str::to_owned);
        }
        Ok(config_options(&*session.view.lock().await))
    }
}

fn config_options(view: &View) -> Vec<Value> {
    let mut options = Vec::new();
    if !view.models.is_empty() {
        options.push(json!({"id":"model","name":"Model","category":"model","type":"select","currentValue":view.model,"options":view.models.iter().filter_map(|model| Some(json!({"value":model.get("modelId")?.as_str()?,"name":model.get("displayLabel").and_then(Value::as_str).unwrap_or("Muse Code model")}))).collect::<Vec<_>>()}));
    }
    if let Some(model) = view
        .models
        .iter()
        .find(|model| model.get("modelId").and_then(Value::as_str) == view.model.as_deref())
        && let Some(efforts) = model.get("variants").and_then(Value::as_array)
        && !efforts.is_empty()
    {
        options.push(json!({"id":"reasoning_effort","name":"Reasoning","category":"thought_level","type":"select","currentValue":view.effort.as_deref().or_else(||model.get("defaultReasoningEffort").and_then(Value::as_str)),"options":efforts.iter().filter_map(Value::as_str).map(|effort| json!({"value":effort,"name":effort})).collect::<Vec<_>>()}));
    }
    options
}

#[cfg(test)]
mod tests;
