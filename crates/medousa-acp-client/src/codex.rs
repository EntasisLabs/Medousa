//! Native Codex app-server over stdio. Credentials, sandboxing and approvals
//! remain owned by the installed Codex CLI; this module only translates its UI
//! protocol into Medousa's shared external-agent events.
//!
//! Protocol: https://developers.openai.com/codex/app-server

use std::collections::{HashMap, HashSet, VecDeque};
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
const EVENT_LIMIT: usize = 4096;
const EVENT_BYTES_LIMIT: usize = 16 * 1024 * 1024;
type Reply = oneshot::Sender<std::result::Result<Value, String>>;

#[derive(Clone)]
struct Approval {
    wire_id: Value,
    choices: Vec<(AcpPermissionOption, Value)>,
}

#[derive(Default)]
struct View {
    thread_id: String,
    active_turn: Option<String>,
    starting: bool,
    terminal: bool,
    events: VecDeque<AcpEvent>,
    event_bytes: usize,
    approvals: HashMap<String, Approval>,
    streamed: HashSet<String>,
    models: Vec<Value>,
    model: Option<String>,
    effort: Option<String>,
    turn_error: Option<String>,
}

impl View {
    fn push(&mut self, event: AcpEvent) -> Result<()> {
        let size = serde_json::to_vec(&event)?.len();
        if self.events.len() >= EVENT_LIMIT
            || self.event_bytes.saturating_add(size) > EVENT_BYTES_LIMIT
        {
            bail!("Codex app-server event backlog exceeded its limit");
        }
        self.event_bytes += size;
        self.events.push_back(event);
        Ok(())
    }

