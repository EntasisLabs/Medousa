//! Manuscript overlay proposals — operator-approved working notes (Phase 8E.3).

use std::fs;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};

use crate::session;

pub const COGNITION_MANUSCRIPT_OVERLAY_PROPOSE: &str = "cognition_manuscript_overlay_propose";
pub const COGNITION_MANUSCRIPT_OVERLAY_LIST: &str = "cognition_manuscript_overlay_list";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManuscriptOverlayProposal {
    pub proposal_id: String,
    pub manuscript_id: String,
    pub appendix: String,
    pub reason: String,
    pub status: String,
    pub proposed_at_utc: chrono::DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

pub fn register_manuscript_overlay_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> StasisResult<()> {
    registry.register_tool(CognitionManuscriptOverlayProposeTool)?;
    registry.register_tool(CognitionManuscriptOverlayListTool)?;
    Ok(())
}

fn overlay_root() -> PathBuf {
    session::medousa_data_dir().join("manuscript-overlays")
}

fn pending_dir() -> PathBuf {
    overlay_root().join("pending")
}

fn proposal_path(proposal_id: &str) -> PathBuf {
    pending_dir().join(format!("{proposal_id}.yaml"))
}

fn slug_token(raw: &str) -> String {
    raw.to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn list_pending_proposals(limit: usize) -> Result<Vec<ManuscriptOverlayProposal>, String> {
    fs::create_dir_all(pending_dir()).map_err(|err| err.to_string())?;
    let mut proposals = Vec::new();
    for entry in fs::read_dir(pending_dir()).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            continue;
        }
        let raw = fs::read_to_string(entry.path()).map_err(|err| err.to_string())?;
        if let Ok(proposal) = serde_yaml::from_str::<ManuscriptOverlayProposal>(&raw) {
            proposals.push(proposal);
        }
    }
    proposals.sort_by(|a, b| b.proposed_at_utc.cmp(&a.proposed_at_utc));
    proposals.truncate(limit.clamp(1, 100));
    Ok(proposals)
}

pub fn propose_overlay(
    manuscript_id: &str,
    appendix: &str,
    reason: &str,
    session_id: Option<String>,
) -> Result<ManuscriptOverlayProposal, String> {
    let manuscript_id = manuscript_id.trim();
    let appendix = appendix.trim();
    let reason = reason.trim();
    if manuscript_id.is_empty() {
        return Err("manuscript_id is required".to_string());
    }
    if appendix.is_empty() {
        return Err("appendix is required".to_string());
    }
    if reason.is_empty() {
        return Err("reason is required".to_string());
    }

    fs::create_dir_all(pending_dir()).map_err(|err| err.to_string())?;
    let stamp = Utc::now().format("%Y%m%d%H%M%S");
    let proposal_id = format!("{}-{}-{}", slug_token(manuscript_id), stamp, &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let proposal = ManuscriptOverlayProposal {
        proposal_id: proposal_id.clone(),
        manuscript_id: manuscript_id.to_string(),
        appendix: appendix.to_string(),
        reason: reason.to_string(),
        status: "pending".to_string(),
        proposed_at_utc: Utc::now(),
        session_id: session_id.filter(|value| !value.trim().is_empty()),
    };
    let yaml = serde_yaml::to_string(&proposal).map_err(|err| err.to_string())?;
    fs::write(proposal_path(&proposal_id), yaml).map_err(|err| err.to_string())?;
    Ok(proposal)
}

pub struct CognitionManuscriptOverlayProposeTool;

#[async_trait]
impl StasisTool for CognitionManuscriptOverlayProposeTool {
    fn name(&self) -> &'static str {
        COGNITION_MANUSCRIPT_OVERLAY_PROPOSE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Propose a session-scoped manuscript overlay appendix for operator approval — never mutates kernel STTP. \
             Writes a pending YAML under ~/.local/share/medousa/manuscript-overlays/pending/. \
             Operator approves by promoting the file to user manuscripts (manual for now).",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["manuscript_id", "appendix", "reason"],
            "properties": {
                "manuscript_id": { "type": "string", "description": "Target manuscript id e.g. base-researcher" },
                "appendix": { "type": "string", "description": "Markdown/YAML appendix to merge at spawn when approved" },
                "reason": { "type": "string", "description": "Why this overlay helps future turns" },
                "session_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let manuscript_id = input
            .get("manuscript_id")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("manuscript_id is required".to_string()))?;
        let appendix = input
            .get("appendix")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("appendix is required".to_string()))?;
        let reason = input
            .get("reason")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("reason is required".to_string()))?;
        let session_id = input
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string);

        let proposal = propose_overlay(manuscript_id, appendix, reason, session_id)
            .map_err(StasisError::PortFailure)?;

        Ok(json!({
            "ok": true,
            "proposal_id": proposal.proposal_id,
            "manuscript_id": proposal.manuscript_id,
            "status": proposal.status,
            "path": proposal_path(&proposal.proposal_id).display().to_string(),
            "message": "Overlay proposal queued for operator approval — does not affect live turns until promoted.",
        }))
    }
}

pub struct CognitionManuscriptOverlayListTool;

#[async_trait]
impl StasisTool for CognitionManuscriptOverlayListTool {
    fn name(&self) -> &'static str {
        COGNITION_MANUSCRIPT_OVERLAY_LIST
    }

    fn description(&self) -> Option<&'static str> {
        Some("List pending manuscript overlay proposals awaiting operator approval.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(20);
        let proposals = list_pending_proposals(limit).map_err(StasisError::PortFailure)?;
        Ok(json!({
            "ok": true,
            "count": proposals.len(),
            "pending_dir": pending_dir().display().to_string(),
            "proposals": proposals,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propose_and_list_overlay() {
        let _guard = overlay_test_lock();
        let manuscript_id = format!("test-ms-{}", uuid::Uuid::new_v4().simple());
        let proposal = propose_overlay(
            &manuscript_id,
            "Prefer cognition_capability_invoke for web_research.",
            "Repeated discovery on follow-ups",
            Some("sess-test".to_string()),
        )
        .expect("propose");

        let listed = list_pending_proposals(20).expect("list");
        assert!(
            listed
                .iter()
                .any(|entry| entry.proposal_id == proposal.proposal_id)
        );

        let _ = fs::remove_file(proposal_path(&proposal.proposal_id));
    }

    fn overlay_test_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("overlay test lock")
    }
}
