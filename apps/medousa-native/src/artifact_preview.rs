//! Host-owned isolated HTML. Credentials and transport never enter the guest.
//! Fetches are bounded off-frame; only the native UI-thread host owns WebKit.
use medousa_sdk::MedousaClient;
use rndr_core::*;

const MAX_HTML: usize = 2 * 1024 * 1024;
// JSON can escape each source byte as six bytes, plus bounded metadata.
const MAX_RESPONSE: usize = MAX_HTML * 6 + 64 * 1024;
#[derive(Clone)]
struct Source {
    session: String,
    id: String,
    label: String,
}
impl Component for Source {}
pub struct Request {
    pub session: String,
    id: String,
    nonce: u64,
}
pub struct Update {
    request: Request,
    result: Result<String, String>,
}
struct Viewer {
    source: Source,
    nonce: u64,
    root: Entity,
    anchor: Entity,
    status: Entity,
    inline: bool,
    focus_session: Option<String>,
    surface: crate::surfaces::SurfaceId,
    binding: Option<crate::routing::Binding>,
    html: Option<String>,
}
#[derive(Default)]
struct State {
    next: u64,
    viewer: Option<Viewer>,
    commands: Vec<Request>,
}
pub fn is_html(mime: &str) -> bool {
    mime.split(';')
        .next()
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("text/html"))
}
pub fn mount(world: &mut World, parent: Entity, session: &str, id: &str, label: &str) -> Entity {
    use rndr_ui::{
        UiId,
        controls::{ButtonProps, HostIntent},
    };
    use std::hash::{Hash, Hasher};
    let mut key = std::collections::hash_map::DefaultHasher::new();
    ("html-artifact", session, id).hash(&mut key);
    let root = rndr_ui::controls::Button::mount(
        world,
        parent,
        UiId(key.finish()),
        ButtonProps::new(
            format!("Open HTML artifact · {label}"),
            HostIntent::Command(0x48544d4c),
        ),
    )
    .expect("artifact control")
    .root;
    world.insert(
        root,
        Source {
            session: session.into(),
            id: id.into(),
            label: label.into(),
        },
    );
    world.insert(root, ActivationHandler(open));
    root
}
fn focus_session(world: &World) -> Option<String> {
    world
        .resource::<crate::ui::Model>()
        .and_then(|m| m.chat.session().map(str::to_owned))
}
fn supported() -> bool {
    cfg!(all(
        target_os = "macos",
        feature = "macos-private-webkit-artifacts"
    ))
}
fn open(world: &mut World, anchor: Entity) {
    let Some(source) = world.get::<Source>(anchor).cloned() else {
        return;
    };
    close(world);
    if !world.has_resource::<State>() {
        world.insert_resource(State::default());
    }
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Absolute::fill(0.),
        Layer(190),
        Modal,
        Focusable,
    ));
    crate::native_style::plane(world, root, crate::native_style::canvas());
    let bar = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(64.),
        padding: 12.,
        gap: 8.,
        ..Layout::row()
    },));
    set_parent(world, bar, root);
    if cfg!(target_os = "macos") {
        // The full-window chat viewer shares the native titlebar traffic lights.
        let inset = world.spawn((Layout {
            width: Size::Px(72.),
            height: Size::Px(1.),
            ..Layout::row()
        },));
        set_parent(world, inset, bar);
    }
    let caption = rndr_ui::text(world, bar, &source.label, rndr_ui::Typography::Emphasis).unwrap();
    world.get_mut::<Layout>(caption).unwrap().width = Size::Fill;
    world.get_mut::<Label>(caption).unwrap().overflow = TextOverflow::Ellipsis;
    let button = rndr_ui::controls::Button::mount(
        world,
        bar,
        rndr_ui::UiId(0x48544d43),
        rndr_ui::controls::ButtonProps::new(
            "Close preview",
            rndr_ui::controls::HostIntent::Command(0x48544d43),
        ),
    )
    .unwrap()
    .root;
    world.get_mut::<Layout>(button).unwrap().width = Size::Px(160.);
    world.insert(button, ActivationHandler(|w, _| close(w)));
    let content = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        padding: 16.,
        ..Layout::column()
    },));
    set_parent(world, content, root);
    let status = rndr_ui::text(
        world,
        content,
        if supported() {
            "Loading isolated preview…"
        } else {
            "Preview unavailable · isolated HTML is not enabled on this build"
        },
        rndr_ui::Typography::Secondary,
    )
    .unwrap();
    let focus_session = focus_session(world);
    let surface = crate::surfaces::active(world);
    let binding = crate::workshops::binding(world, surface);
    let state = world.resource_mut::<State>().unwrap();
    state.next += 1;
    let nonce = state.next;
    if supported() {
        state.commands.push(Request {
            session: source.session.clone(),
            id: source.id.clone(),
            nonce,
        });
    }
    state.viewer = Some(Viewer {
        source,
        nonce,
        root,
        anchor,
        status,
        focus_session,
        surface,
        binding,
        inline: false,
        html: None,
    });
    let _ = request_focus(world, button);
}
/// Keep the guest alive while the library rebuilds metadata/actions around it.
pub fn park_inline(world: &mut World, parking: Entity) {
    if let Some(root) = world
        .resource::<State>()
        .and_then(|s| s.viewer.as_ref())
        .filter(|v| v.inline)
        .map(|v| v.root)
    {
        set_parent(world, root, parking);
    }
}

