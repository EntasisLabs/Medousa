//! App-owned presentation tokens. RNDR still owns controls, focus and interaction.
//! Opaque surfaces deliberately work without platform blur or window materials.
use rndr_core::*;
use rndr_ui::{Token, UiColor, UiStyle, UiTheme};

pub const COLUMN: f32 = 720.;
pub const ROW_GAP: f32 = 24.;
pub const INSET: f32 = 20.;
pub const CONTROL_RADIUS: f32 = 6.;
pub const COMPOSER_RADIUS: f32 = 14.;

pub fn canvas() -> Color {
    UiTheme {
        appearance: rndr_ui::Appearance::Dark,
        ..Default::default()
    }
    .color(Token::Content)
}
pub fn navigation() -> Color {
    Color::rgb(0.075, 0.082, 0.098)
}
pub fn rail() -> Color {
    Color::rgb(0.060, 0.067, 0.080)
}
pub fn raised() -> Color {
    Color::rgb(0.155, 0.166, 0.190)
}
pub fn stroke() -> Color {
    Color::rgb(0.225, 0.240, 0.275)
}
pub fn text() -> Color {
    Color::rgb(0.93, 0.94, 0.96)
}
pub fn secondary() -> Color {
    Color::rgb(0.64, 0.68, 0.74)
}
pub fn user() -> Color {
    Color::rgb(0.20, 0.28, 0.43)
}

/// Same horizontal measure for the message lane and composer, including splits.
pub fn column_width(available: f32) -> f32 {
    (available - 2. * INSET).clamp(200., COLUMN)
}

pub fn plane(world: &mut World, entity: Entity, color: Color) {
    world.insert(
        entity,
        UiStyle {
            background: Some(UiColor::Explicit(color)),
            ..Default::default()
        },
    );
}
/// Passive frames only. Never flatten an operating control's focus/hover layers.
pub fn flat(world: &mut World, entity: Entity, color: Color) {
    plane(world, entity, color);
    world.insert(entity, Radius(0.));
}

pub struct DocumentSidebar;
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
        world.get_mut::<Layout>(entity).unwrap().width =
            Size::Px(if width < 960. { 232. } else { 288. });
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
        controls::{Button, ButtonProps, ButtonVariant, HostIntent, IconButton},
    };
    let mut props = ButtonProps::new(label, HostIntent::Command(id));
    props.enabled = enabled;
    props.variant = ButtonVariant::Quiet;
    let icon = match label {
        "Refresh" | "Check latest" => Some(Icon::RefreshCw),
        _ => None,
    };
    let h = if let Some(icon) = icon {
        let h = IconButton::icon(world, parent, UiId(id), icon, label, props).unwrap();
        world.insert(h.label, LabelBox::CONTENT);
        let tip = rndr_ui::inputs::Tooltip::attach(world, h.root, label).unwrap();
        world.insert(tip.root, PopupPlacement::Below);
        world.get_mut::<Layout>(tip.root).unwrap().width = Size::Px(180.);
        h
    } else {
        Button::mount(world, parent, UiId(id), props).unwrap()
    };
    let width = if icon.is_some() {
        30.
    } else {
        (label.chars().count() as f32 * 7. + 20.).clamp(56., 300.)
    };
    *world.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Px(width),
        height: Size::Px(30.),
        ..Layout::row()
    };
    world.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_column_is_bounded_and_responsive() {
        assert_eq!(column_width(1200.), COLUMN);
        assert_eq!(column_width(440.), 400.);
        assert_eq!(column_width(220.), 200.);
    }
}
