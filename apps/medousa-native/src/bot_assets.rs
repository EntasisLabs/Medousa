//! Bundled Medousa mascot avatars. No daemon or local filesystem access is
//! needed at runtime. SVG artwork retains its supplied palette and aspect ratio.
//! `mount` returns a decorative ImageView; name the owning button/header for
//! accessibility. Call `refresh` when the avatar, size, or viewport DPI changes.
use rndr_core::*;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug)]
pub struct Asset {
    /// Stable native identifier, e.g. `jellyfish-focus`.
    pub key: &'static str,
    pub label: &'static str,
    pub svg: &'static str,
}

pub const DEFAULT_KEY: &str = "jellyfish-default";
pub const CATALOG: &[Asset] = &[
    Asset {
        key: "jellyfish-default",
        label: "Jellyfish Default",
        svg: include_str!("bot_assets/jellyfish-default.svg"),
    },
    Asset {
        key: "jellyfish-happy",
        label: "Jellyfish Happy",
        svg: include_str!("bot_assets/jellyfish-happy.svg"),
    },
    Asset {
        key: "jellyfish-chill",
        label: "Jellyfish Chill",
        svg: include_str!("bot_assets/jellyfish-chill.svg"),
    },
    Asset {
        key: "jellyfish-sweet",
        label: "Jellyfish Sweet",
        svg: include_str!("bot_assets/jellyfish-sweet.svg"),
    },
    Asset {
        key: "jellyfish-focus",
        label: "Jellyfish Focus",
        svg: include_str!("bot_assets/jellyfish-focus.svg"),
    },
    Asset {
        key: "jellyfish-sus",
        label: "Jellyfish Sus",
        svg: include_str!("bot_assets/jellyfish-sus.svg"),
    },
    Asset {
        key: "seahorse-default",
        label: "Seahorse Default",
        svg: include_str!("bot_assets/seahorse-default.svg"),
    },
    Asset {
        key: "seahorse-happy",
        label: "Seahorse Happy",
        svg: include_str!("bot_assets/seahorse-happy.svg"),
    },
    Asset {
        key: "seahorse-chill",
        label: "Seahorse Chill",
        svg: include_str!("bot_assets/seahorse-chill.svg"),
    },
    Asset {
        key: "seahorse-sweet",
        label: "Seahorse Sweet",
        svg: include_str!("bot_assets/seahorse-sweet.svg"),
    },
    Asset {
        key: "seahorse-focus",
        label: "Seahorse Focus",
        svg: include_str!("bot_assets/seahorse-focus.svg"),
    },
    Asset {
        key: "seahorse-sus",
        label: "Seahorse Sus",
        svg: include_str!("bot_assets/seahorse-sus.svg"),
    },
    Asset {
        key: "starfish-default",
        label: "Starfish Default",
        svg: include_str!("bot_assets/starfish-default.svg"),
    },
    Asset {
        key: "starfish-happy",
        label: "Starfish Happy",
        svg: include_str!("bot_assets/starfish-happy.svg"),
    },
    Asset {
        key: "starfish-chill",
        label: "Starfish Chill",
        svg: include_str!("bot_assets/starfish-chill.svg"),
    },
    Asset {
        key: "starfish-sweet",
        label: "Starfish Sweet",
        svg: include_str!("bot_assets/starfish-sweet.svg"),
    },
    Asset {
        key: "starfish-focus",
        label: "Starfish Focus",
        svg: include_str!("bot_assets/starfish-focus.svg"),
    },
    Asset {
        key: "starfish-sus",
        label: "Starfish Sus",
        svg: include_str!("bot_assets/starfish-sus.svg"),
    },
];

/// Accept native keys and the persisted Tauri bot avatar syntax. `medousa` is
/// the Tauri name for jellyfish. Invalid references are not silently persisted.
pub fn lookup(key: &str) -> Option<&'static Asset> {
    let key = key.trim();
    if let Some(asset) = CATALOG.iter().find(|asset| asset.key == key) {
        return Some(asset);
    }
    let tail = key.strip_prefix("mascot:")?;
    let (body, mood) = tail.split_once(':').unwrap_or((tail, "default"));
    let body = if body == "medousa" { "jellyfish" } else { body };
    CATALOG.iter().find(|asset| {
        asset
            .key
            .strip_prefix(body)
            .and_then(|s| s.strip_prefix('-'))
            == Some(mood)
    })
}

