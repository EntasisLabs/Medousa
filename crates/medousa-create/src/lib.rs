//! Renderer-independent Create foundation. Synchronous storage methods must be
//! admitted through the daemon's bounded execution service before async use.
mod store;
pub use medousa_types::creative::*;
pub use store::{CreativeDispatch, CreativeStore};

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, thiserror::Error)]
pub enum CreativeError {
    #[error("invalid creative document: {0}")]
    Invalid(String),
    #[error("creative project not found")]
    NotFound,
    #[error("creative revision conflict (current revision {0})")]
    Conflict(u64),
    #[error("unsupported creative document version; original source preserved")]
    Unsupported,
    #[error("creative storage: {0}")]
    Storage(String),
}
pub type Result<T> = std::result::Result<T, CreativeError>;
fn invalid(message: impl Into<String>) -> CreativeError {
    CreativeError::Invalid(message.into())
}
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(invalid("invalid identifier"));
    }
    Ok(())
}
fn bounded_text(value: &str, max: usize, field: &str) -> Result<()> {
    if value.len() > max || value.chars().any(|c| c == '\0') {
        return Err(invalid(format!("invalid {field}")));
    }
    Ok(())
}
fn number(value: f64, min: f64, max: f64) -> Result<()> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(invalid("numeric value out of bounds"));
    }
    Ok(())
}
fn paint(value: &str) -> Result<()> {
    if value == "transparent"
        || (matches!(value.len(), 4 | 5 | 7 | 9)
            && value.starts_with('#')
            && value[1..].bytes().all(|c| c.is_ascii_hexdigit()))
    {
        Ok(())
    } else {
        Err(invalid("paint must be a hex color or transparent"))
    }
}
pub fn validate_scene(
    scene: &CreativeScene,
    assets: &BTreeMap<String, CreativeAsset>,
) -> Result<()> {
    if scene.schema != CREATIVE_SCHEMA || scene.version != CREATIVE_VERSION {
        return Err(CreativeError::Unsupported);
    }
    if scene.nodes.len() > 10_000
        || serde_json::to_vec(scene)
            .map_err(|e| invalid(e.to_string()))?
            .len()
            > MAX_CREATIVE_SCENE_BYTES
    {
        return Err(invalid("scene exceeds capacity"));
    }
    paint(&scene.background)?;
    let mut nodes = BTreeMap::new();
    let mut total_points = 0usize;
    for node in &scene.nodes {
        validate_id(&node.id)?;
        if let Some(name) = &node.name {
            bounded_text(name, 256, "layer name")?;
            if name.trim().is_empty() {
                return Err(invalid("layer name is empty"));
            }
        }
        if nodes.insert(node.id.as_str(), node).is_some() {
            return Err(invalid("duplicate node id"));
        }
        let t = &node.transform;
        number(t.x, -1e9, 1e9)?;
        number(t.y, -1e9, 1e9)?;
        number(t.width, 0., 1e7)?;
        number(t.height, 0., 1e7)?;
        number(t.rotation, -360_000., 360_000.)?;
        number(node.opacity, 0., 1.)?;
        match &node.content {
            CreativeContent::Ink {
                color,
                brush,
                points,
                ..
            } => {
                paint(color)?;
                number(brush.size, 0.25, 160.)?;
                for v in [
                    brush.thinning,
                    brush.smoothing,
                    brush.streamline,
                    brush.opacity,
                ] {
                    number(v, 0., 1.)?;
                }
                total_points = total_points.saturating_add(points.len());
                if points.is_empty() || total_points > 250_000 {
                    return Err(invalid("ink point capacity exceeded"));
                }
                for p in points {
                    number(p.x, -1e9, 1e9)?;
                    number(p.y, -1e9, 1e9)?;
                    if let Some(v) = p.pressure {
                        number(v, 0., 1.)?;
                    }
                    if let Some(v) = p.elapsed_ms {
                        number(v, 0., 86_400_000.)?;
                    }
                    for v in [p.tilt_x, p.tilt_y].into_iter().flatten() {
                        number(v, -90., 90.)?;
                    }
                }
            }
            CreativeContent::Text {
                text,
                color,
                font_family,
                font_size,
                font_weight,
                ..
            } => {
                bounded_text(text, 100_000, "text")?;
                paint(color)?;
                bounded_text(font_family, 128, "font family")?;
                if font_family.is_empty()
                    || !font_family
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
                {
                    return Err(invalid("invalid font family"));
                }
                number(*font_size, 1., 4096.)?;
                if let Some(weight) = font_weight {
                    number(f64::from(*weight), 100., 900.)?;
                }
            }
            CreativeContent::Shape {
                fill,
                stroke,
                stroke_width,
                ..
            } => {
                paint(fill)?;
                paint(stroke)?;
                number(*stroke_width, 0., 1000.)?;
            }
            CreativeContent::Image { asset_id, crop } => {
                validate_id(asset_id)?;
                if !assets.contains_key(asset_id) {
                    return Err(invalid("image references an unknown project asset"));
                }
                if let Some(c) = crop {
                    number(c.x, 0., 1.)?;
                    number(c.y, 0., 1.)?;
                    number(c.width, f64::EPSILON, 1.)?;
                    number(c.height, f64::EPSILON, 1.)?;
                    if c.x + c.width > 1. + 1e-9 || c.y + c.height > 1. + 1e-9 {
                        return Err(invalid("crop extends outside image"));
                    }
                }
            }
            CreativeContent::Frame { name, background } => {
                bounded_text(name, 256, "frame name")?;
                paint(background)?;
            }
            CreativeContent::Group => {}
        }
    }
    // Each parent chain is traversed once, avoiding quadratic validation on a
    // deeply nested but otherwise valid scene supplied by a model or client.
    let mut done = BTreeSet::new();
    for id in nodes.keys() {
        let mut path = BTreeSet::new();
        let mut cursor = Some(*id);
        while let Some(id) = cursor {
            if done.contains(id) {
                break;
            }
            if !path.insert(id) {
                return Err(invalid("cyclic node hierarchy"));
            }
            let node = nodes.get(id).ok_or_else(|| invalid("missing parent"))?;
            cursor = node.parent_id.as_deref();
            if let Some(parent) = cursor {
                let parent_node = nodes.get(parent).ok_or_else(|| invalid("missing parent"))?;
                if !matches!(
                    parent_node.content,
                    CreativeContent::Group | CreativeContent::Frame { .. }
                ) {
                    return Err(invalid("parent must be a group or frame"));
                }
            }
        }
        done.extend(path);
    }
    Ok(())
}
pub fn validate_document(doc: &CreativeDocument) -> Result<()> {
    validate_id(&doc.id)?;
    bounded_text(&doc.title, 256, "project title")?;
    if doc.title.trim().is_empty() {
        return Err(invalid("project title is empty"));
    }
    if doc.assets.len() > 256
        || doc
            .assets
            .values()
            .map(|a| a.byte_size as u128)
            .sum::<u128>()
            > 512 * 1024 * 1024
    {
        return Err(invalid("project asset capacity exceeded"));
    }
    for (id, asset) in &doc.assets {
        if id != &asset.id
            || asset.sha256.len() != 64
            || !asset.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || id != &format!("a_{}", asset.sha256)
            || asset.byte_size == 0
            || asset.byte_size > MAX_CREATIVE_ASSET_BYTES as u64
            || !matches!(
                asset.mime.as_str(),
                "image/png" | "image/jpeg" | "image/webp" | "image/gif"
            )
        {
            return Err(invalid("invalid project asset"));
        }
    }
    validate_scene(&doc.scene, &doc.assets)
}

