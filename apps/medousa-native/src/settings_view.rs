//! Settings layout: medousa-home's section list on the left, the chosen
//! section in a reading column on the right.
use super::*;
use crate::native_style as visual;
use medousa_types::ExternalProvider;
use rndr_ui::{Token, Typography, UiColor, UiStyle, text};

const TILE_W: f32 = 148.;
const TILE_H: f32 = 76.;
/// Package progress bar length.
const BAR: f32 = 320.;

fn panel(w: &mut World, parent: Entity, layout: Layout) -> Entity {
    let e = w.spawn((layout,));
    set_parent(w, e, parent);
    e
}

pub(super) fn mount(w: &mut World, root: Entity) -> (Entity, Entity) {
    let nav = panel(
        w,
        root,
        Layout {
            width: Size::Px(232.),
            height: Size::Fill,
            gap: 2.,
            ..Layout::column()
        },
    );
    visual::navigation_frame(w, nav);
    w.insert(nav, Scroll::default());
    let scroll = panel(
        w,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.,
            ..Layout::column()
        },
    );
    w.insert(scroll, Scroll::default());
    w.insert(
        scroll,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    let body = panel(
        w,
        scroll,
        Layout {
            width: Size::Px(720.),
            height: Size::Hug,
            padding: 28.,
            gap: 10.,
            ..Layout::column()
        },
    );
    (nav, body)
}

fn label(w: &mut World, p: Entity, value: &str, kind: Typography) -> Entity {
    let e = text(w, p, value, kind).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
    e
}

fn quiet(w: &mut World, p: Entity, value: &str) -> Entity {
    let e = label(w, p, value, Typography::Secondary);
    w.insert(
        e,
        UiStyle {
            foreground: Some(Token::Tertiary.into()),
            ..Default::default()
        },
    );
    e
}

fn heading(w: &mut World, p: Entity, value: &str) {
    panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Px(6.),
            ..Layout::row()
        },
    );
    label(w, p, value, Typography::Emphasis);
}

fn button(
    w: &mut World,
    p: Entity,
    s: &mut State,
    name: &str,
    a: Action,
    enabled: bool,
) -> rndr_ui::controls::ControlHandle {
    let id = next();
    let h = visual::document_action(w, p, id, name, enabled);
    s.actions.insert(id, a);
    h
}

/// The chosen row or tile sits on the selected wash, face and border alike.
fn mark(w: &mut World, root: Entity) {
    if let Some(face) = visual::label_face(w, root) {
        visual::plane(w, face, Token::Selected);
        if let Some(border) = w.get::<Parent>(face).map(|p| p.entity)
            && border != root
        {
            visual::plane(w, border, Token::Selected);
        }
    }
    if let Some(sem) = w.get_mut::<Semantics>(root) {
        sem.description = "Selected".into();
    }
}

fn nav(w: &mut World, s: &mut State) {
    let nav = s.nav;
    label(w, nav, "Settings", Typography::Emphasis);
    let mut group = None;
    for section in Section::ALL {
        if group != Some(section.machine()) {
            group = Some(section.machine());
            let name = if section.machine() {
                "This Mac"
            } else {
                "Medousa"
            };
            let h = label(w, nav, name, Typography::Secondary);
            visual::section_heading(w, h);
        }
        let h = button(w, nav, s, section.label(), Action::Section(section), true);
        w.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
        w.remove::<visual::FitLabel>(h.root);
        if section == s.section {
            mark(w, h.root);
        }
        if let Some(sem) = w.get_mut::<Semantics>(h.root) {
            sem.description = section.hint().into();
        }
    }
}

/// A theme tile: the theme's canvas, chrome and action colors over its name,
/// drawn on a quiet button that keeps focus, hover and activation.
fn tile(w: &mut World, p: Entity, s: &mut State, swatch: &crate::theme::Swatch, dark: bool) {
    let h = button(
        w,
        p,
        s,
        &swatch.label,
        Action::Theme(swatch.id.clone()),
        true,
    );
    w.remove::<visual::FitLabel>(h.root);
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Px(TILE_W),
        height: Size::Px(TILE_H),
        ..Layout::column()
    };
    // The name is drawn below; the label keeps only the accessible name.
    w.insert(
        h.label,
        UiStyle {
            foreground: Some(UiColor::Explicit(Color::rgba(0., 0., 0., 0.))),
            ..Default::default()
        },
    );
    // Every inner layer fills the tile, so hover and the selected wash cover
    // all of it rather than a strip at the top.
    let mut layers = children_of(w, h.root);
    while let Some(layer) = layers.pop() {
        if let Some(l) = w.get_mut::<Layout>(layer) {
            l.width = Size::Fill;
            l.height = Size::Fill;
        }
        layers.extend(children_of(w, layer));
    }
    if s.theme == swatch.id {
        mark(w, h.root);
    }
    let colors = if dark { swatch.dark } else { swatch.light };
    let strip = w.spawn((
        Layout {
            width: Size::Px(TILE_W - 20.),
            height: Size::Px(26.),
            gap: 0.,
            ..Layout::row()
        },
        Absolute {
            left: Some(10.),
            top: Some(10.),
            ..Default::default()
        },
        Radius(6.),
        Border {
            color: Color::rgba(0., 0., 0., 0.18),
            width: 1.,
        },
    ));
    set_parent(w, strip, h.root);
    for (i, color) in colors.into_iter().enumerate() {
        let block = w.spawn((
            Layout {
                width: if i == 2 { Size::Px(28.) } else { Size::Fill },
                height: Size::Fill,
                ..Layout::row()
            },
            color,
        ));
        set_parent(w, block, strip);
    }
    let name = text(w, h.root, &swatch.label, Typography::Secondary).unwrap();
    w.insert(
        name,
        Absolute {
            left: Some(10.),
            top: Some(44.),
            ..Default::default()
        },
    );
    w.insert(name, LabelBox::CONTENT);
}

/// A quiet hairline card around a disclosure, as medousa-home's.
fn card(w: &mut World, root: Entity) {
    let line = visual::color(w, Token::SubtleStroke);
    w.insert(
        root,
        Border {
            color: line,
            width: 1.,
        },
    );
    w.insert(root, visual::SemanticBorder(Token::SubtleStroke));
    w.insert(root, Radius(10.));
}

/// Fills every inner layer of a control so its hover and selected wash
/// cover the whole of it.
fn fill_layers(w: &mut World, root: Entity) {
    let mut layers = children_of(w, root);
    while let Some(layer) = layers.pop() {
        if let Some(l) = w.get_mut::<Layout>(layer) {
            l.width = Size::Fill;
            l.height = Size::Fill;
        }
        layers.extend(children_of(w, layer));
    }
}

/// Draws `value` at a fixed spot inside a control, as part of its face.
#[allow(clippy::too_many_arguments)]
fn pinned(
    w: &mut World,
    root: Entity,
    value: &str,
    kind: Typography,
    left: Option<f32>,
    right: Option<f32>,
    top: f32,
    width: Option<f32>,
    quiet_ink: bool,
) {
    let e = text(w, root, value, kind).unwrap();
    w.insert(
        e,
        Absolute {
            left,
            right,
            top: Some(top),
            ..Default::default()
        },
    );
    w.insert(e, LabelBox::CONTENT);
    let l = w.get_mut::<Layout>(e).unwrap();
    l.width = width.map_or(Size::Hug, Size::Px);
    if width.is_some() {
        w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Ellipsis;
    }
    if quiet_ink {
        w.insert(
            e,
            UiStyle {
                foreground: Some(Token::Tertiary.into()),
                ..Default::default()
            },
        );
    }
}

/// medousa-home's summary trigger: what is chosen now, with Change to open
/// its picker in place. Returns whether it is open.
#[allow(clippy::too_many_arguments)]
fn summary(
    w: &mut World,
    p: Entity,
    s: &mut State,
    key: &'static str,
    kicker: &str,
    title: &str,
    meta: &str,
    swatch: Option<[Color; 3]>,
    enabled: bool,
) -> bool {
    let open = s.open.contains(key);
    let h = button(
        w,
        p,
        s,
        &format!("{kicker}: {title}"),
        Action::Toggle(key),
        enabled,
    );
    w.remove::<visual::FitLabel>(h.root);
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(70.),
        ..Layout::column()
    };
    fill_layers(w, h.root);
    card(w, h.root);
    w.insert(
        h.label,
        UiStyle {
            foreground: Some(UiColor::Explicit(Color::rgba(0., 0., 0., 0.))),
            ..Default::default()
        },
    );
    if open {
        mark(w, h.root);
    }
    if let Some(sem) = w.get_mut::<Semantics>(h.root) {
        sem.description = if open {
            "Close".into()
        } else {
            "Change".into()
        };
    }
    let mut left = 14.;
    if let Some(colors) = swatch {
        let strip = w.spawn((
            Layout {
                width: Size::Px(54.),
                height: Size::Px(30.),
                ..Layout::row()
            },
            Absolute {
                left: Some(14.),
                top: Some(20.),
                ..Default::default()
            },
            Radius(6.),
        ));
        set_parent(w, strip, h.root);
        for (i, color) in colors.into_iter().enumerate() {
            let block = w.spawn((
                Layout {
                    width: if i == 2 { Size::Px(14.) } else { Size::Fill },
                    height: Size::Fill,
                    ..Layout::row()
                },
                color,
            ));
            set_parent(w, block, strip);
        }
        left = 82.;
    }
    pinned(
        w,
        h.root,
        kicker,
        Typography::Secondary,
        Some(left),
        None,
        10.,
        None,
        true,
    );
    pinned(
        w,
        h.root,
        title,
        Typography::Emphasis,
        Some(left),
        None,
        27.,
        Some(480.),
        false,
    );
    if !meta.is_empty() {
        pinned(
            w,
            h.root,
            meta,
            Typography::Secondary,
            Some(left),
            None,
            47.,
            Some(480.),
            true,
        );
    }
    pinned(
        w,
        h.root,
        if open { "Close" } else { "Change" },
        Typography::Secondary,
        None,
        Some(14.),
        26.,
        None,
        true,
    );
    open
}

/// medousa-home's "more" disclosure: a title, what it holds, and a chevron.
/// Returns whether it is open.
fn more(
    w: &mut World,
    p: Entity,
    s: &mut State,
    key: &'static str,
    title: &str,
    meta: &str,
) -> bool {
    let open = s.open.contains(key);
    let h = button(w, p, s, title, Action::Toggle(key), true);
    w.remove::<visual::FitLabel>(h.root);
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(40.),
        ..Layout::column()
    };
    fill_layers(w, h.root);
    card(w, h.root);
    w.insert(
        h.label,
        UiStyle {
            foreground: Some(UiColor::Explicit(Color::rgba(0., 0., 0., 0.))),
            ..Default::default()
        },
    );
    if let Some(sem) = w.get_mut::<Semantics>(h.root) {
        sem.description = format!("{}. {meta}", if open { "Expanded" } else { "Collapsed" });
    }
    pinned(
        w,
        h.root,
        title,
        Typography::Emphasis,
        Some(12.),
        None,
        11.,
        None,
        false,
    );
    if !meta.is_empty() {
        pinned(
            w,
            h.root,
            meta,
            Typography::Secondary,
            None,
            Some(40.),
            12.,
            None,
            true,
        );
    }
    let chevron = if open {
        Icon::ChevronUp
    } else {
        Icon::ChevronDown
    };
    pinned(
        w,
        h.root,
        &chevron.scalar().to_string(),
        Typography::Secondary,
        None,
        Some(14.),
        12.,
        None,
        true,
    );
    open
}

fn look(w: &mut World, s: &mut State, body: Entity) {
    heading(w, body, "Look");
    quiet(
        w,
        body,
        "Applies at once and is saved for this workshop, so Medousa opens in it everywhere. Light and dark follow your system.",
    );
    let dark = w
        .resource::<rndr_ui::UiTheme>()
        .is_some_and(|t| t.appearance == rndr_ui::Appearance::Dark);
    let swatches = crate::theme::swatches();
    let current = swatches.iter().find(|t| t.id == s.theme).cloned();
    let (title, colors) = current.as_ref().map_or((s.theme.clone(), None), |t| {
        (t.label.clone(), Some(if dark { t.dark } else { t.light }))
    });
    let meta = if s.saving.is_some() {
        "Saving…"
    } else {
        "25 themes, shared with every Medousa app"
    };
    if summary(w, body, s, "theme", "Theme", &title, meta, colors, true) {
        let grid = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 8.,
                ..Layout::row()
            },
        );
        w.insert(grid, Wrap { line_gap: 8. });
        for swatch in &swatches {
            tile(w, grid, s, swatch, dark);
        }
    }
    if more(
        w,
        body,
        s,
        "prefs-more",
        "More preferences",
        "Code, activity, proofreading",
    ) {
        quiet(
            w,
            body,
            "Code, task activity, proofreading and everyday options are coming to this app soon. Until then, change them in the original Medousa app.",
        );
    }
}

