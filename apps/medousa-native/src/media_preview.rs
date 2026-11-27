//! Session-scoped authenticated thumbnails. Fetch/decode/resize never run in
//! Update; retained views and a bounded cache avoid reloading on stream events.
use medousa_sdk::MedousaClient;
use rndr_core::*;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub(crate) session: String,
    media: String,
    detailed: bool,
}
struct Pixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}
enum State {
    Pending,
    Ready(Pixels),
    Failed(String),
}
struct Entry {
    id: u64,
    state: State,
    used: u64,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    commands: Vec<Key>,
    next: u64,
    clock: u64,
}
struct Preview {
    label: String,
    key: Key,
    compact: bool,
    rendered: bool,
}
struct Loading;
impl Component for Loading {}
impl Component for Preview {}
pub struct PreviewUpdate {
    key: Key,
    result: Result<Pixels, String>,
}
const MAX_CACHE: usize = 32;

pub fn mount(
    world: &mut World,
    parent: Entity,
    session: &str,
    media: &str,
    label: &str,
    compact: bool,
) -> Entity {
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 5.,
            ..Layout::column()
        },
        Preview {
            label: label.into(),
            key: Key {
                session: session.into(),
                media: media.into(),
                detailed: false,
            },
            compact,
            rendered: false,
        },
    ));
    set_parent(world, root, parent);
    if compact {
        let caption = rndr_ui::text(world, root, label, rndr_ui::Typography::Secondary)
            .expect("image caption");
        world.get_mut::<Label>(caption).unwrap().overflow = TextOverflow::Ellipsis;
    }
    world.insert(
        root,
        Semantics {
            name: Some(format!("Image attachment: {label}")),
            ..Default::default()
        },
    );
    let loading = rndr_ui::text(
        world,
        root,
        "Loading image…",
        rndr_ui::Typography::Secondary,
    )
    .expect("loading image");
    world.insert(loading, Loading);
    root
}

struct Gallery(Vec<(String, String)>);
impl Component for Gallery {}

pub fn gallery(
    world: &mut World,
    parent: Entity,
    session: &str,
    images: &[(String, String)],
) -> Entity {
    let row = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 5.,
            ..Layout::row()
        },
        Gallery(images.to_vec()),
    ));
    set_parent(world, row, parent);
    for (id, label) in images.iter().take(2) {
        mount(world, row, session, id, label, false);
    }
    if images.len() > 2 {
        let more = viewer_button(
            world,
            row,
            0x494d07,
            &format!("+{}", images.len() - 2),
            open_image,
        );
        // Overlay the count instead of reserving a third thumbnail column.
        world.insert(
            more,
            Absolute {
                right: Some(6.),
                top: Some(6.),
                ..Default::default()
            },
        );
        world.get_mut::<Layout>(more).unwrap().width = Size::Px(48.);
        world.insert(more, Layer(1));
        world.insert(
            more,
            Semantics {
                name: Some(format!("Show {} more images", images.len() - 2)),
                ..Default::default()
            },
        );
        world.insert(
            more,
            OpenImage {
                key: Key {
                    session: session.into(),
                    media: images[2].0.clone(),
                    detailed: false,
                },
                anchor: row,
            },
        );
    }
    row
}

