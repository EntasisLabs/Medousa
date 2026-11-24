//! Settings, after medousa-home's: the same sections in the same groups. Look
//! switches the theme live and saves it where medousa-home reads it;
//! Connection shows the active workshop, the engine and this app. Parts a
//! section doesn't do yet point to the original app.
use medousa_desktop_host::packages::{Catalog, Progress};
use medousa_sdk::MedousaClient;
use medousa_types::HealthResponse;
use rndr_core::*;
use rndr_ui::controls::HostIntent;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[path = "settings_agent.rs"]
pub(crate) mod agent;
#[path = "settings_channels.rs"]
pub(crate) mod channels;
#[path = "settings_external.rs"]
pub(crate) mod external;
#[path = "settings_providers.rs"]
pub(crate) mod providers;
#[path = "settings_runtime.rs"]
pub(crate) mod runtime;
#[path = "settings_sharing.rs"]
pub(crate) mod sharing;
#[path = "settings_view.rs"]
mod view;

static NEXT: AtomicU64 = AtomicU64::new(0x5345_1000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// ⌘, opens Settings, as on every Mac app.
const OPEN: &str = "settings.open";

/// medousa-home's sections, in its order and groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Preferences,
    Accounts,
    Agent,
    Runtime,
    Sharing,
    ExternalAgents,
    Packages,
    Mcp,
    Connection,
}

impl Section {
    pub(crate) const ALL: [Section; 9] = [
        Section::Preferences,
        Section::Accounts,
        Section::Agent,
        Section::Runtime,
        Section::Sharing,
        Section::ExternalAgents,
        Section::Packages,
        Section::Mcp,
        Section::Connection,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Section::Preferences => "Preferences",
            Section::Accounts => "Accounts",
            Section::Agent => "Medousa Agent",
            Section::Runtime => "Runtime Controls",
            Section::Sharing => "Sharing",
            Section::ExternalAgents => "External Agents",
            Section::Packages => "Packages",
            Section::Mcp => "MCP",
            Section::Connection => "Connection",
        }
    }
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Section::Preferences => "Look, notifications & chrome",
            Section::Accounts => "Connected services & review accounts",
            Section::Agent => "Stance, memory & models",
            Section::Runtime => "Reach, shell, engine & versions",
            Section::Sharing => "Seats, phone, peers & channels",
            Section::ExternalAgents => "External coding agents & connected conversations",
            Section::Packages => "Channel adapters & installs",
            Section::Mcp => "Gateway & tool servers",
            Section::Connection => "Active workshop, engine & files",
        }
    }
    /// medousa-home's quiet groups: Medousa-wide, then this machine.
    pub(crate) fn machine(self) -> bool {
        matches!(self, Section::Packages | Section::Mcp | Section::Connection)
    }
}

