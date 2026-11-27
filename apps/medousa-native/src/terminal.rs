//! Workshop-owned human PTYs. The renderer is offline: never spawn a device shell.
//! Each attachment has an ordered bounded mailbox; dropping it only detaches.
use crate::native_style as visual;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use medousa_sdk::MedousaClient;
use rndr_core::*;
use rndr_ui::{
    Typography, UiId,
    controls::HostIntent,
    tabs::{
        TabId, TabSpec, TabStrip, TabStripEvent, TabStripEventKind, TabStripHandle, TabStripProps,
    },
    text,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, header::AUTHORIZATION},
        protocol::WebSocketConfig,
    },
};

const REFRESH: u64 = 0x5445_0001;
const RECONNECT: u64 = 0x5445_0002;
const NEW: u64 = 0x5445_0003;
const CLOSE: u64 = 0x5445_0004;
const STRIP: u64 = 0x5445_0005;
const GENERAL: u64 = 0x5445_0006;
fn next() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x5446_0000);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[derive(Clone)]
struct Wake;
impl Event for Wake {}
enum Frame {
    Input(Vec<u8>),
    Resize(u16, u16),
}
struct Open {
    tab: u64,
    // Empty means a daemon-root shell, never an implicit Forge undertaking.
    work: String,
    generation: u64,
    session: Option<String>,
    create: bool,
    fresh: bool,
    frames: mpsc::Receiver<Frame>,
    cancelled: oneshot::Receiver<()>,
}
#[derive(Debug)]
enum Fact {
    Bound { session: String, cwd: String },
    Connected { session: String, cwd: String },
    Output(Vec<u8>),
    End(String),
    Catalog(Result<(Vec<Value>, Vec<Value>), String>),
}
struct Update {
    tab: u64,
    work: String,
    generation: u64,
    fact: Fact,
}
struct Service {
    tx: mpsc::Sender<Open>,
    discover: Option<mpsc::Sender<u64>>,
    rx: Mutex<mpsc::Receiver<Update>>,
    _wake: Feed<Wake>,
}

/// Matches only human workspace shells, never Forge/task/SSH sessions.
fn human(v: &Value, work: &str) -> bool {
    (if work.is_empty() {
        v["root_kind"].as_str() == Some("scripts") && v["work_id"].is_null()
    } else {
        v["root_kind"].as_str() == Some("workspace") && v["work_id"].as_str() == Some(work)
    }) && v["argv"].as_array().is_some_and(|a| a.is_empty())
        && v["session_id"].as_str().is_some_and(|s| !s.is_empty())
}
fn active_human(v: &Value) -> bool {
    human(v, v["work_id"].as_str().unwrap_or_default()) && v["exited"].as_bool() == Some(false)
}
fn open_project(v: &Value) -> bool {
    v["id"].as_str().is_some_and(|id| !id.is_empty())
        && matches!(
            v["state"].as_str(),
            Some(
                "draft"
                    | "provisioning"
                    | "ready"
                    | "executing"
                    | "sealing"
                    | "awaiting_review"
                    | "applying_decision"
            )
        )
}
fn creation(work: &str) -> (&'static str, Value) {
    if work.is_empty() {
        ("/v1/sessions/shell", json!({"cols":80,"rows":24}))
    } else {
        (
            "/v1/sessions/workspace-shell",
            json!({"work_id":work,"cols":80,"rows":24}),
        )
    }
}
fn choose(list: &Value, work: &str, bound: Option<&str>) -> Result<Option<Value>, String> {
    let sessions = list["sessions"]
        .as_array()
        .ok_or("Malformed workshop session list")?;
    if let Some(id) = bound {
        return sessions
            .iter()
            .find(|v| v["session_id"].as_str() == Some(id))
            .filter(|v| human(v, work))
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                "Bound human shell no longer available. Use New shell explicitly.".into()
            });
    }
    let mut eligible: Vec<_> = sessions
        .iter()
        .filter(|v| human(v, work) && v["exited"].as_bool() == Some(false))
        .collect();
    // Stable selection independent of server hash-map iteration order.
    eligible.sort_by_key(|v| v["session_id"].as_str().unwrap_or_default());
    Ok(eligible.first().map(|v| (*v).clone()))
}
fn request(
    origin: &str,
    session: &str,
    token: &str,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, String> {
    let origin = medousa_desktop_host::connection::local_url(origin)?;
    let mut url = reqwest::Url::parse(origin.as_str()).map_err(|_| "Invalid workshop origin")?;
    url.set_scheme(if url.scheme() == "https" { "wss" } else { "ws" })
        .map_err(|_| "Invalid workshop scheme")?;
    url.set_path(&format!(
        "/v1/sessions/shell/{}",
        urlencoding::encode(session)
    ));
    url.set_query(None);
    url.set_fragment(None);
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|_| "Invalid terminal route")?;
    let mut auth = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| "Invalid workshop credential")?;
    auth.set_sensitive(true);
    request.headers_mut().insert(AUTHORIZATION, auth);
    Ok(request)
}

