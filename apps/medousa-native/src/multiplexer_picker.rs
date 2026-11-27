//! Workspace "Open or create": one searchable list of what can go in the
//! focused pane, so nothing has to be opened in its own view first. RNDR's
//! palette owns the field, matching, keyboard and focus scope; this module
//! only supplies the catalog and places the choice.
use super::*;
use rndr_ui::palette::{
    CommandPalette, PaletteActivated, PaletteCommand, PaletteHandle, PaletteProps,
};

const WIDTH: f32 = 520.;
/// Below the pane headers, so the list reads as belonging to the content.
const TOP: f32 = 56.;
const COMMAND: u64 = 0x4d57_2000;
/// Per kind, so one large vault never crowds out everything else.
const LIMIT: usize = 200;

#[derive(Clone, Debug)]
pub(super) enum Pick {
    New(SurfaceId),
    Open(Resource, String),
    Browse(SurfaceId),
}

/// Kind, group label, icon, "new" title, and the fallback to its own view.
pub(super) const KINDS: [(SurfaceId, &str, Icon, Option<&str>, &str); 6] = [
    (
        SurfaceId::Chat,
        "Chats",
        Icon::MessageCircle,
        Some("New chat"),
        "Browse all chats…",
    ),
    (
        SurfaceId::Terminal,
        "Shells",
        Icon::Terminal,
        Some("New shell"),
        "Browse shells…",
    ),
    (SurfaceId::Notes, "Notes", Icon::Book, None, "Browse notes…"),
    (
        SurfaceId::Code,
        "Code",
        Icon::FileCode,
        None,
        "Browse code…",
    ),
    (
        SurfaceId::Create,
        "Create",
        Icon::PenTool,
        None,
        "Open Create…",
    ),
    (
        SurfaceId::Reviews,
        "Reviews",
        Icon::CheckCheck,
        None,
        "Open Reviews…",
    ),
];

pub(super) fn mount(w: &mut World, parent: Entity) -> PaletteHandle {
    let anchor = w.spawn((
        Layout {
            width: Size::Px(WIDTH),
            height: Size::Px(1.),
            ..Layout::column()
        },
        Absolute {
            left: Some(0.),
            top: Some(TOP),
            ..Default::default()
        },
        Focusable,
        TabStop(false),
    ));
    set_parent(w, anchor, parent);
    let mut props = PaletteProps::new("Open or create");
    props.placeholder = "Search chats, shells, notes and code".into();
    props.empty = "Nothing matches".into();
    props.width = WIDTH;
    props.list_height = 360.;
    CommandPalette::mount(w, anchor, UiId(0x4d57_0100), props).expect("workspace picker")
}

/// An unsent chat reads as a draft, never as a second "New chat".
pub(super) fn display_title(item: &Item) -> String {
    match &item.resource {
        Resource::Chat { session_id, .. } if session_id.starts_with("pending:") => {
            "Unsent draft".into()
        }
        _ => item.title.clone(),
    }
}
fn base(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
}

