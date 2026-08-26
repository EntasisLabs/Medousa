//! Manuscript overlay proposals — operator-approved working notes (Phase 8E.3).

use std::path::PathBuf;

use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::domain::errors::{Result as StasisResult, StasisError};

use medousa_types::authority_id::{ManuscriptId, ManuscriptOverlayProposalId};

use crate::session;
use crate::store_root::{StoreEntryKind, StorePath, StoreRoot};
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

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

fn pending_store() -> Result<StoreRoot, String> {
    StoreRoot::open_or_create_nofollow(&pending_dir()).map_err(|error| error.to_string())
}

fn proposal_path(proposal_id: &str) -> Result<StorePath, String> {
    let proposal_id =
        ManuscriptOverlayProposalId::parse(proposal_id).map_err(|error| error.to_string())?;
    StorePath::parse(&format!("{}.yaml", proposal_id.storage_key().as_str()))
        .map_err(|error| error.to_string())
}

fn proposal_ambient_path(proposal_id: &str) -> Result<PathBuf, String> {
    Ok(pending_dir().join(proposal_path(proposal_id)?.file_name()))
}

fn legacy_proposal_path(proposal_id: &str) -> Result<StorePath, String> {
    let proposal_id =
        ManuscriptOverlayProposalId::parse(proposal_id).map_err(|error| error.to_string())?;
    StorePath::parse(&format!("{}.yaml", proposal_id.as_str())).map_err(|error| error.to_string())
}

pub fn list_pending_proposals(limit: usize) -> Result<Vec<ManuscriptOverlayProposal>, String> {
    let store = pending_store()?;
    let mut proposals = Vec::new();
    for entry in store.list_root().map_err(|error| error.to_string())? {
        if entry.kind != StoreEntryKind::File || !entry.path.file_name().ends_with(".yaml") {
            continue;
        }
        let raw = store
            .read_limited(&entry.path, 4 * 1024 * 1024)
            .map_err(|error| error.to_string())?;
        if let Ok(proposal) = serde_yaml::from_slice::<ManuscriptOverlayProposal>(&raw)
            && (proposal_path(&proposal.proposal_id).is_ok_and(|path| path == entry.path)
                || legacy_proposal_path(&proposal.proposal_id).is_ok_and(|path| path == entry.path))
        {
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
    let manuscript_id = ManuscriptId::parse(manuscript_id).map_err(|error| error.to_string())?;
    let appendix = appendix.trim();
    let reason = reason.trim();
    if appendix.is_empty() {
        return Err("appendix is required".to_string());
    }
    if reason.is_empty() {
        return Err("reason is required".to_string());
    }

    let proposal_id = format!("overlay-{}", uuid::Uuid::new_v4().simple());
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
    pending_store()?
        .atomic_write(&proposal_path(&proposal_id)?, yaml.as_bytes())
        .map_err(|error| error.to_string())?;
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
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub session_id: CompatOption<String>,
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
    /// Create a session-scoped manuscript overlay proposal for operator review.
    async fn invoke_typed(
        &self,
        input: ManuscriptOverlayProposeInput,
    ) -> stasis::prelude::Result<ManuscriptOverlayProposeOutput> {
        let manuscript_id = input.manuscript_id.as_str();
        let appendix = input.appendix.as_str();
        let reason = input.reason.as_str();

        let proposal = propose_overlay(
            manuscript_id,
            appendix,
            reason,
            input.session_id.into_option(),
        )
        .map_err(StasisError::PortFailure)?;

        Ok(ManuscriptOverlayProposeOutput {
            ok: true,
            path: proposal_ambient_path(&proposal.proposal_id)
                .map_err(StasisError::PortFailure)?
                .display()
                .to_string(),
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
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 100),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub limit: CompatOption<usize>,
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
        let limit = input.limit.into_option().unwrap_or(20);
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
            "Prefer cognition_capability for web_research.",
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

        let _ = pending_store().and_then(|store| {
            store
                .remove_file(&proposal_path(&proposal.proposal_id)?)
                .map_err(|error| error.to_string())
        });
    }

    #[test]
    fn overlay_wire_optionals_remain_lenient_for_legacy_values() {
        let proposal: ManuscriptOverlayProposeInput = serde_json::from_value(serde_json::json!({
            "manuscript_id": "base-researcher",
            "appendix": "notes",
            "reason": "useful",
            "session_id": 42,
        }))
        .expect("proposal input");
        assert!(proposal.session_id.into_option().is_none());

        let list: ManuscriptOverlayListInput = serde_json::from_value(serde_json::json!({
            "limit": "20",
        }))
        .expect("list input");
        assert!(list.limit.into_option().is_none());
    }

    fn overlay_test_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("overlay test lock")
    }
}