#[derive(Clone, Debug)]
enum Action {
    Section(Section),
    Theme(String),
    Refresh,
    Install(String),
    Remove(String),
    OpenInstaller,
    Agent(agent::Patch),
    /// Opens or closes a disclosure: a picker or a "more" group.
    Toggle(&'static str),
    McpEnable(String, bool),
    /// First press asks; a second on the same server removes it.
    McpRemove(String),
    McpRestart,
    /// Starts a device sign-in with a review provider on its first host.
    SignIn(String, String),
    OpenSignIn,
    CopyCode,
    CancelSignIn,
    RetrySignIn,
    Disconnect(String),
    /// Sets up an external agent by account: install, sign in, sign out.
    AgentInstall(&'static str),
    AgentSignIn(&'static str),
    AgentSignOut(&'static str),
    OpenChat,
    /// First press asks; a second on the same conversation removes it.
    ConversationRemove(String),
    CancelRemove,
    ConversationRotate(String),
    CopyCallbackKey,
    DismissCallbackKey,
    /// Leaves agent shell tools off after the empty-allowlist warning.
    KeepShellOff,
    SharedMode(bool),
    /// Shows or hides the pairing QR, fetching it when shown.
    PairingQr,
    RotateInvite,
    CopyInvite,
    /// First press asks; a second on the same device removes it.
    DeviceRemove(String),
    CancelDeviceRemove,
    LanPairing(bool),
    /// Opens or closes a channel's form, filling it from what's saved.
    Channel(&'static str),
    ChannelHeartbeat(&'static str, bool),
    /// Marks a kept token to be forgotten on the next save, or not.
    ForgetToken(&'static str),
    SaveChannel(&'static str),
    OpenGuide(&'static str),
    /// Opens or closes a provider's key form.
    Provider(String),
    SaveProvider,
    /// First press asks; a second clears the open provider's key.
    ClearProviderKey,
    KeepProviderKey,
    /// Saves a runtime list field from its draft: field, draft key.
    SaveList(&'static str, &'static str),
    /// Sets the env override typed in the Add form.
    EnvAdd,
    /// First press asks; a second removes that variable.
    EnvRemove(String),
    KeepEnv,
}

/// The active workshop as the registry describes it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Workshop {
    pub label: String,
    pub kind: String,
    pub url: String,
    pub data_dir: Option<String>,
}

pub struct Command {
    serial: u64,
    kind: Kind,
}
enum Kind {
    Health,
    Workshop,
    SaveTheme(String),
    Packages,
    /// Installs a package, writing progress where the view reads it.
    Install(String, Arc<Mutex<Option<Progress>>>),
    Remove(String),
    OpenInstaller,
    Defaults,
    Patch(agent::Patch),
    McpStatus,
    McpEnable(String, bool),
    McpRemove(String),
    McpRestart,
    Accounts,
    Auth(medousa_types::ReviewProviderAuthRequest),
    Disconnect(String),
    OpenUrl(String),
    Agents,
    AgentInstall(String),
    AgentSignIn(String),
    AgentSignOut(String),
    ConversationRemove(String),
    ConversationRotate(String),
    /// Shared mode, paired devices and, given this Mac's workshop URL, LAN
    /// pairing.
    Sharing(Option<String>),
    SetShared(bool),
    Qr,
    RotateInvite,
    DeviceRemove(String),
    Lan(String, bool),
    Channels,
    SaveChannel(Box<channels::Plan>),
    /// Provider keys; whether the custom provider's id can be read here.
    Providers(bool),
    SaveProvider(Box<providers::Plan>, bool),
    ClearProviderKey(String, bool),
}
pub struct Update {
    serial: u64,
    result: Result<Reply, String>,
}
enum Reply {
    Health(Box<HealthResponse>),
    Workshop(Option<Workshop>),
    Saved,
    Packages(Catalog),
    Done,
    Defaults(serde_json::Value),
    Mcp(Box<medousa_desktop_host::mcp::Status>),
    /// A gateway change went through, with its message.
    McpDone(String),
    Accounts(Box<Accounts>),
    Auth(medousa_types::ReviewProviderAuthResponse),
    Agents(Box<external::Agents>),
    /// An agent setup step went through, with what to tell the user.
    AgentNote(String),
    /// A sign-in started; its CLI is waited for.
    SigningIn(String),
    CallbackKey(String),
    Sharing(Box<sharing::Read>),
    Shared(sharing::SharedMode),
    Qr(Box<sharing::Qr>),
    Lan(serde_json::Value),
    Channels(Box<medousa_desktop_host::channels::Summary>),
    Providers(Box<providers::Read>),
}

/// Waiting for an agent's CLI to report it signed in.
#[derive(Clone, Debug)]
pub(crate) struct Waiting {
    pub account: &'static str,
    pub until: std::time::Instant,
    pub next: std::time::Instant,
}

/// How often, and how long, a sign-in is waited for (medousa-home's).
const WAIT_EVERY: std::time::Duration = std::time::Duration::from_millis(2500);
const WAIT_FOR: std::time::Duration = std::time::Duration::from_secs(180);
/// While the pairing QR shows, paired devices are checked this often so a
/// new phone appears, as medousa-home polls.
const PAIR_POLL: std::time::Duration = std::time::Duration::from_secs(3);

/// Review providers that sign in with a device code, and the connected
/// accounts on this workshop.
#[derive(Clone, Debug, Default)]
pub(crate) struct Accounts {
    pub providers: Vec<medousa_types::ReviewProviderDescriptor>,
    pub connections: Vec<medousa_types::IntegrationConnection>,
}

/// A device sign-in in progress.
#[derive(Clone, Debug)]
pub(crate) struct SignIn {
    pub provider: String,
    pub flow: String,
    pub code: String,
    pub url: String,
    pub interval: u64,
    /// The last check failed; checking waits for Retry.
    pub paused: bool,
    pub copied: bool,
    pub next: std::time::Instant,
}

pub(crate) struct State {
    root: Entity,
    nav: Entity,
    body: Entity,
    section: Section,
    theme: String,
    health: Option<HealthResponse>,
    workshop: Option<Workshop>,
    /// Reads in flight, and the last save's outcome.
    reading: Vec<u64>,
    saving: Option<u64>,
    message: String,
    packages: Option<Catalog>,
    /// The catalog read, and the install, removal or installer launch in
    /// flight with the package it is for.
    listing: Option<u64>,
    busy: Option<(u64, String)>,
    progress: Arc<Mutex<Option<Progress>>>,
    /// Frames kept running while an install reports progress.
    animating: Option<bool>,
    /// The workshop's runtime defaults, their read, and a change in flight.
    defaults: Option<serde_json::Value>,
    defaults_reading: Option<u64>,
    patching: Option<(u64, agent::Patch)>,
    /// Agent shell tools were asked on with no binary allowlist; a second
    /// press turns them on anyway.
    shell_confirm: bool,
    /// The MCP gateway, its read, a change in flight, the server a Remove
    /// is waiting to be confirmed for, and edits that need a restart.
    mcp: Option<medousa_desktop_host::mcp::Status>,
    mcp_reading: Option<u64>,
    /// The change in flight, and whether it is a restart.
    mcp_busy: Option<(u64, bool)>,
    mcp_confirm: Option<String>,
    mcp_restart_needed: bool,
    accounts: Option<Accounts>,
    accounts_reading: Option<u64>,
    /// A begin, check, cancel or disconnect in flight, and for whom.
    account_busy: Option<(u64, String)>,
    sign_in: Option<SignIn>,
    notice: String,
    /// External agents and conversations, their read, the setup step or
    /// conversation change in flight (and for whom), a sign-in being waited
    /// for, the last step's note, the conversation a Remove waits on, and a
    /// callback key shown once.
    agents: Option<external::Agents>,
    agents_reading: Option<u64>,
    agent_busy: Option<(u64, String)>,
    /// Sharing: what was read, the read and the change in flight, the
    /// device a Remove waits on, the pairing QR and its drawing.
    shared: Option<sharing::SharedMode>,
    shared_note: String,
    devices: Option<Vec<sharing::Device>>,
    lan: Option<Result<sharing::Lan, String>>,
    sharing_reading: Option<u64>,
    sharing_busy: Option<u64>,
    sharing_note: String,
    device_confirm: Option<String>,
    qr: Option<sharing::Qr>,
    qr_reading: Option<u64>,
    /// The image id the QR is drawn under, bumped for each new QR, and the
    /// one uploaded last.
    qr_image: u64,
    qr_loaded: u64,
    /// The countdown shown, and when paired devices are next checked.
    qr_secs: u64,
    pair_poll: Option<std::time::Instant>,
    /// Channels on this Mac: what's saved, its read, the save in flight and
    /// for which channel, and that save's outcome.
    channels: Option<medousa_desktop_host::channels::Summary>,
    channels_reading: Option<u64>,
    channel_busy: Option<(u64, &'static str)>,
    channel_note: Option<(&'static str, String)>,
    /// Open forms' text by field key, heartbeat switches by channel, and
    /// kept tokens marked to forget. Drafts outlive the page's rebuilds.
    drafts: HashMap<&'static str, String>,
    heartbeat: HashMap<&'static str, bool>,
    forget: HashSet<&'static str>,
    /// Fields drawn this time, and the one that had focus with its caret.
    fields: Vec<(&'static str, Entity)>,
    /// Provider keys: what the engine keeps, its read, a change in flight
    /// with what to say when it lands, the open form, its note, and a
    /// Clear key waiting to be confirmed.
    providers: Option<providers::Read>,
    providers_reading: Option<u64>,
    provider_busy: Option<(u64, &'static str)>,
    provider_open: Option<String>,
    provider_note: String,
    provider_clear: bool,
    /// Host env overrides: a variable a Remove waits on, and the last note.
    env_confirm: Option<String>,
    env_note: String,
    focused: Option<(&'static str, Caret)>,
    waiting: Option<Waiting>,
    agent_note: String,
    conversation_confirm: Option<String>,
    callback_key: Option<(String, String)>,
    /// Disclosures open on this visit, by key.
    open: HashSet<&'static str>,
    actions: HashMap<u64, Action>,
    queue: Vec<Command>,
    redraw: bool,
    started: bool,
}

struct Host(Option<State>);
fn state(w: &World) -> Option<&State> {
    w.resource::<Host>().and_then(|h| h.0.as_ref())
}
fn state_mut(w: &mut World) -> Option<&mut State> {
    w.resource_mut::<Host>().and_then(|h| h.0.as_mut())
}
fn with_state<R>(w: &mut World, f: impl FnOnce(&mut World, &mut State) -> R) -> Option<R> {
    let mut s = w.resource_mut::<Host>()?.0.take()?;
    let out = f(w, &mut s);
    if let Some(host) = w.resource_mut::<Host>() {
        host.0 = Some(s);
    }
    Some(out)
}

impl State {
    fn enqueue(&mut self, kind: Kind) -> u64 {
        let serial = next();
        self.queue.push(Command { serial, kind });
        serial
    }
    fn list_packages(&mut self) {
        if self.listing.is_none() {
            self.listing = Some(self.enqueue(Kind::Packages));
            self.redraw = true;
        }
    }
    fn read_sharing(&mut self) {
        if self.sharing_reading.is_none() {
            let lan = self
                .local()
                .then(|| self.workshop.as_ref().map(|w| w.url.clone()))
                .flatten();
            self.sharing_reading = Some(self.enqueue(Kind::Sharing(lan)));
            self.redraw = true;
        }
    }
    /// A channel's settings and adapters live where the workshop runs.
    fn read_channels(&mut self) {
        if self.channels_reading.is_none() && self.local() {
            self.channels_reading = Some(self.enqueue(Kind::Channels));
            self.redraw = true;
        }
    }
    fn read_providers(&mut self) {
        if self.providers_reading.is_none() {
            self.providers_reading = Some(self.enqueue(Kind::Providers(self.local())));
            self.redraw = true;
        }
    }
    /// Drops a form's text, and its fields so a rebuild doesn't keep it.
    fn forget_fields(&mut self, keys: &[&'static str]) {
        self.fields.retain(|(k, _)| !keys.contains(k));
        for k in keys {
            self.drafts.remove(k);
        }
    }
    /// Fills an open form from what's saved, once.
    fn seed_channel(&mut self, id: &'static str) {
        let Some(summary) = &self.channels else {
            return;
        };
        let (fields, heartbeat) = channels::fill(summary, id);
        for (k, v) in fields {
            self.drafts.entry(k).or_insert(v);
        }
        self.heartbeat.entry(id).or_insert(heartbeat);
    }
    fn read_qr(&mut self) {
        if self.qr_reading.is_none() {
            self.qr_reading = Some(self.enqueue(Kind::Qr));
            self.redraw = true;
        }
    }
    fn read_agents(&mut self) {
        if self.agents_reading.is_none() {
            self.agents_reading = Some(self.enqueue(Kind::Agents));
            self.redraw = true;
        }
    }
    /// Setup runs the vendor's CLI here, so only for a workshop on this
    /// machine: a local one, or none registered (the built-in engine).
    pub(crate) fn local(&self) -> bool {
        self.started
            && self.reading.is_empty()
            && self.workshop.as_ref().is_none_or(|w| w.kind == "local")
    }
    fn agent_step(&mut self, account: &'static str, kind: Kind) {
        if self.agent_busy.is_none() && self.local() {
            self.message.clear();
            self.agent_note.clear();
            self.waiting = None;
            let serial = self.enqueue(kind);
            self.agent_busy = Some((serial, account.to_string()));
        }
    }
    fn refresh(&mut self) {
        let health = self.enqueue(Kind::Health);
        let workshop = self.enqueue(Kind::Workshop);
        self.reading = vec![health, workshop];
        self.redraw = true;
    }
}

pub fn mount(w: &mut World, parent: Entity) {
    let root = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    },));
    set_parent(w, root, parent);
    crate::native_style::flat(w, root, crate::native_style::canvas());
    let (nav, body) = view::mount(w, root);
    let theme = if cfg!(test) {
        crate::theme::default_id()
    } else {
        crate::theme::active_id()
    };
    w.insert_resource(Host(Some(State {
        root,
        nav,
        body,
        section: Section::Preferences,
        theme,
        health: None,
        workshop: None,
        reading: vec![],
        saving: None,
        message: String::new(),
        packages: None,
        listing: None,
        busy: None,
        progress: Arc::new(Mutex::new(None)),
        animating: None,
        defaults: None,
        defaults_reading: None,
        patching: None,
        shell_confirm: false,
        open: HashSet::new(),
        mcp: None,
        mcp_reading: None,
        mcp_busy: None,
        mcp_confirm: None,
        mcp_restart_needed: false,
        accounts: None,
        accounts_reading: None,
        account_busy: None,
        sign_in: None,
        notice: String::new(),
        agents: None,
        agents_reading: None,
        agent_busy: None,
        shared: None,
        shared_note: String::new(),
        devices: None,
        lan: None,
        sharing_reading: None,
        sharing_busy: None,
        sharing_note: String::new(),
        device_confirm: None,
        qr: None,
        qr_reading: None,
        qr_image: 0x7172_0000_0000,
        qr_loaded: 0,
        qr_secs: 0,
        pair_poll: None,
        channels: None,
        channels_reading: None,
        channel_busy: None,
        channel_note: None,
        drafts: HashMap::new(),
        heartbeat: HashMap::new(),
        forget: HashSet::new(),
        fields: Vec::new(),
        providers: None,
        providers_reading: None,
        provider_busy: None,
        provider_open: None,
        provider_note: String::new(),
        provider_clear: false,
        env_confirm: None,
        env_note: String::new(),
        focused: None,
        waiting: None,
        agent_note: String::new(),
        conversation_confirm: None,
        callback_key: None,
        actions: HashMap::new(),
        queue: vec![],
        redraw: true,
        started: false,
    })));
    w.resource_mut::<Keymap>()
        .unwrap()
        .bind(KeyBinding::new(OPEN, [Keystroke::character(',').primary()]));
}

pub fn activate(w: &mut World) {
    if let Some(s) = state_mut(w) {
        if !s.started {
            s.started = true;
            s.refresh();
        }
        s.redraw = true;
    }
}

pub fn can_deactivate(_: &World) -> bool {
    true
}

pub fn action(w: &mut World, host: &HostIntent) -> bool {
    let HostIntent::Command(id) = host else {
        return false;
    };
    let Some(a) = state(w).and_then(|s| s.actions.get(id).cloned()) else {
        return false;
    };
    with_state(w, |w, s| {
        capture(w, s);
        let Some(a) = runtime_edit(s, a) else {
            s.redraw = true;
            return;
        };
        match a {
            Action::Section(section) => {
                s.shell_confirm = false;
                if section == Section::Connection && s.reading.is_empty() {
                    s.refresh();
                }
                if section == Section::Packages && s.packages.is_none() && s.listing.is_none() {
                    s.list_packages();
                }
                if section == Section::Accounts && s.accounts_reading.is_none() {
                    s.accounts_reading = Some(s.enqueue(Kind::Accounts));
                }
                if section == Section::Mcp && s.mcp_reading.is_none() {
                    s.mcp_reading = Some(s.enqueue(Kind::McpStatus));
                }
                if section == Section::Sharing {
                    s.device_confirm = None;
                    s.read_sharing();
                    s.read_channels();
                    // LAN pairing is offered by where the workshop runs.
                    if s.workshop.is_none() && s.reading.is_empty() {
                        s.refresh();
                    }
                }
                if section == Section::ExternalAgents {
                    s.read_agents();
                    // Setup is offered by where the workshop runs.
                    if s.workshop.is_none() && s.reading.is_empty() {
                        s.refresh();
                    }
                }
                if section == Section::Agent {
                    s.provider_clear = false;
                    s.read_providers();
                }
                if matches!(section, Section::Agent | Section::Runtime)
                    && s.defaults_reading.is_none()
                {
                    s.defaults_reading = Some(s.enqueue(Kind::Defaults));
                }
                s.section = section;
            }
            Action::Install(id) => {
                if s.busy.is_none() {
                    *s.progress.lock().unwrap() = None;
                    s.message.clear();
                    let serial = s.enqueue(Kind::Install(id.clone(), s.progress.clone()));
                    s.busy = Some((serial, id));
                }
            }
            Action::Remove(id) => {
                if s.busy.is_none() {
                    s.message.clear();
                    let serial = s.enqueue(Kind::Remove(id.clone()));
                    s.busy = Some((serial, id));
                }
            }
            Action::Agent(patch) => {
                let unguarded = |s: &State| {
                    s.defaults.as_ref().is_some_and(|d| {
                        let mut d = d.clone();
                        agent::apply(&mut d, &patch);
                        runtime::shell_unguarded(&d)
                    })
                };
                if s.patching.is_none()
                    && patch == agent::Patch::Field(runtime::SHELL_TOOLS.key, true.into())
                    && !s.shell_confirm
                    && unguarded(s)
                {
                    // medousa-home asks before saving this; so does this app.
                    s.shell_confirm = true;
                } else if s.patching.is_none() {
                    s.shell_confirm = false;
                    s.message.clear();
                    // Shown at once; the record confirms it.
                    if let Some(d) = s.defaults.as_mut() {
                        agent::apply(d, &patch);
                    }
                    // A pick closes its picker, as medousa-home's do.
                    s.open.remove(match patch {
                        agent::Patch::Stance(_) => "stance",
                        agent::Patch::Depth(_) => "depth",
                        agent::Patch::Model(..) => "model",
                        // A runtime picker is keyed by its field.
                        agent::Patch::Field(key, _) => key,
                    });
                    let serial = s.enqueue(Kind::Patch(patch.clone()));
                    s.patching = Some((serial, patch));
                }
            }
            Action::McpEnable(id, enabled) => {
                if s.mcp_busy.is_none() {
                    s.message.clear();
                    s.mcp_confirm = None;
                    s.mcp_busy = Some((s.enqueue(Kind::McpEnable(id, enabled)), false));
                }
            }
            Action::McpRemove(id) => {
                if s.mcp_busy.is_none() {
                    if s.mcp_confirm.as_deref() == Some(id.as_str()) {
                        s.mcp_confirm = None;
                        s.message.clear();
                        s.mcp_busy = Some((s.enqueue(Kind::McpRemove(id)), false));
                    } else {
                        s.mcp_confirm = Some(id);
                    }
                }
            }
            Action::McpRestart => {
                if s.mcp_busy.is_none() {
                    s.message.clear();
                    s.mcp_confirm = None;
                    s.mcp_busy = Some((s.enqueue(Kind::McpRestart), true));
                }
            }
            Action::SignIn(provider, host) => {
                if s.account_busy.is_none() && s.sign_in.is_none() {
                    s.message.clear();
                    s.notice.clear();
                    let serial = s.enqueue(Kind::Auth(
                        medousa_types::ReviewProviderAuthRequest::Begin {
                            provider_id: provider.clone(),
                            host,
                            client_id: None,
                        },
                    ));
                    s.account_busy = Some((serial, provider));
                }
            }
            Action::OpenSignIn => {
                if let Some(url) = s.sign_in.as_ref().map(|f| f.url.clone()) {
                    s.enqueue(Kind::OpenUrl(url));
                }
            }
            Action::CopyCode => {
                if let Some(flow) = s.sign_in.as_mut() {
                    if let Some(clipboard) = w.resource_mut::<Clipboard>() {
                        clipboard.text = flow.code.clone();
                        clipboard.html.clear();
                        clipboard.html_text.clear();
                        clipboard.dirty = true;
                    }
                    flow.copied = true;
                }
            }
            Action::CancelSignIn => {
                if let Some(flow) = s.sign_in.take() {
                    s.account_busy = None;
                    s.enqueue(Kind::Auth(
                        medousa_types::ReviewProviderAuthRequest::Cancel { flow_id: flow.flow },
                    ));
                }
            }
            Action::RetrySignIn => {
                if let Some(flow) = s.sign_in.as_mut() {
                    flow.paused = false;
                    flow.next = std::time::Instant::now();
                }
            }
            Action::Disconnect(id) => {
                if s.account_busy.is_none() {
                    s.message.clear();
                    s.notice.clear();
                    let serial = s.enqueue(Kind::Disconnect(id.clone()));
                    s.account_busy = Some((serial, id));
                }
            }
            Action::AgentInstall(account) => {
                s.agent_step(account, Kind::AgentInstall(account.into()))
            }
            Action::AgentSignIn(account) => {
                s.agent_step(account, Kind::AgentSignIn(account.into()))
            }
            Action::AgentSignOut(account) => {
                s.agent_step(account, Kind::AgentSignOut(account.into()))
            }
            Action::OpenChat => {
                crate::surfaces::select(w, crate::surfaces::SurfaceId::Chat);
            }
            Action::ConversationRemove(id) => {
                if s.agent_busy.is_none() {
                    if s.conversation_confirm.as_deref() == Some(id.as_str()) {
                        s.conversation_confirm = None;
                        s.message.clear();
                        let serial = s.enqueue(Kind::ConversationRemove(id.clone()));
                        s.agent_busy = Some((serial, id));
                    } else {
                        s.conversation_confirm = Some(id);
                    }
                }
            }
            Action::CancelRemove => s.conversation_confirm = None,
            Action::ConversationRotate(id) => {
                if s.agent_busy.is_none() {
                    s.message.clear();
                    s.conversation_confirm = None;
                    let serial = s.enqueue(Kind::ConversationRotate(id.clone()));
                    s.agent_busy = Some((serial, id));
                }
            }
            Action::CopyCallbackKey => {
                if let Some((_, key)) = &s.callback_key
                    && let Some(clipboard) = w.resource_mut::<Clipboard>()
                {
                    clipboard.text = key.clone();
                    clipboard.html.clear();
                    clipboard.html_text.clear();
                    clipboard.dirty = true;
                    s.agent_note = "Callback key copied.".into();
                }
            }
            Action::DismissCallbackKey => s.callback_key = None,
            Action::Toggle(key) => {
                if s.open.remove(key) {
                    s.forget_fields(runtime::drafts_for(key));
                    if key == "runtime.env" {
                        s.env_confirm = None;
                        s.env_note.clear();
                    }
                } else {
                    s.open.insert(key);
                }
            }
            Action::Channel(id) => {
                let key = channels::open_key(id);
                if s.open.remove(key) {
                    // Closing drops what was typed, tokens included.
                    let keys = channels::keys(id);
                    s.fields.retain(|(k, _)| !keys.contains(k));
                    for k in keys {
                        s.drafts.remove(k);
                        s.forget.remove(k);
                    }
                    s.heartbeat.remove(id);
                } else {
                    s.open.insert(key);
                    s.seed_channel(id);
                }
                s.channel_note = None;
            }
            Action::ChannelHeartbeat(id, on) => {
                s.heartbeat.insert(id, on);
            }
            Action::ForgetToken(key) => {
                if !s.forget.remove(key) {
                    s.forget.insert(key);
                }
            }
            Action::SaveChannel(id) => {
                if s.channel_busy.is_none() && s.local() {
                    let heartbeat = s.heartbeat.get(id).copied().unwrap_or(false);
                    match channels::plan(id, &s.drafts, heartbeat, &s.forget) {
                        Ok(plan) => {
                            s.channel_note = None;
                            let serial = s.enqueue(Kind::SaveChannel(Box::new(plan)));
                            s.channel_busy = Some((serial, id));
                        }
                        Err(e) => s.channel_note = Some((id, e)),
                    }
                }
            }
            Action::OpenGuide(url) => {
                s.enqueue(Kind::OpenUrl(url.into()));
            }
            Action::Provider(id) => {
                s.forget_fields(&providers::KEYS);
                s.provider_note.clear();
                s.provider_clear = false;
                if s.provider_open.as_deref() == Some(id.as_str()) {
                    s.provider_open = None;
                } else {
                    if let (Some(read), Some(e)) = (&s.providers, providers::find(&id)) {
                        let fields = providers::fill(read, &e);
                        s.drafts.extend(fields);
                    }
                    s.provider_open = Some(id);
                }
            }
            Action::SaveProvider => {
                let open = s.provider_open.as_deref().and_then(providers::find);
                if let (None, Some(e), Some(read)) = (s.provider_busy, open, &s.providers) {
                    match providers::plan(&e, read, &s.drafts) {
                        Ok(plan) if plan == providers::Plan::default() => {
                            s.provider_note = "Nothing changed.".into();
                        }
                        Ok(plan) => {
                            s.provider_note.clear();
                            let local = s.local();
                            let serial = s.enqueue(Kind::SaveProvider(Box::new(plan), local));
                            s.provider_busy = Some((serial, "Saved to this workshop."));
                        }
                        Err(e) => s.provider_note = e,
                    }
                }
            }
            Action::ClearProviderKey => {
                let open = s.provider_open.as_deref().and_then(providers::find);
                if let (None, Some(e), Some(read)) = (s.provider_busy, open, &s.providers) {
                    if s.provider_clear {
                        let kind = read.key_kind(&e);
                        s.provider_clear = false;
                        s.provider_note.clear();
                        let local = s.local();
                        let serial = s.enqueue(Kind::ClearProviderKey(kind, local));
                        s.provider_busy = Some((serial, "Key removed."));
                    } else {
                        s.provider_clear = true;
                    }
                }
            }
            Action::KeepProviderKey => s.provider_clear = false,
            Action::KeepEnv => s.env_confirm = None,
            // Turned into field patches by `runtime_edit`.
            Action::SaveList(..) | Action::EnvAdd | Action::EnvRemove(_) => {}
            Action::OpenInstaller => {
                if s.busy.is_none() {
                    s.message.clear();
                    let serial = s.enqueue(Kind::OpenInstaller);
                    s.busy = Some((serial, String::new()));
                }
            }
            Action::Theme(id) => choose_theme(w, s, id),
            Action::KeepShellOff => s.shell_confirm = false,
            Action::SharedMode(on) => {
                if s.sharing_busy.is_none() {
                    s.sharing_note.clear();
                    s.sharing_busy = Some(s.enqueue(Kind::SetShared(on)));
                }
            }
            Action::PairingQr => {
                if s.open.remove("pair.qr") {
                    s.pair_poll = None;
                } else {
                    s.open.insert("pair.qr");
                    s.sharing_note.clear();
                    s.pair_poll = Some(std::time::Instant::now() + PAIR_POLL);
                    s.read_qr();
                }
            }
            Action::RotateInvite => {
                if s.sharing_busy.is_none() {
                    s.sharing_note.clear();
                    s.sharing_busy = Some(s.enqueue(Kind::RotateInvite));
                }
            }
            Action::CopyInvite => {
                if let Some(qr) = &s.qr
                    && let Some(clipboard) = w.resource_mut::<Clipboard>()
                {
                    clipboard.text = qr.url.clone();
                    clipboard.html.clear();
                    clipboard.html_text.clear();
                    clipboard.dirty = true;
                    s.sharing_note = "Invite link copied.".into();
                }
            }
            Action::DeviceRemove(id) => {
                if s.sharing_busy.is_none() {
                    if s.device_confirm.as_deref() == Some(id.as_str()) {
                        s.device_confirm = None;
                        s.sharing_note.clear();
                        s.sharing_busy = Some(s.enqueue(Kind::DeviceRemove(id)));
                    } else {
                        s.device_confirm = Some(id);
                    }
                }
            }
            Action::CancelDeviceRemove => s.device_confirm = None,
            Action::LanPairing(on) => {
                if s.sharing_busy.is_none()
                    && s.local()
                    && let Some(url) = s.workshop.as_ref().map(|w| w.url.clone())
                {
                    s.sharing_note.clear();
                    s.sharing_busy = Some(s.enqueue(Kind::Lan(url, on)));
                }
            }
            Action::Refresh => {
                if s.section == Section::Packages {
                    s.list_packages();
                } else if s.section == Section::ExternalAgents {
                    s.read_agents();
                } else if s.section == Section::Sharing {
                    s.read_sharing();
                    s.read_channels();
                    if s.open.contains("pair.qr") {
                        s.read_qr();
                    }
                } else if matches!(s.section, Section::Agent | Section::Runtime) {
                    if s.section == Section::Agent {
                        s.read_providers();
                    }
                    if s.defaults_reading.is_none() && s.patching.is_none() {
                        s.defaults_reading = Some(s.enqueue(Kind::Defaults));
                    }
                } else {
                    s.refresh();
                }
            }
        }
        s.redraw = true;
    });
    true
}

/// Turns a runtime list or env edit into a field patch, read from what was
/// typed; `None` when it waits on a second press or was refused.
fn runtime_edit(s: &mut State, a: Action) -> Option<Action> {
    let field = |key: &'static str, value: serde_json::Value| {
        Some(Action::Agent(agent::Patch::Field(key, value)))
    };
    let env = || {
        s.defaults
            .as_ref()
            .and_then(|d| d["env_overrides"].as_str())
            .unwrap_or_default()
            .to_string()
    };
    match a {
        Action::SaveList(key, draft) => {
            let list = runtime::split_list(s.drafts.get(draft).map_or("", String::as_str));
            field(key, list.into())
        }
        Action::EnvAdd => {
            let get = |k| s.drafts.get(k).cloned().unwrap_or_default();
            match runtime::env_set(&env(), &get("rt.env.name"), &get("rt.env.value")) {
                Ok(next) if s.patching.is_none() => {
                    // The value leaves the form and this app's memory.
                    s.forget_fields(&["rt.env.name", "rt.env.value"]);
                    s.env_note.clear();
                    field("env_overrides", next.into())
                }
                Ok(_) => None,
                Err(e) => {
                    s.env_note = e;
                    None
                }
            }
        }
        Action::EnvRemove(name) => {
            if s.env_confirm.as_deref() == Some(name.as_str()) {
                s.env_confirm = None;
                field("env_overrides", runtime::env_remove(&env(), &name).into())
            } else {
                s.env_confirm = Some(name);
                None
            }
        }
        other => Some(other),
    }
}

/// Applies `id` now and saves it for medousa-home and the next launch.
fn choose_theme(w: &mut World, s: &mut State, id: String) {
    if s.theme == id {
        return;
    }
    let medousa = crate::theme::resolve(&id);
    // Swapping the palette is enough: the appearance sync re-applies the
    // theme bridge, code and diff colors on the next frame.
    if let Some(theme) = w.resource_mut::<rndr_ui::UiTheme>() {
        theme.palette = medousa.palette.clone();
    }
    w.insert_resource(medousa.radii);
    s.theme = medousa.id;
    s.message.clear();
    s.saving = Some(s.enqueue(Kind::SaveTheme(s.theme.clone())));
}

/// ⌘, from anywhere opens Settings.
pub fn update(w: &mut World) {
    let fired = w
        .resource::<KeymapFired>()
        .is_some_and(|k| k.actions.iter().any(|a| a == OPEN));
    if fired {
        crate::surfaces::select(w, crate::surfaces::SurfaceId::Settings);
    }
}

pub fn sync(w: &mut World) {
    let active = crate::surfaces::active(w) == crate::surfaces::SurfaceId::Settings;
    with_state(w, |w, s| {
        // An install reports progress from the worker; keep frames coming
        // while it runs so the bar moves, then hand frames back.
        let installing = matches!(&s.busy, Some((_, id)) if !id.is_empty());
        match (installing, s.animating) {
            (true, None) => {
                let was = w.resource::<MotionWatch>().is_some_and(|m| m.active);
                w.insert_resource(MotionWatch { active: true });
                s.animating = Some(was);
            }
            (false, Some(was)) => {
                w.insert_resource(MotionWatch { active: was });
                s.animating = None;
            }
            _ => {}
        }
        if installing && active {
            s.redraw = true;
        }
        // A device sign-in is checked at the provider's interval until it is
        // approved, refused or cancelled.
        if let Some(flow) = s.sign_in.as_mut()
            && !flow.paused
            && s.account_busy.is_none()
            && std::time::Instant::now() >= flow.next
        {
            flow.next =
                std::time::Instant::now() + std::time::Duration::from_secs(flow.interval.max(5));
            let (provider, id) = (flow.provider.clone(), flow.flow.clone());
            let serial = s.enqueue(Kind::Auth(medousa_types::ReviewProviderAuthRequest::Poll {
                flow_id: id,
            }));
            s.account_busy = Some((serial, provider));
        }
        // After a sign-in starts, the agents are read again until its CLI
        // reports it signed in, or medousa-home's three minutes pass.
        if let Some(wait) = s.waiting.as_mut() {
            let now = std::time::Instant::now();
            if now >= wait.until {
                s.waiting = None;
                s.agent_note = "Still waiting for sign-in — finish in the browser or terminal, then press Refresh here.".into();
                s.redraw = true;
            } else if now >= wait.next && s.agents_reading.is_none() {
                wait.next = now + WAIT_EVERY;
                s.read_agents();
            }
        }
        // The pairing QR counts down and is fetched again when it expires;
        // paired devices are checked so a new phone shows up.
        if s.section == Section::Sharing && s.open.contains("pair.qr") && active {
            if let Some(qr) = &s.qr {
                let secs = sharing::seconds_left(qr.expires_at);
                if secs != s.qr_secs {
                    s.qr_secs = secs;
                    s.redraw = true;
                }
                if secs == 0 && qr.expires_at.is_some() {
                    s.read_qr();
                }
            }
            if s.pair_poll
                .is_some_and(|at| std::time::Instant::now() >= at)
            {
                s.pair_poll = Some(std::time::Instant::now() + PAIR_POLL);
                s.read_sharing();
            }
        }
        if s.redraw && active {
            view::render(w, s);
            s.redraw = false;
        }
    });
}

pub fn take_commands(w: &mut World) -> Vec<Command> {
    state_mut(w)
        .map(|s| std::mem::take(&mut s.queue))
        .unwrap_or_default()
}

/// Opens an https page in the default browser. Only https: the address
/// comes from the workshop, so nothing else is handed to the system.
fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Only https sign-in pages can be opened".into());
    }
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(url);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Starts signing an agent in. Codex tries its device sign-in first and opens
/// the verified page; otherwise, or if that isn't available, the vendor's
/// login runs in a terminal window, as in medousa-home.
fn agent_sign_in(account: &str) -> Result<Reply, String> {
    use medousa_desktop_host::external_agents as agents;
    if account == "chatgpt" {
        match agents::begin_device_login() {
            Ok(start) => {
                open_url(&start.url)?;
                return Ok(Reply::SigningIn(match start.code {
                    Some(code) => format!(
                        "Approve in the browser — enter code {code} if asked. Waiting for Codex to finish…"
                    ),
                    None => {
                        "Approve in the browser that opened. Waiting for Codex to finish…".into()
                    }
                }));
            }
            Err(e) if e.to_lowercase().contains("already signed in") => {
                return Ok(Reply::AgentNote("Already signed in.".into()));
            }
            Err(_) => {}
        }
    }
    agents::begin_terminal_login(account).map(Reply::SigningIn)
}

fn workshop() -> Result<Option<Workshop>, String> {
    use medousa_desktop_host::workshop_registry::{active_workshop, load_registry};
    let registry = load_registry()?;
    Ok(active_workshop(&registry).map(|w| Workshop {
        label: w.label.clone(),
        kind: w.kind.clone(),
        url: w.url.clone(),
        data_dir: w.data_dir.clone(),
    }))
}

pub async fn execute(client: &MedousaClient, command: Command) -> Update {
    let result = match command.kind {
        Kind::Health => client
            .health()
            .get()
            .await
            .map(|h| Reply::Health(Box::new(h)))
            .map_err(|e| e.to_string()),
        // Registry reads and writes are small but blocking file IO.
        Kind::Workshop => tokio::task::spawn_blocking(workshop)
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map(Reply::Workshop)),
        Kind::SaveTheme(id) => {
            tokio::task::spawn_blocking(move || crate::theme::save_active_id(&id))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map(|()| Reply::Saved))
        }
        Kind::Defaults => agent::read(client).await.map(Reply::Defaults),
        Kind::Accounts => {
            let providers = client
                .reviews()
                .providers()
                .await
                .map_err(|e| e.to_string());
            let connections: Result<medousa_types::IntegrationConnectionListResponse, _> = client
                .http()
                .get("/v1/integrations")
                .await
                .map_err(|e| e.to_string());
            match (providers, connections) {
                (Ok(p), Ok(c)) => Ok(Reply::Accounts(Box::new(Accounts {
                    providers: p
                        .providers
                        .into_iter()
                        .filter(|p| p.operations.iter().any(|o| o == "device_auth"))
                        .collect(),
                    connections: c.connections,
                }))),
                (Err(e), _) | (_, Err(e)) => Err(e),
            }
        }
        Kind::Auth(request) => client
            .reviews()
            .authenticate(&request)
            .await
            .map(Reply::Auth)
            .map_err(|e| e.to_string()),
        Kind::Disconnect(id) => client
            .http()
            .delete::<serde_json::Value>(&format!("/v1/integrations/{}", urlencoding::encode(&id)))
            .await
            .map(|_| Reply::Done)
            .map_err(|e| e.to_string()),
        Kind::OpenUrl(url) => tokio::task::spawn_blocking(move || open_url(&url))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map(|()| Reply::Done)),
        Kind::Agents => {
            let runtimes = client
                .agents()
                .list_runtimes()
                .await
                .map_err(|e| e.to_string());
            let conversations: Result<medousa_types::ExternalConversationListResponse, String> =
                client
                    .http()
                    .get("/v1/external-conversations")
                    .await
                    .map_err(|e| e.to_string());
            runtimes.map(|r| {
                let (conversations, conversations_error) = match conversations {
                    Ok(c) => (c.conversations, None),
                    Err(e) => (vec![], Some(e)),
                };
                Reply::Agents(Box::new(external::Agents {
                    runtimes: r.runtimes,
                    conversations,
                    conversations_error,
                }))
            })
        }
        Kind::AgentInstall(account) => tokio::task::spawn_blocking(move || {
            medousa_desktop_host::external_agents::install_cli(&account)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map(|done| Reply::AgentNote(done.detail))),
        Kind::AgentSignIn(account) => tokio::task::spawn_blocking(move || agent_sign_in(&account))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r),
        Kind::AgentSignOut(account) => tokio::task::spawn_blocking(move || {
            medousa_desktop_host::external_agents::sign_out(&account)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map(Reply::AgentNote)),
        Kind::ConversationRemove(id) => client
            .http()
            .delete::<serde_json::Value>(&format!(
                "/v1/external-conversations/{}",
                urlencoding::encode(&id)
            ))
            .await
            .map(|_| Reply::Done)
            .map_err(|e| e.to_string()),
        Kind::Sharing(lan) => {
            let shared = client
                .http()
                .get::<sharing::SharedMode>("/v1/shared-mode")
                .await
                .map_err(|e| e.to_string());
            let pairs = client
                .http()
                .get::<sharing::PairStatus>("/pair/status")
                .await
                .map_err(|e| e.to_string());
            let lan = match lan {
                Some(url) => Some(medousa_desktop_host::peer_lan::status(&url).await),
                None => None,
            };
            Ok(Reply::Sharing(Box::new(sharing::Read {
                shared,
                pairs,
                lan,
            })))
        }
        Kind::SetShared(on) => client
            .http()
            .put::<sharing::SharedMode, _>(
                "/v1/shared-mode",
                &serde_json::json!({ "mode": if on { "shared" } else { "personal" } }),
            )
            .await
            .map(Reply::Shared)
            .map_err(|e| e.to_string()),
        Kind::Qr => client
            .http()
            .get::<serde_json::Value>("/qr/image")
            .await
            .map_err(|e| e.to_string())
            .and_then(sharing::decode_qr)
            .map(|q| Reply::Qr(Box::new(q))),
        Kind::RotateInvite => client
            .http()
            .post::<serde_json::Value, _>("/qr/rotate", &serde_json::json!({}))
            .await
            .map(|_| Reply::Done)
            .map_err(|e| e.to_string()),
        Kind::DeviceRemove(id) => client
            .http()
            .delete::<serde_json::Value>(&format!("/pair/{}", urlencoding::encode(&id)))
            .await
            .map(|_| Reply::Done)
            .map_err(|e| e.to_string()),
        Kind::Lan(url, on) => medousa_desktop_host::peer_lan::set_enabled(&url, on)
            .await
            .map(Reply::Lan),
        Kind::Channels => tokio::task::spawn_blocking(medousa_desktop_host::channels::summary)
            .await
            .map(|s| Reply::Channels(Box::new(s)))
            .map_err(|e| e.to_string()),
        Kind::SaveChannel(plan) => save_channel(client, *plan).await,
        Kind::Providers(local) => read_providers(client, local).await,
        Kind::SaveProvider(plan, local) => match save_provider(client, *plan).await {
            Ok(()) => read_providers(client, local).await,
            Err(e) => Err(e),
        },
        Kind::ClearProviderKey(kind, local) => match clear_provider_key(client, &kind).await {
            Ok(()) => read_providers(client, local).await,
            Err(e) => Err(e),
        },
        Kind::ConversationRotate(id) => client
            .http()
            .post_empty::<medousa_types::RotateExternalCallbackResponse>(&format!(
                "/v1/external-conversations/{}/callback-key/rotate",
                urlencoding::encode(&id)
            ))
            .await
            .map(|r| Reply::CallbackKey(r.callback_key))
            .map_err(|e| e.to_string()),
        Kind::McpStatus => Ok(Reply::Mcp(Box::new(
            medousa_desktop_host::mcp::status(client).await,
        ))),
        Kind::McpEnable(id, enabled) => tokio::task::spawn_blocking(move || {
            medousa_desktop_host::mcp::set_server_enabled(&id, enabled)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| {
            r.map(|()| {
                Reply::McpDone(
                    if enabled {
                        "Server enabled"
                    } else {
                        "Server disabled"
                    }
                    .into(),
                )
            })
        }),
        Kind::McpRemove(id) => {
            tokio::task::spawn_blocking(move || medousa_desktop_host::mcp::remove_server(&id))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map(|()| Reply::McpDone("Server removed".into())))
        }
        Kind::McpRestart => match medousa_desktop_host::mcp::restart().await {
            Ok((message, ready)) => {
                // Medousa learns the gateway's tools once it is up.
                if ready {
                    let _ = client.capabilities().reindex().await;
                }
                Ok(Reply::McpDone(message))
            }
            Err(e) => Err(e),
        },
        Kind::Patch(patch) => agent::write(client, &patch).await.map(Reply::Defaults),
        Kind::Packages => Ok(Reply::Packages(
            medousa_desktop_host::packages::catalog().await,
        )),
        Kind::Install(id, progress) => medousa_desktop_host::packages::install(&id, |p| {
            *progress.lock().unwrap() = Some(p);
        })
        .await
        .map(|()| Reply::Done),
        Kind::Remove(id) => {
            tokio::task::spawn_blocking(move || medousa_desktop_host::packages::remove(&id))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map(|()| Reply::Done))
        }
        Kind::OpenInstaller => {
            tokio::task::spawn_blocking(medousa_desktop_host::packages::open_installer)
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map(|()| Reply::Done))
        }
    };
    Update {
        serial: command.serial,
        result,
    }
}

async fn connections(
    client: &MedousaClient,
) -> Result<Vec<medousa_types::IntegrationConnection>, String> {
    client
        .http()
        .get::<medousa_types::IntegrationConnectionListResponse>("/v1/integrations")
        .await
        .map(|l| l.connections)
        .map_err(|e| format!("The engine’s key store couldn’t be read: {e}"))
}

/// The engine's connection for `kind`, made if there is none, as it goes
/// in a route path.
async fn connection_id(client: &MedousaClient, kind: &str) -> Result<String, String> {
    let id = match connections(client)
        .await?
        .into_iter()
        .find(|c| c.kind == kind)
    {
        Some(c) => c.connection_id,
        None => {
            client
                .http()
                .post::<medousa_types::IntegrationConnection, _>(
                    "/v1/integrations",
                    &medousa_types::CreateIntegrationConnectionRequest {
                        kind: kind.into(),
                        label: None,
                        base_url: None,
                    },
                )
                .await
                .map_err(|e| format!("The engine couldn’t keep it: {e}"))?
                .connection_id
        }
    };
    Ok(urlencoding::encode(id.as_str()).into_owned())
}

fn custom_provider_path() -> std::path::PathBuf {
    medousa_desktop_host::paths::medousa_data_dir().join("custom_provider_id")
}

async fn read_providers(client: &MedousaClient, local: bool) -> Result<Reply, String> {
    let connections = connections(client).await?;
    let custom_id = if local {
        tokio::task::spawn_blocking(|| {
            std::fs::read_to_string(custom_provider_path())
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
        .await
        .map_err(|e| e.to_string())?
    } else {
        None
    };
    Ok(Reply::Providers(Box::new(providers::Read {
        connections,
        custom_id,
    })))
}

/// Keeps a provider's custom id here, then its base URL and key with the
/// engine.
async fn save_provider(client: &MedousaClient, plan: providers::Plan) -> Result<(), String> {
    if let Some(id) = plan.custom_id {
        tokio::task::spawn_blocking(move || {
            let path = custom_provider_path();
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, id)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("The custom provider id couldn’t be saved: {e}"))?;
    }
    if let Some((kind, url)) = plan.base_url {
        let id = connection_id(client, &kind).await?;
        client
            .http()
            .patch::<serde_json::Value, _>(
                &format!("/v1/integrations/{id}"),
                &medousa_types::PatchIntegrationConnectionRequest {
                    label: None,
                    // Empty returns to the default.
                    base_url: Some(url.unwrap_or_default()),
                    kind: None,
                },
            )
            .await
            .map_err(|e| format!("The base URL couldn’t be saved: {e}"))?;
    }
    if let Some((kind, key)) = plan.key {
        let id = connection_id(client, &kind).await?;
        client
            .http()
            .put::<serde_json::Value, _>(
                &format!("/v1/integrations/{id}/secrets/api_key"),
                &medousa_types::UpsertIntegrationSecretRequest { value: key },
            )
            .await
            .map_err(|e| format!("The engine couldn’t keep the key: {e}"))?;
    }
    Ok(())
}

async fn clear_provider_key(client: &MedousaClient, kind: &str) -> Result<(), String> {
    for c in connections(client).await? {
        if c.kind == kind && c.secrets.slot(medousa_types::IntegrationSecretSlot::ApiKey) {
            let id = urlencoding::encode(c.connection_id.as_str()).into_owned();
            client
                .http()
                .delete::<serde_json::Value>(&format!("/v1/integrations/{id}/secrets/api_key"))
                .await
                .map_err(|e| format!("The key couldn’t be removed: {e}"))?;
        }
    }
    Ok(())
}

/// Stores a channel's token changes with the engine, then saves its
/// settings here and starts or stops its adapter.
async fn save_channel(client: &MedousaClient, plan: channels::Plan) -> Result<Reply, String> {
    if !plan.tokens.is_empty() {
        let id = connection_id(client, plan.kind).await?;
        for (slot, value) in &plan.tokens {
            let path = format!("/v1/integrations/{id}/secrets/{slot}");
            let done = match value {
                Some(value) => {
                    client
                        .http()
                        .put::<serde_json::Value, _>(
                            &path,
                            &medousa_types::UpsertIntegrationSecretRequest {
                                value: value.clone(),
                            },
                        )
                        .await
                }
                None => client.http().delete::<serde_json::Value>(&path).await,
            };
            done.map_err(|e| format!("The engine couldn’t keep the token: {e}"))?;
        }
    }
    let url = client.base_url().to_string();
    tokio::task::spawn_blocking(move || {
        medousa_desktop_host::channels::save(&plan.save, &url)?;
        Ok(medousa_desktop_host::channels::summary())
    })
    .await
    .map_err(|e| e.to_string())?
    .map(|s| Reply::Channels(Box::new(s)))
}

/// Copies typed text into drafts and notes the focused field and its caret,
/// so the next rebuild keeps both.
fn capture(w: &World, s: &mut State) {
    let focus = w.resource::<Focus>().and_then(|f| f.entity);
    s.focused = None;
    for &(key, e) in &s.fields {
        if !w.is_alive(e) {
            continue;
        }
        if let Some(l) = w.get::<Label>(e) {
            s.drafts.insert(key, l.text.clone());
        }
        if focus == Some(e) {
            s.focused = Some((key, w.get::<Caret>(e).copied().unwrap_or_default()));
        }
    }
}

pub fn apply(w: &mut World, u: Update) {
    with_state(w, |_, s| {
        if s.providers_reading == Some(u.serial) {
            s.providers_reading = None;
            match u.result {
                Ok(Reply::Providers(read)) => {
                    // A form opened before the first read is filled now.
                    let fill = s.providers.is_none();
                    s.providers = Some(*read);
                    if fill
                        && let (Some(read), Some(e)) = (
                            &s.providers,
                            s.provider_open.as_deref().and_then(providers::find),
                        )
                    {
                        for (k, v) in providers::fill(read, &e) {
                            s.drafts.entry(k).or_insert(v);
                        }
                    }
                }
                Err(e) => s.provider_note = format!("Couldn’t read provider keys: {e}"),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if let Some((serial, done)) = s.provider_busy
            && serial == u.serial
        {
            s.provider_busy = None;
            match u.result {
                Ok(Reply::Providers(read)) => {
                    s.providers = Some(*read);
                    // The key leaves the form and this app's memory.
                    s.forget_fields(&["provider.key"]);
                    s.provider_note = done.into();
                }
                Err(e) => {
                    s.provider_note = e;
                    s.read_providers();
                }
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.channels_reading == Some(u.serial) {
            s.channels_reading = None;
            match u.result {
                Ok(Reply::Channels(summary)) => {
                    s.channels = Some(*summary);
                    for c in &channels::CHANNELS {
                        if s.open.contains(channels::open_key(c.id)) {
                            s.seed_channel(c.id);
                        }
                    }
                }
                Err(e) => s.channel_note = Some(("", format!("Couldn’t read channels: {e}"))),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if let Some((serial, id)) = s.channel_busy
            && serial == u.serial
        {
            s.channel_busy = None;
            match u.result {
                Ok(Reply::Channels(summary)) => {
                    s.channels = Some(*summary);
                    // Saved tokens leave the form and this app's memory.
                    if let Some(c) = channels::CHANNELS.iter().find(|c| c.id == id) {
                        for t in c.tokens {
                            s.fields.retain(|(k, _)| *k != t.key);
                            s.drafts.insert(t.key, String::new());
                            s.forget.remove(t.key);
                        }
                    }
                    s.channel_note =
                        Some((id, "Saved — Medousa starts this channel for you.".into()));
                }
                Err(e) => {
                    s.channel_note = Some((id, e));
                    s.read_channels();
                }
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.sharing_reading == Some(u.serial) {
            s.sharing_reading = None;
            match u.result {
                Ok(Reply::Sharing(read)) => {
                    let read = *read;
                    match read.shared {
                        Ok(m) => {
                            s.shared = Some(m);
                            s.shared_note.clear();
                        }
                        Err(e) => {
                            s.shared = None;
                            s.shared_note = sharing::missing_or(&e, "Shared mode");
                        }
                    }
                    match read.pairs {
                        Ok(p) => s.devices = Some(p.paired_devices),
                        Err(e) => {
                            s.devices = None;
                            s.sharing_note = sharing::missing_or(&e, "Phone pairing");
                        }
                    }
                    s.lan = read.lan.map(|r| r.map(|v| sharing::lan(&v)));
                }
                Err(e) => s.sharing_note = format!("Couldn’t read sharing: {e}"),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.qr_reading == Some(u.serial) {
            s.qr_reading = None;
            match u.result {
                Ok(Reply::Qr(qr)) => {
                    s.qr = Some(*qr);
                    s.qr_image += 1;
                }
                Err(e) => s.sharing_note = format!("The pairing QR isn’t ready: {e}"),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.sharing_busy == Some(u.serial) {
            s.sharing_busy = None;
            match u.result {
                Ok(Reply::Shared(m)) => {
                    s.shared = Some(m);
                    s.shared_note.clear();
                }
                Ok(Reply::Lan(v)) => {
                    let lan = sharing::lan(&v);
                    s.sharing_note = lan.message.clone();
                    s.lan = Some(Ok(lan));
                }
                Ok(Reply::Done) => {
                    // A new invite or a removed device: read both again.
                    s.read_sharing();
                    if s.open.contains("pair.qr") {
                        s.read_qr();
                    }
                }
                Err(e) => s.sharing_note = format!("That didn’t go through: {e}"),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.agents_reading == Some(u.serial) {
            s.agents_reading = None;
            match u.result {
                Ok(Reply::Agents(a)) => {
                    if let Some(wait) = &s.waiting
                        && let Some(agent) = external::by_account(wait.account)
                        && a.status(agent) == external::Status::SignedIn
                    {
                        s.waiting = None;
                        s.agent_note = format!("Signed in. {}", external::use_hint(agent));
                    }
                    s.agents = Some(*a);
                }
                Err(e) => s.message = format!("Couldn’t load external agents: {e}"),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.agent_busy
            .as_ref()
            .is_some_and(|(serial, _)| *serial == u.serial)
        {
            let (_, who) = s.agent_busy.take().unwrap();
            match u.result {
                Ok(Reply::SigningIn(note)) => {
                    if let Some(agent) = external::by_account(&who) {
                        let now = std::time::Instant::now();
                        s.waiting = Some(Waiting {
                            account: agent.account,
                            until: now + WAIT_FOR,
                            next: now + WAIT_EVERY,
                        });
                    }
                    s.agent_note = note;
                }
                Ok(Reply::AgentNote(note)) => {
                    s.agent_note = note;
                    s.read_agents();
                }
                Ok(Reply::CallbackKey(key)) => s.callback_key = Some((who, key)),
                Ok(Reply::Done) => {
                    // A conversation was removed; its key goes with it.
                    if s.callback_key.as_ref().is_some_and(|(id, _)| *id == who) {
                        s.callback_key = None;
                    }
                    s.read_agents();
                }
                Err(e) => s.message = e,
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.accounts_reading == Some(u.serial) {
            s.accounts_reading = None;
            match u.result {
                Ok(Reply::Accounts(a)) => s.accounts = Some(*a),
                Err(e) => s.message = format!("Couldn’t load accounts: {e}"),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.account_busy
            .as_ref()
            .is_some_and(|(serial, _)| *serial == u.serial)
        {
            let (_, who) = s.account_busy.take().unwrap();
            use medousa_types::ReviewProviderAuthResponse as R;
            match u.result {
                Ok(Reply::Auth(R::Pending {
                    flow_id,
                    user_code,
                    verification_uri,
                    interval,
                    ..
                })) => {
                    let first = s.sign_in.is_none();
                    let copied = s.sign_in.as_ref().is_some_and(|f| f.copied);
                    s.sign_in = Some(SignIn {
                        provider: who,
                        flow: flow_id,
                        code: user_code,
                        url: verification_uri.clone(),
                        interval,
                        paused: false,
                        copied,
                        next: std::time::Instant::now()
                            + std::time::Duration::from_secs(interval.max(5)),
                    });
                    // The provider's page opens once, as the sign-in starts.
                    if first {
                        s.enqueue(Kind::OpenUrl(verification_uri));
                    }
                }
                Ok(Reply::Auth(R::Connected { account, .. })) => {
                    if let Some(flow) = s.sign_in.take() {
                        s.enqueue(Kind::Auth(
                            medousa_types::ReviewProviderAuthRequest::Cancel { flow_id: flow.flow },
                        ));
                    }
                    s.notice = format!("Connected as {account}.");
                    s.accounts_reading = Some(s.enqueue(Kind::Accounts));
                }
                Ok(Reply::Auth(R::Cancelled)) => s.sign_in = None,
                Ok(Reply::Done) => {
                    // A disconnect: read the accounts again.
                    s.accounts_reading = Some(s.enqueue(Kind::Accounts));
                }
                Err(e) => {
                    s.message = e;
                    // A failed check waits for Retry instead of repeating.
                    if let Some(flow) = s.sign_in.as_mut() {
                        flow.paused = true;
                    }
                }
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.mcp_reading == Some(u.serial) {
            s.mcp_reading = None;
            match u.result {
                Ok(Reply::Mcp(status)) => s.mcp = Some(*status),
                Err(e) => s.message = format!("Couldn’t check MCP: {e}"),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if let Some((serial, restart)) = s.mcp_busy
            && serial == u.serial
        {
            s.mcp_busy = None;
            match u.result {
                Ok(Reply::McpDone(message)) => {
                    // A restart applies every edit; any other edit waits for one.
                    s.mcp_restart_needed = !restart;
                    s.message = message;
                }
                Err(e) => s.message = e,
                _ => {}
            }
            if s.mcp_reading.is_none() {
                s.mcp_reading = Some(s.enqueue(Kind::McpStatus));
            }
            s.redraw = true;
            return;
        }
        if s.defaults_reading == Some(u.serial) {
            s.defaults_reading = None;
            match u.result {
                Ok(Reply::Defaults(d)) if s.patching.is_none() => s.defaults = Some(d),
                Err(e) => s.message = format!("Couldn’t read the workshop’s settings: {e}"),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.patching
            .as_ref()
            .is_some_and(|(serial, _)| *serial == u.serial)
        {
            s.patching = None;
            match u.result {
                Ok(Reply::Defaults(d)) => s.defaults = Some(d),
                Err(e) => {
                    s.message = format!("That change wasn’t saved: {e}");
                    // Show what the record really holds.
                    s.defaults_reading = Some(s.enqueue(Kind::Defaults));
                }
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.listing == Some(u.serial) {
            s.listing = None;
            match u.result {
                Ok(Reply::Packages(catalog)) => s.packages = Some(catalog),
                Err(e) => s.message = format!("Couldn’t load packages: {e}"),
                _ => {}
            }
            s.redraw = true;
            return;
        }
        if s.busy
            .as_ref()
            .is_some_and(|(serial, _)| *serial == u.serial)
        {
            let (_, id) = s.busy.take().unwrap();
            *s.progress.lock().unwrap() = None;
            match u.result {
                // The catalog is read again so installed state is the disk's.
                Ok(_) if !id.is_empty() => s.list_packages(),
                Ok(_) => {}
                Err(e) => s.message = e,
            }
            s.redraw = true;
            return;
        }
        let reading = s.reading.contains(&u.serial);
        if reading {
            s.reading.retain(|r| *r != u.serial);
        } else if s.saving == Some(u.serial) {
            s.saving = None;
        } else {
            return;
        }
        match u.result {
            Ok(Reply::Health(h)) => s.health = Some(*h),
            Ok(Reply::Workshop(w)) => s.workshop = w,
            Ok(
                Reply::Saved
                | Reply::Packages(_)
                | Reply::Done
                | Reply::Defaults(_)
                | Reply::Mcp(_)
                | Reply::McpDone(_)
                | Reply::Accounts(_)
                | Reply::Auth(_)
                | Reply::Agents(_)
                | Reply::AgentNote(_)
                | Reply::SigningIn(_)
                | Reply::CallbackKey(_)
                | Reply::Sharing(_)
                | Reply::Shared(_)
                | Reply::Qr(_)
                | Reply::Lan(_)
                | Reply::Channels(_)
                | Reply::Providers(_),
            ) => {}
            Err(e) if reading => s.message = format!("Couldn’t read the connection: {e}"),
            Err(e) => {
                s.message = format!("The theme is applied here but wasn’t saved for next time: {e}")
            }
        }
        // Channels are offered once it's known where the workshop runs.
        if reading && s.reading.is_empty() && s.section == Section::Sharing {
            s.read_channels();
        }
        // The custom provider's id is read once it's known to be here.
        if reading && s.reading.is_empty() && s.section == Section::Agent && s.local() {
            s.read_providers();
        }
        s.redraw = true;
    });
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
