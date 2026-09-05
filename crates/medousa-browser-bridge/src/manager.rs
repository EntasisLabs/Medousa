use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Mutex;

use medousa_browser_lite::fetch_url_markdown;
use once_cell::sync::Lazy;
use uuid::Uuid;

use crate::model::{
    BrowserControl, BrowserObservation, BrowserObservationCapture, BrowserObservationState,
    BrowserSemanticNode, BrowserSnapshot, BrowserTab, TabGroup, TabOpenedBy,
    BROWSER_OBSERVATION_SCHEMA_VERSION,
};

const OBSERVATION_HISTORY_CAPACITY: usize = 32;
const MAX_OBSERVATION_NODES: usize = 1_024;
const MAX_OBSERVATION_ID_BYTES: usize = 256;
const MAX_OBSERVATION_URL_BYTES: usize = 8 * 1_024;
const MAX_OBSERVATION_TITLE_BYTES: usize = 512;
const MAX_OBSERVATION_ROLE_BYTES: usize = 64;
const MAX_OBSERVATION_NAME_BYTES: usize = 1_024;

#[derive(Debug, Clone)]
struct ObservationDelta {
    from_revision: u64,
    to_revision: u64,
    document_id: String,
    nodes: Vec<BrowserSemanticNode>,
    removed_refs: Vec<String>,
}

#[derive(Debug, Clone)]
struct ObservationRecord {
    current: BrowserObservationCapture,
    revision: u64,
    history: VecDeque<ObservationDelta>,
}

impl ObservationRecord {
    fn new(capture: BrowserObservationCapture) -> Result<Self, String> {
        validate_observation_capture(&capture)?;
        if capture.unchanged {
            return Err("first browser observation cannot be an unchanged marker".to_string());
        }
        Ok(Self {
            current: capture,
            revision: 1,
            history: VecDeque::new(),
        })
    }

    fn ingest(&mut self, capture: BrowserObservationCapture) -> Result<(), String> {
        validate_observation_capture(&capture)?;
        if capture.unchanged {
            if capture.document_id != self.current.document_id || capture.url != self.current.url {
                return Err(
                    "unchanged browser observation does not match the mirrored document"
                        .to_string(),
                );
            }
            self.current.captured_at_ms = capture.captured_at_ms;
            return Ok(());
        }

        let same_document = capture.document_id == self.current.document_id;
        let unchanged = same_document
            && capture.url == self.current.url
            && capture.title == self.current.title
            && capture.viewport == self.current.viewport
            && capture.nodes == self.current.nodes
            && capture.truncated == self.current.truncated;
        if unchanged {
            self.current.captured_at_ms = capture.captured_at_ms;
            return Ok(());
        }

        let from_revision = self.revision;
        self.revision = self.revision.saturating_add(1);
        if !same_document {
            // A document replacement invalidates every opaque element ref. Do
            // not bridge delta cursors across that trust boundary.
            self.history.clear();
            self.current = capture;
            return Ok(());
        }

        let old_nodes = nodes_by_ref(&self.current.nodes);
        let new_nodes = nodes_by_ref(&capture.nodes);
        let nodes = capture
            .nodes
            .iter()
            .filter(|node| old_nodes.get(&node.element_ref) != Some(node))
            .cloned()
            .collect();
        let removed_refs = old_nodes
            .keys()
            .filter(|element_ref| !new_nodes.contains_key(*element_ref))
            .cloned()
            .collect();
        self.history.push_back(ObservationDelta {
            from_revision,
            to_revision: self.revision,
            document_id: capture.document_id.clone(),
            nodes,
            removed_refs,
        });
        while self.history.len() > OBSERVATION_HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.current = capture;
        Ok(())
    }

    fn state(&self) -> BrowserObservationState {
        BrowserObservationState {
            tab_id: self.current.tab_id.clone(),
            url: self.current.url.clone(),
            document_id: self.current.document_id.clone(),
            revision: self.revision,
        }
    }