pub fn tick(world: &mut World) {
    sync_viewer(world);
    if !world.has_resource::<Cache>() {
        world.insert_resource(Cache::default());
    }
    let mut cache = std::mem::take(world.resource_mut::<Cache>().unwrap());
    let views = world
        .query::<Preview>()
        .iter()
        .map(|(e, _)| *e)
        .collect::<Vec<_>>();
    for root in views {
        let view = world.get::<Preview>(root).unwrap();
        if view.rendered {
            continue;
        }
        let key = view.key.clone();
        let compact = view.compact;
        cache.clock += 1;
        if !cache.entries.contains_key(&key) {
            // Never evict an in-flight request. At most three decode at once.
            if cache.entries.len() >= MAX_CACHE
                || (key.detailed && cache.entries.keys().filter(|k| k.detailed).count() >= 2)
            {
                let victim = cache
                    .entries
                    .iter()
                    .filter(|(k, e)| {
                        !matches!(e.state, State::Pending) && (!key.detailed || k.detailed)
                    })
                    .min_by_key(|(_, e)| e.used)
                    .map(|(k, _)| k.clone());
                if let Some(victim) = victim {
                    cache.entries.remove(&victim);
                } else {
                    continue;
                }
            }
            cache.next += 1;
            cache.entries.insert(
                key.clone(),
                Entry {
                    id: 0x6d65_0000_0000 + cache.next,
                    state: State::Pending,
                    used: cache.clock,
                },
            );
            cache.commands.push(key.clone());
        }
        let entry = cache.entries.get_mut(&key).unwrap();
        entry.used = cache.clock;
        match &entry.state {
            State::Pending => {}
            State::Ready(pixels) => {
                for child in children_of(world, root) {
                    if world.has::<Loading>(child) {
                        despawn(world, child);
                    }
                }
                let _ = rndr_render::load_rgba(
                    world,
                    entry.id,
                    pixels.width,
                    pixels.height,
                    pixels.rgba.clone(),
                );
                let image = world.spawn((
                    Layout {
                        width: Size::Px(if compact { 80. } else { 600. }),
                        height: Size::Px(100.),
                        ..Layout::column()
                    },
                    ImageView::new(entry.id).with_radius(if key.detailed { 0. } else { 10. }),
                    AspectRatio(pixels.width as f32 / pixels.height as f32),
                    MaxSize {
                        width: if compact { 80. } else { 600. },
                        height: if compact { 64. } else { 340. },
                    },
                ));
                set_parent(world, image, root);
                if !key.detailed {
                    world.insert(
                        image,
                        OpenImage {
                            key: key.clone(),
                            anchor: root,
                        },
                    );
                    world.insert(image, Button::new("expand-image"));
                    world.insert(image, ReleaseActivation);
                    world.insert(image, Focusable);
                    world.insert(
                        image,
                        Semantics {
                            name: Some(format!(
                                "Expand image: {}",
                                world.get::<Preview>(root).unwrap().label
                            )),
                            ..Default::default()
                        },
                    );
                    world.insert(image, ActivationHandler(open_image));
                }
                world.get_mut::<Preview>(root).unwrap().rendered = true;
            }
            State::Failed(error) => {
                for child in children_of(world, root) {
                    if world.has::<Loading>(child) {
                        despawn(world, child);
                    }
                }
                rndr_ui::text(
                    world,
                    root,
                    &format!("Preview unavailable · {error}"),
                    rndr_ui::Typography::Secondary,
                )
                .expect("preview fallback");
                world.get_mut::<Preview>(root).unwrap().rendered = true;
            }
        }
    }
    // Responsive aspect-fit, without depending on timeline revisions.
    for root in world
        .query::<Preview>()
        .iter()
        .map(|(e, _)| *e)
        .collect::<Vec<_>>()
    {
        let view = world.get::<Preview>(root).unwrap();
        let compact = view.compact;
        let detailed = view.key.detailed;
        let grouped = world
            .get::<Parent>(root)
            .and_then(|p| world.get::<Gallery>(p.entity))
            .is_some_and(|g| g.0.len() > 1);
        let width = world.get::<Rect>(root).map(|r| r.w).unwrap_or(400.).max(1.);
        for image in children_of(world, root)
            .into_iter()
            .filter(|e| world.has::<ImageView>(*e))
            .collect::<Vec<_>>()
        {
            let ratio = world.get::<AspectRatio>(image).unwrap().0;
            let w = if detailed {
                let zoom = world
                    .resource::<Option<Viewer>>()
                    .and_then(Option::as_ref)
                    .map_or(1., |v| v.zoom);
                let port = world
                    .get::<Parent>(root)
                    .and_then(|p| world.get::<Rect>(p.entity))
                    .copied()
                    .unwrap_or_default();
                let w = port.w.min(port.h * ratio).max(1.) * zoom;
                let frame = world.get_mut::<Layout>(root).unwrap();
                frame.width = Size::Px(w.max(port.w));
                frame.height = Size::Px((w / ratio).max(port.h));
                w
            } else {
                width.min(if compact { 80. } else { 600. }).min(
                    (if compact {
                        64.
                    } else if grouped {
                        168.
                    } else {
                        340.
                    }) * ratio,
                )
            };
            if detailed {
                world.remove::<MaxSize>(image);
            }
            let layout = world.get::<Layout>(image).unwrap();
            if layout.width != Size::Px(w) || layout.height != Size::Px(w / ratio) {
                let layout = world.get_mut::<Layout>(image).unwrap();
                layout.width = Size::Px(w);
                layout.height = Size::Px(w / ratio);
            }
        }
    }
    world.insert_resource(cache);
}

