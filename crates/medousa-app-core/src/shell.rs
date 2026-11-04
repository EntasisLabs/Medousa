//! Shell layout application core — desktop/tab/pane identity + focus.
//!
//! Daemon stays workshop authority. Persistence I/O, LME/browser ports, and
//! paint stay in the renderer. This module owns the layout graph and focus
//! pointers so headless clients and Home share one reconciliation path.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;

pub const MAX_SHELL_PANES: usize = 4;
pub const MAX_SHELL_DESKTOPS: usize = 4;
pub const MAIN_GROUP_ID: &str = "main";
pub const DEFAULT_DESKTOP_NAME: &str = "Main";

const RATIO_MIN: f64 = 0.2;
const RATIO_MAX: f64 = 0.8;
const RATIO_DEFAULT: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(transparent)]
pub struct ShellRevision(pub u64);

impl ShellRevision {
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// First-class open work in the shell center host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShellTab {
    Create {
        id: String,
        #[serde(rename = "documentId")]
        document_id: String,
        #[serde(rename = "workshopId")]
        workshop_id: String,
        title: String,
    },
    Chat {
        id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        title: String,
    },
    Lme {
        id: String,
        #[serde(rename = "lmeTabId")]
        lme_tab_id: String,
        title: String,
    },
    Web {
        id: String,
        #[serde(rename = "browserTabId")]
        browser_tab_id: String,
        title: String,
    },
    Surface {
        id: String,
        #[serde(rename = "surfaceId")]
        surface_id: String,
        title: String,
    },
    Terminal {
        id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "workId")]
        work_id: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "executionRuntimeId"
        )]
        execution_runtime_id: Option<String>,
        title: String,
    },
}

impl ShellTab {
    pub fn id(&self) -> &str {
        match self {
            Self::Create { id, .. }
            | Self::Chat { id, .. }
            | Self::Lme { id, .. }
            | Self::Web { id, .. }
            | Self::Surface { id, .. }
            | Self::Terminal { id, .. } => id,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Create { title, .. }
            | Self::Chat { title, .. }
            | Self::Lme { title, .. }
            | Self::Web { title, .. }
            | Self::Surface { title, .. }
            | Self::Terminal { title, .. } => title,
        }
    }

    pub fn set_title(&mut self, title: String) {
        match self {
            Self::Create { title: slot, .. }
            | Self::Chat { title: slot, .. }
            | Self::Lme { title: slot, .. }
            | Self::Web { title: slot, .. }
            | Self::Surface { title: slot, .. }
            | Self::Terminal { title: slot, .. } => *slot = title,
        }
    }

    pub fn chat_session_id(&self) -> Option<&str> {
        match self {
            Self::Chat { session_id, .. } => Some(session_id.as_str()),
            _ => None,
        }
    }

    pub fn lme_tab_id(&self) -> Option<&str> {
        match self {
            Self::Lme { lme_tab_id, .. } => Some(lme_tab_id.as_str()),
            _ => None,
        }
    }

    pub fn browser_tab_id(&self) -> Option<&str> {
        match self {
            Self::Web { browser_tab_id, .. } => Some(browser_tab_id.as_str()),
            _ => None,
        }
    }

