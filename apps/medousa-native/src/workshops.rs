//! Concurrent workshop connections and view-local destinations. There is no
//! application-wide selected daemon: chats retain their owner, and every other
//! surface captures its own binding when it queues work.
use crate::{
    auxiliary::Auxiliary,
    backend::{self, Command, Reply, Request, Update},
    routing::{Binding, SessionKey},
    surfaces::SurfaceId,
};
use medousa_desktop_host::{
    connections::{self, ConnectionPlan, Credentials},
    workshop_registry::{self as saved, WorkshopRegistry, WorkshopServer},
};
use medousa_sdk::MedousaClient;
use rndr_core::*;
use rndr_ui::{
    Typography, UiId,
    controls::{self, ButtonProps, HostIntent},
    text,
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc;

const VIEW: u64 = 0x5753_4000;
const VIEWS: [SurfaceId; 9] = [
    SurfaceId::Chat,
    SurfaceId::Notes,
    SurfaceId::Code,
    SurfaceId::Create,
    SurfaceId::Reviews,
    SurfaceId::Calendar,
    SurfaceId::Peers,
    SurfaceId::Settings,
    SurfaceId::Artifacts,
];
const PAIR: u64 = 0x5753_0002;
const RELOAD: u64 = 0x5753_0003;
const CONNECT: u64 = 0x5753_1000;
const START: u64 = 0x5753_2000;
#[derive(Default, Clone)]
struct Catalog {
    defaults: Option<medousa_types::TuiDefaults>,
    defaults_saved: bool,
    rows: Vec<(String, String)>,
    summaries: Vec<medousa_types::SessionHistorySummary>,
    cursor: Option<String>,
    models: Vec<(String, String)>,
    modes: Vec<(String, String, bool)>,
}
struct Entry {
    saved: WorkshopServer,
    owner: Option<String>,
    status: String,
    connecting: bool,
    catalog_route: u64,
}
pub struct Workshops {
    entries: Vec<Entry>,
    commands: HashMap<String, mpsc::UnboundedSender<Request>>,
    pub(crate) auxiliaries: HashMap<String, Auxiliary>,
    clients: HashMap<String, Arc<MedousaClient>>,
    catalogs: HashMap<String, Catalog>,
    pub(crate) bindings: HashMap<SurfaceId, Binding>,
    destination: Option<String>,
    // Captured once at launch. Never follows later registry selection changes.
    initial_id: Option<String>,
    restore_defaults: bool,
    source_view: SurfaceId,
    notice: String,
    backend_sender: feed::Sender<Reply>,
    setup_sender: feed::Sender<Setup>,
    _backend_feed: feed::Feed<Reply>,
    _setup_feed: feed::Feed<Setup>,
    query: String,
    preferences: crate::source_preferences::Store,
    aliases: HashMap<String, String>,
}
#[derive(Clone)]
enum Setup {
    Connected {
        id: String,
        client: Arc<MedousaClient>,
        owner: String,
        commands: mpsc::UnboundedSender<Request>,
    },
    Failed(String, String),
    Paired(Result<(), String>),
    Health(String, Result<String, String>),
}
impl Event for Setup {}
struct Toolbar {
    root: Entity,
    summary: Entity,
    body: Entity,
    dirty: bool,
    address: Option<Entity>,
    qr: Option<Entity>,
    address_draft: String,
    qr_draft: String,
    source_buttons: HashMap<u64, String>,
    pairing: bool,
}
fn panel(w: &mut World, parent: Entity, layout: Layout) -> Entity {
    let e = w.spawn((layout,));
    set_parent(w, e, parent);
    e
}
fn label(w: &mut World, p: Entity, s: &str) -> Entity {
    text(w, p, s, Typography::Caption).unwrap()
}
fn button(w: &mut World, p: Entity, id: u64, s: &str, on: bool) {
    let mut props = ButtonProps::new(s, HostIntent::Command(id));
    props.enabled = on;
    let handle = controls::Button::mount(w, p, UiId(id), props).unwrap();
    w.get_mut::<Layout>(handle.root).unwrap().width =
        Size::Px((s.chars().count() as f32 * 7. + 28.).clamp(110., 320.));
}
pub fn mount(w: &mut World, parent: Entity) {
    let previous = w
        .resource::<Toolbar>()
        .map(|t| (t.address_draft.clone(), t.qr_draft.clone(), t.pairing));
    let surface = rndr_ui::Surface::Content
        .mount(w, parent, UiId(0x5753_ffff))
        .unwrap();
    *w.get_mut::<Layout>(surface.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Hug,
        ..Layout::column()
    };
    let root = surface.content;
    *w.get_mut::<Layout>(root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Hug,
        padding: 8.,
        gap: 6.,
        ..Layout::column()
    };
    let summary = label(w, root, "Connections");
    let bar = panel(
        w,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },
    );
    label(
        w,
        bar,
        "Saved connections are available together; changes below affect only the chosen view.",
    );
    let body = panel(
        w,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::column()
        },
    );
    w.insert_resource(Toolbar {
        root,
        summary,
        body,
        dirty: true,
        address: None,
        qr: None,
        address_draft: previous.as_ref().map(|p| p.0.clone()).unwrap_or_default(),
        qr_draft: previous.as_ref().map(|p| p.1.clone()).unwrap_or_default(),
        source_buttons: HashMap::new(),
        pairing: previous.is_some_and(|p| p.2),
    });
    redraw(w);
}
fn read_registry() -> Result<WorkshopRegistry, String> {
    match std::fs::read(saved::workshops_registry_path()) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            "Saved workshop registry is invalid. No other source was substituted.".into()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(saved::default_registry()),
        Err(_) => Err("Cannot read saved workshop connections".into()),
    }
}
#[cfg(test)]
pub(crate) fn install_fixture(
    w: &mut World,
    path: std::path::PathBuf,
    sources: &[(&str, &str)],
) -> Vec<mpsc::UnboundedReceiver<Request>> {
    let (backend_feed, backend_sender) = feed::open(w, FeedConfig::queue(256));
    let (setup_feed, setup_sender) = feed::open(w, FeedConfig::queue(32));
    let mut entries = vec![];
    let mut commands = HashMap::new();
    let mut receivers = vec![];
    for (i, (owner, label)) in sources.iter().enumerate() {
        let mut saved = saved::default_personal_workshop();
        saved.id = format!("fixture-{i}");
        saved.label = (*label).into();
        entries.push(Entry {
            saved,
            owner: Some((*owner).into()),
            status: "Fixture".into(),
            connecting: false,
            catalog_route: u64::MAX - i as u64,
        });
        let (tx, rx) = mpsc::unbounded_channel();
        commands.insert((*owner).into(), tx);
        receivers.push(rx);
    }
    w.insert_resource(Workshops {
        entries,
        commands,
        auxiliaries: HashMap::new(),
        clients: HashMap::new(),
        catalogs: HashMap::new(),
        bindings: HashMap::new(),
        destination: None,
        initial_id: None,
        restore_defaults: true,
        source_view: SurfaceId::Chat,
        notice: String::new(),
        backend_sender,
        setup_sender,
        _backend_feed: backend_feed,
        _setup_feed: setup_feed,
        query: String::new(),
        preferences: crate::source_preferences::Store::new(path, None),
        aliases: HashMap::new(),
    });
    receivers
}
#[cfg(test)]
pub(crate) fn restore_fixture(w: &mut World, path: std::path::PathBuf, registry: WorkshopRegistry) {
    restore(w, path, Ok(registry));
}
#[cfg(test)]
pub(crate) fn connected_fixture(
    w: &mut World,
    id: &str,
    owner: &str,
    client: Arc<MedousaClient>,
) -> mpsc::UnboundedReceiver<Request> {
    let (commands, receiver) = mpsc::unbounded_channel();
    w.resource::<Workshops>()
        .unwrap()
        .setup_sender
        .send(Setup::Connected {
            id: id.into(),
            owner: owner.into(),
            client,
            commands,
        })
        .unwrap();
    receiver
}
#[cfg(test)]
pub(crate) fn fail_fixture(w: &mut World, id: &str) {
    w.resource::<Workshops>()
        .unwrap()
        .setup_sender
        .send(Setup::Failed(id.into(), "Fixture offline".into()))
        .unwrap();
}
#[cfg(test)]
pub(crate) fn recover_fixture(w: &mut World, owner: &str) -> mpsc::UnboundedReceiver<Request> {
    let (tx, rx) = mpsc::unbounded_channel();
    w.resource_mut::<Workshops>()
        .unwrap()
        .commands
        .insert(owner.into(), tx);
    rx
}
pub fn install(w: &mut World) {
    let path = medousa_desktop_host::paths::medousa_config_dir().join("native-sources.json");
    restore(w, path, read_registry());
    let ids: Vec<_> = w
        .resource::<Workshops>()
        .unwrap()
        .entries
        .iter()
        .map(|e| e.saved.id.clone())
        .collect();
    for id in ids {
        connect(w, &id, false);
    }
}
/// Pure configuration restore, shared with hermetic launch tests. Connection
/// effects happen only after this complete, read-only restore transaction.
fn restore(w: &mut World, path: std::path::PathBuf, registry: Result<WorkshopRegistry, String>) {
    let (backend_feed, backend_sender) = feed::open(w, FeedConfig::queue(256));
    let (setup_feed, setup_sender) = feed::open(w, FeedConfig::queue(32));
    let initial = registry.as_ref().ok().and_then(initial_id);
    let loaded = crate::source_preferences::Preferences::read(&path);
    let error = loaded.as_ref().err().cloned();
    let prefs = loaded.unwrap_or_default();
    let mut bindings = HashMap::new();
    for surface in [
        SurfaceId::Notes,
        SurfaceId::Code,
        SurfaceId::Create,
        SurfaceId::Reviews,
        SurfaceId::Calendar,
        SurfaceId::Peers,
        SurfaceId::Settings,
        SurfaceId::Artifacts,
    ] {
        if let Some(owner) = prefs.views.get(&format!("{surface:?}")) {
            bindings.insert(surface, Binding::new(owner.clone()));
        }
    }
    w.insert_resource(Workshops {
        entries: vec![],
        commands: HashMap::new(),
        auxiliaries: HashMap::new(),
        clients: HashMap::new(),
        catalogs: HashMap::new(),
        bindings,
        destination: prefs.new_chat,
        initial_id: initial,
        restore_defaults: error.is_none(),
        source_view: SurfaceId::Chat,
        notice: String::new(),
        backend_sender,
        setup_sender,
        _backend_feed: backend_feed,
        _setup_feed: setup_feed,
        query: String::new(),
        preferences: crate::source_preferences::Store::new(path, error),
        aliases: prefs.aliases,
    });
    load_registry(w, registry);
    seed_defaults(w);
}
/// The legacy selection is a migration seed, not runtime activation. Missing or
/// invalid identities stay unresolved rather than borrowing another connection.
fn initial_id(registry: &WorkshopRegistry) -> Option<String> {
    let matches: Vec<_> = registry
        .workshops
        .iter()
        .filter(|e| e.id == registry.active_workshop_id)
        .collect();
    (matches.len() == 1 && saved::is_portal_kind(&matches[0].kind)).then(|| matches[0].id.clone())
}
fn seed_defaults(w: &mut World) {
    let s = w.resource_mut::<Workshops>().unwrap();
    if !s.restore_defaults {
        return;
    }
    let Some(owner) = s
        .entries
        .iter()
        .find(|e| Some(&e.saved.id) == s.initial_id.as_ref())
        .and_then(|e| e.owner.clone())
    else {
        return;
    };
    s.destination.get_or_insert_with(|| owner.clone());
    for surface in VIEWS.into_iter().filter(|v| *v != SurfaceId::Chat) {
        s.bindings
            .entry(surface)
            .or_insert_with(|| Binding::new(owner.clone()));
    }
}
fn note(w: &mut World, s: String) {
    w.resource_mut::<Workshops>().unwrap().notice = s.clone();
    if let Some(t) = w.resource_mut::<Toolbar>() {
        t.dirty = true;
    }
}
fn reload(w: &mut World) {
    load_registry(w, read_registry());
}
fn load_registry(w: &mut World, result: Result<WorkshopRegistry, String>) {
    let registry = match result {
        Ok(r) => r,
        Err(e) => {
            note(w, e);
            return;
        }
    };
    let s = w.resource_mut::<Workshops>().unwrap();
    for saved in registry.workshops.into_iter().filter(|r| r.kind != "peer") {
        if let Some(entry) = s.entries.iter_mut().find(|e| e.saved.id == saved.id) {
            entry.saved = saved;
        } else {
            let n = s.entries.len() as u64;
            s.entries.push(Entry {
                owner: s.aliases.get(&saved.id).cloned(),
                saved,
                status: "Not connected".into(),
                connecting: false,
                catalog_route: u64::MAX - n,
            });
        }
    }
    if let Some(t) = w.resource_mut::<Toolbar>() {
        t.dirty = true;
    }
}
fn client(plan: &ConnectionPlan) -> Result<MedousaClient, String> {
    match &plan.credentials {
        Credentials::Local { data_dir } => backend::client(&plan.origin, data_dir),
        Credentials::Paired { workshop_id, .. } => {
            let config =
                medousa_desktop_host::pairing_client::load_workshop_transport_config_for_id(
                    workshop_id,
                    &plan.origin,
                )
                .ok_or("Saved workshop credentials are unavailable. Check Connection settings; no other source was substituted.")?;
            Ok(MedousaClient::with_transport(
                Arc::new(crate::saved_transport::SavedTransport(
                    config,
                    workshop_id.clone(),
                )),
                plan.origin.clone(),
            ))
        }
    }
}
#[cfg(test)]
#[path = "workshops_health_tests.rs"]
mod health_tests;
#[cfg(test)]
pub(crate) async fn authority_fixture(client: &MedousaClient) -> Result<String, String> {
    authority(client).await
}
async fn authority(client: &MedousaClient) -> Result<String, String> {
    // /health is anonymous liveness only. Identity belongs to authenticated
    // /v1/health, through the same local or pinned remote transport as all reads.
    // The SDK validates the authority and runtime contract; never synthesize an
    // owner from an origin or fall back to another source on failure.
    client
        .health()
        .get()
        .await
        .map(|health| health.runtime.authority_id.to_string())
        .map_err(|_| "Workshop runtime identity unavailable or incompatible. Check this source's connection and daemon version; no fallback was attempted.".into())
}
fn connect(w: &mut World, id: &str, start: bool) {
    let registry = match read_registry() {
        Ok(r) => r,
        Err(e) => {
            note(w, e);
            return;
        }
    };
    let plan = match connections::resolve(
        &registry,
        id,
        &medousa_desktop_host::paths::medousa_data_dir(),
    ) {
        Ok(p) => p,
        Err(e) => {
            note(w, e);
            return;
        }
    };
    let s = w.resource_mut::<Workshops>().unwrap();
    let Some(entry) = s.entries.iter_mut().find(|e| e.saved.id == id) else {
        return;
    };
    if entry.connecting {
        return;
    }
    entry.connecting = true;
    entry.status = "Connecting…".into();
    let tx = s.setup_sender.clone();
    let output = s.backend_sender.clone();
    let catalog_route = entry.catalog_route;
    let id = id.to_string();
    let expected_owner = entry.owner.clone();
    let existing = entry.owner.as_ref().and_then(|o| s.clients.get(o)).cloned();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("workshop runtime");
        runtime.block_on(async move {
            if let Some(client)=existing {
                let _=tx.send(Setup::Health(id,authority(&client).await));return;
            }
            if start {
                let can_start=plan.workshop.id==saved::PERSONAL_WORKSHOP_ID && matches!(plan.credentials,Credentials::Local{..});
                if !can_start {let _=tx.send(Setup::Failed(id,"Only the bundled Personal workshop can be started here".into()));return;}
                let Credentials::Local{ref data_dir}=plan.credentials else{unreachable!()};
                let local=medousa_desktop_host::connection::LocalConnection {origin:plan.origin.clone(),data_dir:data_dir.clone(),workshop:plan.workshop.clone(),allow_bootstrap:true};
                if let Err(e)=local.ensure_ready().await {let _=tx.send(Setup::Failed(id,e));return;}
            }
            let client=match client(&plan){Ok(c)=>Arc::new(c),Err(e)=>{let _=tx.send(Setup::Failed(id,e));return;}};
            let owner=match authority(&client).await {Ok(o)=>o,Err(e)=>{let _=tx.send(Setup::Failed(id,e));return;}};
            if expected_owner.as_ref().is_some_and(|expected|expected!=&owner){
                let _=tx.send(Setup::Failed(id,"This saved connection now advertises a different authority. Existing items remain pinned. Add the replacement as a distinct trusted workshop.".into()));return;
            }
            let (commands,rx)=mpsc::unbounded_channel();
            if tx.send(Setup::Connected{id,client:client.clone(),owner:owner.clone(),commands}).is_err(){return;}
            backend::serve(client,rx,backend::Sink::new(output,owner,catalog_route)).await;
        });
    });
    if let Some(t) = w.resource_mut::<Toolbar>() {
        t.dirty = true;
    }
}
fn bind_terminal(w: &mut World, owner: &str) {
    crate::terminal::disable(w);
    let s = w.resource::<Workshops>().unwrap();
    let Some(entry) = s.entries.iter().find(|e| e.owner.as_deref() == Some(owner)) else {
        return;
    };
    let Some(client) = s.clients.get(owner).cloned() else {
        return;
    };
    let registry = WorkshopRegistry {
        version: saved::REGISTRY_VERSION,
        active_workshop_id: String::new(),
        workshops: vec![entry.saved.clone()],
    };
    let Ok(plan) = connections::resolve(
        &registry,
        &entry.saved.id,
        &medousa_desktop_host::paths::medousa_data_dir(),
    ) else {
        return;
    };
    if let Credentials::Local { data_dir } = plan.credentials {
        crate::terminal::start(w, client, plan.origin, data_dir);
    }
}
pub(crate) fn owner(w: &World, session: Option<&str>) -> Option<String> {
    let s = w.resource::<Workshops>()?;
    if let Some(session) = session {
        SessionKey::parse(session).map(|k| k.authority)
    } else {
        s.destination.clone()
    }
}
pub(crate) fn settings_workshop(w: &World, owner: &str) -> Option<crate::settings::Workshop> {
    let entry = w
        .resource::<Workshops>()?
        .entries
        .iter()
        .find(|e| e.owner.as_deref() == Some(owner))?;
    Some(crate::settings::Workshop {
        label: entry.saved.label.clone(),
        kind: entry.saved.kind.clone(),
        url: entry.saved.url.clone(),
        data_dir: entry.saved.data_dir.clone(),
    })
}
pub(crate) fn commands(w: &World, owner: &str) -> Option<mpsc::UnboundedSender<Request>> {
    w.resource::<Workshops>()?.commands.get(owner).cloned()
}
pub(crate) fn catalog(
    w: &World,
    owner: &str,
) -> (Vec<(String, String)>, Vec<(String, String, bool)>) {
    w.resource::<Workshops>()
        .and_then(|s| s.catalogs.get(owner))
        .map(|c| (c.models.clone(), c.modes.clone()))
        .unwrap_or_default()
}
pub(crate) fn saved_chat_defaults(w: &mut World, value: &serde_json::Value) {
    // auxiliary::tick has already checked the response's captured binding/epoch.
    let Some(owner) = binding(w, SurfaceId::Settings).map(|b| b.authority) else {
        return;
    };
    let Ok(defaults) = serde_json::from_value::<medousa_types::TuiDefaults>(value.clone()) else {
        return;
    };
    let c = w
        .resource_mut::<Workshops>()
        .unwrap()
        .catalogs
        .entry(owner.clone())
        .or_default();
    c.defaults = Some(defaults.clone());
    c.defaults_saved = true;
    crate::ui::workshop_defaults(w, &owner, defaults);
}
pub(crate) fn chat_defaults(w: &World, owner: &str) -> Option<medousa_types::TuiDefaults> {
    w.resource::<Workshops>()?
        .catalogs
        .get(owner)?
        .defaults
        .clone()
}
pub(crate) fn source_label(w: &World, owner: &str) -> String {
    let Some(s) = w.resource::<Workshops>() else {
        return owner.into();
    };
    let labels: Vec<_> = s
        .entries
        .iter()
        .filter(|e| e.owner.as_deref() == Some(owner))
        .map(|e| e.saved.label.as_str())
        .collect();
    let label = labels.first().copied().unwrap_or(owner);
    let duplicate = s
        .entries
        .iter()
        .any(|e| e.saved.label == label && e.owner.as_deref() != Some(owner));
    if duplicate {
        format!(
            "{label} · {}",
            owner
                .chars()
                .rev()
                .take(8)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        )
    } else {
        label.into()
    }
}
pub(crate) fn provenance(w: &World, session: &str) -> Option<String> {
    let s = w.resource::<Workshops>()?;
    (s.entries.len() > 1).then(|| source_for_session(w, session))
}
pub(crate) fn source_for_session(w: &World, session: &str) -> String {
    SessionKey::parse(session)
        .map(|k| source_label(w, &k.authority))
        .unwrap_or_else(|| "Workshop".into())
}
pub(crate) fn binding(w: &World, surface: SurfaceId) -> Option<Binding> {
    w.resource::<Workshops>()?.bindings.get(&surface).cloned()
}
/// Bind an explicit navigation destination to the emitting daemon. Never fall
/// back to a different connected source; reuse the normal unsaved-edit guard.
pub(crate) fn navigate_binding(w: &mut World, surface: SurfaceId, owner: &str) -> bool {
    if binding(w, surface).is_some_and(|b| b.authority == owner) {
        return true;
    }
    if commands(w, owner).is_none() {
        return false;
    }
    if !crate::ui::rebind_workshop_view(w, surface) {
        note(
            w,
            "Save/discard pending edits before following a link to another workshop.".into(),
        );
        return false;
    }
    let Some(s) = w.resource_mut::<Workshops>() else {
        return false;
    };
    s.bindings
        .entry(surface)
        .and_modify(|b| b.select(owner.to_owned()))
        .or_insert_with(|| Binding::new(owner.to_owned()));
    if surface == SurfaceId::Code {
        bind_terminal(w, owner);
    }
    true
}
pub(crate) fn dispatch_list(w: &mut World, query: String, more: bool) -> bool {
    let Some(s) = w.resource_mut::<Workshops>() else {
        return false;
    };
    s.query = query.clone();
    for entry in &s.entries {
        let Some(owner) = &entry.owner else {
            continue;
        };
        let Some(tx) = s.commands.get(owner) else {
            continue;
        };
        let command = if more {
            match s.catalogs.get(owner).and_then(|c| c.cursor.clone()) {
                Some(cursor) => Command::More {
                    query: query.clone(),
                    cursor,
                },
                None => continue,
            }
        } else {
            Command::List(query.clone())
        };
        let _ = tx.send(Request {
            generation: entry.catalog_route,
            command,
        });
    }
    true
}
/// Catalogs are combined here, not by borrowing a focused conversation's client.
pub(crate) fn receive(w: &mut World, reply: &Reply) -> bool {
    if let Some(s) = w.resource_mut::<Workshops>() {
        match &reply.update {
            Update::Renamed { session, title } => {
                for catalog in s.catalogs.values_mut() {
                    for row in &mut catalog.rows {
                        if row.0 == *session {
                            row.1 = title.clone();
                        }
                    }
                }
            }
            Update::Deleted(session) => {
                for catalog in s.catalogs.values_mut() {
                    catalog.rows.retain(|r| r.0 != *session);
                    catalog.summaries.retain(|r| r.session_id != *session);
                }
            }
            _ => {}
        }
    }
    let Some(s) = w.resource::<Workshops>() else {
        return false;
    };
    let owner = s
        .entries
        .iter()
        .find(|e| e.catalog_route == reply.generation)
        .and_then(|e| e.owner.clone());
    let Some(owner) = owner else {
        return false;
    };
    let s = w.resource_mut::<Workshops>().unwrap();
    let c = s.catalogs.entry(owner.clone()).or_default();
    match &reply.update {
        Update::SessionsPage {
            query,
            rows,
            summaries,
            cursor,
            append,
        } => {
            if query != &s.query {
                return true;
            }
            if !append {
                c.rows.clear();
                c.summaries.clear();
            }
            for row in rows {
                if let Some(old) = c.rows.iter_mut().find(|r| r.0 == row.0) {
                    *old = row.clone();
                } else {
                    c.rows.push(row.clone());
                }
            }
            for summary in summaries {
                if let Some(old) = c
                    .summaries
                    .iter_mut()
                    .find(|r| r.session_id == summary.session_id)
                {
                    *old = summary.clone();
                } else {
                    c.summaries.push(summary.clone());
                }
            }
            c.cursor = cursor.clone();
        }
        Update::Defaults(defaults) => {
            // A startup GET may finish after a settings PUT accepted newer data.
            if !c.defaults_saved {
                c.defaults = Some(defaults.as_ref().clone());
            }
        }
        Update::Models(rows) => c.models = rows.clone(),
        Update::Modes(rows) => c.modes = rows.clone(),
        Update::Notice(message) | Update::Unavailable(message) | Update::Failure(message) => {
            for e in &mut s.entries {
                if e.owner.as_deref() == Some(&owner) {
                    e.status = message.clone();
                }
            }
        }
        _ => return false,
    }
    let mut seen = std::collections::HashSet::new();
    let rows = s
        .entries
        .iter()
        .filter_map(|e| e.owner.as_ref())
        .filter_map(|o| s.catalogs.get(o))
        .flat_map(|c| c.rows.clone())
        .filter(|r| seen.insert(r.0.clone()))
        .collect();
    let summaries = s
        .catalogs
        .values()
        .flat_map(|c| c.summaries.clone())
        .collect();
    let cursor = s
        .catalogs
        .values()
        .any(|c| c.cursor.is_some())
        .then(|| "per-source".into());
    let models = catalog(w, &owner);
    crate::ui::workshop_catalog(w, &owner, models.0, models.1, rows, summaries, cursor);
    if let Some(defaults) = chat_defaults(w, &owner) {
        crate::ui::workshop_defaults(w, &owner, defaults);
    }
    true
}
fn field(w: &mut World, p: Entity, name: &str, value: &str) -> Entity {
    let e = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(30.),
            padding: 6.,
            ..Layout::column()
        },
        Label::new(value),
        TextField,
        Focusable,
        Placeholder::new(name),
        Semantics {
            name: Some(name.into()),
            ..Default::default()
        },
    ));
    set_parent(w, e, p);
    e
}
fn persist(w: &World) {
    let s = w.resource::<Workshops>().unwrap();
    let prefs = crate::source_preferences::Preferences {
        aliases: s.aliases.clone(),
        views: s
            .bindings
            .iter()
            .map(|(v, b)| (format!("{v:?}"), b.authority.clone()))
            .collect(),
        new_chat: s.destination.clone(),
    };
    s.preferences.save(prefs);
}
pub fn tick(w: &mut World) {
    if w.resource::<Workshops>().is_none() {
        return;
    }
    let errors = w.resource::<Workshops>().unwrap().preferences.errors();
    for error in errors {
        note(w, error);
    }
    for setup in events::<Setup>(w).to_vec() {
        match setup {
            Setup::Connected {
                id,
                client,
                owner,
                commands,
            } => {
                let duplicate = w
                    .resource::<Workshops>()
                    .unwrap()
                    .clients
                    .contains_key(&owner);
                if !duplicate {
                    let aux = Auxiliary::start(client.clone(), w);
                    let s = w.resource_mut::<Workshops>().unwrap();
                    s.auxiliaries.insert(owner.clone(), aux);
                    s.clients.insert(owner.clone(), client);
                    s.commands.insert(owner.clone(), commands);
                }
                let s = w.resource_mut::<Workshops>().unwrap();
                if let Some(e) = s.entries.iter_mut().find(|e| e.saved.id == id) {
                    e.connecting = false;
                    e.owner = Some(owner.clone());
                    e.status = "Connected".into();
                }
                s.aliases.insert(id.clone(), owner.clone());
                seed_defaults(w);
                if !duplicate && binding(w, SurfaceId::Code).is_some_and(|b| b.authority == owner) {
                    bind_terminal(w, &owner);
                }
                persist(w);
                crate::ui::workshop_ready(w, &owner);
            }
            Setup::Failed(id, error) => {
                let s = w.resource_mut::<Workshops>().unwrap();
                if let Some(e) = s.entries.iter_mut().find(|e| e.saved.id == id) {
                    e.connecting = false;
                    e.status = error;
                }
                let is_default = s
                    .entries
                    .iter()
                    .find(|e| e.saved.id == id)
                    .is_some_and(|e| {
                        e.owner.as_ref() == s.destination.as_ref() && s.destination.is_some()
                    })
                    || (s.destination.is_none() && s.initial_id.as_deref() == Some(&id));
                if is_default {
                    crate::ui::workshop_unavailable(w);
                }
            }
            Setup::Health(id, result) => {
                let s = w.resource_mut::<Workshops>().unwrap();
                if let Some(e) = s.entries.iter_mut().find(|e| e.saved.id == id) {
                    e.connecting = false;
                    e.status=match result {Ok(owner) if e.owner.as_ref()==Some(&owner)=>"Connected".into(),Ok(_)=>"Authority changed. Existing items remain pinned; repair pairing before reconnecting.".into(),Err(e)=>e};
                }
                let query = s.query.clone();
                dispatch_list(w, query, false);
            }
            Setup::Paired(result) => {
                if let Some(t) = w.resource_mut::<Toolbar>() {
                    t.pairing = false;
                }
                note(
                    w,
                    result
                        .as_ref()
                        .err()
                        .cloned()
                        .unwrap_or_else(|| "Workshop paired. Connect its saved row below.".into()),
                );
                if result.is_ok() {
                    reload(w);
                }
            }
        }
        if let Some(t) = w.resource_mut::<Toolbar>() {
            t.dirty = true;
        }
    }
    // Text fields retain their draft across connection-status redraws.
    let fields = w.resource::<Toolbar>().map(|t| (t.address, t.qr));
    if let Some((address, qr)) = fields {
        let a = address
            .and_then(|e| w.get::<Label>(e))
            .map(|l| l.text.clone());
        let q = qr.and_then(|e| w.get::<Label>(e)).map(|l| l.text.clone());
        let t = w.resource_mut::<Toolbar>().unwrap();
        if let Some(a) = a {
            t.address_draft = a;
        }
        if let Some(q) = q {
            t.qr_draft = q;
        }
    }
    redraw(w);
}
pub(crate) fn action(w: &mut World, event: &HostIntent) -> bool {
    if w.resource::<Workshops>().is_none() {
        return false;
    }
    let HostIntent::Command(action) = event else {
        return false;
    };
    let action = *action;
    let mounted = w
        .resource::<Toolbar>()
        .is_some_and(|t| w.get::<Layout>(t.root).is_some());
    if !mounted || crate::surfaces::active(w) != SurfaceId::Settings {
        return false;
    }
    let known = [RELOAD, PAIR].contains(&action)
        || (VIEW..VIEW + VIEWS.len() as u64).contains(&action)
        || (CONNECT..CONNECT + 100).contains(&action)
        || (START..START + 100).contains(&action)
        || w.resource::<Toolbar>()
            .is_some_and(|t| t.source_buttons.contains_key(&action));
    if !known {
        return false;
    }
    if (VIEW..VIEW + VIEWS.len() as u64).contains(&action) {
        w.resource_mut::<Workshops>().unwrap().source_view = VIEWS[(action - VIEW) as usize];
        w.resource_mut::<Toolbar>().unwrap().dirty = true;
    } else if action == RELOAD {
        reload(w);
    } else if action == PAIR {
        let t = w.resource::<Toolbar>().unwrap();
        if t.pairing {
            return true;
        }
        let request = medousa_desktop_host::pairing_client::PairCompleteFromQrRequest {
            qr_url: t.qr_draft.clone(),
            daemon_url: t.address_draft.clone(),
            phone_name: Some("Medousa desktop".into()),
            role: Some("portal".into()),
        };
        let tx = w.resource::<Workshops>().unwrap().setup_sender.clone();
        w.resource_mut::<Toolbar>().unwrap().pairing = true;
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("pairing runtime");
            let result = runtime
                .block_on(
                    medousa_desktop_host::pairing_client::pair_complete_from_qr_concurrent(request),
                )
                .map(|_| ());
            let _ = tx.send(Setup::Paired(result));
        });
    } else if (CONNECT..CONNECT + 100).contains(&action) || (START..START + 100).contains(&action) {
        let start = action >= START;
        let index = (action - if start { START } else { CONNECT }) as usize;
        let id = w
            .resource::<Workshops>()
            .unwrap()
            .entries
            .get(index)
            .map(|e| e.saved.id.clone());
        if let Some(id) = id {
            connect(w, &id, start);
        }
    } else {
        let owner = w
            .resource::<Toolbar>()
            .and_then(|t| t.source_buttons.get(&action))
            .cloned();
        if let Some(owner) = owner {
            let surface = w.resource::<Workshops>().unwrap().source_view;
            let previous = binding(w, surface);
            if surface == SurfaceId::Chat {
                w.resource_mut::<Workshops>().unwrap().destination = Some(owner);
            } else if binding(w, surface).is_some_and(|b| b.authority == owner) { /* already bound */
            } else if crate::ui::rebind_workshop_view(w, surface) {
                let s = w.resource_mut::<Workshops>().unwrap();
                s.bindings
                    .entry(surface)
                    .and_modify(|b| b.select(owner.clone()))
                    .or_insert_with(|| Binding::new(owner));
            } else {
                note(w,"Save/discard pending edits and close active terminals before changing this view's source.".into());
            }
            if surface == SurfaceId::Code
                && binding(w, surface) != previous
                && let Some(binding) = binding(w, surface)
            {
                bind_terminal(w, &binding.authority);
            }
            persist(w);
            w.resource_mut::<Toolbar>().unwrap().dirty = true;
        }
    }
    redraw(w);
    true
}
fn redraw(w: &mut World) {
    if !w
        .resource::<Toolbar>()
        .is_some_and(|t| w.get::<Layout>(t.root).is_some())
    {
        return;
    }
    let surface = w.resource::<Workshops>().unwrap().source_view;
    let s = w.resource::<Workshops>().unwrap();
    let selected = if surface == SurfaceId::Chat {
        s.destination.clone()
    } else {
        s.bindings.get(&surface).map(|b| b.authority.clone())
    };
    let changeable = matches!(
        surface,
        SurfaceId::Chat
            | SurfaceId::Notes
            | SurfaceId::Code
            | SurfaceId::Create
            | SurfaceId::Reviews
            | SurfaceId::Artifacts
    ) || selected.is_none();
    let source = selected
        .as_deref()
        .map(|o| {
            format!(
                "{}{}",
                source_label(w, o),
                if s.clients.contains_key(o) {
                    ""
                } else {
                    " (offline)"
                }
            )
        })
        .unwrap_or_else(|| "Choose a workshop".into());
    let summary = if surface == SurfaceId::Chat {
        format!("New chats: {source}")
    } else {
        format!("{surface:?} source: {source} · selection belongs only to this view")
    };
    let entries: Vec<_> = s
        .entries
        .iter()
        .map(|e| {
            (
                e.saved.id.clone(),
                e.saved.label.clone(),
                e.status.clone(),
                e.connecting,
                e.owner.clone(),
            )
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let owners: Vec<_> = s
        .entries
        .iter()
        .filter_map(|e| e.owner.clone())
        .filter(|o| seen.insert(o.clone()))
        .collect();
    let summary_entity = w.resource::<Toolbar>().unwrap().summary;
    if let Some(l) = w.get_mut::<Label>(summary_entity) {
        if l.text != summary {
            l.text = summary;
            w.resource_mut::<Toolbar>().unwrap().dirty = true;
        }
    }
    if !w.resource::<Toolbar>().unwrap().dirty {
        return;
    }
    let t = w.resource_mut::<Toolbar>().unwrap();
    t.dirty = false;
    t.source_buttons.clear();
    let body = t.body;
    let a = t.address_draft.clone();
    let qr = t.qr_draft.clone();
    let pairing = t.pairing;
    t.address = None;
    t.qr = None;
    let note = w.resource::<Workshops>().unwrap().notice.clone();
    for child in children_of(w, body) {
        despawn(w, child);
    }
    let views = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 4.,
            ..Layout::column()
        },
    );
    for (i, view) in VIEWS.iter().enumerate() {
        button(
            w,
            views,
            VIEW + i as u64,
            &format!("{}{:?}", if *view == surface { "✓ " } else { "" }, view),
            true,
        );
    }
    let destinations = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    for (i, owner) in owners.iter().enumerate() {
        let id = 0x5753_3000 + i as u64;
        let name = source_label(w, owner);
        button(
            w,
            destinations,
            id,
            &format!(
                "{}{}",
                if selected.as_ref() == Some(owner) {
                    "✓ "
                } else {
                    ""
                },
                name
            ),
            changeable,
        );
        w.resource_mut::<Toolbar>()
            .unwrap()
            .source_buttons
            .insert(id, owner.clone());
    }
    if !changeable {
        label(
            w,
            body,
            "This view keeps its launch binding for the window.",
        );
    }
    label(
        w,
        body,
        "Connected sources stay concurrently available. Reconnecting one never redirects another.",
    );
    for (i, (id, name, status, busy, owner)) in entries.into_iter().enumerate() {
        let row = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 8.,
                ..Layout::row()
            },
        );
        label(w, row, &format!("{name} · {status}"));
        button(
            w,
            row,
            CONNECT + i as u64,
            if owner.is_some() {
                "Reconnect / refresh"
            } else {
                "Connect"
            },
            !busy,
        );
        if id == saved::PERSONAL_WORKSHOP_ID && owner.is_none() {
            button(w, row, START + i as u64, "Set up / start Personal", !busy);
        }
    }
    button(w, body, RELOAD, "Reload saved connections", true);
    label(
        w,
        body,
        "Add a workshop using its trusted pairing link. Address and pairing link must refer to that same workshop; no data is copied.",
    );
    let address = field(w, body, "Workshop HTTP(S) address", &a);
    let q = field(w, body, "Trusted pairing link", &qr);
    w.insert(q, Secure);
    let t = w.resource_mut::<Toolbar>().unwrap();
    t.address = Some(address);
    t.qr = Some(q);
    button(
        w,
        body,
        PAIR,
        if pairing {
            "Pairing…"
        } else {
            "Pair and save workshop"
        },
        !pairing,
    );
    if !note.is_empty() {
        label(w, body, &note);
    }
}
