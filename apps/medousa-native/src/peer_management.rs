//! Peer lifecycle UI. Fields survive redraws; mutations require a separate confirmation.
use crate::native_style as visual;
use medousa_desktop_host::peer_management::Request;
use rndr_core::*;
use rndr_ui::{Typography, controls::HostIntent, text as ui_text};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
static NEXT: AtomicU64 = AtomicU64::new(0x504d_0000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
#[derive(Clone)]
pub struct Command {
    serial: u64,
    request: Request,
}
pub struct Update {
    command: Command,
    result: Result<Value, String>,
    qr: Option<QrPixels>,
}
#[derive(Clone)]
struct QrPixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}
#[derive(Clone)]
enum Action {
    Close,
    Connect,
    Trust,
    Refresh,
    Discover,
    Invite,
    Qr,
    LanStatus,
    Lan(bool),
    Share(bool, String),
    ChooseUrl(String),
    Select(String),
    Pair,
    Forget,
    Policy,
    Confirm,
    Cancel,
    Unlock,
}
#[derive(Clone)]
struct State {
    root: Entity,
    connect: Entity,
    trust: Entity,
    status_view: Entity,
    discovered_view: Entity,
    peers_view: Entity,
    confirm_view: Entity,
    invite: Entity,
    url: Entity,
    name: Entity,
    local_invite: Entity,
    qr_view: Entity,
    qr: Option<(QrPixels, String)>,
    item: Entity,
    expires: Entity,
    idle: Entity,
    open: bool,
    trust_tab: bool,
    busy: Option<u64>,
    uncertain: bool,
    refreshed: bool,
    dirty: bool,
    peers: Vec<Value>,
    discovered: Vec<Value>,
    selected: Option<String>,
    pending: Option<Request>,
    status: String,
    actions: HashMap<u64, Action>,
    controls: HashMap<u64, Entity>,
    persistent: Vec<u64>,
    queue: Vec<Command>,
}
fn panel(w: &mut World, p: Entity, l: Layout) -> Entity {
    let e = w.spawn((l,));
    set_parent(w, e, p);
    e
}
fn column(w: &mut World, p: Entity) -> Entity {
    panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::column()
        },
    )
}
fn row(w: &mut World, p: Entity) -> Entity {
    let e = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    w.insert(e, Wrap { line_gap: 6. });
    e
}
fn label(w: &mut World, p: Entity, t: &str, k: Typography) -> Entity {
    let e = ui_text(w, p, t, k).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
    e
}
fn field(w: &mut World, p: Entity, title: &str, value: &str) -> Entity {
    label(w, p, title, Typography::Secondary);
    let e = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(38.),
            padding: 9.,
            ..Layout::column()
        },
        Label::new(value),
        Focusable,
        Prompt {
            submit_on_enter: false,
            max_lines: 1,
            ..Default::default()
        },
        Placeholder::new(title),
        Semantics {
            name: Some(title.into()),
            ..Default::default()
        },
    ));
    set_parent(w, e, p);
    visual::plane(w, e, visual::raised());
    w.insert(e, Radius(8.));
    e
}
fn button(w: &mut World, p: Entity, s: &mut State, t: &str, a: Action, enabled: bool) {
    let id = next();
    let h = visual::document_action(w, p, id, t, enabled);
    s.actions.insert(id, a);
    s.controls.insert(id, h.root);
}
fn clear(w: &mut World, p: Entity) {
    if w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|e| crate::surfaces::descendant(w, e, p))
    {
        w.resource_mut::<Focus>().unwrap().entity = None;
    }
    for e in children_of(w, p) {
        despawn(w, e);
    }
}
fn value(w: &World, e: Entity) -> String {
    w.get::<Label>(e)
        .map(|l| l.text.trim().to_string())
        .unwrap_or_default()
}
fn set(w: &mut World, e: Entity, v: &str) {
    w.get_mut::<Label>(e).unwrap().text = v.into();
}
fn text(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().into()
}
fn with(w: &mut World, f: impl FnOnce(&mut World, &mut State)) {
    let Some(mut s) = w.resource::<State>().cloned() else {
        return;
    };
    f(w, &mut s);
    w.insert_resource(s);
}
pub fn mount(w: &mut World, parent: Entity) {
    let root = panel(
        w,
        parent,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 16.,
            gap: 12.,
            ..Layout::column()
        },
    );
    visual::flat(w, root, visual::canvas());
    w.insert(root, Hidden);
    label(w, root, "Peer connections", Typography::Title);
    let controls = row(w, root);
    let status_view = column(w, root);
    let body = panel(
        w,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 12.,
            ..Layout::column()
        },
    );
    w.insert(body, Scroll::default());
    let connect = column(w, body);
    label(w, connect, "Connect a workshop", Typography::Title);
    label(
        w,
        connect,
        "Discovery only finds addresses. Paste an invite from the other workshop to verify its identity. Pairing here creates a peer, never a portal or execution grant.",
        Typography::Secondary,
    );
    let discover_controls = row(w, connect);
    let discovered_view = column(w, connect);
    let url = field(w, connect, "Workshop URL", "http://");
    let invite = field(w, connect, "Pairing invite URL", "");
    let name = field(w, connect, "Your peer name", "Medousa Peer");
    let pair_controls = row(w, connect);
    label(w, connect, "Invite another device", Typography::Title);
    label(
        w,
        connect,
        "Invite links are sensitive and expire. Share only with the intended device. Opening this panel does not create or rotate an invite. Check LAN status below and explicitly enable pairing if needed.",
        Typography::Secondary,
    );
    let invite_controls = row(w, connect);
    let local_invite = field(w, connect, "Your current invite (copy to share)", "");
    let qr_view = column(w, connect);
    label(w, connect, "LAN pairing window", Typography::Title);
    label(
        w,
        connect,
        "Changing LAN pairing restarts your managed personal engine. Enabling exposes it on this network; disable after pairing. Private tunnels remain available. Custom/unmanaged engines are never restarted here.",
        Typography::Secondary,
    );
    let lan_controls = row(w, connect);
    let trust = column(w, body);
    w.insert(trust, Hidden);
    label(w, trust, "Trusted peers", Typography::Title);
    let peers_view = column(w, trust);
    label(
        w,
        trust,
        "Send an item to the selected peer",
        Typography::Title,
    );
    label(
        w,
        trust,
        "Enter a note path or artifact ID from this workshop (not a local file). Sharing sends the full item; recipients auto-import with rename. Review the destination before confirming. No execution grant is included.",
        Typography::Secondary,
    );
    let item = field(w, trust, "Note path or artifact ID", "");
    let share_controls = column(w, trust);
    label(
        w,
        trust,
        "Trust policy for selected incoming peer",
        Typography::Title,
    );
    label(
        w,
        trust,
        "Expiry and inactivity settings apply only to devices connected to this workshop. The other workshop controls outbound trust. Blank values mean until removed / never expire from inactivity.",
        Typography::Secondary,
    );
    let expires = field(w, trust, "Trust expires (RFC3339 UTC, or blank)", "");
    let idle = field(w, trust, "Expire after unused days (or blank)", "");
    let trust_controls = row(w, trust);
    let confirm_view = column(w, root);
    let mut s = State {
        root,
        connect,
        trust,
        status_view,
        discovered_view,
        peers_view,
        confirm_view,
        invite,
        url,
        name,
        local_invite,
        qr_view,
        qr: None,
        item,
        expires,
        idle,
        open: false,
        trust_tab: false,
        busy: None,
        uncertain: false,
        refreshed: false,
        dirty: true,
        peers: vec![],
        discovered: vec![],
        selected: None,
        pending: None,
        status: String::new(),
        actions: HashMap::new(),
        controls: HashMap::new(),
        persistent: vec![],
        queue: vec![],
    };
    for (title, a) in [
        ("‹ Conversations", Action::Close),
        ("Connect", Action::Connect),
        ("Trusted peers", Action::Trust),
        ("Refresh trust", Action::Refresh),
    ] {
        button(w, controls, &mut s, title, a, true);
    }
    button(
        w,
        discover_controls,
        &mut s,
        "Find nearby workshops",
        Action::Discover,
        true,
    );
    button(
        w,
        pair_controls,
        &mut s,
        "Review pairing",
        Action::Pair,
        true,
    );
    button(
        w,
        invite_controls,
        &mut s,
        "Get current invite",
        Action::Invite,
        true,
    );
    button(
        w,
        trust_controls,
        &mut s,
        "Review trust policy",
        Action::Policy,
        true,
    );
    button(
        w,
        trust_controls,
        &mut s,
        "Forget selected peer…",
        Action::Forget,
        true,
    );
    button(
        w,
        invite_controls,
        &mut s,
        "Show pairing QR",
        Action::Qr,
        true,
    );
    for (title, a) in [
        ("Check LAN status", Action::LanStatus),
        ("Enable LAN…", Action::Lan(true)),
        ("Disable LAN…", Action::Lan(false)),
    ] {
        button(w, lan_controls, &mut s, title, a, true);
    }
    for (artifact, kind) in [(false, "note"), (true, "artifact")] {
        let controls = row(w, share_controls);
        for mode in ["share", "ask", "bring"] {
            button(
                w,
                controls,
                &mut s,
                &format!("{mode} {kind}…"),
                Action::Share(artifact, mode.into()),
                true,
            );
        }
    }
    s.persistent = s.actions.keys().copied().collect();
    w.insert_resource(s);
}
pub fn active(w: &World) -> bool {
    w.resource::<State>().is_some_and(|s| s.open)
}
pub fn can_deactivate(w: &World) -> bool {
    w.resource::<State>().is_none_or(|s| {
        [
            s.invite,
            s.url,
            s.name,
            s.expires,
            s.idle,
            s.local_invite,
            s.item,
        ]
        .iter()
        .all(|e| !w.has::<ImePreview>(*e))
    })
}
pub fn open(w: &mut World) {
    with(w, |w, s| {
        s.open = true;
        s.dirty = true;
        w.remove::<Hidden>(s.root);
        if s.busy.is_none() {
            enqueue(s, Request::Status);
        }
    });
}
fn enqueue(s: &mut State, r: Request) {
    let serial = next();
    s.busy = Some(serial);
    s.pending = None;
    s.status = "Working…".into();
    s.queue.push(Command { serial, request: r });
    s.dirty = true;
}
fn selected(s: &State) -> Option<&Value> {
    s.peers
        .iter()
        .find(|p| Some(&text(p, "id")) == s.selected.as_ref())
}
fn stage(w: &World, s: &State, a: &Action) -> Result<Request, String> {
    match a {
        Action::Lan(enabled) => Ok(Request::LanEnabled { enabled: *enabled }),
        Action::Share(artifact, mode) => {
            let p = selected(s).ok_or("Select a trusted peer first")?;
            let item = value(w, s.item);
            if item.is_empty() {
                return Err("Enter a note path or artifact ID first".into());
            }
            Ok(Request::Share {
                id: text(p, "id"),
                device: text(p, "device"),
                pairing: text(p, "pairing"),
                item,
                artifact: *artifact,
                mode: mode.clone(),
            })
        }
        Action::Pair => {
            let invite = value(w, s.invite);
            let url = value(w, s.url);
            if invite.is_empty() || url == "http://" || url.is_empty() {
                return Err("Paste an invite and workshop URL first".into());
            }
            Ok(Request::Pair {
                invite,
                url,
                name: value(w, s.name),
            })
        }
        Action::Forget | Action::Policy => {
            let p = selected(s).ok_or("Select a trusted peer first")?;
            let device = text(p, "device");
            let pairing = text(p, "pairing");
            if matches!(a, Action::Forget) {
                return Ok(Request::Forget {
                    id: text(p, "id"),
                    device,
                    pairing,
                });
            }
            if !p["inbound"].as_bool().unwrap_or(false) {
                return Err("Outbound trust policy belongs to the other workshop".into());
            }
            let expires = value(w, s.expires);
            let days = value(w, s.idle);
            let idle = if days.is_empty() {
                None
            } else {
                Some(
                    days.parse::<u64>()
                        .ok()
                        .filter(|d| (1..=3650).contains(d))
                        .and_then(|d| d.checked_mul(86400))
                        .ok_or("Use whole inactivity days between 1 and 3650")?,
                )
            };
            if !expires.is_empty() {
                let d = chrono::DateTime::parse_from_rfc3339(&expires)
                    .map_err(|_| "Use an RFC3339 timestamp, e.g. 2027-01-01T00:00:00Z")?;
                if d <= chrono::Utc::now() {
                    return Err("Trust expiry must be in the future".into());
                }
            }
            Ok(Request::Policy {
                device,
                pairing,
                expires: (!expires.is_empty()).then_some(expires),
                idle,
            })
        }
        _ => Err("Unsupported action".into()),
    }
}
pub fn action(w: &mut World, intent: &HostIntent) -> bool {
    if !active(w) {
        return false;
    }
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
    with(w, |w, s| {
        match a {
            Action::Close => {
                s.open = false;
                s.pending = None;
                set(w, s.local_invite, "");
                s.qr = None;
                clear(w, s.qr_view);
                w.insert(s.root, Hidden);
            }
            Action::Connect => {
                s.trust_tab = false;
                s.pending = None;
            }
            Action::Trust => {
                s.trust_tab = true;
                s.pending = None;
            }
            Action::Cancel => s.pending = None,
            Action::Refresh if s.busy.is_none() => {
                s.refreshed = false;
                enqueue(s, Request::Status);
            }
            Action::Discover if s.busy.is_none() => enqueue(s, Request::Discover),
            Action::Invite if s.busy.is_none() => enqueue(s, Request::Invite),
            Action::Qr if s.busy.is_none() => {
                s.qr = None;
                clear(w, s.qr_view);
                enqueue(s, Request::QrImage);
            }
            Action::LanStatus if s.busy.is_none() => enqueue(s, Request::LanStatus),
            Action::ChooseUrl(url) if s.busy.is_none() => {
                set(w, s.url, &url);
                s.pending = None;
                s.status = "Address selected — paste that workshop's invite to pair.".into();
            }
            Action::Select(id) if s.busy.is_none() => {
                s.selected = Some(id);
                s.pending = None;
                if let Some(p) = selected(s) {
                    set(w, s.expires, &text(p, "expires"));
                    set(
                        w,
                        s.idle,
                        &p["idle"]
                            .as_u64()
                            .map(|v| (v as f64 / 86400.).to_string())
                            .unwrap_or_default(),
                    );
                }
            }
            Action::Pair | Action::Forget | Action::Policy | Action::Share(..) | Action::Lan(_)
                if s.busy.is_none() && !s.uncertain =>
            {
                match stage(w, s, &a) {
                    Ok(r) => {
                        s.pending = Some(r);
                        s.status =
                            "Review the exact operation below. Nothing has changed yet.".into();
                    }
                    Err(e) => s.status = e,
                }
            }
            Action::Confirm if s.busy.is_none() && !s.uncertain => {
                if let Some(r) = s.pending.take() {
                    enqueue(s, r);
                }
            }
            Action::Unlock if s.busy.is_none() && s.refreshed => {
                s.uncertain = false;
                s.status="Actions unlocked. Verify current trust before trying again; nothing was replayed.".into();
            }
            _ => {}
        }
        s.dirty = true;
    });
    true
}
pub fn sync(w: &mut World) {
    with(w, |w, s| {
        if s.qr.as_ref().is_some_and(|(_, expires)| {
            chrono::DateTime::parse_from_rfc3339(expires).map_or(true, |d| d <= chrono::Utc::now())
        }) {
            s.qr = None;
            clear(w, s.qr_view);
            set(w, s.local_invite, "");
            s.status = "Pairing QR expired — retrieve a fresh invite.".into();
            s.dirty = true;
        }
    });
    if !can_deactivate(w) || !w.resource::<State>().is_some_and(|s| s.open && s.dirty) {
        return;
    }
    with(w, |w, s| {
        if !s.dirty || !s.open {
            return;
        }
        s.dirty = false;
        if s.trust_tab {
            w.insert(s.connect, Hidden);
            w.remove::<Hidden>(s.trust);
        } else {
            w.remove::<Hidden>(s.connect);
            w.insert(s.trust, Hidden);
        }
        s.actions.retain(|id, _| s.persistent.contains(id));
        s.controls.retain(|id, _| s.persistent.contains(id));
        for p in [
            s.status_view,
            s.discovered_view,
            s.peers_view,
            s.confirm_view,
        ] {
            clear(w, p);
        }
        label(w, s.status_view, &s.status, Typography::Secondary);
        if s.uncertain {
            label(
                w,
                s.status_view,
                "Previous change was not confirmed. No automatic retry. Refresh trust and inspect both workshops before unlocking actions.",
                Typography::Secondary,
            );
            button(
                w,
                s.status_view,
                s,
                "Unlock after checking",
                Action::Unlock,
                s.busy.is_none() && s.refreshed,
            );
        }
        for p in s.discovered.clone() {
            let url = text(&p, "daemonUrl");
            label(
                w,
                s.discovered_view,
                &format!("{} · {} · untrusted discovery", text(&p, "peerName"), url),
                Typography::Secondary,
            );
            if !url.is_empty() {
                button(
                    w,
                    s.discovered_view,
                    s,
                    "Use this address",
                    Action::ChooseUrl(url),
                    s.busy.is_none(),
                );
            }
        }
        if s.peers.is_empty() {
            label(
                w,
                s.peers_view,
                "No loaded peers. Refresh trust to check this workshop.",
                Typography::Secondary,
            );
        }
        for p in s.peers.clone() {
            let id = text(&p, "id");
            let chosen = s.selected.as_ref() == Some(&id);
            button(
                w,
                s.peers_view,
                s,
                &format!("{}{}", if chosen { "✓ " } else { "" }, text(&p, "name")),
                Action::Select(id),
                s.busy.is_none(),
            );
            label(
                w,
                s.peers_view,
                &format!(
                    "{} · {} · pairing {}{}",
                    if p["inbound"] == true {
                        "Connected to you"
                    } else {
                        "Outbound — removal forgets daemon connection credentials only"
                    },
                    text(&p, "device"),
                    text(&p, "pairing"),
                    if p["active"] == false {
                        " · expired/inactive"
                    } else {
                        ""
                    }
                ),
                Typography::Secondary,
            );
        }
        if let Some(r) = s.pending.clone() {
            let summary = match r {
                Request::LanEnabled { enabled } => format!(
                    "{} LAN pairing? This restarts the managed personal engine and interrupts active connections. {}",
                    if enabled { "Enable" } else { "Disable" },
                    if enabled {
                        "The engine becomes reachable from this network. Turn it off after pairing."
                    } else {
                        "The engine returns to loopback-only. Paired devices use private tunnels."
                    }
                ),
                Request::Share {
                    id,
                    device,
                    pairing,
                    item,
                    artifact,
                    mode,
                } => format!(
                    "{mode} {} `{item}` to {id}?\nDevice: {device}\nPairing: {pairing}\nThe full item is sent and auto-imported with rename on the recipient. No execution permissions change.",
                    if artifact { "artifact" } else { "note" }
                ),
                Request::Pair { url, .. } => format!(
                    "Pair with {url} as a peer? The signed invite will verify identity. No execution permissions are granted."
                ),
                Request::Forget {
                    id,
                    device,
                    pairing,
                } => format!(
                    "Forget {id}\nDevice: {device}\nPairing: {pairing}\nIncoming: revoke this device's trust. Outbound: remove this daemon's connection credentials only, not trust held by the remote workshop. Message drafts stay in this window."
                ),
                Request::Policy {
                    device,
                    pairing,
                    expires,
                    idle,
                } => format!(
                    "Update trust for {device}\nPairing: {pairing}\nThis expires its current session; the peer must authenticate again.\nExpires: {} · inactive after: {}",
                    expires.unwrap_or("until removed".into()),
                    idle.map(|n| format!("{} days", n / 86400))
                        .unwrap_or("never".into())
                ),
                _ => String::new(),
            };
            label(w, s.confirm_view, &summary, Typography::Secondary);
            let controls = row(w, s.confirm_view);
            button(w, controls, s, "Confirm change", Action::Confirm, true);
            button(w, controls, s, "Cancel", Action::Cancel, true);
        }
        for (id, e) in &s.controls {
            let enabled = match s.actions.get(id) {
                Some(Action::Close | Action::Connect | Action::Trust | Action::Cancel) => true,
                Some(
                    Action::Pair
                    | Action::Forget
                    | Action::Policy
                    | Action::Confirm
                    | Action::Share(..)
                    | Action::Lan(_),
                ) => s.busy.is_none() && !s.uncertain,
                Some(Action::Unlock) => s.busy.is_none() && s.refreshed,
                _ => s.busy.is_none(),
            };
            if enabled {
                w.remove::<ControlDisabled>(*e);
            } else {
                w.insert(*e, ControlDisabled);
            }
        }
    });
}
pub fn take_commands(w: &mut World) -> Vec<Command> {
    w.resource_mut::<State>()
        .map(|s| std::mem::take(&mut s.queue))
        .unwrap_or_default()
}
pub fn apply(w: &mut World, u: Update) {
    with(w, |w, s| {
        if s.busy != Some(u.command.serial) {
            return;
        }
        s.busy = None;
        match u.result {
            Ok(v) => match u.command.request {
                Request::Status => {
                    s.peers = v["peers"].as_array().cloned().unwrap_or_default();
                    s.refreshed = true;
                    s.pending = None;
                    if !s.peers.iter().any(|p| Some(text(p, "id")) == s.selected) {
                        s.selected = None;
                    }
                    s.status =
                        "Trust refreshed. Discovery and invite retrieval are explicit.".into();
                }
                Request::Discover => {
                    s.discovered = v["workshops"].as_array().cloned().unwrap_or_default();
                    s.status = format!(
                        "{} nearby workshops. Discovery does not authenticate a device.",
                        s.discovered.len()
                    );
                }
                Request::LanStatus | Request::LanEnabled { .. } => {
                    s.status = text(&v, "message");
                    s.qr = None;
                    clear(w, s.qr_view);
                    set(w, s.local_invite, "");
                }
                Request::Share { .. } => {
                    s.status = "Item sent. Return to conversations and refresh to see it. No automatic resend.".into();
                }
                Request::Invite | Request::QrImage => {
                    s.qr = None;
                    clear(w, s.qr_view);
                    if s.open {
                        set(w, s.local_invite, &text(&v, "url"));
                    }
                    if s.open {
                        if let Some(pixels) = u.qr {
                            clear(w, s.qr_view);
                            let id = next();
                            let _ = rndr_render::load_rgba(
                                w,
                                id,
                                pixels.width,
                                pixels.height,
                                pixels.rgba.clone(),
                            );
                            let image = w.spawn((
                                Layout {
                                    width: Size::Px(240.),
                                    height: Size::Px(240.),
                                    ..Layout::column()
                                },
                                ImageView::new(id),
                            ));
                            set_parent(w, image, s.qr_view);
                            s.qr = Some((pixels, text(&v, "expiresAt")));
                        }
                    }
                    s.status = format!(
                        "Invite expires {} · code {}. Keep the link private.",
                        text(&v, "expiresAt"),
                        text(&v, "shortCode")
                    );
                }
                _ => {
                    s.refreshed = false;
                    s.status = "Change completed. Refresh trust to see current devices.".into();
                    if matches!(u.command.request, Request::Pair { .. }) {
                        set(w, s.invite, "");
                    }
                }
            },
            Err(e) => {
                if u.command.request.mutation() {
                    s.uncertain = true;
                    s.refreshed = false;
                    s.pending = None;
                }
                s.status = format!("Operation not confirmed. {e}");
            }
        }
        s.dirty = true;
    });
}
pub async fn execute(client: &Arc<medousa_sdk::MedousaClient>, command: Command) -> Update {
    let mut result =
        medousa_desktop_host::peer_management::PeerManagementService::new(client.clone())
            .execute(&command.request)
            .await;
    let mut qr = None;
    if matches!(command.request, Request::QrImage) {
        if let Ok(v) = &result {
            match decode_qr_pixels(v) {
                Ok(p) => qr = Some(p),
                Err(e) => result = Err(e),
            }
        }
    }
    Update {
        command,
        result,
        qr,
    }
}
fn decode_qr_pixels(v: &Value) -> Result<QrPixels, String> {
    use base64::Engine;
    let encoded = v["pngBase64"].as_str().ok_or("QR image missing")?;
    if encoded.len() > 512 * 1024 {
        return Err("QR image too large".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid QR image")?;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "Invalid QR image")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Invalid or oversized QR image")?
        .into_rgba8();
    Ok(QrPixels {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}
#[cfg(test)]
#[path = "peer_management_tests.rs"]
mod tests;
