//! Create: the workshop's canvas projects. The daemon owns every document;
//! RNDR's canvas editor edits a remote session of one, so strokes and moves
//! show at once while the worker saves them in order. Scenes are the same
//! `medousa-create` v3 JSON medousa-home reads, so both shells edit one file.
use medousa_sdk::MedousaClient;
use medousa_types::{
    CreativeAssetData, CreativeCreateRequest, CreativeHistoryRequest, CreativeListQuery,
    CreativeListResponse, CreativeMutationResponse, CreativeProjectSummary, CreativeTransaction,
};
use rndr_canvas::{
    CanvasError, Content, HistoryRequest, Mutation, Node, Scene, Transaction, Transform,
};
use rndr_canvas_ui::editor::{CanvasEditor, Outgoing};
use rndr_canvas_ui::view::{self as canvas_view, CanvasView};
use rndr_core::*;
use rndr_ui::controls::HostIntent;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "create_ask.rs"]
mod ask;
#[path = "create_canvas.rs"]
mod canvas;
#[path = "create_view.rs"]
mod view;

/// The canvas paints into this surface slot.
const SLOT: u64 = 0x4352_0001;
/// How often an idle open project is re-read for edits made elsewhere
/// (medousa-home, another window, or Medousa accepting a proposal).
const POLL: std::time::Duration = std::time::Duration::from_secs(4);
/// medousa-home's import limit.
const MAX_IMAGE: u64 = 25 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0x4352_1000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// medousa-home's starters, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Starter {
    Blank,
    Screenshot,
    Diagram,
    Concept,
    Composition,
}
impl Starter {
    pub(crate) const ALL: [Starter; 5] = [
        Starter::Blank,
        Starter::Screenshot,
        Starter::Diagram,
        Starter::Concept,
        Starter::Composition,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Starter::Blank => "Blank canvas",
            Starter::Screenshot => "Screenshot markup",
            Starter::Diagram => "Diagram",
            Starter::Concept => "UI concept",
            Starter::Composition => "Image composition",
        }
    }
    fn title(self) -> &'static str {
        match self {
            Starter::Blank => "Untitled canvas",
            other => other.label(),
        }
    }
    /// A blank scene, or one frame sized for the starter.
    fn scene(self) -> Scene {
        // The daemon stores Medousa's header, as medousa-home writes it.
        let mut scene = Scene {
            schema: rndr_canvas::schema::MEDOUSA_SCHEMA.into(),
            version: rndr_canvas::schema::MEDOUSA_VERSION,
            ..Scene::default()
        };
        if self == Starter::Blank {
            return scene;
        }
        let (width, height) = if self == Starter::Composition {
            (1024., 1024.)
        } else {
            (1200., 920.)
        };
        scene.nodes.push(Node::new(
            uuid(),
            Content::Frame {
                name: self.label().into(),
                background: "#f7f5ee".into(),
            },
            Transform {
                x: 0.,
                y: 0.,
                width,
                height,
                rotation: 0.,
            },
        ));
        scene
    }
}

