//! Capability-driven runtime controls. All network work is returned as owned,
//! session-addressed commands; mutations are never retried by this module.
use futures_util::StreamExt;
use medousa_app_core::ChatCommand;
use medousa_sdk::{
    MedousaClient,
    generated::{expand_path, ops},
};
use medousa_types::*;
use rndr_core::*;
use rndr_ui::{
    Typography, UiId,
    controls::{self, ButtonProps, HostIntent},
    text,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

static IDS: AtomicU64 = AtomicU64::new(400_000);
fn id() -> u64 {
    IDS.fetch_add(1, Ordering::Relaxed)
}
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ActiveRuntime {
    #[default]
    Native,
    Agent {
        runtime: String,
        agent_session_id: String,
    },
    Provider {
        conversation_id: String,
    },
}
#[derive(Clone, Debug)]
pub struct RuntimeCommand {
    pub session: String,
    serial: u64,
    operation: Operation,
}
#[derive(Clone, Debug)]
enum Operation {
    Refresh,
    Select {
        runtime: String,
        previous: Option<String>,
    },
    Provider(String),
    Prompt {
        agent: String,
        prompt: String,
        work_id: Option<String>,
    },
    SendProvider {
        conversation: String,
        text: String,
        request_id: String,
    },
    Config {
        agent: String,
        config: String,
        value: serde_json::Value,
    },
    Cancel {
        agent: String,
    },
}
#[derive(Debug)]
pub struct RuntimeUpdate {
    command: RuntimeCommand,
    result: Result<Payload, String>,
    may_have_mutated: bool,
}
#[derive(Debug)]
enum Payload {
    Catalog {
        runtimes: Result<Vec<AgentRuntimeInfo>, String>,
        conversations: Result<Vec<ExternalConversationView>, String>,
    },
    Selected(Option<CreateAgentSessionResponse>),
    Provider(ExternalConversationView),
    Config(Vec<AgentSessionConfigOption>),
    Prompt,
    Cancel,
}
#[derive(Clone, Debug)]
pub struct RuntimeStream {
    pub session: String,
    pub agent_session_id: String,
    pub stream_url: String,
}
/// Pass these canonical commands directly to the host ChatCore, not a second
/// renderer-owned transcript. Streams stay open across multiple ACP prompts.
pub async fn stream(
    client: &MedousaClient,
    source: RuntimeStream,
    sender: tokio::sync::mpsc::UnboundedSender<Result<ChatCommand, String>>,
) {
    let api = client.interactive();
    let events = api.stream_v3(source.stream_url);
    futures_util::pin_mut!(events);
    while let Some(event) = events.next().await {
        let command = event
            .map(|envelope| ChatCommand::ApplyStreamEvent {
                session_id: source.session.clone(),
                envelope,
                expected_revision: None,
                promote_item_id: None,
            })
            .map_err(|e| {
                format!(
                    "Agent {} stream disconnected: {e}. Refresh history before sending again.",
                    source.agent_session_id
                )
            });
        let failed = command.is_err();
        if sender.send(command).is_err() || failed {
            return;
        }
    }
    let _ = sender.send(Err(format!(
        "Agent {} stream ended; reconcile history before sending again.",
        source.agent_session_id
    )));
}
#[derive(Default)]
struct Session {
    active: ActiveRuntime,
    configs: Vec<AgentSessionConfigOption>,
    conversation: Option<ExternalConversationView>,
    pending: Option<u64>,
    error: Option<String>,
    // A failed POST may have been admitted. Do not replay it or allow a
    // second prompt until an explicit refresh/reconciliation by the host.
    uncertain: bool,
    running: bool,
    closed: bool,
    work_id: Option<String>,
    revision: u64,
}
#[derive(Default)]
struct Runtimes {
    catalog: Vec<AgentRuntimeInfo>,
    conversations: Vec<ExternalConversationView>,
    sessions: HashMap<String, Session>,
    queue: Vec<RuntimeCommand>,
    streams: Vec<RuntimeStream>,
    actions: HashMap<Entity, RuntimeCommand>,
    visible: Option<String>,
    rendered: u64,
    revision: u64,
}
fn enabled(runtime: &AgentRuntimeInfo) -> bool {
    runtime.available
        && (runtime.uses_native_turns
            || (runtime.binary_present && runtime.auth_status.as_deref() != Some("signed_out")))
}
pub fn setup(world: &mut World, parent: Entity) -> Entity {
    world.insert_resource(Runtimes::default());
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Hug,
        gap: 4.,
        ..Layout::column()
    },));
    set_parent(world, root, parent);
    root
}
pub fn active(world: &World) -> ActiveRuntime {
    let Some(state) = world.resource::<Runtimes>() else {
        return ActiveRuntime::Native;
    };
    state
        .visible
        .as_ref()
        .and_then(|id| state.sessions.get(id))
        .map(|s| s.active.clone())
        .unwrap_or_default()
}
pub fn busy(world: &World) -> bool {
    world
        .resource::<Runtimes>()
        .and_then(|s| s.visible.as_ref().and_then(|id| s.sessions.get(id)))
        .is_some_and(|s| s.pending.is_some() || s.uncertain || s.running)
}
pub fn conversation<'a>(world: &'a World, session: &str) -> Option<&'a ExternalConversationView> {
    world
        .resource::<Runtimes>()?
        .sessions
        .get(session)?
        .conversation
        .as_ref()
}
pub fn refresh(world: &mut World, session: &str) {
    queue(world, session, Operation::Refresh);
}
fn queue(world: &mut World, session: &str, operation: Operation) -> bool {
    let Some(state) = world.resource_mut::<Runtimes>() else {
        return false;
    };
    let s = state.sessions.entry(session.into()).or_default();
    if s.pending.is_some()
        || (s.uncertain && !matches!(operation, Operation::Refresh | Operation::Cancel { .. }))
        || (s.running && !matches!(operation, Operation::Refresh | Operation::Cancel { .. }))
    {
        return false;
    }
    let serial = id();
    s.pending = Some(serial);
    if matches!(operation, Operation::Prompt { .. }) {
        s.running = true;
    }
    s.error = None;
    s.revision += 1;
    state.queue.push(RuntimeCommand {
        session: session.into(),
        serial,
        operation,
    });
    true
}
/// False means native route. Err means do NOT fall back to native; the runtime
/// explicitly chosen by the user cannot accept this input.
pub fn send(
    world: &mut World,
    session: &str,
    prompt: String,
    has_extra_inputs: bool,
) -> Result<bool, String> {
    let state = world
        .resource::<Runtimes>()
        .ok_or("Runtime controls unavailable")?;
    let Some(s) = state.sessions.get(session) else {
        return Ok(false);
    };
    if s.pending.is_some() || s.uncertain || s.running {
        return Err(
            "Runtime operation pending or outcome uncertain; reconcile before sending".into(),
        );
    }
    if s.closed {
        return Err(
            "Agent session stopped. Choose the runtime again to start a new session".into(),
        );
    }
    let operation = match &s.active {
        ActiveRuntime::Native => return Ok(false),
        ActiveRuntime::Agent {
            agent_session_id, ..
        } => {
            if has_extra_inputs {
                return Err(
                    "Agent prompts currently accept text only; remove attachments and tools".into(),
                );
            }
            Operation::Prompt {
                agent: agent_session_id.clone(),
                prompt,
                work_id: s.work_id.clone(),
            }
        }
        ActiveRuntime::Provider { conversation_id } => {
            if has_extra_inputs {
                return Err(
                    "Connected conversations accept text only; remove attachments and tools".into(),
                );
            }
            Operation::SendProvider {
                conversation: conversation_id.clone(),
                text: prompt,
                request_id: format!(
                    "native-{}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos(),
                    id()
                ),
            }
        }
    };
    if queue(world, session, operation) {
        Ok(true)
    } else {
        Err("Runtime operation already pending".into())
    }
}
pub fn cancel(world: &mut World, session: &str) -> bool {
    let agent = world
        .resource::<Runtimes>()
        .and_then(|r| r.sessions.get(session))
        .and_then(|s| match &s.active {
            ActiveRuntime::Agent {
                agent_session_id, ..
            } => Some(agent_session_id.clone()),
            _ => None,
        });
    agent.is_some_and(|agent| queue(world, session, Operation::Cancel { agent }))
}
fn button(
    world: &mut World,
    root: Entity,
    state: &mut Runtimes,
    session: &str,
    label: String,
    operation: Operation,
    enabled: bool,
) {
    let serial = id();
    let mut props = ButtonProps::new(label, HostIntent::Command(serial));
    props.enabled = enabled;
    let handle = controls::Button::mount(world, root, UiId(serial), props).unwrap();
    state.actions.insert(
        handle.root,
        RuntimeCommand {
            session: session.into(),
            serial,
            operation,
        },
    );
}
pub fn sync(world: &mut World, root: Entity, session: Option<&str>) {
    let mut state = std::mem::take(world.resource_mut::<Runtimes>().expect("runtime setup"));
    let revision = state.revision
        + session
            .and_then(|id| state.sessions.get(id))
            .map_or(0, |s| s.revision);
    if state.visible.as_deref() == session && state.rendered == revision {
        *world.resource_mut::<Runtimes>().unwrap() = state;
        return;
    }
    for child in children_of(world, root) {
        despawn(world, child);
    }
    state.actions.clear();
    state.visible = session.map(str::to_owned);
    state.rendered = revision;
    if let Some(session) = session {
        let s = state.sessions.entry(session.into()).or_default();
        let active = s.active.clone();
        let ready = s.pending.is_none() && !s.uncertain && !s.running;
        let closed = s.closed;
        let configs = s.configs.clone();
        let error = s.error.clone();
        text(
            world,
            root,
            &match &active {
                ActiveRuntime::Native => "Runtime · Medousa".into(),
                ActiveRuntime::Agent { runtime, .. } => format!("Runtime · {runtime}"),
                ActiveRuntime::Provider { .. } => "Runtime · Connected conversation".into(),
            },
            Typography::Secondary,
        )
        .unwrap();
        if let Some(error) = error {
            text(world, root, &error, Typography::Secondary).unwrap();
        }
        let previous = match &active {
            ActiveRuntime::Agent {
                agent_session_id, ..
            } if !closed => Some(agent_session_id.clone()),
            _ => None,
        };
        button(
            world,
            root,
            &mut state,
            session,
            "Medousa".into(),
            Operation::Select {
                runtime: "medousa".into(),
                previous: previous.clone(),
            },
            ready && active != ActiveRuntime::Native,
        );
        for row in state
            .catalog
            .clone()
            .into_iter()
            .filter(|r| !r.uses_native_turns && r.runtime != "medousa")
        {
            let available = enabled(&row);
            let label = if available {
                row.runtime.clone()
            } else {
                format!(
                    "{} · {}",
                    row.runtime,
                    row.detail
                        .as_deref()
                        .or(row.auth_detail.as_deref())
                        .unwrap_or("Unavailable / sign in required")
                )
            };
            let selected =
                matches!(&active, ActiveRuntime::Agent { runtime, .. } if runtime == &row.runtime);
            button(
                world,
                root,
                &mut state,
                session,
                label,
                Operation::Select {
                    runtime: row.runtime,
                    previous: previous.clone(),
                },
                ready && available && (!selected || closed),
            );
        }
        // Provider conversations are actual server records, never hard-coded runtime IDs.
        for row in state.conversations.clone() {
            button(
                world,
                root,
                &mut state,
                session,
                format!("Connected · {} ({:?})", row.label, row.provider),
                Operation::Provider(row.id),
                ready && previous.is_none(),
            );
        }
        if let ActiveRuntime::Agent {
            agent_session_id, ..
        } = active
        {
            for config in configs {
                for choice in config.options {
                    let selected = choice.value == config.current_value;
                    button(
                        world,
                        root,
                        &mut state,
                        session,
                        format!(
                            "{} · {}{}",
                            config.name,
                            choice.name,
                            if selected { " ✓" } else { "" }
                        ),
                        Operation::Config {
                            agent: agent_session_id.clone(),
                            config: config.id.clone(),
                            value: choice.value,
                        },
                        ready && !selected,
                    );
                }
            }
        }
        let pending = state.sessions[session].pending.is_some();
        button(
            world,
            root,
            &mut state,
            session,
            "Refresh runtimes / connections".into(),
            Operation::Refresh,
            !pending,
        );
    }
    *world.resource_mut::<Runtimes>().unwrap() = state;
}
pub fn action(world: &mut World, intent: &ActionIntent) -> bool {
    if controls::host_intent(world, intent).is_none() {
        return false;
    }
    let command = world
        .resource::<Runtimes>()
        .and_then(|r| r.actions.get(&intent.entity))
        .cloned();
    let Some(command) = command else {
        return false;
    };
    if world.resource::<Runtimes>().unwrap().visible.as_deref() == Some(&command.session) {
        queue(world, &command.session, command.operation);
    }
    true
}
pub fn drain_commands(world: &mut World) -> Vec<RuntimeCommand> {
    std::mem::take(&mut world.resource_mut::<Runtimes>().unwrap().queue)
}
pub fn drain_streams(world: &mut World) -> Vec<RuntimeStream> {
    std::mem::take(&mut world.resource_mut::<Runtimes>().unwrap().streams)
}
macro_rules! get {
    ($client:expr, $path:expr) => {{
        match $client
            .transport()
            .get_json($client.base_url(), $path)
            .await
        {
            Ok(value) => serde_json::from_value(value).map_err(|e| e.to_string()),
            Err(error) => Err(error.to_string()),
        }
    }};
}
fn session_path(path: &str, session: &str) -> Result<String, String> {
    expand_path(path, &[("session_id", session)])
}
async fn guard(client: &MedousaClient, session: &str) -> Result<Option<String>, String> {
    let bot: SessionBotResponse = get!(
        client,
        &session_path(ops::SESSIONS_BY_SESSION_ID_BOT_GET.path, session)?
    )?;
    if bot.binding.is_some() {
        return Err("A Bot conversation uses its configured runtime".into());
    }
    let binding: SessionCodeBindingResponse = get!(
        client,
        &session_path(ops::SESSIONS_BY_SESSION_ID_CODE_BINDING_GET.path, session)?
    )?;
    let mode: SessionAgentModeResponse = get!(
        client,
        &session_path(ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_GET.path, session)?
    )?;
    if mode.effective_mode == AgentModeId::Coder && binding.work_id.is_none() {
        return Err("Bind this Coder conversation to a project before starting an agent".into());
    }
    Ok(binding.work_id)
}
pub async fn execute(
    client: &MedousaClient,
    command: RuntimeCommand,
) -> Result<RuntimeUpdate, String> {
    let mut may_have_mutated = false;
    let result = execute_inner(client, &command, &mut may_have_mutated).await;
    // Even transport errors return a scoped update so the pending state clears.
    Ok(RuntimeUpdate {
        command,
        result,
        may_have_mutated,
    })
}
async fn execute_inner(
    client: &MedousaClient,
    command: &RuntimeCommand,
    may_have_mutated: &mut bool,
) -> Result<Payload, String> {
    let api = client.agents();
    match &command.operation {
        Operation::Refresh => {
            let runtimes = api
                .list_runtimes()
                .await
                .map(|r| r.runtimes)
                .map_err(|e| e.to_string());
            let conversations: Result<ExternalConversationListResponse, String> =
                get!(client, ops::EXTERNAL_CONVERSATIONS_GET.path);
            let conversations = conversations.map(|r| r.conversations);
            Ok(Payload::Catalog {
                runtimes,
                conversations,
            })
        }
        Operation::Select { runtime, previous } => {
            let work_id = if runtime != "medousa" {
                let catalog = api.list_runtimes().await.map_err(|e| e.to_string())?;
                if !catalog
                    .runtimes
                    .iter()
                    .any(|r| &r.runtime == runtime && !r.uses_native_turns && enabled(r))
                {
                    return Err("Runtime unavailable or signed out".into());
                }
                guard(client, &command.session).await?
            } else {
                None
            };
            if let Some(previous) = previous {
                *may_have_mutated = true;
                let response = api.cancel(previous).await.map_err(|e| e.to_string())?;
                if !response.cancelled {
                    return Err(response.message);
                }
            }
            if runtime == "medousa" {
                return Ok(Payload::Selected(None));
            }
            *may_have_mutated = true;
            let response = api
                .create_session(&CreateAgentSessionRequest {
                    session_id: command.session.clone(),
                    runtime: runtime.clone(),
                    prompt: None,
                    cwd: None,
                    command: None,
                    args: None,
                    surface: None,
                    work_id: work_id.clone(),
                    resume_provider_token: None,
                    code_context: None,
                })
                .await
                .map_err(|e| e.to_string())?;
            if response.session_id != command.session
                || response.runtime != *runtime
                || response.work_id != work_id
            {
                return Err("Agent response scope mismatch; reconcile before continuing".into());
            }
            Ok(Payload::Selected(Some(response)))
        }
        Operation::Provider(conversation) => {
            let path = expand_path(
                ops::EXTERNAL_CONVERSATIONS_BY_ID_GET.path,
                &[("id", conversation)],
            )?;
            let row: ExternalConversationView = get!(client, &path)?;
            if row.id != *conversation {
                return Err("Conversation response scope mismatch".into());
            }
            Ok(Payload::Provider(row))
        }
        Operation::Prompt {
            agent,
            prompt,
            work_id,
        } => {
            if guard(client, &command.session).await? != *work_id {
                return Err(
                    "Project binding changed; switch runtime to prepare a new agent session".into(),
                );
            }
            *may_have_mutated = true;
            let response = api
                .prompt(
                    agent,
                    &AgentSessionPromptRequest {
                        prompt: prompt.clone(),
                        code_context: None,
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
            if response.agent_session_id != *agent || !response.accepted {
                return Err("Agent prompt not accepted / response scope mismatch".into());
            }
            Ok(Payload::Prompt)
        }
        Operation::Config {
            agent,
            config,
            value,
        } => {
            *may_have_mutated = true;
            let response = api
                .set_config_option(
                    agent,
                    &SetAgentSessionConfigOptionRequest {
                        config_id: config.clone(),
                        value: value.clone(),
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
            if response.agent_session_id != *agent {
                return Err("Config response scope mismatch".into());
            }
            Ok(Payload::Config(response.config_options))
        }
        Operation::Cancel { agent } => {
            *may_have_mutated = true;
            let response = api.cancel(agent).await.map_err(|e| e.to_string())?;
            if response.agent_session_id != *agent || !response.cancelled {
                return Err(response.message);
            }
            Ok(Payload::Cancel)
        }
        Operation::SendProvider {
            conversation,
            text,
            request_id,
        } => {
            let path = expand_path(
                ops::EXTERNAL_CONVERSATIONS_BY_ID_MESSAGES_POST.path,
                &[("id", conversation)],
            )?;
            *may_have_mutated = true;
            let value = client
                .transport()
                .post_json(
                    client.base_url(),
                    &path,
                    serde_json::json!({"request_id":request_id,"text":text}),
                )
                .await
                .map_err(|e| e.to_string())?;
            let row: ExternalConversationView =
                serde_json::from_value(value).map_err(|e| e.to_string())?;
            if row.id != *conversation {
                return Err("Conversation response scope mismatch".into());
            }
            Ok(Payload::Provider(row))
        }
    }
}
pub fn apply(world: &mut World, update: RuntimeUpdate) {
    let state = world.resource_mut::<Runtimes>().expect("runtime setup");
    let s = state
        .sessions
        .entry(update.command.session.clone())
        .or_default();
    if s.pending != Some(update.command.serial) {
        return;
    }
    s.pending = None;
    s.revision += 1;
    match update.result {
        Err(error) => {
            if update.may_have_mutated {
                s.uncertain = true;
            }
            if !update.may_have_mutated
                && matches!(update.command.operation, Operation::Prompt { .. })
            {
                s.running = false;
            }
            s.error = Some(error);
        }
        Ok(payload) => {
            s.error = None;
            match payload {
                Payload::Catalog {
                    runtimes,
                    conversations,
                } => {
                    match runtimes {
                        Ok(rows) => state.catalog = rows,
                        Err(error) => s.error = Some(format!("Runtimes: {error}")),
                    };
                    state.revision += 1;
                    match conversations {
                        Ok(rows) => {
                            if let ActiveRuntime::Provider { conversation_id } = &s.active {
                                s.conversation =
                                    rows.iter().find(|r| &r.id == conversation_id).cloned();
                            }
                            state.conversations = rows;
                        }
                        Err(error) => s.error = Some(format!("Connected conversations: {error}")),
                    }
                    // Catalog reads do NOT resolve an ambiguous agent mutation.
                }
                Payload::Selected(response) => {
                    s.closed = false;
                    s.configs.clear();
                    s.conversation = None;
                    s.active = if let Some(response) = response {
                        s.configs = response.config_options;
                        s.work_id = response.work_id;
                        if response.stream_ready {
                            state.streams.push(RuntimeStream {
                                session: update.command.session,
                                agent_session_id: response.agent_session_id.clone(),
                                stream_url: response.stream_url,
                            });
                        } else {
                            s.uncertain = true;
                            s.error = Some(
                                "Agent stream is not ready; refresh history before sending".into(),
                            );
                        }
                        ActiveRuntime::Agent {
                            runtime: response.runtime,
                            agent_session_id: response.agent_session_id,
                        }
                    } else {
                        ActiveRuntime::Native
                    };
                }
                Payload::Provider(row) => {
                    s.active = ActiveRuntime::Provider {
                        conversation_id: row.id.clone(),
                    };
                    s.conversation = Some(row);
                }
                Payload::Config(options) => s.configs = options,
                Payload::Prompt => {}
                Payload::Cancel => {
                    s.closed = true;
                    s.configs.clear();
                    s.running = false;
                    s.uncertain = false;
                }
            }
        }
    }
}
/// Host may clear a mutation uncertainty only after authoritative history /
/// known-agent reconciliation, never merely on a catalog refresh.
pub fn reconciled(world: &mut World, session: &str) {
    if let Some(s) = world
        .resource_mut::<Runtimes>()
        .and_then(|r| r.sessions.get_mut(session))
    {
        s.uncertain = false;
        s.running = false;
        s.revision += 1;
    }
}
/// Called only after applying a terminal canonical event for this exact agent.
pub fn settled(world: &mut World, session: &str, agent: &str) {
    if let Some(s) = world
        .resource_mut::<Runtimes>()
        .and_then(|r| r.sessions.get_mut(session))
    {
        if matches!(&s.active, ActiveRuntime::Agent { agent_session_id, .. } if agent_session_id == agent)
        {
            s.running = false;
            s.revision += 1;
        }
    }
}
#[cfg(test)]
#[path = "runtimes_tests.rs"]
mod tests;

/// Used by the host only after the server has acknowledged prompt admission.
pub fn admitted(update: &RuntimeUpdate) -> Option<&str> {
    (update.result.is_ok()
        && matches!(
            update.command.operation,
            Operation::Prompt { .. } | Operation::SendProvider { .. }
        ))
    .then_some(update.command.session.as_str())
}
pub fn stream_lost(world: &mut World, session: &str, agent: &str) {
    let Some(state) = world.resource_mut::<Runtimes>() else {
        return;
    };
    if let Some(s) = state.sessions.get_mut(session) {
        if matches!(&s.active,ActiveRuntime::Agent{agent_session_id,..} if agent_session_id==agent)
        {
            s.uncertain = true;
            s.revision += 1;
            state.revision += 1;
        }
    }
}
