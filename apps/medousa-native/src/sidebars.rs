//! Destination sidebars share chat's mechanics: collapse from the band's
//! panel toggle, drag the divider to resize, and both are remembered with
//! the window layout. Each sidebar is found by its marker every frame, so a
//! surface that remounts (Notes, Code on a source change) keeps its sidebar.
use crate::native_style as visual;
use crate::surfaces::SurfaceId;
use crate::workspace::{SIDE_MAX, SIDE_MIN, Side, Workspace};
use rndr_core::*;

/// Destinations with a sidebar, in `Preferences::sides` order.
const SURFACES: [SurfaceId; 7] = [
    SurfaceId::Notes,
    SurfaceId::Code,
    SurfaceId::Create,
    SurfaceId::Reviews,
    SurfaceId::Calendar,
    SurfaceId::Peers,
    SurfaceId::Settings,
];
/// Content keeps at least this much room beside a widened sidebar.
const CONTENT_MIN: f32 = 360.;
const SASH: f32 = 6.;
/// Below this window width a sidebar steps down, as destinations always did,
/// so toolbars beside it keep their actions.
const NARROW: f32 = 960.;
const NARROW_SIDE: f32 = 232.;

/// The divider after a destination's sidebar.
struct Sash {
    surface: usize,
}
impl Component for Sash {}
/// Its hairline, drawn at the divider's centre like chat's.
struct Hairline;
impl Component for Hairline {}

fn slot(surface: SurfaceId) -> Option<usize> {
    SURFACES.iter().position(|s| *s == surface)
}

/// The sidebar and the entity that sits in its row: the sidebar, or the
/// wrapper that holds a scrolling sidebar below the band.
fn find(world: &World, root: Entity) -> Option<(Entity, Entity)> {
    let mut stack = vec![(root, 0)];
    while let Some((e, depth)) = stack.pop() {
        if world.has::<visual::DocumentSidebar>(e) || world.has::<visual::SidebarPane>(e) {
            let parent = world.get::<Parent>(e)?.entity;
            let wrapped = world
                .get::<Layout>(parent)
                .is_some_and(|l| l.axis == Axis::Column)
                && children_of(world, parent).len() == 1;
            return Some((e, if wrapped { parent } else { e }));
        }
        if depth < 4 {
            stack.extend(children_of(world, e).into_iter().map(|c| (c, depth + 1)));
        }
    }
    None
}

/// Whether the active destination has a sidebar to toggle.
pub fn available(world: &World) -> bool {
    let active = crate::surfaces::active(world);
    slot(active).is_some()
        && crate::surfaces::root(world, active).is_some_and(|root| find(world, root).is_some())
}

/// Collapse or expand the active destination's sidebar.
pub fn toggle(world: &mut World) {
    let Some(i) = slot(crate::surfaces::active(world)) else {
        return;
    };
    if let Some(state) = world.resource_mut::<Workspace>() {
        state.prefs.sides[i].open = !state.prefs.sides[i].open;
    }
    crate::workspace::save(world);
}

fn side(world: &World, i: usize) -> Side {
    world
        .resource::<Workspace>()
        .map(|w| w.prefs.sides[i])
        .unwrap_or_default()
}

/// Mount each sidebar's divider, apply its width and visibility, and record
/// a width the user dragged to.
pub fn sync(world: &mut World) {
    let window = world.resource::<Viewport>().map_or(1200., |v| v.width);
    let rail = crate::surfaces::RAIL_WIDTH;
    let stroke = visual::color(world, visual::stroke());
    let plane = visual::color(world, visual::navigation());
    let mut changed = false;
    for (i, surface) in SURFACES.into_iter().enumerate() {
        let Some(root) = crate::surfaces::root(world, surface) else {
            continue;
        };
        let Some((sidebar, slot)) = find(world, root) else {
            continue;
        };
        let Some(row) = world.get::<Parent>(slot).map(|p| p.entity) else {
            continue;
        };
        let sash = ensure_sash(world, row, slot, i);
        let mut state = side(world, i);
        // A drag writes the slot's width; adopt it as the remembered width.
        if let Some(Size::Px(w)) = world.get::<Layout>(slot).map(|l| l.width) {
            let dragged = (w - state.width).abs() > 0.5 && world.has::<Rect>(sash);
            if dragged && slot_width_was_dragged(world, sash) {
                state.width = w.clamp(SIDE_MIN, SIDE_MAX);
                if let Some(ws) = world.resource_mut::<Workspace>() {
                    ws.prefs.sides[i].width = state.width;
                }
                changed = true;
            }
        }
        // Leave the content its minimum in a narrow window, without
        // forgetting the width the user chose.
        let fit = state
            .width
            .min(window - rail - SASH - CONTENT_MIN)
            .min(if window < NARROW {
                NARROW_SIDE
            } else {
                SIDE_MAX
            })
            .max(SIDE_MIN);
        for e in [sidebar, slot] {
            if let Some(layout) = world.get_mut::<Layout>(e) {
                if layout.width != Size::Px(fit) {
                    layout.width = Size::Px(fit);
                }
            }
        }
        // Hide the sidebar itself: a scrolling sidebar's wrapper mirrors it.
        crate::workspace::reveal(world, sidebar, state.open, -12.);
        if state.open {
            world.remove::<Hidden>(sash);
        } else {
            world.insert(sash, Hidden);
        }
        // Quiet divider, as chat's: the plane with a centred hairline.
        if world.resource::<Drag>().and_then(|d| d.entity) != Some(sash) {
            world.insert(sash, plane);
        }
        for line in children_of(world, sash) {
            if world.has::<Hairline>(line) {
                world.insert(line, stroke);
            }
        }
    }
    if changed {
        crate::workspace::save(world);
    }
}

/// Only a width the divider produced counts as the user's choice; the
/// width this module writes itself is not a drag.
fn slot_width_was_dragged(world: &World, sash: Entity) -> bool {
    world.resource::<Drag>().and_then(|d| d.entity) == Some(sash)
}

fn ensure_sash(world: &mut World, row: Entity, slot: Entity, surface: usize) -> Entity {
    let siblings = children_of(world, row);
    let at = siblings.iter().position(|e| *e == slot).unwrap_or(0);
    if let Some(next) = siblings.get(at + 1).copied() {
        if world
            .get::<Sash>(next)
            .is_some_and(|s| s.surface == surface)
        {
            return next;
        }
    }
    // A stale divider (its sidebar remounted) goes with its old position.
    for e in siblings {
        if world.has::<Sash>(e) {
            despawn(world, e);
        }
    }
    let (mut bar, mut spec) = splitter();
    spec.min_leading = SIDE_MIN;
    spec.min_trailing = CONTENT_MIN;
    bar.width = Size::Px(SASH);
    bar.height = Size::Fill;
    let sash = world.spawn((
        bar,
        spec,
        Sash { surface },
        Align {
            items: Items::Center,
            justify: Justify::Center,
        },
        Semantics {
            name: Some("Sidebar width".into()),
            ..Default::default()
        },
        ResolvedColor,
    ));
    let line = world.spawn((
        Layout {
            width: Size::Px(1.),
            height: Size::Fill,
            ..Layout::column()
        },
        Hairline,
        ResolvedColor,
    ));
    set_parent(world, line, sash);
    // Seat the divider right after the sidebar.
    world.insert(sash, Parent { entity: row });
    if let Some(children) = world.get_mut::<Children>(row) {
        let at = children.list.iter().position(|e| *e == slot).unwrap_or(0);
        children.list.insert(at + 1, sash);
    }
    sash
}
