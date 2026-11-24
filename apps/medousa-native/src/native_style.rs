//! App-owned presentation tokens. RNDR still owns controls, focus and interaction.
//! Opaque surfaces deliberately work without platform blur or window materials.
use rndr_core::*;
use rndr_ui::{Token, TypeMetrics, TypeRamp, Typography, UiColor, UiStyle, UiTheme};

pub const COLUMN: f32 = 720.;
pub const INSET: f32 = 20.;
pub const CONTROL_RADIUS: f32 = 6.;
pub const ACTION_SIZE: f32 = 32.;
/// Action icon glyph size: a step under the rail's 20pt, so toolbar and
/// sidebar actions read as the same family as navigation.
pub const ACTION_ICON: f32 = 18.;
/// Sidebar list rows and tracked group headings, as in medousa-home.
pub const ROW_HEIGHT: f32 = 32.;
pub const SECTION_HEIGHT: f32 = 26.;

/// medousa-home's type grammar: 13pt chrome rows, 12/11pt metadata, tracked
/// 11pt group headings, 15pt reading and 13pt code. Scales stay with RNDR.
pub fn ramp() -> TypeRamp {
    TypeRamp {
        label: TypeMetrics::new(13., 18., 400).tracking(-0.08),
        emphasis: TypeMetrics::new(13., 18., 600),
        secondary: TypeMetrics::new(12., 16., 400),
        caption: TypeMetrics::new(11., 14., 400),
        overline: TypeMetrics::new(11., 14., 600).tracking(0.6),
        reading: TypeMetrics::new(15., 24., 400),
        ..TypeRamp::default()
    }
}

/// Size an entity to its label's real measured text instead of guessed
/// character counts. `label` may be the entity itself or a descendant; the
/// padding and label insets between them are included. Re-measured every
/// layout so it follows text scale and ramp changes.
pub struct FitLabel {
    pub label: Entity,
    pub extra: f32,
    pub min: f32,
    pub max: f32,
}
impl Component for FitLabel {}

pub fn fit_labels(world: &mut World) {
    let fits = world
        .query::<FitLabel>()
        .iter()
        .map(|(e, f)| (*e, f.label, f.extra, f.min, f.max))
        .collect::<Vec<_>>();
    if fits.is_empty() {
        return;
    }
    let engine = text_engine(world);
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let scale = world
        .resource::<Viewport>()
        .map(|v| v.scale)
        .filter(|s| s.is_finite() && *s > 0.)
        .unwrap_or(1.);
    for (root, label, extra, min, max) in fits {
        let Some(text) = world.get::<Label>(label).map(|l| l.text.clone()) else {
            continue;
        };
        let style = world
            .get::<TextStyle>(label)
            .cloned()
            .unwrap_or_else(|| theme.typography(Typography::Label));
        let natural = engine.measure(&text, &style, &[], f32::INFINITY).width;
        let mut chrome = world.get::<LabelBox>(label).map_or(0., |b| b.inset_x * 2.);
        let mut e = label;
        while e != root {
            let Some(parent) = world.get::<Parent>(e).map(|p| p.entity) else {
                break;
            };
            chrome += world.get::<Layout>(parent).map_or(0., |l| l.padding * 2.);
            e = parent;
        }
        if label != root {
            chrome += world.get::<Layout>(label).map_or(0., |l| l.padding * 2.);
        }
        // A leading disclosure chevron shares the label's row.
        if let Some(face) = world.get::<Parent>(label).map(|p| p.entity) {
            for sibling in children_of(world, face) {
                if world.has::<DisclosureChevron>(sibling) {
                    chrome += world.get::<Rect>(sibling).map_or(0., |r| r.w);
                }
            }
        }
        // Fractional advances can lose a device pixel at either edge after
        // nested snapping; reserve two, as RNDR's toolbar fit does.
        let width = ((natural + chrome + extra) * scale).ceil() / scale + 2. / scale;
        if let Some(layout) = world.get_mut::<Layout>(root) {
            layout.width = Size::Px(width.clamp(min, max.max(min)));
        }
    }
}

