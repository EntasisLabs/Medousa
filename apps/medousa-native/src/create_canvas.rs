//! The controls that float over the canvas, after medousa-home's editor:
//! a tool strip at the bottom with a shape picker and a selection bar above
//! it, history, fit and the style palette at the top right, zoom at the
//! bottom left, and the layers inspector behind "Details". The canvas stays
//! full-bleed; RNDR's canvas view leaves presses on these controls to them.
use super::ask::Source;
use super::*;
use crate::native_style as visual;
use rndr_canvas::{Alignment, Axis, ShapeKind};
use rndr_canvas_ui::editor::{StyleChange, Tool};
use rndr_ui::{Token, Typography, UiColor, UiStyle, menu, text};

/// medousa-home's `canvasColors`.
pub(crate) const COLORS: [(&str, &str); 9] = [
    ("Ink", "#393e33"),
    ("Gray", "#83918a"),
    ("Purple", "#9474dc"),
    ("Blue", "#4e75d9"),
    ("Teal", "#4f9eaf"),
    ("Amber", "#d79737"),
    ("Green", "#3b9a70"),
    ("Pink", "#d16d8e"),
    ("Red", "#d76956"),
];
/// medousa-home's `canvasSizes`: name, stroke width, text size.
const SIZES: [(&str, f64, f64); 4] = [
    ("S", 2., 20.),
    ("M", 4., 32.),
    ("L", 7., 48.),
    ("XL", 12., 72.),
];
const SHAPES: [(Tool, Icon, &str); 5] = [
    (Tool::Rectangle, Icon::Square, "Rectangle"),
    (Tool::Ellipse, Icon::Circle, "Ellipse"),
    (Tool::Line, Icon::Minus, "Line"),
    (Tool::Arrow, Icon::MoveUpRight, "Arrow"),
    (Tool::Frame, Icon::Frame, "Frame"),
];
const INSET: f32 = 12.;
const DETAILS: f32 = 264.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fill {
    None,
    Soft,
    Solid,
}
impl Fill {
    fn of(fill: &str) -> Self {
        let fill = fill.trim();
        if fill.is_empty() || fill.eq_ignore_ascii_case("transparent") {
            Fill::None
        } else if fill.len() == 9 && fill.ends_with("24") {
            Fill::Soft
        } else {
            Fill::Solid
        }
    }
    /// medousa-home's `fillValue`: a 14% wash of the color, or the color.
    fn value(self, color: &str) -> String {
        match self {
            Fill::None => "transparent".into(),
            Fill::Soft => format!("{}24", &color[..7.min(color.len())]),
            Fill::Solid => color.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Do {
    Tool(Tool),
    Shapes,
    Palette,
    Details,
    Color(&'static str),
    Size(usize),
    Fill(Fill),
    ZoomIn,
    ZoomOut,
    Fit,
    FitSelection,
    Undo,
    Redo,
    Group,
    Ungroup,
    AlignMenu,
    ArrangeMenu,
    Align(Alignment),
    Distribute(Axis),
    Arrange(bool),
    Duplicate,
    Delete,
    Layer(String),
    Visible(String),
    Lock(String),
    Image,
    Ask,
    AskSend,
    AskPreview(bool),
    AskUse,
    AskDismiss,
    AskRetry,
    AskConversation,
}

pub(crate) struct Chrome {
    body: Entity,
    top_left: Entity,
    top_right: Entity,
    bottom: Entity,
    zoom: Entity,
    palette: Entity,
    details: Entity,
    palette_open: bool,
    shapes_open: bool,
    details_open: bool,
    signature: String,
    /// The Align and Arrange buttons, which their menus hang from.
    anchors: [Option<Entity>; 2],
}

fn panel(w: &mut World, parent: Entity, layout: Layout) -> Entity {
    let e = w.spawn((layout,));
    set_parent(w, e, parent);
    e
}

/// A raised floating group: overlay plane, hairline, soft shadow.
fn floating(w: &mut World, parent: Entity, axis: Axis2, at: Absolute, radius: f32) -> Entity {
    let e = panel(
        w,
        parent,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            padding: 4.,
            gap: 2.,
            ..match axis {
                Axis2::Row => Layout::row(),
                Axis2::Column => Layout::column(),
            }
        },
    );
    w.insert(e, at);
    visual::plane(w, e, Token::Overlay);
    w.insert(e, Radius(radius));
    let line = visual::color(w, Token::SubtleStroke);
    w.insert(
        e,
        Border {
            color: line,
            width: 1.,
        },
    );
    // Follows appearance changes; a mount-time color would not.
    w.insert(e, visual::SemanticBorder(Token::SubtleStroke));
    w.insert(
        e,
        Shadows(vec![Shadow::drop(3., 12., Color::rgba(0., 0., 0., 0.12))]),
    );
    w.insert(
        e,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    e
}
enum Axis2 {
    Row,
    Column,
}

pub(super) fn mount(w: &mut World, body: Entity) -> Chrome {
    let top_left = panel(
        w,
        body,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 10.,
            ..Layout::row()
        },
    );
    w.insert(
        top_left,
        Absolute {
            left: Some(INSET),
            top: Some(INSET),
            ..Default::default()
        },
    );
    w.insert(
        top_left,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    let top_right = floating(
        w,
        body,
        Axis2::Row,
        Absolute {
            right: Some(INSET),
            top: Some(INSET),
            ..Default::default()
        },
        8.,
    );
    let bottom = panel(
        w,
        body,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 8.,
            ..Layout::column()
        },
    );
    w.insert(
        bottom,
        Absolute {
            left: Some(0.),
            bottom: Some(16.),
            ..Default::default()
        },
    );
    w.insert(
        bottom,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    let zoom = floating(
        w,
        body,
        Axis2::Row,
        Absolute {
            left: Some(INSET),
            bottom: Some(16.),
            ..Default::default()
        },
        8.,
    );
    let palette = floating(
        w,
        body,
        Axis2::Column,
        Absolute {
            right: Some(INSET),
            top: Some(56.),
            ..Default::default()
        },
        11.,
    );
    w.get_mut::<Layout>(palette).unwrap().padding = 10.;
    w.get_mut::<Layout>(palette).unwrap().gap = 8.;
    w.insert(
        palette,
        Align {
            items: Items::Start,
            justify: Justify::Start,
        },
    );
    w.insert(palette, Hidden);
    let details = floating(
        w,
        body,
        Axis2::Column,
        Absolute {
            left: Some(INSET),
            top: Some(56.),
            ..Default::default()
        },
        11.,
    );
    {
        let l = w.get_mut::<Layout>(details).unwrap();
        l.width = Size::Px(DETAILS);
        l.padding = 8.;
        l.gap = 8.;
    }
    w.insert(
        details,
        Align {
            items: Items::Stretch,
            justify: Justify::Start,
        },
    );
    w.insert(details, Scroll::default());
    w.insert(details, Hidden);
    Chrome {
        body,
        top_left,
        top_right,
        bottom,
        zoom,
        palette,
        details,
        palette_open: false,
        shapes_open: false,
        details_open: false,
        signature: String::new(),
        anchors: [None, None],
    }
}

fn register(s: &mut State, a: Do) -> u64 {
    let id = next();
    s.actions.insert(id, Action::Canvas(a));
    id
}

#[allow(clippy::too_many_arguments)]
fn icon(
    w: &mut World,
    p: Entity,
    s: &mut State,
    i: Icon,
    name: &str,
    a: Do,
    chosen: bool,
    enabled: bool,
) -> rndr_ui::controls::ControlHandle {
    let id = register(s, a);
    let h = visual::icon_action(w, p, id, i, name, enabled);
    // As the rail does: every inner layer fills the square and the glyph is
    // centered in it, so a chosen wash sits evenly around the icon.
    let mut layers = children_of(w, h.root);
    while let Some(layer) = layers.pop() {
        if let Some(l) = w.get_mut::<Layout>(layer) {
            l.height = Size::Fill;
            l.width = Size::Fill;
        }
        layers.extend(children_of(w, layer));
    }
    w.insert(
        h.label,
        LabelBox {
            inset_x: 0.,
            inset_y: 5.,
            min_height: 0.,
        },
    );
    w.insert(
        h.label,
        TextStyle {
            size: 16.,
            align: TextAlign::Center,
            line_height: Some(1.),
            ..Default::default()
        },
    );
    if chosen {
        mark(w, h.root);
    }
    h
}

fn word(
    w: &mut World,
    p: Entity,
    s: &mut State,
    name: &str,
    a: Do,
    chosen: bool,
    enabled: bool,
) -> Entity {
    let id = register(s, a);
    let h = visual::document_action(w, p, id, name, enabled);
    if chosen {
        mark(w, h.root);
    }
    h.root
}

/// The chosen tool, size or fill sits on the selected wash.
fn mark(w: &mut World, root: Entity) {
    // Face and its border layer together, as the calendar's solid buttons,
    // so the wash covers the whole control rather than the inset face.
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

fn caption(w: &mut World, p: Entity, value: &str) {
    let e = text(w, p, value, Typography::Secondary).unwrap();
    // A fill-width label inside a hugging row measures to nothing.
    w.get_mut::<Layout>(e).unwrap().width = Size::Hug;
    w.insert(
        e,
        UiStyle {
            foreground: Some(Token::Tertiary.into()),
            ..Default::default()
        },
    );
}

fn divider(w: &mut World, p: Entity) {
    let e = panel(
        w,
        p,
        Layout {
            width: Size::Px(1.),
            height: Size::Px(20.),
            ..Layout::column()
        },
    );
    visual::plane(w, e, Token::SubtleStroke);
}

/// A round color dot, drawn over a quiet button so the button keeps focus,
/// hover and activation.
fn swatch(w: &mut World, p: Entity, s: &mut State, name: &'static str, color: &str, chosen: bool) {
    let h = icon(
        w,
        p,
        s,
        Icon::Circle,
        name,
        Do::Color(color_of(name)),
        chosen,
        true,
    );
    // The dot is the glyph; the label keeps only the accessible name.
    w.insert(
        h.label,
        UiStyle {
            foreground: Some(UiColor::Explicit(Color::rgba(0., 0., 0., 0.))),
            ..Default::default()
        },
    );
    let root = h.root;
    let dot = w.spawn((
        Layout {
            width: Size::Px(16.),
            height: Size::Px(16.),
            ..Layout::column()
        },
        Absolute {
            left: Some(8.),
            top: Some(8.),
            ..Default::default()
        },
        rndr_canvas_ui::paint::parse_color(color),
        Radius(8.),
        Border {
            color: Color::rgba(0., 0., 0., 0.15),
            width: 1.,
        },
    ));
    set_parent(w, dot, root);
}
fn color_of(name: &str) -> &'static str {
    COLORS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(COLORS[0].1, |(_, c)| c)
}

fn is_shape(tool: Tool) -> bool {
    SHAPES.iter().any(|(t, ..)| *t == tool)
}

/// What the chrome shows, so it is rebuilt only when that changes.
struct Snapshot {
    tool: Tool,
    selection: Vec<String>,
    groups: bool,
    texts: bool,
    shapes: bool,
    stroke: String,
    fill: Fill,
    width: f64,
    font: f64,
    zoom: u32,
    undo: bool,
    redo: bool,
    editing: bool,
    /// Topmost first: id, label, depth, visible, locked.
    layers: Vec<(String, String, usize, bool, bool)>,
}

/// The layer list, topmost first, each group's or frame's children under it.
fn layers(nodes: &[Node]) -> Vec<(String, String, usize, bool, bool)> {
    fn walk(
        nodes: &[Node],
        parent: Option<&str>,
        depth: usize,
        out: &mut Vec<(String, String, usize, bool, bool)>,
    ) {
        for n in nodes
            .iter()
            .rev()
            .filter(|n| n.parent_id.as_deref() == parent)
        {
            let mut label = n.label();
            if let Some(first) = label.get(..1) {
                label = first.to_uppercase() + &label[1..];
            }
            out.push((n.id.clone(), label, depth, n.visible, n.locked));
            if depth < 8 {
                walk(nodes, Some(&n.id), depth + 1, out);
            }
        }
    }
    let mut out = vec![];
    walk(nodes, None, 0, &mut out);
    out
}

fn snapshot(v: &CanvasView) -> Snapshot {
    let e = &v.editor;
    let nodes = e.nodes();
    let picked: Vec<&Node> = nodes
        .iter()
        .filter(|n| e.selection().contains(&n.id))
        .collect();
    Snapshot {
        tool: e.tool(),
        selection: e.selection().to_vec(),
        groups: picked.iter().any(|n| matches!(n.content, Content::Group)),
        texts: !picked.is_empty()
            && picked
                .iter()
                .all(|n| matches!(n.content, Content::Text { .. })),
        shapes: picked.iter().any(|n| {
            matches!(&n.content, Content::Shape { shape, .. } if *shape != ShapeKind::Line)
                || matches!(n.content, Content::Frame { .. })
        }),
        stroke: e.style.stroke.clone(),
        fill: Fill::of(&e.style.fill),
        width: e.style.stroke_width,
        font: e.style.font_size,
        zoom: (e.camera.zoom * 100.).round().max(1.) as u32,
        undo: e.mutation().can_undo,
        redo: e.mutation().can_redo,
        editing: e.editing_text().is_some(),
        layers: layers(&nodes),
    }
}

/// medousa-home's hint for the current tool and selection.
fn hint(n: &Snapshot) -> String {
    if n.editing {
        return "Type here · Esc to finish".into();
    }
    match n.tool {
        Tool::Select if n.selection.len() > 1 => {
            format!("{} selected · Shift adds · ⌘G groups", n.selection.len())
        }
        Tool::Select if !n.selection.is_empty() => "Drag the round handle to rotate".into(),
        Tool::Select => String::new(),
        Tool::Hand => "Drag to move".into(),
        Tool::Pen => "Draw freely · Esc returns to Select".into(),
        Tool::Eraser => "Erase · Esc returns to Select".into(),
        Tool::Text => "Click to type".into(),
        Tool::Frame => "Drag to add a frame".into(),
        _ => "Drag to draw · release to select".into(),
    }
}

pub(super) fn render(w: &mut World, s: &mut State) {
    let Some(n) = w.resource::<CanvasView>().map(snapshot) else {
        return;
    };
    let c = &s.chrome;
    let signature = format!(
        "{:?}|{:?}|{}{}{}|{}|{:?}|{}|{}|{}|{}{}{}|{}{}{}|{:?}",
        n.tool,
        n.selection,
        n.groups,
        n.texts,
        n.shapes,
        n.stroke,
        n.fill,
        n.width,
        n.font,
        n.zoom,
        n.undo,
        n.redo,
        n.editing,
        c.palette_open,
        c.shapes_open,
        c.details_open,
        if c.details_open { &n.layers[..] } else { &[] },
    ) + if s.importing.is_some() {
        "|importing"
    } else {
        ""
    } + &s.ask.signature();
    if signature == s.chrome.signature {
        return;
    }
    s.chrome.signature = signature;
    let c = (
        s.chrome.top_left,
        s.chrome.top_right,
        s.chrome.bottom,
        s.chrome.zoom,
        s.chrome.palette,
    );
    let (top_left, top_right, bottom, zoom, palette) = c;
    if w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|e| {
            [top_left, top_right, bottom, zoom, palette, s.chrome.details]
                .iter()
                .any(|r| crate::surfaces::descendant(w, e, *r))
        })
    {
        w.resource_mut::<Focus>().unwrap().entity = None;
    }
    let details = s.chrome.details;
    // Typing in the Ask prompt survives a rebuild.
    let typing =
        s.ask.field.is_some() && w.resource::<Focus>().and_then(|f| f.entity) == s.ask.field;
    s.ask.field = None;
    for root in [top_left, top_right, bottom, zoom, palette, details] {
        for e in children_of(w, root) {
            despawn(w, e);
        }
    }
    s.actions.retain(|_, a| !matches!(a, Action::Canvas(_)));
    s.chrome.anchors = [None, None];

    // Details and what the tool does.
    let details_open = s.chrome.details_open;
    let t = floating(w, top_left, Axis2::Row, Absolute::default(), 8.);
    w.remove::<Absolute>(t);
    icon(
        w,
        t,
        s,
        Icon::PanelLeft,
        "Details",
        Do::Details,
        details_open,
        true,
    );
    let line = hint(&n);
    if !line.is_empty() {
        caption(w, top_left, &line);
    }
    if details_open {
        w.remove::<Hidden>(details);
        caption(w, details, "Layers");
        if n.layers.is_empty() {
            caption(w, details, "Nothing on the canvas yet.");
        }
        for (id, label, depth, visible, locked) in &n.layers {
            let row = panel(
                w,
                details,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 0.,
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
            if *depth > 0 {
                panel(
                    w,
                    row,
                    Layout {
                        width: Size::Px(14. * *depth as f32),
                        height: Size::Px(1.),
                        ..Layout::row()
                    },
                );
            }
            let chosen = n.selection.contains(id);
            let pick = word(w, row, s, label, Do::Layer(id.clone()), chosen, !*locked);
            w.get_mut::<Layout>(pick).unwrap().width = Size::Fill;
            w.remove::<crate::native_style::FitLabel>(pick);
            icon(
                w,
                row,
                s,
                if *visible { Icon::Eye } else { Icon::EyeOff },
                if *visible { "Hide layer" } else { "Show layer" },
                Do::Visible(id.clone()),
                false,
                true,
            );
            icon(
                w,
                row,
                s,
                Icon::Lock,
                if *locked {
                    "Unlock layer"
                } else {
                    "Lock layer"
                },
                Do::Lock(id.clone()),
                *locked,
                true,
            );
        }
    } else {
        w.insert(details, Hidden);
    }

    // History, fit, and style.
    icon(
        w,
        top_right,
        s,
        Icon::Undo2,
        "Undo",
        Do::Undo,
        false,
        n.undo,
    );
    icon(
        w,
        top_right,
        s,
        Icon::Redo2,
        "Redo",
        Do::Redo,
        false,
        n.redo,
    );
    divider(w, top_right);
    icon(
        w,
        top_right,
        s,
        Icon::Maximize2,
        "Fit canvas",
        Do::Fit,
        false,
        true,
    );
    icon(
        w,
        top_right,
        s,
        Icon::Scan,
        "Fit selection",
        Do::FitSelection,
        false,
        !n.selection.is_empty(),
    );
    divider(w, top_right);
    let dot = w.spawn((
        Layout {
            width: Size::Px(12.),
            height: Size::Px(12.),
            ..Layout::column()
        },
        rndr_canvas_ui::paint::parse_color(&n.stroke),
        Radius(6.),
    ));
    set_parent(w, dot, top_right);
    let palette_open = s.chrome.palette_open;
    word(w, top_right, s, "Style", Do::Palette, palette_open, true);

    // Style palette.
    if palette_open && n.tool != Tool::Hand {
        w.remove::<Hidden>(palette);
        let header = panel(
            w,
            palette,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 6.,
                ..Layout::row()
            },
        );
        w.insert(
            header,
            Align {
                items: Items::Center,
                justify: Justify::SpaceBetween,
            },
        );
        let context = match n.selection.len() {
            0 if n.tool == Tool::Select => "Selection".to_string(),
            0 => "Next object".into(),
            1 => "1 selected".into(),
            k => format!("{k} selected"),
        };
        caption(w, header, &context);
        icon(
            w,
            header,
            s,
            Icon::X,
            "Hide style palette",
            Do::Palette,
            false,
            true,
        );
        if n.tool != Tool::Eraser {
            let row = panel(
                w,
                palette,
                Layout {
                    width: Size::Px(232.),
                    height: Size::Hug,
                    gap: 2.,
                    ..Layout::row()
                },
            );
            w.insert(row, Wrap { line_gap: 2. });
            for (name, color) in COLORS {
                swatch(w, row, s, name, color, n.stroke.eq_ignore_ascii_case(color));
            }
        }
        caption(w, palette, "Size");
        let row = panel(
            w,
            palette,
            Layout {
                width: Size::Hug,
                height: Size::Hug,
                gap: 2.,
                ..Layout::row()
            },
        );
        let text_sized = n.tool == Tool::Text || (n.tool == Tool::Select && n.texts);
        for (i, (name, width, font)) in SIZES.into_iter().enumerate() {
            let chosen = if text_sized {
                n.font == font
            } else {
                n.width == width
            };
            word(w, row, s, name, Do::Size(i), chosen, true);
        }
        let fills = matches!(n.tool, Tool::Rectangle | Tool::Ellipse | Tool::Frame)
            || (n.tool == Tool::Select && n.shapes);
        if fills {
            caption(w, palette, "Fill");
            let row = panel(
                w,
                palette,
                Layout {
                    width: Size::Hug,
                    height: Size::Hug,
                    gap: 2.,
                    ..Layout::row()
                },
            );
            for (name, fill) in [
                ("None", Fill::None),
                ("Soft", Fill::Soft),
                ("Solid", Fill::Solid),
            ] {
                word(w, row, s, name, Do::Fill(fill), n.fill == fill, true);
            }
        }
    } else {
        w.insert(palette, Hidden);
    }

    // Selection actions, the shape picker, then the tool strip.
    if s.ask.open {
        ask_panel(w, s, bottom, &n, typing);
    } else if n.tool == Tool::Select && !n.selection.is_empty() && !n.editing {
        let bar = floating(w, bottom, Axis2::Row, Absolute::default(), 9.);
        w.remove::<Absolute>(bar);
        panel(
            w,
            bar,
            Layout {
                width: Size::Px(4.),
                height: Size::Px(1.),
                ..Layout::row()
            },
        );
        caption(
            w,
            bar,
            &if n.selection.len() == 1 {
                "1 selected".to_string()
            } else {
                format!("{} selected", n.selection.len())
            },
        );
        if n.selection.len() > 1 {
            icon(
                w,
                bar,
                s,
                Icon::Group,
                "Group selection",
                Do::Group,
                false,
                true,
            );
        }
        if n.groups {
            icon(
                w,
                bar,
                s,
                Icon::Ungroup,
                "Ungroup selection",
                Do::Ungroup,
                false,
                true,
            );
        }
        let align = word(w, bar, s, "Align", Do::AlignMenu, false, true);
        let arrange = word(w, bar, s, "Arrange", Do::ArrangeMenu, false, true);
        s.chrome.anchors = [Some(align), Some(arrange)];
        icon(
            w,
            bar,
            s,
            Icon::CopyPlus,
            "Duplicate selection",
            Do::Duplicate,
            false,
            true,
        );
        icon(
            w,
            bar,
            s,
            Icon::Trash2,
            "Delete selection",
            Do::Delete,
            false,
            true,
        );
    }
    if s.chrome.shapes_open {
        let picker = floating(w, bottom, Axis2::Row, Absolute::default(), 9.);
        w.remove::<Absolute>(picker);
        for (tool, glyph, name) in SHAPES {
            icon(
                w,
                picker,
                s,
                glyph,
                name,
                Do::Tool(tool),
                n.tool == tool,
                true,
            );
        }
    }
    let strip = floating(w, bottom, Axis2::Row, Absolute::default(), 11.);
    w.remove::<Absolute>(strip);
    for (tool, glyph, name) in [
        (Tool::Select, Icon::MousePointer2, "Select (V)"),
        (Tool::Hand, Icon::Hand, "Move (H)"),
        (Tool::Pen, Icon::PenTool, "Draw (P)"),
        (Tool::Eraser, Icon::Eraser, "Erase (E)"),
    ] {
        icon(
            w,
            strip,
            s,
            glyph,
            name,
            Do::Tool(tool),
            n.tool == tool,
            true,
        );
    }
    let shapes_chosen = is_shape(n.tool) || s.chrome.shapes_open;
    icon(
        w,
        strip,
        s,
        Icon::Shapes,
        "Shapes (R)",
        Do::Shapes,
        shapes_chosen,
        true,
    );
    icon(
        w,
        strip,
        s,
        Icon::Type,
        "Text (T)",
        Do::Tool(Tool::Text),
        n.tool == Tool::Text,
        true,
    );
    divider(w, strip);
    let importing = s.importing.is_some();
    icon(
        w,
        strip,
        s,
        Icon::ImagePlus,
        "Add image",
        Do::Image,
        false,
        !importing,
    );
    // Lit while open, or while a proposal waits for a decision.
    let waiting = s.ask.view.as_ref().is_some_and(|v| v.ready());
    icon(
        w,
        strip,
        s,
        Icon::Sparkles,
        "Ask Medousa",
        Do::Ask,
        s.ask.open || waiting,
        true,
    );

    // Zoom.
    icon(
        w,
        zoom,
        s,
        Icon::Minus,
        "Zoom out",
        Do::ZoomOut,
        false,
        true,
    );
    word(w, zoom, s, &format!("{}%", n.zoom), Do::Fit, false, true);
    icon(w, zoom, s, Icon::Plus, "Zoom in", Do::ZoomIn, false, true);
}

/// The Ask popover: what is selected, the prompt, and the latest request.
fn ask_panel(w: &mut World, s: &mut State, bottom: Entity, n: &Snapshot, typing: bool) {
    let panel_root = floating(w, bottom, Axis2::Column, Absolute::default(), 11.);
    w.remove::<Absolute>(panel_root);
    {
        let l = w.get_mut::<Layout>(panel_root).unwrap();
        l.width = Size::Px(440.);
        l.padding = 10.;
        l.gap = 8.;
    }
    w.insert(
        panel_root,
        Align {
            items: Items::Stretch,
            justify: Justify::Start,
        },
    );
    let header = panel(
        w,
        panel_root,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    w.insert(
        header,
        Align {
            items: Items::Center,
            justify: Justify::SpaceBetween,
        },
    );
    caption(
        w,
        header,
        &match n.selection.len() {
            0 => "Select an object or frame to work with Medousa".to_string(),
            1 => "1 object selected".into(),
            k => format!("{k} objects selected"),
        },
    );
    // The caption takes what the close button leaves and wraps; at its
    // natural width a long one pushed the button out of the panel.
    if let Some(text) = children_of(w, header).last().copied() {
        w.get_mut::<Layout>(text).unwrap().width = Size::Fill;
        w.get_mut::<Label>(text).unwrap().overflow = TextOverflow::Wrap;
    }
    icon(
        w,
        header,
        s,
        Icon::X,
        "Close Ask Medousa",
        Do::Ask,
        false,
        true,
    );

    let ask = &s.ask;
    let executing = ask.executing();
    let busy = ask.sending() || ask.pending.is_some() || executing;
    let row = panel(
        w,
        panel_root,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
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
    let field = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(36.),
            padding: 8.,
            ..Layout::column()
        },
        Label::new(ask.draft.clone()),
        Semantics {
            name: Some("Ask about selected work".into()),
            ..Default::default()
        },
    ));
    if !busy {
        w.insert(field, Focusable);
        w.insert(
            field,
            Prompt {
                submit_on_enter: true,
                max_lines: 1,
                ..Default::default()
            },
        );
        let hint = if ask.view.as_ref().is_some_and(|v| v.source == Source::Job) {
            "Keep refining this direction…"
        } else {
            "Make this feel a little more playful…"
        };
        w.insert(field, Placeholder::new(hint));
    }
    set_parent(w, field, row);
    visual::plane(w, field, visual::rail());
    w.insert(field, Radius(8.));
    if executing {
        icon(
            w,
            row,
            s,
            Icon::Square,
            "Cancel creative request",
            Do::AskDismiss,
            false,
            !s.ask.sending(),
        );
    } else {
        let ready = !busy && !n.selection.is_empty() && !s.ask.draft.trim().is_empty();
        icon(
            w,
            row,
            s,
            Icon::ArrowUp,
            "Create a proposal",
            Do::AskSend,
            false,
            ready,
        );
    }
    s.ask.field = Some(field);
    if typing && !busy {
        w.resource_mut::<Focus>().unwrap().entity = Some(field);
    }

    // The latest request, as medousa-home words it.
    let ask = &s.ask;
    let view = ask.view.clone();
    let line = |w: &mut World, text: &str| {
        let e = rndr_ui::text(w, panel_root, text, Typography::Secondary).unwrap();
        w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
        w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    };
    if ask.sending() && ask.pending.is_some() {
        line(w, "Sending request…");
    } else if ask.pending.is_some() {
        line(
            w,
            "Your request is saved here. Retry to confirm it was received.",
        );
        word(w, panel_root, s, "Retry request", Do::AskRetry, false, true);
    } else if let Some(v) = &view {
        use medousa_types::CreativeJobStatus as S;
        match v.status {
            S::Queued | S::Dispatching | S::Running => line(w, "Working on your selection…"),
            S::Ready | S::Stale if v.ready() => {
                let revision = w
                    .resource::<CanvasView>()
                    .map_or(0, |c| c.editor.mutation().document.revision);
                let stale = v.status == S::Stale || v.base.is_some_and(|b| b != revision);
                let head = panel(
                    w,
                    panel_root,
                    Layout {
                        width: Size::Fill,
                        height: Size::Hug,
                        gap: 6.,
                        ..Layout::row()
                    },
                );
                w.insert(
                    head,
                    Align {
                        items: Items::Center,
                        justify: Justify::SpaceBetween,
                    },
                );
                caption(
                    w,
                    head,
                    if stale {
                        "Medousa · Canvas updated"
                    } else {
                        "Medousa · Preview"
                    },
                );
                icon(
                    w,
                    head,
                    s,
                    Icon::X,
                    "Dismiss proposal",
                    Do::AskDismiss,
                    false,
                    !s.ask.sending(),
                );
                line(w, v.label.as_deref().unwrap_or("Proposal"));
                if stale {
                    line(
                        w,
                        "Keeps your latest canvas edits. Applying checks for conflicts again.",
                    );
                }
                let choices = panel(
                    w,
                    panel_root,
                    Layout {
                        width: Size::Fill,
                        height: Size::Hug,
                        gap: 4.,
                        ..Layout::row()
                    },
                );
                let preview = s.ask.preview;
                word(
                    w,
                    choices,
                    s,
                    "Original",
                    Do::AskPreview(false),
                    !preview,
                    true,
                );
                word(
                    w,
                    choices,
                    s,
                    "Preview",
                    Do::AskPreview(true),
                    preview,
                    true,
                );
                word(
                    w,
                    choices,
                    s,
                    "Use this",
                    Do::AskUse,
                    false,
                    !s.ask.sending(),
                );
            }
            S::Accepted => line(w, "Applied to the canvas. You can undo this change."),
            S::Cancelled => line(w, "Request cancelled. Your original is still here."),
            S::Failed | S::Interrupted => line(
                w,
                v.error
                    .as_deref()
                    .unwrap_or("This request couldn’t finish. Your original is still here."),
            ),
            _ => {}
        }
        if !v.session.is_empty() {
            word(
                w,
                panel_root,
                s,
                "Open conversation",
                Do::AskConversation,
                false,
                true,
            );
        }
    }
    if let Some(error) = s.ask.error.clone() {
        line(w, &error);
        if s.ask.decision.is_some() {
            word(
                w,
                panel_root,
                s,
                "Retry decision",
                Do::AskRetry,
                false,
                !s.ask.sending(),
            );
        }
    }
}

/// Center the bottom group and size the details panel to the canvas.
pub(super) fn place(w: &mut World, s: &State) {
    let c = &s.chrome;
    let Some(body) = w.get::<Rect>(c.body).copied() else {
        return;
    };
    if let Some(bottom) = w.get::<Rect>(c.bottom).copied() {
        let left = ((body.w - bottom.w) / 2.).max(INSET).round();
        if let Some(at) = w.get_mut::<Absolute>(c.bottom)
            && at.left != Some(left)
        {
            at.left = Some(left);
        }
    }
    // Fit the layers, up to the room between the top controls and the
    // tool strip; past that the panel scrolls.
    let room = (body.h - 56. - 88.).max(120.);
    let natural: f32 = children_of(w, c.details)
        .iter()
        .filter_map(|e| w.get::<Rect>(*e).map(|r| r.h))
        .sum::<f32>()
        + 8. * children_of(w, c.details).len().saturating_sub(1) as f32
        + 16.;
    let height = Size::Px(natural.min(room).round());
    if let Some(l) = w.get_mut::<Layout>(c.details)
        && l.height != height
    {
        l.height = height;
    }
}

fn viewport(w: &World, s: &State) -> [f32; 2] {
    w.get::<Rect>(s.slot).map_or([800., 600.], |r| [r.w, r.h])
}

fn measure(w: &World) -> Box<dyn rndr_canvas::TextMeasure> {
    match rndr_canvas_ui::paint::EngineMeasure::from_world(w) {
        Some(m) => Box::new(m),
        None => Box::new(rndr_canvas::EstimateMeasure),
    }
}

fn open_menu(w: &mut World, s: &mut State, anchor: Entity, entries: Vec<(&str, Do, bool)>) {
    let mut actions = HashMap::new();
    let entries: Vec<menu::MenuEntry> = entries
        .into_iter()
        .map(|(label, a, enabled)| {
            let id = next();
            actions.insert(id, Action::Canvas(a));
            menu::MenuEntry::from(menu::MenuItem::new(id, label).enabled(enabled))
        })
        .collect();
    if let Ok(handle) = menu::open_menu(w, anchor, menu::MenuAt::Below, &entries) {
        s.menu = Some((handle, actions));
    }
}

/// Runs a menu pick on the frame RNDR delivers it.
pub(super) fn picks(w: &mut World, s: &mut State) {
    for ev in events::<menu::MenuPicked>(w).to_vec() {
        if s.menu.as_ref().is_some_and(|(m, _)| m.root == ev.menu) {
            let (_, mut actions) = s.menu.take().unwrap();
            if let Some(Action::Canvas(a)) = actions.remove(&ev.item) {
                run(w, s, a);
            }
        }
    }
}

pub(super) fn run(w: &mut World, s: &mut State, a: Do) {
    let port = viewport(w, s);
    let m = measure(w);
    let anchors = s.chrome.anchors;
    let c = &mut s.chrome;
    match a {
        Do::Shapes => {
            c.shapes_open = !c.shapes_open;
            if c.shapes_open
                && let Some(v) = w.resource_mut::<CanvasView>()
                && !is_shape(v.editor.tool())
            {
                v.editor.set_tool(Tool::Rectangle);
            }
        }
        Do::Palette => c.palette_open = !c.palette_open,
        Do::Ask => {
            s.ask.open = !s.ask.open;
            if s.ask.open {
                ask::refresh(w, s);
            }
        }
        Do::AskSend => ask::start(w, s),
        Do::AskPreview(on) => s.ask.preview = on,
        Do::AskUse => ask::decide(w, s, true),
        Do::AskDismiss => ask::decide(w, s, false),
        Do::AskRetry => ask::retry(w, s),
        Do::AskConversation => {
            if let Some(session) = s.ask.view.as_ref().map(|v| v.session.clone())
                && !session.is_empty()
            {
                crate::ui::open_conversation(w, session);
            }
        }
        Do::Image => {
            if s.importing.is_none()
                && let Some(id) = w
                    .resource::<CanvasView>()
                    .map(|v| v.editor.document_id().to_string())
            {
                s.importing = Some(s.enqueue(Kind::Import(id)));
                s.message.clear();
                s.redraw = true;
            }
        }
        Do::Details => c.details_open = !c.details_open,
        Do::AlignMenu => {
            if let Some(anchor) = anchors[0] {
                let three = w
                    .resource::<CanvasView>()
                    .is_some_and(|v| v.editor.selection().len() >= 3);
                open_menu(
                    w,
                    s,
                    anchor,
                    vec![
                        ("Align left", Do::Align(Alignment::Left), true),
                        ("Align center", Do::Align(Alignment::Center), true),
                        ("Align right", Do::Align(Alignment::Right), true),
                        ("Align top", Do::Align(Alignment::Top), true),
                        ("Align middle", Do::Align(Alignment::Middle), true),
                        ("Align bottom", Do::Align(Alignment::Bottom), true),
                        ("Distribute horizontally", Do::Distribute(Axis::X), three),
                        ("Distribute vertically", Do::Distribute(Axis::Y), three),
                    ],
                );
            }
        }
        Do::ArrangeMenu => {
            if let Some(anchor) = anchors[1] {
                open_menu(
                    w,
                    s,
                    anchor,
                    vec![
                        ("Bring forward", Do::Arrange(true), true),
                        ("Send backward", Do::Arrange(false), true),
                    ],
                );
            }
        }
        a => {
            if matches!(a, Do::Tool(_)) {
                // Picking a shape closes the picker; other tools do too.
                c.shapes_open = false;
            }
            let Some(v) = w.resource_mut::<CanvasView>() else {
                return;
            };
            let e = &mut v.editor;
            match a {
                Do::Tool(tool) => e.set_tool(tool),
                Do::Color(color) => {
                    e.apply_style(StyleChange::Stroke(color.into()));
                }
                Do::Size(i) => {
                    let (_, width, font) = SIZES[i.min(SIZES.len() - 1)];
                    let picked: Vec<Node> = e
                        .nodes()
                        .into_iter()
                        .filter(|n| e.selection().contains(&n.id))
                        .collect();
                    let texts = !picked.is_empty()
                        && picked
                            .iter()
                            .all(|n| matches!(n.content, Content::Text { .. }));
                    if e.tool() == Tool::Text || (e.tool() == Tool::Select && texts) {
                        e.apply_style(StyleChange::FontSize(font));
                    } else {
                        e.apply_style(StyleChange::StrokeWidth(width));
                    }
                }
                Do::Fill(fill) => {
                    let value = fill.value(&e.style.stroke);
                    e.apply_style(StyleChange::Fill(value));
                }
                Do::ZoomIn => e.zoom_by(1.25, port),
                Do::ZoomOut => e.zoom_by(0.8, port),
                Do::Fit => e.zoom_to_fit(port, m.as_ref()),
                Do::FitSelection => e.zoom_to_selection(port, m.as_ref()),
                Do::Undo => {
                    e.undo();
                }
                Do::Redo => {
                    e.redo();
                }
                Do::Group => {
                    e.group();
                }
                Do::Ungroup => {
                    e.ungroup();
                }
                Do::Align(alignment) => {
                    e.align(alignment);
                }
                Do::Distribute(axis) => {
                    e.distribute(axis);
                }
                Do::Arrange(forward) => {
                    e.arrange(forward);
                }
                Do::Duplicate => {
                    e.duplicate();
                }
                Do::Delete => {
                    e.delete_selection();
                }
                Do::Layer(id) => e.select(&[id]),
                Do::Visible(id) => {
                    e.toggle_visible(&id);
                }
                Do::Lock(id) => {
                    e.toggle_locked(&id);
                }
                _ => {}
            }
        }
    }
    s.chrome.signature.clear();
}