/// One fact: a quiet name and its value on the same line.
fn fact(w: &mut World, p: Entity, name: &str, value: &str) {
    let row = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 12.,
            ..Layout::row()
        },
    );
    w.insert(
        row,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    let n = text(w, row, name, Typography::Secondary).unwrap();
    w.get_mut::<Layout>(n).unwrap().width = Size::Px(180.);
    w.insert(
        n,
        UiStyle {
            foreground: Some(Token::Tertiary.into()),
            ..Default::default()
        },
    );
    let v = text(w, row, value, Typography::Label).unwrap();
    w.get_mut::<Layout>(v).unwrap().width = Size::Fill;
    w.get_mut::<Label>(v).unwrap().overflow = TextOverflow::Wrap;
}

fn connection(w: &mut World, s: &mut State, body: Entity) {
    let top = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },
    );
    w.insert(
        top,
        Align {
            items: Items::Center,
            justify: Justify::SpaceBetween,
        },
    );
    let h = text(w, top, "Workshop", Typography::Emphasis).unwrap();
    w.get_mut::<Layout>(h).unwrap().width = Size::Fill;
    let reading = !s.reading.is_empty();
    button(w, top, s, "Refresh", Action::Refresh, !reading);
    match s.workshop.clone() {
        Some(ws) => {
            fact(w, body, "Name", &ws.label);
            let kind = if ws.kind == "local" {
                "On this Mac".to_string()
            } else {
                format!("Remote ({})", ws.kind)
            };
            fact(w, body, "Where", &kind);
            fact(w, body, "Address", &ws.url);
            if let Some(dir) = ws.data_dir.filter(|d| !d.trim().is_empty()) {
                fact(w, body, "Files", &dir);
            }
        }
        None if reading => {
            quiet(w, body, "Reading…");
        }
        None => {
            quiet(w, body, "No active workshop is set up.");
        }
    }
    heading(w, body, "Engine");
    match s.health.clone() {
        Some(h) => {
            fact(w, body, "Status", &h.status);
            let build = if h.runtime.build_revision.trim().is_empty() {
                h.runtime.product_version.clone()
            } else {
                format!(
                    "{} ({})",
                    h.runtime.product_version, h.runtime.build_revision
                )
            };
            fact(w, body, "Version", &build);
            let detail = format!("{} tools · {}", h.tool_registry_count, h.backend);
            if more(w, body, s, "engine-more", "Engine details", &detail) {
                if !h.active_profile_display_name.is_empty() {
                    fact(w, body, "Profile", &h.active_profile_display_name);
                }
                fact(w, body, "Tools", &h.tool_registry_count.to_string());
                if let Some(ms) = h.last_agent_turn_latency_ms {
                    fact(w, body, "Last reply", &format!("{ms} ms"));
                }
                fact(w, body, "Backend", &h.backend);
            }
        }
        None if reading => {
            quiet(w, body, "Checking the engine…");
        }
        None => {
            quiet(
                w,
                body,
                "The engine didn’t answer. Check that the workshop is running, then refresh.",
            );
        }
    }
    heading(w, body, "This app");
    fact(w, body, "Version", env!("CARGO_PKG_VERSION"));
    quiet(
        w,
        body,
        "Switching, adding or repairing workshops still happens in the original Medousa app for now.",
    );
}

/// A titled choice with a hint, as medousa-home's preference choices.
#[allow(clippy::too_many_arguments)]
fn choice(
    w: &mut World,
    p: Entity,
    s: &mut State,
    name: &str,
    hint: &str,
    a: Action,
    chosen: bool,
    enabled: bool,
) {
    let h = button(w, p, s, name, a, enabled);
    w.remove::<visual::FitLabel>(h.root);
    w.get_mut::<Layout>(h.root).unwrap().width = Size::Px(200.);
    if chosen {
        mark(w, h.root);
    }
    if let Some(sem) = w.get_mut::<Semantics>(h.root) {
        sem.description = if chosen {
            format!("Selected. {hint}")
        } else {
            hint.to_string()
        };
    }
}

fn agent_section(w: &mut World, s: &mut State, body: Entity) {
    use super::agent::{self as a, Patch};
    let Some(defaults) = s.defaults.clone() else {
        quiet(
            w,
            body,
            if s.defaults_reading.is_some() {
                "Reading…"
            } else {
                "The engine didn’t answer. Check the workshop, then come back."
            },
        );
        return;
    };
    let stance = a::stance(&defaults);
    let depth = a::depth(&defaults);
    let stances = a::stances(&defaults);
    let main = a::role(&defaults, "main");
    let stance_name = stances
        .iter()
        .find(|(id, ..)| *id == stance)
        .map_or(stance.clone(), |(_, n, _)| n.clone());
    let depth_name = a::DEPTHS
        .iter()
        .find(|(id, ..)| *id == depth)
        .map_or(depth.clone(), |(_, n, _)| n.to_string());
    let model_name = main
        .as_ref()
        .map_or("No model chosen".into(), |(_, m)| m.clone());
    quiet(
        w,
        body,
        &format!("{stance_name} · {depth_name} · {model_name}"),
    );
    let idle = s.patching.is_none();

    heading(w, body, "Answers");
    quiet(
        w,
        body,
        "Stance and how deep she goes. Saved for this workshop.",
    );
    let stance_hint = stances
        .iter()
        .find(|(id, ..)| *id == stance)
        .map(|(_, _, h)| h.clone())
        .unwrap_or_default();
    if summary(
        w,
        body,
        s,
        "stance",
        "Stance",
        &stance_name,
        &stance_hint,
        None,
        idle,
    ) {
        let grid = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 6.,
                ..Layout::row()
            },
        );
        w.insert(grid, Wrap { line_gap: 6. });
        for (id, name, hint) in &stances {
            choice(
                w,
                grid,
                s,
                name,
                hint,
                Action::Agent(Patch::Stance(id.clone())),
                *id == stance,
                idle,
            );
        }
    }
    let depth_hint = a::DEPTHS
        .iter()
        .find(|(id, ..)| *id == depth)
        .map_or("", |(_, _, h)| h)
        .to_string();
    if summary(
        w,
        body,
        s,
        "depth",
        "Depth",
        &depth_name,
        &depth_hint,
        None,
        idle,
    ) {
        let grid = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 6.,
                ..Layout::row()
            },
        );
        w.insert(grid, Wrap { line_gap: 6. });
        for (id, name, hint) in a::DEPTHS {
            choice(
                w,
                grid,
                s,
                name,
                hint,
                Action::Agent(Patch::Depth(id.into())),
                id == depth,
                idle,
            );
        }
    }

    heading(w, body, "Models & providers");
    quiet(
        w,
        body,
        "Who answers. The other roles follow it unless set apart.",
    );
    let provider = main.as_ref().map_or(String::new(), |(p, _)| p.clone());
    if summary(
        w,
        body,
        s,
        "model",
        "Conversation model",
        &model_name,
        &provider,
        None,
        idle,
    ) {
        let models = crate::ui::models(w);
        if models.is_empty() {
            quiet(
                w,
                body,
                "No models are available yet. Add a key under Provider keys below.",
            );
        }
        for (provider, model) in models {
            let chosen = main
                .as_ref()
                .is_some_and(|(p, m)| *p == provider && *m == model);
            let h = button(
                w,
                body,
                s,
                &format!("{model} · {provider}"),
                Action::Agent(Patch::Model(provider.clone(), model.clone())),
                idle,
            );
            w.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
            w.remove::<visual::FitLabel>(h.root);
            if chosen {
                mark(w, h.root);
            }
        }
    }
    let roles: Vec<(&str, String)> = [
        ("vision", "Sees images"),
        ("imageGeneration", "Makes images"),
        ("stt", "Listens"),
    ]
    .into_iter()
    .map(|(key, name)| {
        (
            name,
            a::role(&defaults, key)
                .map_or("Same as conversation".into(), |(p, m)| format!("{m} · {p}")),
        )
    })
    .collect();
    let set_apart = roles
        .iter()
        .filter(|(_, v)| v != "Same as conversation")
        .count();
    let meta = match set_apart {
        0 => "All follow the conversation model".to_string(),
        1 => "1 set apart".into(),
        k => format!("{k} set apart"),
    };
    if more(w, body, s, "roles", "Other roles", &meta) {
        for (name, value) in &roles {
            fact(w, body, name, value);
        }
    }
    if s.patching.is_some() {
        quiet(w, body, "Saving…");
    }
    let keys = match s.providers.as_ref().map(|r| r.keyed()) {
        Some(0) => "No keys yet".to_string(),
        Some(1) => "1 provider with a key".into(),
        Some(n) => format!("{n} providers with keys"),
        None if s.providers_reading.is_some() => "Reading…".into(),
        None => String::new(),
    };
    if more(w, body, s, "providers", "Provider keys", &keys) {
        provider_keys(w, s, body);
    }
    agent_extras(w, s, body, &defaults, idle);
}