#[derive(Default)]
struct Loaded(HashSet<u64>);

struct Artwork {
    key: &'static str,
    size: f32,
}
impl Component for Artwork {}

/// Retained avatar views follow display/DPI changes even when the bot model
/// and navigation document haven't changed, without rebuilding the controls.
pub fn sync(world: &mut World) {
    let images: Vec<_> = world
        .query::<Artwork>()
        .iter()
        .map(|(e, a)| (*e, a.key, a.size))
        .collect();
    for (entity, key, size) in images {
        refresh(world, entity, key, size);
    }
}

fn logical_size(size: f32) -> f32 {
    if size.is_finite() {
        size.clamp(1., 512.)
    } else {
        32.
    }
}

fn pixel_size(world: &World, size: f32) -> u32 {
    let scale = world
        .resource::<Viewport>()
        .copied()
        .unwrap_or_default()
        .pixels_per_point();
    (logical_size(size) * scale).ceil().clamp(1., 1024.) as u32
}

// Dedicated image namespace; both source and resolution are encoded because
// RNDR GPU textures are immutable by identity (same-id uploads stay stale).
fn image_id(index: usize, px: u32) -> u64 {
    0x424f_5400_0000_0000 | ((index as u64) << 16) | px as u64
}

/// Decode at physical resolution and center on a square transparent canvas.
/// RNDR's decode_svg fits but does not pad; ImageView stretches to its rect,
/// so supplying the unpadded non-square jellyfish/seahorse raster distorts it.
fn raster(asset: &Asset, px: u32) -> rndr_render::Raster {
    let source =
        rndr_render::decode_svg(asset.svg.as_bytes(), px, px).expect("valid bundled mascot SVG");
    let mut rgba = vec![0; (px * px * 4) as usize];
    let x = (px - source.width) / 2;
    let y = (px - source.height) / 2;
    for row in 0..source.height {
        let from = (row * source.width * 4) as usize;
        let to = (((row + y) * px + x) * 4) as usize;
        let width = (source.width * 4) as usize;
        rgba[to..to + width].copy_from_slice(&source.rgba[from..from + width]);
    }
    rndr_render::Raster {
        width: px,
        height: px,
        rgba,
    }
}

/// Mount a square avatar in logical points. Unknown/legacy non-mascot keys use
/// the jellyfish default; inspect `lookup` to distinguish them in editor UI.
/// Original palettes are intentional; apply any accent to the host container.
pub fn mount(world: &mut World, parent: Entity, key: &str, size: f32) -> Entity {
    let entity = world.spawn((
        Layout {
            width: Size::Px(logical_size(size)),
            height: Size::Px(logical_size(size)),
            ..Layout::column()
        },
        ImageView::new(0),
    ));
    set_parent(world, entity, parent);
    refresh(world, entity, key, size);
    entity
}

