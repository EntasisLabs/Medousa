//! Settings, after medousa-home's: the same sections in the same groups. Look
//! switches the theme live and saves it where medousa-home reads it;
//! Connection shows the active workshop, the engine and this app. Sections
//! not ported yet say so instead of showing a dead page.
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
    /// Ported to this app so far.
    pub(crate) fn ready(self) -> bool {
        matches!(
            self,
            Section::Preferences
                | Section::Agent
                | Section::Packages
                | Section::Mcp
                | Section::Connection
        )
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
    /// The MCP gateway, its read, a change in flight, the server a Remove
    /// is waiting to be confirmed for, and edits that need a restart.
    mcp: Option<medousa_desktop_host::mcp::Status>,
    mcp_reading: Option<u64>,
    /// The change in flight, and whether it is a restart.
    mcp_busy: Option<(u64, bool)>,
    mcp_confirm: Option<String>,
    mcp_restart_needed: bool,
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
        open: HashSet::new(),
        mcp: None,
        mcp_reading: None,
        mcp_busy: None,
        mcp_confirm: None,
        mcp_restart_needed: false,
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
        match a {
            Action::Section(section) => {
                if section == Section::Connection && s.reading.is_empty() {
                    s.refresh();
                }
                if section == Section::Packages && s.packages.is_none() && s.listing.is_none() {
                    s.list_packages();
                }
                if section == Section::Mcp && s.mcp_reading.is_none() {
                    s.mcp_reading = Some(s.enqueue(Kind::McpStatus));
                }
                if section == Section::Agent && s.defaults_reading.is_none() {
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
                if s.patching.is_none() {
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
            Action::Toggle(key) => {
                if !s.open.remove(key) {
                    s.open.insert(key);
                }
            }
            Action::OpenInstaller => {
                if s.busy.is_none() {
                    s.message.clear();
                    let serial = s.enqueue(Kind::OpenInstaller);
                    s.busy = Some((serial, String::new()));
                }
            }
            Action::Theme(id) => choose_theme(w, s, id),
            Action::Refresh => {
                if s.section == Section::Packages {
                    s.list_packages();
                } else {
                    s.refresh();
                }
            }
        }
        s.redraw = true;
    });
    true
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

pub fn apply(w: &mut World, u: Update) {
    with_state(w, |_, s| {
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
                Err(e) => s.message = format!("Couldn’t read the agent settings: {e}"),
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
                | Reply::McpDone(_),
            ) => {}
            Err(e) if reading => s.message = format!("Couldn’t read the connection: {e}"),
            Err(e) => {
                s.message = format!("The theme is applied here but wasn’t saved for next time: {e}")
            }
        }
        s.redraw = true;
    });
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