// The viewer owns a modal root, never the transcript subtree. Its request uses
// the same session-authorized endpoint but a separate, bounded detail cache.
#[derive(Clone)]
struct OpenImage {
    key: Key,
    anchor: Entity,
}
impl Component for OpenImage {}
struct Viewer {
    images: Vec<(String, String)>,
    index: usize,
    source: OpenImage,
    root: Entity,
    sheet: Entity,
    anchor: Entity,
    focus: Entity,
    scale: Entity,
    zoom: f32,
}
fn close_viewer(world: &mut World) {
    let Some(v) = world
        .resource_mut::<Option<Viewer>>()
        .and_then(Option::take)
    else {
        return;
    };
    if world.is_alive(v.root) {
        despawn(world, v.root);
    }
    if world.is_alive(v.focus) {
        let _ = request_focus(world, v.focus);
    }
}
fn viewer_button(
    world: &mut World,
    parent: Entity,
    id: u64,
    label: &str,
    handler: fn(&mut World, Entity),
) -> Entity {
    use rndr_ui::{
        UiId,
        controls::{ButtonProps, HostIntent},
    };
    let props = ButtonProps::new(label, HostIntent::Command(id));
    let h =
        rndr_ui::controls::Button::mount(world, parent, UiId(id), props).expect("viewer control");
    world.insert(h.root, ActivationHandler(handler));
    world.insert(
        h.root,
        Semantics {
            name: Some(
                match label {
                    "+" => "Zoom in",
                    "−" => "Zoom out",
                    "Fit" => "Fit image to window",
                    "Close" => "Close image preview",
                    other => other,
                }
                .into(),
            ),
            ..Default::default()
        },
    );
    h.root
}
fn open_image(world: &mut World, entity: Entity) {
    let Some(source) = world.get::<OpenImage>(entity).cloned() else {
        return;
    };
    let gallery = if world.has::<Gallery>(source.anchor) {
        source.anchor
    } else {
        world
            .get::<Parent>(source.anchor)
            .map(|p| p.entity)
            .unwrap_or(source.anchor)
    };
    let images = world
        .get::<Gallery>(gallery)
        .map(|g| g.0.clone())
        .unwrap_or_else(|| {
            vec![(
                source.key.media.clone(),
                world
                    .get::<Preview>(source.anchor)
                    .map(|p| p.label.clone())
                    .unwrap_or_default(),
            )]
        });
    let index = images
        .iter()
        .position(|(id, _)| id == &source.key.media)
        .unwrap_or(0);
    show_image(world, entity, source, images, index);
}
fn step_image(world: &mut World, forward: bool) {
    let Some(v) = world.resource::<Option<Viewer>>().and_then(Option::as_ref) else {
        return;
    };
    let n = v.images.len();
    let i = (v.index + if forward { 1 } else { n - 1 }) % n;
    let mut source = v.source.clone();
    source.key.media = v.images[i].0.clone();
    show_image(world, v.focus, source, v.images.clone(), i);
}
fn lightbox_action(
    world: &mut World,
    parent: Entity,
    id: u64,
    icon: Icon,
    name: &str,
    handler: fn(&mut World, Entity),
) -> Entity {
    let h = crate::native_style::icon_action(world, parent, id, icon, name, true);
    let face = world.get::<Parent>(h.label).unwrap().entity;
    world.insert(face, Align::center());
    world.insert(
        face,
        rndr_ui::UiStyle {
            padding: Some(rndr_ui::Space::Zero),
            ..Default::default()
        },
    );
    world.get_mut::<Layout>(h.label).unwrap().width = Size::Hug;
    let mut layer = face;
    while layer != h.root {
        world.get_mut::<Layout>(layer).unwrap().height = Size::Fill;
        layer = world.get::<Parent>(layer).unwrap().entity;
    }
    world.insert(h.root, ActivationHandler(handler));
    h.root
}