/// Fallback models, coding runtime, stage routing, mode suggestions and
/// artifact cleanup, each behind its own disclosure.
fn agent_extras(
    w: &mut World,
    s: &mut State,
    body: Entity,
    defaults: &serde_json::Value,
    idle: bool,
) {
    use super::agent as a;
    // Fallback models.
    let fallbacks = a::fallbacks(defaults);
    let meta = match fallbacks.len() {
        0 => "None".to_string(),
        n => format!("{n} set"),
    };
    if more(w, body, s, "fallbacks", "Fallback models", &meta) {
        quiet(
            w,
            body,
            "Tried in order when the conversation model can’t answer.",
        );
        for (i, slot) in a::FALLBACK_SLOTS.iter().enumerate() {
            // A second fallback follows a first.
            if i > fallbacks.len() {
                break;
            }
            let current = fallbacks.get(i);
            let title = if i == 0 {
                "First fallback"
            } else {
                "Second fallback"
            };
            let side = runtime_row(
                w,
                body,
                title,
                &current.map_or("None".to_string(), |(p, m)| format!("{m} · {p}")),
            );
            let open = s.open.contains(slot);
            named_button(
                w,
                side,
                s,
                if open { "Close" } else { "Change" },
                format!("Change {}", title.to_lowercase()),
                Action::Toggle(slot),
                idle,
            );
            if !open {
                continue;
            }
            if current.is_some() {
                let h = button(
                    w,
                    body,
                    s,
                    "None",
                    Action::Agent(a::Patch::Fallback(i, None)),
                    idle,
                );
                w.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
                w.remove::<visual::FitLabel>(h.root);
            }
            for (provider, model) in crate::ui::models(w) {
                let h = button(
                    w,
                    body,
                    s,
                    &format!("{model} · {provider}"),
                    Action::Agent(a::Patch::Fallback(
                        i,
                        Some((provider.clone(), model.clone())),
                    )),
                    idle,
                );
                w.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
                w.remove::<visual::FitLabel>(h.root);
                if current == Some(&(provider, model)) {
                    mark(w, h.root);
                }
            }
        }
    }

    // Coding runtime.
    let (preferred, coding_fallbacks) = a::coding(defaults);
    let all = a::coding_runtimes();
    let name = |id: &str| {
        all.iter()
            .find(|(i, _)| *i == id)
            .map_or(id.to_string(), |(_, l)| l.to_string())
    };
    if more(w, body, s, "coding", "Coding runtime", &name(&preferred)) {
        quiet(
            w,
            body,
            "Chosen once for future coding assignments. A runtime you ask for in chat takes priority.",
        );
        let set = |p: &str, f: &[String]| {
            Action::Agent(a::Patch::Field("coding_runtime", a::coding_value(p, f)))
        };
        let row = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 6.,
                ..Layout::row()
            },
        );
        w.insert(row, Wrap { line_gap: 6. });
        for (id, label_) in &all {
            let rest: Vec<String> = coding_fallbacks
                .iter()
                .filter(|f| f != id)
                .cloned()
                .collect();
            let h = button(w, row, s, label_, set(id, &rest), idle);
            if *id == preferred {
                mark(w, h.root);
            }
        }
        label(w, body, "Fallbacks", Typography::Label);
        quiet(
            w,
            body,
            if coding_fallbacks.is_empty() {
                "None — Medousa reports the blocker if the preferred runtime is unavailable."
            } else {
                "Tried in order before work starts."
            },
        );
        for (i, f) in coding_fallbacks.iter().enumerate() {
            let (line, col) = split_row(w, body);
            label(
                w,
                col,
                &format!("{}. {}", i + 1, name(f)),
                Typography::Label,
            );
            let mut up = coding_fallbacks.clone();
            let mut down = coding_fallbacks.clone();
            if i > 0 {
                up.swap(i, i - 1);
            }
            if i + 1 < coding_fallbacks.len() {
                down.swap(i, i + 1);
            }
            let without: Vec<String> = coding_fallbacks
                .iter()
                .filter(|x| *x != f)
                .cloned()
                .collect();
            named_button(
                w,
                line,
                s,
                "↑",
                format!("Move {} earlier", name(f)),
                set(&preferred, &up),
                idle && i > 0,
            );
            named_button(
                w,
                line,
                s,
                "↓",
                format!("Move {} later", name(f)),
                set(&preferred, &down),
                idle && i + 1 < coding_fallbacks.len(),
            );
            named_button(
                w,
                line,
                s,
                "Remove",
                format!("Remove {} fallback", name(f)),
                set(&preferred, &without),
                idle,
            );
        }
        let remaining: Vec<_> = all
            .iter()
            .filter(|(id, _)| *id != preferred && !coding_fallbacks.iter().any(|f| f == id))
            .collect();
        if !remaining.is_empty() {
            let row = panel(
                w,
                body,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 6.,
                    ..Layout::row()
                },
            );
            w.insert(row, Wrap { line_gap: 6. });
            for (id, label_) in remaining {
                let mut more_f = coding_fallbacks.clone();
                more_f.push(id.to_string());
                named_button(
                    w,
                    row,
                    s,
                    &format!("+ {label_}"),
                    format!("Add {label_} fallback"),
                    set(&preferred, &more_f),
                    idle,
                );
            }
        }
    }

    // Stage routing.
    let stage_name = a::STAGES
        .iter()
        .find(|(k, _)| *k == s.stage)
        .map_or("Lead", |(_, n)| n);
    if more(w, body, s, "stages", "Stage routing", stage_name) {
        quiet(
            w,
            body,
            "A stage is one step in how a turn is built. Pick one to set its model.",
        );
        match a::stage(defaults, s.stage) {
            None => {
                quiet(
                    w,
                    body,
                    "This workshop hasn’t set up stage routes yet; they appear once it has.",
                );
            }
            Some((provider, model, chain)) => {
                let row = panel(
                    w,
                    body,
                    Layout {
                        width: Size::Fill,
                        height: Size::Hug,
                        gap: 6.,
                        ..Layout::row()
                    },
                );
                w.insert(row, Wrap { line_gap: 6. });
                for (key, n) in a::STAGES {
                    let h = button(w, row, s, n, Action::Stage(key), true);
                    if key == s.stage {
                        mark(w, h.root);
                    }
                }
                s.drafts.entry("stage.provider").or_insert(provider);
                s.drafts.entry("stage.model").or_insert(model);
                s.drafts.entry("stage.chain").or_insert(chain.join(", "));
                field(
                    w,
                    body,
                    s,
                    "stage.provider",
                    "Provider",
                    "",
                    "openai",
                    false,
                );
                field(w, body, s, "stage.model", "Model", "", "", false);
                field(
                    w,
                    body,
                    s,
                    "stage.chain",
                    "Fallback chain",
                    "provider:model, separated by commas.",
                    "provider:model, …",
                    false,
                );
                save_row_stage(w, body, s, &format!("Save {stage_name} stage"), idle);
            }
        }
    }

    // Mode suggestions.
    let mode_meta = match &s.mode_policy {
        Some(Ok(p)) => match p.auto_accept {
            medousa_types::AgentModeAutoAccept::Never => "Ask me",
            medousa_types::AgentModeAutoAccept::Task => "Task-scoped only",
            medousa_types::AgentModeAutoAccept::All => "Accept all",
        }
        .to_string(),
        Some(Err(_)) => "Unavailable".into(),
        None => String::new(),
    };
    if more(w, body, s, "modes", "Mode suggestions", &mode_meta) {
        match s.mode_policy.clone() {
            Some(Ok(policy)) => {
                use medousa_types::AgentModeAutoAccept as Auto;
                let free = s.mode_busy.is_none();
                quiet(w, body, "How much transition authority Medousa has.");
                let row = panel(
                    w,
                    body,
                    Layout {
                        width: Size::Fill,
                        height: Size::Hug,
                        gap: 6.,
                        ..Layout::row()
                    },
                );
                for (auto, name, hint) in [
                    (Auto::Never, "Never — ask me", "Every switch waits for you"),
                    (
                        Auto::Task,
                        "Task-scoped only",
                        "Accepts switches inside a task",
                    ),
                    (Auto::All, "All suggestions", "Accepts every switch"),
                ] {
                    let mut next = policy.clone();
                    next.auto_accept = auto;
                    choice(
                        w,
                        row,
                        s,
                        name,
                        hint,
                        Action::ModePolicy(next),
                        policy.auto_accept == auto,
                        free,
                    );
                }
                let secs = policy.proposal_ttl_seconds;
                let step = |dir: i64| {
                    super::runtime::preset(&MODE_TTLS, secs as i64, dir).map(|v| {
                        let mut next = policy.clone();
                        next.proposal_ttl_seconds = v as u64;
                        Action::ModePolicy(next)
                    })
                };
                stepper(
                    w,
                    body,
                    s,
                    "Suggestion expiry",
                    "5 seconds to 24 hours",
                    &duration(secs),
                    step(-1),
                    step(1),
                    free,
                );
            }
            Some(Err(_)) => {
                quiet(w, body, "Mode policy is unavailable on this workshop.");
            }
            None => {
                quiet(w, body, "Reading…");
            }
        }
    }

    // Artifacts.
    let art_meta = match &s.artifacts {
        Some(Ok(a)) if a.settings.enabled => format!("Kept {} days", a.settings.max_age_days),
        Some(Ok(_)) => "Kept until removed".into(),
        Some(Err(_)) => "Unavailable".into(),
        None => String::new(),
    };
    if more(w, body, s, "artifacts", "Artifacts", &art_meta) {
        match s.artifacts.clone() {
            Some(Ok(status)) => {
                let st = status.settings;
                let free = s.artifacts_busy.is_none();
                switch_row(
                    w,
                    body,
                    s,
                    "Automatic cleanup",
                    "Removes old artifacts on a schedule",
                    st.enabled,
                    Action::Artifacts(!st.enabled, st.max_age_days, st.max_per_session),
                    free,
                );
                let days = |dir: i64| {
                    super::runtime::preset(&ARTIFACT_DAYS, st.max_age_days, dir)
                        .map(|v| Action::Artifacts(st.enabled, v, st.max_per_session))
                };
                stepper(
                    w,
                    body,
                    s,
                    "Keep for",
                    "Older artifacts are removed",
                    &format!("{} days", st.max_age_days),
                    days(-1),
                    days(1),
                    free,
                );
                let per = |dir: i64| {
                    super::runtime::preset(&ARTIFACT_PER_CHAT, st.max_per_session as i64, dir)
                        .map(|v| Action::Artifacts(st.enabled, st.max_age_days, v as usize))
                };
                stepper(
                    w,
                    body,
                    s,
                    "Per chat",
                    "Newest are kept",
                    &st.max_per_session.to_string(),
                    per(-1),
                    per(1),
                    free,
                );
                if let Some(summary) = status.last_run_summary {
                    quiet(w, body, &format!("Last cleanup: {summary}"));
                }
            }
            Some(Err(_)) => {
                quiet(w, body, "Artifact cleanup is unavailable on this workshop.");
            }
            None => {
                quiet(w, body, "Reading…");
            }
        }
    }
    if !s.extras_note.is_empty() {
        let note = s.extras_note.clone();
        label(w, body, &note, Typography::Label);
    }
}

/// Suggestion expiry steps, in seconds.
const MODE_TTLS: [i64; 9] = [5, 15, 30, 60, 300, 900, 3600, 21600, 86400];
const ARTIFACT_DAYS: [i64; 8] = [7, 14, 30, 60, 90, 180, 365, 730];
const ARTIFACT_PER_CHAT: [i64; 7] = [25, 50, 100, 200, 500, 1000, 2000];

fn duration(secs: u64) -> String {
    match secs {
        s if s >= 3600 && s % 3600 == 0 => format!("{} h", s / 3600),
        s if s >= 60 && s % 60 == 0 => format!("{} min", s / 60),
        s => format!("{s} s"),
    }
}

/// A value with − and + buttons, which step to the given actions.
#[allow(clippy::too_many_arguments)]
fn stepper(
    w: &mut World,
    p: Entity,
    s: &mut State,
    title: &str,
    hint: &str,
    shown: &str,
    less: Option<Action>,
    more_: Option<Action>,
    enabled: bool,
) {
    let side = runtime_row(w, p, title, hint);
    let none = || Action::Toggle("");
    let has_less = less.is_some();
    let has_more = more_.is_some();
    named_button(
        w,
        side,
        s,
        "−",
        format!("Less {}", title.to_lowercase()),
        less.unwrap_or_else(none),
        enabled && has_less,
    );
    let v = label(w, side, shown, Typography::Label);
    w.get_mut::<Layout>(v).unwrap().width = Size::Hug;
    named_button(
        w,
        side,
        s,
        "+",
        format!("More {}", title.to_lowercase()),
        more_.unwrap_or_else(none),
        enabled && has_more,
    );
}

/// Each provider with its key and base URL; one opens at a time.
fn provider_keys(w: &mut World, s: &mut State, body: Entity) {
    let Some(read) = s.providers.clone() else {
        if !s.provider_note.is_empty() {
            let note = s.provider_note.clone();
            label(w, body, &note, Typography::Label);
        } else {
            quiet(w, body, "Reading provider keys…");
        }
        return;
    };
    quiet(
        w,
        body,
        "Keys are kept by the workshop’s engine and never shown again.",
    );
    let local = s.local();
    for e in super::providers::entries() {
        if e.id == super::providers::CUSTOM && !local {
            continue;
        }
        let name = super::providers::label(&e);
        let open = s.provider_open.as_deref() == Some(e.id.as_str());
        let side = runtime_row(w, body, &name, &read.summary(&e));
        named_button(
            w,
            side,
            s,
            if open { "Close" } else { "Edit" },
            format!("{} {name}", if open { "Close" } else { "Edit" }),
            Action::Provider(e.id.clone()),
            true,
        );
        if open {
            provider_form(w, s, body, &e, &read, &name);
        }
    }
    if !local {
        quiet(
            w,
            body,
            "A custom provider is set up on the workshop’s own machine.",
        );
    }
}

fn provider_form(
    w: &mut World,
    s: &mut State,
    body: Entity,
    e: &medousa_desktop_host::provider_catalog::ProviderCatalogEntry,
    read: &super::providers::Read,
    name: &str,
) {
    use super::providers as p;
    let form = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            padding: 12.,
            ..Layout::column()
        },
    );
    quiet(w, form, &e.blurb);
    let custom = e.id == p::CUSTOM;
    if custom {
        field(
            w,
            form,
            s,
            "provider.id",
            "Provider id",
            "Genai adapter name sent at inference time.",
            "openai",
            false,
        );
    }
    if p::allows_base_url(e) {
        let hint = match e.id.as_str() {
            "ollama" => "Local Ollama, Ollama Cloud, or any OpenAI-compatible Ollama endpoint.",
            "medousa-local" => "Medousa Engine OpenAI-compatible endpoint on this device.",
            _ if custom => "Required — usually ends in /v1.",
            _ => "Override the default endpoint if needed.",
        };
        let placeholder = e.default_base_url.clone().unwrap_or("https://…".into());
        field(
            w,
            form,
            s,
            "provider.url",
            "API base URL",
            hint,
            &placeholder,
            false,
        );
    }
    let has_key = read.has_key(&read.key_kind(e));
    if p::allows_key(e) {
        let hint = match &e.key_hint {
            _ if custom => "Optional for local endpoints — required for most hosted APIs.".into(),
            Some(h) => format!("Example: {h}"),
            None => "Stored by this workshop.".into(),
        };
        let placeholder = if has_key {
            "Leave blank to keep the saved key".to_string()
        } else {
            e.key_hint.clone().unwrap_or("Paste key".into())
        };
        field(
            w,
            form,
            s,
            "provider.key",
            "API key",
            &hint,
            &placeholder,
            true,
        );
    }
    let idle = s.provider_busy.is_none();
    let row = panel(
        w,
        form,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    named_button(
        w,
        row,
        s,
        if s.provider_busy.is_some() {
            "Saving…"
        } else {
            "Save"
        },
        format!("Save {name}"),
        Action::SaveProvider,
        idle,
    );
    if p::allows_key(e) && has_key {
        if s.provider_clear {
            named_button(
                w,
                row,
                s,
                "Cancel",
                format!("Keep {name} key"),
                Action::KeepProviderKey,
                true,
            );
        }
        named_button(
            w,
            row,
            s,
            if s.provider_clear {
                "Clear key?"
            } else {
                "Clear key"
            },
            if s.provider_clear {
                format!("Confirm clearing {name} key")
            } else {
                format!("Clear {name} key")
            },
            Action::ClearProviderKey,
            idle,
        );
    }
    if !s.provider_note.is_empty() {
        let note = s.provider_note.clone();
        label(w, form, &note, Typography::Label);
    }
}

