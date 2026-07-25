//! Multi-profile session index for Shared-mode rooms.
//!
//! Kept separate from [`crate::session_catalog`] (single-owner). List APIs merge
//! both stores for the caller's bound profile. Transcripts still live under one
//! `session_id` — this table is an index only.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::session::{atomic_write, medousa_data_dir, SessionHistorySummary};
use crate::session_catalog::{
    AUTO_TITLE_MAX_CHARS, PREVIEW_MAX_CHARS, SessionCatalogRow, SessionListPage,
    decode_list_cursor, encode_list_cursor, get_summary,
};
use crate::shared_mode::general_profile_id;

const SHARED_CATALOG_DIR: &str = "shared_catalog";

/// Which catalog a new chat is written into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SessionCatalogKind {
    #[default]
    Single,
    Shared,
}

impl SessionCatalogKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Shared => "shared",
        }
    }

    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("shared") => Self::Shared,
            _ => Self::Single,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SharedSessionCatalogRow {
    pub session_id: String,
    pub preview: String,
    pub turn_count: usize,
    pub last_activity_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Member seats that may see this room.
    #[serde(default)]
    pub member_profile_ids: Vec<String>,
    /// Agent persona / Locus tenant for the room (defaults to `user:general`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile_id: Option<String>,
}

impl SharedSessionCatalogRow {
    pub fn new(
        session_id: impl Into<String>,
        member_profile_ids: Vec<String>,
        agent_profile_id: Option<String>,
    ) -> Self {
        let mut members: Vec<String> = member_profile_ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect();
        members.sort();
        members.dedup();
        Self {
            session_id: session_id.into(),
            preview: String::new(),
            turn_count: 0,
            last_activity_at: Some(Utc::now()),
            display_name: None,
            member_profile_ids: members,
            agent_profile_id: agent_profile_id.or_else(|| Some(general_profile_id())),
        }
    }

    pub fn includes_member(&self, profile_id: &str) -> bool {
        let trimmed = profile_id.trim();
        if trimmed.is_empty() {
            return false;
        }
        self.member_profile_ids.iter().any(|id| id == trimmed)
    }

    fn to_history_summary(&self) -> SessionHistorySummary {
        SessionHistorySummary {
            session_id: self.session_id.clone(),
            display_name: self.display_name.clone(),
            turns: self.turn_count,
            verification_runs: 0,
            last_timestamp: self.last_activity_at,
            last_verification_timestamp: None,
            last_verification_confidence: None,
            last_verification_coverage: None,
            last_verification_verified: None,
            preview: self.preview.clone(),
            catalog: Some("shared".to_string()),
        }
    }
}

fn catalog_dir() -> PathBuf {
    medousa_data_dir().join(SHARED_CATALOG_DIR)
}

fn catalog_path(session_id: &str) -> PathBuf {
    catalog_dir().join(format!("{session_id}.json"))
}

pub fn upsert_shared_row(row: &SharedSessionCatalogRow) -> Result<()> {
    let dir = catalog_dir();
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let path = catalog_path(&row.session_id);
    let raw = serde_json::to_string_pretty(row).context("encode shared catalog row")?;
    atomic_write(&path, raw.as_bytes()).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn get_shared_row(session_id: &str) -> Option<SharedSessionCatalogRow> {
    let path = catalog_path(session_id.trim());
    if !path.is_file() {
        return None;
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

pub fn delete_shared_row(session_id: &str) {
    let path = catalog_path(session_id.trim());
    let _ = fs::remove_file(path);
}

pub fn create_shared_session(
    session_id: &str,
    member_profile_ids: Vec<String>,
    agent_profile_id: Option<String>,
    display_name: Option<String>,
) -> Result<SharedSessionCatalogRow> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        bail!("session_id must not be empty");
    }
    if get_shared_row(session_id).is_some() {
        bail!("shared session already exists: {session_id}");
    }
    if get_summary(session_id).is_some() {
        bail!("session_id already used in single catalog: {session_id}");
    }
    let mut row = SharedSessionCatalogRow::new(session_id, member_profile_ids, agent_profile_id);
    if row.member_profile_ids.is_empty() {
        bail!("shared session requires at least one member profile");
    }
    if let Some(name) = display_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        row.display_name = Some(name.chars().take(AUTO_TITLE_MAX_CHARS).collect());
    }
    upsert_shared_row(&row)?;
    Ok(row)
}

fn list_all_shared_rows() -> Vec<SharedSessionCatalogRow> {
    let dir = catalog_dir();
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut rows = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return rows;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(row) = serde_json::from_str::<SharedSessionCatalogRow>(&raw) {
                rows.push(row);
            }
        }
    }
    rows.sort_by(|a, b| {
        b.last_activity_at
            .cmp(&a.last_activity_at)
            .then_with(|| b.session_id.cmp(&a.session_id))
    });
    rows
}

