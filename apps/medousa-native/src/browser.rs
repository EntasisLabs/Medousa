//! Device-local browsing. No daemon credentials, artifact bridge or agent tools.
use rndr_core::*;
use rndr_ui::{
    Typography, UiId,
    controls::{Button, ButtonProps, ButtonVariant, HostIntent, IconButton},
    tabs::{TabId, TabSpec, TabStrip, TabStripEvent, TabStripEventKind, TabStripProps},
};
use std::time::Duration;

const MAX_TABS: usize = 16;
#[derive(Clone)]
struct Tab {
    id: u64,
    url: String,
    title: String,
    status: String,
    back: bool,
    forward: bool,
    loading: bool,
}
impl Tab {
    fn new(id: u64) -> Self {
        Self {
            id,
            url: String::new(),
            title: "New tab".into(),
            status: "Enter an address or search the web".into(),
            back: false,
            forward: false,
            loading: false,
        }
    }
}
#[derive(Clone)]
enum Command {
    Navigate(String),
    Back,
    Forward,
    Reload,
    Stop,
}
#[derive(Clone)]
enum Action {
    New,
    Select(u64),
    Close(u64),
    Go,
    Command(Command),
}
impl Component for Action {}
#[derive(Clone)]
struct State {
    tabs: Vec<Tab>,
    active: u64,
    next: u64,
    tabs_root: Entity,
    address: Entity,
    viewport: Entity,
    status: Entity,
    commands: Vec<(u64, Command)>,
    dirty: bool,
}
fn row(w: &mut World, p: Entity, height: f32) -> Entity {
    let e = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(height),
        padding: 6.,
        gap: 6.,
        ..Layout::row()
    },));
    set_parent(w, e, p);
    e
}
fn button(w: &mut World, p: Entity, text: &str, action: Action, key: u64, width: f32) {
    let mut props = ButtonProps::new(text, HostIntent::Command(key));
    props.variant = ButtonVariant::Quiet;
    props.name = Some(match &action {
        Action::New => "New browser tab".into(),
        Action::Close(_) => "Close browser tab".into(),
        Action::Go => "Navigate to address or search".into(),
        Action::Command(Command::Back) => "Back".into(),
        Action::Command(Command::Forward) => "Forward".into(),
        Action::Command(Command::Reload) => "Reload page".into(),
        Action::Command(Command::Stop) => "Stop loading".into(),
        _ => text.into(),
    });
    let icon = match &action {
        Action::New => Some(Icon::Plus),
        Action::Close(_) => Some(Icon::X),
        Action::Command(Command::Back) => Some(Icon::ChevronLeft),
        Action::Command(Command::Forward) => Some(Icon::ChevronRight),
        Action::Command(Command::Reload) => Some(Icon::RefreshCw),
        Action::Command(Command::Stop) => Some(Icon::X),
        _ => None,
    };
    let handle = if let Some(icon) = icon {
        let name = props.name.clone().unwrap();
        let h = IconButton::icon(w, p, UiId(key), icon, &name, props).unwrap();
        crate::native_style::action_hint(w, h.root, &name);
        h
    } else {
        Button::mount(w, p, UiId(key), props).unwrap()
    };
    let e = handle.root;
    let layout = w.get_mut::<Layout>(e).unwrap();
    layout.width = Size::Px(width);
    layout.height = Size::Fill;
    layout.padding = 0.;
    if let Some(label_box) = w.get_mut::<LabelBox>(handle.label) {
        label_box.inset_x = 2.;
        label_box.inset_y = 1.;
    }
    w.get_mut::<Label>(handle.label).unwrap().overflow = TextOverflow::Ellipsis;
    w.insert(e, action);
    w.insert(e, ActivationHandler(activate));
}
pub fn mount(w: &mut World, parent: Entity) {
    let root = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(w, root, parent);
    crate::native_style::plane(w, root, crate::native_style::canvas());
    let bar = row(w, root, 48.);
    w.get_mut::<Layout>(bar).unwrap().padding = 8.;
    w.insert(
        bar,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    crate::native_style::plane(w, bar, crate::native_style::navigation());
    button(w, bar, "←", Action::Command(Command::Back), 0x42520001, 36.);
    button(
        w,
        bar,
        "→",
        Action::Command(Command::Forward),
        0x42520002,
        36.,
    );
    let spacer = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::row()
    },));
    set_parent(w, spacer, bar);
    let address_frame = row(w, bar, 32.);
    w.get_mut::<Layout>(address_frame).unwrap().padding = 2.;
    crate::native_style::plane(w, address_frame, crate::native_style::raised());
    w.insert(address_frame, Radius(10.));
    let address = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 6.,
            ..Layout::row()
        },
        Label::new(""),
        TextField,
        Focusable,
        Placeholder::new("Search or enter address"),
        Semantics {
            name: Some("Browser address or search".into()),
            ..Default::default()
        },
    ));
    set_parent(w, address, address_frame);
    let theme = w
        .resource::<rndr_ui::UiTheme>()
        .cloned()
        .unwrap_or_default();
    w.insert(address, theme.typography(Typography::Label));
    button(
        w,
        address_frame,
        "",
        Action::Command(Command::Reload),
        0x42520003,
        28.,
    );
    button(
        w,
        address_frame,
        "",
        Action::Command(Command::Stop),
        0x42520004,
        28.,
    );
    let spacer = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::row()
    },));
    set_parent(w, spacer, bar);
    button(w, bar, "", Action::New, 0x4252ffff, 32.);
    let tabs_bar = row(w, root, 48.);
    crate::native_style::plane(w, tabs_bar, crate::native_style::navigation());
    let tabs_root = row(w, tabs_bar, 36.);
    w.get_mut::<Layout>(tabs_root).unwrap().padding = 0.;
    w.get_mut::<Layout>(tabs_root).unwrap().gap = 4.;
    w.insert(tabs_root, Scroll::default());
    w.insert(tabs_root, Clip::default());
    crate::native_style::rule(w, root);
    let status = rndr_ui::text(w, root, "", Typography::Secondary).unwrap();
    w.insert(status, Hidden);
    let viewport = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(w, viewport, root);
    w.insert_resource(State {
        tabs: vec![Tab::new(1)],
        active: 1,
        next: 2,
        tabs_root,
        address,
        viewport,
        status,
        commands: Vec::new(),
        dirty: true,
    });
    sync(w);
}
/// Only HTTP(S) is admitted from chrome; never dispatch custom schemes locally.
fn destination(raw: &str) -> Result<String, &'static str> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Enter an address or search");
    }
    if raw.chars().any(char::is_control) {
        return Err("Address contains control characters");
    }
    let lower = raw.to_ascii_lowercase();
    let local = lower.starts_with("localhost:")
        || lower.starts_with("127.0.0.1:")
        || lower.starts_with("[::1]");
    let candidate = if lower.starts_with("http://") || lower.starts_with("https://") {
        raw.to_owned()
    } else if local {
        format!("http://{raw}")
    } else if raw.contains("://")
        || raw
            .split_once(':')
            .is_some_and(|(s, _)| !s.contains('.') && s.chars().all(|c| c.is_ascii_alphabetic()))
    {
        return Err("Only HTTP and HTTPS addresses are supported");
    } else if !raw.chars().any(char::is_whitespace) && (raw.contains('.') || lower == "localhost") {
        format!("https://{raw}")
    } else {
        format!(
            "https://www.google.com/search?q={}",
            urlencoding::encode(raw)
        )
    };
    let url = reqwest::Url::parse(&candidate).map_err(|_| "Invalid web address")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use an HTTP(S) address without embedded credentials");
    }
    Ok(url.into())
}
fn activate(w: &mut World, e: Entity) {
    if crate::surfaces::active(w) != crate::surfaces::SurfaceId::Web {
        return;
    }
    let Some(action) = w.get::<Action>(e).cloned() else {
        return;
    };
    perform(w, action);
}
fn perform(w: &mut World, action: Action) {
    let Some(mut s) = w.resource::<State>().cloned() else {
        return;
    };
    match action {
        Action::New if s.tabs.len() < MAX_TABS => {
            let id = s.next;
            s.next += 1;
            s.tabs.push(Tab::new(id));
            s.active = id;
            replace_field_text(w, s.address, "");
            s.dirty = true;
            let _ = request_focus(w, s.address);
        }
        Action::Select(id) => {
            if let Some(t) = s.tabs.iter().find(|t| t.id == id) {
                s.active = id;
                replace_field_text(w, s.address, &t.url);
                s.dirty = true;
            }
        }
        Action::Close(id) => {
            s.tabs.retain(|t| t.id != id);
            s.commands.retain(|(tab, _)| *tab != id);
            if s.tabs.is_empty() {
                let id = s.next;
                s.next += 1;
                s.tabs.push(Tab::new(id));
            }
            if !s.tabs.iter().any(|t| t.id == s.active) {
                s.active = s.tabs.last().unwrap().id;
            }
            let t = s.tabs.iter().find(|t| t.id == s.active).unwrap();
            replace_field_text(w, s.address, &t.url);
            s.dirty = true;
        }
        Action::Go => {
            let raw = w
                .get::<Label>(s.address)
                .map(|l| l.text.as_str())
                .unwrap_or_default();
            match destination(raw) {
                Ok(url) => {
                    s.commands.push((s.active, Command::Navigate(url.clone())));
                    let t = s.tabs.iter_mut().find(|t| t.id == s.active).unwrap();
                    t.url = url;
                    t.loading = true;
                    t.status = "Loading…".into();
                }
                Err(error) => {
                    s.tabs.iter_mut().find(|t| t.id == s.active).unwrap().status = error.into();
                }
            }
            s.dirty = true;
        }
        Action::Command(c) => {
            let t = s.tabs.iter().find(|t| t.id == s.active).unwrap();
            let available = match c {
                Command::Back => t.back,
                Command::Forward => t.forward,
                Command::Stop => t.loading,
                Command::Reload => !t.url.is_empty(),
                Command::Navigate(_) => true,
            };
            if available {
                s.commands.push((s.active, c));
            }
        }
        _ => {}
    }
    w.insert_resource(s);
    sync(w);
    frame_wake(w).wake();
}
pub fn update(w: &mut World) {
    if crate::surfaces::active(w) != crate::surfaces::SurfaceId::Web {
        return;
    }
    let Some(s) = w.resource::<State>().cloned() else {
        return;
    };
    let requests: Vec<_> = events::<TabStripEvent>(w)
        .iter()
        .filter(|e| {
            w.get::<Parent>(e.strip)
                .is_some_and(|p| p.entity == s.tabs_root)
        })
        .map(|e| e.kind)
        .collect();
    for request in requests {
        match request {
            TabStripEventKind::Activated(id) => perform(w, Action::Select(id.0)),
            TabStripEventKind::CloseRequested(id) => perform(w, Action::Close(id.0)),
            TabStripEventKind::Reordered(m) => {
                let Some(mut state) = w.resource::<State>().cloned() else {
                    continue;
                };
                let specs: Vec<_> = state
                    .tabs
                    .iter()
                    .map(|t| TabSpec::new(t.id, &t.title))
                    .collect();
                if let Some(order) = rndr_ui::tabs::reorder_tabs(&specs, m.id, m.target, m.place) {
                    state
                        .tabs
                        .sort_by_key(|t| order.iter().position(|spec| spec.id.0 == t.id).unwrap());
                    state.dirty = true;
                    w.insert_resource(state);
                }
            }
            _ => {} // No cross-pane moves or splits in the device browser.
        }
    }
    if events::<Submitted>(w).iter().any(|e| e.entity == s.address) {
        perform(w, Action::Go);
    }
    sync(w);
}
/// Browser-only styling on RNDR's semantic strip; interaction stays RNDR-owned.
pub fn layout(w: &mut World) {
    let Some(s) = w.resource::<State>().cloned() else {
        return;
    };
    let Some(width) = w.get::<Rect>(s.tabs_root).map(|r| r.w) else {
        return;
    };
    let tab_width = (width / s.tabs.len() as f32).max(150.);
    let frame = w.get::<Parent>(s.address).unwrap().entity;
    let toolbar = w.get::<Parent>(frame).unwrap().entity;
    if let Some(available) = w.get::<Rect>(toolbar).map(|r| r.w) {
        w.get_mut::<Layout>(frame).unwrap().width = Size::Px((available - 160.).clamp(120., 680.));
    }
    let theme = w
        .resource::<rndr_ui::UiTheme>()
        .cloned()
        .unwrap_or_default();
    for e in w.entities_with::<rndr_core::Tab>() {
        // Do not restyle the application's chat/workspace strips.
        let mut ancestor = Some(e);
        let mut ours = false;
        for _ in 0..8 {
            let Some(node) = ancestor else { break };
            if node == s.tabs_root {
                ours = true;
                break;
            }
            ancestor = w.get::<Parent>(node).map(|p| p.entity);
        }
        if !ours {
            continue;
        }
        if let Some(l) = w.get_mut::<Layout>(e) {
            l.min_width = tab_width;
            l.width = Size::Px(tab_width);
        }
        w.insert(e, Radius(8.));
        for child in children_of(w, e) {
            let Some(label) = w.get::<Label>(child) else {
                continue;
            };
            let icon = label
                .text
                .chars()
                .next()
                .is_some_and(rndr_core::is_icon_scalar);
            let mut style = theme.typography(Typography::Label);
            style.align = TextAlign::Center;
            if icon {
                style.size = 16.;
                style.line_height = Some(1.);
                w.insert(
                    child,
                    LabelBox {
                        inset_x: 0.,
                        inset_y: 0.,
                        min_height: 0.,
                    },
                );
            }
            w.insert(child, style);
        }
    }
}
fn sync(w: &mut World) {
    let Some(mut s) = w.resource::<State>().cloned() else {
        return;
    };
    if s.dirty {
        let tabs: Vec<_> = s
            .tabs
            .iter()
            .map(|t| {
                TabSpec::new(
                    t.id,
                    if t.title.trim().is_empty() {
                        "New tab"
                    } else {
                        &t.title
                    },
                )
                .icon(Icon::Globe)
            })
            .collect();
        let strip = TabStrip::mount(
            w,
            s.tabs_root,
            UiId(0x42530000),
            TabStripProps::new("Browser tabs"),
            &tabs,
            TabId(s.active),
        )
        .unwrap();
        // WebKit's retained viewport is below the strip, not inside a tab panel.
        w.insert(strip.body, Hidden);
        let t = s.tabs.iter().find(|t| t.id == s.active).unwrap();
        w.get_mut::<Label>(s.status).unwrap().text = t.status.clone();
        // Routine loading/ready text is not an extra toolbar row.
        if t.status.starts_with("Enter ") || t.status == "Loading…" || t.status == "Ready" {
            w.insert(s.status, Hidden);
        } else {
            w.remove::<Hidden>(s.status);
        }
        s.dirty = false;
    }
    // One trailing reload/stop slot; unavailable navigation is visibly disabled.
    let t = s.tabs.iter().find(|t| t.id == s.active).unwrap();
    for e in w.entities_with::<Action>() {
        let Some(a) = w.get::<Action>(e) else {
            continue;
        };
        let (enabled, hidden) = match a {
            Action::Command(Command::Back) => (t.back, false),
            Action::Command(Command::Forward) => (t.forward, false),
            Action::Command(Command::Reload) => (!t.url.is_empty(), t.loading),
            Action::Command(Command::Stop) => (t.loading, !t.loading),
            Action::New => (s.tabs.len() < MAX_TABS, false),
            _ => continue,
        };
        let (key, icon, name) = match a {
            Action::Command(Command::Back) => (0x42520001, Icon::ChevronLeft, "Back"),
            Action::Command(Command::Forward) => (0x42520002, Icon::ChevronRight, "Forward"),
            Action::Command(Command::Reload) => (0x42520003, Icon::RefreshCw, "Reload page"),
            Action::Command(Command::Stop) => (0x42520004, Icon::X, "Stop loading"),
            Action::New => (0x4252ffff, Icon::Plus, "New browser tab"),
            _ => continue,
        };
        let parent = w.get::<Parent>(e).unwrap().entity;
        let mut props = ButtonProps::new("", HostIntent::Command(key));
        props.variant = ButtonVariant::Quiet;
        props.enabled = enabled;
        IconButton::icon(w, parent, UiId(key), icon, name, props).unwrap();
        if hidden {
            w.insert(e, Hidden);
        } else {
            w.remove::<Hidden>(e);
        }
    }
    w.insert_resource(s);
}
#[cfg(any(test, all(target_os = "macos", feature = "native-browser")))]
fn occluded(w: &World) -> bool {
    w.entities_with::<Modal>()
        .iter()
        .chain(w.entities_with::<AnchoredPopup>().iter())
        .any(|e| {
            let mut current = Some(*e);
            for _ in 0..128 {
                let Some(entity) = current else {
                    return true;
                };
                if w.has::<Hidden>(entity) {
                    return false;
                }
                current = w.get::<Parent>(entity).map(|p| p.entity);
            }
            true
        })
}