fn accounts(w: &mut World, s: &mut State, body: Entity) {
    quiet(w, body, "Connect the services you use with this workshop.");
    let Some(accounts) = s.accounts.clone() else {
        quiet(
            w,
            body,
            if s.accounts_reading.is_some() {
                "Loading accounts…"
            } else {
                ""
            },
        );
        return;
    };
    if accounts.providers.is_empty() {
        quiet(
            w,
            body,
            "Install a review provider in Packages to connect an account.",
        );
    }
    let idle = s.account_busy.is_none() && s.sign_in.is_none();
    for provider in &accounts.providers {
        heading(w, body, &provider.label);
        let top = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 8.,
                ..Layout::row()
            },
        );
        w.insert(
            top,
            Align {
                items: Items::Center,
                justify: Justify::Start,
            },
        );
        let lead = text(
            w,
            top,
            "Browse and share reviews in Medousa.",
            Typography::Secondary,
        )
        .unwrap();
        w.get_mut::<Layout>(lead).unwrap().width = Size::Fill;
        let host = provider.hosts.first().cloned().unwrap_or_default();
        let starting = s
            .account_busy
            .as_ref()
            .is_some_and(|(_, who)| *who == provider.id)
            && s.sign_in.is_none();
        let verb = if starting {
            "Waiting for browser…".to_string()
        } else {
            format!("Sign in with {}", provider.label)
        };
        button(
            w,
            top,
            s,
            &verb,
            Action::SignIn(provider.id.clone(), host),
            idle,
        );
        for account in accounts
            .connections
            .iter()
            .filter(|a| a.kind == provider.id)
        {
            let line = panel(
                w,
                body,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    padding: 6.,
                    gap: 12.,
                    ..Layout::row()
                },
            );
            w.insert(
                line,
                Align {
                    items: Items::Center,
                    justify: Justify::Start,
                },
            );
            let col = panel(
                w,
                line,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 2.,
                    ..Layout::column()
                },
            );
            let name = account
                .label
                .clone()
                .unwrap_or_else(|| provider.label.clone());
            label(w, col, &name, Typography::Emphasis);
            if let Some(url) = &account.base_url {
                quiet(w, col, url.trim_start_matches("https://"));
            }
            let id = account.connection_id.to_string();
            let h = button(
                w,
                line,
                s,
                "Disconnect",
                Action::Disconnect(id),
                s.account_busy.is_none(),
            );
            if let Some(sem) = w.get_mut::<Semantics>(h.root) {
                sem.name = Some(format!("Disconnect {name}"));
            }
        }
        if let Some(flow) = s.sign_in.clone().filter(|f| f.provider == provider.id) {
            quiet(
                w,
                body,
                &format!(
                    "Enter this code on {} to connect your account.",
                    provider.label
                ),
            );
            let code = text(w, body, &flow.code, Typography::Title).unwrap();
            w.insert(
                code,
                TextStyle {
                    family: "monospace".into(),
                    size: 22.,
                    letter_spacing: 2.5,
                    ..Default::default()
                },
            );
            let row = panel(
                w,
                body,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 6.,
                    ..Layout::row()
                },
            );
            w.insert(row, Wrap { line_gap: 6. });
            button(
                w,
                row,
                s,
                if flow.copied { "Copied" } else { "Copy code" },
                Action::CopyCode,
                true,
            );
            button(w, row, s, "Open sign-in page", Action::OpenSignIn, true);
            button(w, row, s, "Cancel", Action::CancelSignIn, true);
            if flow.paused {
                button(w, row, s, "Retry connection", Action::RetrySignIn, true);
            } else {
                quiet(
                    w,
                    body,
                    "Waiting for approval… This page updates automatically.",
                );
            }
        }
    }
    if !s.notice.is_empty() {
        let n = s.notice.clone();
        quiet(w, body, &n);
    }
}

/// A row with its content column on the left and controls on the right.
fn split_row(w: &mut World, p: Entity) -> (Entity, Entity) {
    let line = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            padding: 6.,
            gap: 12.,
            ..Layout::row()
        },
    );
    w.insert(
        line,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    let col = panel(
        w,
        line,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 2.,
            ..Layout::column()
        },
    );
    (line, col)
}

/// A button whose spoken name says what it acts on.
fn named_button(
    w: &mut World,
    p: Entity,
    s: &mut State,
    shown: &str,
    spoken: String,
    a: Action,
    enabled: bool,
) {
    let h = button(w, p, s, shown, a, enabled);
    if let Some(sem) = w.get_mut::<Semantics>(h.root) {
        sem.name = Some(spoken);
    }
}

fn agent_row(
    w: &mut World,
    s: &mut State,
    p: Entity,
    agents: &external::Agents,
    agent: &'static external::Agent,
) {
    use external::Status;
    let local = s.local();
    let status = agents.status(agent);
    let waiting = s
        .waiting
        .as_ref()
        .is_some_and(|x| x.account == agent.account);
    let busy = s
        .agent_busy
        .as_ref()
        .is_some_and(|(_, who)| who == agent.account);
    let idle = s.agent_busy.is_none();
    let (line, col) = split_row(w, p);
    label(w, col, agent.label, Typography::Emphasis);
    let note = if status == Status::Missing {
        if local && agent.managed {
            format!(
                "{} isn’t on this workshop. Install it with the vendor’s official installer.",
                agent.cli
            )
        } else {
            format!(
                "{} isn’t on this workshop. Install and set it up on the workshop host, then refresh.",
                agent.cli
            )
        }
    } else if waiting {
        "Finish signing in in the browser or terminal. This updates when the CLI reports you’re signed in.".into()
    } else if status == Status::SignedOut {
        agents.detail(agent)
    } else if agents.ready(agent) {
        external::use_hint(agent)
    } else {
        agent.hint.into()
    };
    quiet(w, col, &note);
    let state = if waiting {
        "Waiting…"
    } else {
        match status {
            Status::Missing => "Not installed",
            Status::SignedIn => "Signed in",
            Status::SignedOut => "Signed out",
            Status::Unknown => "Installed",
        }
    };
    // Status and controls sit in fixed columns so rows line up.
    let pill = text(w, line, state, Typography::Secondary).unwrap();
    w.get_mut::<Layout>(pill).unwrap().width = Size::Px(96.);
    let controls = panel(
        w,
        line,
        Layout {
            width: Size::Px(200.),
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    w.insert(
        controls,
        Align {
            items: Items::Center,
            justify: Justify::End,
        },
    );
    if status != Status::SignedIn {
        w.insert(
            pill,
            UiStyle {
                foreground: Some(Token::Tertiary.into()),
                ..Default::default()
            },
        );
    }
    let name = agent.label;
    if status == Status::Missing {
        if local && agent.managed {
            named_button(
                w,
                controls,
                s,
                if busy { "Installing…" } else { "Install" },
                format!("Install {name}"),
                Action::AgentInstall(agent.account),
                idle,
            );
        }
    } else if agents.ready(agent) {
        named_button(
            w,
            controls,
            s,
            "Open Chat",
            format!("Open Chat with {name}"),
            Action::OpenChat,
            true,
        );
        if local && let Some(out) = agent.sign_out {
            named_button(
                w,
                controls,
                s,
                out,
                format!("{out} {name}"),
                Action::AgentSignOut(agent.account),
                idle,
            );
        }
    } else if local && agent.managed {
        let verb = if waiting {
            "Waiting…"
        } else if busy {
            "Signing in…"
        } else {
            "Sign in"
        };
        named_button(
            w,
            controls,
            s,
            verb,
            format!("Sign in to {name}"),
            Action::AgentSignIn(agent.account),
            idle && !waiting,
        );
    }
}

fn external_agents(w: &mut World, s: &mut State, body: Entity) {
    quiet(
        w,
        body,
        "Connect independent agent runtimes and provider-hosted conversations. Their accounts stay separate from the Medousa Agent and its model providers.",
    );
    let top = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },
    );
    button(
        w,
        top,
        s,
        if s.agents_reading.is_some() {
            "Refreshing…"
        } else {
            "Refresh"
        },
        Action::Refresh,
        s.agents_reading.is_none(),
    );
    let Some(agents) = s.agents.clone() else {
        quiet(w, body, "Loading external agents…");
        return;
    };
    if !s.local() && s.started && s.reading.is_empty() {
        quiet(
            w,
            body,
            "This workshop runs on another machine. Set agents up on its host, then refresh here.",
        );
    }
    heading(w, body, "Coding agents");
    // Agents Medousa can set up, and any the workshop has, lead; the rest
    // wait behind a disclosure.
    let (lead, rest): (Vec<_>, Vec<_>) = external::AGENTS
        .iter()
        .partition(|a| a.managed || agents.status(a) != external::Status::Missing);
    for agent in lead {
        agent_row(w, s, body, &agents, agent);
    }
    if !rest.is_empty() {
        let meta = format!("{} not on this workshop", rest.len());
        if more(w, body, s, "agents.more", "More agents", &meta) {
            for agent in rest {
                agent_row(w, s, body, &agents, agent);
            }
        }
    }
    if !s.agent_note.is_empty() {
        let n = s.agent_note.clone();
        label(w, body, &n, Typography::Secondary);
    }

    heading(w, body, "Connected conversations");
    if let Some((id, key)) = s.callback_key.clone() {
        label(
            w,
            body,
            "Save this callback key now. It is shown once.",
            Typography::Emphasis,
        );
        quiet(
            w,
            body,
            &format!(
                "The agent VM sends authenticated events to /v1/external-conversations/{id}/events with x-medousa-bridge-key and a paired workshop credential."
            ),
        );
        let k = label(w, body, &key, Typography::Label);
        w.insert(
            k,
            TextStyle {
                family: "monospace".into(),
                ..Default::default()
            },
        );
        let row = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 6.,
                ..Layout::row()
            },
        );
        button(w, row, s, "Copy key", Action::CopyCallbackKey, true);
        button(w, row, s, "Done", Action::DismissCallbackKey, true);
    }
    if let Some(e) = &agents.conversations_error {
        quiet(w, body, &format!("Couldn’t load conversations: {e}"));
    }
    let meta = match agents.conversations.len() {
        0 => "None yet".to_string(),
        1 => "1 connected".to_string(),
        n => format!("{n} connected"),
    };
    if !more(
        w,
        body,
        s,
        "conversations",
        "Muse, Instinct, Dots & Grok",
        &meta,
    ) {
        return;
    }
    let idle = s.agent_busy.is_none();
    for (provider, title, what) in external::PROVIDERS {
        let list = agents.of(provider);
        let (_, col) = split_row(w, body);
        label(w, col, title, Typography::Emphasis);
        quiet(
            w,
            col,
            &if list.is_empty() {
                format!("{what} · none connected")
            } else {
                what.to_string()
            },
        );
        for c in list {
            let (line, col) = split_row(w, body);
            w.get_mut::<Layout>(line).unwrap().padding = 6.;
            label(w, col, &c.label, Typography::Label);
            quiet(w, col, &external::place(c));
            if provider == ExternalProvider::GrokBot {
                named_button(
                    w,
                    line,
                    s,
                    "Rotate key",
                    format!("Rotate callback key for {}", c.label),
                    Action::ConversationRotate(c.id.clone()),
                    idle,
                );
            }
            let confirming = s.conversation_confirm.as_deref() == Some(c.id.as_str());
            named_button(
                w,
                line,
                s,
                if confirming {
                    "Confirm removal"
                } else {
                    "Remove"
                },
                format!(
                    "{} {}",
                    if confirming {
                        "Confirm removal of"
                    } else {
                        "Remove"
                    },
                    c.label
                ),
                Action::ConversationRemove(c.id.clone()),
                idle,
            );
            if confirming {
                button(w, line, s, "Cancel", Action::CancelRemove, true);
            }
        }
        match s.conv_form.clone() {
            Some(form) if form.provider == provider => conversation_form(w, s, body, &form, title),
            _ => {
                let row = panel(
                    w,
                    body,
                    Layout {
                        width: Size::Hug,
                        height: Size::Hug,
                        ..Layout::row()
                    },
                );
                named_button(
                    w,
                    row,
                    s,
                    if provider == ExternalProvider::GrokBot {
                        "Add bot"
                    } else {
                        "Add session"
                    },
                    format!("Add {title}"),
                    Action::ConversationAdd(provider),
                    true,
                );
            }
        }
    }
}