    fn project(&self, since_revision: Option<u64>, max_nodes: usize) -> BrowserObservation {
        let max_nodes = max_nodes.clamp(1, MAX_OBSERVATION_NODES);
        let Some(base_revision) = since_revision.filter(|revision| *revision > 0) else {
            return self.full_projection(max_nodes);
        };
        if base_revision == self.revision {
            return self.delta_projection(base_revision, Vec::new(), Vec::new(), max_nodes);
        }
        if base_revision > self.revision {
            return self.full_projection(max_nodes);
        }

        let mut cursor = base_revision;
        let mut changed = BTreeMap::<String, BrowserSemanticNode>::new();
        let mut removed = BTreeSet::<String>::new();
        for delta in &self.history {
            if delta.from_revision != cursor {
                continue;
            }
            if delta.document_id != self.current.document_id {
                return self.full_projection(max_nodes);
            }
            for node in &delta.nodes {
                removed.remove(&node.element_ref);
                changed.insert(node.element_ref.clone(), node.clone());
            }
            for element_ref in &delta.removed_refs {
                changed.remove(element_ref);
                removed.insert(element_ref.clone());
            }
            cursor = delta.to_revision;
            if cursor == self.revision {
                return self.delta_projection(
                    base_revision,
                    changed.into_values().collect(),
                    removed.into_iter().collect(),
                    max_nodes,
                );
            }
        }
        self.full_projection(max_nodes)
    }

    fn full_projection(&self, max_nodes: usize) -> BrowserObservation {
        let response_truncated = self.current.nodes.len() > max_nodes;
        BrowserObservation {
            schema_version: BROWSER_OBSERVATION_SCHEMA_VERSION,
            tab_id: self.current.tab_id.clone(),
            url: self.current.url.clone(),
            title: self.current.title.clone(),
            document_id: self.current.document_id.clone(),
            revision: self.revision,
            base_revision: None,
            full: true,
            viewport: self.current.viewport.clone(),
            nodes: self.current.nodes.iter().take(max_nodes).cloned().collect(),
            removed_refs: Vec::new(),
            truncated: self.current.truncated || response_truncated,
            captured_at_ms: self.current.captured_at_ms,
            untrusted_content: true,
        }
    }

    fn delta_projection(
        &self,
        base_revision: u64,
        nodes: Vec<BrowserSemanticNode>,
        removed_refs: Vec<String>,
        max_nodes: usize,
    ) -> BrowserObservation {
        let response_truncated = nodes.len() > max_nodes;
        BrowserObservation {
            schema_version: BROWSER_OBSERVATION_SCHEMA_VERSION,
            tab_id: self.current.tab_id.clone(),
            url: self.current.url.clone(),
            title: self.current.title.clone(),
            document_id: self.current.document_id.clone(),
            revision: self.revision,
            base_revision: Some(base_revision),
            full: false,
            viewport: self.current.viewport.clone(),
            nodes: nodes.into_iter().take(max_nodes).collect(),
            removed_refs,
            truncated: self.current.truncated || response_truncated,
            captured_at_ms: self.current.captured_at_ms,
            untrusted_content: true,
        }
    }
}

fn nodes_by_ref(nodes: &[BrowserSemanticNode]) -> BTreeMap<String, &BrowserSemanticNode> {
    nodes
        .iter()
        .map(|node| (node.element_ref.clone(), node))
        .collect()
}

