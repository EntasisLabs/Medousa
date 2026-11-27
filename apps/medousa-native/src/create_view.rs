//! Create's sidebar (new from a starter, pinned and recent projects, save
//! status) and the frame the canvas, toolbar and inspector mount into.
use super::*;
use crate::native_style as visual;
use rndr_ui::{Typography, text};

pub(super) struct Parts {
    pub sidebar: Entity,
    pub editor: Entity,
    pub empty: Entity,
    pub slot: Entity,
    pub chrome: super::canvas::Chrome,
}

fn panel(w: &mut World, parent: Entity, layout: Layout) -> Entity {
    let e = w.spawn((layout,));
    set_parent(w, e, parent);
    e
}

pub(super) fn mount(w: &mut World, root: Entity, slot_id: u64) -> Parts {
    let sidebar = panel(
        w,
        root,
        Layout {
            width: Size::Px(288.),
            height: Size::Fill,
            gap: 2.,
            ..Layout::column()
        },
    );
    visual::navigation_frame(w, sidebar);
    w.insert(sidebar, Scroll::default());
    let main = panel(
        w,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.,
            ..Layout::column()
        },
    );
    let editor = panel(
        w,
        main,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.,
            min_height: 0.,
            ..Layout::column()
        },
    );
    w.insert(editor, Hidden);
    // The canvas fills the editor; tools, style, zoom and details float
    // over it, as in medousa-home's editor.
    let body = panel(
        w,
        editor,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.,
            min_height: 0.,
            ..Layout::column()
        },
    );
    let slot = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.,
            min_height: 0.,
            ..Layout::column()
        },
        SurfaceSlot { id: slot_id },
        Semantics {
            name: Some("Canvas".into()),
            ..Default::default()
        },
    ));
    set_parent(w, slot, body);
    let chrome = super::canvas::mount(w, body);
    let empty = panel(
        w,
        main,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
    );
    w.insert(
        empty,
        Align {
            items: Items::Center,
            justify: Justify::Center,
        },
    );
    Parts {
        sidebar,
        editor,
        empty,
        slot,
        chrome,
    }
}

fn label(w: &mut World, parent: Entity, value: &str, kind: Typography) -> Entity {
    let e = text(w, parent, value, kind).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
    e
}

fn heading(w: &mut World, parent: Entity, value: &str) {
    let e = label(w, parent, value, Typography::Secondary);
    visual::section_heading(w, e);
}

fn button(
    w: &mut World,
    parent: Entity,
    s: &mut State,
    name: &str,
    a: Action,
    enabled: bool,
) -> rndr_ui::controls::ControlHandle {
    let id = next();
    let h = visual::document_action(w, parent, id, name, enabled);
    s.actions.insert(id, a);
    h
}

/// A full-width project row; the open one sits on the selected wash.
fn project(w: &mut World, parent: Entity, s: &mut State, p: &CreativeProjectSummary) {
    let title = if p.title.trim().is_empty() {
        "Untitled canvas"
    } else {
        p.title.as_str()
    };
    let h = button(w, parent, s, title, Action::Open(p.id.clone()), true);
    if let Some(fit) = w.get_mut::<Layout>(h.root) {
        fit.width = Size::Fill;
    }
    w.remove::<crate::native_style::FitLabel>(h.root);
    if s.open.as_deref() == Some(p.id.as_str())
        && let Some(face) = visual::label_face(w, h.root)
    {
        visual::plane(w, face, rndr_ui::Token::Selected);
    }
}

pub(super) fn render(w: &mut World, s: &mut State) {
    if w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|e| {
            crate::surfaces::descendant(w, e, s.sidebar)
                || crate::surfaces::descendant(w, e, s.empty)
        })
    {
        w.resource_mut::<Focus>().unwrap().entity = None;
    }
    for e in children_of(w, s.sidebar)
        .into_iter()
        .chain(children_of(w, s.empty))
    {
        despawn(w, e);
    }
    // Only the sidebar's own actions: the canvas controls share this table and
    // are rebuilt on their own schedule, so clearing them here left every
    // floating button pointing at an id that no longer did anything.
    s.actions.retain(|_, a| matches!(a, Action::Canvas(_)));
    let sidebar = s.sidebar;
    let header = panel(
        w,
        sidebar,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 4.,
            ..Layout::row()
        },
    );
    w.insert(
        header,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    label(w, header, "Create", Typography::Emphasis);
    let id = next();
    visual::icon_action(
        w,
        header,
        id,
        Icon::RefreshCw,
        "Refresh projects",
        s.list.is_none(),
    );
    s.actions.insert(id, Action::Refresh);

    heading(w, sidebar, "New");
    let creating = s.creating.is_some();
    for starter in Starter::ALL {
        let h = button(
            w,
            sidebar,
            s,
            starter.label(),
            Action::New(starter),
            !creating,
        );
        w.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
        w.remove::<crate::native_style::FitLabel>(h.root);
    }
    let projects = s.projects.clone();
    let (pinned, recent): (Vec<_>, Vec<_>) = projects.iter().partition(|p| p.pinned);
    for (title, list) in [("Pinned", pinned), ("Recent", recent)] {
        if list.is_empty() {
            continue;
        }
        heading(w, sidebar, title);
        for p in list {
            project(w, sidebar, s, p);
        }
    }
    if projects.is_empty() && s.list.is_none() && s.message.is_empty() {
        label(w, sidebar, "No projects yet.", Typography::Secondary);
    }

    let (saving, stalled) = s.status;
    let status = if stalled {
        "Not saved. Your edits are kept here."
    } else if saving {
        "Saving…"
    } else {
        ""
    };
    for line in [status, s.message.as_str()] {
        if !line.is_empty() {
            label(w, sidebar, line, Typography::Secondary);
        }
    }
    if stalled || (s.creating.is_some() && !s.message.is_empty()) {
        button(w, sidebar, s, "Retry", Action::Retry, true);
    }

    let open = s.open.is_some()
        && w.resource::<CanvasView>()
            .is_some_and(|v| s.open.as_deref() == Some(v.editor.document_id()));
    if open {
        w.remove::<Hidden>(s.editor);
        w.insert(s.empty, Hidden);
    } else {
        w.insert(s.editor, Hidden);
        w.remove::<Hidden>(s.empty);
        let empty = s.empty;
        let opening = s.open.is_some();
        let landing = visual::landing(
            w,
            empty,
            Icon::PenTool,
            "Create",
            if opening {
                "Opening…"
            } else {
                "Sketch, mark up and compose on a canvas you and Medousa share."
            },
        );
        if !opening {
            for starter in Starter::ALL {
                let id = next();
                visual::landing_tile(
                    w,
                    landing.actions,
                    id,
                    starter.icon(),
                    starter.label(),
                    !creating,
                );
                s.actions.insert(id, Action::New(starter));
            }
            let mut recent = s.projects.clone();
            recent.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            if !recent.is_empty() {
                visual::landing_section(w, landing.list, "Recent");
            }
            for p in recent.into_iter().take(5) {
                let id = next();
                let title = if p.title.trim().is_empty() {
                    "Untitled canvas"
                } else {
                    p.title.as_str()
                };
                let meta = Some(visual::relative_day_str(&p.updated_at)).filter(|m| !m.is_empty());
                visual::landing_row(w, landing.list, id, title, meta);
                s.actions.insert(id, Action::Open(p.id.clone()));
            }
        }
    }
}