/// Compose both native guests without sharing their security or cookie state.
#[derive(Default)]
pub struct Host {
    artifacts: crate::artifact_preview::Host,
    #[cfg(all(target_os = "macos", feature = "native-browser"))]
    browsers: std::collections::HashMap<u64, rndr_browser::Browser>,
}
impl rndr_render::NativeHost for Host {
    fn frame(&mut self, w: &mut World, c: rndr_render::NativeHostContext<'_>) -> Option<Duration> {
        let a = self.artifacts.frame(w, c);
        let b = self.browse(w, c);
        match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
    fn detach(&mut self, w: &mut World) {
        #[cfg(all(target_os = "macos", feature = "native-browser"))]
        {
            self.browsers.clear();
        }
        self.artifacts.detach(w);
    }
}
#[cfg(all(target_os = "macos", feature = "native-browser"))]
impl Host {
    fn browse(&mut self, w: &mut World, c: rndr_render::NativeHostContext<'_>) -> Option<Duration> {
        use rndr_browser::{Browser, BrowserEvent, ClientRect, LoadOutcome};
        let active = crate::surfaces::active(w) == crate::surfaces::SurfaceId::Web;
        let visible = active && c.visible && !occluded(w);
        let Some(mut s) = w.resource::<State>().cloned() else {
            return None;
        };
        self.browsers
            .retain(|id, _| s.tabs.iter().any(|t| t.id == *id));
        for (id, cmd) in std::mem::take(&mut s.commands) {
            if let std::collections::hash_map::Entry::Vacant(e) = self.browsers.entry(id) {
                match Browser::attach(&c.window) {
                    Ok(mut browser) => {
                        browser.hide();
                        e.insert(browser);
                    }
                    Err(error) => {
                        if let Some(t) = s.tabs.iter_mut().find(|t| t.id == id) {
                            t.status = format!("Browser unavailable: {error}");
                            t.loading = false;
                            s.dirty = true;
                        }
                        continue;
                    }
                }
            }
            let b = self.browsers.get_mut(&id).unwrap();
            let result = match cmd {
                Command::Navigate(url) => b.navigate(&url),
                Command::Back => b.back(),
                Command::Forward => b.forward(),
                Command::Reload => b.reload(),
                Command::Stop => {
                    b.stop();
                    Ok(())
                }
            };
            if let Err(error) = result {
                if let Some(t) = s.tabs.iter_mut().find(|t| t.id == id) {
                    t.status = error.to_string();
                    t.loading = false;
                    s.dirty = true;
                }
            }
        }
        let rect = w.get::<Rect>(s.viewport).copied().unwrap_or_default();
        let mut prompted = false;
        for (id, b) in &mut self.browsers {
            if visible && *id == s.active && rect.w > 0. && rect.h > 0. {
                if let Ok(bounds) = ClientRect::try_new(rect.x, rect.y, rect.w, rect.h) {
                    if b.set_bounds(bounds).is_ok() {
                        b.show();
                    } else {
                        b.hide();
                    }
                } else {
                    b.hide();
                }
            } else {
                b.hide();
            }
            for event in b.frame() {
                match event {
                    BrowserEvent::Navigation(nav) => {
                        if let Some(t) = s.tabs.iter_mut().find(|t| t.id == *id) {
                            t.url = nav.url;
                            t.title = if nav.title.is_empty() {
                                "Browser".into()
                            } else {
                                nav.title
                            };
                            t.back = nav.can_go_back;
                            t.forward = nav.can_go_forward;
                            t.loading = nav.load == LoadOutcome::Loading;
                            t.status = match nav.load {
                                LoadOutcome::Loading => "Loading…",
                                LoadOutcome::Failed => {
                                    "Page failed to load · check the address and retry"
                                }
                                LoadOutcome::Stopped => "Stopped",
                                _ => "Ready",
                            }
                            .into();
                            if *id == s.active
                                && w.resource::<Focus>().and_then(|f| f.entity) != Some(s.address)
                            {
                                replace_field_text(w, s.address, &t.url);
                            }
                            s.dirty = true;
                        }
                    }
                    BrowserEvent::Download(intent) => {
                        // An inactive/covered page must never open a device file panel.
                        if !visible || *id != s.active || prompted {
                            let _ = b.cancel_download(intent.token);
                            continue;
                        }
                        prompted = true;
                        let name = std::path::Path::new(&intent.suggested_name)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .filter(|n| !n.is_empty())
                            .unwrap_or("download");
                        let chosen = rfd::FileDialog::new()
                            .set_parent(&c.window)
                            .set_title("Save browser download")
                            .set_file_name(name)
                            .save_file();
                        let result = if let Some(path) = chosen {
                            b.accept_download(intent.token, &path)
                        } else {
                            b.cancel_download(intent.token)
                        };
                        if let Err(error) = result {
                            let _ = b.cancel_download(intent.token);
                            if let Some(t) = s.tabs.iter_mut().find(|t| t.id == *id) {
                                t.status = format!("Download not saved: {error}");
                                s.dirty = true;
                            }
                        }
                    }
                    BrowserEvent::FileInput(intent) => {
                        if !visible || *id != s.active || prompted {
                            let _ = b.cancel_file_input(intent.token);
                            continue;
                        }
                        prompted = true;
                        let dialog = rfd::FileDialog::new()
                            .set_parent(&c.window)
                            .set_title("Choose files to share with this website");
                        let paths = if intent.multiple {
                            dialog.pick_files().unwrap_or_default()
                        } else {
                            dialog.pick_file().into_iter().collect()
                        };
                        if let Err(error) = b.provide_file_input(intent.token, &paths) {
                            let _ = b.cancel_file_input(intent.token);
                            if let Some(t) = s.tabs.iter_mut().find(|t| t.id == *id) {
                                t.status = format!("Files not shared: {error}");
                                s.dirty = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let changed = s.dirty;
        w.insert_resource(s);
        if changed {
            sync(w);
            c.request_redraw();
        }
        (!self.browsers.is_empty()).then_some(Duration::from_millis(if visible {
            100
        } else {
            500
        }))
    }
}
#[cfg(not(all(target_os = "macos", feature = "native-browser")))]
impl Host {
    fn browse(&mut self, w: &mut World, _: rndr_render::NativeHostContext<'_>) -> Option<Duration> {
        if let Some(s) = w.resource_mut::<State>() {
            if !s.commands.is_empty() {
                s.commands.clear();
                for t in &mut s.tabs {
                    t.loading = false;
                    t.status = "Native browser unavailable on this build".into();
                }
                s.dirty = true;
            }
        }
        sync(w);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn address_admission() {
        assert_eq!(
            destination(" example.com/path ").unwrap(),
            "https://example.com/path"
        );
        assert_eq!(
            destination("localhost:3000").unwrap(),
            "http://localhost:3000/"
        );
        assert!(
            destination("cats and dogs")
                .unwrap()
                .contains("q=cats%20and%20dogs")
        );
        for raw in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "medousa://command",
            "data:text/html,hi",
            "https://user:pass@example.com",
            "",
            "https://x\n.com",
        ] {
            assert!(destination(raw).is_err(), "{raw}");
        }
    }
    #[test]
    fn commands_are_tab_scoped_and_inactive_chrome_cannot_navigate() {
        let mut app = crate::ui::app();
        app.update();
        let w = app.world_mut();
        crate::surfaces::select(w, crate::surfaces::SurfaceId::Web);
        let address = w.resource::<State>().unwrap().address;
        replace_field_text(w, address, "example.org");
        perform(w, Action::Go);
        perform(w, Action::New);
        let s = w.resource::<State>().unwrap();
        assert_eq!(s.commands.len(), 1);
        assert_eq!(s.commands[0].0, 1);
        assert_ne!(s.active, 1);
        let e = w.spawn((Action::Go,));
        crate::surfaces::select(w, crate::surfaces::SurfaceId::Chat);
        activate(w, e);
        assert_eq!(w.resource::<State>().unwrap().commands.len(), 1);
        perform(w, Action::Close(1));
        assert!(w.resource::<State>().unwrap().commands.is_empty());
        assert!(crate::workshops::binding(w, crate::surfaces::SurfaceId::Web).is_none());
    }
    #[test]
    fn semantic_tab_requests_preserve_identity_and_browser_state() {
        let mut app = crate::ui::app();
        app.update();
        crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Web);
        perform(app.world_mut(), Action::New);
        let root = app.world().resource::<State>().unwrap().tabs_root;
        let strip = children_of(app.world(), root)[0];
        send(
            app.world_mut(),
            TabStripEvent {
                strip,
                kind: TabStripEventKind::Activated(TabId(1)),
            },
        );
        app.update();
        assert_eq!(app.world().resource::<State>().unwrap().active, 1);
        assert_eq!(children_of(app.world(), root)[0], strip);
        send(
            app.world_mut(),
            TabStripEvent {
                strip,
                kind: TabStripEventKind::CloseRequested(TabId(1)),
            },
        );
        app.update();
        let s = app.world().resource::<State>().unwrap();
        assert_eq!(s.active, 2);
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(children_of(app.world(), root)[0], strip);
    }
    #[test]
    fn compact_navigation_has_one_reload_stop_slot_and_disabled_history() {
        let mut app = crate::ui::app();
        app.update();
        let w = app.world_mut();
        let find = |w: &World, key| {
            w.entities_with::<Action>()
                .into_iter()
                .find(|e| match (w.get::<Action>(*e), key) {
                    (Some(Action::Command(Command::Back)), 1)
                    | (Some(Action::Command(Command::Reload)), 2)
                    | (Some(Action::Command(Command::Stop)), 3) => true,
                    _ => false,
                })
                .unwrap()
        };
        let back = find(w, 1);
        let reload = find(w, 2);
        let stop = find(w, 3);
        assert!(!w.get::<rndr_core::Button>(back).unwrap().enabled);
        assert!(!w.has::<Hidden>(reload));
        assert!(w.has::<Hidden>(stop));
        let s = w.resource_mut::<State>().unwrap();
        s.tabs[0].back = true;
        s.tabs[0].loading = true;
        sync(w);
        assert!(w.get::<rndr_core::Button>(back).unwrap().enabled);
        assert!(w.has::<Hidden>(reload));
        assert!(!w.has::<Hidden>(stop));
    }
    #[test]
    fn modals_and_popups_suspend_guest() {
        let mut w = World::new();
        assert!(!occluded(&w));
        let modal = w.spawn((Modal,));
        assert!(occluded(&w));
        w.insert(modal, Hidden);
        assert!(!occluded(&w));
    }
    #[test]
    fn viewport_and_tab_strip_are_bounded_at_small_sizes() {
        let mut app = crate::ui::app();
        app.world_mut().insert_resource(Viewport::new(760., 540.));
        app.update();
        crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Web);
        for _ in 0..MAX_TABS + 3 {
            perform(app.world_mut(), Action::New);
        }
        for _ in 0..5 {
            app.update();
        }
        let w = app.world();
        let s = w.resource::<State>().unwrap();
        assert_eq!(s.tabs.len(), MAX_TABS);
        let viewport = w.get::<Rect>(s.viewport).unwrap();
        let strip = w.get::<Rect>(s.tabs_root).unwrap();
        assert!(viewport.w > 300. && viewport.h > 200., "{viewport:?}");
        assert!(viewport.x >= crate::surfaces::RAIL_WIDTH);
        assert!(viewport.x + viewport.w <= 761.);
        assert!(strip.x + strip.w <= 761.);
        assert!(w.has::<Clip>(s.tabs_root));
    }
    #[test]
    #[ignore = "GPU browser chrome visual acceptance"]
    fn chrome_visual_acceptance() {
        use std::sync::Arc;
        let mut app = crate::ui::app();
        app.world_mut()
            .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
        let mut exporter = rndr_render::Exporter::new().unwrap();
        std::fs::create_dir_all("verification").unwrap();
        app.update();
        crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Web);
        perform(app.world_mut(), Action::New);
        for (width, height, dark, name) in [
            (1200, 850, true, "wide"),
            (760, 540, true, "narrow"),
            (1200, 850, false, "light"),
        ] {
            publish_system_appearance(
                app.world_mut(),
                SystemAppearance {
                    dark: Some(dark),
                    ..Default::default()
                },
            );
            app.world_mut()
                .insert_resource(Viewport::new(width as f32, height as f32));
            for _ in 0..8 {
                app.update();
            }
            let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, width, height, 1.)
                    .unwrap()
            });
            image::save_buffer(
                format!("verification/native-browser-{name}.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    #[test]
    fn retained_tabs_and_close_last() {
        let mut app = crate::ui::app();
        app.update();
        let w = app.world_mut();
        crate::surfaces::select(w, crate::surfaces::SurfaceId::Web);
        perform(w, Action::New);
        let second = w.resource::<State>().unwrap().active;
        assert_eq!(w.resource::<State>().unwrap().tabs.len(), 2);
        perform(w, Action::Select(1));
        perform(w, Action::Close(1));
        assert_eq!(w.resource::<State>().unwrap().active, second);
        perform(w, Action::Close(second));
        assert_eq!(w.resource::<State>().unwrap().tabs.len(), 1);
        let id = w.resource::<State>().unwrap().active;
        crate::surfaces::select(w, crate::surfaces::SurfaceId::Chat);
        crate::surfaces::select(w, crate::surfaces::SurfaceId::Web);
        assert_eq!(w.resource::<State>().unwrap().active, id);
    }
}