/// Adds one conversation, after medousa-home's: Grok and Dots take their
/// details; Muse and Instinct go through the linked WhatsApp.
fn conversation_form(
    w: &mut World,
    s: &mut State,
    body: Entity,
    form: &super::external::ConvForm,
    title: &str,
) {
    let p = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            padding: 12.,
            ..Layout::column()
        },
    );
    let grok = form.provider == ExternalProvider::GrokBot;
    field(
        w,
        p,
        s,
        "conv.label",
        if grok { "Bot label" } else { "Session name" },
        "",
        "",
        false,
    );
    match form.provider {
        ExternalProvider::Dots => {
            quiet(
                w,
                p,
                "Add your dot and the Medousa Slack app to a dedicated channel. The outgoing user token must belong to the dot’s owner and have chat:write access. The Slack adapter is set up under Sharing → Channels.",
            );
            field(
                w,
                p,
                s,
                "conv.target",
                "Slack channel ID",
                "",
                "C0123456789",
                false,
            );
            field(
                w,
                p,
                s,
                "conv.dot_user",
                "Dot Slack member or bot ID",
                "",
                "U0123456789",
                false,
            );
            field(
                w,
                p,
                s,
                "conv.slack_token",
                "Your Slack user token",
                "",
                "xoxp-…",
                true,
            );
        }
        ExternalProvider::GrokBot => {
            field(w, p, s, "conv.target", "Bot name", "", "", false);
            field(
                w,
                p,
                s,
                "conv.webhook_url",
                "Webhook POST URL",
                "",
                "https://…",
                false,
            );
            field(w, p, s, "conv.webhook_key", "Webhook key", "", "", true);
        }
        _ => whatsapp_part(w, s, p, form),
    }
    let row = panel(
        w,
        p,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    let saving = form.saving.is_some();
    let ready = form.provider != ExternalProvider::Muse
        || form
            .discovery
            .as_ref()
            .is_some_and(|d| d.observed_chat_jid.is_some());
    named_button(
        w,
        row,
        s,
        if saving { "Connecting…" } else { "Connect" },
        format!("Connect {title}"),
        Action::ConversationSave,
        !saving && ready,
    );
    named_button(
        w,
        row,
        s,
        "Cancel",
        format!("Cancel adding {title}"),
        Action::ConversationCancel,
        !saving,
    );
    if !form.note.is_empty() {
        let note = form.note.clone();
        label(w, p, &note, Typography::Label);
    }
}

/// The linked WhatsApp for Muse and Instinct: its pairing QR or state, then
/// Instinct's number or Muse's chat discovery.
fn whatsapp_part(w: &mut World, s: &mut State, p: Entity, form: &super::external::ConvForm) {
    use medousa_types::ExternalWhatsAppPairingState as Wa;
    let local = s.local();
    quiet(
        w,
        p,
        &format!(
            "Uses the WhatsApp adapter from Packages, set up under Sharing → Channels with deliver bind 127.0.0.1:7423. {}",
            if local {
                "Medousa starts it when you add a session."
            } else {
                "Start the adapter on the workshop machine."
            }
        ),
    );
    match form
        .pairing
        .as_ref()
        .map(|r| (&r.pairing, &r.adapter_error))
    {
        Some((Ok(pairing), adapter_error)) => {
            match (pairing.state, &pairing.qr) {
                (Wa::QrReady, Some((qw, qh, rgba))) => {
                    if s.wa_loaded != s.wa_image {
                        let _ = rndr_render::load_rgba(w, s.wa_image, *qw, *qh, rgba.clone());
                        s.wa_loaded = s.wa_image;
                    }
                    let mat = w.spawn((
                        Layout {
                            width: Size::Px(244.),
                            height: Size::Px(244.),
                            padding: 12.,
                            ..Layout::column()
                        },
                        Color::rgba(1., 1., 1., 1.),
                        Radius(14.),
                    ));
                    set_parent(w, mat, p);
                    let image = w.spawn((
                        Layout {
                            width: Size::Px(220.),
                            height: Size::Px(220.),
                            ..Layout::column()
                        },
                        ImageView::new(s.wa_image),
                        Semantics {
                            name: Some("WhatsApp Linked Devices pairing QR".into()),
                            ..Default::default()
                        },
                    ));
                    set_parent(w, image, mat);
                    quiet(
                        w,
                        p,
                        "On your phone: WhatsApp → Settings → Linked Devices → Link a Device, then scan this. It refreshes when it expires.",
                    );
                }
                (Wa::Connected, _) => {
                    label(
                        w,
                        p,
                        if form.provider == ExternalProvider::Muse {
                            "WhatsApp linked. Find the Muse chat below."
                        } else {
                            "WhatsApp linked. Enter Instinct’s international number below."
                        },
                        Typography::Label,
                    );
                }
                (Wa::LoggedOut, _) => {
                    label(
                        w,
                        p,
                        "WhatsApp signed out. Save WhatsApp again under Sharing → Channels to restart the adapter and get a new QR.",
                        Typography::Label,
                    );
                }
                _ => {
                    quiet(
                        w,
                        p,
                        "Waiting for the workshop’s WhatsApp pairing QR. If WhatsApp is already linked and this stays blank, update the WhatsApp adapter in Packages and restart it.",
                    );
                }
            }
            if let Some(e) = adapter_error {
                label(
                    w,
                    p,
                    &format!("The WhatsApp adapter couldn’t start: {e}"),
                    Typography::Label,
                );
            }
        }
        Some((Err(e), _)) => {
            quiet(w, p, &format!("Pairing status unavailable: {e}"));
        }
        None => {
            quiet(w, p, "Checking WhatsApp…");
        }
    }
    if form.provider == ExternalProvider::Instinct {
        field(
            w,
            p,
            s,
            "conv.target",
            "Instinct WhatsApp phone number",
            "Include the country code.",
            "+1 555 123 4567",
            false,
        );
        return;
    }
    let row = panel(
        w,
        p,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    let looking = form.discovery_reading.is_none();
    button(
        w,
        row,
        s,
        if form.discovery.is_some() {
            "Restart discovery"
        } else {
            "Find Muse chat"
        },
        Action::MuseFind,
        looking,
    );
    if let Some(d) = form.discovery.clone() {
        if d.observed_chat_jid.is_none() {
            button(w, row, s, "Check for code", Action::MuseCheck, looking);
        }
        let code = label(w, p, &d.challenge, Typography::Emphasis);
        w.insert(
            code,
            TextStyle {
                family: "monospace".into(),
                ..Default::default()
            },
        );
        quiet(
            w,
            p,
            &format!(
                "Send this code in your Muse chat on WhatsApp: {}",
                d.challenge
            ),
        );
        quiet(
            w,
            p,
            if d.observed_chat_jid.is_some() {
                "Chat ID observed. This does not verify delivery to Muse."
            } else {
                "Waiting for the linked adapter to see your code. Discovery expires after five minutes."
            },
        );
    }
}

fn mcp(w: &mut World, s: &mut State, body: Entity) {
    quiet(
        w,
        body,
        "Connect tools from MCP servers. Medousa reaches them through the gateway, which this Mac runs for a local workshop.",
    );
    let Some(status) = s.mcp.clone() else {
        quiet(
            w,
            body,
            if s.mcp_reading.is_some() {
                "Checking the gateway…"
            } else {
                ""
            },
        );
        return;
    };
    let idle = s.mcp_busy.is_none();
    heading(w, body, "Gateway");
    let row = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },
    );
    w.insert(
        row,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    let line = if status.reachable {
        let mut parts = vec!["Running".to_string()];
        if let Some(n) = status.connected_servers {
            parts.push(format!("{n} connected"));
        }
        if let Some(n) = status.catalog_entries {
            parts.push(format!("{n} tools"));
        }
        parts.join(" · ")
    } else {
        "Not running".to_string()
    };
    let v = text(w, row, &line, Typography::Emphasis).unwrap();
    w.get_mut::<Layout>(v).unwrap().width = Size::Fill;
    if status.local {
        let verb = if status.reachable {
            "Restart gateway"
        } else {
            "Start gateway"
        };
        button(w, row, s, verb, Action::McpRestart, idle);
    }
    if !status.reachable && !status.message.is_empty() {
        quiet(w, body, &status.message);
    }
    if s.mcp_busy.is_some_and(|(_, restart)| restart) {
        quiet(w, body, "Restarting… this can take a few seconds.");
    } else if s.mcp_restart_needed {
        quiet(w, body, "Restart the gateway to apply your changes.");
    }
    if !status.local {
        quiet(w, body, "This workshop manages its own MCP servers.");
    }

    heading(w, body, "Your servers");
    if status.servers.is_empty() {
        quiet(w, body, "No servers yet.");
    }
    for server in &status.servers {
        let line = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                padding: 6.,
                gap: 12.,
                ..Layout::row()
            },
        );
        w.insert(
            line,
            Align {
                items: Items::Center,
                justify: Justify::Start,
            },
        );
        let col = panel(
            w,
            line,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 2.,
                ..Layout::column()
            },
        );
        label(w, col, &server.title, Typography::Emphasis);
        let state = match (server.enabled, server.connected) {
            (false, _) => "Off".to_string(),
            (true, true) => format!("Connected · {} tools", server.tool_count),
            (true, false) => "Not connected".to_string(),
        };
        let oauth = s.mcp_oauth.get(&server.id).cloned();
        let state = match &oauth {
            Some(o) if o.connected => format!("{state} · Signed in"),
            _ => state,
        };
        quiet(w, col, &format!("{} · {state}", server.id));
        // Sign-in runs through the engine, wherever the workshop is.
        if let Some(o) = oauth {
            let signing = s.mcp_signin.is_some();
            let (face, spoken, a) = if o.connected {
                (
                    "Sign out",
                    format!("Sign out of {}", server.title),
                    Action::McpSignOut(server.id.clone()),
                )
            } else {
                (
                    "Sign in",
                    format!("Sign in to {}", server.title),
                    Action::McpSignIn(server.id.clone()),
                )
            };
            named_button(w, line, s, face, spoken, a, !signing);
        }
        if !status.local {
            continue;
        }
        let side = panel(
            w,
            line,
            Layout {
                width: Size::Hug,
                height: Size::Hug,
                gap: 4.,
                ..Layout::row()
            },
        );
        w.insert(
            side,
            Align {
                items: Items::Center,
                justify: Justify::End,
            },
        );
        let tools_open = s.mcp_tools_open.as_deref() == Some(server.id.as_str());
        named_button(
            w,
            side,
            s,
            if tools_open { "Hide tools" } else { "Tools" },
            format!("Tools for {}", server.title),
            Action::McpTools(server.id.clone()),
            true,
        );
        named_button(
            w,
            side,
            s,
            "Edit",
            format!("Edit {}", server.title),
            Action::McpEdit(server.id.clone()),
            idle,
        );
        let verb = if server.enabled {
            "Turn off"
        } else {
            "Turn on"
        };
        let h = button(
            w,
            side,
            s,
            verb,
            Action::McpEnable(server.id.clone(), !server.enabled),
            idle,
        );
        if let Some(sem) = w.get_mut::<Semantics>(h.root) {
            sem.name = Some(format!("{verb} {}", server.title));
        }
        let confirming = s.mcp_confirm.as_deref() == Some(server.id.as_str());
        let h = button(
            w,
            side,
            s,
            if confirming { "Remove?" } else { "Remove" },
            Action::McpRemove(server.id.clone()),
            idle,
        );
        if let Some(sem) = w.get_mut::<Semantics>(h.root) {
            sem.name = Some(format!("Remove {}", server.title));
            if confirming {
                sem.description = "Press again to remove".into();
            }
        }
        if confirming {
            w.insert(
                h.label,
                UiStyle {
                    foreground: Some(Token::DangerText.into()),
                    ..Default::default()
                },
            );
        }
        if tools_open {
            mcp_tools(w, s, body, &server.id, idle);
        }
    }
    if status.local {
        match s.mcp_form.clone() {
            Some(form) => mcp_form(w, s, body, &form),
            None => {
                let row = panel(
                    w,
                    body,
                    Layout {
                        width: Size::Hug,
                        height: Size::Hug,
                        ..Layout::row()
                    },
                );
                button(w, row, s, "Add a server", Action::McpAdd, idle);
            }
        }
        quiet(
            w,
            body,
            "Tools turn on or off per server; discovery hints teach Medousa the words you’d use to ask for one.",
        );
    }
    if more(
        w,
        body,
        s,
        "mcp-advanced",
        "Advanced",
        "Address, config and log",
    ) {
        fact(w, body, "Gateway", &status.gateway_url);
        if !status.config_path.is_empty() {
            fact(w, body, "Config", &status.config_path);
        }
        if status.local {
            fact(
                w,
                body,
                "Log",
                &medousa_desktop_host::mcp::log_path().display().to_string(),
            );
        }
    }
}

