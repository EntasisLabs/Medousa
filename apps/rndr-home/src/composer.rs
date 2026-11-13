//! Prompt input boundary and synchronous draft reset adapter.
use crate::presentation::{self, button};
use crate::shell::{muted, row};
use rndr_app::child;
use rndr_core::*;
use rndr_ui::Typography;

#[derive(Default)]
pub(super) struct SubmitBoundary {
    pub(super) safe: bool,
}

// rndr's reset_field_draft requires a transactional TextField opt-in. Keep
// that marker only for this synchronous reset: leaving it on a Prompt would
// route Enter/Shift+Enter through the single-line transaction dispatcher.
pub(super) fn reset_prompt(world: &mut World, entity: Entity, text: &str) {
    world.insert(entity, FieldTransactions(|_, _, _| {}));
    assert!(reset_field_draft(world, entity, text));
    world.remove::<FieldTransactions>(entity);
    if let Some(prompt) = world.get_mut::<Prompt>(entity) {
        prompt.scroll = 0.;
    }
}
// Prompt currently emits Submitted after folding the entire key batch rather
// than at the Enter boundary. Refuse ambiguous batches: never send enlarged
// inference accidentally. Ordinary Enter-at-end still uses native Prompt.
pub(super) fn guard_submit_boundary(world: &mut World) {
    let input = world.resource::<Input>().unwrap();
    let presses = input
        .keys
        .iter()
        .filter(|k| !k.released && !k.synthetic)
        .collect::<Vec<_>>();
    let enter = presses
        .iter()
        .position(|k| k.logical == LogicalKey::Named(NamedKey::Enter) && !k.modifiers.shift);
    let safe = enter.is_none_or(|i| i + 1 == presses.len())
        && input.ime_preedit.is_empty()
        && input.ime_commit.is_empty()
        && !(input.keys.is_empty() && input.key == Some(Key::Enter) && !input.chars.is_empty());
    world.resource_mut::<SubmitBoundary>().unwrap().safe = safe;
}

pub(super) struct Handles {
    pub(super) composer: Entity,
    pub(super) model: Entity,
    pub(super) send: Entity,
    pub(super) cancel: Entity,
    pub(super) runtime: Entity,
    pub(super) mode: Entity,
    pub(super) behavior: Entity,
}

pub(super) fn setup(world: &mut World, center: Entity) -> Handles {
    let theme = presentation::theme(world);
    let stickies = muted(world, center, "Sticky notes · not ported");
    world.get_mut::<TextStyle>(stickies).unwrap().align = TextAlign::End;
    // Semantic surface owns its chrome; the content slot retains our Prompt
    // and controls. Neither theme changes nor remounts replace editor state.
    let composer_surface = rndr_ui::Surface::Raised
        .mount(world, center, rndr_ui::UiId(1))
        .expect("live composer parent and unique component key");
    let box_ = composer_surface.content;
    world.insert(composer_surface.root, Radius(18.));
    presentation::outline(world, composer_surface.root);
    world
        .get_mut::<Layout>(composer_surface.root)
        .unwrap()
        .padding = 10.;
    world.get_mut::<Layout>(box_).unwrap().gap = 4.;
    let composer = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            min_height: 28.,
            ..Layout::column()
        },
        Label::new(""),
        Prompt::new().max_lines(6),
        TextField,
        Focusable,
    ));
    world.insert(composer, Placeholder::new("Message Medousa…"));
    world.insert(composer, theme.typography(Typography::Reading));
    world.insert(
        composer,
        Semantics {
            name: Some("Message · Enter to send, Shift Enter for new line".into()),
            ..Default::default()
        },
    );
    child(world, box_, composer);
    let controls = row(world, box_, Size::Px(34.));
    let plus = button(world, controls, "attachments", "+");
    world.get_mut::<Layout>(plus).unwrap().width = Size::Px(28.);
    world.insert(plus, Color::rgba(0., 0., 0., 0.));
    world.insert(
        plus,
        Semantics {
            name: Some("Attachments · not ported".into()),
            ..Default::default()
        },
    );
    let model = button(world, controls, "model", "Select model");
    world.get_mut::<Layout>(model).unwrap().width = Size::Fill;
    world.insert(model, Color::rgba(0., 0., 0., 0.));
    world.get_mut::<TextStyle>(model).unwrap().size = 13.;
    let model_air = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    child(world, controls, model_air);
    let send = button(world, controls, "send", "↑");
    world.get_mut::<Layout>(send).unwrap().width = Size::Px(32.);
    world.insert(
        send,
        Semantics {
            name: Some("Send message · may incur inference charges".into()),
            ..Default::default()
        },
    );
    world.insert(send, Radius(16.));
    let cancel = button(world, controls, "cancel", "Stop");
    world.get_mut::<Layout>(cancel).unwrap().width = Size::Px(58.);
    for e in [model, plus, send, cancel] {
        let layout = world.get_mut::<Layout>(e).unwrap();
        layout.min_height = 0.;
        layout.height = Size::Px(32.);
    }
    world.get_mut::<TextStyle>(send).unwrap().align = TextAlign::Center;
    world.get_mut::<Label>(model).unwrap().overflow = TextOverflow::Ellipsis;
    let settings = row(world, center, Size::Px(28.));
    let runtime = button(world, settings, "runtime", "Medousa v");
    let mode = button(world, settings, "mode", "Mode v");
    let air = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    child(world, settings, air);
    let behavior = button(world, settings, "behavior", "Behavior v");
    for (e, width) in [(runtime, 110.), (mode, 110.), (behavior, 105.)] {
        world.get_mut::<Layout>(e).unwrap().min_height = 0.;
        world.get_mut::<Layout>(e).unwrap().height = Size::Px(28.);
        world.insert(e, Color::rgba(0., 0., 0., 0.));
        world.get_mut::<Layout>(e).unwrap().width = Size::Px(width);
        world.get_mut::<TextStyle>(e).unwrap().size = 12.;
        presentation::secondary(world, e);
    }
    Handles {
        composer,
        model,
        send,
        cancel,
        runtime,
        mode,
        behavior,
    }
}