pub fn start(
    w: &mut World,
    client: Arc<MedousaClient>,
    origin: String,
    data_dir: std::path::PathBuf,
) {
    let (tx, mut opens) = mpsc::channel::<Open>(8);
    let (discover, mut discoveries) = mpsc::channel::<u64>(1);
    let (updates, rx) = mpsc::channel::<Update>(64);
    let (wake, sender) = feed::open(w, FeedConfig::latest());
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("terminal runtime");
        runtime.block_on(async move {
            let catalog_client = client.clone();
            let catalog_updates = updates.clone();
            let catalog_sender = sender.clone();
            tokio::spawn(async move {
                while let Some(generation) = discoveries.recv().await {
                    let fact = Fact::Catalog(discover_catalog(&catalog_client).await);
                    let _ = publish(
                        &catalog_updates,
                        &catalog_sender,
                        Update {
                            tab: 0,
                            work: String::new(),
                            generation,
                            fact,
                        },
                    )
                    .await;
                }
            });
            while let Some(open) = opens.recv().await {
                let client = client.clone();
                let origin = origin.clone();
                let data_dir = data_dir.clone();
                let updates = updates.clone();
                let sender = sender.clone();
                tokio::spawn(async move {
                    let work = open.work.clone();
                    let tab = open.tab;
                    let generation = open.generation;
                    let result = attach(&client, &origin, &data_dir, open, &updates, &sender).await;
                    let _ = publish(
                        &updates,
                        &sender,
                        Update {
                            tab,
                            work,
                            generation,
                            fact: Fact::End(result.err().unwrap_or_else(|| {
                                "Disconnected — reconnect explicitly. Input is not replayed.".into()
                            })),
                        },
                    )
                    .await;
                });
            }
        });
    });
    w.insert_resource(Service {
        discover: Some(discover),
        tx,
        rx: Mutex::new(rx),
        _wake: wake,
    });
    activate(w);
}
async fn publish(
    tx: &mpsc::Sender<Update>,
    wake: &feed::Sender<Wake>,
    update: Update,
) -> Result<(), String> {
    tx.send(update)
        .await
        .map_err(|_| "Terminal window closed")?;
    let _ = wake.try_send(Wake);
    Ok(())
}
async fn attach(
    client: &MedousaClient,
    origin: &str,
    data_dir: &std::path::Path,
    mut open: Open,
    tx: &mpsc::Sender<Update>,
    wake: &feed::Sender<Wake>,
) -> Result<(), String> {
    medousa_desktop_host::connection::local_url(origin)?;
    if !matches!(
        open.cancelled.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ) {
        return Ok(());
    }
    let list: Value = client.http().get("/v1/sessions/shell").await.map_err(
        |_| "Could not discover workshop shells. Check Connection and session host availability.",
    )?;
    let session = match if open.fresh {
        None
    } else {
        choose(&list, &open.work, open.session.as_deref())?
    } {
        Some(v) => v,
        None if open.create => {
            let (route, body) = creation(&open.work);
            client.http().post::<_,Value>(route,&body).await.map_err(|_|"Shell creation not confirmed. Refresh sessions to discover it; creation is never automatically retried.")?
        }
        None => return Err("No running human shell found. Use New shell explicitly.".into()),
    };
    if !matches!(
        open.cancelled.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ) {
        return Ok(());
    }
    if !human(&session, &open.work) {
        return Err("Workshop returned a different shell authority".into());
    }
    if session["exited"].as_bool() == Some(true) {
        return Err(format!(
            "Shell exited ({}). Use New shell explicitly.",
            session["exit_code"]
        ));
    }
    let id = session["session_id"]
        .as_str()
        .ok_or("Missing shell identity")?;
    // Retain exact identity even if authentication/attachment subsequently fails.
    publish(
        tx,
        wake,
        Update {
            tab: open.tab,
            work: open.work.clone(),
            generation: open.generation,
            fact: Fact::Bound {
                session: id.to_owned(),
                cwd: session["cwd"].as_str().unwrap_or_default().into(),
            },
        },
    )
    .await?;
    let secret = medousa_local_credential::load_home_local_secret(data_dir)
        .map_err(|_| "Workshop credential unavailable")?;
    let req = request(origin, id, secret.token())?;
    let (ws, _) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        connect_async_with_config(
            req,
            Some(
                WebSocketConfig::default()
                    .max_message_size(Some(1024 * 1024))
                    .max_frame_size(Some(1024 * 1024)),
            ),
            false,
        ),
    )
    .await
    .map_err(|_| "Terminal connection timed out")?
    .map_err(|_| "Terminal connection failed. Check workshop authentication and session host.")?;
    publish(
        tx,
        wake,
        Update {
            tab: open.tab,
            work: open.work.clone(),
            generation: open.generation,
            fact: Fact::Connected {
                session: id.to_owned(),
                cwd: session["cwd"].as_str().unwrap_or_default().into(),
            },
        },
    )
    .await?;
    bridge(ws, &mut open, tx, wake).await
}
async fn bridge<S>(
    mut ws: tokio_tungstenite::WebSocketStream<S>,
    open: &mut Open,
    tx: &mpsc::Sender<Update>,
    wake: &feed::Sender<Wake>,
) -> Result<(), String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        tokio::select! {
            biased;
            _ = &mut open.cancelled => return Ok(()),
            frame=open.frames.recv()=>{
                let Some(frame)=frame else {let _=ws.close(None).await;return Ok(());};
                let value=match frame {Frame::Input(bytes)=>json!({"type":"stdin","data":STANDARD.encode(bytes)}), Frame::Resize(cols,rows)=>json!({"type":"resize","cols":cols.max(2),"rows":rows.max(1)})};
                tokio::time::timeout(std::time::Duration::from_secs(10), ws.send(Message::Text(value.to_string().into()))).await.map_err(|_|"Terminal write timed out. Input will not be replayed.")?.map_err(|_|"Disconnected. Input delivery unconfirmed; it will not be replayed.")?;
            }
            message=ws.next()=>{
                let fact=match message {
                    Some(Ok(Message::Text(text)))=> match decode_text(&text)? {Some(f)=>f,None=>continue},
                    Some(Ok(Message::Binary(bytes)))=>Fact::Output(bytes.to_vec()),
                    Some(Ok(Message::Ping(bytes)))=>{ws.send(Message::Pong(bytes)).await.map_err(|_|"Terminal disconnected")?;continue;},
                    Some(Ok(Message::Pong(_)))=>continue,
                    _=>return Err("Disconnected — reconnect explicitly. Input is not replayed.".into()),
                };
                if let Fact::End(message)=fact {return Err(message);}
                publish(tx,wake,Update{tab:open.tab,work:open.work.clone(),generation:open.generation,fact}).await?;

            }
        }
    }
}
fn decode_text(text: &str) -> Result<Option<Fact>, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "Malformed terminal message")?;
    Ok(match value["type"].as_str() {
        Some("stdout") => Some(Fact::Output(
            STANDARD
                .decode(value["data"].as_str().ok_or("Missing terminal output")?)
                .map_err(|_| "Invalid terminal output encoding")?,
        )),
        Some("exit") => Some(Fact::End(format!(
            "Shell exited ({}). Use New shell explicitly.",
            value["exit_code"]
        ))),
        Some("error") => {
            return Err(format!(
                "Terminal protocol error: {}",
                value["message"].as_str().unwrap_or("unknown error")
            ));
        }
        _ => None,
    })
}