/// Inline document viewport. Reparenting the same source preserves DOM and scroll.
pub fn inline(world: &mut World, parent: Entity, session: &str, id: &str, label: &str) {
    let binding = crate::workshops::binding(world, crate::surfaces::SurfaceId::Artifacts);
    let existing = world
        .resource::<State>()
        .and_then(|s| s.viewer.as_ref())
        .filter(|v| {
            v.inline
                && v.source.session == session
                && v.source.id == id
                && v.binding == binding
                && world.is_alive(v.root)
        })
        .map(|v| v.root);
    if let Some(root) = existing {
        set_parent(world, root, parent);
        world
            .resource_mut::<State>()
            .unwrap()
            .viewer
            .as_mut()
            .unwrap()
            .anchor = parent;
        return;
    }
    close(world);
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        min_width: 0.,
        min_height: 0.,
        ..Layout::column()
    },));
    set_parent(world, root, parent);
    crate::native_style::plane(world, root, crate::native_style::canvas());
    let status = rndr_ui::text(world, root, if supported() {
        "Loading artifact…"
    } else {
        "Live preview isn’t available in this build. Open the source chat or export HTML to view a copy."
    }, rndr_ui::Typography::Secondary).unwrap();
    world.get_mut::<Layout>(status).unwrap().width = Size::Fill;
    world.get_mut::<Label>(status).unwrap().overflow = TextOverflow::Wrap;
    if !world.has_resource::<State>() {
        world.insert_resource(State::default());
    }
    let focus_session = focus_session(world);
    let state = world.resource_mut::<State>().unwrap();
    state.next += 1;
    let nonce = state.next;
    if supported() {
        state.commands.push(Request {
            session: session.into(),
            id: id.into(),
            nonce,
        });
    }
    state.viewer = Some(Viewer {
        source: Source {
            session: session.into(),
            id: id.into(),
            label: label.into(),
        },
        nonce,
        root,
        anchor: parent,
        status,
        focus_session,
        surface: crate::surfaces::SurfaceId::Artifacts,
        binding,
        inline: true,
        html: None,
    });
}
#[cfg(test)]
pub fn inline_snapshot(world: &World) -> Option<(Entity, u64)> {
    world
        .resource::<State>()
        .and_then(|s| s.viewer.as_ref())
        .filter(|v| v.inline)
        .map(|v| (v.root, v.nonce))
}
pub fn close_inline(world: &mut World) {
    if world
        .resource::<State>()
        .and_then(|s| s.viewer.as_ref())
        .is_some_and(|v| v.inline)
    {
        close(world);
    }
}