// Reserve the readout's place so changing zoom doesn't shift the buttons
// under the pointer. Center the measured text, not a padded label box.
fn zoom_readout(world: &mut World, dock: Entity) -> Entity {
    let slot = world.spawn((
        Layout {
            width: Size::Px(50.),
            height: Size::Px(crate::native_style::ACTION_SIZE),
            ..Layout::row()
        },
        Align::center(),
    ));
    set_parent(world, slot, dock);
    let label = rndr_ui::text(world, slot, "Fit", rndr_ui::Typography::Secondary).unwrap();
    world.insert(label, LabelBox::CONTENT);
    world.get_mut::<Layout>(label).unwrap().width = Size::Hug;
    label
}

fn lightbox_close(world: &mut World, parent: Entity) {
    let h = crate::native_style::icon_action(
        world,
        parent,
        0x494d04,
        Icon::X,
        "Close image preview",
        true,
    );
    *world.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Px(32.),
        height: Size::Px(32.),
        padding: 1.,
        ..Layout::column()
    };
    // Size the painted layers as well as the hit target. A content-height
    // face otherwise hugs the glyph at the upper left of the larger button.
    let face = world.get::<Parent>(h.label).unwrap().entity;
    world.insert(face, Align::center());
    world.insert(
        face,
        rndr_ui::UiStyle {
            padding: Some(rndr_ui::Space::Zero),
            ..Default::default()
        },
    );
    world.get_mut::<Layout>(h.label).unwrap().width = Size::Hug;
    let mut layer = face;
    while layer != h.root {
        world.get_mut::<Layout>(layer).unwrap().height = Size::Fill;
        world.insert(layer, Radius(16.));
        layer = world.get::<Parent>(layer).unwrap().entity;
    }
    world.insert(h.root, Radius(16.));
    world.insert(h.root, ActivationHandler(|w, _| close_viewer(w)));
}
fn adjust_zoom(world: &mut World, factor: f32) {
    if let Some(v) = world
        .resource_mut::<Option<Viewer>>()
        .and_then(Option::as_mut)
    {
        v.zoom = (v.zoom * factor).clamp(1., 6.);
        let (scale, zoom) = (v.scale, v.zoom);
        world.get_mut::<Label>(scale).unwrap().text = if zoom == 1. {
            "Fit".into()
        } else {
            format!("{zoom:.2}×")
        };
    }
}
fn show_image(
    world: &mut World,
    entity: Entity,
    source: OpenImage,
    images: Vec<(String, String)>,
    index: usize,
) {
    close_viewer(world);
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Layer(180),
        Focusable,
        Modal,
        Color::rgba(0., 0., 0., 0.85),
        Semantics {
            name: Some("Image preview".into()),
            ..Default::default()
        },
    ));
    let backdrop = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Absolute::fill(0.),
        Button::new("close-image-backdrop"),
        ReleaseActivation,
        ActivationHandler(|w, _| close_viewer(w)),
    ));
    set_parent(world, backdrop, root);
    let sheet = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 12.,
            gap: 8.,
            ..Layout::column()
        },
        Absolute::fill(16.),
        Radius(12.),
    ));
    set_parent(world, sheet, root);
    let bar = world.spawn((Layout {
        width: Size::Fill,
        gap: 8.,
        ..Layout::row()
    },));
    set_parent(world, bar, sheet);
    use crate::native_style as style;
    style::plane(world, sheet, style::raised());
    world.insert(
        bar,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let caption = rndr_ui::text(world, bar, &images[index].1, rndr_ui::Typography::Emphasis)
        .expect("image title");
    world.get_mut::<Layout>(caption).unwrap().width = Size::Fill;
    world.get_mut::<Label>(caption).unwrap().overflow = TextOverflow::Ellipsis;
    // Give the quiet close action a circular resting surface without
    // overriding RNDR's hover, pressed or keyboard-focus treatment.
    let close_surface = world.spawn((Layout::column(), Radius(16.)));
    set_parent(world, close_surface, bar);
    style::plane(world, close_surface, style::rail());
    lightbox_close(world, close_surface);
    let body = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Scroll::default(),
    ));
    set_parent(world, body, sheet);
    style::plane(world, body, style::canvas());
    let dock = world.spawn((
        Layout {
            width: Size::Fill,
            gap: 4.,
            ..Layout::row()
        },
        Align::center(),
    ));
    set_parent(world, dock, sheet);
    if images.len() > 1 {
        lightbox_action(
            world,
            dock,
            0x494d05,
            Icon::ChevronLeft,
            "Previous image",
            |w, _| step_image(w, false),
        );
        let count = rndr_ui::text(
            world,
            dock,
            &format!("{} of {}", index + 1, images.len()),
            rndr_ui::Typography::Secondary,
        )
        .unwrap();
        world.insert(
            count,
            style::FitLabel {
                label: count,
                extra: 3.,
                min: 36.,
                max: 120.,
            },
        );
        lightbox_action(
            world,
            dock,
            0x494d06,
            Icon::ChevronRight,
            "Next image",
            |w, _| step_image(w, true),
        );
        let divider = world.spawn((Layout {
            width: Size::Px(1.),
            height: Size::Px(18.),
            ..Layout::column()
        },));
        set_parent(world, divider, dock);
        style::plane(world, divider, style::stroke());
    }
    lightbox_action(world, dock, 0x494d01, Icon::Minus, "Zoom out", |w, _| {
        adjust_zoom(w, 1. / 1.5)
    });
    let scale = zoom_readout(world, dock);
    lightbox_action(world, dock, 0x494d02, Icon::Plus, "Zoom in", |w, _| {
        adjust_zoom(w, 1.5)
    });
    lightbox_action(
        world,
        dock,
        0x494d03,
        Icon::Maximize2,
        "Fit image to window",
        |w, _| adjust_zoom(w, 0.),
    );
    let preview = mount(
        world,
        body,
        &source.key.session,
        &source.key.media,
        "Image preview · zoom with + / − · scroll to pan · Esc to close",
        false,
    );
    world.insert(preview, Align::center());
    world.get_mut::<Preview>(preview).unwrap().key.detailed = true;
    world.insert_resource(Some(Viewer {
        images,
        index,
        source: source.clone(),
        root,
        sheet,
        anchor: source.anchor,
        focus: entity,
        scale,
        zoom: 1.,
    }));
    let _ = request_focus(world, root);
}
pub(crate) fn viewer_open(world: &World) -> bool {
    world
        .resource::<Option<Viewer>>()
        .is_some_and(Option::is_some)
}
fn sync_viewer(world: &mut World) {
    let Some(v) = world.resource::<Option<Viewer>>().and_then(Option::as_ref) else {
        return;
    };
    let stale = !world.is_alive(v.root) || !world.is_alive(v.anchor);
    let escape = world.resource::<Input>().is_some_and(|i| {
        i.key == Some(rndr_core::Key::Escape)
            || i.keys.iter().any(|k| {
                !k.released && !k.synthetic && k.logical == LogicalKey::Named(NamedKey::Escape)
            })
    });
    if stale || escape {
        close_viewer(world);
        if escape {
            if let Some(i) = world.resource_mut::<Input>() {
                i.key = None;
                i.keys
                    .retain(|k| k.logical != LogicalKey::Named(NamedKey::Escape));
            }
        }
        return;
    }
    if v.images.len() > 1 {
        let direction = world.resource::<Input>().and_then(|input| {
            input.keys.iter().find_map(|event| {
                if event.released || event.synthetic {
                    return None;
                }
                match event.logical {
                    LogicalKey::Named(NamedKey::ArrowLeft) => Some(false),
                    LogicalKey::Named(NamedKey::ArrowRight) => Some(true),
                    _ => None,
                }
            })
        });
        if let Some(forward) = direction {
            let input = world.resource_mut::<Input>().unwrap();
            input.keys.retain(|e| {
                !matches!(
                    e.logical,
                    LogicalKey::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight)
                )
            });
            if matches!(
                input.key,
                Some(rndr_core::Key::Left | rndr_core::Key::Right)
            ) {
                input.key = None;
            }
            step_image(world, forward);
            return;
        }
    }
    let sheet = v.sheet;
    let viewport = world.resource::<Viewport>().copied().unwrap_or_default();
    let layout = world.get_mut::<Layout>(sheet).unwrap();
    layout.width = Size::Px((viewport.width - 32.).max(1.));
    layout.height = Size::Px((viewport.height - 32.).max(1.));
}