/// A random v4-shaped id. Creation retries reuse the request, so this only
/// has to be unique, not secret.
fn uuid() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut bits = [0u64; 2];
    for (i, word) in bits.iter_mut().enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(next());
        h.write_usize(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        *word = h.finish();
    }
    let n = (u128::from(bits[0]) << 64 | u128::from(bits[1]))
        & !(0xf000u128 << 64 | 0xc000u128 << 48)
        | 0x4000u128 << 64
        | 0x8000u128 << 48;
    let hex = format!("{n:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[derive(Clone, Debug)]
pub struct Command {
    serial: u64,
    kind: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    List,
    Create(CreativeCreateRequest),
    Get(String),
    Transact(String, Transaction),
    History(String, HistoryRequest),
    Asset(String, String),
    /// Pick an image, add it to project `0`, and read the project again.
    Import(String),
    /// Ask Medousa about project `0`.
    Ask(String, ask::AskKind),
}
pub struct Update {
    command: Command,
    result: Result<Response, CanvasError>,
}
enum Response {
    Listed(CreativeListResponse),
    Created(String),
    Document(Box<Mutation>),
    Asset {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Imported(Box<Imported>),
    Ask(ask::AskReply),
    /// The picker was dismissed.
    Cancelled,
}
pub struct Imported {
    asset: String,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    document: Mutation,
}

#[derive(Clone, Debug)]
enum Action {
    Open(String),
    New(Starter),
    Refresh,
    Retry,
    Canvas(canvas::Do),
}

pub(crate) struct State {
    sidebar: Entity,
    /// Toolbar, canvas and side panels; hidden until a project is open.
    editor: Entity,
    empty: Entity,
    slot: Entity,
    chrome: canvas::Chrome,
    /// The open Align or Arrange menu and what its items do.
    menu: Option<(rndr_ui::menu::MenuHandle, HashMap<u64, Action>)>,
    projects: Vec<CreativeProjectSummary>,
    open: Option<String>,
    queue: Vec<Command>,
    list: Option<u64>,
    creating: Option<(u64, CreativeCreateRequest)>,
    opening: Option<u64>,
    /// Asset ids already asked for in the open project, and their textures.
    assets: HashSet<String>,
    actions: HashMap<u64, Action>,
    message: String,
    started: bool,
    redraw: bool,
    /// Save status last drawn, so the sidebar redraws when it changes.
    status: (bool, bool),
    /// The last idle re-read of the open project, and the one in flight.
    polled: std::time::Instant,
    poll: Option<u64>,
    importing: Option<u64>,
    ask: ask::Ask,
}

/// The surface state. Taken out while a system builds UI against the world.
struct Host(Option<State>);
fn state(w: &World) -> Option<&State> {
    w.resource::<Host>().and_then(|h| h.0.as_ref())
}
fn state_mut(w: &mut World) -> Option<&mut State> {
    w.resource_mut::<Host>().and_then(|h| h.0.as_mut())
}

pub fn mount(w: &mut World, parent: Entity) {
    let root = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    },));
    set_parent(w, root, parent);
    crate::native_style::flat(w, root, crate::native_style::canvas());
    let parts = view::mount(w, root, SLOT);
    w.insert_resource(Host(Some(State {
        sidebar: parts.sidebar,
        editor: parts.editor,
        empty: parts.empty,
        slot: parts.slot,
        chrome: parts.chrome,
        menu: None,
        projects: vec![],
        open: None,
        queue: vec![],
        list: None,
        creating: None,
        opening: None,
        assets: HashSet::new(),
        actions: HashMap::new(),
        message: String::new(),
        started: false,
        redraw: true,
        status: (false, false),
        polled: std::time::Instant::now(),
        poll: None,
        importing: None,
        ask: ask::Ask::default(),
    })));
}

pub fn activate(w: &mut World) {
    if let Some(s) = state_mut(w) {
        // Every return re-lists: projects may have been made or renamed in
        // medousa-home meanwhile.
        s.started = true;
        if s.list.is_none() {
            s.refresh();
        }
        s.redraw = true;
    }
}

pub fn can_deactivate(w: &World) -> bool {
    // A text node mid-edit commits on the next click; leaving would strand it.
    w.resource::<CanvasView>()
        .is_none_or(|v| v.editor.editing_text().is_none())
}

impl State {
    fn enqueue(&mut self, kind: Kind) -> u64 {
        let serial = next();
        self.queue.push(Command { serial, kind });
        serial
    }
    fn refresh(&mut self) {
        self.list = Some(self.enqueue(Kind::List));
    }
    fn open(&mut self, id: String) {
        self.open = Some(id.clone());
        self.opening = Some(self.enqueue(Kind::Get(id)));
        self.redraw = true;
    }
}

