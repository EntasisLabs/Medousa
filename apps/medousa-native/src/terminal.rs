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
    split::{SplitSash, SplitSashHandle, SplitSashProps},
    text,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
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

const TOGGLE: u64 = 0x5445_0001;
const RECONNECT: u64 = 0x5445_0002;
const NEW: u64 = 0x5445_0003;
const HIDE: u64 = 0x5445_0004;
const SPLIT: u64 = 0x5445_0005;

#[derive(Clone)]
struct Wake;
impl Event for Wake {}
enum Frame {
    Input(Vec<u8>),
    Resize(u16, u16),
}
struct Open {
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
}
struct Update {
    work: String,
    generation: u64,
    fact: Fact,
}
struct Service {
    tx: mpsc::Sender<Open>,
    rx: Mutex<mpsc::Receiver<Update>>,
    _wake: Feed<Wake>,
}

/// Matches only human workspace shells, never Forge/task/SSH sessions.
fn human(v: &Value, work: &str) -> bool {
    v["root_kind"].as_str() == Some("workspace")
        && v["work_id"].as_str() == Some(work)
        && v["argv"].as_array().is_some_and(|a| a.is_empty())
        && v["session_id"].as_str().is_some_and(|s| !s.is_empty())
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
    let (updates, rx) = mpsc::channel::<Update>(64);
    let (wake, sender) = feed::open(w, FeedConfig::latest());
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("terminal runtime");
        runtime.block_on(async move {
            while let Some(open) = opens.recv().await {
                let client = client.clone();
                let origin = origin.clone();
                let data_dir = data_dir.clone();
                let updates = updates.clone();
                let sender = sender.clone();
                tokio::spawn(async move {
                    let work = open.work.clone();
                    let generation = open.generation;
                    let result = attach(&client, &origin, &data_dir, open, &updates, &sender).await;
                    let _ = publish(
                        &updates,
                        &sender,
                        Update {
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
        tx,
        rx: Mutex::new(rx),
        _wake: wake,
    });
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
    let list: Value = client.http().get("/v1/sessions/shell").await.map_err(
        |_| "Could not discover workshop shells. Check Connection and session host availability.",
    )?;
    let session=match if open.fresh {None} else {choose(&list,&open.work,open.session.as_deref())?} {
        Some(v)=>v,
        None if open.create=>client.http().post::<_,Value>("/v1/sessions/workspace-shell",&json!({"work_id":open.work,"cols":80,"rows":24})).await.map_err(|_|"Shell creation not confirmed. Reconnect to discover it; creation is never automatically retried.")?,
        None=>return Err("No running human shell found. Use New shell explicitly.".into()),
    };
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
                publish(tx,wake,Update{work:open.work.clone(),generation:open.generation,fact}).await?;

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

struct Pane {
    entity: Entity,
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
struct DockState(Option<Dock>);
struct Dock {
    parent: Entity,
    split: SplitSashHandle,
    header: Entity,
    body: Entity,
    panes: HashMap<String, Pane>,
    selected: Option<String>,
    visible: bool,
    ratio: f64,
    redraw: bool,
}
fn props(visible: bool) -> SplitSashProps {
    let mut p = SplitSashProps::new("Terminal dock height");
    p.axis = Axis::Column;
    p.enabled = visible;
    p.min_leading = 100.;
    p.min_trailing = if visible { 120. } else { 0. };
    p.default_ratio = 0.65;
    p
}
pub fn mount(w: &mut World, parent: Entity) -> Entity {
    let split =
        SplitSash::mount(w, parent, UiId(SPLIT), props(false), 1.).expect("terminal dock split");
    for entity in [split.leading, split.trailing] {
        w.insert(
            entity,
            rndr_ui::UiStyle {
                background: Some(visual::canvas().into()),
                ..Default::default()
            },
        );
    }
    let header = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 4.,
            padding: 4.,
            ..Layout::row()
        },
        Wrap::default(),
    ));
    set_parent(w, header, split.trailing);
    w.insert(
        header,
        rndr_ui::UiStyle {
            background: Some(visual::canvas().into()),
            ..Default::default()
        },
    );
    let body = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(w, body, split.trailing);
    w.insert(split.trailing, Hidden);
    w.insert(split.sash, Hidden);
    w.insert_resource(DockState(Some(Dock {
        parent,
        split,
        header,
        body,
        panes: HashMap::new(),
        selected: None,
        visible: false,
        ratio: 0.65,
        redraw: true,
    })));
    split.leading
}
pub fn toolbar(w: &mut World, parent: Entity, enabled: bool) {
    visual::document_action(w, parent, TOGGLE, "Terminal", enabled);
}
pub fn focused(w: &World) -> bool {
    w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|e| w.has::<rndr_term::Terminal>(e))
}
fn unfocus(w: &mut World, e: Entity) {
    if let Some(f) = w.resource_mut::<Focus>() {
        if f.entity == Some(e) {
            f.entity = None;
        }
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
fn connect(w: &mut World, d: &mut Dock, new: bool, initial: bool) {
    let Some(work) = d.selected.clone() else {
        return;
    };
    if !d.panes.contains_key(&work) {
        let e = w.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },
            Semantics {
                name: Some("Workshop terminal".into()),
                ..Default::default()
            },
        ));
        set_parent(w, e, d.body);
        d.panes.insert(
            work.clone(),
            Pane {
                entity: e,
                generation: 0,
                session: None,
                cwd: String::new(),
                status: String::new(),
                connected: false,
                frames: None,
                cancel: None,
                size: None,
            },
        );
    }
    let p = d.panes.get_mut(&work).unwrap();
    if p.frames.is_some() {
        return;
    }
    p.frames = None;
    p.cancel = None;
    p.connected = false;
    p.size = None;
    p.generation += 1;
    if new {
        p.session = None;
    }
    // Server replays its scrollback on attach; reset only this emulator, not PTY.
    if let Err(e) = rndr_term::open_terminal(w, p.entity, rndr_term::TerminalOptions::offline()) {
        p.status = e.to_string();
        return;
    }
    input_enabled(w, p.entity, false);
    let (frames, rx) = mpsc::channel(32);
    let (cancel, cancelled) = oneshot::channel();
    let open = Open {
        work,
        generation: p.generation,
        session: p.session.clone(),
        create: new || initial,
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
pub(crate) fn can_rebind(w: &World) -> bool {
    w.resource::<DockState>()
        .and_then(|d| d.0.as_ref())
        .is_none_or(|d| d.panes.is_empty())
}
/// A disconnected source must never retain another daemon's terminal service.
pub(crate) fn disable(w: &mut World) {
    let (tx, _) = mpsc::channel(1);
    let (_, rx) = mpsc::channel(1);
    let (wake, _) = feed::open(w, FeedConfig::queue(1));
    w.insert_resource(Service {
        tx,
        rx: Mutex::new(rx),
        _wake: wake,
    });
}
pub fn action(w: &mut World, host: &HostIntent) -> bool {
    let HostIntent::Command(id) = host else {
        return false;
    };
    if ![TOGGLE, RECONNECT, NEW, HIDE].contains(id) || w.resource::<DockState>().is_none() {
        return false;
    }
    let mut d = w.resource_mut::<DockState>().unwrap().0.take().unwrap();
    match *id {
        TOGGLE => {
            d.visible = !d.visible;
            if d.visible
                && d.selected
                    .as_ref()
                    .is_some_and(|s| !d.panes.contains_key(s))
            {
                connect(w, &mut d, false, true);
            }
        }
        HIDE => d.visible = false,
        RECONNECT => connect(w, &mut d, false, false),
        NEW => connect(w, &mut d, true, false),
        _ => {}
    }
    d.redraw = true;
    w.resource_mut::<DockState>().unwrap().0 = Some(d);
    true
}
pub fn sync(w: &mut World, selected: Option<String>) {
    let Some(mut d) = w.resource_mut::<DockState>().and_then(|s| s.0.take()) else {
        return;
    };
    if d.selected != selected {
        d.selected = selected;
        d.redraw = true;
        if d.visible
            && d.selected
                .as_ref()
                .is_some_and(|s| !d.panes.contains_key(s))
        {
            connect(w, &mut d, false, false);
        }
    }
    let updates = w
        .resource::<Service>()
        .map(|s| {
            let mut rx = s.rx.lock().unwrap();
            let mut out = Vec::new();
            for _ in 0..64 {
                match rx.try_recv() {
                    Ok(u) => out.push(u),
                    Err(_) => break,
                }
            }
            out
        })
        .unwrap_or_default();
    for u in updates {
        let Some(p) = d
            .panes
            .get_mut(&u.work)
            .filter(|p| p.generation == u.generation)
        else {
            continue;
        };
        match u.fact {
            Fact::Bound { session, cwd } => {
                p.session = Some(session);
                p.cwd = cwd;
            }
            Fact::Connected { session, cwd } => {
                p.session = Some(session);
                p.cwd = cwd;
                p.connected = true;
                p.status = "Connected".into();
                input_enabled(w, p.entity, true);
            }
            Fact::Output(bytes) => {
                rndr_term::feed_terminal(w, p.entity, &bytes);
                continue;
            }
            Fact::End(message) => {
                p.connected = false;
                p.frames = None;
                p.cancel = None;
                p.status = message;
                input_enabled(w, p.entity, false);
            }
        }
        d.redraw = true;
    }
    let active = crate::surfaces::active(w) == crate::surfaces::SurfaceId::Code;
    let visible = d.visible && d.selected.is_some();
    for (work, p) in &mut d.panes {
        let show = visible && d.selected.as_ref() == Some(work);
        if show {
            w.remove::<Hidden>(p.entity);
        } else {
            w.insert(p.entity, Hidden);
            unfocus(w, p.entity);
        }
        // Drain emulator-generated replies even when hidden. Never retain input
        // across a disconnect or reconnect generation.
        let bytes = rndr_term::take_terminal_bytes(w, p.entity).unwrap_or_default();
        if p.connected {
            let size = w.get::<MonoGrid>(p.entity).map(|g| (g.cols, g.rows));
            let mut failed = false;
            if show && active && size != p.size {
                if let (Some((cols, rows)), Some(tx)) = (size, p.frames.as_ref()) {
                    failed = tx.try_send(Frame::Resize(cols, rows)).is_err();
                    if !failed {
                        p.size = size;
                    }
                }
            }
            if !bytes.is_empty() {
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
        if !active {
            unfocus(w, p.entity);
        }
    }
    for e in [d.split.trailing, d.split.sash] {
        if visible {
            w.remove::<Hidden>(e);
        } else {
            w.insert(e, Hidden);
        }
    }
    for event in events::<rndr_ui::split::SplitSashChange>(w) {
        if visible && event.sash == d.split.sash {
            d.ratio = event.ratio;
        }
    }
    let ratio = if visible { d.ratio } else { 1. };
    // Idempotent mount preserves the user's drag ratio between frames.
    SplitSash::mount(w, d.parent, UiId(SPLIT), props(visible), ratio).expect("terminal dock");
    // Wrap establishes a paintable node; explicitly own its surface as well
    // as the split pane so its default fill cannot leak through.
    w.insert(d.header, visual::color(w, visual::canvas()));
    if d.redraw {
        for child in children_of(w, d.header) {
            despawn(w, child);
        }
        let p = d.selected.as_ref().and_then(|s| d.panes.get(s));
        let label = p
            .map(|p| format!("Terminal · {} · {}", p.cwd, p.status))
            .unwrap_or_else(|| "Select an undertaking to open a terminal".into());
        let label = text(w, d.header, &label, Typography::Caption).unwrap();
        w.get_mut::<Layout>(label).unwrap().width = Size::Fill;
        rndr_ui::inputs::Tooltip::attach(
            w,
            label,
            &p.map(|p| format!("{}\n{}", p.session.as_deref().unwrap_or(""), p.status))
                .unwrap_or_else(|| "Workshop-owned human shell".into()),
        )
        .unwrap();
        visual::document_action(
            w,
            d.header,
            RECONNECT,
            "Reconnect",
            p.is_some_and(|p| p.frames.is_none()),
        );
        visual::document_action(
            w,
            d.header,
            NEW,
            "New shell",
            p.is_some_and(|p| p.frames.is_none()),
        );
        visual::document_action(w, d.header, HIDE, "Hide", true);
        d.redraw = false;
    }
    w.resource_mut::<DockState>().unwrap().0 = Some(d);
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
pub(crate) mod tests;