async fn discover_catalog(client: &MedousaClient) -> Result<(Vec<Value>, Vec<Value>), String> {
    let list: Value = client
        .http()
        .get("/v1/sessions/shell")
        .await
        .map_err(|_| "Could not discover workshop shells")?;
    let mut sessions: Vec<Value> = list["sessions"]
        .as_array()
        .ok_or("Malformed workshop session list")?
        .iter()
        .filter(|s| active_human(s))
        .cloned()
        .collect();
    sessions.sort_by_key(|s| s["session_id"].as_str().unwrap_or_default().to_owned());
    let mut projects = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen = std::collections::HashSet::new();
    loop {
        let mut query = vec![("limit", "256".to_owned())];
        if let Some(c) = cursor.take() {
            query.push(("cursor", c));
        }
        let value: Value = client
            .http()
            .get_query("/v1/forge/items", &query)
            .await
            .map_err(|_| "Could not discover undertakings")?;
        let items = value
            .as_array()
            .or_else(|| value["items"].as_array())
            .ok_or("Malformed undertaking list")?;
        projects.extend(
            items
                .iter()
                .filter(|p| p["id"].as_str().is_some_and(|s| !s.is_empty()))
                .cloned(),
        );
        if value["truncated"].as_bool() != Some(true) {
            break;
        }
        let c = value["next_cursor"]
            .as_str()
            .ok_or("Missing undertaking cursor")?
            .to_owned();
        if !seen.insert(c.clone()) || seen.len() > 64 {
            return Err("Undertaking discovery pagination limit".into());
        }
        cursor = Some(c);
    }
    Ok((projects, sessions))
}
struct Pane {
    entity: Entity,
    name: String,
    work: String,
    generation: u64,
    session: Option<String>,
    cwd: String,
    status: String,
    connected: bool,
    frames: Option<mpsc::Sender<Frame>>,
    cancel: Option<oneshot::Sender<()>>,
    size: Option<(u16, u16)>,
}
#[derive(Default)]
struct ViewState(Option<View>);
struct View {
    sidebar: Entity,
    header: Entity,
    tabs: Entity,
    body: Entity,
    empty: Entity,
    strip: Option<TabStripHandle>,
    panes: HashMap<u64, Pane>,
    order: Vec<u64>,
    selected: Option<u64>,
    workspace_visible: HashSet<u64>,
    work: Option<String>,
    projects: Vec<Value>,
    sessions: Vec<Value>,
    names: HashMap<String, String>,
    next_shell: usize,
    actions: HashMap<u64, Action>,
    discovery: Option<u64>,
    catalog_dirty: bool,
    status: String,
    redraw: bool,
}
#[derive(Clone)]
enum Action {
    NewWork(String),
    Attach(String, String),
}
fn column(w: &mut World, parent: Entity) -> Entity {
    let e = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(w, e, parent);
    e
}
pub fn mount_view(w: &mut World, parent: Entity) {
    crate::ui::chrome::reset_terminal_tabs(w);
    let row = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    },));
    set_parent(w, row, parent);
    let sidebar = column(w, row);
    w.get_mut::<Layout>(sidebar).unwrap().width = Size::Px(250.);
    w.insert(sidebar, Scroll::default());
    visual::navigation_frame(w, sidebar);
    w.get_mut::<Layout>(sidebar).unwrap().padding = 12.;
    w.get_mut::<Layout>(sidebar).unwrap().gap = 8.;
    w.insert(sidebar, visual::SidebarPane);
    let main = column(w, row);
    visual::flat(w, main, visual::canvas());
    let header = column(w, main);
    w.get_mut::<Layout>(header).unwrap().padding = 12.;
    w.get_mut::<Layout>(header).unwrap().gap = 6.;
    w.get_mut::<Layout>(header).unwrap().height = Size::Hug;
    let tabs = column(w, main);
    w.get_mut::<Layout>(tabs).unwrap().height = Size::Hug;
    let body = column(w, main);
    let empty = text(
        w,
        body,
        "Open a shell from the sidebar, or choose an active session.",
        Typography::Label,
    )
    .unwrap();
    w.insert_resource(ViewState(Some(View {
        sidebar,
        header,
        tabs,
        body,
        empty,
        strip: None,
        panes: HashMap::new(),
        order: Vec::new(),
        selected: None,
        workspace_visible: HashSet::new(),
        work: None,
        projects: Vec::new(),
        sessions: Vec::new(),
        names: HashMap::new(),
        next_shell: 1,
        actions: HashMap::new(),
        discovery: None,
        catalog_dirty: false,
        status: String::new(),
        redraw: true,
    })));
}
pub fn focused(w: &World) -> bool {
    w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|e| w.has::<rndr_term::Terminal>(e))
}
fn unfocus(w: &mut World, e: Entity) {
    if let Some(f) = w.resource_mut::<Focus>()
        && f.entity == Some(e)
    {
        f.entity = None;
    }
}
fn input_enabled(w: &mut World, e: Entity, enabled: bool) {
    if enabled {
        w.insert(e, Focusable);
        w.insert(e, CapturesKeys);
    } else {
        w.remove::<Focusable>(e);
        w.remove::<CapturesKeys>(e);
        unfocus(w, e);
    }
}
fn refresh(w: &World, d: &mut View) {
    if d.discovery.is_some() {
        return;
    }
    let id = next();
    if w.resource::<Service>()
        .and_then(|s| s.discover.as_ref())
        .is_some_and(|tx| tx.try_send(id).is_ok())
    {
        d.discovery = Some(id);
        d.status = "Discovering workshop shells…".into();
    } else {
        d.status = "Terminal unavailable for this workshop. Local authenticated workshop connection required.".into();
    }
    d.redraw = true;
}
pub fn activate(w: &mut World) {
    if let Some(mut d) = w.resource_mut::<ViewState>().and_then(|s| s.0.take()) {
        refresh(w, &mut d);
        w.resource_mut::<ViewState>().unwrap().0 = Some(d);
    }
}
pub fn open_work(w: &mut World, work: String) {
    if let Some(d) = w.resource_mut::<ViewState>().and_then(|s| s.0.as_mut()) {
        d.work = Some(work);
        d.redraw = true;
    }
}
/// Retained shell attachments in tab order: id, friendly title, entity, work, session.
/// Entities may be reparented; the transport and terminal emulator remain owned here.
pub fn workspace_panes(w: &World) -> Vec<(u64, String, Entity, String, Option<String>)> {
    let Some(d) = w.resource::<ViewState>().and_then(|s| s.0.as_ref()) else {
        return Vec::new();
    };
    d.order
        .iter()
        .filter_map(|id| {
            d.panes.get(id).map(|p| {
                (
                    *id,
                    p.name.clone(),
                    p.entity,
                    p.work.clone(),
                    p.session.clone(),
                )
            })
        })
        .collect()
}
pub fn workspace_selected(w: &World) -> Option<u64> {
    w.resource::<ViewState>()?.0.as_ref()?.selected
}
/// All retained terminals share this binding; source changes are guarded by can_rebind.
pub fn workspace_owner(w: &World) -> Option<String> {
    crate::workshops::binding(w, crate::surfaces::SurfaceId::Terminal).map(|b| b.authority)
}
/// Declare visible Workspace panes before tick. Pass [] when leaving Workspace.
/// The caller owns placement and must keep these entities alive when rebuilding groups.
pub fn workspace_show(w: &mut World, ids: &[u64]) {
    if let Some(d) = w.resource_mut::<ViewState>().and_then(|s| s.0.as_mut()) {
        d.workspace_visible = ids
            .iter()
            .copied()
            .filter(|id| d.panes.contains_key(id))
            .collect();
    }
}
/// Restore focused Terminal ownership without detaching or clearing emulator state.
pub fn workspace_restore(w: &mut World) {
    if let Some(d) = w.resource::<ViewState>().and_then(|s| s.0.as_ref()) {
        let body = d.body;
        let entities: Vec<_> = d.panes.values().map(|p| p.entity).collect();
        for entity in entities {
            set_parent(w, entity, body);
        }
    }
    workspace_show(w, &[]);
}
/// Explicitly create a daemon-root shell; never implicitly select an undertaking.
pub fn workspace_new(w: &mut World) -> Option<u64> {
    let mut d = w.resource_mut::<ViewState>()?.0.take()?;
    let before = d.order.len();
    new_shell(w, &mut d, String::new());
    let id = (d.order.len() > before).then_some(d.selected).flatten();
    w.resource_mut::<ViewState>().unwrap().0 = Some(d);
    id
}
/// Attach a known shell, reusing an existing attachment without reconnecting it.
/// The daemon's normal human-shell validation remains authoritative.
pub fn workspace_attach(w: &mut World, work: String, session: String) -> Option<u64> {
    if session.is_empty() {
        return None;
    }
    let mut d = w.resource_mut::<ViewState>()?.0.take()?;
    let existing = d
        .panes
        .iter()
        .find(|(_, p)| p.work == work && p.session.as_ref() == Some(&session))
        .map(|(id, _)| *id);
    let id = if let Some(id) = existing {
        d.selected = Some(id);
        d.redraw = true;
        Some(id)
    } else if d.panes.len() < 32 && w.resource::<Service>().is_some_and(|s| !s.tx.is_closed()) {
        let id = add_tab(w, &mut d, work, Some(session));
        connect(w, &mut d, id, false);
        Some(id)
    } else {
        None
    };
    w.resource_mut::<ViewState>().unwrap().0 = Some(d);
    id
}
// Retain names across refresh and detach; catalog order is not shell identity.
fn shell_name(d: &mut View, session: Option<&str>) -> String {
    if let Some(name) = session.and_then(|id| d.names.get(id)) {
        return name.clone();
    }
    let name = format!("Shell {}", d.next_shell);
    d.next_shell += 1;
    if let Some(id) = session {
        d.names.insert(id.into(), name.clone());
    }
    name
}
fn add_tab(w: &mut World, d: &mut View, work: String, session: Option<String>) -> u64 {
    if let Some(existing) = session.as_ref().and_then(|s| {
        d.panes
            .iter()
            .find(|(_, p)| p.work == work && p.session.as_ref() == Some(s))
            .map(|(id, _)| *id)
    }) {
        d.selected = Some(existing);
        d.redraw = true;
        return existing;
    }
    let name = shell_name(d, session.as_deref());
    let id = next();
    let e = column(w, d.body);
    d.panes.insert(
        id,
        Pane {
            entity: e,
            name,
            work,
            generation: 0,
            session,
            cwd: String::new(),
            status: String::new(),
            connected: false,
            frames: None,
            cancel: None,
            size: None,
        },
    );
    d.order.push(id);
    d.selected = Some(id);
    d.redraw = true;
    id
}
fn connect(w: &mut World, d: &mut View, id: u64, new: bool) {
    let Some(p) = d.panes.get_mut(&id) else {
        return;
    };
    if p.frames.is_some() {
        return;
    }
    if !new && p.session.is_none() {
        p.status = "Select an active session to attach, or use New shell explicitly.".into();
        d.redraw = true;
        return;
    }
    p.generation = next();
    p.connected = false;
    p.size = None;
    if let Err(e) = rndr_term::open_terminal(w, p.entity, rndr_term::TerminalOptions::offline()) {
        p.status = e.to_string();
        return;
    }
    input_enabled(w, p.entity, false);
    let (frames, rx) = mpsc::channel(32);
    let (cancel, cancelled) = oneshot::channel();
    let open = Open {
        tab: id,
        work: p.work.clone(),
        generation: p.generation,
        session: p.session.clone(),
        create: new,
        fresh: new,
        frames: rx,
        cancelled,
    };
    if w.resource::<Service>()
        .is_some_and(|s| s.tx.try_send(open).is_ok())
    {
        p.frames = Some(frames);
        p.cancel = Some(cancel);
        p.status = "Connecting to workshop…".into();
    } else {
        p.status = "Terminal unavailable. Check Connection; reconnect explicitly.".into();
    }
    d.redraw = true;
}
fn close(w: &mut World, d: &mut View, id: u64) {
    if let Some(p) = d.panes.remove(&id) {
        input_enabled(w, p.entity, false);
        drop(p.cancel);
        drop(p.frames);
        despawn(w, p.entity);
    }
    d.order.retain(|i| *i != id);
    d.workspace_visible.remove(&id);
    if d.selected == Some(id) {
        d.selected = d.order.last().copied();
    }
    d.redraw = true;
}
pub(crate) fn can_rebind(w: &World) -> bool {
    w.resource::<ViewState>()
        .and_then(|s| s.0.as_ref())
        .is_none_or(|d| d.panes.is_empty())
}
pub(crate) fn disable(w: &mut World) {
    let (tx, _) = mpsc::channel(1);
    let (_, rx) = mpsc::channel(1);
    let (wake, _) = feed::open(w, FeedConfig::latest());
    w.insert_resource(Service {
        tx,
        discover: None,
        rx: Mutex::new(rx),
        _wake: wake,
    });
    if let Some(d) = w.resource_mut::<ViewState>().and_then(|s| s.0.as_mut()) {
        d.discovery = None;
        d.catalog_dirty = false;
        d.projects.clear();
        d.sessions.clear();
        d.work = None;
        d.status="Terminal unavailable for this workshop. Local authenticated workshop connection required.".into();
        d.redraw = true;
    }
}
fn new_shell(w: &mut World, d: &mut View, work: String) {
    if d.panes.len() >= 32 {
        d.status = "Detach a tab before opening more terminals (32 maximum).".into();
        d.redraw = true;
    } else if w.resource::<Service>().is_some_and(|s| !s.tx.is_closed()) {
        let id = add_tab(w, d, work, None);
        connect(w, d, id, true);
    }
}
pub fn action(w: &mut World, host: &HostIntent) -> bool {
    let HostIntent::Command(id) = host else {
        return false;
    };
    let Some(mut d) = w.resource_mut::<ViewState>().and_then(|s| s.0.take()) else {
        return false;
    };
    let handled = match *id {
        REFRESH => {
            refresh(w, &mut d);
            true
        }
        GENERAL => {
            new_shell(w, &mut d, String::new());
            true
        }
        NEW => {
            let work = d.work.clone().unwrap_or_default();
            new_shell(w, &mut d, work);
            true
        }
        RECONNECT => {
            if let Some(id) = d.selected {
                connect(w, &mut d, id, false);
            }
            true
        }
        CLOSE => {
            if let Some(id) = d.selected {
                close(w, &mut d, id);
            }
            true
        }
        _ => match d.actions.get(id).cloned() {
            Some(Action::NewWork(work)) => {
                new_shell(w, &mut d, work);
                true
            }
            Some(Action::Attach(work, session)) => {
                let existing = d
                    .panes
                    .values()
                    .any(|p| p.work == work && p.session.as_deref() == Some(&session));
                if d.panes.len() >= 32 && !existing {
                    d.status = "Detach a tab before opening more terminals (32 maximum).".into();
                    d.redraw = true;
                } else {
                    let id = add_tab(w, &mut d, work, Some(session));
                    connect(w, &mut d, id, false);
                }
                true
            }
            None => false,
        },
    };
    w.resource_mut::<ViewState>().unwrap().0 = Some(d);
    handled
}
pub fn tick(w: &mut World) {
    let Some(mut d) = w.resource_mut::<ViewState>().and_then(|s| s.0.take()) else {
        return;
    };
    let active = crate::surfaces::active(w) == crate::surfaces::SurfaceId::Terminal;
    if active {
        for event in events::<TabStripEvent>(w).to_vec() {
            if !d.strip.is_some_and(|s| s.root == event.strip) {
                continue;
            }
            match event.kind {
                TabStripEventKind::Activated(TabId(id)) if d.panes.contains_key(&id) => {
                    d.selected = Some(id);
                    d.redraw = true;
                }
                TabStripEventKind::CloseRequested(TabId(id)) => close(w, &mut d, id),
                _ => {}
            }
        }
    }
    let updates = w
        .resource::<Service>()
        .map(|s| {
            let mut rx = s.rx.lock().unwrap();
            (0..64)
                .filter_map(|_| rx.try_recv().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for u in updates {
        if let Fact::Catalog(result) = u.fact {
            if d.discovery != Some(u.generation) {
                continue;
            }
            d.discovery = None;
            match result {
                Ok((projects, sessions)) => {
                    d.projects = projects;
                    d.sessions = sessions;
                    d.status.clear();
                }
                Err(e) => d.status = e,
            }
            d.redraw = true;
            continue;
        }
        let Some(p) = d
            .panes
            .get_mut(&u.tab)
            .filter(|p| p.work == u.work && p.generation == u.generation)
        else {
            continue;
        };
        match u.fact {
            Fact::Bound { session, cwd } => {
                d.names.insert(session.clone(), p.name.clone());
                d.catalog_dirty = true;
                p.session = Some(session);
                p.cwd = cwd;
            }
            Fact::Connected { session, cwd } => {
                d.names.insert(session.clone(), p.name.clone());
                p.session = Some(session);
                p.cwd = cwd;
                p.connected = true;
                p.status = "Connected".into();
            }
            Fact::Output(bytes) => {
                rndr_term::feed_terminal(w, p.entity, &bytes);
                continue;
            }
            Fact::End(message) => {
                d.catalog_dirty = true;
                p.connected = false;
                p.frames = None;
                p.cancel = None;
                p.status = message;
            }
            Fact::Catalog(_) => unreachable!(),
        }
        d.redraw = true;
    }
    // An event during an in-flight discovery must trigger a subsequent read,
    // rather than letting that older snapshot mask a newly created/exited shell.
    if d.catalog_dirty && d.discovery.is_none() {
        refresh(w, &mut d);
        d.catalog_dirty = false;
    }
    for (id, p) in &mut d.panes {
        let workspace = !active && d.workspace_visible.contains(id);
        let show = if active {
            d.selected == Some(*id)
        } else {
            workspace
        };
        let interactive = show && (active || workspace);
        if show {
            w.remove::<Hidden>(p.entity);
        } else {
            w.insert(p.entity, Hidden);
        }
        input_enabled(w, p.entity, interactive && p.connected);
        let focused = w.resource::<Focus>().and_then(|f| f.entity) == Some(p.entity);
        // Hidden/off-surface input is drained and dropped, never replayed.
        let bytes = rndr_term::take_terminal_bytes(w, p.entity).unwrap_or_default();
        if p.connected && interactive {
            let mut failed = false;
            let size = w.get::<MonoGrid>(p.entity).map(|g| (g.cols, g.rows));
            if size != p.size
                && let (Some((cols, rows)), Some(tx)) = (size, p.frames.as_ref())
            {
                failed = tx.try_send(Frame::Resize(cols, rows)).is_err();
                if !failed {
                    p.size = size;
                }
            }
            if !bytes.is_empty() && (active || focused) {
                failed |= p
                    .frames
                    .as_ref()
                    .is_none_or(|tx| tx.try_send(Frame::Input(bytes)).is_err());
            }
            if failed {
                p.connected = false;
                p.frames = None;
                p.cancel = None;
                p.status =
                    "Terminal backpressure/disconnect. Input not replayed; reconnect explicitly."
                        .into();
                input_enabled(w, p.entity, false);
                d.redraw = true;
            }
        }
    }
    if d.panes.is_empty() {
        w.remove::<Hidden>(d.empty);
    } else {
        w.insert(d.empty, Hidden);
    }
    if d.redraw {
        render(w, &mut d);
        d.redraw = false;
    }
    w.resource_mut::<ViewState>().unwrap().0 = Some(d);
}
fn sidebar_action(w: &mut World, parent: Entity, id: u64, label: &str, hint: &str) {
    let h = visual::document_action(w, parent, id, label, true);
    visual::action_hint(w, h.root, hint);
    w.remove::<visual::FitLabel>(h.root);
    w.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
}
fn sidebar_row(w: &mut World, parent: Entity) -> Entity {
    let row = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Hug,
        gap: 4.,
        ..Layout::row()
    },));
    set_parent(w, row, parent);
    row
}
fn render(w: &mut World, d: &mut View) {
    for parent in [d.header, d.sidebar] {
        for child in children_of(w, parent) {
            despawn(w, child);
        }
    }
    d.actions.clear();
    let available = w.resource::<Service>().is_some_and(|s| !s.tx.is_closed());
    let title = sidebar_row(w, d.sidebar);
    let label = text(w, title, "Terminal", Typography::Label).unwrap();
    w.get_mut::<Layout>(label).unwrap().width = Size::Fill;
    visual::icon_action(
        w,
        title,
        GENERAL,
        Icon::Plus,
        "New shell without undertaking",
        available && d.panes.len() < 32,
    );
    visual::icon_action(
        w,
        title,
        REFRESH,
        Icon::RefreshCw,
        "Refresh sessions",
        d.discovery.is_none(),
    );
    let p = d.selected.and_then(|id| d.panes.get(&id));
    if let Some(p) = p {
        let tools = sidebar_row(w, d.sidebar);
        let label = text(w, tools, "Selected tab", Typography::Caption).unwrap();
        w.get_mut::<Layout>(label).unwrap().width = Size::Fill;
        visual::icon_action(
            w,
            tools,
            RECONNECT,
            Icon::RefreshCw,
            "Reconnect selected shell",
            available && p.frames.is_none() && p.session.is_some(),
        );
        visual::icon_action(
            w,
            tools,
            CLOSE,
            Icon::X,
            "Detach selected tab (keep shell running)",
            true,
        );
    }
    visual::rule(w, d.sidebar);
    text(w, d.sidebar, "Active sessions", Typography::Caption).unwrap();
    let sessions: Vec<_> = d
        .sessions
        .iter()
        .filter(|s| active_human(s))
        .cloned()
        .collect();
    let mut count = 0;
    for session in &sessions {
        let Some(shell) = session["session_id"].as_str() else {
            continue;
        };
        let work = session["work_id"].as_str().unwrap_or_default();
        let title = d
            .projects
            .iter()
            .find(|p| p["id"].as_str() == Some(work))
            .and_then(|p| p["title"].as_str())
            .unwrap_or(if work.is_empty() {
                "Workshop shell"
            } else {
                "Undertaking shell"
            })
            .to_owned();
        let label = shell_name(d, Some(shell));
        let hint = format!("{label} · {title}\nSession: {shell}");
        let id = next();
        d.actions
            .insert(id, Action::Attach(work.into(), shell.into()));
        sidebar_action(w, d.sidebar, id, &label, &hint);
        count += 1;
    }
    if count == 0 {
        text(w, d.sidebar, "No active sessions", Typography::Caption).unwrap();
    }
    visual::rule(w, d.sidebar);
    text(w, d.sidebar, "Open undertakings", Typography::Caption).unwrap();
    let mut count = 0;
    for project in d.projects.iter().filter(|p| open_project(p)) {
        let work = project["id"].as_str().unwrap();
        let title = project["title"].as_str().unwrap_or(work);
        let row = sidebar_row(w, d.sidebar);
        let label = text(w, row, title, Typography::Label).unwrap();
        w.get_mut::<Layout>(label).unwrap().width = Size::Fill;
        w.get_mut::<Label>(label).unwrap().overflow = TextOverflow::Ellipsis;
        visual::action_hint(w, label, title);
        let id = next();
        d.actions.insert(id, Action::NewWork(work.into()));
        visual::icon_action(
            w,
            row,
            id,
            Icon::Plus,
            &format!("New shell for {title}"),
            available && d.panes.len() < 32,
        );
        count += 1;
    }
    if count == 0 {
        text(w, d.sidebar, "No open undertakings", Typography::Caption).unwrap();
    }
    let p = d.selected.and_then(|id| d.panes.get(&id));
    if let Some(p) = p {
        text(
            w,
            d.header,
            &format!("{} · {}", p.cwd, p.status),
            Typography::Caption,
        )
        .unwrap();
    }
    if !d.status.is_empty() {
        text(w, d.header, &d.status, Typography::Caption).unwrap();
    }
    if let Some(selected) = d.selected {
        w.remove::<Hidden>(d.tabs);
        let specs: Vec<_> = d
            .order
            .iter()
            .filter_map(|id| d.panes.get(id).map(|p| TabSpec::new(*id, &p.name)))
            .collect();
        let strip = TabStrip::mount(
            w,
            d.tabs,
            UiId(STRIP),
            TabStripProps::new("Workshop terminals"),
            &specs,
            TabId(selected),
        )
        .expect("terminal tabs");
        for id in &d.order {
            if let Some(p) = d.panes.get(id)
                && let Some(tab) = strip.tab(w, TabId(*id))
                && let Some(session) = &p.session
            {
                visual::action_hint(w, tab, &format!("{}\nSession: {session}", p.name));
            }
        }
        w.insert(
            strip.root,
            Layout {
                width: Size::Fill,
                height: Size::Px(36.),
                ..Layout::column()
            },
        );
        w.insert(strip.body, Hidden);
        crate::ui::chrome::terminal_tabs(w, strip);
        d.strip = Some(strip);
    } else {
        crate::ui::chrome::clear_terminal_tabs(w);
        d.strip = None;
        w.insert(d.tabs, Hidden);
    }
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
pub(crate) mod tests;
