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
use rndr_ui::menu::{MenuAt, MenuEntry, MenuHandle, MenuItem, MenuPicked, open_menu};
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
    menu: Option<RuntimeMenu>,
    visible: Option<String>,
    revision: u64,
}
/// The open runtime menu: what each item does, for the session it was built for.
struct RuntimeMenu {
    anchor: Entity,
    handle: MenuHandle,
    session: String,
    picks: HashMap<u64, Pick>,
}
enum Pick {
    Run(Operation),
    Setup,
}
fn enabled(runtime: &AgentRuntimeInfo) -> bool {
    runtime.available
        && (runtime.uses_native_turns
            || (runtime.binary_present && runtime.auth_status.as_deref() != Some("signed_out")))
}
pub fn setup(world: &mut World) {
    world.insert_resource(Runtimes::default());
}
/// The conversation now on screen; [`active`] and [`busy`] answer for it.
pub fn show(world: &mut World, session: Option<&str>) {
    let state = world.resource_mut::<Runtimes>().expect("runtime setup");
    if state.visible.as_deref() != session {
        state.visible = session.map(str::to_owned);
    }
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
/// What the composer's runtime chip reads for `session`.
pub fn label(world: &World, session: Option<&str>) -> String {
    let active = session
        .and_then(|id| world.resource::<Runtimes>()?.sessions.get(id))
        .map(|s| s.active.clone())
        .unwrap_or_default();
    match active {
        ActiveRuntime::Native => "Medousa".into(),
        ActiveRuntime::Agent { runtime, .. } => runtime,
        ActiveRuntime::Provider { conversation_id } => world
            .resource::<Runtimes>()
            .and_then(|r| r.conversations.iter().find(|c| c.id == conversation_id))
            .map_or_else(|| "Connected".into(), |c| c.label.clone()),
    }
}
/// Choose who answers `session`: Medousa, an installed agent runtime or a
/// connected conversation, with the agent's own options in submenus. Runtimes
/// that still need installing collapse into one submenu leading to Packages.
pub fn open(world: &mut World, anchor: Entity, session: &str) -> bool {
    if let Some(old) = world.resource_mut::<Runtimes>().unwrap().menu.take() {
        old.handle.close(world);
    }
    let state = world.resource_mut::<Runtimes>().unwrap();
    let s = state.sessions.entry(session.into()).or_default();
    let active = s.active.clone();
    let ready = s.pending.is_none() && !s.uncertain && !s.running;
    let closed = s.closed;
    let configs = s.configs.clone();
    let error = s.error.clone();
    let pending = s.pending.is_some();
    let catalog = state.catalog.clone();
    let conversations = state.conversations.clone();
    let mut picks = HashMap::new();
    // The current choice reads checked, not dimmed; picking it does nothing.
    let mut item = |label: String, pick: Option<Pick>, checked: Option<bool>, enabled: bool| {
        let serial = id();
        if let Some(pick) = pick {
            picks.insert(serial, pick);
        }
        let mut item = MenuItem::new(serial, label).enabled(enabled);
        if let Some(checked) = checked {
            item = item.checked(checked);
        }
        MenuEntry::from(item)
    };
    let previous = match &active {
        ActiveRuntime::Agent {
            agent_session_id, ..
        } if !closed => Some(agent_session_id.clone()),
        _ => None,
    };
    let mut entries = Vec::new();
    if let Some(error) = error {
        entries.push(MenuEntry::Group(error));
    }
    entries.push(MenuEntry::Group("Runtime".into()));
    let native = active == ActiveRuntime::Native;
    entries.push(item(
        "Medousa".into(),
        (!native).then(|| {
            Pick::Run(Operation::Select {
                runtime: "medousa".into(),
                previous: previous.clone(),
            })
        }),
        Some(native),
        ready,
    ));
    let (installed, missing): (Vec<_>, Vec<_>) = catalog
        .into_iter()
        .filter(|r| !r.uses_native_turns && r.runtime != "medousa")
        .partition(enabled);
    for row in installed {
        let selected =
            matches!(&active, ActiveRuntime::Agent { runtime, .. } if runtime == &row.runtime);
        entries.push(item(
            row.runtime.clone(),
            (!selected || closed).then(|| {
                Pick::Run(Operation::Select {
                    runtime: row.runtime,
                    previous: previous.clone(),
                })
            }),
            Some(selected),
            ready,
        ));
    }
    // Provider conversations are actual server records, never hard-coded runtime IDs.
    if !conversations.is_empty() {
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::Group("Connected".into()));
        for row in conversations {
            let selected = matches!(&active, ActiveRuntime::Provider { conversation_id } if conversation_id == &row.id);
            entries.push(item(
                format!("{} · {:?}", row.label, row.provider),
                (!selected).then(|| Pick::Run(Operation::Provider(row.id))),
                Some(selected),
                ready && (selected || previous.is_none()),
            ));
        }
    }
    if let ActiveRuntime::Agent {
        agent_session_id, ..
    } = &active
    {
        let options: Vec<_> = configs
            .into_iter()
            .filter(|c| !c.options.is_empty())
            .map(|config| {
                let choices = config
                    .options
                    .iter()
                    .map(|choice| {
                        let selected = choice.value == config.current_value;
                        item(
                            choice.name.clone(),
                            (!selected).then(|| {
                                Pick::Run(Operation::Config {
                                    agent: agent_session_id.clone(),
                                    config: config.id.clone(),
                                    value: choice.value.clone(),
                                })
                            }),
                            Some(selected),
                            ready,
                        )
                    })
                    .collect();
                MenuEntry::submenu(config.name.clone(), choices)
            })
            .collect();
        if !options.is_empty() {
            entries.push(MenuEntry::Separator);
            entries.extend(options);
        }
    }
    entries.push(MenuEntry::Separator);
    if !missing.is_empty() {
        let rows = missing
            .into_iter()
            .map(|row| {
                let detail = row
                    .detail
                    .as_deref()
                    .or(row.auth_detail.as_deref())
                    .unwrap_or("Install or sign in");
                item(
                    format!("{} · {detail}", row.runtime),
                    Some(Pick::Setup),
                    None,
                    true,
                )
            })
            .collect::<Vec<_>>();
        entries.push(MenuEntry::submenu(
            format!("Needs setup ({})", rows.len()),
            rows,
        ));
    }
    entries.push(item(
        "Refresh runtimes".into(),
        Some(Pick::Run(Operation::Refresh)),
        None,
        !pending,
    ));
    if let Ok(handle) = open_menu(world, anchor, MenuAt::Below, &entries) {
        world.resource_mut::<Runtimes>().unwrap().menu = Some(RuntimeMenu {
            anchor,
            handle,
            session: session.into(),
            picks,
        });
        true
    } else {
        false
    }
}
/// Apply a pick from the runtime menu. Picks for a conversation no longer
/// shown are dropped rather than applied to whichever is current now.
pub fn menu_events(world: &mut World, current: Option<&str>) {
    for event in events::<MenuPicked>(world).to_vec() {
        let Some(state) = world.resource_mut::<Runtimes>() else {
            return;
        };
        if state
            .menu
            .as_ref()
            .is_none_or(|m| m.handle.root != event.menu)
        {
            continue;
        }
        let mut menu = state.menu.take().unwrap();
        if current != Some(menu.session.as_str()) {
            continue;
        }
        match menu.picks.remove(&event.item) {
            Some(Pick::Run(operation)) => {
                queue(world, &menu.session, operation);
            }
            Some(Pick::Setup) => crate::settings::show_packages(world),
            None => {}
        }
    }
    let closed = world
        .resource::<Runtimes>()
        .and_then(|r| r.menu.as_ref())
        .is_some_and(|m| !m.handle.is_open(world));
    if closed {
        world.resource_mut::<Runtimes>().unwrap().menu = None;
    }
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
    let mut wire = command.clone();
    wire.session = crate::routing::wire_owned(&wire.session);
    let result = execute_inner(client, &wire, &mut may_have_mutated).await;
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
    let reopen = world.resource::<Runtimes>().and_then(|state| {
        let menu = state.menu.as_ref()?;
        (menu.session == update.command.session
            && state.visible.as_deref() == Some(menu.session.as_str())
            && menu.handle.is_open(world))
        .then(|| (menu.anchor, menu.session.clone()))
    });
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
    if let Some((anchor, session)) = reopen {
        open(world, anchor, &session);
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