pub fn drain_commands(world: &mut World) -> Vec<Key> {
    world
        .resource_mut::<Cache>()
        .map(|c| std::mem::take(&mut c.commands))
        .unwrap_or_default()
}
pub fn apply(world: &mut World, update: PreviewUpdate) {
    if let Some(entry) = world
        .resource_mut::<Cache>()
        .and_then(|c| c.entries.get_mut(&update.key))
    {
        entry.state = match update.result {
            Ok(pixels) => State::Ready(pixels),
            Err(error) => State::Failed(error),
        };
    }
}
fn path(key: &Key) -> String {
    // Encode the opaque ID as one path component, never an external URL.
    let id = urlencoding::encode(&key.media);
    medousa_sdk::transport::path_with_query(
        &format!("/v1/media/{id}"),
        &[("session_id", crate::routing::wire_owned(&key.session))],
    )
}
pub async fn execute(client: &MedousaClient, key: Key) -> PreviewUpdate {
    let started = std::time::Instant::now();
    let detailed = key.detailed;
    let result = async {
        let bytes = client
            .transport()
            .get_bytes(client.base_url(), &path(&key), 25 * 1024 * 1024)
            .await
            .map_err(|_| "Could not load image".to_string())?;
        tokio::task::spawn_blocking(move || decode_at(&bytes, detailed))
            .await
            .map_err(|_| "Image decoder unavailable".to_string())?
    }
    .await;
    if cfg!(debug_assertions) {
        eprintln!(
            "native image preview elapsed_ms={} ready={}",
            started.elapsed().as_millis(),
            result.is_ok()
        );
    }
    PreviewUpdate { key, result }
}
#[cfg(test)]
fn decode(bytes: &[u8]) -> Result<Pixels, String> {
    decode_at(bytes, false)
}
fn decode_at(bytes: &[u8], detailed: bool) -> Result<Pixels, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "Unsupported image")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Unsupported, damaged, or oversized image")?;
    // Never fabricate extra resolution for small images.
    let image = image
        .thumbnail(
            image.width().min(if detailed { 4096 } else { 800 }),
            image.height().min(if detailed { 4096 } else { 600 }),
        )
        .to_rgba8();
    Ok(Pixels {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

#[cfg(test)]
pub fn resolve_fixture(world: &mut World, bytes: &[u8]) -> usize {
    tick(world);
    let _ = drain_commands(world);
    let keys: Vec<_> = world
        .resource::<Cache>()
        .unwrap()
        .entries
        .keys()
        .cloned()
        .collect();
    let count = keys.len();
    for key in keys {
        apply(
            world,
            PreviewUpdate {
                result: decode_at(bytes, key.detailed),
                key,
            },
        );
    }
    tick(world);
    count
}

#[cfg(test)]
#[path = "media_preview_tests.rs"]
mod tests;
