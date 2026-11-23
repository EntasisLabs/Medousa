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
    }
    quiet(
        w,
        body,
        "Adding a conversation isn’t in this app yet; add one in the original Medousa app.",
    );
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
        quiet(
            w,
            body,
            &if modules.is_empty() {
                "Empty list means the full tool catalog is allowed.".to_string()
            } else {
                modules.join(", ")
            },
        );
        quiet(
            w,
            body,
            "Edit the list in the original Medousa app for now.",
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
        let none = || "None".to_string();
        fact(
            w,
            body,
            "Allowed binaries",
            &if binaries.is_empty() {
                none()
            } else {
                binaries.join(", ")
            },
        );
        fact(
            w,
            body,
            "Writable roots",
            &if roots.is_empty() {
                none()
            } else {
                roots.join(", ")
            },
        );
        quiet(
            w,
            body,
            "Edit these lists in the original Medousa app for now.",
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
        // Only names: values may be secrets.
        for line in env.lines().filter(|l| !l.trim().is_empty()) {
            let name = line.split('=').next().unwrap_or_default().trim();
            fact(w, body, name, "Set");
        }
        quiet(
            w,
            body,
            "KEY=value per line, applied when a turn starts. Edit them in the original Medousa app for now.",
        );
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
    quiet(
        w,
        body,
        "Peers, trusted workshops and canvas backups are still managed in the original Medousa app.",
    );

    heading(w, body, "Channels");
    quiet(
        w,
        body,
        "Messaging doors — Telegram, Discord, Slack and WhatsApp are set up in the original Medousa app for now.",
    );
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
        Section::Accounts => accounts(w, s, body),
        Section::ExternalAgents => external_agents(w, s, body),
        Section::Runtime => runtime_section(w, s, body),
        Section::Sharing => sharing_section(w, s, body),
    }
}
