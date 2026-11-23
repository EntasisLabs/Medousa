//! Settings, after medousa-home's: the same sections in the same groups. Look
//! switches the theme live and saves it where medousa-home reads it;
//! Connection shows the active workshop, the engine and this app. Sections
//! not ported yet say so instead of showing a dead page.
use medousa_sdk::MedousaClient;
use medousa_types::HealthResponse;
use rndr_core::*;
use rndr_ui::controls::HostIntent;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

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
        matches!(self, Section::Preferences | Section::Connection)
    }
}

#[derive(Clone, Debug)]
enum Action {
    Section(Section),
    Theme(String),
    Refresh,
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
}
pub struct Update {
    serial: u64,
    result: Result<Reply, String>,
}
enum Reply {
    Health(Box<HealthResponse>),
    Workshop(Option<Workshop>),
    Saved,
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
                s.section = section;
            }
            Action::Theme(id) => choose_theme(w, s, id),
            Action::Refresh => s.refresh(),
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
    };
    Update {
        serial: command.serial,
        result,
    }
}

pub fn apply(w: &mut World, u: Update) {
    with_state(w, |_, s| {
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
            Ok(Reply::Saved) => {}
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