/// One server's tools: each on or off, with its discovery hints.
fn mcp_tools(w: &mut World, s: &mut State, body: Entity, server: &str, idle: bool) {
    let p = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 4.,
            padding: 12.,
            ..Layout::column()
        },
    );
    let Some((tools, note)) = s.mcp_tools.clone() else {
        quiet(w, p, "Loading tools…");
        return;
    };
    if !note.is_empty() {
        quiet(w, p, &note);
    }
    if tools.is_empty() {
        quiet(
            w,
            p,
            "No tools are visible yet. Connect this server, then look again.",
        );
        return;
    }
    for tool in tools {
        let mut meta = tool.name.clone();
        if tool.enabled && !tool.available {
            meta.push_str(" · Unavailable");
        }
        if !tool.hints.is_empty() {
            meta.push_str(&format!(" · {}", tool.hints.join(", ")));
        }
        let side = runtime_row(w, p, &tool.title, &meta);
        let editing = s.mcp_hint_tool.as_deref() == Some(tool.name.as_str());
        named_button(
            w,
            side,
            s,
            if editing { "Close" } else { "Hints" },
            format!("Hints for {}", tool.title),
            Action::McpHints(tool.name.clone()),
            idle,
        );
        let h = button(
            w,
            side,
            s,
            if tool.enabled { "On" } else { "Off" },
            Action::McpTool(server.to_string(), tool.name.clone(), !tool.enabled),
            idle,
        );
        w.remove::<visual::FitLabel>(h.root);
        w.get_mut::<Layout>(h.root).unwrap().width = Size::Px(64.);
        if tool.enabled {
            mark(w, h.root);
        }
        if let Some(sem) = w.get_mut::<Semantics>(h.root) {
            sem.name = Some(tool.title.clone());
            sem.description = if tool.enabled {
                "On".into()
            } else {
                "Off".into()
            };
        }
        if editing {
            field(
                w,
                p,
                s,
                "mcp.hints",
                "Discovery hints",
                "Words you’d use to ask for this tool, separated by commas.",
                "open a file, read notes",
                false,
            );
            let row = panel(
                w,
                p,
                Layout {
                    width: Size::Hug,
                    height: Size::Hug,
                    ..Layout::row()
                },
            );
            named_button(
                w,
                row,
                s,
                "Save & restart",
                format!("Save hints for {}", tool.title),
                Action::McpSaveHints,
                idle,
            );
        }
    }
}

/// Adds or edits one server: how it is reached, then saves and restarts.
fn mcp_form(w: &mut World, s: &mut State, body: Entity, form: &super::McpForm) {
    use medousa_desktop_host::mcp::Transport;
    let form_panel = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            padding: 12.,
            ..Layout::column()
        },
    );
    let p = form_panel;
    match &form.editing {
        Some(id) => {
            label(w, p, &format!("Edit {id}"), Typography::Emphasis);
        }
        None => {
            label(w, p, "Add a server", Typography::Emphasis);
            field(
                w,
                p,
                s,
                "mcp.id",
                "Server id",
                "Letters, numbers, hyphens and underscores.",
                "files",
                false,
            );
        }
    }
    if form.loading.is_some() {
        quiet(w, p, "Reading…");
        return;
    }
    field(w, p, s, "mcp.title", "Title", "", "Files", false);
    label(w, p, "Connection", Typography::Label);
    let row = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    for (t, name, hint) in [
        (Transport::Stdio, "Local command", "Runs on this Mac"),
        (Transport::Http, "HTTP", "Streamable HTTP endpoint"),
        (Transport::Sse, "SSE", "Server-sent events endpoint"),
    ] {
        choice(
            w,
            row,
            s,
            name,
            hint,
            Action::McpTransport(t),
            form.transport == t,
            true,
        );
    }
    match form.transport {
        Transport::Stdio => {
            field(
                w,
                p,
                s,
                "mcp.command",
                "Command",
                "The program to start.",
                "npx",
                false,
            );
            field(
                w,
                p,
                s,
                "mcp.args",
                "Arguments",
                "Separated by spaces.",
                "-y @modelcontextprotocol/server-filesystem /Users/you",
                false,
            );
        }
        Transport::Http | Transport::Sse => {
            field(
                w,
                p,
                s,
                "mcp.url",
                "URL",
                "",
                "https://mcp.example.com/mcp",
                false,
            );
            let hint = match (form.forget_token, form.has_token) {
                (true, _) => "Forgotten when you save",
                (false, true) => "Kept on this Mac — paste again only to replace",
                (false, false) => "Optional",
            };
            field(w, p, s, "mcp.token", "Bearer token", hint, "", true);
            if form.has_token {
                switch_row(
                    w,
                    p,
                    s,
                    "Forget saved token",
                    "Clears it when you save, unless a new one is pasted",
                    form.forget_token,
                    Action::McpForgetToken,
                    true,
                );
            }
        }
    }
    let row = panel(
        w,
        p,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    let saving = form.saving.is_some();
    named_button(
        w,
        row,
        s,
        if saving {
            "Saving…"
        } else {
            "Save & restart"
        },
        "Save server".into(),
        Action::McpSave,
        s.mcp_busy.is_none(),
    );
    named_button(
        w,
        row,
        s,
        "Cancel",
        "Cancel server".into(),
        Action::McpCancel,
        !saving,
    );
    if !form.note.is_empty() {
        let note = form.note.clone();
        label(w, p, &note, Typography::Label);
    }
}

/// A thin progress bar `percent` full.
/// A picker over one runtime field: its summary, and its choices when open.
#[allow(clippy::too_many_arguments)]
fn runtime_pick(
    w: &mut World,
    p: Entity,
    s: &mut State,
    d: &serde_json::Value,
    pick: &super::runtime::Pick,
    kicker: &str,
    meta: Option<&str>,
    idle: bool,
) {
    use super::agent::Patch;
    let (chosen, name, hint) = *pick.chosen(d);
    if summary(
        w,
        p,
        s,
        pick.key,
        kicker,
        name,
        meta.unwrap_or(hint),
        None,
        idle,
    ) {
        let grid = panel(
            w,
            p,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 6.,
                ..Layout::row()
            },
        );
        w.insert(grid, Wrap { line_gap: 6. });
        for (id, name, hint) in pick.choices {
            choice(
                w,
                grid,
                s,
                name,
                hint,
                Action::Agent(Patch::Field(pick.key, (*id).into())),
                *id == chosen,
                idle,
            );
        }
    }
}

/// A setting's name and what it does, with its control on the right.
fn runtime_row(w: &mut World, p: Entity, label_: &str, hint: &str) -> Entity {
    let (line, col) = split_row(w, p);
    label(w, col, label_, Typography::Label);
    quiet(w, col, hint);
    let side = panel(
        w,
        line,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    w.insert(
        side,
        Align {
            items: Items::Center,
            justify: Justify::End,
        },
    );
    side
}

/// A Save button for the stage routing form.
fn save_row_stage(w: &mut World, p: Entity, s: &mut State, spoken: &str, enabled: bool) {
    let row = panel(
        w,
        p,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            ..Layout::row()
        },
    );
    named_button(w, row, s, "Save", spoken.into(), Action::SaveStage, enabled);
}

/// A Save button for a runtime list field, under its text field.
#[allow(clippy::too_many_arguments)]
fn save_row(
    w: &mut World,
    p: Entity,
    s: &mut State,
    spoken: &str,
    key: &'static str,
    draft: &'static str,
    enabled: bool,
) {
    let row = panel(
        w,
        p,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            ..Layout::row()
        },
    );
    named_button(
        w,
        row,
        s,
        "Save",
        spoken.into(),
        Action::SaveList(key, draft),
        enabled,
    );
}

/// An on/off setting: its name and hint, with an On/Off button.
#[allow(clippy::too_many_arguments)]
fn switch_row(
    w: &mut World,
    p: Entity,
    s: &mut State,
    label_: &str,
    hint: &str,
    on: bool,
    a: Action,
    enabled: bool,
) {
    let side = runtime_row(w, p, label_, hint);
    let h = button(w, side, s, if on { "On" } else { "Off" }, a, enabled);
    w.remove::<visual::FitLabel>(h.root);
    w.get_mut::<Layout>(h.root).unwrap().width = Size::Px(64.);
    if on {
        mark(w, h.root);
    }
    if let Some(sem) = w.get_mut::<Semantics>(h.root) {
        sem.name = Some(label_.into());
        sem.description = if on { "On".into() } else { "Off".into() };
    }
}

fn runtime_flag(
    w: &mut World,
    p: Entity,
    s: &mut State,
    d: &serde_json::Value,
    flag: &super::runtime::Flag,
    idle: bool,
) {
    let on = flag.on(d);
    switch_row(
        w,
        p,
        s,
        flag.label,
        flag.hint,
        on,
        Action::Agent(super::agent::Patch::Field(flag.key, (!on).into())),
        idle,
    );
}

fn runtime_num(
    w: &mut World,
    p: Entity,
    s: &mut State,
    d: &serde_json::Value,
    num: &super::runtime::Num,
    idle: bool,
) {
    use super::agent::Patch;
    let side = runtime_row(w, p, num.label, num.hint);
    for (dir, glyph, verb) in [(-1, "−", "Lower"), (1, "+", "Raise")] {
        let next = num.stepped(d, dir);
        let enabled = idle && next.is_some();
        let a = Action::Agent(Patch::Field(
            num.key,
            next.unwrap_or(serde_json::Value::Null),
        ));
        if dir == 1 {
            let slot = panel(
                w,
                side,
                Layout {
                    width: Size::Px(84.),
                    height: Size::Hug,
                    ..Layout::row()
                },
            );
            w.insert(
                slot,
                Align {
                    items: Items::Center,
                    justify: Justify::Center,
                },
            );
            text(w, slot, &num.shown(d), Typography::Label).unwrap();
        }
        named_button(
            w,
            side,
            s,
            glyph,
            format!("{verb} {}", num.label),
            a,
            enabled,
        );
    }
}