fn close(world: &mut World) {
    let Some(v) = world.resource_mut::<State>().and_then(|s| {
        s.commands.clear();
        s.viewer.take()
    }) else {
        return;
    };
    if world.is_alive(v.root) {
        despawn(world, v.root);
    }
    if !v.inline && world.is_alive(v.anchor) {
        let _ = request_focus(world, v.anchor);
    }
}
pub fn tick(world: &mut World) {
    let Some(v) = world.resource::<State>().and_then(|s| s.viewer.as_ref()) else {
        return;
    };
    let escape = world.resource::<Input>().is_some_and(|i| {
        i.key == Some(Key::Escape)
            || i.keys
                .iter()
                .any(|k| !k.released && k.logical == LogicalKey::Named(NamedKey::Escape))
    });
    if !world.is_alive(v.anchor)
        || !world.is_alive(v.root)
        || (v.surface == crate::surfaces::SurfaceId::Chat
            && focus_session(world) != v.focus_session)
        || crate::surfaces::active(world) != v.surface
        || crate::workshops::binding(world, v.surface) != v.binding
        || (!v.inline
            && world
                .entities_with::<Modal>()
                .iter()
                .any(|e| world.get::<Layer>(*e).is_some_and(|layer| layer.0 > 190)))
        || (escape && !v.inline)
    {
        close(world);
        if escape && let Some(input) = world.resource_mut::<Input>() {
            input.key = None;
            input
                .keys
                .retain(|k| k.logical != LogicalKey::Named(NamedKey::Escape));
        }
    }
}
pub fn drain_commands(world: &mut World) -> Vec<Request> {
    world
        .resource_mut::<State>()
        .map(|s| std::mem::take(&mut s.commands))
        .unwrap_or_default()
}
fn admitted(request: &Request, value: serde_json::Value) -> Result<String, String> {
    let response: medousa_types::ArtifactFetchResponse =
        serde_json::from_value(value).map_err(|_| "Invalid artifact response")?;
    if response.artifact_id != request.id || !is_html(&response.mime) {
        return Err("Artifact identity or content type changed".into());
    }
    if response.body.len() > MAX_HTML || response.byte_size > MAX_HTML {
        return Err("HTML artifact exceeds 2 MiB preview limit".into());
    }
    Ok(response.body)
}
/// Shared bounded transport for previews and device-side HTML export.
pub async fn fetch_html(client: &MedousaClient, session: &str, id: &str) -> Result<String, String> {
    let request = Request {
        session: session.into(),
        id: id.into(),
        nonce: 0,
    };
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let value = client.transport().post_json_bounded(client.base_url(), "/v1/runtime/artifact/fetch", serde_json::json!({"session_id": crate::routing::wire_owned(session), "artifact_id": id}), MAX_RESPONSE).await.map_err(|_| "Could not load HTML artifact".to_owned())?;
        admitted(&request, value)
    }).await.unwrap_or_else(|_| Err("HTML artifact request timed out".into()))
}
pub async fn execute(client: &MedousaClient, request: Request) -> Update {
    let result = fetch_html(client, &request.session, &request.id).await;
    Update { request, result }
}

/// The daemon historically saved non-scrolling embed chrome into HTML. Like
/// Tauri's prepareArtifactHtml, discard only that identified host stylesheet.
/// Never override authored overflow/layout, mutate the saved original, or change
/// the bytes returned by fetch_html (also used for exact-byte HTML export).
fn strip_legacy_host_style(html: &str) -> String {
    static LEGACY: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    LEGACY
        .get_or_init(|| {
            regex::Regex::new(
        r#"(?is)<style\b[^>]*\s+id\s*=\s*["']medousa-artifact-host["'][^>]*>.*?</style\s*>"#
    ).expect("legacy artifact style pattern")
        })
        .replace_all(html, "")
        .into_owned()
}

fn prepare_html(html: &str) -> String {
    let mut html = strip_legacy_host_style(html);
    // Quiet browser chrome, not an authored layout override. Low-specificity
    // selectors yield to artifact styles and never force overflow on guest apps.
    const SCROLL_STYLE: &str = concat!(
        "<style id=\"medousa-native-scrollbars\">",
        ":where(html,body){scrollbar-width:thin;scrollbar-color:#8888 transparent}",
        ":where(html,body)::-webkit-scrollbar{width:8px;height:8px}",
        ":where(html,body)::-webkit-scrollbar-track{background:transparent}",
        ":where(html,body)::-webkit-scrollbar-thumb{background:#8888;border-radius:4px}",
        "</style>"
    );
    if html.len() + SCROLL_STYLE.len() <= MAX_HTML
        && !html.contains("id=\"medousa-native-scrollbars\"")
    {
        let lower = html.to_ascii_lowercase();
        let at = lower
            .find("</head>")
            .or_else(|| lower.find("<body"))
            .unwrap_or_else(|| {
                lower
                    .find('>')
                    .filter(|_| lower.starts_with("<!doctype"))
                    .map_or(0, |i| i + 1)
            });
        html.insert_str(at, SCROLL_STYLE);
    }
    html
}

