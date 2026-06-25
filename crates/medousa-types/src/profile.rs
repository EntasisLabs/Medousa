use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExportBundle {
    pub format_version: u32,
    pub profile_id: String,
    pub display_name: String,
    pub exported_at: DateTime<Utc>,
    pub identity: ProfileIdentityExport,
    pub locus: ProfileLocusExport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileIdentityExport {
    pub user_id: String,
    pub channel_id: String,
    pub identity_context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLocusExport {
    pub profile_slug: String,
    pub sessions: Vec<ProfileLocusSessionExport>,
    pub node_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLocusSessionExport {
    pub chat_session_id: String,
    pub scoped_session_id: String,
    pub nodes: Vec<ProfileLocusNodeExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLocusNodeExport {
    pub sync_key: String,
    pub raw: String,
}
