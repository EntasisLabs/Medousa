//! Identity-first bot shortcuts and a shared navigation editor. A bot shortcut
//! resumes the daemon's primary conversation; it never creates a parallel chat.
use crate::{bot_backend as api, routing::Binding, surfaces::SurfaceId};
use medousa_types::BotProfile;
use rndr_core::*;
use rndr_ui::{
    Typography, UiId,
    controls::{self, ButtonProps, HostIntent},
    text,
};
use std::collections::HashMap;
const PLUS: u64 = 0x424f_0001;
const CLOSE: u64 = 0x424f_0002;
const NEW: u64 = 0x424f_0003;
const SAVE: u64 = 0x424f_0004;
const BACK: u64 = 0x424f_0005;
const EDIT: u64 = 0x424f_0006;
const REFRESH: u64 = 0x424f_0007;
const PIN_NEW: u64 = 0x424f_0008;
#[derive(Clone)]
enum Action {
    Open(String),
    Edit(String),
    Avatar(String),
    Toggle(String, bool),
    Up(String),
    Down(String),
    Pin(String, bool),
    Accent(String, String),
}
#[derive(Clone, Default)]
struct Draft {
    bot: Option<BotProfile>,
    name: String,
    description: String,
    instructions: String,
    instructions_loaded: bool,
    avatar: String,
    pin_new: bool,
}
struct State {
    root: Entity,
    shortcuts: Entity,
    badge: Entity,
    binding: Option<Binding>,
    generation: u64,
    bots: Vec<BotProfile>,
    pending: bool,
    nav_pending: bool,
    commands: Vec<api::Request>,
    nav_commands: Vec<crate::nav_layout::Command>,
    snapshot: Option<crate::nav_layout::Snapshot>,
    open: bool,
    draft: Option<Draft>,
    fields: Vec<Entity>,
    dirty: bool,
    parked: HashMap<String, Draft>,
    save_instructions: Option<(String, String)>,
    notice: String,
    actions: HashMap<u64, Action>,
    next: u64,
}
fn panel(w: &mut World, p: Entity, l: Layout) -> Entity {
    let e = w.spawn((l,));
    set_parent(w, e, p);
    e
}
fn label(w: &mut World, p: Entity, s: &str, k: Typography) -> Entity {
    let e = text(w, p, s, k).unwrap();
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
    e
}
fn button(w: &mut World, p: Entity, id: u64, s: &str, on: bool) -> Entity {
    let mut props = ButtonProps::new(s, HostIntent::Command(id));
    props.enabled = on;
    let handle = controls::Button::mount(w, p, UiId(id), props).unwrap();
    let root = handle.root;
    w.insert(
        root,
        crate::native_style::FitLabel {
            label: handle.label,
            extra: 4.,
            min: 40.,
            max: 220.,
        },
    );
    if let Some(layout) = w.get_mut::<Layout>(root) {
        layout.height = Size::Px(36.);
    }
    for node in descendants(w, root) {
        w.get_mut::<Layout>(node).unwrap().height = Size::Fill;
    }
    root
}
/// Equal-size native targets: every focus/hover layer fills the target, not
/// the text button's old Hug-height capsule. Decorative artwork is independent.
fn rail_button(
    w: &mut World,
    p: Entity,
    id: u64,
    name: &str,
    icon: Icon,
    enabled: bool,
) -> controls::ControlHandle {
    let mut props = ButtonProps::new(name, HostIntent::Command(id));
    props.variant = controls::ButtonVariant::Quiet;
    props.enabled = enabled;
    let h = controls::IconButton::icon(w, p, UiId(id), icon, name, props).unwrap();
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Px(52.),
        height: Size::Px(52.),
        padding: 2.,
        ..Layout::column()
    };
    for node in descendants(w, h.root) {
        w.get_mut::<Layout>(node).unwrap().height = Size::Fill;
    }
    w.insert(
        h.label,
        LabelBox {
            inset_x: 5.,
            inset_y: 5.,
            min_height: 0.,
        },
    );
    w.insert(h.root, Radius(14.));
    rndr_ui::inputs::Tooltip::attach(w, h.root, name).unwrap();
    h
}
fn action_button(
    w: &mut World,
    p: Entity,
    s: &mut State,
    title: &str,
    a: Action,
    on: bool,
) -> Entity {
    s.next += 1;
    let id = s.next;
    s.actions.insert(id, a);
    button(w, p, id, title, on)
}
/// Editor rows reserve the trailing controls; names never dictate column widths.
fn editor_row(w: &mut World, p: Entity) -> Entity {
    let row = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Px(44.),
            gap: 6.,
            ..Layout::row()
        },
    );
    w.insert(
        row,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    row
}
fn editor_name(w: &mut World, p: Entity, name: &str) -> Entity {
    let e = label(w, p, name, Typography::Label);
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Ellipsis;
    e
}
fn editor_action(
    w: &mut World,
    p: Entity,
    s: &mut State,
    icon: Icon,
    name: &str,
    action: Action,
    enabled: bool,
) -> Entity {
    s.next += 1;
    s.actions.insert(s.next, action);
    crate::native_style::icon_action(w, p, s.next, icon, name, enabled).root
}
fn field(w: &mut World, p: Entity, name: &str, value: &str, multi: bool) -> Entity {
    label(w, p, name, Typography::Caption);
    let e = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(if multi { 80. } else { 36. }),
            padding: 8.,
            ..Layout::column()
        },
        Label::new(value),
        Focusable,
        Prompt {
            submit_on_enter: false,
            max_lines: if multi { 4 } else { 1 },
            ..Default::default()
        },
        Placeholder::new(name),
        Semantics {
            name: Some(name.into()),
            ..Default::default()
        },
    ));
    set_parent(w, e, p);
    crate::native_style::plane(w, e, crate::native_style::raised());
    w.insert(e, Radius(8.));
    e
}
fn descendants(w: &World, e: Entity) -> Vec<Entity> {
    let mut nodes = children_of(w, e);
    let mut out = Vec::new();
    while let Some(e) = nodes.pop() {
        nodes.extend(children_of(w, e));
        out.push(e);
    }
    out
}
fn bot_accent(s: &State, id: &str) -> Option<Color> {
    let hex = s.snapshot.as_ref()?.bot_accent(id)?;
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()?;
    Some(Color::rgba(
        ((value >> 16) & 255) as f32 / 255.,
        ((value >> 8) & 255) as f32 / 255.,
        (value & 255) as f32 / 255.,
        1.,
    ))
}
fn clear(w: &mut World, e: Entity) {
    if w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|f| crate::surfaces::descendant(w, f, e))
    {
        w.resource_mut::<Focus>().unwrap().entity = None;
    }
    for child in children_of(w, e) {
        despawn(w, child);
    }
}
struct Slot(Option<State>);
fn with(w: &mut World, f: impl FnOnce(&mut World, &mut State)) {
    // Resource temporarily removed so view construction can use the World.
    let Some(mut s) = w.resource_mut::<Slot>().and_then(|slot| slot.0.take()) else {
        return;
    };
    f(w, &mut s);
    w.resource_mut::<Slot>().unwrap().0 = Some(s);
}
pub fn mount(w: &mut World, parent: Entity) {
    let rail = crate::surfaces::rail(w).unwrap();
    let shortcuts = panel(
        w,
        rail,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::column()
        },
    );
    w.insert(
        shortcuts,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    if let Some(c) = w.get_mut::<Children>(rail) {
        c.list.retain(|e| *e != shortcuts);
        c.list.insert(1.min(c.list.len()), shortcuts);
    }
    let plus = rail_button(w, rail, PLUS, "Customize navigation", Icon::Plus, true).root;
    w.insert(
        plus,
        Semantics {
            name: Some("Customize navigation".into()),
            ..Default::default()
        },
    );
    rndr_ui::inputs::Tooltip::attach(w, plus, "Customize navigation").unwrap();
    // Preserve the original footer order: edit navigation above Settings.
    crate::surfaces::settings_to_foot(w);
    let root = panel(
        w,
        parent,
        Layout {
            width: Size::Px(368.),
            height: Size::Fill,
            padding: 18.,
            gap: 10.,
            ..Layout::column()
        },
    );
    w.insert(
        root,
        Absolute {
            right: Some(0.),
            top: Some(0.),
            bottom: Some(0.),
            ..Default::default()
        },
    );
    w.insert(root, Scroll::default());
    w.insert(root, Layer(120));
    w.insert(
        root,
        Padding {
            top: 54.,
            left: 18.,
            right: 18.,
            bottom: 18.,
        },
    );
    crate::native_style::plane(w, root, crate::native_style::navigation());
    w.insert(root, Hidden);
    let badge = panel(
        w,
        parent,
        Layout {
            width: Size::Px(220.),
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },
    );
    w.insert(
        badge,
        Absolute {
            left: Some(420.),
            top: Some(8.),
            ..Default::default()
        },
    );
    w.insert(badge, Hidden);
    w.insert(badge, Layer(40));
    w.insert_resource(Slot(Some(State {
        root,
        shortcuts,
        badge,
        binding: None,
        generation: 0,
        bots: vec![],
        pending: false,
        nav_pending: false,
        commands: vec![],
        nav_commands: vec![],
        snapshot: None,
        open: false,
        draft: None,
        fields: vec![],
        dirty: true,
        parked: HashMap::new(),
        save_instructions: None,
        notice: String::new(),
        actions: HashMap::new(),
        next: 0x424f_1000,
    })));
}
fn capture(w: &World, s: &mut State) {
    if let Some(d) = &mut s.draft {
        for (i, e) in s.fields.iter().enumerate() {
            if let Some(l) = w.get::<Label>(*e) {
                match i {
                    0 => d.name = l.text.clone(),
                    1 => d.description = l.text.clone(),
                    2 => d.instructions = l.text.clone(),
                    _ => {}
                }
            }
        }
    }
}
fn composing(w: &World, s: &State) -> bool {
    s.fields.iter().any(|e| w.has::<ImePreview>(*e))
}
fn queue(s: &mut State, command: api::Command) {
    if s.pending {
        return;
    }
    if let Some(binding) = s.binding.clone() {
        s.pending = true;
        s.commands.push(api::Request { binding, command });
        s.notice = "Working…".into();
        s.dirty = true;
    }
}
fn nav(s: &mut State, operation: crate::nav_layout::Operation) {
    if s.nav_pending {
        return;
    }
    if let Some(b) = &s.binding {
        s.nav_pending = true;
        s.nav_commands.push(crate::nav_layout::Command {
            authority: b.authority.clone(),
            generation: s.generation,
            profile_id: None,
            operation,
        });
        s.dirty = true;
    }
}
fn render(w: &mut World, s: &mut State) {
    clear(w, s.root);
    clear(w, s.shortcuts);
    clear(w, s.badge);
    s.fields.clear();
    s.actions.clear();
    let ready = s.binding.is_some() && !s.pending;
    // Shortcut presentation is generated from confirmed persisted pins only.
    let pinned_bots: Vec<_> = s
        .bots
        .iter()
        .filter(|b| pinned(s, &b.bot_id.to_string()))
        .cloned()
        .collect();
    if !pinned_bots.is_empty() {
        let divider = panel(
            w,
            s.shortcuts,
            Layout {
                width: Size::Px(36.),
                height: Size::Px(1.),
                ..Layout::column()
            },
        );
        crate::native_style::plane(w, divider, rndr_ui::Token::SubtleStroke);
    }
    for bot in pinned_bots {
        s.next += 1;
        let id = s.next;
        s.actions.insert(id, Action::Open(bot.bot_id.to_string()));
        let h = rail_button(
            w,
            s.shortcuts,
            id,
            &bot.display_name,
            Icon::MessageCircle,
            ready,
        );
        let e = h.root;
        // Hide the canonical glyph without changing its accessible name.
        w.insert(h.label, Hidden);
        for entity in descendants(w, e) {
            if let Some(label) = w.get_mut::<Label>(entity) {
                label.text.clear();
            }
        }
        let image = crate::bot_assets::mount(
            w,
            e,
            bot.avatar_ref.as_deref().unwrap_or("jellyfish-default"),
            44.,
        );
        w.insert(
            image,
            Absolute {
                left: Some(4.),
                top: Some(4.),
                ..Default::default()
            },
        );
        rndr_ui::inputs::Tooltip::attach(w, e, &bot.display_name).unwrap();
        // Accent is a restrained selected wash, never a solid avatar box.
        let selected = crate::surfaces::active(w) == SurfaceId::Bots
            && bot.primary_session_id.as_ref().is_some_and(|session| {
                Some(crate::routing::qualify(
                    s.binding.as_ref().map_or("", |b| b.authority.as_str()),
                    session,
                )) == crate::ui::active_chat_session(w)
            });
        if selected {
            let mut color = bot_accent(s, &bot.bot_id.to_string())
                .unwrap_or_else(|| crate::native_style::color(w, rndr_ui::Token::Accent));
            color.a = 0.14;
            let wash = panel(
                w,
                e,
                Layout {
                    width: Size::Px(48.),
                    height: Size::Px(48.),
                    ..Layout::column()
                },
            );
            w.insert(
                wash,
                Absolute {
                    left: Some(2.),
                    top: Some(2.),
                    ..Default::default()
                },
            );
            w.insert(wash, color);
            w.insert(wash, Radius(12.));
            // Artwork paints above the wash.
            if let Some(children) = w.get_mut::<Children>(e) {
                children.list.retain(|child| *child != wash);
                children.list.insert(0, wash);
            }
        }
    }
    let current = crate::ui::active_chat_session(w);
    let current_bot = s
        .bots
        .iter()
        .find(|b| {
            b.primary_session_id.as_ref().is_some_and(|id| {
                Some(crate::routing::qualify(
                    s.binding.as_ref().map_or("", |b| b.authority.as_str()),
                    id,
                )) == current
            })
        })
        .cloned();
    if crate::surfaces::active(w) == SurfaceId::Chat && !s.open {
        if let Some(bot) = current_bot {
            crate::ui::bot_composer_identity(w, Some(&bot.display_name));
            w.remove::<Hidden>(s.badge);
            avatar(
                w,
                s.badge,
                bot.avatar_ref.as_deref().unwrap_or("jellyfish-default"),
                26.,
            );
            button(
                w,
                s.badge,
                EDIT,
                &format!("{} · Customize", bot.display_name),
                ready,
            );
        } else {
            crate::ui::bot_composer_identity(w, None);
            w.insert(s.badge, Hidden);
        }
    } else {
        w.insert(s.badge, Hidden);
    }
    if !s.open {
        w.insert(s.root, Hidden);
        return;
    }
    w.remove::<Hidden>(s.root);
    let head = panel(
        w,
        s.root,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },
    );
    let title = label(
        w,
        head,
        if s.draft.is_some() {
            "Make it yours"
        } else {
            "Customize navigation"
        },
        Typography::Title,
    );
    w.get_mut::<Layout>(title).unwrap().width = Size::Fill;
    crate::native_style::icon_action(
        w,
        head,
        CLOSE,
        Icon::X,
        "Close navigation editor",
        !s.pending,
    );
    // Keep the editor header reachable while the lists scroll independently.
    if s.draft.is_some() {
        w.insert(s.root, Scroll::default());
    } else {
        w.remove::<Scroll>(s.root);
    }
    if !s.notice.is_empty() {
        label(w, s.root, &s.notice, Typography::Caption);
    }
    if let Some(d) = s.draft.clone() {
        avatar(w, s.root, &d.avatar, 64.);
        s.fields.push(field(w, s.root, "Name", &d.name, false));
        s.fields
            .push(field(w, s.root, "Description", &d.description, true));
        label(w, s.root, "Choose a character", Typography::Label);
        let choices = panel(
            w,
            s.root,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 8.,
                ..Layout::row()
            },
        );
        for key in ["jellyfish-default", "seahorse-default", "starfish-default"] {
            let e = action_button(
                w,
                choices,
                s,
                key.split('-').next().unwrap(),
                Action::Avatar(key.into()),
                ready,
            );
            avatar(w, e, key, 28.);
        }
        label(w, s.root, "Behavior", Typography::Label);
        label(
            w,
            s.root,
            "Describe how your bot should respond. Deeper capabilities stay in its manuscript.",
            Typography::Caption,
        );
        s.fields
            .push(field(w, s.root, "Instructions", &d.instructions, true));
        if d.bot.is_none() {
            button(
                w,
                s.root,
                PIN_NEW,
                if d.pin_new {
                    "✓ Add to navigation"
                } else {
                    "+ Add to navigation"
                },
                ready,
            );
        }
        if let Some(bot) = &d.bot {
            let id = bot.bot_id.to_string();
            let pin = pinned(s, &id);
            action_button(
                w,
                s.root,
                s,
                if pin {
                    "Unpin from navigation"
                } else {
                    "Pin to navigation"
                },
                Action::Pin(id, !pin),
                !s.nav_pending,
            );
        }
        if let Some(bot) = &d.bot {
            label(w, s.root, "Accent color", Typography::Label);
            let colors = panel(
                w,
                s.root,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 6.,
                    ..Layout::row()
                },
            );
            for (name, hex) in [
                ("Purple", "#a78bfa"),
                ("Orange", "#f59e0b"),
                ("Blue", "#3b82f6"),
                ("Teal", "#14b8a6"),
                ("Pink", "#ec4899"),
            ] {
                action_button(
                    w,
                    colors,
                    s,
                    name,
                    Action::Accent(bot.bot_id.to_string(), hex.into()),
                    !s.nav_pending && s.snapshot.is_some(),
                );
            }
        }
        button(
            w,
            s.root,
            SAVE,
            if d.bot.is_some() {
                "Save changes"
            } else {
                "Create & chat"
            },
            ready && d.instructions_loaded,
        );
        button(w, s.root, BACK, "Back", !s.pending);
    } else {
        label(
            w,
            s.root,
            "Choose what appears in your sidebar.",
            Typography::Caption,
        );
        let toolbar = editor_row(w, s.root);
        editor_name(w, toolbar, "Your bots");
        crate::native_style::document_action(w, toolbar, NEW, "Create bot", ready);
        crate::native_style::icon_action(
            w,
            toolbar,
            REFRESH,
            Icon::RefreshCw,
            "Refresh bots and navigation",
            ready && !s.nav_pending,
        );
        let body = panel(
            w,
            s.root,
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                gap: 4.,
                ..Layout::column()
            },
        );
        w.insert(body, Scroll::default());
        if s.bots.is_empty() {
            label(
                w,
                body,
                "No bots yet. Create one to make a personal chat companion.",
                Typography::Caption,
            );
        }
        for bot in s.bots.clone() {
            let id = bot.bot_id.to_string();
            let row = editor_row(w, body);
            avatar(
                w,
                row,
                bot.avatar_ref.as_deref().unwrap_or("jellyfish-default"),
                28.,
            );
            editor_name(w, row, &bot.display_name);
            editor_action(
                w,
                row,
                s,
                Icon::MessageCircle,
                &format!("Chat with {}", bot.display_name),
                Action::Open(id.clone()),
                ready,
            );
            editor_action(
                w,
                row,
                s,
                Icon::Pencil,
                &format!("Edit {}", bot.display_name),
                Action::Edit(id.clone()),
                ready,
            );
            let pin = pinned(s, &id);
            editor_action(
                w,
                row,
                s,
                if pin { Icon::Check } else { Icon::Plus },
                &format!("{} {}", if pin { "Unpin" } else { "Pin" }, bot.display_name),
                Action::Pin(id, !pin),
                !s.nav_pending && s.snapshot.is_some(),
            );
        }
        crate::native_style::rule(w, body);
        label(w, body, "App destinations", Typography::Label);
        label(
            w,
            body,
            "Eye: show or hide · Arrows: change order",
            Typography::Caption,
        );
        let destinations = nav_destinations(s);
        let mut entries = crate::surfaces::DESTINATIONS.to_vec();
        entries.sort_by_key(|(_, id, _)| {
            destinations
                .iter()
                .position(|(key, _)| key == id)
                .unwrap_or(usize::MAX)
        });
        let movable: Vec<_> = entries
            .iter()
            .filter(|(_, id, _)| {
                crate::nav_layout::toggleable(id)
                    && destinations
                        .iter()
                        .any(|(key, visible)| key == id && *visible)
            })
            .map(|(_, id, _)| *id)
            .collect();
        for (surface, id, name) in entries {
            if surface == SurfaceId::Settings {
                continue;
            }
            let visible = destinations
                .iter()
                .find(|(key, _)| key == id)
                .map_or(true, |(_, v)| *v);
            let row = editor_row(w, body);
            editor_name(w, row, name);
            let editable = !s.nav_pending && s.snapshot.is_some();
            let toggleable = crate::nav_layout::toggleable(id);
            editor_action(
                w,
                row,
                s,
                if !toggleable {
                    Icon::Lock
                } else if visible {
                    Icon::Eye
                } else {
                    Icon::EyeOff
                },
                &format!(
                    "{} {name}",
                    if !toggleable {
                        "Always visible:"
                    } else if visible {
                        "Hide"
                    } else {
                        "Show"
                    }
                ),
                Action::Toggle(id.into(), !visible),
                editable && toggleable,
            );
            let index = movable.iter().position(|key| *key == id);
            editor_action(
                w,
                row,
                s,
                Icon::ArrowUp,
                &format!("Move {name} up"),
                Action::Up(id.into()),
                editable && index.is_some_and(|i| i > 0),
            );
            editor_action(
                w,
                row,
                s,
                Icon::ArrowDown,
                &format!("Move {name} down"),
                Action::Down(id.into()),
                editable && index.is_some_and(|i| i + 1 < movable.len()),
            );
        }
    }
}
fn avatar(w: &mut World, p: Entity, key: &str, size: f32) {
    crate::bot_assets::mount(w, p, key, size);
}
fn pinned(s: &State, id: &str) -> bool {
    s.snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.bot_pins().iter().any(|key| key == id))
}
fn nav_destinations(s: &State) -> Vec<(String, bool)> {
    s.snapshot
        .as_ref()
        .map(|s| {
            s.destinations()
                .into_iter()
                .map(|d| (d.id, d.visible))
                .collect()
        })
        .unwrap_or_default()
}
pub(crate) fn is_bot_session(w: &World, session: &str) -> bool {
    if crate::ui::is_known_bot_session(w, session) { return true; }
    w.resource::<Slot>().and_then(|slot| slot.0.as_ref()).is_some_and(|s| {
        s.bots.iter().any(|b| b.primary_session_id.as_ref().is_some_and(|id| {
            crate::routing::qualify(s.binding.as_ref().map_or("", |b| b.authority.as_str()), id) == session
        }))
    })
}
pub(crate) fn has_active_identity(w: &World) -> bool {
    let Some(s) = w.resource::<Slot>().and_then(|slot| slot.0.as_ref()) else {
        return false;
    };
    let current = crate::ui::active_chat_session(w);
    s.bots
        .iter()
        .filter(|bot| pinned(s, &bot.bot_id.to_string()))
        .any(|bot| {
            bot.primary_session_id.as_ref().is_some_and(|session| {
                Some(crate::routing::qualify(
                    s.binding.as_ref().map_or("", |b| b.authority.as_str()),
                    session,
                )) == current
            })
        })
}
pub fn sync(w: &mut World) {
    crate::bot_assets::sync(w);
    with(w, |w, s| {
        let width = w.resource::<Viewport>().map_or(1200., |v| v.width);
        let badge_left = crate::ui::bot_identity_left(w, 220.);
        if let Some(position) = w.get_mut::<Absolute>(s.badge) {
            position.left = Some(badge_left.unwrap_or_else(|| {
                crate::surfaces::RAIL_WIDTH
                    + (width - crate::surfaces::RAIL_WIDTH - 220.).max(0.) / 2.
            }));
        }
        if let Some(viewport) = w.resource::<Viewport>() {
            let width = (viewport.width - crate::surfaces::RAIL_WIDTH).clamp(260., 368.);
            if let Some(layout) = w.get_mut::<Layout>(s.root) {
                layout.width = Size::Px(width);
            }
        }
        if s.open
            && !s.pending
            && w.resource::<Input>()
                .is_some_and(|i| i.key == Some(Key::Escape))
        {
            s.open = false;
            s.dirty = true;
        }
        let owner = crate::workshops::owner(w, crate::ui::active_chat_session(w).as_deref())
            .or_else(|| crate::workshops::owner(w, None));
        if owner.as_deref() != s.binding.as_ref().map(|b| b.authority.as_str()) {
            // Park edits by daemon; never show or save another workshop's bot.
            capture(w, s);
            if let (Some(binding), Some(draft)) = (&s.binding, s.draft.take()) {
                s.parked.insert(binding.authority.clone(), draft);
            }
            s.fields.clear();
            s.open = false;
            s.generation += 1;
            s.binding = owner.map(|a| Binding {
                authority: a,
                epoch: s.generation,
            });
            s.bots.clear();
            s.snapshot = None;
            s.pending = false;
            s.nav_pending = false;
            s.notice.clear();
            s.dirty = true;
            if s.binding.is_some() {
                queue(s, api::Command::List);
                nav(s, crate::nav_layout::Operation::Read);
            }
        }
        // Header must follow ordinary chat/tab switches, not just bot actions.
        let key = (
            crate::ui::active_chat_session(w),
            crate::surfaces::active(w),
        );
        let prev = w.resource::<BadgeKey>().map(|k| k.0.clone());
        if prev.as_ref() != Some(&key) {
            w.insert_resource(BadgeKey(key));
            s.dirty = true;
        }
        if s.dirty && !composing(w, s) {
            capture(w, s);
            render(w, s);
            s.dirty = false;
        }
    });
}
struct BadgeKey((Option<String>, SurfaceId));
pub fn action(w: &mut World, intent: &HostIntent) -> bool {
    let HostIntent::Command(id) = intent else {
        return false;
    };
    let Some(s) = w.resource::<Slot>().and_then(|slot| slot.0.as_ref()) else {
        return false;
    };
    let a = s.actions.get(id).cloned();
    if a.is_none()
        && !matches!(
            *id,
            PLUS | CLOSE | NEW | SAVE | BACK | EDIT | REFRESH | PIN_NEW
        )
    {
        return false;
    }
    with(w, |w, s| {
        if composing(w, s) {
            return;
        }
        capture(w, s);
        match *id {
            PLUS => {
                if s.draft.is_none() {
                    if let Some(binding) = &s.binding {
                        s.draft = s.parked.remove(&binding.authority);
                        s.fields.clear();
                    }
                }
                s.open = true;
            }
            CLOSE if !s.pending => {
                s.open = false;
                s.draft = None;
            }
            BACK if !s.pending => s.draft = None,
            NEW => {
                s.draft = Some(Draft {
                    avatar: "jellyfish-default".into(),
                    instructions_loaded: true,
                    ..Default::default()
                });
                s.fields.clear();
                s.open = true;
            }
            REFRESH => {
                queue(s, api::Command::List);
                nav(s, crate::nav_layout::Operation::Read);
            }
            EDIT => {
                let session = crate::ui::active_chat_session(w);
                if let Some(b) = s
                    .bots
                    .iter()
                    .find(|b| {
                        b.primary_session_id.as_ref().is_some_and(|id| {
                            Some(crate::routing::qualify(
                                s.binding.as_ref().map_or("", |b| b.authority.as_str()),
                                id,
                            )) == session
                        })
                    })
                    .cloned()
                {
                    edit(s, b);
                }
            }
            PIN_NEW => {
                if let Some(d) = &mut s.draft {
                    d.pin_new = !d.pin_new;
                }
            }
            SAVE => {
                if let Some(d) = s.draft.clone() {
                    if d.name.trim().is_empty() {
                        s.notice = "Give your bot a name.".into();
                    } else if !d.instructions_loaded {
                        s.notice = "Wait for instructions to load before saving.".into();
                    } else if let Some(b) = d.bot {
                        let mut request = api::edit_request(&b);
                        request.display_name = d.name.trim().into();
                        request.role_description = Some(d.description);
                        request.avatar_ref = Some(d.avatar);
                        s.save_instructions =
                            Some((b.primary_manuscript_id.clone(), d.instructions));
                        queue(
                            s,
                            api::Command::Update {
                                bot_id: b.bot_id.to_string(),
                                request,
                            },
                        );
                    } else {
                        queue(
                            s,
                            api::Command::CreatePersonalized {
                                display_name: d.name.trim().into(),
                                role_description: Some(d.description),
                                avatar_ref: Some(d.avatar),
                                instructions: d.instructions,
                            },
                        );
                    }
                }
            }
            _ => match a {
                Some(Action::Open(bot_id)) => queue(s, api::Command::Open { bot_id }),
                Some(Action::Edit(id)) => {
                    if let Some(b) = s.bots.iter().find(|b| b.bot_id.to_string() == id).cloned() {
                        edit(s, b);
                    }
                }
                Some(Action::Avatar(key)) => {
                    if let Some(d) = &mut s.draft {
                        d.avatar = key;
                    }
                }
                Some(Action::Toggle(surface_id, visible)) => {
                    let current = crate::surfaces::active(w);
                    let hiding_current = !visible
                        && crate::surfaces::DESTINATIONS
                            .iter()
                            .any(|(surface, id, _)| *surface == current && *id == surface_id);
                    if hiding_current {
                        crate::surfaces::select(
                            w,
                            if current == SurfaceId::Chat {
                                SurfaceId::Workspace
                            } else {
                                SurfaceId::Chat
                            },
                        );
                    }
                    if hiding_current && crate::surfaces::active(w) == current {
                        s.notice="Finish the current edit or composition before hiding this destination.".into();
                    } else {
                        nav(
                            s,
                            crate::nav_layout::Operation::SetVisibility {
                                surface_id,
                                visible,
                            },
                        );
                    }
                }
                Some(Action::Up(surface_id)) => {
                    let ds: Vec<_> = nav_destinations(s)
                        .into_iter()
                        .filter(|(key, _)| {
                            crate::nav_layout::toggleable(key)
                                && nav_destinations(s)
                                    .iter()
                                    .any(|(id, visible)| id == key && *visible)
                                && crate::surfaces::DESTINATIONS
                                    .iter()
                                    .any(|(_, id, _)| id == key)
                        })
                        .collect();
                    if let Some(i) = ds.iter().position(|(id, _)| id == &surface_id) {
                        if i > 0 {
                            nav(
                                s,
                                crate::nav_layout::Operation::Reorder {
                                    surface_id,
                                    before_surface_id: Some(ds[i - 1].0.clone()),
                                },
                            );
                        }
                    }
                }
                Some(Action::Down(surface_id)) => {
                    let ds: Vec<_> = nav_destinations(s)
                        .into_iter()
                        .filter(|(key, visible)| {
                            *visible
                                && crate::nav_layout::toggleable(key)
                                && crate::surfaces::DESTINATIONS
                                    .iter()
                                    .any(|(_, id, _)| id == key)
                        })
                        .collect();
                    if let Some(i) = ds.iter().position(|(id, _)| id == &surface_id) {
                        if i + 1 < ds.len() {
                            nav(
                                s,
                                crate::nav_layout::Operation::Reorder {
                                    surface_id,
                                    before_surface_id: ds.get(i + 2).map(|(id, _)| id.clone()),
                                },
                            );
                        }
                    }
                }
                Some(Action::Accent(bot_id, accent)) => nav(
                    s,
                    crate::nav_layout::Operation::SetBotAccent {
                        bot_id,
                        accent: Some(accent),
                    },
                ),
                Some(Action::Pin(bot_id, pinned)) => nav(
                    s,
                    crate::nav_layout::Operation::SetBotPinned { bot_id, pinned },
                ),
                _ => {}
            },
        }
        s.dirty = true;
    });
    true
}
fn edit(s: &mut State, b: BotProfile) {
    let manuscript_id = b.primary_manuscript_id.clone();
    s.draft = Some(Draft {
        name: b.display_name.clone(),
        description: b.role_description.clone().unwrap_or_default(),
        instructions: String::new(),
        instructions_loaded: false,
        pin_new: false,
        avatar: b
            .avatar_ref
            .clone()
            .unwrap_or_else(|| "jellyfish-default".into()),
        bot: Some(b),
    });
    s.fields.clear();
    s.open = true;
    queue(s, api::Command::GetInstructions { manuscript_id });
}
pub fn take_commands(w: &mut World) -> Vec<api::Request> {
    w.resource_mut::<Slot>()
        .and_then(|slot| slot.0.as_mut())
        .map(|s| std::mem::take(&mut s.commands))
        .unwrap_or_default()
}
pub fn take_nav_commands(w: &mut World) -> Vec<crate::nav_layout::Command> {
    w.resource_mut::<Slot>()
        .and_then(|slot| slot.0.as_mut())
        .map(|s| std::mem::take(&mut s.nav_commands))
        .unwrap_or_default()
}
pub fn apply(w: &mut World, u: api::Update) {
    with(w, |w, s| {
        if s.binding.as_ref() != Some(&u.binding) {
            return;
        }
        s.pending = false;
        match u.result {
            Err(e) => {
                s.save_instructions = None;
                s.notice = e;
            }
            Ok(api::Outcome::Listed(bots)) => {
                for session in bots.iter().filter_map(|b| b.primary_session_id.as_ref()) {
                    crate::ui::remember_bot_session(w, crate::routing::qualify(&u.binding.authority, session));
                }
                s.bots = bots.into_iter().filter(|b| !b.archived).collect();
                s.notice.clear();
            }
            Ok(api::Outcome::Updated(bot)) => {
                if let Some(b) = s.bots.iter_mut().find(|b| b.bot_id == bot.bot_id) {
                    *b = bot.clone();
                }
                if let Some(d) = &mut s.draft {
                    d.bot = Some(bot);
                }
                if let Some((manuscript_id, instructions)) = s.save_instructions.take() {
                    queue(
                        s,
                        api::Command::SaveInstructions {
                            manuscript_id,
                            instructions,
                        },
                    );
                } else {
                    s.notice = "Saved".into();
                }
            }
            Ok(api::Outcome::Instructions {
                manuscript_id,
                instructions,
            }) => {
                if let Some(d) = &mut s.draft {
                    if d.bot
                        .as_ref()
                        .is_some_and(|b| b.primary_manuscript_id == manuscript_id)
                    {
                        d.instructions = instructions;
                        d.instructions_loaded = true;
                        s.fields.clear();
                    }
                }
                s.notice.clear();
            }
            Ok(api::Outcome::Created(response)) | Ok(api::Outcome::Opened(response)) => {
                let handle =
                    crate::routing::qualify(&u.binding.authority, &response.binding.session_id);
                let bot = response.bot;
                if s.draft
                    .as_ref()
                    .is_some_and(|d| d.bot.is_none() && d.pin_new)
                {
                    nav(
                        s,
                        crate::nav_layout::Operation::SetBotPinned {
                            bot_id: bot.bot_id.to_string(),
                            pinned: true,
                        },
                    );
                }
                if let Some(b) = s.bots.iter_mut().find(|b| b.bot_id == bot.bot_id) {
                    *b = bot;
                } else {
                    s.bots.push(bot);
                }
                s.open = false;
                s.draft = None;
                s.notice.clear();
                crate::ui::open_bot_conversation(w, handle);
            }
        }
        s.dirty = true;
    });
}
pub fn apply_nav(w: &mut World, u: crate::nav_layout::Update) {
    with(w, |w, s| {
        if s.binding.as_ref().map(|b| b.authority.as_str()) != Some(u.command.authority.as_str())
            || s.generation != u.command.generation
        {
            return;
        }
        s.nav_pending = false;
        match u.result {
            Ok(snapshot) => {
                s.snapshot = Some(snapshot);
                crate::surfaces::configure_rail(w, &nav_destinations(s));
                s.notice.clear();
            }
            Err(e) => s.notice = e,
        }
        s.dirty = true;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const BOT: &str = "bot_0123456789abcdef0123456789abcdef";
    fn bot() -> BotProfile {
        serde_json::from_value(json!({"schema_version":3,"bot_id":BOT,"owner_profile_id":"user:default","display_name":"Ada","role_description":"Your thoughtful coding companion","avatar_ref":"jellyfish-default","primary_manuscript_id":"ada-identity","additional_manuscript_ids":[],"memory_scope_id":"memory:ada","primary_session_id":"primary-chat","revision":7,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"})).unwrap()
    }
    fn fixture() -> App {
        let mut app = crate::ui::app();
        app.world_mut().insert_resource(Viewport::new(1200., 800.));
        app.update();
        let w = app.world_mut();
        let handle = crate::routing::qualify("a", "primary-chat");
        w.resource_mut::<crate::ui::Model>()
            .unwrap()
            .chat
            .hydrate(handle, "assistant".into(), vec![], None)
            .unwrap();
        let binding = Binding {
            authority: "a".into(),
            epoch: 1,
        };
        let mut spec = serde_json::to_value(
            medousa_types::environment_default::default_environment_spec("personal"),
        )
        .unwrap();
        crate::nav_layout::mutate(
            &mut spec,
            &crate::nav_layout::Operation::SetBotPinned {
                bot_id: BOT.into(),
                pinned: true,
            },
        )
        .unwrap();
        with(w, |w, s| {
            s.binding = Some(binding);
            s.generation = 1;
            s.bots = vec![bot()];
            s.snapshot = Some(crate::nav_layout::Snapshot { spec, revision: 1 });
            s.pending = false;
            s.dirty = false;
            render(w, s);
        });
        app
    }
    #[test]
    fn navigation_editor_keeps_approved_overlay_and_does_not_navigate() {
        let mut app = fixture();
        let dir = tempfile::tempdir().unwrap();
        let _channels = crate::workshops::install_fixture(
            app.world_mut(),
            dir.path().join("sources.json"),
            &[("a", "Workshop A")],
        );
        let w = app.world_mut();
        let session = crate::ui::active_chat_session(w);
        let chat = crate::surfaces::root(w, SurfaceId::Chat).unwrap();
        let view = crate::surfaces::root(w, SurfaceId::Bots).unwrap();
        action(w, &HostIntent::Command(PLUS));
        sync(w);
        assert_eq!(crate::surfaces::active(w), SurfaceId::Chat);
        assert!(!w.has::<Hidden>(chat));
        assert!(w.has::<Hidden>(view));
        let s = w.resource::<Slot>().unwrap().0.as_ref().unwrap();
        assert!(!crate::surfaces::descendant(w, s.root, view));
        assert!(w.has::<Absolute>(s.root));
        assert!(w.has::<Layer>(s.root));
        assert_eq!(w.get::<Layout>(s.root).unwrap().width, Size::Px(368.));
        action(w, &HostIntent::Command(CLOSE));
        sync(w);
        assert_eq!(crate::surfaces::active(w), SurfaceId::Chat);
        assert!(!w.has::<Hidden>(chat));
        assert!(w.has::<Hidden>(view));
        assert_eq!(crate::ui::active_chat_session(w), session);
    }
    #[test]
    fn navigation_editor_leaves_selected_destination_unchanged() {
        let mut app = fixture();
        let dir = tempfile::tempdir().unwrap();
        let _channels = crate::workshops::install_fixture(
            app.world_mut(),
            dir.path().join("sources.json"),
            &[("a", "Workshop A")],
        );
        let w = app.world_mut();
        crate::surfaces::select(w, SurfaceId::Notes);
        action(w, &HostIntent::Command(PLUS));
        action(w, &HostIntent::Command(NEW));
        sync(w);
        let field = w.resource::<Slot>().unwrap().0.as_ref().unwrap().fields[0];
        w.get_mut::<Label>(field).unwrap().text = "My unsaved companion".into();
        w.resource_mut::<Focus>().unwrap().entity = Some(field);
        assert_eq!(crate::surfaces::active(w), SurfaceId::Notes);
        sync(w);
        let s = w.resource::<Slot>().unwrap().0.as_ref().unwrap();
        assert!(s.open);
        assert!(s.draft.is_some());
        action(w, &HostIntent::Command(CLOSE));
        assert_eq!(crate::surfaces::active(w), SurfaceId::Notes);
        // Opening and closing the settled editor never changes destinations.
        crate::surfaces::select(w, SurfaceId::Notes);
        action(w, &HostIntent::Command(PLUS));
        action(w, &HostIntent::Command(CLOSE));
        assert_eq!(crate::surfaces::active(w), SurfaceId::Notes);
    }
    #[test]
    fn stale_bot_and_nav_results_do_not_cross_authorities() {
        let mut app = fixture();
        let w = app.world_mut();
        apply(
            w,
            api::Update {
                binding: Binding {
                    authority: "b".into(),
                    epoch: 1,
                },
                result: Ok(api::Outcome::Listed(vec![])),
            },
        );
        assert_eq!(
            w.resource::<Slot>().unwrap().0.as_ref().unwrap().bots.len(),
            1
        );
        let s = w.resource::<Slot>().unwrap().0.as_ref().unwrap();
        let old = s.snapshot.clone().unwrap();
        apply_nav(
            w,
            crate::nav_layout::Update {
                command: crate::nav_layout::Command {
                    authority: "a".into(),
                    generation: 0,
                    profile_id: None,
                    operation: crate::nav_layout::Operation::Read,
                },
                result: Ok(old),
            },
        );
        assert_eq!(
            w.resource::<Slot>().unwrap().0.as_ref().unwrap().generation,
            1
        );
    }
    #[test]
    fn plus_opens_editor_and_create_uses_plain_instructions() {
        let mut app = fixture();
        let w = app.world_mut();
        assert!(action(w, &HostIntent::Command(PLUS)));
        assert!(w.resource::<Slot>().unwrap().0.as_ref().unwrap().open);
        action(w, &HostIntent::Command(NEW));
        with(w, |w, s| {
            render(w, s);
        });
        let fields = w
            .resource::<Slot>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .fields
            .clone();
        w.get_mut::<Label>(fields[0]).unwrap().text = "Personal bot".into();
        w.get_mut::<Label>(fields[2]).unwrap().text = "Be concise and kind".into();
        action(w, &HostIntent::Command(SAVE));
        let commands = take_commands(w);
        assert!(
            matches!(&commands[0].command,api::Command::CreatePersonalized{display_name,instructions,..} if display_name=="Personal bot"&&instructions=="Be concise and kind")
        );
    }
    #[test]
    fn bot_shortcut_queues_open_not_new_and_unpin_does_not_delete() {
        let mut app = fixture();
        let w = app.world_mut();
        let id = *w
            .resource::<Slot>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .actions
            .iter()
            .find(|(_, a)| matches!(a, Action::Open(_)))
            .unwrap()
            .0;
        action(w, &HostIntent::Command(id));
        assert!(matches!(&take_commands(w)[0].command,api::Command::Open{bot_id} if bot_id==BOT));
        with(w, |w, s| {
            s.pending = false;
            s.open = true;
            render(w, s);
        });
        let id = *w
            .resource::<Slot>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .actions
            .iter()
            .find(|(_, a)| matches!(a, Action::Pin(_, false)))
            .unwrap()
            .0;
        action(w, &HostIntent::Command(id));
        assert!(
            matches!(&take_nav_commands(w)[0].operation,crate::nav_layout::Operation::SetBotPinned{bot_id,pinned:false} if bot_id==BOT)
        );
        assert_eq!(
            w.resource::<Slot>().unwrap().0.as_ref().unwrap().bots.len(),
            1
        );
    }
    #[test]
    fn bot_editor_keeps_draft_across_avatar_and_pin_updates() {
        let mut app = fixture();
        let w = app.world_mut();
        with(w, |w, s| {
            s.draft = Some(Draft {
                name: "Ada".into(),
                instructions: "Be thoughtful".into(),
                instructions_loaded: true,
                avatar: "jellyfish-default".into(),
                bot: Some(bot()),
                ..Default::default()
            });
            s.open = true;
            render(w, s);
        });
        let fields = w
            .resource::<Slot>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .fields
            .clone();
        w.get_mut::<Label>(fields[0]).unwrap().text = "Ada revised".into();
        let id = *w
            .resource::<Slot>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .actions
            .iter()
            .find(|(_, a)| matches!(a,Action::Avatar(key)if key=="starfish-default"))
            .unwrap()
            .0;
        action(w, &HostIntent::Command(id));
        with(w, |w, s| {
            capture(w, s);
            render(w, s);
            assert_eq!(s.draft.as_ref().unwrap().name, "Ada revised");
            assert_eq!(s.draft.as_ref().unwrap().avatar, "starfish-default");
        });
    }
    #[test]
    fn bot_and_edit_targets_keep_improved_geometry_on_original_rail() {
        let mut app = fixture();
        let dir = tempfile::tempdir().unwrap();
        let _channels = crate::workshops::install_fixture(
            app.world_mut(),
            dir.path().join("sources.json"),
            &[("a", "Workshop A")],
        );
        for (width, height) in [(1200., 1000.), (760., 1000.)] {
            app.world_mut()
                .insert_resource(Viewport::new(width, height));
            for _ in 0..5 {
                app.update();
            }
            let w = app.world();
            let s = w.resource::<Slot>().unwrap().0.as_ref().unwrap();
            let target = children_of(w, s.shortcuts)
                .into_iter()
                .find(|e| w.has::<Focusable>(*e))
                .expect("pinned bot target");
            let rect = w.get::<Rect>(target).unwrap();
            assert_eq!((rect.w, rect.h), (52., 52.));
            let artwork = children_of(w, target)
                .into_iter()
                .find(|e| w.has::<ImageView>(*e))
                .unwrap();
            let image = w.get::<Rect>(artwork).unwrap();
            assert_eq!((image.w, image.h), (44., 44.));
            assert!((image.x + image.w / 2. - rect.x - rect.w / 2.).abs() < 0.1);
            assert!((image.y + image.h / 2. - rect.y - rect.h / 2.).abs() < 0.1);
            for node in w.entities_with::<Semantics>() {
                if w.get::<Semantics>(node).unwrap().name.as_deref() == Some("Customize navigation")
                    && w.has::<Focusable>(node)
                {
                    let r = w.get::<Rect>(node).unwrap();
                    assert_eq!((r.w, r.h), (52., 52.));
                    assert!(
                        r.y >= 0. && r.y + r.h <= height,
                        "footer must remain reachable: {r:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn editor_rows_align_and_header_stays_outside_scroll() {
        let mut app = fixture();
        let dir = tempfile::tempdir().unwrap();
        let _channels = crate::workshops::install_fixture(
            app.world_mut(),
            dir.path().join("sources.json"),
            &[("a", "Workshop A")],
        );
        action(app.world_mut(), &HostIntent::Command(PLUS));
        for (width, height) in [(1200., 800.), (760., 560.)] {
            app.world_mut()
                .insert_resource(Viewport::new(width, height));
            for _ in 0..5 {
                app.update();
            }
            let w = app.world();
            let s = w.resource::<Slot>().unwrap().0.as_ref().unwrap();
            assert!(!w.has::<Scroll>(s.root));
            let body = children_of(w, s.root)
                .into_iter()
                .find(|e| w.has::<Scroll>(*e))
                .expect("independent list scroll");
            let body_rect = w.get::<Rect>(body).unwrap();
            let panel_rect = w.get::<Rect>(s.root).unwrap();
            assert!(
                body_rect.h > 100.
                    && body_rect.y + body_rect.h <= panel_rect.y + panel_rect.h + 0.1
            );
            let mut action_x = None;
            for row in children_of(w, body) {
                let controls: Vec<_> = children_of(w, row)
                    .into_iter()
                    .filter(|e| w.has::<Focusable>(*e))
                    .collect();
                if controls.len() != 3 {
                    continue;
                }
                let r = w.get::<Rect>(row).unwrap();
                assert_eq!(r.h, 44.);
                let xs: Vec<_> = controls
                    .iter()
                    .map(|e| {
                        let rect = w.get::<Rect>(*e).unwrap();
                        assert!(rect.x >= r.x && rect.x + rect.w <= r.x + r.w + 0.1);
                        assert!(rect.y >= r.y && rect.y + rect.h <= r.y + r.h + 0.1);
                        rect.x
                    })
                    .collect();
                if let Some(previous) = &action_x {
                    assert_eq!(&xs, previous);
                }
                action_x = Some(xs);
            }
            assert!(action_x.is_some());
        }
    }
    #[test]
    fn editor_reorders_both_directions_without_moving_safety_doors() {
        let mut app = fixture();
        action(app.world_mut(), &HostIntent::Command(PLUS));
        with(app.world_mut(), |w, s| {
            render(w, s);
            let ordered: Vec<_> = nav_destinations(s)
                .into_iter()
                .filter(|(id, visible)| {
                    *visible
                        && crate::nav_layout::toggleable(id)
                        && crate::surfaces::DESTINATIONS
                            .iter()
                            .any(|(_, key, _)| key == id)
                })
                .collect();
            let first = ordered[0].0.clone();
            let second = ordered[1].0.clone();
            let id = *s
                .actions
                .iter()
                .find(|(_, a)| matches!(a, Action::Down(key) if key == &first))
                .unwrap()
                .0;
            s.nav_commands.clear();
            // Invoke after returning State to its slot below.
            w.insert_resource(TestEditorAction(id, first, second));
        });
        let TestEditorAction(id, first, second) =
            app.world().resource::<TestEditorAction>().unwrap().clone();
        action(app.world_mut(), &HostIntent::Command(id));
        with(app.world_mut(), |w, s| {
            let command = s.nav_commands.pop().unwrap();
            crate::nav_layout::mutate(&mut s.snapshot.as_mut().unwrap().spec, &command.operation)
                .unwrap();
            let order = nav_destinations(s);
            assert!(
                order.iter().position(|(key, _)| key == &second)
                    < order.iter().position(|(key, _)| key == &first)
            );
            s.nav_pending = false;
            render(w, s);
        });
        let up = {
            let s = app.world().resource::<Slot>().unwrap().0.as_ref().unwrap();
            *s.actions
                .iter()
                .find(|(_, a)| matches!(a, Action::Up(key) if key == &first))
                .unwrap()
                .0
        };
        action(app.world_mut(), &HostIntent::Command(up));
        with(app.world_mut(), |_, s| {
            let command = s.nav_commands.pop().unwrap();
            assert!(
                matches!(command.operation, crate::nav_layout::Operation::Reorder { before_surface_id: Some(ref target), .. } if target == &second)
            );
        });
    }
    #[derive(Clone)]
    struct TestEditorAction(u64, String, String);
    #[test]
    fn personal_rail_visual_acceptance() {
        if std::env::var_os("MEDOUSA_RENDER_BOTS").is_none() {
            return;
        }
        use std::sync::Arc;
        let mut app = fixture();
        let dir = tempfile::tempdir().unwrap();
        let _channels = crate::workshops::install_fixture(
            app.world_mut(),
            dir.path().join("sources.json"),
            &[("a", "Workshop A")],
        );
        with(app.world_mut(), |w, s| {
            for (n, avatar) in [(2, "seahorse-happy"), (3, "starfish-sweet")] {
                let mut b = bot();
                b.bot_id = format!("bot_{n:032x}").parse().unwrap();
                b.display_name = format!("Companion {n}");
                b.avatar_ref = Some(avatar.into());
                b.primary_session_id = Some(format!("companion-{n}"));
                crate::nav_layout::mutate(
                    &mut s.snapshot.as_mut().unwrap().spec,
                    &crate::nav_layout::Operation::SetBotPinned {
                        bot_id: b.bot_id.to_string(),
                        pinned: true,
                    },
                )
                .unwrap();
                s.bots.push(b);
            }
            render(w, s);
        });
        app.world_mut()
            .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
        app.world_mut()
            .insert_resource(rndr_ui::InteractionMotion::snap());
        let mut exporter = rndr_render::Exporter::new().unwrap();
        for (width, height, scale, dark, name) in [
            (1200, 1000, 1., true, "dark"),
            (760, 1000, 2., true, "retina"),
            (1200, 1000, 1., false, "light"),
        ] {
            app.world_mut().insert_resource(Viewport {
                width: width as f32,
                height: height as f32,
                scale,
                ..Default::default()
            });
            publish_system_appearance(
                app.world_mut(),
                SystemAppearance {
                    dark: Some(dark),
                    ..Default::default()
                },
            );
            for _ in 0..8 {
                app.update();
            }
            let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                exporter
                    .render_frame(
                        Some(world),
                        frame,
                        (width as f32 * scale) as u32,
                        (height as f32 * scale) as u32,
                        scale,
                    )
                    .unwrap()
            });
            std::fs::create_dir_all("verification").unwrap();
            image::save_buffer(
                format!("verification/native-personal-rail-{name}.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    #[test]
    fn navigation_editor_and_personalization_render() {
        if std::env::var_os("MEDOUSA_RENDER_BOTS").is_none() {
            return;
        }
        let mut app = fixture();
        // Keep synthetic binding stable during render; production binding comes from the active chat owner.
        let dir = tempfile::tempdir().unwrap();
        let _channels = crate::workshops::install_fixture(
            app.world_mut(),
            dir.path().join("sources.json"),
            &[("a", "Workshop A")],
        );
        action(app.world_mut(), &HostIntent::Command(PLUS));
        with(app.world_mut(), |w, s| {
            for (n, name, key) in [
                (2, "Uzzy", "jellyfish-happy"),
                (3, "Eddy", "seahorse-default"),
                (
                    4,
                    "A companion with a deliberately long display name",
                    "starfish-default",
                ),
            ] {
                let mut b = bot();
                b.bot_id = format!("bot_{n:032x}").parse().unwrap();
                b.display_name = name.into();
                b.avatar_ref = Some(key.into());
                s.bots.push(b);
            }
            render(w, s);
        });
        crate::ui::visual_tests::render_dark(&mut app, "native-bots-navigation");
        let s = app.world().resource::<Slot>().unwrap().0.as_ref().unwrap();
        eprintln!(
            "BOT DEBUG open={} dirty={} root={:?} hidden={} rect={:?} parent={:?}",
            s.open,
            s.dirty,
            s.root,
            app.world().has::<Hidden>(s.root),
            app.world().get::<Rect>(s.root),
            app.world().get::<Parent>(s.root)
        );
        assert!(s.open && !app.world().has::<Hidden>(s.root));
        let mut ancestor = s.root;
        while let Some(parent) = app.world().get::<Parent>(ancestor) {
            ancestor = parent.entity;
            assert!(
                !app.world().has::<Hidden>(ancestor),
                "hidden ancestor {ancestor:?}"
            );
        }
        let rect = app.world().get::<Rect>(s.root).unwrap();
        assert!(rect.w > 300. && rect.x < 1200., "{rect:?}");
        action(app.world_mut(), &HostIntent::Command(NEW));
        crate::ui::visual_tests::render_dark(&mut app, "native-bots-customize");
    }
}
