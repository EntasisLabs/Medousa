use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabOpenedBy {
    Agent,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserControl {
    Agent,
    User,
    AwaitingOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTab {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub favicon: Option<String>,
    pub opened_by: TabOpenedBy,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: String,
    #[serde(default)]
    pub chat_session_id: Option<String>,
    #[serde(default)]
    pub work_card_id: Option<String>,
    pub tabs: Vec<BrowserTab>,
    pub control: BrowserControl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroupState {
    pub tab_group: TabGroup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSnapshot {
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub markdown: String,
    #[serde(default)]
    pub links: Vec<String>,
}