fn with_state<R>(w: &mut World, f: impl FnOnce(&mut World, &mut State) -> R) -> Option<R> {
    let mut s = w.resource_mut::<Host>()?.0.take()?;
    let out = f(w, &mut s);
    if let Some(host) = w.resource_mut::<Host>() {
        host.0 = Some(s);
    }
    Some(out)
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
            Action::Open(id) => {
                if s.open.as_deref() != Some(id.as_str()) {
                    s.open(id);
                }
            }
            Action::New(starter) => {
                if s.creating.is_none() {
                    let request = CreativeCreateRequest {
                        id: uuid(),
                        title: starter.title().into(),
                        scene: convert(&starter.scene()).expect("starter scenes are valid"),
                    };
                    let serial = s.enqueue(Kind::Create(request.clone()));
                    s.creating = Some((serial, request));
                    s.message.clear();
                }
            }
            Action::Refresh => s.refresh(),
            Action::Canvas(a) => {
                canvas::run(w, s, a);
                return;
            }
            Action::Retry => {
                if let Some(v) = w.resource_mut::<CanvasView>() {
                    v.editor.retry();
                }
                if let Some((_, request)) = s.creating.clone() {
                    // Same id: a create that did land is returned, not doubled.
                    let serial = s.enqueue(Kind::Create(request.clone()));
                    s.creating = Some((serial, request));
                }
            }
        }
        s.redraw = true;
    });
    true
}

/// Canvas input only while Create is showing, and keys only while nothing
/// else (the sidebar, a dialog) holds focus, so tool letters never fire while
/// you type elsewhere.
pub fn take_wheel(w: &mut World) {
    if crate::surfaces::active(w) == crate::surfaces::SurfaceId::Create {
        canvas_view::take_wheel(w);
    }
}

pub fn sync(w: &mut World) {
    let active = crate::surfaces::active(w) == crate::surfaces::SurfaceId::Create;
    if active && w.resource::<CanvasView>().is_some() {
        // A press on the canvas takes keyboard focus from the sidebar.
        let pressed = w.resource::<Input>().and_then(|i| {
            let p = &i.pointer;
            p.pressed.then_some(p.position)
        });
        let slot = state(w).map(|s| s.slot);
        if let (Some([x, y]), Some(slot)) = (pressed, slot)
            && w.get::<Rect>(slot)
                .is_some_and(|r| x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h)
            && let Some(f) = w.resource_mut::<Focus>()
        {
            f.entity = None;
        }
        let elsewhere = w.resource::<Focus>().and_then(|f| f.entity).is_some();
        let held = if elsewhere {
            w.resource_mut::<Input>()
                .map(|i| (std::mem::take(&mut i.keys), std::mem::take(&mut i.chars)))
        } else {
            None
        };
        canvas_view::update(w);
        if let (Some((keys, chars)), Some(i)) = (held, w.resource_mut::<Input>()) {
            i.keys = keys;
            i.chars = chars;
        }
    }
    with_state(w, |w, s| {
        if let Some(v) = w.resource_mut::<CanvasView>() {
            let id = v.editor.document_id().to_string();
            for request in v.editor.take_outgoing() {
                let kind = match request {
                    Outgoing::Get => Kind::Get(id.clone()),
                    Outgoing::Transact(t) => Kind::Transact(id.clone(), t),
                    Outgoing::History(h) => Kind::History(id.clone(), h),
                };
                s.enqueue(kind);
            }
            let missing: Vec<String> = v
                .editor
                .mutation()
                .document
                .assets
                .keys()
                .filter(|a| !s.assets.contains(*a))
                .cloned()
                .collect();
            // Pick up edits made elsewhere while this one has nothing unsaved;
            // RNDR keeps a newer read under any edit made meanwhile.
            if active
                && !v.editor.is_saving()
                && v.editor.editing_text().is_none()
                && s.poll.is_none()
                && s.polled.elapsed() >= POLL
            {
                s.polled = std::time::Instant::now();
                s.poll = Some(s.enqueue(Kind::Get(id.clone())));
            }
            let status = (v.editor.is_saving(), v.editor.is_stalled());
            if status != s.status {
                s.status = status;
                s.redraw = true;
            }
            for asset in missing {
                s.assets.insert(asset.clone());
                s.enqueue(Kind::Asset(id.clone(), asset));
            }
        }
        if s.redraw && active {
            view::render(w, s);
            s.redraw = false;
        }
        if active {
            // The Ask prompt's text, kept across rebuilds; Enter sends it.
            if let Some(field) = s.ask.field
                && let Some(label) = w.get::<Label>(field)
            {
                s.ask.draft = label.text.clone();
            }
            let sent = events::<Submitted>(w)
                .iter()
                .any(|e| Some(e.entity) == s.ask.field);
            if sent {
                ask::start(w, s);
            }
            ask::tick(w, s);
            ask::sync_preview(w, s);
            canvas::picks(w, s);
            canvas::render(w, s);
            canvas::place(w, s);
        }
    });
}

