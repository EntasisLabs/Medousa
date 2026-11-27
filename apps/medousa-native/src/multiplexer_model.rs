//! Pure, persisted Workspace arrangement. Resources are references, never copied content.
//!
//! Layout IDs identify UI instances; Resource equality identifies backing data. A resource
//! may appear on several desktops but is deduplicated within each desktop. Closing a tab
//! never closes/deletes its underlying resource. Templates contain geometry only.
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub type Id = u64;
pub type ModelResult<T> = Result<T, String>;
pub const MAX_DESKTOPS: usize = 32;
pub const MAX_GROUPS: usize = 32;
pub const MAX_TABS: usize = 256;
pub const MAX_TEMPLATES: usize = 32;
const MAX_DEPTH: usize = 16;
const MAX_TEXT: usize = 4096;
const MAX_NAME: usize = 256;
const MIN_RATIO: f64 = 0.05;
const MAX_RATIO: f64 = 0.95;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    Chat {
        session_id: String,
        workshop_id: String,
    },
    Terminal {
        session_id: String,
        workshop_id: String,
        work_id: Option<String>,
    },
    Note {
        path: String,
        workshop_id: String,
    },
    Code {
        path: String,
        workshop_id: String,
        work_id: String,
    },
    Create {
        document_id: String,
        workshop_id: String,
    },
    Review {
        work_id: String,
        workshop_id: String,
    },
}
impl Resource {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Chat { .. } => "chat",
            Self::Terminal { .. } => "terminal",
            Self::Note { .. } => "note",
            Self::Code { .. } => "code",
            Self::Create { .. } => "create",
            Self::Review { .. } => "review",
        }
    }
    pub fn to_json(&self) -> Value {
        match self {
            Self::Chat {
                session_id,
                workshop_id,
            } => json!({"kind":"chat","session_id":session_id,"workshop_id":workshop_id}),
            Self::Terminal {
                session_id,
                workshop_id,
                work_id,
            } => {
                json!({"kind":"terminal","session_id":session_id,"workshop_id":workshop_id,"work_id":work_id})
            }
            Self::Note { path, workshop_id } => {
                json!({"kind":"note","path":path,"workshop_id":workshop_id})
            }
            Self::Code {
                path,
                workshop_id,
                work_id,
            } => {
                json!({"kind":"code","path":path,"workshop_id":workshop_id,"work_id":work_id})
            }
            Self::Create {
                document_id,
                workshop_id,
            } => json!({"kind":"create","document_id":document_id,"workshop_id":workshop_id}),
            Self::Review {
                work_id,
                workshop_id,
            } => json!({"kind":"review","work_id":work_id,"workshop_id":workshop_id}),
        }
    }
    pub fn from_json(v: &Value) -> ModelResult<Self> {
        let result = match text(v, "kind", 32)?.as_str() {
            "chat" => Self::Chat {
                session_id: text(v, "session_id", MAX_TEXT)?,
                workshop_id: text(v, "workshop_id", MAX_TEXT)?,
            },
            "terminal" => Self::Terminal {
                session_id: text(v, "session_id", MAX_TEXT)?,
                workshop_id: text(v, "workshop_id", MAX_TEXT)?,
                work_id: optional_text(v, "work_id")?,
            },
            "note" => Self::Note {
                path: text(v, "path", MAX_TEXT)?,
                workshop_id: text(v, "workshop_id", MAX_TEXT)?,
            },
            "code" => Self::Code {
                path: text(v, "path", MAX_TEXT)?,
                work_id: text(v, "work_id", MAX_TEXT)?,
                workshop_id: text(v, "workshop_id", MAX_TEXT)?,
            },
            "create" => Self::Create {
                document_id: text(v, "document_id", MAX_TEXT)?,
                workshop_id: text(v, "workshop_id", MAX_TEXT)?,
            },
            "review" => Self::Review {
                work_id: text(v, "work_id", MAX_TEXT)?,
                workshop_id: text(v, "workshop_id", MAX_TEXT)?,
            },
            _ => return Err("Unknown resource kind".into()),
        };
        Ok(result)
    }
    fn validate(&self) -> ModelResult<()> {
        let valid = |s: &str| checked_text(s, MAX_TEXT).map(|_| ());
        match self {
            Self::Chat {
                session_id,
                workshop_id,
            } => {
                valid(session_id)?;
                valid(workshop_id)
            }
            Self::Terminal {
                session_id,
                workshop_id,
                work_id,
            } => {
                valid(session_id)?;
                valid(workshop_id)?;
                if let Some(w) = work_id {
                    valid(w)?;
                }
                Ok(())
            }
            Self::Code {
                path,
                workshop_id,
                work_id,
            } => {
                valid(path)?;
                valid(workshop_id)?;
                valid(work_id)
            }
            Self::Note { path, workshop_id } => {
                valid(path)?;
                valid(workshop_id)
            }
            Self::Create {
                document_id,
                workshop_id,
            } => {
                valid(document_id)?;
                valid(workshop_id)
            }
            Self::Review {
                work_id,
                workshop_id,
            } => {
                valid(work_id)?;
                valid(workshop_id)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub id: Id,
    pub title: String,
    pub resource: Resource,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: Id,
    pub tabs: Vec<Tab>,
    pub active_tab: Option<Id>,
}
impl Group {
    fn empty(id: Id) -> Self {
        Self {
            id,
            tabs: Vec::new(),
            active_tab: None,
        }
    }
    pub fn active(&self) -> Option<&Tab> {
        self.tabs.iter().find(|t| Some(t.id) == self.active_tab)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SplitNode {
    Group(Id),
    Split {
        axis: Axis,
        ratio: f64,
        first: Box<SplitNode>,
        second: Box<SplitNode>,
    },
}
impl SplitNode {
    pub fn group_ids(&self) -> Vec<Id> {
        match self {
            Self::Group(id) => vec![*id],
            Self::Split { first, second, .. } => {
                let mut ids = first.group_ids();
                ids.extend(second.group_ids());
                ids
            }
        }
    }
    fn depth_of(&self, group: Id, depth: usize) -> Option<usize> {
        match self {
            Self::Group(id) => (*id == group).then_some(depth),
            Self::Split { first, second, .. } => first
                .depth_of(group, depth + 1)
                .or_else(|| second.depth_of(group, depth + 1)),
        }
    }
    fn split(&mut self, group: Id, new_group: Id, edge: Edge) -> bool {
        match self {
            Self::Group(id) if *id == group => {
                let old = Self::Group(group);
                let new = Self::Group(new_group);
                let (first, second) = if matches!(edge, Edge::Left | Edge::Top) {
                    (new, old)
                } else {
                    (old, new)
                };
                *self = Self::Split {
                    axis: if matches!(edge, Edge::Left | Edge::Right) {
                        Axis::Horizontal
                    } else {
                        Axis::Vertical
                    },
                    ratio: 0.5,
                    first: Box::new(first),
                    second: Box::new(second),
                };
                true
            }
            Self::Group(_) => false,
            Self::Split { first, second, .. } => {
                first.split(group, new_group, edge) || second.split(group, new_group, edge)
            }
        }
    }
    fn without(self, group: Id) -> Option<Self> {
        match self {
            Self::Group(id) => (id != group).then_some(Self::Group(id)),
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.without(group), second.without(group)) {
                (Some(first), Some(second)) => Some(Self::Split {
                    axis,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (a, b) => a.or(b),
            },
        }
    }
    /// Address a split by child path (false = first, true = second).
    pub fn set_ratio(&mut self, path: &[bool], ratio: f64) -> ModelResult<()> {
        if !ratio.is_finite() || !(MIN_RATIO..=MAX_RATIO).contains(&ratio) {
            return Err("Split ratio out of range".into());
        }
        match self {
            Self::Split {
                ratio: r,
                first,
                second,
                ..
            } => {
                if let Some((next, rest)) = path.split_first() {
                    if *next {
                        second.set_ratio(rest, ratio)
                    } else {
                        first.set_ratio(rest, ratio)
                    }
                } else {
                    *r = ratio;
                    Ok(())
                }
            }
            Self::Group(_) => Err("Split not found".into()),
        }
    }
    pub fn to_json(&self) -> Value {
        match self {
            Self::Group(id) => json!({"group":id}),
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                json!({"axis":match axis{Axis::Horizontal=>"horizontal",Axis::Vertical=>"vertical"},"ratio":ratio,"first":first.to_json(),"second":second.to_json()})
            }
        }
    }
    fn from_json(v: &Value, depth: usize, leaves: &mut usize) -> ModelResult<Self> {
        if depth > MAX_DEPTH {
            return Err("Layout exceeds maximum depth".into());
        }
        if v.get("group").is_some() {
            *leaves += 1;
            if *leaves > MAX_GROUPS {
                return Err("Too many groups".into());
            }
            // A node cannot be both a split and a leaf.
            if v.get("axis").is_some() || v.get("first").is_some() || v.get("second").is_some() {
                return Err("Ambiguous layout node".into());
            }
            return Ok(Self::Group(id(v, "group")?));
        }
        let axis = match v.get("axis").and_then(Value::as_str) {
            Some("horizontal") => Axis::Horizontal,
            Some("vertical") => Axis::Vertical,
            _ => return Err("Invalid split axis".into()),
        };
        let ratio = v
            .get("ratio")
            .and_then(Value::as_f64)
            .filter(|r| r.is_finite() && (MIN_RATIO..=MAX_RATIO).contains(r))
            .ok_or("Invalid split ratio")?;
        Ok(Self::Split {
            axis,
            ratio,
            first: Box::new(Self::from_json(&v["first"], depth + 1, leaves)?),
            second: Box::new(Self::from_json(&v["second"], depth + 1, leaves)?),
        })
    }
    fn remap(&mut self, map: &HashMap<Id, Id>) {
        match self {
            Self::Group(id) => *id = map[id],
            Self::Split { first, second, .. } => {
                first.remap(map);
                second.remap(map);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Desktop {
    pub id: Id,
    pub name: String,
    pub root: SplitNode,
    pub groups: Vec<Group>,
    pub focused_group: Id,
    pub zoomed_group: Option<Id>,
}
impl Desktop {
    fn empty(id: Id, group: Id, name: String) -> Self {
        Self {
            id,
            name,
            root: SplitNode::Group(group),
            groups: vec![Group::empty(group)],
            focused_group: group,
            zoomed_group: None,
        }
    }
    pub fn group(&self, id: Id) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }
    pub fn group_mut(&mut self, id: Id) -> Option<&mut Group> {
        self.groups.iter_mut().find(|g| g.id == id)
    }
    pub fn tab(&self, id: Id) -> Option<&Tab> {
        self.groups
            .iter()
            .flat_map(|g| &g.tabs)
            .find(|t| t.id == id)
    }
    pub fn tab_group(&self, id: Id) -> Option<Id> {
        self.groups
            .iter()
            .find(|g| g.tabs.iter().any(|t| t.id == id))
            .map(|g| g.id)
    }
    fn prune_empty(&mut self, group: Id) {
        if self.groups.len() > 1 && self.group(group).is_some_and(|g| g.tabs.is_empty()) {
            self.groups.retain(|g| g.id != group);
            self.root = self
                .root
                .clone()
                .without(group)
                .expect("another group remains");
            if self.focused_group == group {
                self.focused_group = self.root.group_ids()[0];
            }
            if self.zoomed_group == Some(group) {
                self.zoomed_group = None;
            }
        }
    }
    pub fn to_json(&self) -> Value {
        json!({"id":self.id,"name":self.name,"root":self.root.to_json(),"groups":self.groups.iter().map(|g|json!({"id":g.id,"active_tab":g.active_tab,"tabs":g.tabs.iter().map(|t|json!({"id":t.id,"title":t.title,"resource":t.resource.to_json()})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"focused_group":self.focused_group,"zoomed_group":self.zoomed_group})
    }
}

/// A layout template deliberately has no tab IDs, titles, or resource references.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutTemplate {
    pub id: Id,
    pub name: String,
    pub root: SplitNode,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Multiplexer {
    pub desktops: Vec<Desktop>,
    pub active_desktop: Id,
    pub templates: Vec<LayoutTemplate>,
    next_id: Id,
}
impl Default for Multiplexer {
    fn default() -> Self {
        Self {
            desktops: vec![Desktop::empty(1, 2, "Desktop 1".into())],
            active_desktop: 1,
            templates: Vec::new(),
            next_id: 3,
        }
    }
}
impl Multiplexer {
    fn allocate(&mut self) -> ModelResult<Id> {
        let id = self.next_id;
        if id >= u64::MAX - 1 {
            return Err("Layout ID space exhausted".into());
        }
        self.next_id = id.checked_add(1).ok_or("Layout ID space exhausted")?;
        Ok(id)
    }
    pub fn active(&self) -> &Desktop {
        self.desktops
            .iter()
            .find(|d| d.id == self.active_desktop)
            .expect("validated active desktop")
    }
    pub fn active_mut(&mut self) -> &mut Desktop {
        self.desktops
            .iter_mut()
            .find(|d| d.id == self.active_desktop)
            .expect("validated active desktop")
    }
    pub fn select_desktop(&mut self, id: Id) -> ModelResult<()> {
        if !self.desktops.iter().any(|d| d.id == id) {
            return Err("Desktop not found".into());
        }
        self.active_desktop = id;
        Ok(())
    }
    pub fn create_desktop(&mut self, name: impl Into<String>) -> ModelResult<Id> {
        let name = name.into();
        checked_text(&name, MAX_NAME)?;
        if self.desktops.len() >= MAX_DESKTOPS {
            return Err("Too many desktops".into());
        }
        let id = self.allocate()?;
        let group = self.allocate()?;
        self.desktops.push(Desktop::empty(id, group, name));
        self.active_desktop = id;
        Ok(id)
    }
    pub fn rename_desktop(&mut self, id: Id, name: impl Into<String>) -> ModelResult<()> {
        let name = name.into();
        checked_text(&name, MAX_NAME)?;
        self.desktops
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or("Desktop not found")?
            .name = name;
        Ok(())
    }
    /// Duplicate arrangement and references, not backing sessions/documents.
    pub fn duplicate_desktop(&mut self, id: Id, name: impl Into<String>) -> ModelResult<Id> {
        let name = name.into();
        checked_text(&name, MAX_NAME)?;
        if self.desktops.len() >= MAX_DESKTOPS {
            return Err("Too many desktops".into());
        }
        let mut desktop = self
            .desktops
            .iter()
            .find(|d| d.id == id)
            .ok_or("Desktop not found")?
            .clone();
        desktop.id = self.allocate()?;
        desktop.name = name;
        let mut map = HashMap::new();
        for group in &mut desktop.groups {
            let old = group.id;
            group.id = self.allocate()?;
            map.insert(old, group.id);
            for tab in &mut group.tabs {
                let old = tab.id;
                tab.id = self.allocate()?;
                if group.active_tab == Some(old) {
                    group.active_tab = Some(tab.id);
                }
            }
        }
        desktop.root.remap(&map);
        desktop.focused_group = map[&desktop.focused_group];
        desktop.zoomed_group = desktop.zoomed_group.map(|g| map[&g]);
        let id = desktop.id;
        self.desktops.push(desktop);
        self.active_desktop = id;
        Ok(id)
    }
    /// Keep at least one desktop; deleting an arrangement never destroys its resources.
    pub fn delete_desktop(&mut self, id: Id) -> ModelResult<()> {
        let index = self
            .desktops
            .iter()
            .position(|d| d.id == id)
            .ok_or("Desktop not found")?;
        if self.desktops.len() == 1 {
            return Err("Cannot delete the last desktop".into());
        }
        self.desktops.remove(index);
        if self.active_desktop == id {
            self.active_desktop = self.desktops[index.min(self.desktops.len() - 1)].id;
        }
        Ok(())
    }
    pub fn open(&mut self, resource: Resource, title: impl Into<String>) -> ModelResult<Id> {
        let group = self.active().focused_group;
        self.open_in_group(group, resource, title)
    }
    /// If already open on this desktop, focus that instance without renaming/moving it.
    pub fn open_in_group(
        &mut self,
        group: Id,
        resource: Resource,
        title: impl Into<String>,
    ) -> ModelResult<Id> {
        resource.validate()?;
        let title = title.into();
        checked_text(&title, MAX_NAME)?;
        if self.active().group(group).is_none() {
            return Err("Group not found".into());
        }
        if let Some(id) = self
            .active()
            .groups
            .iter()
            .flat_map(|g| &g.tabs)
            .find(|t| t.resource == resource)
            .map(|t| t.id)
        {
            self.activate_tab(id)?;
            return Ok(id);
        }
        if self
            .active()
            .groups
            .iter()
            .map(|g| g.tabs.len())
            .sum::<usize>()
            >= MAX_TABS
        {
            return Err("Too many tabs".into());
        }
        let id = self.allocate()?;
        let desktop = self.active_mut();
        let target = desktop.group_mut(group).unwrap();
        target.tabs.push(Tab {
            id,
            title,
            resource,
        });
        target.active_tab = Some(id);
        desktop.focused_group = group;
        if desktop.zoomed_group.is_some() {
            desktop.zoomed_group = Some(group);
        }
        Ok(id)
    }
    /// Bind draft identities across all desktops, preserving every tab ID/title.
    /// Reject collisions atomically rather than silently losing another UI instance.
    pub fn replace_resource(&mut self, old: &Resource, new: Resource) -> ModelResult<usize> {
        new.validate()?;
        if old == &new {
            return Ok(0);
        }
        for d in &self.desktops {
            let has_old = d
                .groups
                .iter()
                .flat_map(|g| &g.tabs)
                .any(|t| &t.resource == old);
            let has_new = d
                .groups
                .iter()
                .flat_map(|g| &g.tabs)
                .any(|t| t.resource == new);
            if has_old && has_new {
                return Err("Resource already open on a desktop".into());
            }
        }
        let mut replaced = 0;
        for tab in self
            .desktops
            .iter_mut()
            .flat_map(|d| &mut d.groups)
            .flat_map(|g| &mut g.tabs)
        {
            if &tab.resource == old {
                tab.resource = new.clone();
                replaced += 1;
            }
        }
        Ok(replaced)
    }
    pub fn activate_tab(&mut self, tab: Id) -> ModelResult<()> {
        let d = self.active_mut();
        let group = d.tab_group(tab).ok_or("Tab not found")?;
        d.group_mut(group).unwrap().active_tab = Some(tab);
        d.focused_group = group;
        if d.zoomed_group.is_some() {
            d.zoomed_group = Some(group);
        }
        Ok(())
    }
    pub fn rename_tab(&mut self, tab: Id, title: impl Into<String>) -> ModelResult<()> {
        let title = title.into();
        checked_text(&title, MAX_NAME)?;
        let tab = self
            .active_mut()
            .groups
            .iter_mut()
            .flat_map(|g| &mut g.tabs)
            .find(|t| t.id == tab)
            .ok_or("Tab not found")?;
        tab.title = title;
        Ok(())
    }
    pub fn focus_group(&mut self, group: Id) -> ModelResult<()> {
        let d = self.active_mut();
        if d.group(group).is_none() {
            return Err("Group not found".into());
        }
        d.focused_group = group;
        if d.zoomed_group.is_some() {
            d.zoomed_group = Some(group);
        }
        Ok(())
    }
    pub fn toggle_zoom(&mut self, group: Id) -> ModelResult<()> {
        let was_zoomed = self.active().zoomed_group == Some(group);
        self.focus_group(group)?;
        let d = self.active_mut();
        d.zoomed_group = if was_zoomed { None } else { Some(group) };
        Ok(())
    }
    pub fn close_tab(&mut self, tab: Id) -> ModelResult<()> {
        let d = self.active_mut();
        let group = d.tab_group(tab).ok_or("Tab not found")?;
        Self::take_tab(d.group_mut(group).unwrap(), tab)?;
        d.prune_empty(group);
        Ok(())
    }
    fn take_tab(group: &mut Group, tab: Id) -> ModelResult<Tab> {
        let index = group
            .tabs
            .iter()
            .position(|t| t.id == tab)
            .ok_or("Tab not found")?;
        let result = group.tabs.remove(index);
        if group.active_tab == Some(tab) {
            group.active_tab = group
                .tabs
                .get(index.min(group.tabs.len().saturating_sub(1)))
                .map(|t| t.id);
        }
        Ok(result)
    }
    /// Index is the final destination index after removal, clamped to destination length.
    pub fn move_tab(&mut self, tab: Id, target_group: Id, index: usize) -> ModelResult<()> {
        let d = self.active_mut();
        if d.group(target_group).is_none() {
            return Err("Target group not found".into());
        }
        let source = d.tab_group(tab).ok_or("Tab not found")?;
        let tab = Self::take_tab(d.group_mut(source).unwrap(), tab)?;
        let target = d.group_mut(target_group).unwrap();
        target.active_tab = Some(tab.id);
        target.tabs.insert(index.min(target.tabs.len()), tab);
        d.focused_group = target_group;
        if d.zoomed_group.is_some() {
            d.zoomed_group = Some(target_group);
        }
        if source != target_group {
            d.prune_empty(source);
        }
        Ok(())
    }
    pub fn split_group(&mut self, group: Id, edge: Edge) -> ModelResult<Id> {
        let d = self.active();
        let depth = d.root.depth_of(group, 0).ok_or("Group not found")?;
        if d.groups.len() >= MAX_GROUPS || depth >= MAX_DEPTH {
            return Err("Layout limit reached".into());
        }
        let id = self.allocate()?;
        let d = self.active_mut();
        d.root.split(group, id, edge);
        d.groups.push(Group::empty(id));
        d.focused_group = id;
        d.zoomed_group = None;
        Ok(id)
    }
    pub fn remove_empty_group(&mut self, group: Id) -> ModelResult<()> {
        let d = self.active_mut();
        let g = d.group(group).ok_or("Group not found")?;
        if !g.tabs.is_empty() {
            return Err("Group is not empty".into());
        }
        d.prune_empty(group);
        Ok(())
    }
    pub fn set_split_ratio(&mut self, path: &[bool], ratio: f64) -> ModelResult<()> {
        self.active_mut().root.set_ratio(path, ratio)
    }
    pub fn save_template(&mut self, name: impl Into<String>) -> ModelResult<Id> {
        let name = name.into();
        checked_text(&name, MAX_NAME)?;
        if self.templates.len() >= MAX_TEMPLATES {
            return Err("Too many templates".into());
        }
        // Normalize geometry leaves to local slots, not live group IDs.
        let mut root = self.active().root.clone();
        let map = root
            .group_ids()
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i as Id + 1))
            .collect();
        root.remap(&map);
        let id = self.allocate()?;
        self.templates.push(LayoutTemplate { id, name, root });
        Ok(id)
    }
    pub fn delete_template(&mut self, id: Id) -> ModelResult<()> {
        let i = self
            .templates
            .iter()
            .position(|t| t.id == id)
            .ok_or("Template not found")?;
        self.templates.remove(i);
        Ok(())
    }
    /// Apply geometry to this desktop while preserving every tab/reference. Existing
    /// groups are assigned in tree order; excess groups' tabs join the last slot.
    pub fn apply_template(&mut self, id: Id) -> ModelResult<()> {
        let mut root = self
            .templates
            .iter()
            .find(|t| t.id == id)
            .ok_or("Template not found")?
            .root
            .clone();
        let slots = root.group_ids();
        let mut map = HashMap::new();
        let mut groups = Vec::new();
        for slot in slots {
            let id = self.allocate()?;
            map.insert(slot, id);
            groups.push(Group::empty(id));
        }
        root.remap(&map);
        let d = self.active_mut();
        let old_active = d.group(d.focused_group).and_then(|g| g.active_tab);
        for (i, old_id) in d.root.group_ids().iter().enumerate() {
            let old = d.group(*old_id).unwrap();
            let dest = i.min(groups.len() - 1);
            groups[dest].tabs.extend(old.tabs.clone());
            if old.active_tab.is_some() {
                groups[dest].active_tab = old.active_tab;
            }
        }
        for g in &mut groups {
            if g.active_tab.is_none() {
                g.active_tab = g.tabs.first().map(|t| t.id);
            }
        }
        d.focused_group = old_active
            .and_then(|tab| {
                groups
                    .iter()
                    .find(|g| g.tabs.iter().any(|t| t.id == tab))
                    .map(|g| g.id)
            })
            .unwrap_or(groups[0].id);
        if let Some(tab) = old_active {
            groups
                .iter_mut()
                .find(|g| g.id == d.focused_group)
                .unwrap()
                .active_tab = Some(tab);
        }
        d.groups = groups;
        d.root = root;
        d.zoomed_group = None;
        Ok(())
    }
    pub fn to_json(&self) -> Value {
        json!({"version":1,"active_desktop":self.active_desktop,"desktops":self.desktops.iter().map(Desktop::to_json).collect::<Vec<_>>(),"templates":self.templates.iter().map(|t|json!({"id":t.id,"name":t.name,"root":t.root.to_json()})).collect::<Vec<_>>()})
    }
    /// Strict graph/reference validation; unknown additive fields are ignored. IDs
    /// are globally unique (template-local slots excepted) and allocation is rebuilt.
    pub fn from_json(v: &Value) -> ModelResult<Self> {
        if v.get("version").and_then(Value::as_u64) != Some(1) {
            return Err("Unsupported workspace layout version".into());
        }
        let desktops_json = array(v, "desktops", MAX_DESKTOPS)?;
        if desktops_json.is_empty() {
            return Err("At least one desktop required".into());
        }
        let mut ids = HashSet::new();
        let mut desktops = Vec::new();
        for d in desktops_json {
            let desktop_id = unique_id(d, "id", &mut ids)?;
            let name = text(d, "name", MAX_NAME)?;
            let root = SplitNode::from_json(&d["root"], 0, &mut 0)?;
            let mut leaf_ids = HashSet::new();
            for id in root.group_ids() {
                if !leaf_ids.insert(id) {
                    return Err("Repeated group in split tree".into());
                }
            }
            let mut groups = Vec::new();
            let mut resources = HashSet::new();
            let mut total_tabs = 0;
            for g in array(d, "groups", MAX_GROUPS)? {
                let group_id = unique_id(g, "id", &mut ids)?;
                if !leaf_ids.remove(&group_id) {
                    return Err("Group absent from split tree".into());
                }
                let tabs_json = array(g, "tabs", MAX_TABS)?;
                total_tabs += tabs_json.len();
                if total_tabs > MAX_TABS {
                    return Err("Too many tabs".into());
                }
                let mut tabs = Vec::new();
                for t in tabs_json {
                    let tab_id = unique_id(t, "id", &mut ids)?;
                    let title = text(t, "title", MAX_NAME)?;
                    let resource = Resource::from_json(&t["resource"])?;
                    if !resources.insert(resource.clone()) {
                        return Err("Duplicate resource on desktop".into());
                    }
                    tabs.push(Tab {
                        id: tab_id,
                        title,
                        resource,
                    });
                }
                let active_tab = optional_id(g, "active_tab")?;
                if tabs.is_empty() != active_tab.is_none()
                    || active_tab.is_some_and(|id| !tabs.iter().any(|t| t.id == id))
                {
                    return Err("Invalid active tab".into());
                }
                groups.push(Group {
                    id: group_id,
                    tabs,
                    active_tab,
                });
            }
            if groups.is_empty() || !leaf_ids.is_empty() {
                return Err("Split tree references missing groups".into());
            }
            let focused_group = id(d, "focused_group")?;
            let zoomed_group = optional_id(d, "zoomed_group")?;
            if !groups.iter().any(|g| g.id == focused_group)
                || zoomed_group.is_some_and(|id| !groups.iter().any(|g| g.id == id))
            {
                return Err("Invalid focused/zoomed group".into());
            }
            if zoomed_group.is_some_and(|id| id != focused_group) {
                return Err("Zoom must follow focus".into());
            }
            desktops.push(Desktop {
                id: desktop_id,
                name,
                root,
                groups,
                focused_group,
                zoomed_group,
            });
        }
        let active_desktop = id(v, "active_desktop")?;
        if !desktops.iter().any(|d| d.id == active_desktop) {
            return Err("Active desktop missing".into());
        }
        let mut templates = Vec::new();
        for t in array(v, "templates", MAX_TEMPLATES)? {
            let template_id = unique_id(t, "id", &mut ids)?;
            let name = text(t, "name", MAX_NAME)?;
            let root = SplitNode::from_json(&t["root"], 0, &mut 0)?;
            let leaves = root.group_ids();
            if leaves.iter().copied().collect::<HashSet<_>>().len() != leaves.len() {
                return Err("Duplicate template slot".into());
            }
            templates.push(LayoutTemplate {
                id: template_id,
                name,
                root,
            });
        }
        let next_id = ids
            .into_iter()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .filter(|n| *n < u64::MAX)
            .ok_or("Layout ID space exhausted")?;
        Ok(Self {
            desktops,
            active_desktop,
            templates,
            next_id,
        })
    }
}

fn checked_text(s: &str, max: usize) -> ModelResult<&str> {
    if s.trim().is_empty() || s.len() > max || s.chars().any(char::is_control) {
        Err("Invalid or oversized layout text".into())
    } else {
        Ok(s)
    }
}
fn text(v: &Value, key: &str, max: usize) -> ModelResult<String> {
    let s = v
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing text: {key}"))?;
    Ok(checked_text(s, max)?.to_owned())
}
fn optional_text(v: &Value, key: &str) -> ModelResult<Option<String>> {
    match v.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(checked_text(s, MAX_TEXT)?.to_owned())),
        _ => Err(format!("Invalid optional text: {key}")),
    }
}
fn id(v: &Value, key: &str) -> ModelResult<Id> {
    v.get(key)
        .and_then(Value::as_u64)
        .filter(|id| *id > 0 && *id < u64::MAX - 1)
        .ok_or_else(|| format!("Invalid ID: {key}"))
}
fn optional_id(v: &Value, key: &str) -> ModelResult<Option<Id>> {
    match v.get(key) {
        Some(Value::Null) => Ok(None),
        Some(_) => id(v, key).map(Some),
        None => Err(format!("Missing optional ID: {key}")),
    }
}
fn unique_id(v: &Value, key: &str, ids: &mut HashSet<Id>) -> ModelResult<Id> {
    let id = id(v, key)?;
    if !ids.insert(id) {
        return Err("Duplicate layout ID".into());
    }
    Ok(id)
}
fn array<'a>(v: &'a Value, key: &str, max: usize) -> ModelResult<&'a Vec<Value>> {
    v.get(key)
        .and_then(Value::as_array)
        .filter(|a| a.len() <= max)
        .ok_or_else(|| format!("Missing or oversized array: {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chat(id: &str) -> Resource {
        Resource::Chat {
            session_id: id.into(),
            workshop_id: "local".into(),
        }
    }
    fn roundtrip(m: &Multiplexer) -> Multiplexer {
        let bytes = serde_json::to_vec(&m.to_json()).unwrap();
        let restored = Multiplexer::from_json(&serde_json::from_slice(&bytes).unwrap()).unwrap();
        assert_eq!(m.to_json(), restored.to_json());
        restored
    }
    #[test]
    fn draft_binding_is_atomic_and_preserves_ui_instances() {
        let mut m = Multiplexer::default();
        let draft = chat("pending:draft-1");
        let actual = chat("real-session");
        let first = m.active_desktop;
        let tab = m.open(draft.clone(), "My draft").unwrap();
        m.duplicate_desktop(first, "Other desktop").unwrap();
        assert_eq!(m.replace_resource(&draft, actual.clone()).unwrap(), 2);
        m.select_desktop(first).unwrap();
        assert_eq!(m.active().tab(tab).unwrap().resource, actual);
        assert_eq!(m.active().tab(tab).unwrap().title, "My draft");
        m.open(draft.clone(), "Another draft").unwrap();
        let before = m.to_json();
        assert!(m.replace_resource(&draft, actual).is_err());
        assert_eq!(m.to_json(), before);
        roundtrip(&m);
    }
    #[test]
    fn chat_workshops_and_code_undertakings_do_not_alias() {
        let mut m = Multiplexer::default();
        for workshop_id in ["local", "remote"] {
            m.open(
                Resource::Chat {
                    session_id: "same-session-id".into(),
                    workshop_id: workshop_id.into(),
                },
                "Chat",
            )
            .unwrap();
        }
        for work_id in ["work-one", "work-two"] {
            m.open(
                Resource::Code {
                    path: "src/main.rs".into(),
                    workshop_id: "local".into(),
                    work_id: work_id.into(),
                },
                "Code",
            )
            .unwrap();
        }
        assert_eq!(m.active().groups[0].tabs.len(), 4);
        roundtrip(&m);
    }
    #[test]
    fn reader_rejects_repeated_leaves_and_invalid_template_slots() {
        let mut m = Multiplexer::default();
        let g = m.active().focused_group;
        m.split_group(g, Edge::Right).unwrap();
        m.save_template("Pair").unwrap();
        let base = m.to_json();
        let mut v = base.clone();
        v["desktops"][0]["root"]["second"] = v["desktops"][0]["root"]["first"].clone();
        assert!(Multiplexer::from_json(&v).is_err());
        let mut v = base.clone();
        v["templates"][0]["root"]["second"] = v["templates"][0]["root"]["first"].clone();
        assert!(Multiplexer::from_json(&v).is_err());
        let mut v = base.clone();
        v["templates"][0]["id"] = v["desktops"][0]["id"].clone();
        assert!(Multiplexer::from_json(&v).is_err());
        let mut v = base.clone();
        v["desktops"][0]["zoomed_group"] = json!(g);
        assert!(Multiplexer::from_json(&v).is_err());
    }
    #[test]
    fn all_resource_kinds_preserve_identity() {
        let resources = vec![
            chat("session"),
            Resource::Terminal {
                session_id: "shell".into(),
                workshop_id: "remote".into(),
                work_id: None,
            },
            Resource::Terminal {
                session_id: "shell2".into(),
                workshop_id: "remote".into(),
                work_id: Some("undertaking".into()),
            },
            Resource::Note {
                path: "notes/a.md".into(),
                workshop_id: "remote".into(),
            },
            Resource::Code {
                path: "src/main.rs".into(),
                workshop_id: "local".into(),
                work_id: "undertaking".into(),
            },
            Resource::Create {
                document_id: "document".into(),
                workshop_id: "remote".into(),
            },
            Resource::Review {
                work_id: "work".into(),
                workshop_id: "remote".into(),
            },
        ];
        let mut m = Multiplexer::default();
        for r in &resources {
            assert_eq!(&Resource::from_json(&r.to_json()).unwrap(), r);
            m.open(r.clone(), r.kind()).unwrap();
        }
        let m = roundtrip(&m);
        assert_eq!(
            m.active().groups[0]
                .tabs
                .iter()
                .map(|t| t.resource.clone())
                .collect::<Vec<_>>(),
            resources
        );
    }
    #[test]
    fn open_deduplicates_across_groups_and_focuses_existing() {
        let mut m = Multiplexer::default();
        let original = m.active().focused_group;
        let a = m.open(chat("a"), "Original").unwrap();
        let other = m.split_group(original, Edge::Right).unwrap();
        m.toggle_zoom(other).unwrap();
        assert_eq!(m.open(chat("a"), "Replacement").unwrap(), a);
        assert_eq!(m.active().focused_group, original);
        assert_eq!(m.active().zoomed_group, Some(original));
        assert_eq!(m.active().tab(a).unwrap().title, "Original");
        assert!(m.active().group(other).unwrap().tabs.is_empty());
        roundtrip(&m);
    }
    #[test]
    fn workshop_and_kind_are_part_of_resource_identity() {
        let mut m = Multiplexer::default();
        let a = m
            .open(
                Resource::Code {
                    path: "same".into(),
                    workshop_id: "one".into(),
                    work_id: "undertaking".into(),
                },
                "One",
            )
            .unwrap();
        let b = m
            .open(
                Resource::Code {
                    path: "same".into(),
                    workshop_id: "two".into(),
                    work_id: "undertaking".into(),
                },
                "Two",
            )
            .unwrap();
        let c = m
            .open(
                Resource::Note {
                    path: "same".into(),
                    workshop_id: "one".into(),
                },
                "Note",
            )
            .unwrap();
        assert_ne!(a, b);
        assert_ne!(a, c);
        roundtrip(&m);
    }
    #[test]
    fn reorder_and_cross_group_move_preserve_tab_ids() {
        let mut m = Multiplexer::default();
        let left = m.active().focused_group;
        let a = m.open(chat("a"), "A").unwrap();
        let b = m.open(chat("b"), "B").unwrap();
        let c = m.open(chat("c"), "C").unwrap();
        m.move_tab(a, left, usize::MAX).unwrap();
        assert_eq!(
            m.active()
                .group(left)
                .unwrap()
                .tabs
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            vec![b, c, a]
        );
        let right = m.split_group(left, Edge::Right).unwrap();
        m.move_tab(b, right, 0).unwrap();
        assert_eq!(m.active().group(left).unwrap().active_tab, Some(a));
        m.move_tab(c, right, 0).unwrap();
        m.move_tab(a, right, 1).unwrap();
        assert_eq!(m.active().root, SplitNode::Group(right));
        assert_eq!(
            m.active()
                .group(right)
                .unwrap()
                .tabs
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            vec![c, a, b]
        );
        roundtrip(&m);
    }
    #[test]
    fn close_selects_neighbor_and_collapses_nested_tree() {
        let mut m = Multiplexer::default();
        let left = m.active().focused_group;
        m.open(chat("left"), "Left").unwrap();
        let right = m.split_group(left, Edge::Right).unwrap();
        let a = m.open(chat("a"), "A").unwrap();
        let b = m.open(chat("b"), "B").unwrap();
        let c = m.open(chat("c"), "C").unwrap();
        m.activate_tab(b).unwrap();
        m.close_tab(b).unwrap();
        assert_eq!(m.active().group(right).unwrap().active_tab, Some(c));
        let bottom = m.split_group(right, Edge::Bottom).unwrap();
        let z = m.open(chat("z"), "Z").unwrap();
        m.toggle_zoom(bottom).unwrap();
        m.close_tab(z).unwrap();
        assert_eq!(m.active().groups.len(), 2);
        assert!(m.active().zoomed_group.is_none());
        m.close_tab(a).unwrap();
        m.close_tab(c).unwrap();
        assert_eq!(m.active().root, SplitNode::Group(left));
        roundtrip(&m);
    }
    #[test]
    fn last_empty_group_and_desktop_are_retained() {
        let mut m = Multiplexer::default();
        let tab = m.open(chat("a"), "A").unwrap();
        m.close_tab(tab).unwrap();
        let group = m.active().focused_group;
        m.remove_empty_group(group).unwrap();
        assert_eq!(m.active().groups.len(), 1);
        assert!(m.delete_desktop(m.active_desktop).is_err());
        roundtrip(&m);
    }
    #[test]
    fn edge_geometry_nested_ratios_and_zoom() {
        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            let mut m = Multiplexer::default();
            let old = m.active().focused_group;
            let new = m.split_group(old, edge).unwrap();
            let SplitNode::Split {
                axis,
                first,
                second,
                ..
            } = &m.active().root
            else {
                panic!()
            };
            assert_eq!(
                *axis,
                if matches!(edge, Edge::Left | Edge::Right) {
                    Axis::Horizontal
                } else {
                    Axis::Vertical
                }
            );
            assert_eq!(
                **first,
                SplitNode::Group(if matches!(edge, Edge::Left | Edge::Top) {
                    new
                } else {
                    old
                })
            );
            assert_ne!(first, second);
            m.set_split_ratio(&[], 0.7).unwrap();
            let next = m.split_group(new, Edge::Bottom).unwrap();
            m.toggle_zoom(old).unwrap();
            assert_eq!(m.active().zoomed_group, Some(old));
            m.toggle_zoom(next).unwrap();
            assert_eq!(m.active().zoomed_group, Some(next));
            m.toggle_zoom(next).unwrap();
            assert!(m.active().zoomed_group.is_none());
            roundtrip(&m);
        }
    }
    #[test]
    fn desktops_are_independent_arrangements_of_shared_resources() {
        let mut m = Multiplexer::default();
        let first = m.active_desktop;
        let tab = m.open(chat("shared"), "Shared").unwrap();
        let group = m.active().focused_group;
        m.toggle_zoom(group).unwrap();
        let second = m.duplicate_desktop(first, "Development").unwrap();
        let duplicate = m.active().groups[0].tabs[0].id;
        assert_ne!(tab, duplicate);
        assert_eq!(m.active().tab(duplicate).unwrap().resource, chat("shared"));
        m.rename_desktop(second, "Writing").unwrap();
        m.close_tab(duplicate).unwrap();
        m.select_desktop(first).unwrap();
        assert!(m.active().tab(tab).is_some());
        let third = m.create_desktop("New").unwrap();
        assert_ne!(third, first);
        m.delete_desktop(third).unwrap();
        m.delete_desktop(second).unwrap();
        assert_eq!(m.active_desktop, first);
        roundtrip(&m);
    }
    #[test]
    fn templates_are_geometry_only_and_apply_without_dropping_tabs() {
        let mut m = Multiplexer::default();
        let single = m.save_template("Single").unwrap();
        let g = m.active().focused_group;
        let a = m.open(chat("secret-session"), "Private title").unwrap();
        let other = m.split_group(g, Edge::Right).unwrap();
        let b = m.open(chat("second-secret"), "Other title").unwrap();
        m.set_split_ratio(&[], 0.3).unwrap();
        let pair = m.save_template("Pair").unwrap();
        let serialized = m.to_json()["templates"].to_string();
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("title"));
        assert!(!serialized.contains("resource"));
        m.apply_template(single).unwrap();
        assert_eq!(m.active().groups.len(), 1);
        assert!(m.active().tab(a).is_some() && m.active().tab(b).is_some());
        assert_eq!(
            m.active()
                .group(m.active().focused_group)
                .unwrap()
                .active_tab,
            Some(b)
        );
        m.apply_template(pair).unwrap();
        assert_eq!(m.active().groups.len(), 2);
        assert!(m.active().group(other).is_none());
        assert!(m.active().tab(a).is_some() && m.active().tab(b).is_some());
        roundtrip(&m);
        m.delete_template(single).unwrap();
        assert!(m.apply_template(single).is_err());
    }
    #[test]
    fn invalid_operations_do_not_change_serialized_layout() {
        let mut m = Multiplexer::default();
        let tab = m.open(chat("a"), "A").unwrap();
        let before = m.to_json();
        assert!(m.move_tab(tab, 999, 0).is_err());
        assert!(m.close_tab(999).is_err());
        assert!(m.focus_group(999).is_err());
        assert!(m.toggle_zoom(999).is_err());
        assert!(m.split_group(999, Edge::Left).is_err());
        assert!(m.select_desktop(999).is_err());
        assert!(m.rename_tab(tab, "").is_err());
        assert!(m.create_desktop("\nBad").is_err());
        assert!(m.open(chat(""), "Bad").is_err());
        assert_eq!(m.to_json(), before);
    }
    #[test]
    fn reader_rejects_dangling_duplicate_and_inconsistent_references() {
        let mut m = Multiplexer::default();
        m.open(chat("a"), "A").unwrap();
        let base = m.to_json();
        let corruptions = vec![
            ("/version", json!(2)),
            ("/active_desktop", json!(999)),
            ("/desktops", json!([])),
            ("/desktops/0/id", json!(0)),
            ("/desktops/0/name", json!("")),
            ("/desktops/0/root/group", json!(999)),
            ("/desktops/0/focused_group", json!(999)),
            ("/desktops/0/zoomed_group", json!(999)),
            ("/desktops/0/groups/0/id", json!(1)),
            ("/desktops/0/groups/0/active_tab", Value::Null),
            ("/desktops/0/groups/0/tabs/0/id", json!(2)),
            (
                "/desktops/0/groups/0/tabs/0/resource/kind",
                json!("unknown"),
            ),
        ];
        for (path, replacement) in corruptions {
            let mut v = base.clone();
            *v.pointer_mut(path).unwrap() = replacement;
            assert!(Multiplexer::from_json(&v).is_err(), "{path}");
        }
        let mut v = base.clone();
        let mut duplicate = v["desktops"][0]["groups"][0]["tabs"][0].clone();
        duplicate["id"] = json!(99);
        v["desktops"][0]["groups"][0]["tabs"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(Multiplexer::from_json(&v).is_err());
        for v in [
            Value::Null,
            json!(42),
            json!("layout"),
            json!({"version":1}),
        ] {
            assert!(Multiplexer::from_json(&v).is_err());
        }
    }
    #[test]
    fn read_limits_and_split_depth_are_bounded() {
        let base = Multiplexer::default().to_json();
        let mut oversized = base.clone();
        oversized["desktops"] = json!(vec![base["desktops"][0].clone(); MAX_DESKTOPS + 1]);
        assert!(Multiplexer::from_json(&oversized).is_err());
        let mut m = Multiplexer::default();
        for _ in 0..MAX_DEPTH {
            m.split_group(m.active().focused_group, Edge::Bottom)
                .unwrap();
        }
        assert!(
            m.split_group(m.active().focused_group, Edge::Bottom)
                .is_err()
        );
        roundtrip(&m);
        let mut deep = json!({"group":2});
        for _ in 0..MAX_DEPTH + 1 {
            deep = json!({"axis":"horizontal","ratio":0.5,"first":deep,"second":{"group":2}});
        }
        let mut v = base.clone();
        v["desktops"][0]["root"] = deep;
        assert!(Multiplexer::from_json(&v).is_err());
        let mut v = base.clone();
        v["desktops"][0]["name"] = json!("x".repeat(MAX_NAME + 1));
        assert!(Multiplexer::from_json(&v).is_err());
    }
    #[test]
    fn ratios_and_ambiguous_nodes_are_rejected() {
        let mut m = Multiplexer::default();
        let g = m.active().focused_group;
        m.split_group(g, Edge::Right).unwrap();
        let before = m.to_json();
        for ratio in [f64::NAN, f64::INFINITY, -0.1, 0.0, 1.0, 2.0] {
            assert!(m.set_split_ratio(&[], ratio).is_err());
        }
        assert_eq!(m.to_json(), before);
        for r in [json!(0), json!(1), Value::Null, json!("0.5")] {
            let mut v = before.clone();
            v["desktops"][0]["root"]["ratio"] = r;
            assert!(Multiplexer::from_json(&v).is_err());
        }
        let mut v = Multiplexer::default().to_json();
        v["desktops"][0]["root"]["first"] = json!({"group":2});
        assert!(Multiplexer::from_json(&v).is_err());
    }
    #[test]
    fn generated_operation_sequences_keep_graph_valid() {
        let mut m = Multiplexer::default();
        let mut random = 0x12345678_u64;
        for i in 0..400_u64 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let group = m.active().focused_group;
            let tabs = m
                .active()
                .groups
                .iter()
                .flat_map(|g| &g.tabs)
                .map(|t| t.id)
                .collect::<Vec<_>>();
            match (random >> 32) % 8 {
                0 | 1 => {
                    let _ = m.open(chat(&format!("session-{}", i % 31)), format!("Tab {i}"));
                }
                2 => {
                    let _ = m.split_group(group, Edge::Right);
                }
                3 if !tabs.is_empty() => {
                    let _ = m.close_tab(tabs[(random as usize) % tabs.len()]);
                }
                4 if !tabs.is_empty() => {
                    let _ = m.move_tab(
                        tabs[(random as usize) % tabs.len()],
                        group,
                        (random % 17) as usize,
                    );
                }
                5 => {
                    let _ = m.toggle_zoom(group);
                }
                6 => {
                    let _ = m.remove_empty_group(group);
                }
                _ => {
                    let ids = m.active().root.group_ids();
                    m.focus_group(ids[(random as usize) % ids.len()]).unwrap();
                }
            }
            roundtrip(&m);
        }
    }
}