fn catalog(w: &World, only: Option<SurfaceId>) -> (Vec<PaletteCommand>, Vec<Pick>) {
    let Some(s) = w.resource::<State>() else {
        return (vec![], vec![]);
    };
    let mut commands = vec![];
    let mut picks = vec![];
    let mut push = |title: &str, group: &str, icon: Icon, pick: Pick| {
        let mut c = PaletteCommand::new(COMMAND + picks.len() as u64, title);
        c.group = Some(group.into());
        c.icon = Some(icon);
        commands.push(c);
        picks.push(pick);
    };
    for (kind, group, icon, new, browse) in KINDS {
        if only.is_some_and(|o| o != kind) {
            continue;
        }
        if let Some(title) = new {
            push(title, group, Icon::Plus, Pick::New(kind));
        }
        let mut seen: Vec<Resource> = vec![];
        for item in s.items.iter().filter(|i| surface(&i.resource) == kind) {
            seen.push(item.resource.clone());
            push(
                &display_title(item),
                group,
                icon,
                Pick::Open(item.resource.clone(), item.title.clone()),
            );
        }
        let mut known: Vec<(String, Resource)> = vec![];
        match kind {
            SurfaceId::Chat => {
                for (session, title) in crate::ui::conversations::workspace_catalog(w) {
                    let Some(owner) = crate::workshops::owner(w, Some(&session)) else {
                        continue;
                    };
                    let title = if title.trim().is_empty() {
                        "Untitled chat".to_owned()
                    } else {
                        title
                    };
                    known.push((
                        title,
                        Resource::Chat {
                            session_id: session,
                            workshop_id: owner,
                        },
                    ));
                }
            }
            SurfaceId::Notes => {
                if let Some(owner) = owner(w, SurfaceId::Notes) {
                    for (path, title) in crate::notes::workspace_catalog(w) {
                        let title = if title.trim().is_empty() {
                            base(&path)
                        } else {
                            title
                        };
                        known.push((
                            title,
                            Resource::Note {
                                path,
                                workshop_id: owner.clone(),
                            },
                        ));
                    }
                }
            }
            SurfaceId::Code => {
                if let Some(owner) = owner(w, SurfaceId::Code) {
                    for (work, _project, path) in crate::code::workspace_catalog(w) {
                        known.push((
                            path.clone(),
                            Resource::Code {
                                path,
                                work_id: work,
                                workshop_id: owner.clone(),
                            },
                        ));
                    }
                }
            }
            _ => {}
        }
        for (title, resource) in known.into_iter().take(LIMIT) {
            if seen.contains(&resource) {
                continue;
            }
            let tab = match &resource {
                Resource::Code { path, .. } => base(path),
                _ => title.clone(),
            };
            push(&title, group, icon, Pick::Open(resource, tab));
        }
        push(browse, group, Icon::ArrowUpRight, Pick::Browse(kind));
    }
    (commands, picks)
}

/// Refresh the catalog for `only` (or every kind) and show the picker.
pub(super) fn open(w: &mut World, only: Option<SurfaceId>) {
    let (commands, picks) = catalog(w, only);
    let Some(s) = w.resource_mut::<State>() else {
        return;
    };
    s.picks = picks;
    let Some(handle) = s.picker else {
        return;
    };
    let _ = handle.set_commands(w, &commands);
    handle.open(w);
}

/// Each frame: center over the panes and place whatever was chosen.
pub(super) fn update(w: &mut World) {
    let Some((handle, body)) = w
        .resource::<State>()
        .and_then(|s| Some((s.picker?, s.chrome.as_ref()?.body)))
    else {
        return;
    };
    let parent = w.get::<Parent>(handle.anchor).map(|p| p.entity);
    if let (Some(area), Some(origin)) = (
        w.get::<Rect>(body).copied(),
        parent.and_then(|p| w.get::<Rect>(p)).copied(),
    ) {
        let left = (area.x - origin.x + (area.w - WIDTH) / 2.).max(0.);
        let absolute = w.get_mut::<Absolute>(handle.anchor).unwrap();
        if absolute.left != Some(left) {
            absolute.left = Some(left);
        }
    }
    for event in rndr_core::events::<PaletteActivated>(w).to_vec() {
        if event.palette != handle.root {
            continue;
        }
        handle.close(w);
        let pick = w.resource::<State>().and_then(|s| {
            s.picks
                .get(event.id.0.wrapping_sub(COMMAND) as usize)
                .cloned()
        });
        if let Some(pick) = pick {
            place(w, pick);
        }
    }
}

pub(super) fn place(w: &mut World, pick: Pick) {
    match pick {
        Pick::New(SurfaceId::Terminal) => run(w, Action::NewShell),
        Pick::New(_) => run(w, Action::NewChat),
        Pick::Browse(kind) => run(w, Action::Browse(kind)),
        Pick::Open(resource, title) => {
            let known = w.resource::<State>().is_some_and(|s| {
                s.items.iter().any(|i| i.resource == resource)
                    || s.model
                        .active()
                        .groups
                        .iter()
                        .flat_map(|g| &g.tabs)
                        .any(|t| t.resource == resource)
            });
            if known {
                run(w, Action::Open(resource));
                return;
            }
            if !can_deactivate(w) {
                return;
            }
            let s = w.resource_mut::<State>().unwrap();
            if let Err(error) = s.model.open(resource.clone(), title) {
                s.notice = error;
                return;
            }
            s.rendered.clear();
            s.catalog_key.clear();
            // Load the backing view in place; its own surface is never shown.
            reopen(w, &resource);
            sync(w);
        }
    }
}

#[cfg(test)]
pub(super) fn ids(w: &World, only: Option<SurfaceId>) -> Vec<(String, Pick)> {
    let (commands, picks) = catalog(w, only);
    commands.into_iter().map(|c| c.title).zip(picks).collect()
}