    pub fn surface_id(&self) -> Option<&str> {
        match self {
            Self::Surface { surface_id, .. } => Some(surface_id.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EditorGroup {
    pub id: String,
    pub tab_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum SplitAxis {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum SplitEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SplitNode {
    Group {
        id: String,
    },
    Branch {
        id: String,
        direction: SplitAxis,
        ratio: f64,
        a: Box<SplitNode>,
        b: Box<SplitNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ShellDesktopLayout {
    pub tabs: Vec<ShellTab>,
    pub groups: Vec<EditorGroup>,
    pub split_root: SplitNode,
    pub active_group_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoomed_group_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ShellDesktop {
    pub id: String,
    pub name: String,
    pub layout: ShellDesktopLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ShellSnapshot {
    pub workspace_scope_id: String,
    pub revision: ShellRevision,
    pub desktops: Vec<ShellDesktop>,
    pub active_desktop_id: String,
    /// Focused desktop layout mirrored for paint convenience.
    pub layout: ShellDesktopLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellCommand {
    /// Replace all layout state (bootstrap / workshop switch restore).
    Hydrate {
        workspace_scope_id: String,
        desktops: Vec<ShellDesktop>,
        active_desktop_id: String,
    },
    /// Empty shell for a workspace scope.
    Reset {
        workspace_scope_id: String,
    },
    OpenTab {
        tab: ShellTab,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_id: Option<String>,
        #[serde(default = "default_true")]
        activate: bool,
    },
    CloseTab {
        tab_id: String,
    },
    FocusTab {
        tab_id: String,
    },
    FocusGroup {
        group_id: String,
    },
    PatchTabTitle {
        tab_id: String,
        title: String,
    },
    SplitPane {
        group_id: String,
        edge: SplitEdge,
        new_group_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        move_tab_id: Option<String>,
    },
    ClosePane {
        group_id: String,
    },
    SetSplitRatio {
        branch_id: String,
        ratio: f64,
    },
    SwitchDesktop {
        desktop_id: String,
    },
    CreateDesktop {
        desktop_id: String,
        name: String,
    },
    RenameDesktop {
        desktop_id: String,
        name: String,
    },
    RemoveDesktop {
        desktop_id: String,
    },
    SetZoomedGroup {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_id: Option<String>,
    },
    /// Move a tab between panes on the active desktop.
    MoveTab {
        tab_id: String,
        to_group_id: String,
    },
    /// Move a tab from the active desktop onto another desktop's focused pane.
    /// Does not switch the active desktop.
    MoveTabToDesktop {
        tab_id: String,
        desktop_id: String,
    },
    /// Move every tab in a pane onto another desktop, then drop the empty pane
    /// when more than one pane remains. Does not switch the active desktop.
    MovePaneToDesktop {
        group_id: String,
        desktop_id: String,
    },
    Snapshot,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellUpdate {
    Snapshot { snapshot: ShellSnapshot },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ShellCoreError {
    #[error("shell not initialized")]
    NotInitialized,
    #[error("desktop `{0}` not found")]
    DesktopNotFound(String),
    #[error("group `{0}` not found")]
    GroupNotFound(String),
    #[error("tab `{0}` not found")]
    TabNotFound(String),
    #[error("pane limit ({MAX_SHELL_PANES}) reached")]
    PaneLimit,
    #[error("desktop limit ({MAX_SHELL_DESKTOPS}) reached")]
    DesktopLimit,
    #[error("cannot remove the last desktop")]
    LastDesktop,
    #[error("cannot close the last pane")]
    LastPane,
    #[error("invalid shell layout: {0}")]
    InvalidLayout(String),
}

#[derive(Debug, Clone)]
pub struct ShellCore {
    workspace_scope_id: String,
    revision: ShellRevision,
    desktops: Vec<ShellDesktop>,
    active_desktop_id: String,
}

impl Default for ShellCore {
    fn default() -> Self {
        Self::new("")
    }
}

impl ShellCore {
    pub fn new(workspace_scope_id: impl Into<String>) -> Self {
        let scope = workspace_scope_id.into();
        let desktop_id = "desktop-main".to_string();
        Self {
            workspace_scope_id: scope,
            revision: ShellRevision(0),
            desktops: vec![ShellDesktop {
                id: desktop_id.clone(),
                name: DEFAULT_DESKTOP_NAME.into(),
                layout: empty_layout(),
            }],
            active_desktop_id: desktop_id,
        }
    }

    pub fn workspace_scope_id(&self) -> &str {
        &self.workspace_scope_id
    }

    pub fn active_desktop_id(&self) -> &str {
        &self.active_desktop_id
    }

    pub fn revision(&self) -> ShellRevision {
        self.revision
    }

    pub fn snapshot(&self) -> Result<ShellSnapshot, ShellCoreError> {
        let layout = self.active_layout()?.clone();
        Ok(ShellSnapshot {
            workspace_scope_id: self.workspace_scope_id.clone(),
            revision: self.revision,
            desktops: self.desktops.clone(),
            active_desktop_id: self.active_desktop_id.clone(),
            layout,
        })
    }

    pub fn dispatch(&mut self, command: ShellCommand) -> Result<ShellUpdate, ShellCoreError> {
        match command {
            ShellCommand::Hydrate {
                workspace_scope_id,
                desktops,
                active_desktop_id,
            } => {
                if desktops.is_empty() {
                    return Err(ShellCoreError::InvalidLayout(
                        "hydrate requires at least one desktop".into(),
                    ));
                }
                if desktops.len() > MAX_SHELL_DESKTOPS {
                    return Err(ShellCoreError::DesktopLimit);
                }
                for desktop in &desktops {
                    validate_layout(&desktop.layout)?;
                }
                if !desktops.iter().any(|d| d.id == active_desktop_id) {
                    return Err(ShellCoreError::DesktopNotFound(active_desktop_id));
                }
                self.workspace_scope_id = workspace_scope_id;
                self.desktops = desktops;
                self.active_desktop_id = active_desktop_id;
                self.bump();
                self.ok_snapshot()
            }
            ShellCommand::Reset { workspace_scope_id } => {
                *self = Self::new(workspace_scope_id);
                self.ok_snapshot()
            }
            ShellCommand::OpenTab {
                tab,
                group_id,
                activate,
            } => {
                self.open_tab(tab, group_id, activate)?;
                self.ok_snapshot()
            }
            ShellCommand::CloseTab { tab_id } => {
                self.close_tab(&tab_id)?;
                self.ok_snapshot()
            }
            ShellCommand::FocusTab { tab_id } => {
                self.focus_tab(&tab_id)?;
                self.ok_snapshot()
            }
            ShellCommand::FocusGroup { group_id } => {
                self.focus_group(&group_id)?;
                self.ok_snapshot()
            }
            ShellCommand::PatchTabTitle { tab_id, title } => {
                self.patch_title(&tab_id, title)?;
                self.ok_snapshot()
            }
            ShellCommand::SplitPane {
                group_id,
                edge,
                new_group_id,
                move_tab_id,
            } => {
                self.split_pane(&group_id, edge, &new_group_id, move_tab_id.as_deref())?;
                self.ok_snapshot()
            }
            ShellCommand::ClosePane { group_id } => {
                self.close_pane(&group_id)?;
                self.ok_snapshot()
            }
            ShellCommand::SetSplitRatio { branch_id, ratio } => {
                let layout = self.active_layout_mut()?;
                layout.split_root = set_branch_ratio(layout.split_root.clone(), &branch_id, ratio);
                self.bump();
                self.ok_snapshot()
            }
            ShellCommand::SwitchDesktop { desktop_id } => {
                if !self.desktops.iter().any(|d| d.id == desktop_id) {
                    return Err(ShellCoreError::DesktopNotFound(desktop_id));
                }
                self.active_desktop_id = desktop_id;
                self.bump();
                self.ok_snapshot()
            }
            ShellCommand::CreateDesktop { desktop_id, name } => {
                if self.desktops.len() >= MAX_SHELL_DESKTOPS {
                    return Err(ShellCoreError::DesktopLimit);
                }
                if self.desktops.iter().any(|d| d.id == desktop_id) {
                    return Err(ShellCoreError::InvalidLayout(format!(
                        "desktop `{desktop_id}` already exists"
                    )));
                }
                self.desktops.push(ShellDesktop {
                    id: desktop_id.clone(),
                    name,
                    layout: empty_layout(),
                });
                self.active_desktop_id = desktop_id;
                self.bump();
                self.ok_snapshot()
            }
            ShellCommand::RenameDesktop { desktop_id, name } => {
                let desktop = self
                    .desktops
                    .iter_mut()
                    .find(|d| d.id == desktop_id)
                    .ok_or(ShellCoreError::DesktopNotFound(desktop_id))?;
                desktop.name = name;
                self.bump();
                self.ok_snapshot()
            }
            ShellCommand::RemoveDesktop { desktop_id } => {
                if self.desktops.len() <= 1 {
                    return Err(ShellCoreError::LastDesktop);
                }
                let idx = self
                    .desktops
                    .iter()
                    .position(|d| d.id == desktop_id)
                    .ok_or_else(|| ShellCoreError::DesktopNotFound(desktop_id.clone()))?;
                self.desktops.remove(idx);
                if self.active_desktop_id == desktop_id {
                    self.active_desktop_id = self.desktops[0].id.clone();
                }
                self.bump();
                self.ok_snapshot()
            }
            ShellCommand::SetZoomedGroup { group_id } => {
                let layout = self.active_layout_mut()?;
                if let Some(ref id) = group_id
                    && !layout.groups.iter().any(|g| g.id == *id)
                {
                    return Err(ShellCoreError::GroupNotFound(id.clone()));
                }
                layout.zoomed_group_id = group_id;
                self.bump();
                self.ok_snapshot()
            }
            ShellCommand::MoveTab {
                tab_id,
                to_group_id,
            } => {
                self.move_tab(&tab_id, &to_group_id)?;
                self.ok_snapshot()
            }
            ShellCommand::MoveTabToDesktop {
                tab_id,
                desktop_id,
            } => {
                self.move_tab_to_desktop(&tab_id, &desktop_id)?;
                self.ok_snapshot()
            }
            ShellCommand::MovePaneToDesktop {
                group_id,
                desktop_id,
            } => {
                self.move_pane_to_desktop(&group_id, &desktop_id)?;
                self.ok_snapshot()
            }
            ShellCommand::Snapshot => self.ok_snapshot(),
        }
    }

    fn ok_snapshot(&self) -> Result<ShellUpdate, ShellCoreError> {
        Ok(ShellUpdate::Snapshot {
            snapshot: self.snapshot()?,
        })
    }

    fn bump(&mut self) {
        self.revision = self.revision.next();
    }

    fn active_layout(&self) -> Result<&ShellDesktopLayout, ShellCoreError> {
        self.desktops
            .iter()
            .find(|d| d.id == self.active_desktop_id)
            .map(|d| &d.layout)
            .ok_or_else(|| ShellCoreError::DesktopNotFound(self.active_desktop_id.clone()))
    }

    fn active_layout_mut(&mut self) -> Result<&mut ShellDesktopLayout, ShellCoreError> {
        let id = self.active_desktop_id.clone();
        self.desktops
            .iter_mut()
            .find(|d| d.id == id)
            .map(|d| &mut d.layout)
            .ok_or(ShellCoreError::DesktopNotFound(id))
    }

    fn open_tab(
        &mut self,
        tab: ShellTab,
        group_id: Option<String>,
        activate: bool,
    ) -> Result<(), ShellCoreError> {
        let target_group = group_id
            .clone()
            .unwrap_or_else(|| self.active_layout().map(|l| l.active_group_id.clone()).unwrap_or_default());

        // Reuse existing identity when the same work is already open.
        let reusable = self
            .find_reusable_tab(&tab, group_id.as_deref())
            .map(str::to_string);
        if let Some(existing_id) = reusable {
            let title = tab.title().to_string();
            if !title.is_empty() {
                self.patch_title(&existing_id, title)?;
            }
            if activate {
                self.focus_tab(&existing_id)?;
            } else {
                self.bump();
            }
            return Ok(());
        }

        let layout = self.active_layout_mut()?;
        if !layout.groups.iter().any(|g| g.id == target_group) {
            return Err(ShellCoreError::GroupNotFound(target_group));
        }
        if layout.tabs.iter().any(|t| t.id() == tab.id()) {
            return Err(ShellCoreError::InvalidLayout(format!(
                "tab `{}` already exists",
                tab.id()
            )));
        }

        // Surface tabs are singletons — drop any prior surface of the same id.
        if let Some(surface_id) = tab.surface_id() {
            let drop_ids: Vec<String> = layout
                .tabs
                .iter()
                .filter(|t| t.surface_id() == Some(surface_id))
                .map(|t| t.id().to_string())
                .collect();
            for id in drop_ids {
                remove_tab_from_layout(layout, &id);
            }
        }

        let tab_id = tab.id().to_string();
        layout.tabs.push(tab);
        for group in &mut layout.groups {
            if group.id == target_group {
                group.tab_ids.push(tab_id.clone());
                if activate || group.active_tab_id.is_none() {
                    group.active_tab_id = Some(tab_id.clone());
                }
            }
        }
        if activate {
            layout.active_group_id = target_group;
        }
        self.bump();
        Ok(())
    }

    fn find_reusable_tab(&self, tab: &ShellTab, group_id: Option<&str>) -> Option<&str> {
        let Ok(layout) = self.active_layout() else {
            return None;
        };
        let in_group = |tab_id: &str, gid: &str| {
            layout
                .groups
                .iter()
                .find(|g| g.id == gid)
                .is_some_and(|g| g.tab_ids.iter().any(|id| id == tab_id))
        };
        for existing in &layout.tabs {
            let same = match (tab, existing) {
                (
                    ShellTab::Chat {
                        session_id: a, ..
                    },
                    ShellTab::Chat {
                        session_id: b, ..
                    },
                ) => a == b,
                (ShellTab::Lme { lme_tab_id: a, .. }, ShellTab::Lme { lme_tab_id: b, .. }) => a == b,
                (
                    ShellTab::Web {
                        browser_tab_id: a, ..
                    },
                    ShellTab::Web {
                        browser_tab_id: b, ..
                    },
                ) => a == b,
                (
                    ShellTab::Surface { surface_id: a, .. },
                    ShellTab::Surface { surface_id: b, .. },
                ) => a == b,
                (
                    ShellTab::Terminal {
                        session_id: a, ..
                    },
                    ShellTab::Terminal {
                        session_id: b, ..
                    },
                ) => a == b,
                (
                    ShellTab::Create {
                        document_id: a, ..
                    },
                    ShellTab::Create {
                        document_id: b, ..
                    },
                ) => a == b,
                _ => false,
            };
            if !same {
                continue;
            }
            if let Some(gid) = group_id {
                if in_group(existing.id(), gid) {
                    return Some(existing.id());
                }
            } else {
                return Some(existing.id());
            }
        }
        None
    }

    fn close_tab(&mut self, tab_id: &str) -> Result<(), ShellCoreError> {
        let layout = self.active_layout_mut()?;
        if !layout.tabs.iter().any(|t| t.id() == tab_id) {
            return Err(ShellCoreError::TabNotFound(tab_id.to_string()));
        }
        remove_tab_from_layout(layout, tab_id);
        self.bump();
        Ok(())
    }

    fn focus_tab(&mut self, tab_id: &str) -> Result<(), ShellCoreError> {
        let layout = self.active_layout_mut()?;
        if !layout.tabs.iter().any(|t| t.id() == tab_id) {
            return Err(ShellCoreError::TabNotFound(tab_id.to_string()));
        }
        let group_id = layout
            .groups
            .iter()
            .find(|g| g.tab_ids.iter().any(|id| id == tab_id))
            .map(|g| g.id.clone())
            .ok_or_else(|| ShellCoreError::TabNotFound(tab_id.to_string()))?;
        for group in &mut layout.groups {
            if group.id == group_id {
                group.active_tab_id = Some(tab_id.to_string());
            }
        }
        layout.active_group_id = group_id;
        self.bump();
        Ok(())
    }

    fn focus_group(&mut self, group_id: &str) -> Result<(), ShellCoreError> {
        let layout = self.active_layout_mut()?;
        if !layout.groups.iter().any(|g| g.id == group_id) {
            return Err(ShellCoreError::GroupNotFound(group_id.to_string()));
        }
        layout.active_group_id = group_id.to_string();
        self.bump();
        Ok(())
    }

    fn patch_title(&mut self, tab_id: &str, title: String) -> Result<(), ShellCoreError> {
        let layout = self.active_layout_mut()?;
        let tab = layout
            .tabs
            .iter_mut()
            .find(|t| t.id() == tab_id)
            .ok_or_else(|| ShellCoreError::TabNotFound(tab_id.to_string()))?;
        tab.set_title(title);
        self.bump();
        Ok(())
    }

    fn split_pane(
        &mut self,
        group_id: &str,
        edge: SplitEdge,
        new_group_id: &str,
        move_tab_id: Option<&str>,
    ) -> Result<(), ShellCoreError> {
        let layout = self.active_layout_mut()?;
        if count_leaves(&layout.split_root) >= MAX_SHELL_PANES {
            return Err(ShellCoreError::PaneLimit);
        }
        if !layout.groups.iter().any(|g| g.id == group_id) {
            return Err(ShellCoreError::GroupNotFound(group_id.to_string()));
        }
        if layout.groups.iter().any(|g| g.id == new_group_id) {
            return Err(ShellCoreError::InvalidLayout(format!(
                "group `{new_group_id}` already exists"
            )));
        }
        let Some(next_root) = split_leaf_at_edge(&layout.split_root, group_id, edge, new_group_id)
        else {
            return Err(ShellCoreError::GroupNotFound(group_id.to_string()));
        };
        layout.split_root = next_root;
        layout.groups.push(EditorGroup {
            id: new_group_id.to_string(),
            tab_ids: Vec::new(),
            active_tab_id: None,
        });
        if let Some(tab_id) = move_tab_id {
            move_tab_between_groups(layout, tab_id, group_id, new_group_id)?;
        }
        layout.active_group_id = new_group_id.to_string();
        self.bump();
        Ok(())
    }

    fn close_pane(&mut self, group_id: &str) -> Result<(), ShellCoreError> {
        let layout = self.active_layout_mut()?;
        if count_leaves(&layout.split_root) <= 1 {
            return Err(ShellCoreError::LastPane);
        }
        let merge_target = merge_target_for_leaf(&layout.split_root, group_id)
            .ok_or_else(|| ShellCoreError::GroupNotFound(group_id.to_string()))?;
        let (next_root, removed) = remove_leaf(layout.split_root.clone(), group_id);
        if !removed {
            return Err(ShellCoreError::GroupNotFound(group_id.to_string()));
        }
        layout.split_root = next_root;
        if let Some(group) = layout.groups.iter().find(|g| g.id == group_id).cloned() {
            for tab_id in &group.tab_ids {
                if let Some(target) = layout.groups.iter_mut().find(|g| g.id == merge_target) {
                    if !target.tab_ids.iter().any(|id| id == tab_id) {
                        target.tab_ids.push(tab_id.clone());
                    }
                    if target.active_tab_id.is_none() {
                        target.active_tab_id = Some(tab_id.clone());
                    }
                }
            }
        }
        layout.groups.retain(|g| g.id != group_id);
        if layout.active_group_id == group_id {
            layout.active_group_id = merge_target;
        }
        if layout.zoomed_group_id.as_deref() == Some(group_id) {
            layout.zoomed_group_id = None;
        }
        self.bump();
        Ok(())
    }

    fn move_tab(&mut self, tab_id: &str, to_group_id: &str) -> Result<(), ShellCoreError> {
        let layout = self.active_layout_mut()?;
        if !layout.groups.iter().any(|g| g.id == to_group_id) {
            return Err(ShellCoreError::GroupNotFound(to_group_id.to_string()));
        }
        let from = layout
            .groups
            .iter()
            .find(|g| g.tab_ids.iter().any(|id| id == tab_id))
            .map(|g| g.id.clone())
            .ok_or_else(|| ShellCoreError::TabNotFound(tab_id.to_string()))?;
        if from == to_group_id {
            return Ok(());
        }
        move_tab_between_groups(layout, tab_id, &from, to_group_id)?;
        layout.active_group_id = to_group_id.to_string();
        self.bump();
        Ok(())
    }

    fn move_tab_to_desktop(
        &mut self,
        tab_id: &str,
        desktop_id: &str,
    ) -> Result<(), ShellCoreError> {
        if desktop_id == self.active_desktop_id {
            return Err(ShellCoreError::InvalidLayout(
                "target desktop must differ from the active desktop".into(),
            ));
        }
        if !self.desktops.iter().any(|d| d.id == desktop_id) {
            return Err(ShellCoreError::DesktopNotFound(desktop_id.to_string()));
        }

        let tab = {
            let layout = self.active_layout_mut()?;
            let tab = layout
                .tabs
                .iter()
                .find(|t| t.id() == tab_id)
                .cloned()
                .ok_or_else(|| ShellCoreError::TabNotFound(tab_id.to_string()))?;
            remove_tab_from_layout(layout, tab_id);
            tab
        };

        let target = self
            .desktops
            .iter_mut()
            .find(|d| d.id == desktop_id)
            .ok_or_else(|| ShellCoreError::DesktopNotFound(desktop_id.to_string()))?;
        append_tabs_to_layout(&mut target.layout, vec![tab]);
        self.bump();
        Ok(())
    }

    fn move_pane_to_desktop(
        &mut self,
        group_id: &str,
        desktop_id: &str,
    ) -> Result<(), ShellCoreError> {
        if desktop_id == self.active_desktop_id {
            return Err(ShellCoreError::InvalidLayout(
                "target desktop must differ from the active desktop".into(),
            ));
        }
        if !self.desktops.iter().any(|d| d.id == desktop_id) {
            return Err(ShellCoreError::DesktopNotFound(desktop_id.to_string()));
        }

        let moved = {
            let layout = self.active_layout_mut()?;
            let group = layout
                .groups
                .iter()
                .find(|g| g.id == group_id)
                .cloned()
                .ok_or_else(|| ShellCoreError::GroupNotFound(group_id.to_string()))?;
            let tabs: Vec<ShellTab> = group
                .tab_ids
                .iter()
                .filter_map(|id| layout.tabs.iter().find(|t| t.id() == id).cloned())
                .collect();
            for tab in &tabs {
                remove_tab_from_layout(layout, tab.id());
            }

            if count_leaves(&layout.split_root) > 1 {
                let merge_target = merge_target_for_leaf(&layout.split_root, group_id)
                    .ok_or_else(|| ShellCoreError::GroupNotFound(group_id.to_string()))?;
                let (next_root, removed) = remove_leaf(layout.split_root.clone(), group_id);
                if !removed {
                    return Err(ShellCoreError::GroupNotFound(group_id.to_string()));
                }
                layout.split_root = next_root;
                layout.groups.retain(|g| g.id != group_id);
                if layout.active_group_id == group_id {
                    layout.active_group_id = merge_target;
                }
                if layout.zoomed_group_id.as_deref() == Some(group_id) {
                    layout.zoomed_group_id = None;
                }
            }
            tabs
        };

        let target = self
            .desktops
            .iter_mut()
            .find(|d| d.id == desktop_id)
            .ok_or_else(|| ShellCoreError::DesktopNotFound(desktop_id.to_string()))?;
        append_tabs_to_layout(&mut target.layout, moved);
        self.bump();
        Ok(())
    }
}

fn empty_layout() -> ShellDesktopLayout {
    ShellDesktopLayout {
        tabs: Vec::new(),
        groups: vec![EditorGroup {
            id: MAIN_GROUP_ID.into(),
            tab_ids: Vec::new(),
            active_tab_id: None,
        }],
        split_root: SplitNode::Group {
            id: MAIN_GROUP_ID.into(),
        },
        active_group_id: MAIN_GROUP_ID.into(),
        zoomed_group_id: None,
    }
}

fn validate_layout(layout: &ShellDesktopLayout) -> Result<(), ShellCoreError> {
    let leaves = collect_group_ids(&layout.split_root);
    if leaves.len() > MAX_SHELL_PANES {
        return Err(ShellCoreError::PaneLimit);
    }
    if leaves.is_empty() {
        return Err(ShellCoreError::InvalidLayout("split tree has no leaves".into()));
    }
    for leaf in &leaves {
        if !layout.groups.iter().any(|g| g.id == *leaf) {
            return Err(ShellCoreError::InvalidLayout(format!(
                "missing group `{leaf}` for split leaf"
            )));
        }
    }
    if !layout.groups.iter().any(|g| g.id == layout.active_group_id) {
        return Err(ShellCoreError::InvalidLayout(format!(
            "active group `{}` missing",
            layout.active_group_id
        )));
    }
    let tab_ids: std::collections::BTreeSet<_> =
        layout.tabs.iter().map(|t| t.id().to_string()).collect();
    for group in &layout.groups {
        for tab_id in &group.tab_ids {
            if !tab_ids.contains(tab_id) {
                return Err(ShellCoreError::InvalidLayout(format!(
                    "group `{}` references missing tab `{tab_id}`",
                    group.id
                )));
            }
        }
    }
    Ok(())
}

fn remove_tab_from_layout(layout: &mut ShellDesktopLayout, tab_id: &str) {
    layout.tabs.retain(|t| t.id() != tab_id);
    for group in &mut layout.groups {
        group.tab_ids.retain(|id| id != tab_id);
        if group.active_tab_id.as_deref() == Some(tab_id) {
            group.active_tab_id = group.tab_ids.last().cloned();
        }
    }
}

fn append_tabs_to_layout(layout: &mut ShellDesktopLayout, incoming: Vec<ShellTab>) {
    if incoming.is_empty() {
        return;
    }
    let dest_group_id = if layout
        .groups
        .iter()
        .any(|g| g.id == layout.active_group_id)
    {
        layout.active_group_id.clone()
    } else {
        layout
            .groups
            .first()
            .map(|g| g.id.clone())
            .unwrap_or_else(|| MAIN_GROUP_ID.into())
    };
    let existing: std::collections::HashSet<String> =
        layout.tabs.iter().map(|t| t.id().to_string()).collect();
    let fresh: Vec<ShellTab> = incoming
        .into_iter()
        .filter(|t| !existing.contains(t.id()))
        .collect();
    if fresh.is_empty() {
        return;
    }
    let fresh_ids: Vec<String> = fresh.iter().map(|t| t.id().to_string()).collect();
    layout.tabs.extend(fresh);
    for group in &mut layout.groups {
        if group.id == dest_group_id {
            group.tab_ids.extend(fresh_ids.iter().cloned());
            group.active_tab_id = fresh_ids.last().cloned().or(group.active_tab_id.clone());
        }
    }
    layout.active_group_id = dest_group_id;
}

fn move_tab_between_groups(
    layout: &mut ShellDesktopLayout,
    tab_id: &str,
    from: &str,
    to: &str,
) -> Result<(), ShellCoreError> {
    if !layout.tabs.iter().any(|t| t.id() == tab_id) {
        return Err(ShellCoreError::TabNotFound(tab_id.to_string()));
    }
    for group in &mut layout.groups {
        if group.id == from {
            group.tab_ids.retain(|id| id != tab_id);
            if group.active_tab_id.as_deref() == Some(tab_id) {
                group.active_tab_id = group.tab_ids.last().cloned();
            }
        }
    }
    for group in &mut layout.groups {
        if group.id == to {
            if !group.tab_ids.iter().any(|id| id == tab_id) {
                group.tab_ids.push(tab_id.to_string());
            }
            group.active_tab_id = Some(tab_id.to_string());
        }
    }
    Ok(())
}

fn clamp_ratio(ratio: f64) -> f64 {
    if !ratio.is_finite() {
        return RATIO_DEFAULT;
    }
    ratio.clamp(RATIO_MIN, RATIO_MAX)
}

fn count_leaves(node: &SplitNode) -> usize {
    match node {
        SplitNode::Group { .. } => 1,
        SplitNode::Branch { a, b, .. } => count_leaves(a) + count_leaves(b),
    }
}

fn collect_group_ids(node: &SplitNode) -> Vec<String> {
    match node {
        SplitNode::Group { id } => vec![id.clone()],
        SplitNode::Branch { a, b, .. } => {
            let mut out = collect_group_ids(a);
            out.extend(collect_group_ids(b));
            out
        }
    }
}

fn split_leaf_at_edge(
    root: &SplitNode,
    group_id: &str,
    edge: SplitEdge,
    new_group_id: &str,
) -> Option<SplitNode> {
    let branch_direction = match edge {
        SplitEdge::Left | SplitEdge::Right => SplitAxis::Column,
        SplitEdge::Top | SplitEdge::Bottom => SplitAxis::Row,
    };
    let new_first = matches!(edge, SplitEdge::Left | SplitEdge::Top);
    let new_leaf = SplitNode::Group {
        id: new_group_id.to_string(),
    };

    fn walk(
        node: &SplitNode,
        group_id: &str,
        branch_direction: SplitAxis,
        new_first: bool,
        new_leaf: &SplitNode,
    ) -> Option<SplitNode> {
        match node {
            SplitNode::Group { id } => {
                if id != group_id {
                    return None;
                }
                Some(SplitNode::Branch {
                    id: format!("branch-{group_id}-{new_first}"),
                    direction: branch_direction,
                    ratio: RATIO_DEFAULT,
                    a: Box::new(if new_first {
                        new_leaf.clone()
                    } else {
                        node.clone()
                    }),
                    b: Box::new(if new_first {
                        node.clone()
                    } else {
                        new_leaf.clone()
                    }),
                })
            }
            SplitNode::Branch {
                id,
                direction,
                ratio,
                a,
                b,
            } => {
                if let Some(next_a) = walk(a, group_id, branch_direction, new_first, new_leaf) {
                    return Some(SplitNode::Branch {
                        id: id.clone(),
                        direction: *direction,
                        ratio: *ratio,
                        a: Box::new(next_a),
                        b: b.clone(),
                    });
                }
                if let Some(next_b) = walk(b, group_id, branch_direction, new_first, new_leaf) {
                    return Some(SplitNode::Branch {
                        id: id.clone(),
                        direction: *direction,
                        ratio: *ratio,
                        a: a.clone(),
                        b: Box::new(next_b),
                    });
                }
                None
            }
        }
    }

    walk(root, group_id, branch_direction, new_first, &new_leaf)
}

fn merge_target_for_leaf(root: &SplitNode, group_id: &str) -> Option<String> {
    fn walk(node: &SplitNode, group_id: &str) -> Option<String> {
        match node {
            SplitNode::Group { .. } => None,
            SplitNode::Branch { a, b, .. } => {
                if let SplitNode::Group { id } = a.as_ref()
                    && id == group_id
                {
                    return collect_group_ids(b).into_iter().next();
                }
                if let SplitNode::Group { id } = b.as_ref()
                    && id == group_id
                {
                    return collect_group_ids(a).into_iter().next_back();
                }
                walk(a, group_id).or_else(|| walk(b, group_id))
            }
        }
    }
    walk(root, group_id)
}

fn remove_leaf(root: SplitNode, group_id: &str) -> (SplitNode, bool) {
    match root {
        SplitNode::Group { .. } => (root, false),
        SplitNode::Branch { id, direction, ratio, a, b } => {
            if let SplitNode::Group { id: ref leaf } = *a
                && leaf == group_id
            {
                return (*b, true);
            }
            if let SplitNode::Group { id: ref leaf } = *b
                && leaf == group_id
            {
                return (*a, true);
            }
            let (next_a, removed_a) = remove_leaf(*a, group_id);
            if removed_a {
                return (
                    SplitNode::Branch {
                        id,
                        direction,
                        ratio,
                        a: Box::new(next_a),
                        b,
                    },
                    true,
                );
            }
            let (next_b, removed_b) = remove_leaf(*b, group_id);
            if removed_b {
                return (
                    SplitNode::Branch {
                        id,
                        direction,
                        ratio,
                        a: Box::new(next_a),
                        b: Box::new(next_b),
                    },
                    true,
                );
            }
            (
                SplitNode::Branch {
                    id,
                    direction,
                    ratio,
                    a: Box::new(next_a),
                    b: Box::new(next_b),
                },
                false,
            )
        }
    }
}

fn set_branch_ratio(root: SplitNode, branch_id: &str, ratio: f64) -> SplitNode {
    let next_ratio = clamp_ratio(ratio);
    match root {
        SplitNode::Group { .. } => root,
        SplitNode::Branch {
            id,
            direction,
            ratio: old,
            a,
            b,
        } => {
            if id == branch_id {
                SplitNode::Branch {
                    id,
                    direction,
                    ratio: next_ratio,
                    a,
                    b,
                }
            } else {
                SplitNode::Branch {
                    id,
                    direction,
                    ratio: old,
                    a: Box::new(set_branch_ratio(*a, branch_id, ratio)),
                    b: Box::new(set_branch_ratio(*b, branch_id, ratio)),
                }
            }
        }
    }
}