/// A path that keeps its last segments when space runs out: leading
/// segments collapse into "…" rather than eliding the file name at the end.
/// Fitted against the label's own laid-out width each layout pass.
pub struct PathCrumb(pub Vec<String>);
impl Component for PathCrumb {}

pub fn path_crumbs(world: &mut World) {
    let crumbs = world
        .query::<PathCrumb>()
        .iter()
        .map(|(e, c)| (*e, c.0.clone()))
        .collect::<Vec<_>>();
    if crumbs.is_empty() {
        return;
    }
    let engine = text_engine(world);
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    for (e, parts) in crumbs {
        let Some(rect) = world.get::<Rect>(e).copied() else {
            continue;
        };
        let style = world
            .get::<TextStyle>(e)
            .cloned()
            .unwrap_or_else(|| theme.typography(Typography::Label));
        let inset = world.get::<LabelBox>(e).map_or(0., |b| b.inset_x * 2.);
        let available = rect.w - inset;
        let join = |from: usize| {
            let tail = parts[from..].join("  ›  ");
            if from > 0 {
                format!("…  ›  {tail}")
            } else {
                tail
            }
        };
        let mut text = join(parts.len().saturating_sub(1));
        for from in 0..parts.len() {
            let candidate = join(from);
            if engine.measure(&candidate, &style, &[], f32::INFINITY).width <= available {
                text = candidate;
                break;
            }
        }
        if let Some(label) = world.get_mut::<Label>(e)
            && label.text != text
        {
            label.text = text;
        }
    }
}

/// Group heading recipe on an existing row or text label: sentence case in
/// the third text tier, so grey carries the hierarchy rather than caps and
/// weight. RNDR controls republish labels from props, so callers pass text.
pub fn section_heading(world: &mut World, label: Entity) {
    // An app UiStyle outranks the control's per-frame recipe.
    world.insert(
        label,
        UiStyle {
            foreground: Some(Token::Tertiary.into()),
            typography: Some(Typography::Secondary),
            ..Default::default()
        },
    );
}