fn runtime_section(w: &mut World, s: &mut State, body: Entity) {
    use super::runtime as r;
    let Some(d) = s.defaults.clone() else {
        quiet(
            w,
            body,
            if s.defaults_reading.is_some() {
                "Reading…"
            } else {
                "The engine didn’t answer. Check the workshop, then come back."
            },
        );
        return;
    };
    quiet(w, body, &r::summary(&d));
    let idle = s.patching.is_none();

    heading(w, body, "Reach");
    quiet(w, body, "How she uses tools on a turn.");
    runtime_pick(w, body, s, &d, &r::POSTURE, "Tool posture", None, idle);
    runtime_pick(w, body, s, &d, &r::SPECIALIST, "Specialists", None, idle);
    runtime_pick(
        w,
        body,
        s,
        &d,
        &r::WEB_SEARCH,
        "Web search",
        Some("Preferred provider when she looks things up"),
        idle,
    );
    runtime_flag(w, body, s, &d, &r::SEARCH_FALLBACKS, idle);
    runtime_num(w, body, s, &d, &r::TOOL_ROUNDS, idle);
    let modules = r::list(&d, "allowed_modules");
    if more(
        w,
        body,
        s,
        "runtime.tools",
        "Allowed tools",
        if modules.is_empty() {
            "Full catalog"
        } else {
            "Custom list"
        },
    ) {
        s.drafts.entry("rt.modules").or_insert(modules.join(", "));
        field(
            w,
            body,
            s,
            "rt.modules",
            "Module names",
            "Tools she can call, separated by commas.",
            "websearch.search, fetch.url",
            false,
        );
        if modules.is_empty() {
            quiet(
                w,
                body,
                "Empty list means the full tool catalog is allowed.",
            );
        }
        save_row(
            w,
            body,
            s,
            "Save allowed tools",
            "allowed_modules",
            "rt.modules",
            idle,
        );
    }

    heading(w, body, "Shell");
    quiet(w, body, "Process sandbox — locked down until you open it.");
    runtime_flag(w, body, s, &d, &r::SHELL_TOOLS, idle);
    if s.shell_confirm {
        label(
            w,
            body,
            "No binaries are allowed-listed, so any command basename inside the jail can run. Turn agent shell tools on anyway?",
            Typography::Label,
        );
        let row = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 8.,
                ..Layout::row()
            },
        );
        button(
            w,
            row,
            s,
            "Turn on anyway",
            Action::Agent(super::agent::Patch::Field(r::SHELL_TOOLS.key, true.into())),
            idle,
        );
        button(w, row, s, "Keep off", Action::KeepShellOff, true);
    } else if r::shell_unguarded(&d) {
        quiet(
            w,
            body,
            "Agents are unlocked with an empty binary allowlist — list trusted tools under Allowlists.",
        );
    }
    runtime_flag(w, body, s, &d, &r::SHELL_NETWORK, idle);
    runtime_num(w, body, s, &d, &r::SHELL_TIMEOUT, idle);
    runtime_num(w, body, s, &d, &r::SHELL_OUTPUT, idle);
    let binaries = r::list(&d, "shell_allowed_binaries");
    let roots = r::list(&d, "shell_writable_roots");
    if more(
        w,
        body,
        s,
        "runtime.allowlists",
        "Allowlists",
        if binaries.is_empty() {
            "Any basename"
        } else {
            "Custom binaries"
        },
    ) {
        s.drafts.entry("rt.binaries").or_insert(binaries.join(", "));
        s.drafts.entry("rt.roots").or_insert(roots.join(", "));
        field(
            w,
            body,
            s,
            "rt.binaries",
            "Allowed binaries",
            "Command basenames, separated by commas.",
            "git, ls, rg",
            false,
        );
        save_row(
            w,
            body,
            s,
            "Save allowed binaries",
            "shell_allowed_binaries",
            "rt.binaries",
            idle,
        );
        field(
            w,
            body,
            s,
            "rt.roots",
            "Writable roots",
            "Absolute paths, separated by commas.",
            "/Users/you/projects",
            false,
        );
        save_row(
            w,
            body,
            s,
            "Save writable roots",
            "shell_writable_roots",
            "rt.roots",
            idle,
        );
    }

    heading(w, body, "Engine");
    quiet(w, body, "Diagnostics and where working memory lives.");
    runtime_flag(w, body, s, &d, &r::THINKING, idle);
    runtime_flag(w, body, s, &d, &r::TELEMETRY, idle);
    runtime_num(w, body, s, &d, &r::THINKING_LINES, idle);
    runtime_pick(w, body, s, &d, &r::BACKEND, "Host store", None, idle);
    if more(
        w,
        body,
        s,
        "runtime.budgets",
        "Tool budgets",
        "Helpers, follow-ups, restricted mode",
    ) {
        quiet(
            w,
            body,
            "Refine helper turns, follow-ups, and restricted mode. Per-turn cap is above.",
        );
        for num in &r::BUDGETS {
            runtime_num(w, body, s, &d, num, idle);
        }
    }
    if more(
        w,
        body,
        s,
        "runtime.quality",
        "Quality",
        "Retries and verifier floors",
    ) {
        for num in &r::QUALITY {
            runtime_num(w, body, s, &d, num, idle);
        }
    }
    let env = d["env_overrides"].as_str().unwrap_or_default();
    let env_lines = env.lines().filter(|l| !l.trim().is_empty()).count();
    if more(
        w,
        body,
        s,
        "runtime.env",
        "Host env overrides",
        &match env_lines {
            0 => "None".to_string(),
            1 => "1 variable".into(),
            k => format!("{k} variables"),
        },
    ) {
        quiet(
            w,
            body,
            "Applied when a turn starts. Values may be secrets, so only names are shown; adding a name again replaces its value.",
        );
        for name in r::env_names(env) {
            let (line, col) = split_row(w, body);
            label(w, col, &name, Typography::Label);
            let confirming = s.env_confirm.as_deref() == Some(name.as_str());
            if confirming {
                named_button(
                    w,
                    line,
                    s,
                    "Cancel",
                    format!("Keep {name}"),
                    Action::KeepEnv,
                    true,
                );
            }
            named_button(
                w,
                line,
                s,
                if confirming { "Remove?" } else { "Remove" },
                if confirming {
                    format!("Confirm removal of {name}")
                } else {
                    format!("Remove {name}")
                },
                Action::EnvRemove(name.clone()),
                idle,
            );
        }
        field(
            w,
            body,
            s,
            "rt.env.name",
            "Variable name",
            "",
            "API_TIMEOUT",
            false,
        );
        field(w, body, s, "rt.env.value", "Value", "", "value", true);
        let row = panel(
            w,
            body,
            Layout {
                width: Size::Hug,
                height: Size::Hug,
                ..Layout::row()
            },
        );
        named_button(
            w,
            row,
            s,
            "Add variable",
            "Add variable".into(),
            Action::EnvAdd,
            idle,
        );
        if !s.env_note.is_empty() {
            let note = s.env_note.clone();
            label(w, body, &note, Typography::Label);
        }
    }
    if s.patching.is_some() {
        quiet(w, body, "Saving…");
    }
    if more(
        w,
        body,
        s,
        "runtime.more",
        "More runtime settings",
        "Computer control, activity, versions, capacity",
    ) {
        quiet(
            w,
            body,
            "Computer control, world activity, Versions and worker capacity are coming to this app soon. Until then, change them in the original Medousa app.",
        );
    }
}

/// The pairing QR, its short code and countdown, and what to do with it.
fn pairing_qr(w: &mut World, s: &mut State, body: Entity) {
    use super::sharing as sh;
    let Some(qr) = s.qr.clone() else {
        quiet(
            w,
            body,
            if s.qr_reading.is_some() {
                "Generating QR…"
            } else {
                "The pairing QR isn’t ready yet. Press Refresh to try again."
            },
        );
        return;
    };
    let column = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::column()
        },
    );
    w.insert(
        column,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    if s.qr_loaded != s.qr_image {
        let _ = rndr_render::load_rgba(w, s.qr_image, qr.width, qr.height, qr.rgba.clone());
        s.qr_loaded = s.qr_image;
    }
    // A white mat keeps the code scannable in a dark theme.
    let mat = w.spawn((
        Layout {
            width: Size::Px(200.),
            height: Size::Px(200.),
            padding: 12.,
            ..Layout::column()
        },
        Color::rgba(1., 1., 1., 1.),
        Radius(14.),
    ));
    set_parent(w, mat, column);
    let image = w.spawn((
        Layout {
            width: Size::Px(176.),
            height: Size::Px(176.),
            ..Layout::column()
        },
        ImageView::new(s.qr_image),
        Semantics {
            name: Some("QR code for phone pairing".into()),
            ..Default::default()
        },
    ));
    set_parent(w, image, mat);
    // Short lines hug their text so they sit centred under the code.
    let centred = |w: &mut World, value: &str, kind: Typography, faint: bool| {
        let e = text(w, column, value, kind).unwrap();
        w.get_mut::<Layout>(e).unwrap().width = Size::Hug;
        if faint {
            w.insert(
                e,
                UiStyle {
                    foreground: Some(Token::Tertiary.into()),
                    ..Default::default()
                },
            );
        }
    };
    centred(w, &sh::short_code(&qr.short_code), Typography::Title, false);
    centred(
        w,
        "Short code fallback if the camera can’t scan",
        Typography::Secondary,
        true,
    );
    let secs = sh::seconds_left(qr.expires_at);
    if secs > 0 {
        centred(
            w,
            &format!("Refreshes in {}", sh::countdown(secs)),
            Typography::Secondary,
            true,
        );
    }
    quiet(
        w,
        column,
        "Open Medousa on your phone on the same Wi‑Fi and scan this code — or enter the short code manually. Off-LAN? Copy the link and paste it in the app.",
    );
    let row = panel(
        w,
        column,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },
    );
    let idle = s.sharing_busy.is_none();
    button(w, row, s, "Copy link", Action::CopyInvite, true);
    button(w, row, s, "Rotate invite", Action::RotateInvite, idle);
}

fn sharing_section(w: &mut World, s: &mut State, body: Entity) {
    let shared_on = s.shared.as_ref().is_some_and(|m| m.shared());
    quiet(
        w,
        body,
        &format!(
            "{} · phone, peers & channels",
            if shared_on { "Shared" } else { "Personal" }
        ),
    );
    let idle = s.sharing_busy.is_none();
    if s.sharing_reading.is_some() && s.shared.is_none() && s.devices.is_none() {
        quiet(w, body, "Reading…");
    }

    heading(w, body, "Shared");
    quiet(w, body, "Team seats on this brain — vault stays shared.");
    let meta = match &s.shared {
        _ if !s.shared_note.is_empty() => s.shared_note.clone(),
        Some(m) if m.shared() => format!("Seats via Phone invites · admin {}", m.root_profile_id),
        _ => "Personal hats stay as today".into(),
    };
    switch_row(
        w,
        body,
        s,
        "Shared mode",
        &meta,
        shared_on,
        Action::SharedMode(!shared_on),
        idle && s.shared.is_some(),
    );

    heading(w, body, "Phone");
    quiet(w, body, "Pair as a second portal on the same Wi‑Fi.");
    let open = s.open.contains("pair.qr");
    let side = runtime_row(w, body, "Pairing QR", "Same Wi‑Fi — generated when you ask");
    named_button(
        w,
        side,
        s,
        if open { "Hide" } else { "Show" },
        if open {
            "Hide pairing QR".into()
        } else {
            "Show pairing QR".into()
        },
        Action::PairingQr,
        true,
    );
    if open {
        pairing_qr(w, s, body);
    }
    match &s.devices {
        Some(devices) if devices.is_empty() => {
            quiet(w, body, "No phones paired yet.");
        }
        Some(devices) => {
            for d in devices.clone() {
                let (line, col) = split_row(w, body);
                label(w, col, &d.phone_name, Typography::Label);
                quiet(w, col, &super::sharing::device_meta(&d));
                let confirming = s.device_confirm.as_deref() == Some(d.pairing_id.as_str());
                if confirming {
                    named_button(
                        w,
                        line,
                        s,
                        "Cancel",
                        format!("Keep {}", d.phone_name),
                        Action::CancelDeviceRemove,
                        true,
                    );
                }
                named_button(
                    w,
                    line,
                    s,
                    if confirming { "Remove?" } else { "Remove" },
                    if confirming {
                        format!("Confirm removal of {}", d.phone_name)
                    } else {
                        format!("Remove {}", d.phone_name)
                    },
                    Action::DeviceRemove(d.pairing_id.clone()),
                    idle,
                );
            }
        }
        None => {}
    }

    heading(w, body, "Nearby");
    quiet(w, body, "Wi‑Fi reachability, peers, and canvas backups.");
    match &s.lan {
        Some(Ok(lan)) => {
            let (on, message) = (lan.enabled, lan.message.clone());
            switch_row(
                w,
                body,
                s,
                "LAN pairing",
                &format!("{message} Changing this restarts the engine."),
                on,
                Action::LanPairing(!on),
                idle,
            );
        }
        Some(Err(e)) => {
            let e = e.clone();
            fact(w, body, "LAN pairing", &format!("Not available here: {e}"));
        }
        None if !s.local() && s.reading.is_empty() => {
            quiet(
                w,
                body,
                "This workshop runs on another machine; its LAN pairing is set there.",
            );
        }
        None => {}
    }
    let side = runtime_row(
        w,
        body,
        "Peers & trusted workshops",
        "Connect workshops, invite devices and set trust on the Peers screen",
    );
    named_button(
        w,
        side,
        s,
        "Open",
        "Open Peers".into(),
        Action::OpenPeers,
        true,
    );
    quiet(
        w,
        body,
        "Canvas backups are still managed in the original Medousa app.",
    );

    channels_part(w, s, body);
    if !s.sharing_note.is_empty() {
        let note = s.sharing_note.clone();
        label(w, body, &note, Typography::Label);
    }
    let row = panel(
        w,
        body,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            ..Layout::row()
        },
    );
    button(
        w,
        row,
        s,
        "Refresh",
        Action::Refresh,
        s.sharing_reading.is_none(),
    );
}

/// A one-line text field whose text is kept in `s.drafts` under `key`, so a
/// rebuild keeps what was typed. A secure one shows bullets.
#[allow(clippy::too_many_arguments)]
fn field(
    w: &mut World,
    p: Entity,
    s: &mut State,
    key: &'static str,
    title: &str,
    hint: &str,
    placeholder: &str,
    secure: bool,
) {
    label(w, p, title, Typography::Label);
    if !hint.is_empty() {
        quiet(w, p, hint);
    }
    let value = s.drafts.get(key).cloned().unwrap_or_default();
    let e = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(32.),
            padding: 8.,
            ..Layout::column()
        },
        Label::new(value),
        TextField,
        Focusable,
        Placeholder::new(placeholder),
        Semantics {
            name: Some(title.into()),
            ..Default::default()
        },
    ));
    set_parent(w, e, p);
    visual::plane(w, e, visual::raised());
    w.insert(e, Radius(8.));
    if secure {
        w.insert(e, Secure);
    }
    s.fields.push((key, e));
}