pub fn apply(world: &mut World, update: Update) {
    let Some(v) = world
        .resource_mut::<State>()
        .and_then(|s| s.viewer.as_mut())
        .filter(|v| {
            v.nonce == update.request.nonce
                && v.source.session == update.request.session
                && v.source.id == update.request.id
        })
    else {
        return;
    };
    let status = v.status;
    let message = match update.result {
        Ok(html) => {
            v.html = Some(prepare_html(&html));
            "Preparing isolated preview…".into()
        }
        Err(error) => format!("Preview unavailable · {error}"),
    };
    if let Some(label) = world.get_mut::<Label>(status) {
        label.text = message;
    }
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "macos-private-webkit-artifacts")
))]
/// Native guest views sit above RNDR's painted UI. Layer trusted popup pixels
/// above the guest with transparency; never hide the whole
/// document just because a tooltip opened.
/// Modal surfaces still suspend the guest to preserve input/focus isolation.
fn inline_occluded(world: &World) -> bool {
    world
        .entities_with::<Modal>()
        .iter()
        .any(|e| !world.has::<Hidden>(*e))
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "macos-private-webkit-artifacts")
))]
fn popup_regions(world: &World, bounds: Rect) -> Vec<(Entity, Rect)> {
    layered_roots(world)
        .into_iter()
        .filter(|e| world.has::<AnchoredPopup>(*e))
        .filter_map(|e| {
            if world.has::<Hidden>(e) {
                return None;
            }
            let rect = *world.get::<Rect>(e)?;
            if rect.w <= 0. || rect.h <= 0. {
                return None;
            }
            let mut paint = rect;
            if let Some(Shadows(shadows)) = world.get::<Shadows>(e) {
                for shadow in shadows.iter().filter(|s| !s.inset && s.color.a > 0.) {
                    // Match rndr_paint's outer-shadow support: 3 sigma + 1px.
                    let pad = shadow.spread + 1.5 * shadow.blur.max(0.) + 1.;
                    let left = rect.x + shadow.offset[0] - pad;
                    let top = rect.y + shadow.offset[1] - pad;
                    let right = rect.x + rect.w + shadow.offset[0] + pad;
                    let bottom = rect.y + rect.h + shadow.offset[1] + pad;
                    let x = paint.x.min(left);
                    let y = paint.y.min(top);
                    paint = Rect {
                        x,
                        y,
                        w: (paint.x + paint.w).max(right) - x,
                        h: (paint.y + paint.h).max(bottom) - y,
                    };
                }
            }
            paint = transformed_rect(world, e, paint);
            intersect(paint, bounds).map(|rect| (e, rect))
        })
        .collect()
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "macos-private-webkit-artifacts")
))]
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    (x.is_finite()
        && y.is_finite()
        && right.is_finite()
        && bottom.is_finite()
        && right > x
        && bottom > y)
        .then_some(Rect {
            x,
            y,
            w: right - x,
            h: bottom - y,
        })
}
#[cfg(any(
    test,
    all(target_os = "macos", feature = "macos-private-webkit-artifacts")
))]
fn transformed_rect(world: &World, e: Entity, rect: Rect) -> Rect {
    let Some(transform) = world.get::<Transform>(e) else {
        return rect;
    };
    let origin = world.get::<Rect>(e).copied().unwrap_or(rect);
    let [a, b, c, d, tx, ty] = transform.matrix(origin);
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for [x, y] in [
        [rect.x, rect.y],
        [rect.x + rect.w, rect.y],
        [rect.x, rect.y + rect.h],
        [rect.x + rect.w, rect.y + rect.h],
    ] {
        let p = [a * x + c * y + tx, b * x + d * y + ty];
        for i in 0..2 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    Rect {
        x: min[0],
        y: min[1],
        w: max[0] - min[0],
        h: max[1] - min[1],
    }
}
#[cfg(test)]
fn inline_occlusions(world: &World, bounds: Rect) -> Vec<Rect> {
    popup_regions(world, bounds)
        .into_iter()
        .map(|(_, rect)| rect)
        .collect()
}

#[cfg(all(target_os = "macos", feature = "macos-private-webkit-artifacts"))]
#[path = "artifact_overlays.rs"]
mod overlays;

/// Never stored in ECS: platform views must stay on the event-loop thread.
#[cfg(all(target_os = "macos", feature = "macos-private-webkit-artifacts"))]
#[derive(Default)]
pub struct Host {
    nonce: Option<u64>,
    preview: Option<rndr_browser::ArtifactPreview>,
    overlays: overlays::Cache,
}
#[cfg(all(target_os = "macos", feature = "macos-private-webkit-artifacts"))]
impl rndr_render::NativeHost for Host {
    fn frame(
        &mut self,
        world: &mut World,
        context: rndr_render::NativeHostContext<'_>,
    ) -> Option<std::time::Duration> {
        let had_viewer = world
            .resource::<State>()
            .is_some_and(|s| s.viewer.is_some());
        tick(world);
        let Some(v) = world.resource::<State>().and_then(|s| s.viewer.as_ref()) else {
            self.preview = None;
            self.nonce = None;
            self.overlays.clear();
            if had_viewer {
                context.request_redraw();
            }
            return None;
        };
        let nonce = v.nonce;
        let status = v.status;
        let bounds = if v.inline {
            world.get::<Rect>(v.root).copied()
        } else {
            None
        };
        let inline = v.inline;
        let occluded = inline && inline_occluded(world);
        if self.nonce != Some(nonce) {
            self.preview = None; // retire the previous JS/DOM before replacement
            self.nonce = None;
            self.overlays.clear();
        }
        let html = v.html.as_deref()?;
        let result = (|| {
            if self.nonce != Some(nonce) {
                self.nonce = Some(nonce);
                self.preview = Some(rndr_browser::ArtifactPreview::attach(
                    &context.window,
                    html,
                )?);
            }
            let Some(preview) = self.preview.as_mut() else {
                return Ok(None);
            };
            let width = context.physical_size[0] as f32 / context.scale_factor as f32;
            let height = context.physical_size[1] as f32 / context.scale_factor as f32;
            let rect = if inline {
                bounds.unwrap_or_default()
            } else {
                Rect {
                    x: 0.,
                    y: 64.,
                    w: width,
                    h: (height - 64.).max(0.),
                }
            };
            if rect.w <= 0. || rect.h <= 0. || !context.visible || occluded {
                preview.hide();
                return preview.frame().map(Some);
            }
            preview.set_bounds(
                rndr_browser::ClientRect::try_new(rect.x, rect.y, rect.w, rect.h)
                    .map_err(rndr_browser::ArtifactError::Rect)?,
            )?;
            let overlays = self
                .overlays
                .sync(
                    world,
                    rect,
                    context.scale_factor as f32,
                    context.physical_size,
                )
                .map_err(|_| rndr_browser::ArtifactError::Overlay("popup rasterization failed"))?;
            preview.set_overlays(overlays)?;
            if context.visible {
                preview.show();
            } else {
                preview.hide();
            }
            preview.frame().map(Some)
        })();
        let (message, poll) = match result {
            Ok(Some(rndr_browser::ArtifactState::Ready)) => {
                ("".to_owned(), Some(std::time::Duration::from_millis(250)))
            }
            Ok(Some(_)) => (
                "Preparing isolated preview…".into(),
                Some(std::time::Duration::from_millis(16)),
            ),
            Ok(None) => return None,
            Err(error) => {
                self.preview = None;
                (format!("Preview unavailable · {error}"), None)
            }
        };
        if world
            .get::<Label>(status)
            .is_some_and(|label| label.text != message)
        {
            world.get_mut::<Label>(status).unwrap().text = message;
            context.request_redraw();
        }
        poll
    }
    fn detach(&mut self, world: &mut World) {
        self.preview = None;
        self.nonce = None;
        self.overlays.clear();
        close(world);
    }
}

#[cfg(not(all(target_os = "macos", feature = "macos-private-webkit-artifacts")))]
#[derive(Default)]
pub struct Host;
#[cfg(not(all(target_os = "macos", feature = "macos-private-webkit-artifacts")))]
impl rndr_render::NativeHost for Host {
    fn frame(
        &mut self,
        world: &mut World,
        _: rndr_render::NativeHostContext<'_>,
    ) -> Option<std::time::Duration> {
        tick(world);
        None
    }
    fn detach(&mut self, world: &mut World) {
        close(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (World, Entity) {
        let mut app = crate::ui::app();
        app.update();
        let world = app.world_mut();
        let parent = world.spawn((Layout::column(),));
        let anchor = mount(world, parent, "session", "artifact", "demo");
        (std::mem::take(world), anchor)
    }
    fn request(nonce: u64) -> Request {
        Request {
            session: "session".into(),
            id: "artifact".into(),
            nonce,
        }
    }
    #[test]
    fn legacy_scroll_lock_is_removed_without_touching_authored_css_or_markup() {
        let authored =
            "<style>body{background:#101413;overflow:hidden}.panel{overflow:auto}</style>";
        for tag in [
            "<style id=\"medousa-artifact-host\">html,body{overflow:hidden;scrollbar-width:none}</style>",
            "<STYLE media='all' ID = 'medousa-artifact-host'>\nhtml,body{overflow:hidden}\n</STYLE >",
        ] {
            let raw = format!("<!doctype html><head>{authored}{tag}{tag}</head><body>é</body>");
            let prepared = strip_legacy_host_style(&raw);
            assert_eq!(
                prepared,
                format!("<!doctype html><head>{authored}</head><body>é</body>")
            );
            assert_eq!(strip_legacy_host_style(&prepared), prepared);
        }
        let unrelated = "<style id='custom'>body{overflow:hidden}</style><div id='medousa-artifact-host'>keep</div>";
        assert_eq!(strip_legacy_host_style(unrelated), unrelated);
    }
    #[test]
    fn native_scroll_chrome_is_idempotent_and_does_not_force_layout() {
        let max_html = "x".repeat(MAX_HTML);
        assert_eq!(prepare_html(&max_html), max_html);
        for raw in [
            "<!doctype html><html><head></head><body>hi</body></html>",
            "<body>hi</body>",
            "<p>fragment</p>",
            "<!DOCTYPE HTML><p>fragment</p>",
        ] {
            let html = prepare_html(raw);
            assert_eq!(prepare_html(&html), html);
            assert!(html.contains("scrollbar-width:thin"));
            assert!(!html.contains("overflow:"));
            assert!(!html.contains("!important"));
            if raw.starts_with("<!") {
                assert!(html.starts_with("<!"));
            }
        }
    }
    #[test]
    fn inline_guest_clips_overlapping_popups_but_only_modals_suspend_document() {
        let mut world = World::new();
        let bounds = Rect {
            x: 300.,
            y: 200.,
            w: 400.,
            h: 300.,
        };
        assert!(!inline_occluded(&world));
        assert!(inline_occlusions(&world, bounds).is_empty());
        let anchor = world.spawn((Layout::row(),));
        let popup = world.spawn((
            AnchoredPopup {
                anchor,
                max_height: 200.,
                closed: |_, _| {},
            },
            Rect {
                x: 20.,
                y: 100.,
                w: 200.,
                h: 300.,
            },
        ));
        assert!(!inline_occluded(&world));
        assert!(inline_occlusions(&world, bounds).is_empty());
        world.get_mut::<Rect>(popup).unwrap().x = 350.;
        assert!(!inline_occluded(&world));
        assert_eq!(
            inline_occlusions(&world, bounds),
            vec![Rect {
                x: 350.,
                y: 200.,
                w: 200.,
                h: 200.
            }]
        );
        // Passive hover/focus hints use the same local clipping, not hide().
        world.insert(popup, PassivePopup);
        assert!(!inline_occluded(&world));
        assert_eq!(inline_occlusions(&world, bounds).len(), 1);
        world.insert(popup, Hidden);
        assert!(!inline_occluded(&world));
        assert!(inline_occlusions(&world, bounds).is_empty());
        let modal = world.spawn((Modal,));
        assert!(inline_occluded(&world));
        world.insert(modal, Hidden);
        assert!(!inline_occluded(&world));
    }
    #[test]
    fn popup_cutout_tracks_painted_transform_shadow_and_dismissal() {
        let mut world = World::new();
        let bounds = Rect {
            x: 0.,
            y: 0.,
            w: 1000.,
            h: 800.,
        };
        let anchor = world.spawn((Layout::row(),));
        let popup = world.spawn((
            AnchoredPopup {
                anchor,
                max_height: 200.,
                closed: |_, _| {},
            },
            Rect {
                x: 100.,
                y: 100.,
                w: 200.,
                h: 40.,
            },
            PassivePopup,
            Transform::translate(0., -4.),
            Shadows(vec![Shadow::drop(2., 8., Color::rgba(0., 0., 0., 0.2))]),
        ));
        assert_eq!(
            inline_occlusions(&world, bounds),
            vec![Rect {
                x: 87.,
                y: 85.,
                w: 226.,
                h: 66.
            }]
        );
        assert!(!inline_occluded(&world));
        world.insert(popup, Hidden);
        assert!(inline_occlusions(&world, bounds).is_empty());
        world.remove::<Hidden>(popup);
        world.get_mut::<Rect>(popup).unwrap().y = 900.;
        assert!(inline_occlusions(&world, bounds).is_empty());
        world.get_mut::<Rect>(popup).unwrap().y = 100.;
        world.get_mut::<Rect>(popup).unwrap().w = 0.;
        assert!(inline_occlusions(&world, bounds).is_empty());
    }
    #[test]
    fn inline_reparent_retains_document_nonce_and_does_not_steal_focus() {
        let (mut world, anchor) = fixture();
        crate::surfaces::select(&mut world, crate::surfaces::SurfaceId::Artifacts);
        let p = world.spawn((Layout::column(),));
        let q = world.spawn((Layout::column(),));
        assert!(request_focus(&mut world, anchor));
        inline(&mut world, p, "session", "artifact", "demo");
        let first = inline_snapshot(&world).unwrap();
        let commands = drain_commands(&mut world);
        if supported() {
            apply(
                &mut world,
                Update {
                    request: Request {
                        session: "session".into(),
                        id: "artifact".into(),
                        nonce: first.1,
                    },
                    result: Ok("<p>live</p>".into()),
                },
            );
            assert_eq!(commands.len(), 1);
        }
        park_inline(&mut world, q);
        despawn(&mut world, p);
        inline(&mut world, q, "session", "artifact", "demo");
        assert_eq!(inline_snapshot(&world), Some(first));
        assert!(drain_commands(&mut world).is_empty());
        assert_eq!(world.resource::<Focus>().unwrap().entity, Some(anchor));
        if supported() {
            assert_eq!(
                world
                    .resource::<State>()
                    .unwrap()
                    .viewer
                    .as_ref()
                    .unwrap()
                    .html
                    .as_deref(),
                Some(prepare_html("<p>live</p>").as_str())
            );
        }
        inline(&mut world, q, "session", "other", "other");
        assert_ne!(inline_snapshot(&world), Some(first));
        assert!(!world.is_alive(first.0));
        apply(
            &mut world,
            Update {
                request: Request {
                    session: "session".into(),
                    id: "artifact".into(),
                    nonce: first.1,
                },
                result: Ok("late".into()),
            },
        );
        assert!(
            world
                .resource::<State>()
                .unwrap()
                .viewer
                .as_ref()
                .unwrap()
                .html
                .is_none()
        );
    }
    #[test]
    fn admission_requires_exact_identity_html_and_source_byte_limit() {
        let value = |id: &str, mime: &str, body: &str| serde_json::json!({ "artifact_id": id, "mime": mime, "label": "demo", "body": body, "byte_size": body.len() });
        assert_eq!(
            admitted(
                &request(1),
                value("artifact", "text/html; charset=utf-8", "")
            ),
            Ok(String::new())
        );
        assert!(admitted(&request(1), value("other", "text/html", "<p>hi</p>")).is_err());
        assert!(
            admitted(
                &request(1),
                value("artifact", "application/xhtml+xml", "<p>hi</p>")
            )
            .is_err()
        );
        assert!(
            admitted(
                &request(1),
                value("artifact", "text/html", &"é".repeat(MAX_HTML / 2 + 1))
            )
            .is_err()
        );
        assert!(
            admitted(
                &request(1),
                value("artifact", "text/html", &"a".repeat(MAX_HTML))
            )
            .is_ok()
        );
    }
    #[test]
    fn replacement_close_and_late_completion_are_fenced() {
        let (mut world, anchor) = fixture();
        open(&mut world, anchor);
        let first = world
            .resource::<State>()
            .unwrap()
            .viewer
            .as_ref()
            .unwrap()
            .nonce;
        let old_root = world
            .resource::<State>()
            .unwrap()
            .viewer
            .as_ref()
            .unwrap()
            .root;
        open(&mut world, anchor);
        assert!(!world.is_alive(old_root));
        apply(
            &mut world,
            Update {
                request: request(first),
                result: Ok("stale".into()),
            },
        );
        assert!(
            world
                .resource::<State>()
                .unwrap()
                .viewer
                .as_ref()
                .unwrap()
                .html
                .is_none()
        );
        let nonce = world
            .resource::<State>()
            .unwrap()
            .viewer
            .as_ref()
            .unwrap()
            .nonce;
        apply(
            &mut world,
            Update {
                request: request(nonce),
                result: Ok("fresh".into()),
            },
        );
        assert_eq!(
            world
                .resource::<State>()
                .unwrap()
                .viewer
                .as_ref()
                .unwrap()
                .html
                .as_deref(),
            Some(prepare_html("fresh").as_str())
        );
        close(&mut world);
        apply(
            &mut world,
            Update {
                request: request(nonce),
                result: Ok("late".into()),
            },
        );
        assert!(world.resource::<State>().unwrap().viewer.is_none());
        assert!(drain_commands(&mut world).is_empty());
    }
    #[test]
    fn destroyed_anchor_and_escape_retire_modal() {
        let (mut world, anchor) = fixture();
        open(&mut world, anchor);
        despawn(&mut world, anchor);
        tick(&mut world);
        assert!(world.resource::<State>().unwrap().viewer.is_none());
        let parent = world.spawn((Layout::column(),));
        let anchor = mount(&mut world, parent, "session", "artifact", "demo");
        open(&mut world, anchor);
        world.resource_mut::<Input>().unwrap().key = Some(Key::Escape);
        tick(&mut world);
        assert!(world.resource::<State>().unwrap().viewer.is_none());
        assert_eq!(world.resource::<Input>().unwrap().key, None);
    }
    #[test]
    fn distinct_artifacts_never_share_retained_control_identity() {
        let (mut world, anchor) = fixture();
        let parent = world.get::<Parent>(anchor).unwrap().entity;
        let second = mount(&mut world, parent, "session", "second", "other");
        assert_ne!(anchor, second);
        assert_eq!(world.get::<Source>(anchor).unwrap().id, "artifact");
        assert_eq!(world.get::<Source>(second).unwrap().id, "second");
        assert_eq!(
            mount(&mut world, parent, "session", "artifact", "demo"),
            anchor
        );
    }
    #[test]
    fn supported_build_queues_isolated_preview_fetch() {
        if !supported() {
            return;
        }
        let (mut world, anchor) = fixture();
        open(&mut world, anchor);
        let commands = drain_commands(&mut world);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].session, "session");
        assert_eq!(commands[0].id, "artifact");
        let status = world
            .resource::<State>()
            .unwrap()
            .viewer
            .as_ref()
            .unwrap()
            .status;
        assert_eq!(
            world.get::<Label>(status).unwrap().text,
            "Loading isolated preview…"
        );
    }
    #[test]
    fn unavailable_build_does_not_fetch_or_fallback() {
        if supported() {
            return;
        }
        let (mut world, anchor) = fixture();
        open(&mut world, anchor);
        assert!(drain_commands(&mut world).is_empty());
        let status = world
            .resource::<State>()
            .unwrap()
            .viewer
            .as_ref()
            .unwrap()
            .status;
        assert!(
            world
                .get::<Label>(status)
                .unwrap()
                .text
                .contains("Preview unavailable")
        );
    }
}

#[cfg(test)]
mod visual_tests {
    use super::*;
    #[test]
    #[ignore = "manual GPU visual evidence"]
    fn export_native_artifact_modal() {
        let mut app = crate::ui::app();
        app.world_mut().insert_resource(Viewport::new(1000., 700.));
        app.update();
        let parent = app.world_mut().spawn((Layout::column(),));
        let anchor = mount(
            app.world_mut(),
            parent,
            "session",
            "artifact",
            "Interactive HTML demo",
        );
        open(app.world_mut(), anchor);
        for _ in 0..8 {
            app.update();
        }
        let mut exporter = rndr_render::Exporter::new().unwrap();
        let path = std::env::var("MEDOUSA_ARTIFACT_SCREENSHOT").expect("screenshot path");
        let (_, frame) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, 1000, 700, 1.)
                .unwrap()
        });
        image::save_buffer(
            path,
            &frame.rgba,
            frame.width,
            frame.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
