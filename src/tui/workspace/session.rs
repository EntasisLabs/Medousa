//! Workspace session — desktops, groups, tabs (Home shellTabs subset for TUI).

use serde::{Deserialize, Serialize};

use super::split_tree::{
    SplitNode, collect_group_ids, count_leaves, leaf_order, merge_target_for_leaf, neighbor_in_direction,
    remove_leaf, split_leaf, split_leaf_at_edge,
};

/// Soft cap on leaf panes per virtual desktop (aligned with Home).
pub const MAX_SHELL_PANES: usize = 4;
/// Virtual desktops hard cap.
pub const MAX_SHELL_DESKTOPS: usize = 4;
/// Soft cap on tabs across a desktop layout.
pub const MAX_TABS: usize = 16;

pub const MAIN_GROUP_ID: &str = "main";
pub const DEFAULT_DESKTOP_NAME: &str = "Main";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Right,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellTabKind {
    Chat,
    Notes,
    Code,
    Review,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ShellTab {
    Chat {
        id: String,
        session_id: String,
        title: String,
    },
    Notes {
        id: String,
        path: String,
        title: String,
    },
    Code {
        id: String,
        path: String,
        work_id: Option<String>,
        title: String,
    },
    Review {
        id: String,
        work_id: String,
        title: String,
    },
    Terminal {
        id: String,
        session_id: String,
        work_id: Option<String>,
        title: String,
    },
}

impl ShellTab {
    pub fn id(&self) -> &str {
        match self {
            Self::Chat { id, .. }
            | Self::Notes { id, .. }
            | Self::Code { id, .. }
            | Self::Review { id, .. }
            | Self::Terminal { id, .. } => id,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Chat { title, .. }
            | Self::Notes { title, .. }
            | Self::Code { title, .. }
            | Self::Review { title, .. }
            | Self::Terminal { title, .. } => title,
        }
    }

    pub fn kind(&self) -> ShellTabKind {
        match self {
            Self::Chat { .. } => ShellTabKind::Chat,
            Self::Notes { .. } => ShellTabKind::Notes,
            Self::Code { .. } => ShellTabKind::Code,
            Self::Review { .. } => ShellTabKind::Review,
            Self::Terminal { .. } => ShellTabKind::Terminal,
        }
    }

    pub fn chat_session_id(&self) -> Option<&str> {
        match self {
            Self::Chat { session_id, .. } => Some(session_id.as_str()),
            _ => None,
        }
    }

    pub fn notes_path(&self) -> Option<&str> {
        match self {
            Self::Notes { path, .. } => Some(path.as_str()),
            _ => None,
        }
    }
}

pub type ChatTab = ShellTab;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorGroup {
    pub id: String,
    pub tab_ids: Vec<String>,
    pub active_tab_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellDesktopLayout {
    pub tabs: Vec<ShellTab>,
    pub groups: Vec<EditorGroup>,
    pub split_root: SplitNode,
    pub active_group_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoomed_group_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellDesktop {
    pub id: String,
    pub name: String,
    pub layout: ShellDesktopLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceShell {
    pub version: u32,
    pub desktops: Vec<ShellDesktop>,
    pub active_desktop_id: String,
}

pub fn new_tab_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}",
        uuid::Uuid::new_v4().simple(),
        &uuid::Uuid::new_v4().simple().to_string()[..5]
    )
}

pub fn new_group_id() -> String {
    new_tab_id("group")
}

pub fn new_chat_tab(session_id: impl Into<String>, title: impl Into<String>) -> ShellTab {
    ShellTab::Chat {
        id: new_tab_id("chat"),
        session_id: session_id.into(),
        title: title.into(),
    }
}

pub fn new_notes_tab(path: impl Into<String>, title: impl Into<String>) -> ShellTab {
    let path = path.into();
    let title = title.into();
    ShellTab::Notes {
        id: new_tab_id("notes"),
        path,
        title,
    }
}

impl WorkspaceShell {
    pub fn bootstrap(session_id: &str, title: &str) -> Self {
        let tab = new_chat_tab(session_id, title);
        let group = EditorGroup {
            id: MAIN_GROUP_ID.to_string(),
            tab_ids: vec![tab.id().to_string()],
            active_tab_id: Some(tab.id().to_string()),
        };
        let desktop_id = new_tab_id("desktop");
        let desktop = ShellDesktop {
            id: desktop_id.clone(),
            name: DEFAULT_DESKTOP_NAME.to_string(),
            layout: ShellDesktopLayout {
                tabs: vec![tab],
                groups: vec![group],
                split_root: SplitNode::Group {
                    id: MAIN_GROUP_ID.to_string(),
                },
                active_group_id: MAIN_GROUP_ID.to_string(),
                zoomed_group_id: None,
            },
        };
        Self {
            version: 1,
            desktops: vec![desktop],
            active_desktop_id: desktop_id,
        }
    }

    pub fn active_desktop(&self) -> &ShellDesktop {
        self.desktops
            .iter()
            .find(|d| d.id == self.active_desktop_id)
            .or_else(|| self.desktops.first())
            .expect("workspace always has a desktop")
    }

    pub fn active_desktop_mut(&mut self) -> &mut ShellDesktop {
        let id = self.active_desktop_id.clone();
        if let Some(idx) = self.desktops.iter().position(|d| d.id == id) {
            return &mut self.desktops[idx];
        }
        &mut self.desktops[0]
    }

    pub fn layout(&self) -> &ShellDesktopLayout {
        &self.active_desktop().layout
    }

    pub fn layout_mut(&mut self) -> &mut ShellDesktopLayout {
        &mut self.active_desktop_mut().layout
    }

    pub fn pane_count(&self) -> usize {
        count_leaves(&self.layout().split_root)
    }

    pub fn active_group(&self) -> Option<&EditorGroup> {
        let layout = self.layout();
        layout
            .groups
            .iter()
            .find(|g| g.id == layout.active_group_id)
    }

    pub fn active_tab(&self) -> Option<&ShellTab> {
        let layout = self.layout();
        let group = self.active_group()?;
        let tab_id = group.active_tab_id.as_deref()?;
        layout.tabs.iter().find(|t| t.id() == tab_id)
    }

    pub fn focused_chat_session_id(&self) -> Option<&str> {
        self.active_tab().and_then(|t| t.chat_session_id())
    }

    pub fn tab_by_id(&self, tab_id: &str) -> Option<&ShellTab> {
        self.layout().tabs.iter().find(|t| t.id() == tab_id)
    }

    pub fn group_by_id(&self, group_id: &str) -> Option<&EditorGroup> {
        self.layout().groups.iter().find(|g| g.id == group_id)
    }

    pub fn group_active_tab<'a>(&'a self, group_id: &str) -> Option<&'a ShellTab> {
        let group = self.group_by_id(group_id)?;
        let tab_id = group.active_tab_id.as_deref()?;
        self.tab_by_id(tab_id)
    }

    pub fn split_active(&mut self, direction: SplitDirection, new_session_id: &str) -> bool {
        if self.pane_count() >= MAX_SHELL_PANES {
            return false;
        }
        let from_group = self.layout().active_group_id.clone();
        let new_group_id = new_group_id();
        let Some((root, _)) = split_leaf(
            &self.layout().split_root,
            &from_group,
            direction,
            new_group_id.clone(),
        ) else {
            return false;
        };

        let tab = new_chat_tab(new_session_id, short_session_title(new_session_id));
        let tab_id = tab.id().to_string();
        let layout = self.layout_mut();
        if layout.tabs.len() >= MAX_TABS {
            return false;
        }
        layout.split_root = root;
        layout.tabs.push(tab);
        layout.groups.push(EditorGroup {
            id: new_group_id.clone(),
            tab_ids: vec![tab_id.clone()],
            active_tab_id: Some(tab_id),
        });
        layout.active_group_id = new_group_id;
        layout.zoomed_group_id = None;
        true
    }

    pub fn split_active_at_edge(&mut self, edge: SplitEdge, new_session_id: &str) -> bool {
        if self.pane_count() >= MAX_SHELL_PANES {
            return false;
        }
        let from_group = self.layout().active_group_id.clone();
        let new_group_id = new_group_id();
        let Some((root, _)) = split_leaf_at_edge(
            &self.layout().split_root,
            &from_group,
            edge,
            new_group_id.clone(),
        ) else {
            return false;
        };

        let tab = new_chat_tab(new_session_id, short_session_title(new_session_id));
        let tab_id = tab.id().to_string();
        let layout = self.layout_mut();
        if layout.tabs.len() >= MAX_TABS {
            return false;
        }
        layout.split_root = root;
        layout.tabs.push(tab);
        layout.groups.push(EditorGroup {
            id: new_group_id.clone(),
            tab_ids: vec![tab_id.clone()],
            active_tab_id: Some(tab_id),
        });
        layout.active_group_id = new_group_id;
        layout.zoomed_group_id = None;
        true
    }

    pub fn focus_neighbor(&mut self, dir: FocusDir) -> bool {
        let layout = self.layout();
        let Some(next) =
            neighbor_in_direction(&layout.split_root, &layout.active_group_id, dir)
        else {
            return false;
        };
        self.layout_mut().active_group_id = next;
        true
    }

    pub fn toggle_zoom(&mut self) {
        let layout = self.layout_mut();
        if layout.zoomed_group_id.is_some() {
            layout.zoomed_group_id = None;
        } else {
            layout.zoomed_group_id = Some(layout.active_group_id.clone());
        }
    }

    pub fn close_active_pane(&mut self) -> bool {
        if self.pane_count() <= 1 {
            return false;
        }
        let closing_id = self.layout().active_group_id.clone();
        let target_id =
            merge_target_for_leaf(&self.layout().split_root, &closing_id).unwrap_or_default();

        // Move tabs from closing group into merge target.
        let closing_tabs = self
            .layout()
            .groups
            .iter()
            .find(|g| g.id == closing_id)
            .map(|g| g.tab_ids.clone())
            .unwrap_or_default();

        {
            let layout = self.layout_mut();
            if let Some(target) = layout.groups.iter_mut().find(|g| g.id == target_id) {
                for tab_id in closing_tabs {
                    if !target.tab_ids.contains(&tab_id) {
                        target.tab_ids.push(tab_id);
                    }
                }
            }
            layout.groups.retain(|g| g.id != closing_id);
            let (root, removed) = remove_leaf(&layout.split_root, &closing_id);
            if !removed {
                return false;
            }
            layout.split_root = root;
            layout.active_group_id = if target_id.is_empty() {
                leaf_order(&layout.split_root)
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| MAIN_GROUP_ID.to_string())
            } else {
                target_id
            };
            if layout.zoomed_group_id.as_deref() == Some(closing_id.as_str()) {
                layout.zoomed_group_id = None;
            }
            // Drop orphan tabs not referenced by any group.
            let live: std::collections::HashSet<_> = layout
                .groups
                .iter()
                .flat_map(|g| g.tab_ids.iter().cloned())
                .collect();
            layout.tabs.retain(|t| live.contains(t.id()));
        }
        true
    }

    pub fn open_notes_tab_in_active(&mut self, path: &str, title: &str) -> bool {
        let layout = self.layout_mut();
        if layout.tabs.len() >= MAX_TABS {
            return false;
        }
        let active_group_id = layout.active_group_id.clone();
        if let Some(existing) = layout.tabs.iter().find(|t| {
            t.notes_path() == Some(path)
                && layout
                    .groups
                    .iter()
                    .find(|g| g.id == active_group_id)
                    .is_some_and(|g| g.tab_ids.iter().any(|id| id == t.id()))
        }) {
            let existing_id = existing.id().to_string();
            if let Some(group) = layout.groups.iter_mut().find(|g| g.id == active_group_id) {
                group.active_tab_id.replace(existing_id);
            }
            return true;
        }

        let tab = new_notes_tab(path, title);
        let tab_id = tab.id().to_string();
        layout.tabs.push(tab);
        if let Some(group) = layout.groups.iter_mut().find(|g| g.id == active_group_id) {
            group.tab_ids.push(tab_id.clone());
            group.active_tab_id = Some(tab_id);
        }
        true
    }

    pub fn open_chat_tab_in_active(&mut self, session_id: &str, title: &str) -> bool {
        let layout = self.layout_mut();
        if layout.tabs.len() >= MAX_TABS {
            return false;
        }
        // Focus existing chat for same session if present in active group.
        let active_group_id = layout.active_group_id.clone();
        if let Some(existing) = layout.tabs.iter().find(|t| {
            t.chat_session_id() == Some(session_id)
                && layout
                    .groups
                    .iter()
                    .find(|g| g.id == active_group_id)
                    .is_some_and(|g| g.tab_ids.iter().any(|id| id == t.id()))
        }) {
            let existing_id = existing.id().to_string();
            if let Some(group) = layout.groups.iter_mut().find(|g| g.id == active_group_id) {
                group.active_tab_id = Some(existing_id);
            }
            return true;
        }

        let tab = new_chat_tab(session_id, title);
        let tab_id = tab.id().to_string();
        layout.tabs.push(tab);
        if let Some(group) = layout.groups.iter_mut().find(|g| g.id == active_group_id) {
            group.tab_ids.push(tab_id.clone());
            group.active_tab_id = Some(tab_id);
        }
        true
    }

    pub fn cycle_tab(&mut self, forward: bool) -> bool {
        let layout = self.layout_mut();
        let active_group_id = layout.active_group_id.clone();
        let Some(group) = layout.groups.iter_mut().find(|g| g.id == active_group_id) else {
            return false;
        };
        if group.tab_ids.len() <= 1 {
            return false;
        }
        let Some(active) = group.active_tab_id.clone() else {
            return false;
        };
        let Some(idx) = group.tab_ids.iter().position(|id| id == &active) else {
            return false;
        };
        let next = if forward {
            (idx + 1) % group.tab_ids.len()
        } else {
            (idx + group.tab_ids.len() - 1) % group.tab_ids.len()
        };
        group.active_tab_id = Some(group.tab_ids[next].clone());
        true
    }

    pub fn switch_desktop(&mut self, index: usize) -> bool {
        if index >= self.desktops.len() || index >= MAX_SHELL_DESKTOPS {
            return false;
        }
        self.active_desktop_id = self.desktops[index].id.clone();
        true
    }

    pub fn create_desktop(&mut self, session_id: &str) -> bool {
        if self.desktops.len() >= MAX_SHELL_DESKTOPS {
            return false;
        }
        let tab = new_chat_tab(session_id, short_session_title(session_id));
        let group_id = new_group_id();
        let tab_id = tab.id().to_string();
        let desktop_id = new_tab_id("desktop");
        let name = format!("Desktop {}", self.desktops.len() + 1);
        self.desktops.push(ShellDesktop {
            id: desktop_id.clone(),
            name,
            layout: ShellDesktopLayout {
                tabs: vec![tab],
                groups: vec![EditorGroup {
                    id: group_id.clone(),
                    tab_ids: vec![tab_id.clone()],
                    active_tab_id: Some(tab_id),
                }],
                split_root: SplitNode::Group { id: group_id.clone() },
                active_group_id: group_id,
                zoomed_group_id: None,
            },
        });
        self.active_desktop_id = desktop_id;
        true
    }

    pub fn rebind_focused_chat_session(&mut self, session_id: &str, title: &str) {
        let layout = self.layout_mut();
        let active_group_id = layout.active_group_id.clone();
        let active_tab_id = layout
            .groups
            .iter()
            .find(|g| g.id == active_group_id)
            .and_then(|g| g.active_tab_id.clone());
        let Some(tab_id) = active_tab_id else {
            return;
        };
        if let Some(ShellTab::Chat {
            session_id: sid,
            title: t,
            ..
        }) = layout.tabs.iter_mut().find(|t| t.id() == tab_id)
        {
            *sid = session_id.to_string();
            *t = title.to_string();
        }
    }

    pub fn sanitize(&mut self) {
        // Ensure every leaf has a group; drop empty desktops except one.
        if self.desktops.is_empty() {
            *self = Self::bootstrap(&uuid::Uuid::new_v4().simple().to_string(), "Chat");
            return;
        }
        if !self.desktops.iter().any(|d| d.id == self.active_desktop_id) {
            self.active_desktop_id = self.desktops[0].id.clone();
        }
        for desktop in &mut self.desktops {
            let leaf_ids = collect_group_ids(&desktop.layout.split_root);
            for leaf in &leaf_ids {
                if !desktop.layout.groups.iter().any(|g| &g.id == leaf) {
                    desktop.layout.groups.push(EditorGroup {
                        id: leaf.clone(),
                        tab_ids: Vec::new(),
                        active_tab_id: None,
                    });
                }
            }
            // Ensure each group has at least one chat tab.
            for group in &mut desktop.layout.groups {
                if group.tab_ids.is_empty() {
                    let session = uuid::Uuid::new_v4().simple().to_string();
                    let tab = new_chat_tab(&session, short_session_title(&session));
                    let tab_id = tab.id().to_string();
                    group.tab_ids.push(tab_id.clone());
                    group.active_tab_id = Some(tab_id);
                    desktop.layout.tabs.push(tab);
                } else if group
                    .active_tab_id
                    .as_ref()
                    .is_none_or(|id| !group.tab_ids.contains(id))
                {
                    group.active_tab_id = group.tab_ids.first().cloned();
                }
            }
            if !leaf_ids.contains(&desktop.layout.active_group_id) {
                desktop.layout.active_group_id = leaf_ids
                    .first()
                    .cloned()
                    .unwrap_or_else(|| MAIN_GROUP_ID.to_string());
            }
        }
    }
}