/// Channels: how many are ready, then each with its form when opened.
fn channels_part(w: &mut World, s: &mut State, body: Entity) {
    heading(w, body, "Channels");
    quiet(
        w,
        body,
        "Messaging doors — reach Medousa from Telegram, Discord, Slack and WhatsApp.",
    );
    if !s.local() {
        if s.reading.is_empty() {
            quiet(
                w,
                body,
                "This workshop runs on another machine; its channels are set up there.",
            );
        }
        return;
    }
    match s.channels.clone() {
        Some(summary) => {
            quiet(
                w,
                body,
                &format!(
                    "{} of {} ready",
                    channels::ready(&summary),
                    channels::CHANNELS.len()
                ),
            );
            for c in &channels::CHANNELS {
                channel(w, s, body, c, &summary);
            }
        }
        None if s.channels_reading.is_some() => {
            quiet(w, body, "Reading channels…");
        }
        None => {}
    }
    if let Some(("", note)) = &s.channel_note {
        let note = note.clone();
        label(w, body, &note, Typography::Label);
    }
}

fn channel(
    w: &mut World,
    s: &mut State,
    body: Entity,
    c: &'static channels::Meta,
    summary: &medousa_desktop_host::channels::Summary,
) {
    use medousa_desktop_host::channels::Status;
    let status = summary.status(c.id);
    let open = s.open.contains(channels::open_key(c.id));
    let side = runtime_row(
        w,
        body,
        c.name,
        &format!("{} · {}", c.description, channels::status_label(status)),
    );
    let (face, spoken) = match (open, status) {
        (true, _) => ("Close", format!("Close {}", c.name)),
        (false, Status::NeedsSetup) => ("Set up", format!("Set up {}", c.name)),
        (false, _) => ("Edit", format!("Edit {}", c.name)),
    };
    named_button(w, side, s, face, spoken, Action::Channel(c.id), true);
    if !open {
        return;
    }
    let form = panel(
        w,
        body,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            padding: 12.,
            ..Layout::column()
        },
    );
    quiet(w, form, c.tagline);
    label(
        w,
        form,
        if c.id == "whatsapp" {
            "Bridge setup"
        } else {
            "Connect"
        },
        Typography::Emphasis,
    );
    quiet(w, form, c.blurb);
    if let Some((url, name)) = c.guide {
        let row = panel(
            w,
            form,
            Layout {
                width: Size::Hug,
                height: Size::Hug,
                ..Layout::row()
            },
        );
        button(w, row, s, name, Action::OpenGuide(url), true);
    }
    for t in c.tokens {
        let kept = channels::token_set(summary, t.key);
        let forgetting = s.forget.contains(t.key);
        let hint = match (forgetting, kept) {
            (true, _) => "Forgotten when you save",
            (false, true) => "Kept on this Mac — paste again only to replace",
            (false, false) => "Waiting for a token",
        };
        field(w, form, s, t.key, t.title, hint, t.placeholder, true);
        if kept {
            switch_row(
                w,
                form,
                s,
                &format!("Forget saved {}", t.title.to_lowercase()),
                "Clears it when you save, unless a new one is pasted",
                forgetting,
                Action::ForgetToken(t.key),
                true,
            );
        }
    }
    let (who, copy) = match c.id {
        "telegram" => (
            "Who may reach her",
            "After the token is live, message the bot /whoami and paste your numeric ID — only you get in.",
        ),
        "discord" => (
            "How she’s called",
            "The character before commands in your servers.",
        ),
        "slack" => (
            "Who may reach her",
            "Socket Mode only listens to the Slack users you list here.",
        ),
        _ => ("", ""),
    };
    match c.id {
        "telegram" | "discord" | "slack" => {
            label(w, form, who, Typography::Emphasis);
            quiet(w, form, copy);
        }
        _ => {}
    }
    match c.id {
        "telegram" => field(
            w,
            form,
            s,
            "telegram.users",
            "Your Telegram user ID",
            "From /whoami — separate more with commas",
            "123456789",
            false,
        ),
        "discord" => field(
            w,
            form,
            s,
            "discord.prefix",
            "Command prefix",
            "",
            "!",
            false,
        ),
        "slack" => field(
            w,
            form,
            s,
            "slack.users",
            "Allowed Slack user IDs",
            "Comma-separated",
            "U0123ABCD",
            false,
        ),
        _ => {
            field(
                w,
                form,
                s,
                "whatsapp.bind",
                "Deliver bind",
                "Where the bridge listens",
                "127.0.0.1:7423",
                false,
            );
            field(
                w,
                form,
                s,
                "whatsapp.users",
                "Allowed JIDs",
                "Who may message her — comma-separated",
                "user@s.whatsapp.net",
                false,
            );
            label(w, form, "Bridge extras", Typography::Emphasis);
            quiet(
                w,
                form,
                "Optional overrides — most bridges never need these.",
            );
            field(
                w,
                form,
                s,
                "whatsapp.url",
                "Deliver URL",
                "Optional",
                "",
                false,
            );
            field(
                w,
                form,
                s,
                "whatsapp.db",
                "Session DB path",
                "Optional",
                "",
                false,
            );
        }
    }
    if more(
        w,
        form,
        s,
        channels::advanced_key(c.id),
        "Advanced",
        "Heartbeat nudges and delivery targets",
    ) {
        let on = s.heartbeat.get(c.id).copied().unwrap_or(false);
        let (where_, key, title, placeholder) = match c.id {
            "telegram" => (
                "On configured chats",
                "telegram.chats",
                "Heartbeat chat IDs",
                "-1001234567890",
            ),
            "discord" => (
                "On configured channels",
                "discord.channels",
                "Heartbeat channel IDs",
                "",
            ),
            "slack" => (
                "On configured channels",
                "slack.channels",
                "Heartbeat channel IDs",
                "",
            ),
            _ => (
                "On configured chats",
                "whatsapp.jids",
                "Heartbeat chat JIDs",
                "",
            ),
        };
        switch_row(
            w,
            form,
            s,
            "Heartbeat nudges",
            where_,
            on,
            Action::ChannelHeartbeat(c.id, !on),
            true,
        );
        field(
            w,
            form,
            s,
            key,
            title,
            "Comma-separated",
            placeholder,
            false,
        );
    }
    let busy = s.channel_busy.is_some();
    let saving = s.channel_busy.is_some_and(|(_, id)| id == c.id);
    let row = panel(
        w,
        form,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            ..Layout::row()
        },
    );
    named_button(
        w,
        row,
        s,
        if saving {
            "Saving…"
        } else {
            "Save & connect"
        },
        format!("Save {}", c.name),
        Action::SaveChannel(c.id),
        !busy,
    );
    if let Some((id, note)) = &s.channel_note
        && *id == c.id
    {
        let note = note.clone();
        label(w, form, &note, Typography::Label);
    }
}

fn bar(w: &mut World, p: Entity, percent: f32) {
    let track = panel(
        w,
        p,
        Layout {
            width: Size::Px(BAR),
            height: Size::Px(4.),
            ..Layout::row()
        },
    );
    visual::plane(w, track, Token::ControlHover);
    w.insert(track, Radius(2.));
    let fill = panel(
        w,
        track,
        Layout {
            width: Size::Px((percent.clamp(2., 100.) / 100. * BAR).round()),
            height: Size::Fill,
            ..Layout::row()
        },
    );
    visual::plane(w, fill, Token::Accent);
    w.insert(fill, Radius(2.));
}

fn packages(w: &mut World, s: &mut State, body: Entity) {
    quiet(
        w,
        body,
        "Medousa already has the engine. Add optional workshop capabilities here — computer control, coding, offline brain, and channels. MCP lives under MCP.",
    );
    let Some(catalog) = s.packages.clone() else {
        if s.listing.is_some() {
            quiet(w, body, "Loading catalog…");
        }
        return;
    };
    let busy = s.busy.clone();
    let progress = s.progress.lock().unwrap().clone();
    for row in &catalog.packages {
        let line = panel(
            w,
            body,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                padding: 6.,
                gap: 12.,
                ..Layout::row()
            },
        );
        w.insert(
            line,
            Align {
                items: Items::Start,
                justify: Justify::Start,
            },
        );
        let text_col = panel(
            w,
            line,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 2.,
                ..Layout::column()
            },
        );
        label(w, text_col, &row.display_name, Typography::Emphasis);
        let mut meta = row.hint.clone();
        if let Some(size) = medousa_desktop_host::packages::format_bytes(row.size_bytes) {
            meta.push_str(&format!(" · ~{size}"));
        }
        if let (true, Some(v)) = (row.installed, &row.installed_version) {
            meta.push_str(&format!(" · v{v}"));
        } else if let Some(v) = &row.available_version {
            meta.push_str(&format!(" · v{v}"));
        }
        quiet(w, text_col, &meta);
        let mine = busy.as_ref().is_some_and(|(_, id)| *id == row.id);
        if mine && let Some(p) = progress.as_ref() {
            bar(w, text_col, p.percent);
            quiet(
                w,
                text_col,
                &format!("{} · {} · {}", p.display_name, p.phase_label, p.message),
            );
        } else if mine {
            quiet(w, text_col, "Starting…");
        }
        let side = panel(
            w,
            line,
            Layout {
                width: Size::Hug,
                height: Size::Hug,
                gap: 4.,
                ..Layout::row()
            },
        );
        w.insert(
            side,
            Align {
                items: Items::Center,
                justify: Justify::End,
            },
        );
        let idle = busy.is_none();
        if row.installed && !row.update_available {
            let done = text(w, side, "Installed", Typography::Secondary).unwrap();
            w.get_mut::<Layout>(done).unwrap().width = Size::Hug;
            w.insert(
                done,
                UiStyle {
                    foreground: Some(Token::SuccessText.into()),
                    ..Default::default()
                },
            );
            if row.optional {
                let name = format!("Remove {}", row.display_name);
                let h = button(w, side, s, "Remove", Action::Remove(row.id.clone()), idle);
                if let Some(sem) = w.get_mut::<Semantics>(h.root) {
                    sem.name = Some(name);
                }
            }
        } else {
            let verb = if row.update_available {
                "Update"
            } else {
                "Install"
            };
            let h = button(w, side, s, verb, Action::Install(row.id.clone()), idle);
            if let Some(sem) = w.get_mut::<Semantics>(h.root) {
                sem.name = Some(format!("{verb} {}", row.display_name));
            }
        }
    }
    if more(
        w,
        body,
        s,
        "installer",
        "Advanced",
        "Repair the app and engine",
    ) {
        if catalog.installer_available {
            quiet(w, body, "Repair or change the app and engine themselves.");
            button(
                w,
                body,
                s,
                "Open Medousa Installer",
                Action::OpenInstaller,
                busy.is_none(),
            );
        } else {
            quiet(
                w,
                body,
                "Medousa Installer isn’t on this Mac. It’s only needed to repair or change the app and engine.",
            );
        }
    }
    let refresh = button(
        w,
        body,
        s,
        "Check for updates",
        Action::Refresh,
        s.listing.is_none() && busy.is_none(),
    );
    let _ = refresh;
}

pub(super) fn render(w: &mut World, s: &mut State) {
    capture(w, s);
    s.fields.clear();
    if w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|e| crate::surfaces::descendant(w, e, s.root))
    {
        w.resource_mut::<Focus>().unwrap().entity = None;
    }
    for root in [s.nav, s.body] {
        for e in children_of(w, root) {
            despawn(w, e);
        }
    }
    s.actions.clear();
    nav(w, s);
    let body = s.body;
    label(w, body, s.section.label(), Typography::Title);
    quiet(w, body, s.section.hint());
    if !s.message.is_empty() {
        let m = s.message.clone();
        label(w, body, &m, Typography::Secondary);
    }
    match s.section {
        Section::Preferences => look(w, s, body),
        Section::Connection => connection(w, s, body),
        Section::Packages => packages(w, s, body),
        Section::Agent => agent_section(w, s, body),
        Section::Mcp => mcp(w, s, body),
        Section::Accounts => accounts(w, s, body),
        Section::ExternalAgents => external_agents(w, s, body),
        Section::Runtime => runtime_section(w, s, body),
        Section::Sharing => sharing_section(w, s, body),
    }
    // The field that had focus gets it back, caret and all.
    if let Some((key, caret)) = s.focused.take()
        && let Some(&(_, e)) = s.fields.iter().find(|(k, _)| *k == key)
    {
        w.resource_mut::<Focus>().unwrap().entity = Some(e);
        w.insert(e, caret);
    }
}