    fn pop(&mut self) -> Option<AcpEvent> {
        let event = self.events.pop_front()?;
        self.event_bytes = self
            .event_bytes
            .saturating_sub(serde_json::to_vec(&event).map_or(0, |bytes| bytes.len()));
        Some(event)
    }
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
        let input = input
            .as_mut()
            .context("Codex app-server connection is closed")?;
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        tokio::time::timeout(REQUEST_TIMEOUT, async {
            input.write_all(&bytes).await?;
            input.flush().await
        })
        .await
        .context("Codex app-server write timed out")??;
        Ok(())
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        if self.closed.load(Ordering::Acquire) {
            bail!("Codex app-server connection is closed");
        }
        let id = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (send, recv) = oneshot::channel();
        self.pending.lock().await.insert(id, send);
        // Codex app-server deliberately omits the JSON-RPC version field.
        if let Err(error) = self
            .write(&json!({"id":id,"method":method,"params":params}))
            .await
        {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, recv).await {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(error))) => bail!("Codex {method}: {error}"),
            Ok(Err(_)) => bail!("Codex {method}: connection closed before reply"),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                bail!("Codex {method} timed out")
            }
        }
    }

    async fn fail(&self, message: String) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        for (_, reply) in self.pending.lock().await.drain() {
            let _ = reply.send(Err(message.clone()));
        }
        let mut view = self.view.lock().await;
        if !view.terminal {
            // Retain the failure even when a stalled consumer filled the queue.
            if view.events.len() >= EVENT_LIMIT || view.event_bytes >= EVENT_BYTES_LIMIT {
                view.events.clear();
                view.event_bytes = 0;
            }
            view.events.push_back(AcpEvent::Error { message });
            view.terminal = true;
        }
        view.active_turn = None;
        view.starting = false;
        view.approvals.clear();
        self.changed.notify_waiters();
    }

    async fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        for (_, reply) in self.pending.lock().await.drain() {
            let _ = reply.send(Err("Codex app-server connection closed".into()));
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
        if let Some(method) = value.get("method").and_then(Value::as_str) {
            let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
            if let Some(id) = value.get("id") {
                self.server_request(id.clone(), method, params).await?;
            } else {
                self.notification(method, params).await?;
            }
        } else if let Some(id) = value.get("id").and_then(Value::as_u64) {
            if let Some(reply) = self.pending.lock().await.remove(&id) {
                let result = if let Some(error) = value.get("error") {
                    Err(error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("request failed")
                        .to_owned())
                } else {
                    value
                        .get("result")
                        .cloned()
                        .ok_or_else(|| "missing result".into())
                };
                let _ = reply.send(result);
            }
        } else {
            bail!("Codex app-server sent an invalid response");
        }
        Ok(())
    }

    async fn server_request(&self, id: Value, method: &str, params: Value) -> Result<()> {
        if !id.is_string() && !id.is_i64() && !id.is_u64() {
            bail!("Codex server request has an invalid id");
        }
        if matches!(
            method,
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
        ) {
            let mut view = self.view.lock().await;
            // Never grant authority to callbacks from another thread or a retired turn.
            if !belongs_to_turn(&view, &params) {
                drop(view);
                self.write(&json!({"id":id,"result":{"decision":"cancel"}}))
                    .await?;
                return Ok(());
            }
            let choices = approval_choices(method, &params);
            if choices.is_empty() {
                bail!("Codex approval offered no supported choices");
            }
            let local_id = serde_json::to_string(&id)?;
            let summary = approval_summary(method, &params);
            let options = choices.iter().map(|(option, _)| option.clone()).collect();
            view.approvals.insert(
                local_id.clone(),
                Approval {
                    wire_id: id,
                    choices,
                },
            );
            view.push(AcpEvent::PermissionRequest {
                id: local_id,
                summary,
                options,
            })?;
            self.changed.notify_waiters();
            return Ok(());
        }
        // Structured questions, elicitation and token refresh require dedicated
        // answers. An Allow/Deny button must never fabricate those answers.
        self.write(&json!({"id":id,"error":{"code":-32601,"message":format!("Medousa does not yet support Codex request {method}")}})).await?;
        if matches!(
            method,
            "item/tool/requestUserInput"
                | "mcpServer/elicitation/request"
                | "account/chatgptAuthTokens/refresh"
                | "item/permissions/requestApproval"
        ) {
            bail!(
                "Codex requested {method}, which this client cannot present yet. Continue in Codex or refine the prompt."
            );
        }
        Ok(())
    }

    async fn notification(&self, method: &str, params: Value) -> Result<()> {
        let mut view = self.view.lock().await;
        if method == "serverRequest/resolved" {
            if params.get("threadId").and_then(Value::as_str) == Some(view.thread_id.as_str())
                && let Some(id) = params.get("requestId")
            {
                view.approvals.remove(&serde_json::to_string(id)?);
            }
            return Ok(());
        }
        if !belongs_to_turn(&view, &params) {
            return Ok(());
        }
        if view.events.len() >= EVENT_LIMIT {
            bail!("Codex app-server event backlog exceeded its limit");
        }
        match method {
            "turn/started" => {
                if let Some(id) = params.pointer("/turn/id").and_then(Value::as_str)
                    && (view.starting || view.active_turn.is_none())
                {
                    view.active_turn = Some(id.into());
                }
            }
            "item/agentMessage/delta" => {
                if let Some(text) = params.get("delta").and_then(Value::as_str) {
                    if let Some(id) = params.get("itemId").and_then(Value::as_str) {
                        view.streamed.insert(id.into());
                    }
                    view.push(AcpEvent::MessageDelta { text: text.into() })?;
                }
            }
            "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" | "item/plan/delta" => {
                if let Some(text) = params.get("delta").and_then(Value::as_str) {
                    view.push(AcpEvent::ReasoningDelta { text: text.into() })?;
                }
            }
            "item/started" | "item/completed" => {
                let item = params
                    .get("item")
                    .context("Codex item event missing item")?;
                let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
                let id = item.get("id").and_then(Value::as_str).unwrap_or("");
                if kind == "agentMessage" && method == "item/completed" {
                    // Completed text is authoritative only when no deltas were delivered.
                    if !view.streamed.contains(id)
                        && let Some(text) = item.get("text").and_then(Value::as_str)
                    {
                        view.push(AcpEvent::MessageDelta { text: text.into() })?;
                    }
                } else if matches!(
                    kind,
                    "commandExecution"
                        | "fileChange"
                        | "mcpToolCall"
                        | "dynamicToolCall"
                        | "webSearch"
                        | "imageGeneration"
                        | "collabAgentToolCall"
                ) {
                    view.push(AcpEvent::ToolCall {
                        id: id.into(),
                        name: kind.into(),
                        input: normalize_tool_item(item, method == "item/completed"),
                    })?;
                }
            }
            "error" => {
                // Retryable errors are informational. Completion is exclusively
                // the turn/completed notification, never a turn/start RPC ack.
                if params.get("willRetry").and_then(Value::as_bool) != Some(true) {
                    view.turn_error = params
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
            }
            "turn/completed" if !view.terminal => {
                let status = params
                    .pointer("/turn/status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if status == "completed" {
                    view.push(AcpEvent::Done)?;
                } else {
                    let message = params
                        .pointer("/turn/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| view.turn_error.take())
                        .unwrap_or_else(|| format!("Codex turn {status}"));
                    view.push(AcpEvent::Error { message })?;
                }
                view.terminal = true;
                view.active_turn = None;
                view.approvals.clear();
            }
            _ => {}
        }
        self.changed.notify_waiters();
        Ok(())
    }
}

fn belongs_to_turn(view: &View, params: &Value) -> bool {
    if view.thread_id.is_empty() || view.terminal || (!view.starting && view.active_turn.is_none())
    {
        return false;
    }
    if params.get("threadId").and_then(Value::as_str) != Some(view.thread_id.as_str()) {
        return false;
    }
    let turn = params
        .get("turnId")
        .and_then(Value::as_str)
        .or_else(|| params.pointer("/turn/id").and_then(Value::as_str));
    (view.starting && view.active_turn.is_none())
        || turn.is_none()
        || turn == view.active_turn.as_deref()
}

fn normalize_tool_item(item: &Value, completed: bool) -> Value {
    let mut input = item.clone();
    let native = item.get("status").and_then(Value::as_str);
    let status = match native {
        Some("completed") => "completed",
        Some("failed" | "declined") => "failed",
        Some("interrupted" | "cancelled" | "canceled") => "cancelled",
        Some("inProgress") if !completed => "in_progress",
        None if completed => "completed",
        Some(_) if completed => "failed",
        _ => "in_progress",
    };
    if let Some(native) = native {
        input["nativeStatus"] = json!(native);
    }
    input["status"] = json!(status);
    input
}

fn selected_effort(view: &View) -> Option<&str> {
    let model = view
        .models
        .iter()
        .find(|model| model.get("model").and_then(Value::as_str) == view.model.as_deref())?;
    let efforts = model
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array);
    // Explicitly choose this model's advertised default when switching models.
    // Omitting effort would retain the previous model's setting in app-server.
    view.effort
        .as_deref()
        .filter(|current| {
            efforts.is_some_and(|efforts| {
                efforts.iter().any(|effort| {
                    effort.get("reasoningEffort").and_then(Value::as_str) == Some(*current)
                })
            })
        })
        .or_else(|| model.get("defaultReasoningEffort").and_then(Value::as_str))
        .or_else(|| efforts?.first()?.get("reasoningEffort")?.as_str())
}