pub fn take_commands(w: &mut World) -> Vec<Command> {
    state_mut(w)
        .map(|s| std::mem::take(&mut s.queue))
        .unwrap_or_default()
}

/// The daemon's error, as the canvas session reads it. The SDK reports
/// non-2xx replies as `"<status>: <body>"`.
fn canvas_error(error: medousa_sdk::SdkError) -> CanvasError {
    let text = error.to_string();
    let status = text
        .split_once("http error: ")
        .map_or(text.as_str(), |(_, rest)| rest)
        .get(..3)
        .and_then(|s| s.parse::<u16>().ok());
    match status {
        Some(409) => {
            let current = text
                .rsplit_once("current revision ")
                .and_then(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|n| n.parse().ok());
            match current {
                Some(revision) => CanvasError::Conflict(revision),
                None => CanvasError::Unsupported,
            }
        }
        Some(404) => CanvasError::NotFound,
        Some(400 | 403 | 413 | 422) => CanvasError::Invalid(text),
        _ => CanvasError::Storage(text),
    }
}

/// Between RNDR's canvas types and the daemon's: both are the same
/// `medousa-create` JSON, so this is a lossless re-read.
fn convert<A: serde::Serialize, B: serde::de::DeserializeOwned>(
    value: &A,
) -> Result<B, CanvasError> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|e| CanvasError::Invalid(e.to_string()))
}

fn mutation(response: CreativeMutationResponse) -> Result<Response, CanvasError> {
    convert::<_, Mutation>(&response).map(|m| Response::Document(Box::new(m)))
}

pub async fn execute(client: &MedousaClient, command: Command) -> Update {
    let api = client.creative();
    let result = match &command.kind {
        Kind::List => api
            .list(&CreativeListQuery {
                cursor: None,
                limit: Some(200),
            })
            .await
            .map_err(canvas_error)
            .map(Response::Listed),
        Kind::Create(request) => api
            .create(request)
            .await
            .map_err(canvas_error)
            .map(|doc| Response::Created(doc.id)),
        Kind::Get(id) => match api.get(id).await {
            Ok(m) => mutation(m),
            Err(e) => Err(canvas_error(e)),
        },
        Kind::Transact(id, t) => match convert::<_, CreativeTransaction>(t) {
            Ok(t) => match api.transact(id, &t).await {
                Ok(m) => mutation(m),
                Err(e) => Err(canvas_error(e)),
            },
            Err(e) => Err(e),
        },
        Kind::History(id, h) => match convert::<_, CreativeHistoryRequest>(h) {
            Ok(h) => match api.history(id, &h).await {
                Ok(m) => mutation(m),
                Err(e) => Err(canvas_error(e)),
            },
            Err(e) => Err(e),
        },
        Kind::Asset(id, asset) => match api.asset(id, asset).await {
            Ok(data) => decode(&data),
            Err(e) => Err(canvas_error(e)),
        },
        Kind::Import(id) => import(client, id).await,
        Kind::Ask(project, kind) => ask::execute(client, project, kind).await.map(Response::Ask),
    };
    Update { command, result }
}

/// medousa-home's import: a PNG, JPEG or WebP under 25 MiB, uploaded, then the
/// project read again so the new asset is in the document the image joins.
async fn import(client: &MedousaClient, id: &str) -> Result<Response, CanvasError> {
    use base64::Engine;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Add an image to the canvas")
        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
        .pick_file()
        .await
    else {
        return Ok(Response::Cancelled);
    };
    let path = file.path().to_path_buf();
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if len > MAX_IMAGE {
            return Err("Choose an image under 25 MiB".into());
        }
        std::fs::read(&path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| CanvasError::Storage(e.to_string()))?
    .map_err(CanvasError::Invalid)?;
    let format = image::guess_format(&bytes)
        .map_err(|_| CanvasError::Invalid("Choose a PNG, JPEG, or WebP image".into()))?;
    if !matches!(
        format,
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
    ) {
        return Err(CanvasError::Invalid(
            "Choose a PNG, JPEG, or WebP image".into(),
        ));
    }
    let pixels = image::load_from_memory(&bytes)
        .map_err(|e| CanvasError::Invalid(e.to_string()))?
        .to_rgba8();
    let api = client.creative();
    let asset = api
        .import_asset(
            id,
            &medousa_types::CreativeAssetImportRequest {
                data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                context: None,
            },
        )
        .await
        .map_err(canvas_error)?;
    let document = convert::<_, Mutation>(&api.get(id).await.map_err(canvas_error)?)?;
    Ok(Response::Imported(Box::new(Imported {
        asset: asset.id,
        width: pixels.width(),
        height: pixels.height(),
        rgba: pixels.into_raw(),
        document,
    })))
}

