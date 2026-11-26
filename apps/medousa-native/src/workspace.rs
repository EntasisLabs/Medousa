//! Window-local layout. Never stored in the workshop daemon or chat state.
use rndr_core::*;
use rndr_ui::{
    Surface, UiId,
    split::{PaneFill, SplitSash, SplitSashChange, SplitSashHandle, SplitSashProps},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

/// A destination's sidebar: shown or collapsed, and its width in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Side {
    pub open: bool,
    pub width: f32,
}
impl Default for Side {
    fn default() -> Self {
        Self {
            open: true,
            width: SIDE_DEFAULT,
        }
    }
}
pub const SIDE_DEFAULT: f32 = 288.;
pub const SIDE_MIN: f32 = 200.;
pub const SIDE_MAX: f32 = 480.;
/// Sidebars of the non-chat destinations, by surface name.
pub const SIDE_NAMES: [&str; 8] = [
    "notes",
    "code",
    "create",
    "reviews",
    "calendar",
    "peers",
    "settings",
    "artifacts",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preferences {
    pub sidebar_ratio: f64,
    pub chat_ratio: f64,
    pub sidebar_open: bool,
    pub details_open: bool,
    /// Non-chat destinations, in [`SIDE_NAMES`] order.
    pub sides: [Side; 8],
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            sidebar_ratio: 0.20,
            chat_ratio: 0.68,
            sidebar_open: true,
            details_open: false,
            sides: [Side::default(); 8],
        }
    }
}
impl Preferences {
    fn read(path: &Path) -> Self {
        let value = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        let mut p = Self::default();
        if let Some(v) = value.filter(|v| v["version"] == 1) {
            p.sidebar_ratio = v["sidebar_ratio"]
                .as_f64()
                .filter(|n| n.is_finite() && (0.05..=0.5).contains(n))
                .unwrap_or(p.sidebar_ratio);
            p.chat_ratio = v["chat_ratio"]
                .as_f64()
                .filter(|n| n.is_finite() && (0.2..=0.9).contains(n))
                .unwrap_or(p.chat_ratio);
            p.sidebar_open = v["sidebar_open"].as_bool().unwrap_or(p.sidebar_open);
            p.details_open = v["details_open"].as_bool().unwrap_or(p.details_open);
            for (i, name) in SIDE_NAMES.iter().enumerate() {
                let side = &v["sidebars"][name];
                if let Some(open) = side["open"].as_bool() {
                    p.sides[i].open = open;
                }
                if let Some(width) = side["width"]
                    .as_f64()
                    .filter(|w| w.is_finite() && (SIDE_MIN as f64..=SIDE_MAX as f64).contains(w))
                {
                    p.sides[i].width = width as f32;
                }
            }
        }
        p
    }
    fn write(self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let sidebars: serde_json::Map<String, serde_json::Value> = SIDE_NAMES
            .iter()
            .zip(self.sides)
            .map(|(name, side)| {
                (
                    (*name).to_owned(),
                    serde_json::json!({"open": side.open, "width": side.width}),
                )
            })
            .collect();
        let data = serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1, "sidebar_ratio": self.sidebar_ratio, "chat_ratio": self.chat_ratio,
            "sidebar_open": self.sidebar_open, "details_open": self.details_open,
            "sidebars": sidebars,
        }))?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temporary, data)?;
        std::fs::rename(temporary, path)
    }
}

pub struct Workspace {
    pub prefs: Preferences,
    outer: Option<SplitSashHandle>,
    inner: Option<SplitSashHandle>,
    geometry: (u32, bool, bool),
    settling: u8,
    last_sent: Preferences,
    save: Option<mpsc::Sender<Preferences>>,
    errors: Option<std::sync::Mutex<mpsc::Receiver<String>>>,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            prefs: Preferences::default(),
            outer: None,
            inner: None,
            geometry: (0, false, false),
            settling: 3,
            last_sent: Preferences::default(),
            save: None,
            errors: None,
        }
    }
}
impl Workspace {
    /// Read once before the render loop; debounce all writes off the UI thread.
    pub fn local(path: PathBuf) -> Self {
        let prefs = Preferences::read(&path);
        let (tx, rx) = mpsc::channel::<Preferences>();
        let (errors, error_rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(mut pending) = rx.recv() {
                loop {
                    match rx.recv_timeout(Duration::from_millis(150)) {
                        Ok(newer) => pending = newer,
                        Err(
                            mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected,
                        ) => break,
                    }
                }
                if let Err(error) = pending.write(&path) {
                    let _ = errors.send(format!("Could not save window layout: {error}"));
                }
            }
        });
        Self {
            prefs,
            last_sent: prefs,
            save: Some(tx),
            errors: Some(std::sync::Mutex::new(error_rx)),
            ..Default::default()
        }
    }
}

