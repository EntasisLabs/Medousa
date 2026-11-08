//! Semantic styling on the existing core input tree; no parallel dispatcher.
use rndr_core::*;
use rndr_ui::{Appearance, Corner, Token, Typography, UiStyle, UiTheme};

pub fn theme(world: &World) -> UiTheme {
    world.resource::<UiTheme>().cloned().unwrap_or(UiTheme {
        appearance: Appearance::Dark,
        ..Default::default()
    })
}

pub fn apply_theme(world: &mut World, theme: UiTheme) {
    rndr_ui::apply_theme_bridge(world, theme);
    // Reading messages are on the canvas, not opaque document cards. Retain
    // theme-aware code/link/highlight colors without filling every paragraph.
    world.resource_mut::<MarkdownTheme>().unwrap().surface = Color::rgba(0., 0., 0., 0.);
    let color = self::theme(world).color(Token::SubtleStroke);
    for e in world.entities_with::<ThemedOutline>() {
        world.insert(e, Border { color, width: 1. });
    }
}

struct ThemedOutline;
impl Component for ThemedOutline {}

pub fn outline(world: &mut World, entity: Entity) {
    world.insert(entity, ThemedOutline);
    world.insert(
        entity,
        Border {
            color: theme(world).color(Token::SubtleStroke),
            width: 1.,
        },
    );
}

pub fn surface(world: &mut World, entity: Entity, token: Token, corner: Option<Corner>) {
    // Explicitly transfer these properties to UiStyle. Authored overrides made
    // afterwards (e.g. a transparent quiet button) retain rndr's precedence.
    world.remove::<Color>(entity);
    world.remove::<Radius>(entity);
    world.insert(
        entity,
        UiStyle {
            background: Some(token.into()),
            corner,
            ..Default::default()
        },
    );
}

pub fn label(world: &mut World, parent: Entity, text: &str, heading: bool) -> Entity {
    let e = rndr_app::label(world, parent, text, heading);
    let role = if heading {
        Typography::Heading
    } else {
        Typography::Reading
    };
    world.insert(e, theme(world).typography(role));
    world.insert(
        e,
        UiStyle {
            foreground: Some(Token::Text.into()),
            ..Default::default()
        },
    );
    e
}

pub fn secondary(world: &mut World, entity: Entity) {
    world.get_mut::<Label>(entity).unwrap().color = None;
    world.get_mut::<UiStyle>(entity).unwrap().foreground = Some(Token::Secondary.into());
}

pub fn button(world: &mut World, parent: Entity, action: &str, text: &str) -> Entity {
    let e = rndr_app::button(world, parent, action, text);
    world.get_mut::<Label>(e).unwrap().color = None;
    surface(world, e, Token::Raised, Some(Corner::Control));
    world.get_mut::<UiStyle>(e).unwrap().foreground = Some(Token::Text.into());
    world.insert(e, theme(world).typography(Typography::Label));
    e
}

// Opaque application-local tokens, not strings interpreted as executable text.
pub const LIGHT: u64 = 0x4d01;
pub const DARK: u64 = 0x4d02;
pub const CONTRAST: u64 = 0x4d03;

pub fn appearance_controls(world: &mut World, parent: Entity) {
    use rndr_ui::controls::{Button, ButtonProps, ButtonVariant, HostIntent};
    for (id, label) in [
        (LIGHT, "Use light appearance"),
        (DARK, "Use dark appearance"),
        (CONTRAST, "Toggle high contrast"),
    ] {
        let mut props = ButtonProps::new(label, HostIntent::Command(id));
        props.variant = ButtonVariant::Quiet;
        props.description =
            "Local to this window; does not submit a prompt or change workshop defaults".into();
        Button::mount(world, parent, rndr_ui::UiId(id), props)
            .expect("live appearance controls parent and unique stable key");
    }
}

pub fn appearance_action(world: &World, intent: &ActionIntent) -> Option<&'static str> {
    use rndr_ui::controls::{HostIntent, host_intent};
    match host_intent(world, intent)? {
        HostIntent::Command(LIGHT) => Some("appearance:light"),
        HostIntent::Command(DARK) => Some("appearance:dark"),
        HostIntent::Command(CONTRAST) => Some("appearance:contrast"),
        _ => None,
    }
}

/// Same reading measure for transcript and composer, bounded at narrow widths.
/// Only the width is taken: Markdown owns its own padding and measurement.
pub fn reading_width(available: f32, theme: &UiTheme) -> f32 {
    match rndr_ui::reading_layout_at_measure(available, 75., theme).width {
        Size::Px(width) => width,
        _ => unreachable!("rndr reading layout resolves a point width"),
    }
}
