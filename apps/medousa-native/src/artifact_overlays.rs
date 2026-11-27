//! Trusted RNDR popup pixels above a native guest; never a screenshot of HTML.
use super::*;
use rndr_browser::{ClientRect, OverlayBitmap};
use rndr_paint::{Frame, Op};

#[derive(Default)]
pub(super) struct Cache {
    exporter: Option<rndr_render::Exporter>,
    entries: Vec<Entry>,
    overlays: Vec<OverlayBitmap>,
}
struct Entry {
    entity: Entity,
    paint: String,
    // Hold atlas snapshots so allocator address reuse cannot fool the key.
    _coverage: Option<std::sync::Arc<FontAtlas>>,
    _color: Option<std::sync::Arc<ColorAtlas>>,
}

impl Cache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.overlays.clear();
    }

    pub(super) fn sync(
        &mut self,
        world: &World,
        guest: Rect,
        scale: f32,
        viewport: [u32; 2],
    ) -> Result<&[OverlayBitmap], String> {
        if !scale.is_finite() || scale <= 0. || viewport.contains(&0) {
            return Err("Invalid popup raster scale".into());
        }
        let mut old: Vec<_> = std::mem::take(&mut self.entries)
            .into_iter()
            .zip(std::mem::take(&mut self.overlays))
            .collect();
        let mut entries = Vec::new();
        let mut overlays = Vec::new();
        let mut bytes = 0usize;
        for (entity, rect) in popup_regions(world, guest) {
            if entries.len() >= 32 {
                return Err("Too many native popup overlays".into());
            }
            // Snap outward to physical pixels, preserving Retina glyph edges.
            let x = (rect.x * scale).floor() / scale;
            let y = (rect.y * scale).floor() / scale;
            let width = ((rect.x + rect.w) * scale).ceil() as u32 - (x * scale).round() as u32;
            let height = ((rect.y + rect.h) * scale).ceil() as u32 - (y * scale).round() as u32;
            bytes = bytes.saturating_add(
                (width as usize)
                    .saturating_mul(height as usize)
                    .saturating_mul(4),
            );
            if width == 0
                || height == 0
                || width > 4096
                || height > 4096
                || bytes > 16 * 1024 * 1024
            {
                return Err("Native popup overlay exceeds pixel budget".into());
            }
            let bounds = ClientRect::try_new(x, y, width as f32 / scale, height as f32 / scale)
                .map_err(|e| format!("Invalid native popup bounds: {e:?}"))?;
            let pointer_bounds = if world.has::<PassivePopup>(entity) {
                None
            } else {
                world.get::<Rect>(entity).copied().and_then(|r| {
                    let r = transformed_rect(world, entity, r);
                    intersect(r, guest).and_then(|r| ClientRect::try_new(r.x, r.y, r.w, r.h).ok())
                })
            };
            let (quads, draws) = rndr_paint::paint_subtree(world, entity);
            let frame = Frame {
                quads,
                draws,
                slots: vec![],
            };
            let engine = text_engine(world);
            let atlas = engine.coverage_atlas();
            let color = engine.color_atlas();
            let atlas_id = atlas.as_ref().map(|a| a.r8.as_ptr());
            let color_id = color.as_ref().map(|a| a.rgba.as_ptr());
            let mut paint = format!(
                "{bounds:?}:{pointer_bounds:?}:{scale}:{viewport:?}:{atlas_id:?}:{color_id:?}:{frame:?}"
            );
            // These sources can change pixels without changing their draw IDs.
            if frame.draws.iter().any(|d| {
                matches!(
                    d.op,
                    Op::Image(_) | Op::Surface(_) | Op::EffectSlot(_) | Op::Backdrop(_)
                )
            }) {
                use std::fmt::Write;
                let _ = write!(
                    paint,
                    ":{}",
                    world.resource::<Time>().map_or(0, |t| t.frame)
                );
            }
            let bitmap = if let Some(index) = old
                .iter()
                .position(|(e, _)| e.entity == entity && e.paint == paint)
            {
                old.swap_remove(index).1
            } else {
                if self.exporter.is_none() {
                    self.exporter = Some(
                        rndr_render::Exporter::new()
                            .map_err(|e| format!("Native popup renderer unavailable: {e}"))?,
                    );
                }
                let image = self
                    .exporter
                    .as_mut()
                    .unwrap()
                    .render_frame(Some(world), &frame, viewport[0], viewport[1], scale)
                    .map_err(|e| format!("Native popup could not render: {e}"))?;
                let rgba = crop_pixels(
                    &image,
                    (x * scale).round() as u32,
                    (y * scale).round() as u32,
                    width,
                    height,
                )?;
                OverlayBitmap {
                    bounds,
                    width,
                    height,
                    rgba,
                    pointer_bounds,
                }
            };
            entries.push(Entry {
                entity,
                paint,
                _coverage: atlas,
                _color: color,
            });
            overlays.push(bitmap);
        }
        self.overlays = overlays;
        self.entries = entries;
        Ok(&self.overlays)
    }
}

/// Screen-space UI quads are rendered in window coordinates, then only the
/// small popup region is uploaded. Transparent subtree pixels are not guest
/// pixels. The exporter bounds transient readback; unchanged paint reuses it.
fn crop_pixels(
    image: &rndr_render::RgbaImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    if x.checked_add(width).is_none_or(|right| right > image.width)
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > image.height)
    {
        return Err("Popup raster lies outside the window".into());
    }
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for row in y..y + height {
        let start = (row as usize * image.width as usize + x as usize) * 4;
        pixels.extend_from_slice(&image.rgba[start..start + width as usize * 4]);
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_raster_is_cropped_transparent_and_cached_without_guest_pixels() {
        let mut world = World::new();
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
            Color::rgba(0.9, 0.2, 0.1, 1.),
            ResolvedColor,
            Shadows(vec![Shadow::drop(2., 8., Color::rgba(0., 0., 0., 0.2))]),
        ));
        let guest = Rect {
            x: 0.,
            y: 0.,
            w: 1000.,
            h: 800.,
        };
        let mut cache = Cache::default();
        let overlays = cache.sync(&world, guest, 2., [2000, 1600]).unwrap();
        assert_eq!(overlays.len(), 1);
        let bitmap = &overlays[0];
        assert_eq!((bitmap.width, bitmap.height), (452, 132));
        assert!(bitmap.pointer_bounds.is_some());
        let center = (66 * bitmap.width as usize + 226) * 4;
        assert!(
            bitmap.rgba[center] > 200,
            "popup color must survive cropping"
        );
        assert_eq!(bitmap.rgba[center + 3], 255, "popup body stays opaque");
        assert!(
            bitmap.rgba[3] < 30,
            "outside shadow must remain transparent"
        );
        let pixels = bitmap.rgba.as_ptr();
        let again = cache.sync(&world, guest, 2., [2000, 1600]).unwrap();
        assert_eq!(
            pixels,
            again[0].rgba.as_ptr(),
            "unchanged popup reuses pixels, not a guest snapshot"
        );
        world.insert(popup, PassivePopup);
        assert!(
            cache.sync(&world, guest, 2., [2000, 1600]).unwrap()[0]
                .pointer_bounds
                .is_none()
        );
        world.insert(popup, Hidden);
        assert!(
            cache
                .sync(&world, guest, 2., [2000, 1600])
                .unwrap()
                .is_empty()
        );
    }
}