fn validate_observation_capture(capture: &BrowserObservationCapture) -> Result<(), String> {
    if capture.tab_id.trim().is_empty()
        || capture.url.trim().is_empty()
        || capture.document_id.trim().is_empty()
    {
        return Err("browser observation identity is incomplete".to_string());
    }
    if capture.tab_id.len() > MAX_OBSERVATION_ID_BYTES
        || capture.document_id.len() > MAX_OBSERVATION_ID_BYTES
    {
        return Err("browser observation identity exceeds its byte limit".to_string());
    }
    if capture.url.len() > MAX_OBSERVATION_URL_BYTES
        || capture.title.len() > MAX_OBSERVATION_TITLE_BYTES
    {
        return Err("browser observation page metadata exceeds its byte limit".to_string());
    }
    if capture.unchanged && !capture.nodes.is_empty() {
        return Err("unchanged browser observation cannot contain nodes".to_string());
    }
    if capture.nodes.len() > MAX_OBSERVATION_NODES {
        return Err(format!(
            "browser observation exceeds {MAX_OBSERVATION_NODES} node limit"
        ));
    }
    let mut refs = BTreeSet::new();
    for node in &capture.nodes {
        if node.element_ref.trim().is_empty() {
            return Err("browser observation contains an empty element ref".to_string());
        }
        if node.element_ref.len() > MAX_OBSERVATION_ID_BYTES
            || node
                .parent_ref
                .as_deref()
                .is_some_and(|value| value.len() > MAX_OBSERVATION_ID_BYTES)
        {
            return Err("browser observation element ref exceeds its byte limit".to_string());
        }
        if node.role.len() > MAX_OBSERVATION_ROLE_BYTES
            || node.tag.len() > MAX_OBSERVATION_ROLE_BYTES
            || node.name.len() > MAX_OBSERVATION_NAME_BYTES
            || node
                .value
                .as_deref()
                .is_some_and(|value| value.len() > MAX_OBSERVATION_NAME_BYTES)
            || node
                .href
                .as_deref()
                .is_some_and(|value| value.len() > MAX_OBSERVATION_URL_BYTES)
        {
            return Err("browser observation node metadata exceeds its byte limit".to_string());
        }
        if node.sensitive && node.value.is_some() {
            return Err("sensitive browser observations cannot expose values".to_string());
        }
        if !refs.insert(node.element_ref.as_str()) {
            return Err(format!(
                "browser observation contains duplicate element ref {}",
                node.element_ref
            ));
        }
    }
    Ok(())
}

#[derive(Default)]
struct TabGroupRegistry {
    groups: HashMap<String, TabGroup>,
    current_group_id: Option<String>,
    observations: HashMap<String, ObservationRecord>,
}

impl TabGroupRegistry {
    fn current(&self) -> Option<TabGroup> {
        self.current_group_id
            .as_ref()
            .and_then(|group_id| self.groups.get(group_id))
            .cloned()
    }

    fn touch(&mut self, tab_group_id: &str) {
        if self.groups.contains_key(tab_group_id) {
            self.current_group_id = Some(tab_group_id.to_string());
        }
    }
}

static REGISTRY: Lazy<Mutex<TabGroupRegistry>> =
    Lazy::new(|| Mutex::new(TabGroupRegistry::default()));

fn tab_label_from_url(url: &str) -> String {
    let trimmed = url.trim();
    if let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    {
        let host = rest.split('/').next().unwrap_or(rest);
        if !host.is_empty() {
            return host.to_string();
        }
    }
    trimmed.to_string()
}

pub struct TabGroupManager;

impl TabGroupManager {
    pub fn create_group(chat_session_id: Option<String>, work_card_id: Option<String>) -> TabGroup {
        let id = format!("tg-{}", Uuid::new_v4());
        let group = TabGroup {
            id: id.clone(),
            chat_session_id,
            work_card_id,
            tabs: Vec::new(),
            control: BrowserControl::User,
        };
        let mut registry = REGISTRY.lock().expect("tab groups");
        registry.groups.insert(id.clone(), group.clone());
        registry.touch(&id);
        group
    }

    pub fn get_group(tab_group_id: &str) -> Option<TabGroup> {
        let mut registry = REGISTRY.lock().expect("tab groups");
        if tab_group_id == "current" {
            // Compatibility callers do not carry a group id yet. Resolve the
            // explicit last-touched group rather than HashMap iteration order.
            return registry.current();
        }
        let group = registry.groups.get(tab_group_id).cloned();
        if group.is_some() {
            registry.touch(tab_group_id);
        }
        group
    }

    pub fn ensure_group(tab_group_id: &str) -> TabGroup {
        if let Some(group) = Self::get_group(tab_group_id) {
            return group;
        }
        let group = TabGroup {
            id: tab_group_id.to_string(),
            chat_session_id: None,
            work_card_id: None,
            tabs: Vec::new(),
            control: BrowserControl::User,
        };
        let mut registry = REGISTRY.lock().expect("tab groups");
        registry
            .groups
            .insert(tab_group_id.to_string(), group.clone());
        registry.touch(tab_group_id);
        group
    }