fn row_matches_query(row: &SharedSessionCatalogRow, query: &str) -> bool {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return true;
    }
    row.session_id.to_ascii_lowercase().contains(&needle)
        || row.preview.to_ascii_lowercase().contains(&needle)
        || row
            .display_name
            .as_ref()
            .is_some_and(|name| name.to_ascii_lowercase().contains(&needle))
}

/// Shared rooms visible to `profile_id`.
pub fn list_shared_for_profile(
    profile_id: &str,
    limit: usize,
    query: Option<&str>,
) -> Vec<SharedSessionCatalogRow> {
    let limit = limit.max(1);
    list_all_shared_rows()
        .into_iter()
        .filter(|row| row.includes_member(profile_id))
        .filter(|row| query.is_none_or(|q| row_matches_query(row, q)))
        .take(limit)
        .collect()
}

/// Merge single-profile catalog + shared catalog for one seat.
pub fn list_merged_sessions_for_profile(
    profile_id: &str,
    limit: usize,
    query: Option<&str>,
    cursor: Option<&str>,
) -> SessionListPage {
    let limit = limit.max(1);
    let single = crate::session_catalog::list_sessions_page(
        limit.saturating_mul(2),
        query,
        cursor,
        Some(profile_id),
    );
    let shared = list_shared_for_profile(profile_id, limit.saturating_mul(2), query);

    let mut merged: Vec<(DateTime<Utc>, String, SessionHistorySummary)> = Vec::new();
    for session in single.sessions {
        let at = session
            .last_timestamp
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now));
        merged.push((at, session.session_id.clone(), session));
    }
    for row in shared {
        let summary = row.to_history_summary();
        let at = summary
            .last_timestamp
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now));
        // Prefer single-catalog row if the same id somehow exists in both.
        if merged.iter().any(|(_, id, _)| id == &summary.session_id) {
            continue;
        }
        merged.push((at, summary.session_id.clone(), summary));
    }

    if let Some(cursor_row) = cursor.and_then(decode_list_cursor) {
        merged.retain(|(at, id, _)| {
            *at < cursor_row.last_activity_at
                || (*at == cursor_row.last_activity_at && id.as_str() < cursor_row.session_id.as_str())
        });
    }

    merged.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let has_more = merged.len() > limit;
    let page: Vec<_> = merged.into_iter().take(limit).map(|(_, _, s)| s).collect();
    let next_cursor = if has_more {
        page.last().map(|session| {
            // Reuse single-catalog cursor encoding via a thin adapter row.
            let adapter = SessionCatalogRow {
                session_id: session.session_id.clone(),
                preview: session.preview.clone(),
                turn_count: session.turns,
                last_activity_at: session.last_timestamp,
                display_name: session.display_name.clone(),
                verification_run_count: 0,
                last_verification_at: None,
                last_verification_confidence: None,
                last_verification_coverage: None,
                last_verification_verified: None,
                profile_id: None,
            };
            encode_list_cursor(&adapter)
        })
    } else {
        None
    };

    SessionListPage {
        sessions: page,
        next_cursor,
    }
}

/// Touch preview/activity after a turn (shared rooms only).
pub fn touch_shared_session(
    session_id: &str,
    preview: Option<&str>,
    display_name: Option<&str>,
) -> Result<()> {
    let Some(mut row) = get_shared_row(session_id) else {
        bail!("shared session not found: {session_id}");
    };
    row.turn_count = row.turn_count.saturating_add(1);
    row.last_activity_at = Some(Utc::now());
    if let Some(preview) = preview.map(str::trim).filter(|value| !value.is_empty()) {
        row.preview = preview.chars().take(PREVIEW_MAX_CHARS).collect();
    }
    if let Some(name) = display_name.map(str::trim).filter(|value| !value.is_empty()) {
        row.display_name = Some(name.chars().take(AUTO_TITLE_MAX_CHARS).collect());
    }
    upsert_shared_row(&row)
}

pub fn set_shared_display_name(session_id: &str, display_name: &str) -> Result<()> {
    let Some(mut row) = get_shared_row(session_id) else {
        bail!("shared session not found: {session_id}");
    };
    let name = display_name.trim();
    if name.is_empty() {
        bail!("display_name must not be empty");
    }
    row.display_name = Some(name.chars().take(AUTO_TITLE_MAX_CHARS).collect());
    upsert_shared_row(&row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_kind_parse() {
        assert_eq!(SessionCatalogKind::parse(Some("shared")), SessionCatalogKind::Shared);
        assert_eq!(SessionCatalogKind::parse(None), SessionCatalogKind::Single);
    }

    #[test]
    fn membership_filter() {
        let row = SharedSessionCatalogRow::new(
            "room-1",
            vec!["user:alice".into(), "user:bob".into()],
            None,
        );
        assert!(row.includes_member("user:alice"));
        assert!(!row.includes_member("user:carol"));
        assert_eq!(row.agent_profile_id.as_deref(), Some("user:general"));
    }
}
