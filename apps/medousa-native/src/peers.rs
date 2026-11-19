//! Retained human/workshop conversations, separate from agent assignments.
use rndr_core::*;
use rndr_ui::controls::HostIntent;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "peers_view.rs"]
mod view;
static NEXT: AtomicU64 = AtomicU64::new(0x5045_0000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
fn text(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().into()
}
#[derive(Clone, Debug)]
struct Peer {
    id: String,
    device: String,
    name: String,
    inbound: bool,
    ready: bool,
}
impl Peer {
    fn parse(v: &Value) -> Option<Self> {
        let id = text(v, "workshopId");
        let device = text(v, "workshopDeviceId");
        if id.is_empty() || device.is_empty() {
            return None;
        }
        Some(Self {
            id,
            device,
            name: text(v, "label"),
            inbound: v["inbound"].as_bool().unwrap_or(false),
            ready: v["hasSessionToken"].as_bool().unwrap_or(false),
        })
    }
}
fn involves(m: &Value, p: &Peer) -> bool {
    // No short-prefix identity matching: another peer must not inherit a thread.
    if m["sinkKind"].as_str() == Some("peer") {
        return m["workshopId"].as_str() == Some(&p.id);
    }
    m["fromDeviceId"].as_str() == Some(&p.device) || m["toDeviceId"].as_str() == Some(&p.device)
}
fn unread(m: &Value) -> bool {
    m["direction"].as_str() != Some("out") && m["readAt"].is_null()
}
#[derive(Clone, Debug)]
pub struct Command {
    serial: u64,
    kind: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    Refresh,
    Send {
        peer: String,
        body: String,
        revision: u64,
    },
    Read {
        peer: String,
        messages: Vec<Value>,
    },
}
pub struct Update {
    command: Command,
    result: Result<Response, String>,
}
enum Response {
    Snapshot {
        peers: Vec<Value>,
        messages: Vec<Value>,
        warnings: Vec<String>,
    },
    Sent(Value),
    Read,
}
#[derive(Clone)]
enum Action {
    Refresh,
    Select(String),
    Back,
    Search,
    Clear,
    Send,
    MarkRead,
    AllowResend,
}
struct State {
    root: Entity,
    list: Entity,
    header: Entity,
    thread: Entity,
    compose: Entity,
    sidebar: Entity,
    detail: Entity,
    search: Entity,
    editor: Option<Entity>,
    send: Option<Entity>,
    peers: Vec<Peer>,
    messages: Vec<Value>,
    selected: Option<String>,
    drafts: HashMap<String, String>,
    actions: HashMap<u64, Action>,
    queue: Vec<Command>,
    read: Option<u64>,
    mutation: Option<u64>,
    uncertain: HashMap<String, String>,
    status: String,
    warnings: Vec<String>,
    query: String,
    revisions: HashMap<String, u64>,
    persistent: Vec<u64>,
    started: bool,
    redraw: bool,
    narrow: bool,
    composer_peer: Option<String>,
    scroll_peer: Option<String>,
}
fn empty(root: Entity) -> State {
    State {
        root,
        list: root,
        header: root,
        thread: root,
        compose: root,
        sidebar: root,
        detail: root,
        search: root,
        editor: None,
        send: None,
        peers: vec![],
        messages: vec![],
        selected: None,
        drafts: HashMap::new(),
        actions: HashMap::new(),
        queue: vec![],
        read: None,
        mutation: None,
        uncertain: HashMap::new(),
        status: String::new(),
        warnings: vec![],
        query: String::new(),
        revisions: HashMap::new(),
        persistent: vec![],
        started: false,
        redraw: true,
        narrow: false,
        composer_peer: None,
        scroll_peer: None,
    }
}
impl State {
    fn enqueue(&mut self, kind: Kind) -> u64 {
        let serial = next();
        self.queue.push(Command { serial, kind });
        serial
    }
    fn refresh(&mut self) {
        if self.read.is_none() && self.mutation.is_none() {
            self.read = Some(self.enqueue(Kind::Refresh));
            self.redraw = true;
        }
    }
    fn peer(&self) -> Option<&Peer> {
        self.peers
            .iter()
            .find(|p| Some(&p.id) == self.selected.as_ref())
    }
    fn draft(&self) -> &str {
        self.selected
            .as_ref()
            .and_then(|id| self.drafts.get(id))
            .map(String::as_str)
            .unwrap_or("")
    }
}
pub fn mount(w: &mut World, parent: Entity) {
    view::mount(w, parent);
}
pub fn activate(w: &mut World) {
    if let Some(s) = w.resource_mut::<State>() {
        if !s.started {
            s.started = true;
        }
        s.refresh();
    }
}
fn capture(w: &World, s: &mut State) {
    if let (Some(id), Some(label)) = (&s.composer_peer, s.editor.and_then(|e| w.get::<Label>(e))) {
        if s.drafts.get(id) != Some(&label.text) {
            *s.revisions.entry(id.clone()).or_default() += 1;
            s.drafts.insert(id.clone(), label.text.clone());
        }
    }
}
pub fn can_deactivate(w: &World) -> bool {
    w.resource::<State>().is_none_or(|s| {
        [Some(s.search), s.editor]
            .into_iter()
            .flatten()
            .all(|e| !w.has::<ImePreview>(e))
    })
}
fn with_state(w: &mut World, f: impl FnOnce(&mut World, &mut State)) {
    let Some(s) = w.resource_mut::<State>() else {
        return;
    };
    let mut s = std::mem::replace(s, empty(s.root));
    f(w, &mut s);
    w.insert_resource(s);
}
pub fn sync(w: &mut World) {
    let can = can_deactivate(w);
    with_state(w, |w, s| {
        capture(w, s);
        let narrow = w.resource::<Viewport>().is_some_and(|v| v.width < 900.);
        if narrow != s.narrow {
            s.narrow = narrow;
            s.redraw = true;
        }
        if s.redraw && can {
            view::render(w, s);
            s.redraw = false;
        }
        if let Some(e) = s.send {
            let enabled = s.peer().is_some_and(|p| p.ready || p.inbound)
                && !s.draft().trim().is_empty()
                && s.mutation.is_none()
                && s.read.is_none()
                && can
                && !s
                    .selected
                    .as_ref()
                    .is_some_and(|p| s.uncertain.contains_key(p));
            if enabled {
                w.remove::<ControlDisabled>(e);
            } else {
                w.insert(e, ControlDisabled);
            }
        }
    });
}
pub fn action(w: &mut World, intent: &HostIntent) -> bool {
    let HostIntent::Command(id) = intent else {
        return false;
    };
    let Some(a) = w
        .resource::<State>()
        .and_then(|s| s.actions.get(id))
        .cloned()
    else {
        return false;
    };
    if !can_deactivate(w) {
        return true;
    }
    with_state(w, |w, s| {
        capture(w, s);
        match a {
            Action::Refresh => s.refresh(),
            Action::Select(id) => {
                if s.peers.iter().any(|p| p.id == id) {
                    s.selected = Some(id);
                }
            }
            Action::Back => s.selected = None,
            Action::Search => {
                s.query = w
                    .get::<Label>(s.search)
                    .map(|l| l.text.trim().to_lowercase())
                    .unwrap_or_default()
            }
            Action::Clear => {
                s.query.clear();
                w.get_mut::<Label>(s.search).unwrap().text.clear();
            }
            Action::Send if s.mutation.is_none() && s.read.is_none() => {
                if let Some(p) = s
                    .peer()
                    .cloned()
                    .filter(|p| (p.ready || p.inbound) && !s.uncertain.contains_key(&p.id))
                {
                    let body = s.draft().trim().to_string();
                    if !body.is_empty() {
                        s.mutation = Some(s.enqueue(Kind::Send {
                            revision: *s.revisions.get(&p.id).unwrap_or(&0),
                            peer: p.id,
                            body,
                        }));
                        s.status = "Sending…".into();
                    }
                }
            }
            Action::MarkRead if s.mutation.is_none() && s.read.is_none() => {
                if let Some(p) = s.peer().cloned() {
                    let messages = s
                        .messages
                        .iter()
                        .filter(|m| involves(m, &p) && unread(m))
                        .cloned()
                        .collect::<Vec<_>>();
                    if !messages.is_empty() {
                        s.mutation = Some(s.enqueue(Kind::Read {
                            peer: p.id,
                            messages,
                        }));
                        s.status = "Marking read…".into();
                    }
                }
            }
            Action::AllowResend => {
                if let Some(id) = &s.selected {
                    s.uncertain.remove(id);
                }
                s.status = "Send unlocked. Check the refreshed thread before sending again.".into();
            }
            _ => {}
        }
        s.redraw = true;
    });
    true
}
pub fn take_commands(w: &mut World) -> Vec<Command> {
    w.resource_mut::<State>()
        .map(|s| std::mem::take(&mut s.queue))
        .unwrap_or_default()
}
pub fn apply(w: &mut World, u: Update) {
    with_state(w, |w, s| {
        capture(w, s);
        let c = u.command;
        match c.kind {
            Kind::Refresh if s.read == Some(c.serial) => {
                s.read = None;
                match u.result {
                    Ok(Response::Snapshot {
                        peers,
                        messages,
                        warnings,
                    }) => {
                        s.peers = peers.iter().filter_map(Peer::parse).collect();
                        s.messages = messages;
                        s.warnings = warnings;
                        s.messages.sort_by(|a, b| {
                            text(a, "sentAt")
                                .cmp(&text(b, "sentAt"))
                                .then_with(|| text(a, "id").cmp(&text(b, "id")))
                        });
                        if s.selected
                            .as_ref()
                            .is_some_and(|id| !s.peers.iter().any(|p| &p.id == id))
                        {
                            s.selected = None;
                        }
                        s.status = "Updated · use Refresh for new messages".into();
                    }
                    Err(e) => s.status = format!("Refresh failed; keeping the previous view. {e}"),
                    _ => {}
                }
            }
            Kind::Send {
                peer,
                body,
                revision,
            } if s.mutation == Some(c.serial) => {
                s.mutation = None;
                match u.result {
                    Ok(Response::Sent(m)) => {
                        if s.revisions.get(&peer).copied().unwrap_or(0) == revision
                            && s.drafts.get(&peer).is_some_and(|d| d.trim() == body)
                            && !s.editor.is_some_and(|e| w.has::<ImePreview>(e))
                        {
                            s.drafts.insert(peer.clone(), String::new());
                            if s.composer_peer.as_ref() == Some(&peer) {
                                if let Some(e) = s.editor {
                                    w.get_mut::<Label>(e).unwrap().text.clear();
                                }
                            }
                        }
                        s.messages.push(m);
                        s.status = "Sent".into();
                        s.refresh();
                    }
                    Err(e) => {
                        s.uncertain.insert(peer, e.clone());
                        s.status =
                            format!("Send not confirmed. Draft kept; no automatic retry. {e}");
                    }
                    _ => {}
                }
            }
            Kind::Read { .. } if s.mutation == Some(c.serial) => {
                s.mutation = None;
                match u.result {
                    Ok(Response::Read) => {
                        s.status = "Marked read".into();
                        s.refresh();
                    }
                    Err(e) => {
                        s.status =
                            format!("Read status not confirmed. Refresh before retrying. {e}")
                    }
                    _ => {}
                }
            }
            _ => return,
        }
        s.redraw = true;
    });
}
pub async fn execute(
    client: &std::sync::Arc<medousa_sdk::MedousaClient>,
    command: Command,
) -> Update {
    let service = medousa_desktop_host::peers::PeerService::new(client.clone());
    let result = match &command.kind {
        Kind::Refresh => service.snapshot().await.and_then(|s| {
            Ok(Response::Snapshot {
                peers: s
                    .peers
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?,
                messages: s.messages,
                warnings: s.errors,
            })
        }),
        Kind::Send { peer, body, .. } => service.send(peer, body).await.map(Response::Sent),
        Kind::Read { messages, .. } => {
            let mut failure = None;
            for message in messages {
                if let Err(e) = service.mark_read(message).await {
                    failure = Some(e);
                    break;
                }
            }
            match failure {
                Some(e) => Err(e),
                None => Ok(Response::Read),
            }
        }
    };
    Update { command, result }
}
#[cfg(test)]
#[path = "peers_tests.rs"]
mod tests;
