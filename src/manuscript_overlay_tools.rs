//! Manuscript overlay proposals — operator-approved working notes (Phase 8E.3).

use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::domain::errors::{Result as StasisResult, StasisError};

use crate::session;
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_MANUSCRIPT_OVERLAY_PROPOSE: &str = "cognition_manuscript_overlay_propose";
pub const COGNITION_MANUSCRIPT_OVERLAY_LIST: &str = "cognition_manuscript_overlay_list";

const COGNITION_MANUSCRIPT_OVERLAY_PROPOSE_ID: ToolId =
    ToolId::new(COGNITION_MANUSCRIPT_OVERLAY_PROPOSE);
const COGNITION_MANUSCRIPT_OVERLAY_LIST_ID: ToolId = ToolId::new(COGNITION_MANUSCRIPT_OVERLAY_LIST);

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ManuscriptOverlayProposal {
    pub proposal_id: String,
    pub manuscript_id: String,
    pub appendix: String,
    pub reason: String,
    pub status: String,
    #[schemars(with = "String")]
    pub proposed_at_utc: chrono::DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

pub fn register_manuscript_overlay_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionManuscriptOverlayProposeTool)?;
    registry.register_typed_tool(CognitionManuscriptOverlayListTool)?;
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
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
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
    proposals.sort_by_key(|b| std::cmp::Reverse(b.proposed_at_utc));
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
    let proposal_id = format!(
        "{}-{}-{}",
        slug_token(manuscript_id),
        stamp,
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManuscriptOverlayProposeInput {
    /// Target manuscript id e.g. base-researcher
    pub manuscript_id: String,
    /// Markdown/YAML appendix to merge at spawn when approved
    pub appendix: String,
    /// Why this overlay helps future turns
    pub reason: String,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ManuscriptOverlayProposeOutput {
    pub ok: bool,
    pub proposal_id: String,
    pub manuscript_id: String,
    pub status: String,
    pub path: String,
    pub message: String,
}

#[medousa_tool(id = COGNITION_MANUSCRIPT_OVERLAY_PROPOSE_ID)]
impl CognitionManuscriptOverlayProposeTool {
    /// Propose a session-scoped manuscript overlay appendix for operator approval — never mutates kernel STTP. Writes a pending YAML under the Medousa data dir at manuscript-overlays/pending/. Operator approves by promoting the file to user manuscripts (manual for now).
    async fn invoke_typed(
        &self,
        input: ManuscriptOverlayProposeInput,
    ) -> stasis::prelude::Result<ManuscriptOverlayProposeOutput> {
        let manuscript_id = input.manuscript_id.as_str();
        let appendix = input.appendix.as_str();
        let reason = input.reason.as_str();

        let proposal = propose_overlay(manuscript_id, appendix, reason, input.session_id)
            .map_err(StasisError::PortFailure)?;

        Ok(ManuscriptOverlayProposeOutput {
            ok: true,
            path: proposal_path(&proposal.proposal_id).display().to_string(),
            proposal_id: proposal.proposal_id,
            manuscript_id: proposal.manuscript_id,
            status: proposal.status,
            message: "Overlay proposal queued for operator approval — does not affect live turns until promoted."
                .to_string(),
        })
    }
}

pub struct CognitionManuscriptOverlayListTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManuscriptOverlayListInput {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 100),
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ManuscriptOverlayListOutput {
    pub ok: bool,
    pub count: usize,
    pub pending_dir: String,
    pub proposals: Vec<ManuscriptOverlayProposal>,
}

#[medousa_tool(id = COGNITION_MANUSCRIPT_OVERLAY_LIST_ID)]
impl CognitionManuscriptOverlayListTool {
    /// List pending manuscript overlay proposals awaiting operator approval.
    async fn invoke_typed(
        &self,
        input: ManuscriptOverlayListInput,
    ) -> stasis::prelude::Result<ManuscriptOverlayListOutput> {
        let limit = input.limit.unwrap_or(20);
        let proposals = list_pending_proposals(limit).map_err(StasisError::PortFailure)?;
        Ok(ManuscriptOverlayListOutput {
            ok: true,
            count: proposals.len(),
            pending_dir: pending_dir().display().to_string(),
            proposals,
        })
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