/// medousa-home row grammar: title fills, metadata is a trailing caption on
/// the same line rather than a second line clipped by the row height.
pub fn trailing_meta(world: &mut World, h: rndr_ui::controls::ControlHandle) {
    let Some(face) = world.get::<Parent>(h.label).map(|p| p.entity) else {
        return;
    };
    if let Some(l) = world.get_mut::<Layout>(face) {
        l.axis = Axis::Row;
    }
    world.insert(
        face,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    for child in children_of(world, face) {
        // Skip the label, a leading disclosure chevron and RNDR's pinned
        // selection bar.
        if child == h.label || world.has::<Absolute>(child) || world.has::<DisclosureChevron>(child)
        {
            continue;
        }
        if let Some(l) = world.get_mut::<Layout>(child) {
            l.width = Size::Hug;
        }
        // Measured, so short counts like "95 turns" never ellipsize while
        // the title keeps the remaining width.
        if world.has::<Label>(child) {
            world.insert(
                child,
                FitLabel {
                    label: child,
                    extra: 0.,
                    min: 0.,
                    max: 120.,
                },
            );
        }
        world.insert(
            child,
            UiStyle {
                typography: Some(Typography::Caption),
                ..Default::default()
            },
        );
    }
}

/// One tooltip recipe for app-owned icon actions. RNDR owns visibility, Escape,
/// focus and pointer-leave suppression; the app only supplies readable geometry.
pub fn action_hint(world: &mut World, target: Entity, label: &str) -> Entity {
    let tip = rndr_ui::inputs::Tooltip::attach(world, target, label).unwrap();
    world.insert(tip.root, PopupPlacement::Below);
    world.insert(
        tip.root,
        FitLabel {
            label: tip.root,
            extra: 0.,
            min: 0.,
            max: 360.,
        },
    );
    world.get_mut::<Label>(tip.root).unwrap().overflow = TextOverflow::Wrap;
    tip.root
}
pub const COMPOSER_RADIUS: f32 = 14.;

pub fn canvas() -> Token {
    Token::Content
}
pub fn navigation() -> Token {
    Token::Navigation
}
/// The rail shares the navigation plane, as in medousa-home's shell.
pub fn rail() -> Token {
    Token::Navigation
}
pub fn raised() -> Token {
    Token::Raised
}
pub fn stroke() -> Token {
    Token::SubtleStroke
}
pub fn text() -> Token {
    Token::Text
}
pub fn secondary() -> Token {
    Token::Secondary
}
/// User bubbles are a quiet wash, not the selection color.
pub fn user() -> Token {
    Token::Raised
}

pub fn color(world: &World, token: Token) -> Color {
    match world.resource::<UiTheme>() {
        Some(theme) => theme.color(token),
        None => UiTheme::default().color(token),
    }
}

/// Reapply the explicit RNDR bridge only when effective settings change.
/// Preserve independent UI/reading scales, contrast and font choices. RNDR
/// currently reports light/dark and reduced motion, not OS text size/contrast.
pub fn sync_appearance(world: &mut World) {
    let current = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let effective = current.clone().follow_system(&system_appearance(world));
    let previous = world.resource::<AppliedTheme>().map(|p| &p.0);
    if previous != Some(&effective) {
        rndr_ui::apply_theme_bridge(world, effective.clone());
        // Markdown is embedded in app-owned reading planes and user bubbles.
        world.resource_mut::<MarkdownTheme>().unwrap().surface = Color::rgba(0., 0., 0., 0.);
        crate::code::refresh_appearance(world);
        crate::diff_presentation::refresh_appearance(world);
        world.insert_resource(AppliedTheme(effective));
    }
}
struct AppliedTheme(UiTheme);

/// Reading zoom uses the existing keymap; it never edits document content or
/// steals the editor's undo stack. Keep chrome and reading scales independent.
pub fn install_zoom(world: &mut World) {
    let keymap = world.resource_mut::<Keymap>().unwrap();
    for (action, key) in [
        ("reading.zoom-in", '='),
        ("reading.zoom-in", '+'),
        ("reading.zoom-out", '-'),
        ("reading.zoom-reset", '0'),
    ] {
        keymap.bind(KeyBinding::new(
            action,
            [Keystroke::character(key).primary()],
        ));
    }
    keymap.bind(KeyBinding::new(
        "reading.zoom-in",
        [Keystroke::character('+').primary().shift()],
    ));
}

pub fn reading_zoom(world: &mut World) {
    let actions = world
        .resource::<KeymapFired>()
        .map(|k| k.actions.clone())
        .unwrap_or_default();
    let Some(theme) = world.resource_mut::<UiTheme>() else {
        return;
    };
    for action in actions {
        match action.as_str() {
            "reading.zoom-in" => theme.reading_scale = (theme.reading_scale + 0.1).min(2.),
            "reading.zoom-out" => theme.reading_scale = (theme.reading_scale - 0.1).max(0.8),
            "reading.zoom-reset" => theme.reading_scale = 1.,
            _ => {}
        }
    }
}

pub struct SemanticBorder(pub Token);
impl Component for SemanticBorder {}

/// Border is a core primitive, so keep app-owned decorative borders in sync.
/// Never attach to RNDR focus rings: those belong to its interaction system.
pub fn borders(world: &mut World) {
    let borders = world
        .query::<SemanticBorder>()
        .iter()
        .map(|(e, b)| (*e, b.0))
        .collect::<Vec<_>>();
    for (e, token) in borders {
        let color = color(world, token);
        if let Some(border) = world.get_mut::<Border>(e) {
            border.color = color;
        }
    }
}

/// Interaction-scale ease, matching medousa-home's 120–160ms transitions.
pub const EASE_SECONDS: f32 = 0.15;

/// Presented value of `prop` before the motion runtime owns it.
fn resting(world: &World, entity: Entity, prop: motion::Prop) -> f32 {
    if let Some(target) = motion::target(world, entity, prop) {
        return target;
    }
    let transform = world.get::<Transform>(entity).copied().unwrap_or_default();
    match prop {
        motion::Prop::Opacity => world.get::<Opacity>(entity).map_or(1., |o| o.0),
        motion::Prop::Rotate => transform.rotate,
        motion::Prop::X => transform.translate[0],
        motion::Prop::Y => transform.translate[1],
        _ => f32::NAN,
    }
}

/// Ease `prop` toward `target` while a clock drives frames. Headless steps and
/// offscreen renders have none, so the value settles at once and reads exactly.
pub fn ease(world: &mut World, entity: Entity, prop: motion::Prop, target: f32) {
    if resting(world, entity, prop) == target {
        return;
    }
    if world.resource::<Time>().is_some_and(|t| !t.delta.is_zero()) {
        let curve = motion::Curve::tween(EASE_SECONDS, motion::Easing::EaseOut);
        motion::animate(world, entity, prop, target, curve);
        return;
    }
    if motion::target(world, entity, prop).is_some() {
        motion::set(world, entity, prop, target);
    }
    // The runtime samples at the end of the frame; present the value now too.
    match prop {
        motion::Prop::Opacity => world.insert(entity, Opacity(target)),
        motion::Prop::Rotate | motion::Prop::X | motion::Prop::Y => {
            let mut transform = world.get::<Transform>(entity).copied().unwrap_or_default();
            match prop {
                motion::Prop::Rotate => transform.rotate = target,
                motion::Prop::X => transform.translate[0] = target,
                _ => transform.translate[1] = target,
            }
            world.insert(entity, transform);
        }
        _ => {}
    }
}

/// A disclosure chevron owned by a retained control.
pub struct Disclosure {
    chevron: Entity,
    open: bool,
}
impl Component for Disclosure {}
struct DisclosureChevron;
impl Component for DisclosureChevron {}

/// One right-pointing chevron leading `face`, turned a quarter when `open`.
/// It is decorative: the owning control keeps the name and expanded state.
pub fn disclosure(world: &mut World, owner: Entity, face: Entity, open: bool) {
    let angle = if open {
        std::f32::consts::FRAC_PI_2
    } else {
        0.
    };
    if let Some(d) = world.get::<Disclosure>(owner)
        && world.is_alive(d.chevron)
    {
        let (chevron, was) = (d.chevron, d.open);
        if was != open {
            world.insert(owner, Disclosure { chevron, open });
            ease(world, chevron, motion::Prop::Rotate, angle);
        }
        return;
    }
    let chevron = world.spawn((
        Layout::column(),
        Label::new(Icon::ChevronRight.scalar().to_string()),
        LabelBox::CONTENT,
        UiStyle {
            foreground: Some(secondary().into()),
            typography: Some(Typography::Caption),
            ..Default::default()
        },
        Transform {
            rotate: angle,
            ..Default::default()
        },
        DisclosureChevron,
    ));
    set_parent(world, chevron, face);
    if let Some(children) = world.get_mut::<Children>(face) {
        children.list.retain(|e| *e != chevron);
        children.list.insert(0, chevron);
    }
    if let Some(l) = world.get_mut::<Layout>(face) {
        l.axis = Axis::Row;
    }
    world.insert(
        face,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    world.insert(owner, Disclosure { chevron, open });
}

/// The face holding a control's label: where a leading chevron belongs.
pub fn label_face(world: &World, root: Entity) -> Option<Entity> {
    let mut pending = vec![root];
    while let Some(e) = pending.pop() {
        if e != root && world.has::<Label>(e) && !world.has::<DisclosureChevron>(e) {
            return world.get::<Parent>(e).map(|p| p.entity);
        }
        pending.extend(children_of(world, e).into_iter().rev());
    }
    None
}

/// A scrollbar appears only when its viewport overflows, or while the user
/// operates it. The native target, focus route and drag gesture are unchanged.
/// Last seen scroll offset of a scrollbar's target and when it last moved.
struct ScrollWake {
    offset: [f32; 2],
    at: Option<std::time::Duration>,
}
impl Component for ScrollWake {}
/// How long a scrollbar lingers after its content stops moving.
const SCROLL_LINGER: std::time::Duration = std::time::Duration::from_millis(1200);

/// An overflowing scrollbar shows while the pointer is over its viewport, for
/// a moment after the content scrolls, or while focused or dragged; at rest it
/// fades out so a long conversation doesn't carry a line down its edge.
pub fn scrollbar_visibility(world: &mut World, bar: Entity, overflows: bool) {
    let active = world.resource::<Focus>().and_then(|f| f.entity) == Some(bar)
        || scrollbar_dragging(world, bar);
    let target = world.get::<Scrollbar>(bar).map(|s| s.target);
    let offset = target
        .and_then(|t| world.get::<Scroll>(t))
        .map_or([0., 0.], |s| s.offset);
    let now = world.resource::<Time>().map(|t| t.now).unwrap_or_default();
    let at = match world.get::<ScrollWake>(bar) {
        Some(wake) if wake.offset != offset => Some(now),
        Some(wake) => wake.at,
        None => None,
    };
    world.insert(bar, ScrollWake { offset, at });
    let recent = at.is_some_and(|at| now.saturating_sub(at) < SCROLL_LINGER);
    let pointer = world.resource::<Input>().map(|i| i.pointer.position);
    let hovered = [Some(bar), target].into_iter().flatten().any(|e| {
        world.get::<Rect>(e).is_some_and(|r| {
            pointer.is_some_and(|[x, y]| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
        })
    });
    ease(
        world,
        bar,
        motion::Prop::Opacity,
        if active || (overflows && (hovered || recent)) {
            1.
        } else {
            0.
        },
    );
}

/// App-wide resting chrome: split lines are hairlines and scrollbars rest
/// hidden until the pointer, a scroll or focus wakes them. Hover, focus and drag keep RNDR's own ink.
pub fn quiet_chrome(world: &mut World) {
    for bar in world.entities_with::<Scrollbar>() {
        // RNDR's shared geometry covers both Scroll viewports and editors.
        if let Some(geometry) = scrollbar_geometry(world, bar) {
            scrollbar_visibility(world, bar, geometry.max > 1.);
        }
    }
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let drag = world.resource::<Drag>().and_then(|d| d.entity);
    let pointer = world.resource::<Input>().map(|i| i.pointer.position);
    let line = color(world, stroke());
    for sash in world.entities_with::<Splitter>() {
        let Some(rect) = world.get::<Rect>(sash).copied() else {
            continue;
        };
        let hovered = pointer.is_some_and(|[x, y]| {
            x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h
        });
        if focus == Some(sash) || drag == Some(sash) || hovered {
            continue;
        }
        let vertical = rect.h >= rect.w;
        let span = if vertical { rect.w } else { rect.h };
        let inset = ((span - 1.) * 0.5).max(0.);
        for child in children_of(world, sash) {
            world.insert(child, line);
            world.insert(
                child,
                if vertical {
                    Absolute {
                        left: Some(inset),
                        right: Some(inset),
                        top: Some(0.),
                        bottom: Some(0.),
                    }
                } else {
                    Absolute {
                        left: Some(0.),
                        right: Some(0.),
                        top: Some(inset),
                        bottom: Some(inset),
                    }
                },
            );
        }
    }
}

/// Same horizontal measure for the message lane and composer, including splits.
pub fn column_width(available: f32) -> f32 {
    (available - 2. * INSET).clamp(200., COLUMN)
}

pub fn plane(world: &mut World, entity: Entity, color: Token) {
    world.insert(
        entity,
        UiStyle {
            background: Some(UiColor::Token(color)),
            ..Default::default()
        },
    );
}
/// Passive frames only. Never flatten an operating control's focus/hover layers.
pub fn flat(world: &mut World, entity: Entity, color: Token) {
    plane(world, entity, color);
    world.insert(entity, Radius(0.));
}

pub struct DocumentSidebar;
pub struct LibraryBreadcrumbs;
impl Component for LibraryBreadcrumbs {}
impl Component for DocumentSidebar {}
pub struct ReadingDocument;
impl Component for ReadingDocument {}

pub fn document_layouts(world: &mut World) {
    let width = world.resource::<Viewport>().map_or(1200., |v| v.width);
    for entity in world
        .query::<DocumentSidebar>()
        .iter()
        .map(|(e, _)| *e)
        .collect::<Vec<_>>()
    {
        // The notes rail fits three independently actionable labels plus two icons.
        let rail = if world.has::<LibraryBreadcrumbs>(entity) {
            (width * 0.37).clamp(232., 344.)
        } else if width < 960. {
            232.
        } else {
            288.
        };
        world.get_mut::<Layout>(entity).unwrap().width = Size::Px(rail);
    }
    for entity in world
        .query::<ReadingDocument>()
        .iter()
        .map(|(e, _)| *e)
        .collect::<Vec<_>>()
    {
        let available = world
            .get::<Parent>(entity)
            .and_then(|p| world.get::<Rect>(p.entity))
            .map_or(800., |r| r.w);
        // MarkdownView's virtualized block measurement uses its native 16pt
        // gutter. Bound the view itself, not its internal padding, so heading
        // wrapping and retained block heights agree at every window width.
        world.get_mut::<Layout>(entity).unwrap().width =
            Size::Px((available - 24.).clamp(120., COLUMN + 32.));
        world.insert(entity, AlignSelf(Items::Center));
    }
}

/// Document navigation matches chat's quiet sidebar without nested cards.
pub fn navigation_frame(world: &mut World, entity: Entity) {
    flat(world, entity, navigation());
    world.insert(entity, DocumentSidebar);
    world.insert(entity, ResolvedColor);
    world.get_mut::<Layout>(entity).unwrap().padding = 8.;
}

pub fn document_action(
    world: &mut World,
    parent: Entity,
    id: u64,
    label: &str,
    enabled: bool,
) -> rndr_ui::controls::ControlHandle {
    use rndr_ui::{
        UiId,
        controls::{Button, ButtonProps, ButtonVariant, HostIntent},
    };
    if let Some(icon) = match label {
        "Refresh" | "Check latest" => Some(Icon::RefreshCw),
        _ => None,
    } {
        return icon_action(world, parent, id, icon, label, enabled);
    }
    let mut props = ButtonProps::new(label, HostIntent::Command(id));
    props.enabled = enabled;
    props.variant = ButtonVariant::Quiet;
    let h = Button::mount(world, parent, UiId(id), props).unwrap();
    *world.get_mut::<Layout>(h.root).unwrap() = Layout {
        height: Size::Px(ACTION_SIZE),
        ..Layout::row()
    };
    // medousa-home's `px-2` text actions: 8pt either side of the label.
    world.insert(
        h.root,
        FitLabel {
            label: h.label,
            extra: 4.,
            min: 48.,
            max: 300.,
        },
    );
    world.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
    h
}

/// Square quiet icon action with a hint. `label` is its accessible name.
pub fn icon_action(
    world: &mut World,
    parent: Entity,
    id: u64,
    icon: Icon,
    label: &str,
    enabled: bool,
) -> rndr_ui::controls::ControlHandle {
    use rndr_ui::{
        UiId,
        controls::{ButtonProps, ButtonVariant, HostIntent, IconButton},
    };
    let mut props = ButtonProps::new(label, HostIntent::Command(id));
    props.enabled = enabled;
    props.variant = ButtonVariant::Quiet;
    let h = IconButton::icon(world, parent, UiId(id), icon, label, props).unwrap();
    world.insert(h.label, LabelBox::CONTENT);
    action_hint(world, h.root, label);
    *world.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Px(ACTION_SIZE),
        height: Size::Px(ACTION_SIZE),
        ..Layout::row()
    };
    h
}

/// One-point rule between stacked regions; Border is all-sided in RNDR.
pub fn rule(world: &mut World, parent: Entity) -> Entity {
    let e = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    set_parent(world, e, parent);
    plane(world, e, stroke());
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appearance_updates_existing_surfaces_and_preserves_settings() {
        let mut app = crate::ui::app();
        app.update();
        let w = app.world_mut();
        let e = w.spawn((Layout::column(),));
        plane(w, e, canvas());
        w.insert(
            e,
            Border {
                color: Color::rgb(1., 0., 0.),
                width: 1.,
            },
        );
        w.insert(e, SemanticBorder(stroke()));
        let mut settings = w.resource::<UiTheme>().unwrap().clone();
        settings.text_scale = 1.25;
        settings.reading_scale = 1.5;
        settings.contrast = rndr_ui::Contrast::High;
        w.insert_resource(settings);
        for dark in [true, false, true] {
            publish_system_appearance(
                w,
                SystemAppearance {
                    dark: Some(dark),
                    reduced_motion: true,
                    ..Default::default()
                },
            );
            sync_appearance(w);
            borders(w);
            rndr_ui::resolve_styles(w);
            let theme = w.resource::<UiTheme>().unwrap();
            assert_eq!(theme.appearance == rndr_ui::Appearance::Dark, dark);
            assert_eq!(theme.text_scale, 1.25);
            assert_eq!(theme.reading_scale, 1.5);
            assert_eq!(theme.contrast, rndr_ui::Contrast::High);
            assert_eq!(w.get::<Color>(e), Some(&theme.color(canvas())));
            assert_eq!(w.get::<Border>(e).unwrap().color, theme.color(stroke()));
            assert_eq!(w.resource::<Theme>().unwrap().text, theme.color(text()));
            assert!(system_appearance(w).reduced_motion);
        }
        // A platform with no preference does not force an arbitrary mode.
        publish_system_appearance(w, SystemAppearance::default());
        sync_appearance(w);
        assert_eq!(
            w.resource::<UiTheme>().unwrap().appearance,
            rndr_ui::Appearance::Dark
        );
    }

    #[test]
    fn reading_zoom_is_bounded_independent_and_reversible() {
        let mut app = crate::ui::app();
        app.update();
        let w = app.world_mut();
        w.resource_mut::<UiTheme>().unwrap().text_scale = 1.25;
        for (action, expected) in [
            ("reading.zoom-in", 2.),
            ("reading.zoom-out", 0.8),
            ("reading.zoom-reset", 1.),
        ] {
            for _ in 0..30 {
                w.insert_resource(KeymapFired {
                    actions: vec![action.into()],
                });
                reading_zoom(w);
            }
            sync_appearance(w);
            let theme = w.resource::<UiTheme>().unwrap();
            assert!((theme.reading_scale - expected).abs() < 0.001);
            assert_eq!(theme.text_scale, 1.25);
        }
    }

    #[test]
    fn shared_column_is_bounded_and_responsive() {
        assert_eq!(column_width(1200.), COLUMN);
        assert_eq!(column_width(440.), 400.);
        assert_eq!(column_width(220.), 200.);
    }
}

/// Size every icon-only label as an action icon. Controls mount glyphs at
/// body-text size; one late pass keeps every view's actions consistent.
pub fn action_icons(world: &mut World) {
    let icons: Vec<Entity> = world
        .query::<Label>()
        .iter()
        .filter(|(_, label)| {
            let mut chars = label.text.chars();
            matches!((chars.next(), chars.next()), (Some(ch), None) if rndr_core::is_icon_scalar(ch))
        })
        .map(|(e, _)| *e)
        .collect();
    for entity in icons {
        // A glyph is never elided; it centres in its face without text insets.
        if let Some(label) = world.get_mut::<Label>(entity) {
            if label.overflow != TextOverflow::Visible {
                label.overflow = TextOverflow::Visible;
            }
        }
        let flush = LabelBox {
            inset_x: 0.,
            inset_y: 0.,
            min_height: 0.,
        };
        if world.get::<LabelBox>(entity) != Some(&flush) {
            world.insert(entity, flush);
        }
        // A fixed-size button: its inner layers fill it and centre the glyph,
        // so the hover fill and the icon share one centre.
        let mut node = world.get::<Parent>(entity).map(|p| p.entity);
        let mut layers = Vec::new();
        let mut fixed = false;
        while let Some(e) = node {
            if world.has::<rndr_core::Button>(e) {
                // Compact icon buttons become the standard square, so every
                // view's hover fill has one shape and margin.
                if let Some(layout) = world.get_mut::<Layout>(e) {
                    let square = Size::Px(ACTION_SIZE);
                    if matches!(layout.width, Size::Px(w) if (24. ..=ACTION_SIZE).contains(&w))
                        && !matches!(layout.height, Size::Px(h) if h > ACTION_SIZE)
                        && (layout.width != square || layout.height != square)
                    {
                        layout.width = Size::Px(ACTION_SIZE);
                        layout.height = Size::Px(ACTION_SIZE);
                    }
                }
                fixed = world.get::<Layout>(e).is_some_and(|l| {
                    matches!(l.height, Size::Px(_)) && matches!(l.width, Size::Px(_))
                });
                if fixed {
                    world.insert(e, Align::center());
                    if let Some(layout) = world.get_mut::<Layout>(e) {
                        if layout.padding != 0. {
                            layout.padding = 0.;
                        }
                    }
                }
                break;
            }
            layers.push(e);
            node = world.get::<Parent>(e).map(|p| p.entity);
        }
        if fixed {
            for e in layers {
                if let Some(layout) = world.get_mut::<Layout>(e) {
                    if layout.width != Size::Fill || layout.height != Size::Fill {
                        layout.width = Size::Fill;
                        layout.height = Size::Fill;
                    }
                }
                if world.get::<Align>(e) != Some(&Align::center()) {
                    world.insert(e, Align::center());
                }
            }
        }
        // One em square: some glyphs measure wider than they draw, and the
        // sprite is drawn from the label's leading edge. Its face centres it.
        if let Some(layout) = world.get_mut::<Layout>(entity) {
            let em = Size::Px(ACTION_ICON);
            if layout.width != em || layout.height != em {
                layout.width = em;
                layout.height = em;
            }
        }
        if let Some(face) = world.get::<Parent>(entity).map(|p| p.entity) {
            if world.get::<Align>(face) != Some(&Align::center()) {
                world.insert(face, Align::center());
            }
        }
        // Centred on a one-em line, as the rail's icons are: the glyph sits in
        // the middle of its face and the face stays square, not text-tall.
        let fit = |style: &mut TextStyle| {
            style.size = ACTION_ICON;
            style.align = TextAlign::Center;
            style.line_height = Some(1.);
        };
        match world.get_mut::<TextStyle>(entity) {
            Some(style)
                if style.size != ACTION_ICON
                    || style.align != TextAlign::Center
                    || style.line_height != Some(1.) =>
            {
                fit(style)
            }
            Some(_) => {}
            None => {
                let mut style = world
                    .resource::<rndr_ui::UiTheme>()
                    .cloned()
                    .unwrap_or_default()
                    .typography(rndr_ui::Typography::Label);
                fit(&mut style);
                world.insert(entity, style);
            }
        }
    }
}