    pub fn set_control(tab_group_id: &str, control: BrowserControl) -> Option<TabGroup> {
        let mut registry = REGISTRY.lock().expect("tab groups");
        let group = {
            let group = registry.groups.get_mut(tab_group_id)?;
            group.control = control;
            group.clone()
        };
        registry.touch(tab_group_id);
        Some(group)
    }

    pub fn link_work_card(tab_group_id: &str, work_card_id: Option<&str>) -> Option<TabGroup> {
        let mut registry = REGISTRY.lock().expect("tab groups");
        let group = {
            let group = registry.groups.get_mut(tab_group_id)?;
            group.work_card_id = work_card_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            group.clone()
        };
        registry.touch(tab_group_id);
        Some(group)
    }

    pub fn open_tab(
        tab_group_id: &str,
        url: &str,
        title: Option<&str>,
        opened_by: TabOpenedBy,
    ) -> Option<BrowserTab> {
        let mut registry = REGISTRY.lock().expect("tab groups");
        let tab = {
            let group = registry.groups.get_mut(tab_group_id)?;
            for tab in &mut group.tabs {
                tab.active = false;
            }
            let tab = BrowserTab {
                id: format!("tab-{}", Uuid::new_v4()),
                url: url.trim().to_string(),
                title: title
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| tab_label_from_url(url)),
                favicon: None,
                opened_by,
                active: true,
            };
            group.tabs.push(tab.clone());
            tab
        };
        registry.touch(tab_group_id);
        Some(tab)
    }

    pub fn navigate_active_tab(
        tab_group_id: &str,
        url: &str,
        title: Option<&str>,
        opened_by: TabOpenedBy,
    ) -> Option<BrowserTab> {
        let mut registry = REGISTRY.lock().expect("tab groups");
        let active = {
            let group = registry.groups.get_mut(tab_group_id)?;
            let active_idx = group.tabs.iter().position(|tab| tab.active);
            active_idx.map(|idx| {
                let tab = &mut group.tabs[idx];
                tab.url = url.trim().to_string();
                tab.title = title
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| tab_label_from_url(&tab.url));
                tab.opened_by = opened_by;
                tab.clone()
            })
        };
        if let Some(tab) = active {
            registry.touch(tab_group_id);
            return Some(tab);
        }
        drop(registry);
        Self::open_tab(tab_group_id, url, title, opened_by)
    }

    pub fn activate_tab(tab_group_id: &str, tab_id: &str) -> Option<TabGroup> {
        let mut registry = REGISTRY.lock().expect("tab groups");
        let group = {
            let group = registry.groups.get_mut(tab_group_id)?;
            let mut found = false;
            for tab in &mut group.tabs {
                let active = tab.id == tab_id;
                tab.active = active;
                if active {
                    found = true;
                }
            }
            found.then(|| group.clone())
        }?;
        registry.touch(tab_group_id);
        Some(group)
    }

    pub fn close_tab(tab_group_id: &str, tab_id: &str) -> Option<TabGroup> {
        let mut registry = REGISTRY.lock().expect("tab groups");
        let group = {
            let group = registry.groups.get_mut(tab_group_id)?;
            let was_active = group
                .tabs
                .iter()
                .find(|tab| tab.id == tab_id)
                .is_some_and(|tab| tab.active);
            group.tabs.retain(|tab| tab.id != tab_id);
            if was_active && !group.tabs.is_empty() {
                if let Some(last) = group.tabs.last_mut() {
                    last.active = true;
                }
            }
            group.clone()
        };
        registry.observations.remove(tab_id);
        registry.touch(tab_group_id);
        Some(group)
    }

    pub fn record_observation(
        tab_group_id: &str,
        capture: BrowserObservationCapture,
        since_revision: Option<u64>,
        max_nodes: usize,
    ) -> Result<BrowserObservation, String> {
        let mut registry = REGISTRY.lock().expect("tab groups");
        let group = registry
            .groups
            .get(tab_group_id)
            .ok_or_else(|| format!("tab group not found: {tab_group_id}"))?;
        let active_tab_id = group
            .tabs
            .iter()
            .find(|tab| tab.active)
            .map(|tab| tab.id.clone())
            .ok_or_else(|| "no active tab".to_string())?;
        if active_tab_id != capture.tab_id {
            return Err("browser observation is not bound to the active tab".to_string());
        }

        match registry.observations.get_mut(&capture.tab_id) {
            Some(record) => record.ingest(capture)?,
            None => {
                registry
                    .observations
                    .insert(capture.tab_id.clone(), ObservationRecord::new(capture)?);
            }
        }
        registry.touch(tab_group_id);
        registry
            .observations
            .get(&active_tab_id)
            .map(|record| record.project(since_revision, max_nodes))
            .ok_or_else(|| "browser observation mirror is unavailable".to_string())
    }

    pub fn observation_state(tab_group_id: &str, tab_id: &str) -> Option<BrowserObservationState> {
        let registry = REGISTRY.lock().expect("tab groups");
        let group = registry.groups.get(tab_group_id)?;
        if !group.tabs.iter().any(|tab| tab.active && tab.id == tab_id) {
            return None;
        }
        registry
            .observations
            .get(tab_id)
            .map(ObservationRecord::state)
    }

    pub fn observation_node(
        tab_group_id: &str,
        tab_id: &str,
        document_id: &str,
        revision: u64,
        element_ref: &str,
    ) -> Option<BrowserSemanticNode> {
        let registry = REGISTRY.lock().expect("tab groups");
        let group = registry.groups.get(tab_group_id)?;
        if !group.tabs.iter().any(|tab| tab.active && tab.id == tab_id) {
            return None;
        }
        let record = registry.observations.get(tab_id)?;
        if record.revision != revision || record.current.document_id != document_id {
            return None;
        }
        record
            .current
            .nodes
            .iter()
            .find(|node| node.element_ref == element_ref)
            .cloned()
    }

    /// Return the complete current mirror for a revision-bound pixel capture.
    /// Unlike `record_observation`, this never returns a delta.
    pub fn current_observation(tab_group_id: &str, tab_id: &str) -> Option<BrowserObservation> {
        let registry = REGISTRY.lock().expect("tab groups");
        let group = registry.groups.get(tab_group_id)?;
        if !group.tabs.iter().any(|tab| tab.active && tab.id == tab_id) {
            return None;
        }
        registry
            .observations
            .get(tab_id)
            .map(|record| record.full_projection(MAX_OBSERVATION_NODES))
    }

    pub fn snapshot_active_tab(
        tab_group_id: &str,
        max_chars: usize,
    ) -> Result<BrowserSnapshot, String> {
        let group = Self::get_group(tab_group_id)
            .ok_or_else(|| format!("tab group not found: {tab_group_id}"))?;
        let tab = group
            .tabs
            .iter()
            .find(|tab| tab.active)
            .ok_or_else(|| "no active tab".to_string())?;
        let fetched = fetch_url_markdown(&tab.url, max_chars)?;
        Ok(BrowserSnapshot {
            tab_id: tab.id.clone(),
            url: fetched.url,
            title: if fetched.title.is_empty() {
                tab.title.clone()
            } else {
                fetched.title
            },
            markdown: fetched.markdown,
            links: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic_node(element_ref: &str, name: &str) -> BrowserSemanticNode {
        BrowserSemanticNode {
            element_ref: element_ref.to_string(),
            parent_ref: None,
            role: "button".to_string(),
            name: name.to_string(),
            tag: "button".to_string(),
            value: None,
            href: None,
            disabled: false,
            checked: None,
            selected: None,
            bounds: None,
            sensitive: false,
        }
    }

    fn observation_capture(
        document_id: &str,
        nodes: Vec<BrowserSemanticNode>,
    ) -> BrowserObservationCapture {
        BrowserObservationCapture {
            tab_id: "tab-one".to_string(),
            url: "https://example.test/".to_string(),
            title: "Example".to_string(),
            document_id: document_id.to_string(),
            viewport: crate::model::BrowserObservationViewport {
                width: 1280,
                height: 720,
                scroll_x: 0,
                scroll_y: 0,
                device_scale_factor: 2.0,
            },
            nodes,
            truncated: false,
            unchanged: false,
            captured_at_ms: 10,
        }
    }

    fn group(id: &str) -> TabGroup {
        TabGroup {
            id: id.to_string(),
            chat_session_id: None,
            work_card_id: None,
            tabs: Vec::new(),
            control: BrowserControl::User,
        }
    }

    #[test]
    fn current_group_uses_explicit_last_touched_identity() {
        let mut registry = TabGroupRegistry::default();
        registry.groups.insert("one".to_string(), group("one"));
        registry.groups.insert("two".to_string(), group("two"));

        assert!(registry.current().is_none());
        registry.touch("one");
        assert_eq!(registry.current().map(|group| group.id), Some("one".into()));
        registry.touch("two");
        assert_eq!(registry.current().map(|group| group.id), Some("two".into()));
        registry.touch("missing");
        assert_eq!(registry.current().map(|group| group.id), Some("two".into()));
    }

    #[test]
    fn semantic_observation_returns_bounded_delta_after_known_revision() {
        let mut record = ObservationRecord::new(observation_capture(
            "doc-one",
            vec![semantic_node("ref-one", "Before")],
        ))
        .expect("initial observation");

        record
            .ingest(observation_capture(
                "doc-one",
                vec![
                    semantic_node("ref-one", "After"),
                    semantic_node("ref-two", "New"),
                ],
            ))
            .expect("updated observation");
        let delta = record.project(Some(1), 16);

        assert!(!delta.full);
        assert_eq!(delta.base_revision, Some(1));
        assert_eq!(delta.revision, 2);
        assert_eq!(delta.nodes.len(), 2);
        assert!(delta.removed_refs.is_empty());
        assert!(delta.untrusted_content);
    }

    #[test]
    fn document_replacement_invalidates_old_refs_with_a_full_projection() {
        let mut record = ObservationRecord::new(observation_capture(
            "doc-one",
            vec![semantic_node("old-ref", "Old")],
        ))
        .expect("initial observation");
        record
            .ingest(observation_capture(
                "doc-two",
                vec![semantic_node("new-ref", "New")],
            ))
            .expect("replacement observation");

        let projection = record.project(Some(1), 16);
        assert!(projection.full);
        assert_eq!(projection.document_id, "doc-two");
        assert_eq!(projection.revision, 2);
        assert_eq!(projection.nodes[0].element_ref, "new-ref");
    }

    #[test]
    fn unchanged_marker_reuses_mirror_without_advancing_revision() {
        let mut record = ObservationRecord::new(observation_capture(
            "doc-one",
            vec![semantic_node("ref-one", "Stable")],
        ))
        .expect("initial observation");
        let mut marker = observation_capture("doc-one", Vec::new());
        marker.unchanged = true;
        marker.captured_at_ms = 20;
        record.ingest(marker).expect("unchanged marker");

        let projection = record.project(Some(1), 16);
        assert_eq!(projection.revision, 1);
        assert!(!projection.full);
        assert!(projection.nodes.is_empty());
        assert_eq!(projection.captured_at_ms, 20);
    }

    #[test]
    fn semantic_observation_rejects_unbounded_or_sensitive_values() {
        let mut node = semantic_node("ref-one", "Safe");
        node.sensitive = true;
        node.value = Some("must-not-cross-the-boundary".to_string());
        let error = ObservationRecord::new(observation_capture("doc-one", vec![node]))
            .expect_err("sensitive value must fail");
        assert!(error.contains("cannot expose values"));

        let mut node = semantic_node("ref-two", "Safe");
        node.element_ref = "x".repeat(MAX_OBSERVATION_ID_BYTES + 1);
        let error = ObservationRecord::new(observation_capture("doc-one", vec![node]))
            .expect_err("oversized ref must fail");
        assert!(error.contains("element ref exceeds"));
    }
}