fn ensure_idle_thread(result: &Value) -> Result<()> {
    let active = result
        .pointer("/thread/status/type")
        .and_then(Value::as_str)
        == Some("active")
        || result
            .pointer("/thread/turns")
            .and_then(Value::as_array)
            .is_some_and(|turns| {
                turns
                    .iter()
                    .any(|turn| turn.get("status").and_then(Value::as_str) == Some("inProgress"))
            });
    if active {
        bail!(
            "This Codex thread still has an active turn. Wait for it to finish in Codex before reconnecting; Medousa cannot safely replay its pending events yet."
        );
    }
    Ok(())
}

fn approval_summary(method: &str, params: &Value) -> String {
    let mut summary = if method == "item/commandExecution/requestApproval" {
        params
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("Codex wants to run a command")
            .to_owned()
    } else {
        "Codex wants to change files".into()
    };
    if let Some(reason) = params.get("reason").and_then(Value::as_str) {
        summary.push('\n');
        summary.push_str(reason);
    }
    if let Some(cwd) = params.get("cwd").and_then(Value::as_str) {
        summary.push_str("\nDirectory: ");
        summary.push_str(cwd);
    }
    if let Some(root) = params.get("grantRoot").and_then(Value::as_str) {
        summary.push_str("\nRequested write root: ");
        summary.push_str(root);
    }
    summary
}

