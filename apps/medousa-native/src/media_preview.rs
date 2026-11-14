//! Session-scoped authenticated thumbnails. Fetch/decode/resize never run in
//! Update; retained views and a bounded cache avoid reloading on stream events.
use medousa_sdk::MedousaClient;
use rndr_core::*;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    session: String,
    media: String,
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
            key: Key {
                session: session.into(),
                media: media.into(),
            },
            compact,
            rendered: false,
        },
    ));
    set_parent(world, root, parent);
    let caption =
        rndr_ui::text(world, root, label, rndr_ui::Typography::Secondary).expect("image caption");
    world.get_mut::<Label>(caption).unwrap().overflow = TextOverflow::Ellipsis;
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

pub fn tick(world: &mut World) {
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
            if cache.entries.len() >= MAX_CACHE {
                let victim = cache
                    .entries
                    .iter()
                    .filter(|(_, e)| !matches!(e.state, State::Pending))
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
                        width: Size::Px(if compact { 80. } else { 400. }),
                        height: Size::Px(100.),
                        ..Layout::column()
                    },
                    ImageView::new(entry.id).with_radius(10.),
                    AspectRatio(pixels.width as f32 / pixels.height as f32),
                    MaxSize {
                        width: if compact { 80. } else { 400. },
                        height: if compact { 64. } else { 280. },
                    },
                ));
                set_parent(world, image, root);
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
        let compact = world.get::<Preview>(root).unwrap().compact;
        let width = world.get::<Rect>(root).map(|r| r.w).unwrap_or(400.).max(1.);
        for image in children_of(world, root)
            .into_iter()
            .filter(|e| world.has::<ImageView>(*e))
            .collect::<Vec<_>>()
        {
            let ratio = world.get::<AspectRatio>(image).unwrap().0;
            let w = width
                .min(if compact { 80. } else { 400. })
                .min((if compact { 64. } else { 280. }) * ratio);
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
        &[("session_id", key.session.clone())],
    )
}
pub async fn execute(client: &MedousaClient, key: Key) -> PreviewUpdate {
    let started = std::time::Instant::now();
    let result = async {
        let bytes = client
            .transport()
            .get_bytes(client.base_url(), &path(&key), 25 * 1024 * 1024)
            .await
            .map_err(|_| "Could not load image".to_string())?;
        tokio::task::spawn_blocking(move || decode(&bytes))
            .await
            .map_err(|_| "Image decoder unavailable".to_string())?
    }
    .await;
    eprintln!(
        "native image preview elapsed_ms={} ready={}",
        started.elapsed().as_millis(),
        result.is_ok()
    );
    PreviewUpdate { key, result }
}
fn decode(bytes: &[u8]) -> Result<Pixels, String> {
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
        .map_err(|_| "Unsupported, damaged, or oversized image")?
        .thumbnail(800, 600)
        .to_rgba8();
    Ok(Pixels {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

#[cfg(test)]
#[path = "media_preview_tests.rs"]
mod tests;