pub fn mount(world: &mut World, root: Entity) -> (SplitSashHandle, SplitSashHandle) {
    if world.resource::<Workspace>().is_none() {
        world.insert_resource(Workspace::default());
    }
    let prefs = world.resource::<Workspace>().unwrap().prefs;
    let outer = SplitSash::mount(
        world,
        root,
        UiId(0x5753_0001),
        sidebar_props(1200., true),
        prefs.sidebar_ratio,
    )
    .unwrap();
    let inner = SplitSash::mount(
        world,
        outer.trailing,
        UiId(0x5753_0002),
        details_props(false, 1200.),
        1.,
    )
    .unwrap();
    let state = world.resource_mut::<Workspace>().unwrap();
    state.outer = Some(outer);
    state.inner = Some(inner);
    (outer, inner)
}
fn sidebar_props(width: f32, visible: bool) -> SplitSashProps {
    let mut props = SplitSashProps::new("Conversation sidebar width");
    props.min_leading = if visible { 200. } else { 0. };
    // Keep the conversation central even at the sidebar's largest size.
    props.min_trailing = (width - 368.).max(440.);
    props.step = 0.015;
    props.page = 0.05;
    props.default_ratio = 0.20;
    props.enabled = visible;
    // The pane keeps the sidebar's own surface while its contents ease in.
    props.leading_fill = PaneFill::Theme(Surface::Navigation);
    props
}
fn details_props(visible: bool, width: f32) -> SplitSashProps {
    let mut props = SplitSashProps::new("Conversation details width");
    // Reserve the dock's usable minimum after the fixed app rail. At the
    // smallest supported window the chat yields 22px, not the dock controls.
    props.min_leading = (width - 286.).clamp(360., 440.);
    props.min_trailing = if visible { 280. } else { 0. };
    props.step = 0.025;
    props.default_ratio = 0.68;
    props.enabled = visible;
    props.trailing_fill = PaneFill::Theme(Surface::Navigation);
    props
}
fn show(world: &mut World, e: Entity, visible: bool) {
    if visible {
        world.remove::<Hidden>(e);
    } else {
        world.insert(e, Hidden);
    }
}
/// Show a side pane, easing its contents in from the pane's own edge when it
/// reappears. The pane surface stays solid, layout is final at once so the
/// reading lane never reflows per frame, and hiding stays immediate.
pub(crate) fn reveal(world: &mut World, e: Entity, visible: bool, from_x: f32) {
    let appearing = visible && world.has::<Hidden>(e);
    show(world, e, visible);
    // Never animate a pane's first appearance at launch.
    let laid_out = world.get::<Rect>(e).is_some_and(|r| r.w > 0.);
    if appearing && laid_out && world.resource::<Time>().is_some_and(|t| !t.delta.is_zero()) {
        for content in children_of(world, e) {
            motion::enter(
                world,
                content,
                motion::Transition::slide(from_x, 0.)
                    .with_anim(motion::Curve::tween(0.18, motion::Easing::EaseOut).into()),
            );
        }
    }
}
/// Return the effective chat width. Responsive collapse does not overwrite the
/// user's sidebar preference or saved ratios, and no content is remounted.
pub fn sync(world: &mut World, sidebar_open: bool, details_open: bool) -> f32 {
    let width = crate::surfaces::chat_width(world);
    let sidebar = sidebar_open && (!details_open || width >= 1100.);
    let changes = events::<SplitSashChange>(world).to_vec();
    let state = world.resource_mut::<Workspace>().unwrap();
    let outer = state.outer.unwrap();
    let inner = state.inner.unwrap();
    let geometry = (width.round() as u32, sidebar, details_open);
    if geometry != state.geometry {
        state.settling = 3;
        state.geometry = geometry;
    }
    let reconfigure = state.settling > 0;
    if state.settling == 0 {
        for change in changes {
            if sidebar && change.sash == outer.sash {
                state.prefs.sidebar_ratio = change.ratio;
            }
            if details_open && change.sash == inner.sash {
                state.prefs.chat_ratio = change.ratio;
            }
        }
    } else {
        state.settling -= 1;
    }
    state.prefs.sidebar_open = sidebar_open;
    state.prefs.details_open = details_open;
    if state.prefs != state.last_sent {
        if let Some(tx) = &state.save {
            let _ = tx.send(state.prefs);
        }
        state.last_sent = state.prefs;
    }
    let prefs = state.prefs;
    let available = width
        - if sidebar {
            ((width - 6.) * prefs.sidebar_ratio as f32).clamp(200., 360.) + 6.
        } else {
            0.
        };
    // Nested split and reading-column measurements use the preceding Rect.
    // Pump a bounded convergence window; idle must not need a pointer wiggle.
    let needs_frame = state.settling > 0 || !events::<SplitSashChange>(world).is_empty();
    if needs_frame {
        frame_wake(world).wake();
    }
    // Do not force the preceding ratio during pointer motion: Input already
    // applied its delta. Only reconfigure after visibility/viewport changes.
    if reconfigure {
        SplitSash::mount(
            world,
            world.get::<Parent>(outer.root).unwrap().entity,
            UiId(0x5753_0001),
            sidebar_props(width, sidebar),
            if sidebar { prefs.sidebar_ratio } else { 0. },
        )
        .unwrap();
        SplitSash::mount(
            world,
            outer.trailing,
            UiId(0x5753_0002),
            details_props(details_open, available),
            if details_open { prefs.chat_ratio } else { 1. },
        )
        .unwrap();
    }
    reveal(world, outer.leading, sidebar, -12.);
    show(world, outer.sash, sidebar);
    reveal(world, inner.trailing, details_open, 12.);
    show(world, inner.sash, details_open);
    // Thin, low-contrast divider; RNDR retains its focus/drag color and cursor.
    for split in [outer, inner] {
        world.get_mut::<Layout>(split.sash).unwrap().width = Size::Px(6.);
    }
    if details_open {
        let min_chat = (available - 286.).clamp(360., 440.);
        ((available - 6.) * prefs.chat_ratio as f32)
            .clamp(min_chat, (available - 286.).max(min_chat))
    } else {
        available
    }
}
/// App-owned quiet chrome. Keep RNDR's active focus/drag treatment intact.
pub fn chrome(world: &mut World) {
    let Some(state) = world.resource::<Workspace>() else {
        return;
    };
    let handles = [state.outer, state.inner];
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let drag = world.resource::<Drag>().and_then(|d| d.entity);
    for handle in handles.into_iter().flatten() {
        if focus != Some(handle.sash) && drag != Some(handle.sash) {
            world.insert(
                handle.sash,
                crate::native_style::color(world, crate::native_style::navigation()),
            );
        }
    }
}
/// Queue a write when a destination sidebar changed outside the chat split.
pub fn save(world: &mut World) {
    let Some(state) = world.resource_mut::<Workspace>() else {
        return;
    };
    if state.prefs != state.last_sent {
        if let Some(tx) = &state.save {
            let _ = tx.send(state.prefs);
        }
        state.last_sent = state.prefs;
    }
}
pub fn error(world: &World) -> Option<String> {
    world
        .resource::<Workspace>()?
        .errors
        .as_ref()?
        .lock()
        .ok()?
        .try_recv()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_roundtrip_and_invalid_values_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("native/layout.json");
        let p = Preferences {
            sidebar_ratio: 0.26,
            chat_ratio: 0.61,
            sidebar_open: false,
            details_open: true,
            sides: {
                let mut sides = [Side::default(); 8];
                sides[0] = Side {
                    open: false,
                    width: 340.,
                };
                sides[6].width = 220.;
                sides
            },
        };
        p.write(&path).unwrap();
        assert_eq!(Preferences::read(&path), p);
        std::fs::write(&path, r#"{"version":1,"sidebar_ratio":-1,"chat_ratio":4}"#).unwrap();
        assert_eq!(Preferences::read(&path), Preferences::default());
        std::fs::write(&path, "broken").unwrap();
        assert_eq!(Preferences::read(&path), Preferences::default());
    }
}