fn approval_choices(method: &str, params: &Value) -> Vec<(AcpPermissionOption, Value)> {
    let mut decisions = params
        .get("availableDecisions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            vec![
                json!("accept"),
                json!("acceptForSession"),
                json!("decline"),
                json!("cancel"),
            ]
        });
    // The native proposal is the only authority for amendment contents.
    if method == "item/commandExecution/requestApproval"
        && params.get("availableDecisions").is_none()
    {
        if let Some(amendment) = params
            .get("proposedExecpolicyAmendment")
            .filter(|v| v.is_array())
        {
            decisions
                .push(json!({"acceptWithExecpolicyAmendment":{"execpolicy_amendment":amendment}}));
        }
        if let Some(amendments) = params
            .get("proposedNetworkPolicyAmendments")
            .and_then(Value::as_array)
        {
            for amendment in amendments {
                decisions.push(
                    json!({"applyNetworkPolicyAmendment":{"network_policy_amendment":amendment}}),
                );
            }
        }
    }
    decisions
        .into_iter()
        .enumerate()
        .filter_map(|(index, decision)| {
            let (name, kind) = match decision.as_str() {
                Some("accept") => ("Allow once".to_owned(), "allow_once"),
                Some("acceptForSession") => {
                    ("Allow for this Codex session".to_owned(), "allow_always")
                }
                Some("decline") => ("Deny".to_owned(), "reject_once"),
                Some("cancel") => ("Deny and stop turn".to_owned(), "reject_once"),
                _ if method == "item/commandExecution/requestApproval"
                    && decision.get("acceptWithExecpolicyAmendment").is_some() =>
                {
                    let amendment =
                        decision.pointer("/acceptWithExecpolicyAmendment/execpolicy_amendment")?;
                    if params.get("proposedExecpolicyAmendment") != Some(amendment) {
                        return None;
                    }
                    (
                        format!("Always allow matching command: {amendment}"),
                        "allow_always",
                    )
                }
                _ if method == "item/commandExecution/requestApproval"
                    && decision.get("applyNetworkPolicyAmendment").is_some() =>
                {
                    let amendment = decision
                        .pointer("/applyNetworkPolicyAmendment/network_policy_amendment")?;
                    if !params
                        .get("proposedNetworkPolicyAmendments")
                        .and_then(Value::as_array)
                        .is_some_and(|offered| offered.contains(amendment))
                    {
                        return None;
                    }
                    let action = amendment.get("action")?.as_str()?;
                    let kind = match action {
                        "allow" => "allow_always",
                        "deny" => "reject_always",
                        _ => return None,
                    };
                    (
                        format!(
                            "Remember network rule: {action} {}",
                            amendment.get("host")?.as_str()?
                        ),
                        kind,
                    )
                }
                _ => return None,
            };
            Some((
                AcpPermissionOption {
                    option_id: format!("decision-{index}"),
                    name,
                    kind: kind.into(),
                },
                decision,
            ))
        })
        .collect()
}

/// A separate Codex app-server and independent reader for each owned thread.
#[derive(Default)]
pub struct CodexClient {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl CodexClient {
    pub fn new() -> Self {
        Self::default()
    }