pub fn apply_operations(
    doc: &CreativeDocument,
    ops: &[CreativeOperation],
) -> Result<CreativeDocument> {
    if ops.is_empty() || ops.len() > 1000 {
        return Err(invalid("transaction must contain 1–1000 operations"));
    }
    let mut next = doc.clone();
    let mut cascaded = BTreeSet::new();
    for op in ops {
        match op {
            CreativeOperation::PutNode { node } => {
                if let Some(old) = next.scene.nodes.iter_mut().find(|n| n.id == node.id) {
                    *old = node.clone();
                } else {
                    next.scene.nodes.push(node.clone());
                }
            }
            CreativeOperation::RemoveNode { id } => {
                if !next.scene.nodes.iter().any(|n| &n.id == id) {
                    // Older clients may include explicit descendant removals
                    // after their parent. A genuinely absent target is invalid.
                    if cascaded.contains(id) {
                        continue;
                    }
                    return Err(invalid("removed node does not exist"));
                }
                let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
                for node in &next.scene.nodes {
                    if let Some(parent) = node.parent_id.as_deref() {
                        children.entry(parent).or_default().push(&node.id);
                    }
                }
                let mut removed = BTreeSet::from([id.as_str()]);
                let mut queue = vec![id.as_str()];
                while let Some(parent) = queue.pop() {
                    if let Some(descendants) = children.get(parent) {
                        for child in descendants {
                            if removed.insert(child) {
                                queue.push(child);
                            }
                        }
                    }
                }
                cascaded.extend(
                    removed
                        .iter()
                        .filter(|target| **target != id)
                        .map(|target| (*target).to_string()),
                );
                let removed: BTreeSet<String> = removed.into_iter().map(str::to_string).collect();
                next.scene.nodes.retain(|node| !removed.contains(&node.id));
            }
            CreativeOperation::Reorder { ids } => {
                let mut by_id: BTreeMap<_, _> = next
                    .scene
                    .nodes
                    .drain(..)
                    .map(|n| (n.id.clone(), n))
                    .collect();
                if ids.len() != by_id.len() {
                    return Err(invalid("reorder must include every node"));
                }
                for id in ids {
                    next.scene.nodes.push(
                        by_id
                            .remove(id)
                            .ok_or_else(|| invalid("reorder contains duplicate or unknown id"))?,
                    );
                }
            }
            CreativeOperation::SetBackground { color } => next.scene.background = color.clone(),
            CreativeOperation::Rename { title } => next.title = title.clone(),
            CreativeOperation::Pin { pinned } => next.pinned = *pinned,
        }
    }
    validate_document(&next)?;
    Ok(next)
}

#[cfg(test)]
mod tests;