pub fn short_session_title(session_id: &str) -> String {
    let short = if session_id.len() > 8 {
        &session_id[..8]
    } else {
        session_id
    };
    format!("Chat {short}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_single_chat_pane() {
        let shell = WorkspaceShell::bootstrap("sess-1", "Chat");
        assert_eq!(shell.pane_count(), 1);
        assert_eq!(shell.focused_chat_session_id(), Some("sess-1"));
    }

    #[test]
    fn split_respects_max_panes() {
        let mut shell = WorkspaceShell::bootstrap("s0", "Chat");
        assert!(shell.split_active(SplitDirection::Right, "s1"));
        assert!(shell.split_active(SplitDirection::Down, "s2"));
        assert!(shell.split_active(SplitDirection::Right, "s3"));
        assert!(!shell.split_active(SplitDirection::Right, "s4"));
        assert_eq!(shell.pane_count(), MAX_SHELL_PANES);
    }

    #[test]
    fn close_pane_merges_tabs() {
        let mut shell = WorkspaceShell::bootstrap("s0", "Chat");
        assert!(shell.split_active(SplitDirection::Right, "s1"));
        assert_eq!(shell.pane_count(), 2);
        assert!(shell.close_active_pane());
        assert_eq!(shell.pane_count(), 1);
        assert_eq!(shell.layout().tabs.len(), 2);
    }

    #[test]
    fn focus_neighbor_and_zoom() {
        let mut shell = WorkspaceShell::bootstrap("s0", "Chat");
        assert!(shell.split_active(SplitDirection::Right, "s1"));
        let right = shell.layout().active_group_id.clone();
        assert!(shell.focus_neighbor(FocusDir::Left));
        assert_ne!(shell.layout().active_group_id, right);
        shell.toggle_zoom();
        assert!(shell.layout().zoomed_group_id.is_some());
        shell.toggle_zoom();
        assert!(shell.layout().zoomed_group_id.is_none());
    }

    #[test]
    fn desktops_capped() {
        let mut shell = WorkspaceShell::bootstrap("s0", "Chat");
        assert!(shell.create_desktop("s1"));
        assert!(shell.create_desktop("s2"));
        assert!(shell.create_desktop("s3"));
        assert!(!shell.create_desktop("s4"));
        assert_eq!(shell.desktops.len(), MAX_SHELL_DESKTOPS);
        assert!(shell.switch_desktop(0));
    }
}