/// Refresh an avatar mounted by this module, reusing existing image uploads.
/// No-op for despawned entities or entities without image/layout components. Safe to call on every frame.
pub fn refresh(world: &mut World, entity: Entity, key: &str, size: f32) {
    if !world.has::<ImageView>(entity) || !world.has::<Layout>(entity) {
        return;
    }
    let asset = lookup(key).unwrap_or_else(|| lookup(DEFAULT_KEY).expect("bundled default avatar"));
    let index = CATALOG
        .iter()
        .position(|entry| entry.key == asset.key)
        .unwrap();
    world.insert(
        entity,
        Artwork {
            key: asset.key,
            size: logical_size(size),
        },
    );
    let px = pixel_size(world, size);
    let id = image_id(index, px);
    if !world.has_resource::<Loaded>() {
        world.insert_resource(Loaded::default());
    }
    if !world.resource::<Loaded>().unwrap().0.contains(&id) {
        let raster = raster(asset, px);
        rndr_render::load_rgba(world, id, raster.width, raster.height, raster.rgba)
            .expect("valid bundled mascot raster");
        world.resource_mut::<Loaded>().unwrap().0.insert(id);
    }
    world.get_mut::<ImageView>(entity).unwrap().image = id;
    let layout = world.get_mut::<Layout>(entity).unwrap();
    layout.width = Size::Px(logical_size(size));
    layout.height = Size::Px(logical_size(size));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_complete_and_persisted_keys_are_compatible() {
        assert_eq!(CATALOG.len(), 18);
        let mut keys = HashSet::new();
        for asset in CATALOG {
            assert!(keys.insert(asset.key));
            assert_eq!(lookup(asset.key).unwrap().key, asset.key);
            let (body, mood) = asset.key.split_once('-').unwrap();
            let body = if body == "jellyfish" { "medousa" } else { body };
            assert_eq!(
                lookup(&format!("mascot:{body}:{mood}")).unwrap().key,
                asset.key
            );
        }
        assert_eq!(lookup("mascot:medousa").unwrap().key, DEFAULT_KEY);
        assert_eq!(
            lookup("mascot:jellyfish:focus").unwrap().key,
            "jellyfish-focus"
        );
        for invalid in [
            "",
            "mascot:",
            "mascot:unknown",
            "mascot:medousa:bogus",
            "mascot:seahorse:happy:extra",
        ] {
            assert!(lookup(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn all_sources_rasterize_at_rail_and_header_sizes_without_distortion() {
        for asset in CATALOG {
            for px in [1, 32, 64, 96, 192, 1024] {
                let image = raster(asset, px);
                assert_eq!((image.width, image.height), (px, px));
                assert_eq!(image.rgba.len(), (px * px * 4) as usize);
                assert!(image.rgba.chunks_exact(4).any(|p| p[3] > 0));
                if px > 1 {
                    assert!(image.rgba.chunks_exact(4).any(|p| p[3] == 0));
                }
            }
        }
    }

    #[test]
    fn mount_reuses_uploads_and_refresh_changes_identity() {
        let mut world = World::new();
        let parent = world.spawn(Layout::column());
        let a = mount(&mut world, parent, "mascot:medousa", 32.);
        let b = mount(&mut world, parent, "jellyfish-default", 32.);
        let first = world.get::<ImageView>(a).unwrap().image;
        assert_eq!(first, world.get::<ImageView>(b).unwrap().image);
        assert_eq!(world.resource::<Loaded>().unwrap().0.len(), 1);
        refresh(&mut world, a, "starfish-happy", 32.);
        assert_ne!(first, world.get::<ImageView>(a).unwrap().image);
        refresh(&mut world, a, "jellyfish-default", 64.);
        assert_ne!(first, world.get::<ImageView>(a).unwrap().image);
        assert_eq!(world.resource::<Loaded>().unwrap().0.len(), 3);
        let fallback = mount(&mut world, parent, "unknown", f32::NAN);
        assert_eq!(first, world.get::<ImageView>(fallback).unwrap().image);
    }

    #[test]
    fn refresh_tracks_viewport_dpi_without_changing_logical_size() {
        let mut world = World::new();
        world.insert_resource(Viewport {
            scale: 1.,
            ..Default::default()
        });
        let parent = world.spawn(Layout::column());
        let avatar = mount(&mut world, parent, DEFAULT_KEY, 32.);
        assert_eq!(
            world.get::<ImageView>(avatar).unwrap().image,
            image_id(0, 32)
        );
        world.resource_mut::<Viewport>().unwrap().scale = 2.;
        sync(&mut world);
        assert_eq!(
            world.get::<ImageView>(avatar).unwrap().image,
            image_id(0, 64)
        );
        assert_eq!(world.get::<Layout>(avatar).unwrap().width, Size::Px(32.));
        assert_eq!(world.resource::<Loaded>().unwrap().0.len(), 2);
    }

    #[test]
    fn image_ids_are_unique_and_size_inputs_bounded() {
        let mut ids = HashSet::new();
        for index in 0..CATALOG.len() {
            for px in 1..=1024 {
                assert!(ids.insert(image_id(index, px)));
            }
        }
        assert_eq!(logical_size(-1.), 1.);
        assert_eq!(logical_size(f32::INFINITY), 32.);
        assert_eq!(logical_size(99999.), 512.);
    }
}
