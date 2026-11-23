//! Settings layout: medousa-home's section list on the left, the chosen
//! section in a reading column on the right.
use super::*;
use crate::native_style as visual;
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
                "No models are available yet. Connect a provider in the original Medousa app.",
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
    if more(
        w,
        body,
        s,
        "agent-more",
        "More agent settings",
        "Keys, routing, retention",
    ) {
        quiet(
            w,
            body,
            "Provider keys, fallbacks, stage routing, coding runtime, mode suggestions and retention are coming to this app soon. Until then, change them in the original Medousa app.",
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
        quiet(w, col, &format!("{} · {state}", server.id));
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
    }
    if status.local && more(w, body, s, "mcp-add", "Add a server", "stdio, HTTP or SSE") {
        quiet(
            w,
            body,
            "Adding servers, per-tool settings and sign-in are coming to this app soon. Until then, add them in the original Medousa app.",
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

/// A thin progress bar `percent` full.
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
        other => {
            debug_assert!(!other.ready());
            quiet(
                w,
                body,
                "Coming to this app soon. Until then, change these in the original Medousa app.",
            );
        }
    }
}
