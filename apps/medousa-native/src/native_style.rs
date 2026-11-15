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