    async fn session(&self, id: &AcpSessionId) -> Result<Arc<Session>> {
        self.sessions
            .lock()
            .await
            .get(&id.0)
            .cloned()
            .context("unknown native Codex session")
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
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .env_remove("MODEL_PROVIDER")
            .env_remove("CODEX_CONFIG")
            .env_remove("DEFAULT_AUTH_REQUEST")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command
            .spawn()
            .context("could not launch native Codex app-server")?;
        let input = child.stdin.take().context("Codex stdin unavailable")?;
        let stdout = child.stdout.take().context("Codex stdout unavailable")?;
        let mut stderr = child.stderr.take().context("Codex stderr unavailable")?;
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
                let mut frame = Vec::new();
                let read = (&mut reader)
                    .take((FRAME_LIMIT + 1) as u64)
                    .read_until(b'\n', &mut frame)
                    .await;
                let Some(session) = weak.upgrade() else {
                    break;
                };
                let error = match read {
                    Ok(0) => Some("Codex app-server exited or closed stdout".to_owned()),
                    Ok(_) if frame.len() > FRAME_LIMIT => {
                        Some("Codex app-server frame exceeded 10 MiB".to_owned())
                    }
                    Ok(_) => match serde_json::from_slice(&frame) {
                        Ok(value) => session
                            .receive(value)
                            .await
                            .err()
                            .map(|error| error.to_string()),
                        Err(_) => Some("Codex app-server sent malformed JSON".to_owned()),
                    },
                    Err(error) => Some(format!("Codex app-server stdout failed: {error}")),
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
            session.request("initialize", json!({"clientInfo":{"name":"medousa","title":"Medousa","version":env!("CARGO_PKG_VERSION")}})).await?;
            session.write(&json!({"method":"initialized"})).await?;
            let account = session.request("account/read", json!({"refreshToken":false})).await?;
            if account.pointer("/account/type").and_then(Value::as_str) != Some("chatgpt") {
                bail!("Codex requires a saved ChatGPT login. Run `codex login` with the installed CLI, then reconnect.");
            }
            let mut models = Vec::new();
            let mut cursor: Option<String> = None;
            let mut seen = HashSet::new();
            loop {
                let mut params = json!({"limit":100,"includeHidden":false});
                if let Some(cursor) = &cursor { params["cursor"] = json!(cursor); }
                let catalog = session.request("model/list", params).await?;
                models.extend(catalog.get("data").and_then(Value::as_array).context("Codex model/list missing data")?.iter().filter(|model| model.get("hidden").and_then(Value::as_bool) != Some(true)).cloned());
                cursor = catalog.get("nextCursor").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
                match &cursor {
                    Some(cursor) if seen.len() < 100 && seen.insert(cursor.clone()) => {}
                    Some(_) => bail!("Codex model/list returned an invalid pagination cursor"),
                    None => break,
                }
            }
            let resume = resume.filter(|id| !id.trim().is_empty());
            let mut params = json!({"cwd":session.cwd});
            if let Some(resume) = resume { params["threadId"] = json!(resume); }
            // Omitted sandbox/approval fields preserve the user's Codex policy.
            let result = session.request(if resume.is_some() {"thread/resume"} else {"thread/start"}, params).await?;
            let wire = result.pointer("/thread/id").and_then(Value::as_str).context("Codex thread reply missing thread id")?.to_owned();
            if resume.is_some_and(|requested| requested != wire) { bail!("Codex returned an unexpected resumed thread identity"); }
            if result.get("modelProvider").and_then(Value::as_str).is_some_and(|provider| provider != "openai") {
                bail!("Codex is configured with a custom model provider. Select the OpenAI provider in Codex to use the ChatGPT runtime.");
            }
            // Startup events arrive before this client owns the thread. Resume
            // only idle threads, rather than lose pending approvals or deltas.
            ensure_idle_thread(&result)?;
            let mut view = session.view.lock().await;
            view.thread_id = wire.clone();
            view.models = models;
            view.model = result.get("model").and_then(Value::as_str).map(str::to_owned);
            view.effort = result.get("reasoningEffort").and_then(Value::as_str).map(str::to_owned);
            view.effort = selected_effort(&view).map(str::to_owned);
            Ok::<_, anyhow::Error>((wire, resume.is_some()))
        }.await;
        match startup {
            Ok((wire, resumed)) => {
                let id = AcpSessionId(format!("codex-native-{}", uuid::Uuid::now_v7()));
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
                .thread_id
                .clone(),
        )
    }

    pub async fn session_capabilities(
        &self,
        id: &AcpSessionId,
    ) -> medousa_types::AgentRuntimeCapabilities {
        medousa_types::AgentRuntimeCapabilities {
            resume: self.session(id).await.is_ok(),
            ..Default::default()
        }
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
        let mut view = session.view.lock().await;
        if view.starting || view.active_turn.is_some() {
            bail!("Wait for the Codex turn to finish before changing its settings");
        }
        let options = config_options(&view);
        let option = options
            .iter()
            .find(|option| option["id"] == config_id)
            .context("unsupported Codex setting")?;
        if !option["options"]
            .as_array()
            .is_some_and(|choices| choices.iter().any(|choice| choice["value"] == value))
        {
            bail!("Codex did not advertise this setting value");
        }
        match config_id {
            "model" => {
                view.model = value.as_str().map(str::to_owned);
                view.effort = None;
                view.effort = selected_effort(&view).map(str::to_owned);
            }
            "reasoning_effort" => {
                view.effort = value.as_str().map(str::to_owned);
            }
            _ => bail!("unsupported Codex setting"),
        }
        Ok(config_options(&view))
    }

    pub async fn prompt(&self, id: &AcpSessionId, text: &str) -> Result<()> {
        let session = self.session(id).await?;
        let params = {
            let mut view = session.view.lock().await;
            if view.starting || view.active_turn.is_some() {
                bail!("Codex already has an active turn");
            }
            view.starting = true;
            view.terminal = false;
            view.events.clear();
            view.event_bytes = 0;
            view.streamed.clear();
            view.turn_error = None;
            let mut params = json!({"threadId":view.thread_id,"cwd":session.cwd,"input":[{"type":"text","text":text}]});
            if let Some(model) = &view.model {
                params["model"] = json!(model);
            }
            if let Some(effort) = selected_effort(&view) {
                params["effort"] = json!(effort);
            }
            params
        };
        match session.request("turn/start", params).await {
            Ok(result) => {
                let Some(turn_id) = result.pointer("/turn/id").and_then(Value::as_str) else {
                    session
                        .fail("Codex turn/start reply missing turn id".into())
                        .await;
                    session.shutdown().await;
                    bail!("Codex turn/start reply missing turn id");
                };
                let mut view = session.view.lock().await;
                if view
                    .active_turn
                    .as_deref()
                    .is_some_and(|active| active != turn_id)
                {
                    drop(view);
                    session
                        .fail("Codex admitted an unexpected turn identity".into())
                        .await;
                    session.shutdown().await;
                    bail!("Codex admitted an unexpected turn identity");
                }
                // Completion may race ahead of this RPC response.
                if !view.terminal {
                    view.active_turn = Some(turn_id.into());
                }
                view.starting = false;
                Ok(())
            }
            Err(error) => {
                session
                    .fail(format!("Codex prompt admission failed: {error}"))
                    .await;
                session.shutdown().await;
                Err(error)
            }
        }
    }

    pub async fn next_event(&self, id: &AcpSessionId) -> Result<Option<AcpEvent>> {
        let session = self.session(id).await?;
        let notified = session.changed.notified();
        if let Some(event) = session.view.lock().await.pop() {
            return Ok(Some(event));
        }
        let _ = tokio::time::timeout(Duration::from_millis(200), notified).await;
        let event = session.view.lock().await.pop();
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
            .context("Codex approval expired")?;
        let expected = if approved { "accept" } else { "decline" };
        let choice = approval
            .choices
            .iter()
            .find(|(_, decision)| decision.as_str() == Some(expected))
            .context("Codex did not offer this approval decision")?;
        self.respond_permission_choice(id, request_id, Some(&choice.0.option_id))
            .await
    }

    pub async fn respond_permission_choice(
        &self,
        id: &AcpSessionId,
        request_id: &str,
        option_id: Option<&str>,
    ) -> Result<()> {
        let session = self.session(id).await?;
        let mut view = session.view.lock().await;
        let approval = view
            .approvals
            .get(request_id)
            .context("Codex approval expired")?;
        let choice = match option_id {
            Some(id) => approval
                .choices
                .iter()
                .find(|(option, _)| option.option_id == id),
            None => approval
                .choices
                .iter()
                .find(|(_, decision)| decision.as_str() == Some("cancel"))
                .or_else(|| {
                    approval
                        .choices
                        .iter()
                        .find(|(_, decision)| decision.as_str() == Some("decline"))
                }),
        }
        .context("Codex did not offer this approval choice")?;
        let response = json!({"id":approval.wire_id,"result":{"decision":choice.1}});
        // Serialize retirement and reply so a resolved callback cannot acquire a
        // stale permission after cancellation or serverRequest/resolved.
        session.write(&response).await?;
        view.approvals.remove(request_id);
        Ok(())
    }

    pub async fn cancel(&self, id: &AcpSessionId) -> Result<()> {
        let session = self
            .sessions
            .lock()
            .await
            .remove(&id.0)
            .context("unknown native Codex session")?;
        let (thread_id, turn_id) = {
            let view = session.view.lock().await;
            (view.thread_id.clone(), view.active_turn.clone())
        };
        if let Some(turn_id) = turn_id {
            // The RPC ack is only receipt. Await terminal acknowledgement before
            // falling back to stopping the isolated app-server process.
            let interrupted = tokio::time::timeout(Duration::from_secs(3), async {
                session
                    .request(
                        "turn/interrupt",
                        json!({"threadId":thread_id,"turnId":turn_id}),
                    )
                    .await?;
                loop {
                    let changed = session.changed.notified();
                    if session.view.lock().await.terminal {
                        break;
                    }
                    changed.await;
                }
                Ok::<_, anyhow::Error>(())
            })
            .await;
            session.shutdown().await;
            interrupted.context("Codex interrupt timed out; app-server was stopped")??;
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
}

fn config_options(view: &View) -> Vec<Value> {
    let mut options = Vec::new();
    if !view.models.is_empty() {
        options.push(json!({"id":"model","name":"Model","category":"model","type":"select","currentValue":view.model,
            "options":view.models.iter().filter_map(|model| Some(json!({"value":model.get("model")?.as_str()?,"name":model.get("displayName").and_then(Value::as_str).unwrap_or("Codex model")}))).collect::<Vec<_>>()}));
    }
    if let Some(model) = view
        .models
        .iter()
        .find(|model| model.get("model").and_then(Value::as_str) == view.model.as_deref())
        && let Some(efforts) = model
            .get("supportedReasoningEfforts")
            .and_then(Value::as_array)
            .filter(|values| !values.is_empty())
    {
        options.push(json!({"id":"reasoning_effort","name":"Reasoning","category":"thought_level","type":"select",
                "currentValue":selected_effort(view),
                "options":efforts.iter().filter_map(|effort| Some(json!({"value":effort.get("reasoningEffort")?.as_str()?,"name":effort.get("reasoningEffort")?.as_str()?,"description":effort.get("description")}))).collect::<Vec<_>>()}));
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_choices_preserve_structured_native_decisions() {
        let params = json!({"proposedExecpolicyAmendment":["cargo","test"]});
        let choices = approval_choices("item/commandExecution/requestApproval", &params);
        assert!(choices.iter().any(|(_, decision)| decision == &json!({"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["cargo","test"]}})));
        let restricted = approval_choices(
            "item/commandExecution/requestApproval",
            &json!({"availableDecisions":["decline"],"proposedExecpolicyAmendment":["cargo"]}),
        );
        assert_eq!(restricted.len(), 1);
        assert_eq!(restricted[0].1, "decline");
    }

    #[test]
    fn unoffered_amendments_are_not_granted() {
        let choices = approval_choices(
            "item/commandExecution/requestApproval",
            &json!({"availableDecisions":[{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["sh"]}}],"proposedExecpolicyAmendment":["cargo"]}),
        );
        assert!(choices.is_empty());
    }

    #[test]
    fn config_options_use_native_catalog_and_efforts() {
        let view = View {
            models: vec![
                json!({"id":"catalog-id","model":"model-slug","displayName":"Latest Codex","defaultReasoningEffort":"medium","supportedReasoningEfforts":[{"reasoningEffort":"high","description":"More reasoning"}]}),
            ],
            model: Some("model-slug".into()),
            ..Default::default()
        };
        let options = config_options(&view);
        assert_eq!(options[0]["options"][0]["value"], "model-slug");
        assert_eq!(options[1]["currentValue"], "medium");
        assert_eq!(options[1]["options"][0]["value"], "high");
    }

    #[test]
    fn notifications_are_scoped_to_owned_active_turn() {
        let mut view = View {
            thread_id: "owned".into(),
            active_turn: Some("active".into()),
            ..Default::default()
        };
        assert!(belongs_to_turn(
            &view,
            &json!({"threadId":"owned","turnId":"active"})
        ));
        assert!(!belongs_to_turn(
            &view,
            &json!({"threadId":"other","turnId":"active"})
        ));
        assert!(!belongs_to_turn(
            &view,
            &json!({"threadId":"owned","turnId":"old"})
        ));
        view.terminal = true;
        assert!(!belongs_to_turn(
            &view,
            &json!({"threadId":"owned","turnId":"active"})
        ));
    }

    #[test]
    fn model_switch_uses_new_default_instead_of_inheriting_old_effort() {
        let mut view = View {
            models: vec![
                json!({"model":"new-model","defaultReasoningEffort":"low","supportedReasoningEfforts":[{"reasoningEffort":"low"},{"reasoningEffort":"medium"}]}),
            ],
            model: Some("new-model".into()),
            effort: Some("xhigh".into()),
            ..Default::default()
        };
        assert_eq!(selected_effort(&view), Some("low"));
        view.effort = None;
        assert_eq!(selected_effort(&view), Some("low"));
        view.effort = Some("medium".into());
        assert_eq!(selected_effort(&view), Some("medium"));
    }

    #[test]
    fn native_terminal_tool_states_are_shared_terminal_states() {
        assert_eq!(
            normalize_tool_item(&json!({"status":"declined"}), true)["status"],
            "failed"
        );
        assert_eq!(
            normalize_tool_item(&json!({"status":"interrupted"}), true)["status"],
            "cancelled"
        );
        assert_eq!(
            normalize_tool_item(&json!({"status":"inProgress"}), false)["status"],
            "in_progress"
        );
        assert_eq!(
            normalize_tool_item(&json!({"type":"webSearch"}), true)["status"],
            "completed"
        );
    }

    #[test]
    fn active_resume_is_explicitly_rejected() {
        assert!(
            ensure_idle_thread(&json!({"thread":{"status":{"type":"active"},"turns":[]}})).is_err()
        );
        assert!(
            ensure_idle_thread(
                &json!({"thread":{"turns":[{"id":"running","status":"inProgress"}]}})
            )
            .is_err()
        );
        assert!(ensure_idle_thread(&json!({"thread":{"status":{"type":"idle"},"turns":[{"id":"done","status":"completed"}]}})).is_ok());
    }
}
