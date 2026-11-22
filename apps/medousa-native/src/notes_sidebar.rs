use super::*;
use rndr_ui::menu::{MenuAt, MenuEntry, MenuItem, open_menu};
#[derive(Clone, Copy)]
pub(super) enum Picker {
    Vault,
    Group,
    Mode,
}
const GROUPS: &[(&str, &str)] = &[
    ("Journal", "journal/"),
    ("Projects", "projects/"),
    ("Finance", "finance/"),
    ("Inbox", "inbox/"),
];
fn mode_name(mode: Browse) -> &'static str {
    match mode {
        Browse::Recent => "Recent",
        Browse::Folders => "Folders",
        Browse::All => "Modified",
    }
}
pub(super) fn sidebar_header(w: &mut World, n: &mut Notes, header: Entity) {
    let search = n.search.unwrap();
    let row = n.search_row.unwrap();
    if n.searching {
        w.insert(header, Hidden);
        w.remove::<Hidden>(row);
        w.remove::<Hidden>(search);
        for child in children_of(w, row) {
            if child != search {
                despawn(w, child);
            }
        }
        let id = next();
        visual::icon_action(w, row, id, Icon::X, "Close search", true);
        n.actions.insert(id, Action::SearchList);
        return;
    }
    w.remove::<Hidden>(header);
    w.insert(row, Hidden);
    w.insert(search, Hidden);
    let root = n
        .roots
        .iter()
        .find(|r| r.active)
        .map_or("Vault", |r| r.label.as_str());
    let group = GROUPS
        .iter()
        .find(|(_, prefix)| Some(*prefix) == n.group)
        .map_or("All notes", |(name, _)| *name);
    for (picker, label) in [
        (Picker::Vault, root),
        (Picker::Group, group),
        (Picker::Mode, mode_name(n.browse)),
    ] {
        let id = next();
        if !matches!(picker, Picker::Vault) {
            let slash = text(w, header, "/", Typography::Label).expect("breadcrumb separator");
            w.insert(slash, UiStyle { foreground: Some(rndr_ui::Token::Secondary.into()), ..UiStyle::default() });
            w.get_mut::<Layout>(slash).unwrap().width = Size::Hug;
        }
        let h = visual::document_action(w, header, id, label, true);
        // Every crumb is sized to its own label so separators stay evenly
        // spaced; a short vault name leaves room for the longer group label.
        let fit = w.get_mut::<visual::FitLabel>(h.root).unwrap();
        (fit.min, fit.max) = match picker {
            Picker::Vault => (42., 96.),
            _ => (32., 120.),
        };
        w.get_mut::<LabelBox>(h.label).unwrap().inset_x = 2.;
        // Hug the face so the focus ring is uniform on every edge.
        *w.get_mut::<Layout>(h.root).unwrap() = Layout {
            padding: 1.,
            ..Layout::column()
        };
        n.actions.insert(id, Action::Picker(picker, h.root));
    }
    let spacer = w.spawn((Layout {
        width: Size::Fill,
        ..Layout::row()
    },));
    set_parent(w, spacer, header);
    for (icon, label, action, enabled) in [
        (
            Icon::Plus,
            "New note",
            Action::Manage(management::Action::New),
            management::idle(n),
        ),
        (Icon::Search, "Filter notes", Action::SearchList, true),
    ] {
        let id = next();
        visual::icon_action(w, header, id, icon, label, enabled);
        n.actions.insert(id, action);
    }
}
pub(super) fn open_picker(w: &mut World, n: &mut Notes, picker: Picker, anchor: Entity) {
    if !can_deactivate_views(w, n) {
        return;
    }
    use management::Action as M;
    let mut actions = HashMap::new();
    let mut entry = |label: &str, action: Action, checked: Option<bool>, enabled: bool| {
        let id = next();
        actions.insert(id, action);
        let mut item = MenuItem::new(id, label).enabled(enabled);
        item.checked = checked;
        MenuEntry::from(item)
    };
    let mut entries = vec![];
    match picker {
        Picker::Group => {
            entries.push(entry(
                "All notes",
                Action::Group(None),
                Some(n.group.is_none()),
                true,
            ));
            for &(name, prefix) in GROUPS {
                entries.push(entry(
                    name,
                    Action::Group(Some(prefix)),
                    Some(n.group == Some(prefix)),
                    true,
                ));
            }
        }
        Picker::Mode => {
            for mode in [Browse::Recent, Browse::Folders, Browse::All] {
                entries.push(entry(
                    mode_name(mode),
                    Action::Browse(mode),
                    Some(n.browse == mode),
                    true,
                ));
            }
        }
        Picker::Vault => {
            let idle = management::idle(n);
            for root in n.roots.iter().take(200) {
                entries.push(entry(
                    &root.label,
                    Action::Manage(M::Switch(root.id.clone())),
                    Some(root.active),
                    idle && !root.active,
                ));
            }
            entries.push(entry(
                "Manage vaults…",
                Action::Manage(M::Roots),
                None,
                idle,
            ));
            let mut tools = vec![
                entry("Refresh", Action::Refresh, None, idle),
                entry("Search whole vault…", Action::Manage(M::Search), None, idle),
                entry("Trash…", Action::Manage(M::Trash), None, idle),
            ];
            if let Some(path) = &n.selected {
                for (name, a) in [
                    ("Rename / move…", M::Move),
                    ("Move to trash…", M::Delete),
                    ("Close note", M::CloseNote),
                ] {
                    tools.push(entry(
                        name,
                        Action::Manage(a),
                        None,
                        idle && management::clean(n, path),
                    ));
                }
            }
            entries.push(MenuEntry::submenu("Vault tools", tools));
        }
    }
    if let Ok(menu) = open_menu(w, anchor, MenuAt::Below, &entries) {
        n.menu = Some((menu, actions));
    }
}
