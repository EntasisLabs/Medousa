//! Runtime-neutral Forge wire types.
//!
//! These records cross work-environment and federation boundaries, so their
//! serde contract must remain available to embedded clients without linking
//! Forge's host filesystem and process implementation.

use serde::{Deserialize, Serialize};

/// Exact native review coordinates. Copy these from work.project_review;
/// approval never substitutes the current checkout for the reviewed revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectReviewPin {
    pub attempt_id: String,
    pub environment_generation: u32,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub baseline_oid: String,
    pub reviewed_head_oid: String,
    pub expected_base_oid: String,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectReviewQuery {
    pub work_id: String,
    /// Select an exact sealed candidate; omission selects the latest sealed attempt.
    #[serde(default)]
    pub attempt_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectPrepareMergeInput {
    pub work_id: String,
    /// Only acknowledge capture risks when explicitly authorized by the user.
    #[serde(default)]
    pub ack_risks: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectReviewFileQuery {
    pub work_id: String,
    pub reviewed: ProjectReviewPin,
    /// Exact changed path returned by the sealed review.
    pub path: String,
    /// Bounded diff output, up to 512 KiB; omission uses 64 KiB.
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectIntegrationStrategy {
    FastForwardOnly,
    KeepCheckout,
    PreserveBranch,
    ExportPatch,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectApproveInput {
    pub work_id: String,
    pub reviewed: ProjectReviewPin,
    /// Select explicitly from the review's allowed strategies. Approval alone
    /// does not merge or close the undertaking.
    pub strategy: ProjectIntegrationStrategy,
    pub rationale: String,
    #[serde(default)]
    pub acknowledged_violations: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectApplyInput {
    pub work_id: String,
    /// Exact native decision returned by work.approve_project.
    pub decision_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectRequestChangesInput {
    pub work_id: String,
    pub reviewed: ProjectReviewPin,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProjectDiscardInput {
    pub work_id: String,
}

/// Governance over paths an executor may touch, and rules for checkpoint
/// capture. Violations are evidence — they never prove containment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkPolicy {
    /// Normalized git-path globs the executor is allowed to modify. Empty =
    /// everything allowed (violations still computed for report completeness).
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// Normalized git-path globs that are always violations.
    #[serde(default)]
    pub denied_paths: Vec<String>,
    /// Additional protected refs beyond the always-protected base ref.
    #[serde(default)]
    pub protected_refs: Vec<String>,
    /// Capture all non-ignored changes into the checkpoint commit.
    #[serde(default = "default_true")]
    pub checkpoint_capture_all: bool,
    /// Maximum bytes for any single captured file (0 = unlimited).
    #[serde(default)]
    pub checkpoint_max_file_bytes: u64,
    /// Maximum total bytes captured (0 = unlimited).
    #[serde(default)]
    pub checkpoint_max_total_bytes: u64,
    /// Path classes excluded from checkpoint capture (normalized git-path globs).
    #[serde(default)]
    pub checkpoint_exclude_paths: Vec<String>,
    /// Include ignored/untracked files in checkpoints (usually false).
    #[serde(default)]
    pub checkpoint_include_ignored: bool,
    /// Scan captured content for likely secrets before committing.
    #[serde(default = "default_true")]
    pub checkpoint_secret_scan: bool,
    /// Risky checkpoints require explicit acknowledgment instead of being
    /// blocked outright.
    #[serde(default)]
    pub checkpoint_allow_risky_with_ack: bool,
}

fn default_true() -> bool {
    true
}

impl Default for WorkPolicy {
    fn default() -> Self {
        Self {
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            protected_refs: Vec::new(),
            checkpoint_capture_all: true,
            checkpoint_max_file_bytes: 0,
            checkpoint_max_total_bytes: 0,
            checkpoint_exclude_paths: Vec::new(),
            checkpoint_include_ignored: false,
            checkpoint_secret_scan: true,
            checkpoint_allow_risky_with_ack: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    /// Normalized git path.
    pub path: String,
    pub status: ChangeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    #[serde(default)]
    pub is_binary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    /// Unmerged / conflicted path (`git status` porcelain `u`).
    Unmerged,
}

#[cfg(test)]
mod lifecycle_contract_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn approval_requires_exact_coordinates_and_rejects_authority_overrides() {
        let reviewed = json!({"attempt_id":"attempt-1", "environment_generation":1,
            "evidence_id":"evidence-1", "evidence_digest":"a".repeat(64),
            "baseline_oid":"b".repeat(40), "reviewed_head_oid":"c".repeat(40),
            "expected_base_oid":"b".repeat(40)});
        let input = json!({"work_id":"work-1", "reviewed":reviewed,
            "strategy":"fast_forward_only", "rationale":"Reviewed the sealed changes"});
        let parsed: ProjectApproveInput = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(parsed.strategy, ProjectIntegrationStrategy::FastForwardOnly);
        assert!(parsed.acknowledged_violations.is_empty());
        for key in ["reviewed", "strategy", "rationale"] {
            let mut missing = input.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<ProjectApproveInput>(missing).is_err());
        }
        for key in ["owner", "actor", "authorized", "execution_grant"] {
            let mut forged = input.clone();
            forged[key] = json!("other");
            assert!(serde_json::from_value::<ProjectApproveInput>(forged).is_err());
            let mut forged = input.clone();
            forged["reviewed"][key] = json!("other");
            assert!(serde_json::from_value::<ProjectApproveInput>(forged).is_err());
        }
        for key in [
            "attempt_id",
            "environment_generation",
            "evidence_id",
            "evidence_digest",
            "baseline_oid",
            "reviewed_head_oid",
            "expected_base_oid",
        ] {
            let mut missing = input.clone();
            missing["reviewed"].as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<ProjectApproveInput>(missing).is_err());
        }
        assert!(serde_json::from_value::<ProjectApplyInput>(json!({"work_id":"work-1"})).is_err());
        assert!(
            serde_json::from_value::<ProjectIntegrationStrategy>(json!("merge_whatever")).is_err()
        );
    }
}
