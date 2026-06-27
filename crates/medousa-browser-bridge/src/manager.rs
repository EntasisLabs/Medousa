use std::collections::HashMap;
use std::sync::Mutex;

use medousa_browser_lite::fetch_url_markdown;
use once_cell::sync::Lazy;
use uuid::Uuid;

use crate::model::{
    BrowserControl, BrowserSnapshot, BrowserTab, TabGroup, TabOpenedBy,
};

static GROUPS: Lazy<Mutex<HashMap<String, TabGroup>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn tab_label_from_url(url: &str) -> String {
    let trimmed = url.trim();
    if let Some(rest) = trimmed.strip_prefix("https://").or_else(|| trimmed.strip_prefix("http://")) {
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
        GROUPS
            .lock()
            .expect("tab groups")
            .insert(id.clone(), group.clone());
        group
    }

    pub fn get_group(tab_group_id: &str) -> Option<TabGroup> {
        GROUPS
            .lock()
            .expect("tab groups")
            .get(tab_group_id)
            .cloned()
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
        GROUPS
            .lock()
            .expect("tab groups")
            .insert(tab_group_id.to_string(), group.clone());
        group
    }

    pub fn set_control(tab_group_id: &str, control: BrowserControl) -> Option<TabGroup> {
        let mut guard = GROUPS.lock().expect("tab groups");
        let group = guard.get_mut(tab_group_id)?;
        group.control = control;
        Some(group.clone())
    }

    pub fn link_work_card(tab_group_id: &str, work_card_id: Option<&str>) -> Option<TabGroup> {
        let mut guard = GROUPS.lock().expect("tab groups");
        let group = guard.get_mut(tab_group_id)?;
        group.work_card_id = work_card_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        Some(group.clone())
    }

    pub fn open_tab(
        tab_group_id: &str,
        url: &str,
        title: Option<&str>,
        opened_by: TabOpenedBy,
    ) -> Option<BrowserTab> {
        let mut guard = GROUPS.lock().expect("tab groups");
        let group = guard.get_mut(tab_group_id)?;
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
        Some(tab)
    }

    pub fn navigate_active_tab(
        tab_group_id: &str,
        url: &str,
        title: Option<&str>,
        opened_by: TabOpenedBy,
    ) -> Option<BrowserTab> {
        let mut guard = GROUPS.lock().expect("tab groups");
        let group = guard.get_mut(tab_group_id)?;
        let active_idx = group.tabs.iter().position(|tab| tab.active);
        if let Some(idx) = active_idx {
            let tab = &mut group.tabs[idx];
            tab.url = url.trim().to_string();
            tab.title = title
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| tab_label_from_url(&tab.url));
            tab.opened_by = opened_by;
            return Some(tab.clone());
        }
        drop(guard);
        Self::open_tab(tab_group_id, url, title, opened_by)
    }

    pub fn activate_tab(tab_group_id: &str, tab_id: &str) -> Option<TabGroup> {
        let mut guard = GROUPS.lock().expect("tab groups");
        let group = guard.get_mut(tab_group_id)?;
        let mut found = false;
        for tab in &mut group.tabs {
            let active = tab.id == tab_id;
            tab.active = active;
            if active {
                found = true;
            }
        }
        if !found {
            return None;
        }
        Some(group.clone())
    }

    pub fn close_tab(tab_group_id: &str, tab_id: &str) -> Option<TabGroup> {
        let mut guard = GROUPS.lock().expect("tab groups");
        let group = guard.get_mut(tab_group_id)?;
        let was_active = group
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .is_some_and(|tab| tab.active);
        group.tabs.retain(|tab| tab.id != tab_id);
        if group.tabs.is_empty() {
            return Some(group.clone());
        }
        if was_active {
            if let Some(last) = group.tabs.last_mut() {
                last.active = true;
            }
        }
        Some(group.clone())
    }

    pub fn snapshot_active_tab(tab_group_id: &str, max_chars: usize) -> Result<BrowserSnapshot, String> {
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