/// Where an imported image goes: centered in the selected frame or group,
/// else the first unlocked frame, else a 1200 × 920 area at the origin,
/// scaled down to fit, as medousa-home places it.
fn placement(
    nodes: &[Node],
    selection: &[String],
    width: u32,
    height: u32,
) -> (Option<String>, Transform) {
    let container =
        |n: &&Node| matches!(n.content, Content::Frame { .. } | Content::Group) && !n.locked;
    let parent = nodes
        .iter()
        .filter(|n| selection.contains(&n.id))
        .find(container)
        .or_else(|| {
            nodes
                .iter()
                .filter(|n| matches!(n.content, Content::Frame { .. }))
                .find(container)
        });
    let area = parent.map_or(
        Transform {
            x: 0.,
            y: 0.,
            width: 1200.,
            height: 920.,
            rotation: 0.,
        },
        |p| p.transform,
    );
    let (w, h) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let scale = (area.width / w).min(area.height / h).min(1.);
    let (w, h) = (w * scale, h * scale);
    (
        parent.map(|p| p.id.clone()),
        Transform {
            x: area.x + (area.width - w) / 2.,
            y: area.y + (area.height - h) / 2.,
            width: w,
            height: h,
            rotation: 0.,
        },
    )
}

fn decode(data: &CreativeAssetData) -> Result<Response, CanvasError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.data_base64.as_bytes())
        .map_err(|e| CanvasError::Invalid(e.to_string()))?;
    let image = image::load_from_memory(&bytes)
        .map_err(|e| CanvasError::Invalid(e.to_string()))?
        .to_rgba8();
    Ok(Response::Asset {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

/// GPU textures are cached by id, so each project's asset gets its own.
fn texture_id(project: &str, asset: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (project, asset).hash(&mut h);
    0x4352_0000_0000_0000 | (h.finish() & 0x0000_ffff_ffff_ffff)
}

fn install(w: &mut World, s: &mut State, document: Mutation) {
    let mut editor = CanvasEditor::remote(document);
    // medousa-home's default ink, legible on the workspace and on artboards.
    editor.style.stroke = canvas::COLORS[0].1.into();
    let mut view = CanvasView::new(editor, SLOT);
    view.backdrop = crate::native_style::color(w, crate::native_style::canvas());
    if let Some(theme) = w.resource::<rndr_ui::UiTheme>() {
        view.overlay = rndr_canvas_ui::overlay::OverlayTheme::from_theme(theme);
    }
    canvas_view::install(w, view);
    s.assets.clear();
    // A new project starts with its own requests and proposals.
    s.ask = ask::Ask {
        open: s.ask.open,
        ..Default::default()
    };
    ask::refresh(w, s);
}

pub fn apply(w: &mut World, u: Update) {
    with_state(w, |w, s| apply_update(w, s, u));
}

fn apply_update(w: &mut World, s: &mut State, u: Update) {
    let Update { command, result } = u;
    match command.kind {
        Kind::List => {
            if s.list != Some(command.serial) {
                return;
            }
            s.list = None;
            match result {
                Ok(Response::Listed(r)) => {
                    s.projects = r.projects;
                    s.projects.sort_by(|a, b| {
                        b.pinned
                            .cmp(&a.pinned)
                            .then_with(|| b.updated_at.cmp(&a.updated_at))
                    });
                }
                Err(e) => s.message = format!("Projects could not be loaded: {e}"),
                _ => {}
            }
        }
        Kind::Create(_) => {
            if s.creating.as_ref().map(|c| c.0) != Some(command.serial) {
                return;
            }
            match result {
                Ok(Response::Created(id)) => {
                    s.creating = None;
                    s.open(id);
                    s.refresh();
                }
                Err(e) => {
                    s.message = format!("The project was not created: {e}");
                }
                _ => {}
            }
        }
        Kind::Get(id) => {
            if s.poll == Some(command.serial) {
                s.poll = None;
                // A failed background read is not worth a message; the
                // next one tries again.
                if result.is_err() {
                    return;
                }
            }
            if s.open.as_deref() != Some(id.as_str()) {
                return;
            }
            let fresh = s.opening == Some(command.serial)
                || w.resource::<CanvasView>()
                    .is_none_or(|v| v.editor.document_id() != id);
            match result {
                Ok(Response::Document(m)) => {
                    s.opening = None;
                    if fresh {
                        install(w, s, *m);
                    } else if let Some(v) = w.resource_mut::<CanvasView>() {
                        v.editor.replace(*m);
                    }
                    s.message.clear();
                }
                Err(e) => {
                    if fresh {
                        s.opening = None;
                        s.open = None;
                        s.message = format!("The project could not be opened: {e}");
                    } else if let Some(v) = w.resource_mut::<CanvasView>() {
                        v.editor.fail_get(e);
                    }
                }
                _ => {}
            }
        }
        Kind::Transact(id, Transaction { id: request, .. })
        | Kind::History(id, HistoryRequest { id: request, .. }) => {
            let Some(v) = w.resource_mut::<CanvasView>() else {
                return;
            };
            if v.editor.document_id() != id {
                return;
            }
            match result {
                Ok(Response::Document(m)) => {
                    let summary = (m.document.title.clone(), m.document.updated_at.clone());
                    v.editor.confirm(&request, *m);
                    if let Some(p) = s.projects.iter_mut().find(|p| p.id == id) {
                        (p.title, p.updated_at) = summary;
                    }
                }
                Err(e) => {
                    v.editor.fail(&request, e);
                }
                _ => {}
            }
        }
        Kind::Ask(project, kind) => {
            let result = match result {
                Ok(Response::Ask(reply)) => Ok(reply),
                Err(e) => Err(e),
                Ok(_) => return,
            };
            ask::apply(w, s, command.serial, &project, kind, result);
        }
        Kind::Import(id) => {
            if s.importing != Some(command.serial) {
                return;
            }
            s.importing = None;
            let imported = match result {
                Ok(Response::Imported(imported)) => *imported,
                Ok(_) => return,
                Err(e) => {
                    s.message = format!("The image was not added: {e}");
                    s.redraw = true;
                    return;
                }
            };
            let Imported {
                asset,
                width,
                height,
                rgba,
                document,
            } = imported;
            let texture = texture_id(&id, &asset);
            let loaded = rndr_render::load_rgba(w, texture, width, height, rgba).is_ok();
            let Some(v) = w.resource_mut::<CanvasView>() else {
                return;
            };
            if v.editor.document_id() != id {
                return;
            }
            v.editor.replace(document);
            if loaded {
                v.images.insert(asset.clone(), texture);
            }
            s.assets.insert(asset.clone());
            let nodes = v.editor.nodes();
            let (parent, transform) = placement(&nodes, v.editor.selection(), width, height);
            let node_id = v.editor.new_id("image");
            let mut node = Node::new(
                node_id.clone(),
                Content::Image {
                    asset_id: asset,
                    crop: None,
                },
                transform,
            );
            node.parent_id = parent;
            if v.editor
                .commit("Import image", vec![rndr_canvas::Operation::put(node)])
            {
                v.editor.select(&[node_id]);
            }
        }
        Kind::Asset(id, asset) => {
            let Ok(Response::Asset {
                width,
                height,
                rgba,
            }) = result
            else {
                // Unloaded images paint as their placeholder; asking again
                // on the next reopen is enough.
                return;
            };
            let texture = texture_id(&id, &asset);
            if rndr_render::load_rgba(w, texture, width, height, rgba).is_ok()
                && let Some(v) = w.resource_mut::<CanvasView>()
                && v.editor.document_id() == id
            {
                v.images.insert(asset, texture);
            }
        }
    }
    s.redraw = true;
}

#[cfg(test)]
#[path = "create_tests.rs"]
mod tests;
