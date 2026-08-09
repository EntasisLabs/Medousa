//! Cognition tools for skill discovery, policy-gated proposals, and sandbox probes (H6–H7).

use std::sync::Arc;

use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use crate::events::TuiEvent;
use crate::identity_manuscript::build_manuscript_context;
use crate::openshell_handoff::collect_openshell_doctor_report;
use crate::openshell_sandbox_run::{OPENSHELL_SANDBOX_RUN_JOB_TYPE, OpenshellSandboxRunPayload};
use crate::runtime_composition_ext::RuntimeCompositionExt;
use crate::runtime_job_spec::ToolJobSpec;
use crate::skill_execution::{
    SkillAdoptionProposal, SkillScriptEntry, SkillScriptRiskClass, SkillSecurityLevel,
    build_sandbox_payload_for_skill, discover_skill_for_manuscript, evaluate_skill_adoption,
    resolve_skill_assets_dir,
};
use crate::skill_import::resolve_skill_source;
use crate::turn_continuation::{ContinuationAwaitMode, TurnContinuationScope, wire_turn_child_job};
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_SKILL_DISCOVER: &str = "cognition_skill_discover";
pub const COGNITION_SKILL_PROPOSE: &str = "cognition_skill_propose";
pub const COGNITION_SKILL_PROBE: &str = "cognition_skill_probe";

const COGNITION_SKILL_DISCOVER_ID: ToolId = ToolId::new(COGNITION_SKILL_DISCOVER);
const COGNITION_SKILL_PROPOSE_ID: ToolId = ToolId::new(COGNITION_SKILL_PROPOSE);
const COGNITION_SKILL_PROBE_ID: ToolId = ToolId::new(COGNITION_SKILL_PROBE);

pub const SKILL_COGNITION_TOOLS: &[&str] = &[
    COGNITION_SKILL_DISCOVER,
    COGNITION_SKILL_PROPOSE,
    COGNITION_SKILL_PROBE,
];

pub fn is_skill_cognition_tool(name: &str) -> bool {
    name.starts_with("cognition_skill_")
}

pub fn register_skill_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionSkillDiscoverTool)?;
    registry.register_typed_tool(CognitionSkillProposeTool)?;
    registry.register_typed_tool(CognitionSkillProbeTool::new(runtime, event_tx, turn_scope))?;
    Ok(())
}

pub struct CognitionSkillDiscoverTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillDiscoverInput {
    /// Imported skill manuscript id (preferred)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub manuscript_id: Option<String>,
    /// Raw skill directory or SKILL.md path before import
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub skill_path: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SkillDiscoverOutput {
    pub skill_id: String,
    pub assets_dir: String,
    pub has_scripts: bool,
    pub max_risk_class: SkillScriptRiskClass,
    pub max_risk_score: u8,
    pub scripts: Vec<SkillScriptEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_hint: Option<String>,
}

#[medousa_tool(id = COGNITION_SKILL_DISCOVER_ID)]
impl CognitionSkillDiscoverTool {
    /// Discover runnable scripts in an imported skill manuscript or raw skill directory. Returns risk classes for on-the-fly skill learning (observe before execute).
    async fn invoke_typed(
        &self,
        input: SkillDiscoverInput,
    ) -> stasis::prelude::Result<SkillDiscoverOutput> {
        if let Some(id) = input
            .manuscript_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let report = discover_skill_for_manuscript(id)
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            return Ok(SkillDiscoverOutput {
                skill_id: report.skill_id,
                assets_dir: report.assets_dir,
                has_scripts: report.has_scripts,
                max_risk_class: report.max_risk_class,
                max_risk_score: report.max_risk_score,
                scripts: report.scripts,
                import_hint: None,
            });
        }

        let skill_path = input
            .skill_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_skill_discover: manuscript_id or skill_path is required".to_string(),
                )
            })?;
        let source = resolve_skill_source(std::path::Path::new(skill_path))
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let scripts = crate::skill_execution::discover_skill_scripts(&source)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let (max_risk_class, max_risk_score) = scripts.iter().fold(
            (crate::skill_execution::SkillScriptRiskClass::ReadOnly, 0u8),
            |(class, score), script| {
                if script.risk_score >= score {
                    (script.risk_class, script.risk_score)
                } else {
                    (class, score)
                }
            },
        );
        Ok(SkillDiscoverOutput {
            skill_id: source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("skill")
                .to_string(),
            assets_dir: source.display().to_string(),
            has_scripts: !scripts.is_empty(),
            max_risk_class,
            max_risk_score,
            scripts,
            import_hint: Some(
                "Run medousa skill-import to adopt this skill as a manuscript specialty."
                    .to_string(),
            ),
        })
    }
}

pub struct CognitionSkillProposeTool;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillSecurityLevelInput {
    #[serde(alias = "inspect", alias = "read")]
    Observe,
    #[serde(alias = "proposal")]
    Propose,
    #[serde(alias = "run", alias = "execute")]
    Sandbox,
    #[serde(alias = "block")]
    Deny,
}

impl From<SkillSecurityLevelInput> for SkillSecurityLevel {
    fn from(value: SkillSecurityLevelInput) -> Self {
        match value {
            SkillSecurityLevelInput::Observe => Self::Observe,
            SkillSecurityLevelInput::Propose => Self::Propose,
            SkillSecurityLevelInput::Sandbox => Self::Sandbox,
            SkillSecurityLevelInput::Deny => Self::Deny,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillProposeInput {
    pub manuscript_id: String,
    pub security_level: SkillSecurityLevelInput,
    /// Relative script path (e.g. scripts/run.sh) for sandbox evaluation
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SkillProposalNextTools {
    pub observe: Vec<String>,
    pub propose: Vec<String>,
    pub sandbox: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SkillProposeOutput {
    pub skill_id: String,
    pub requested_level: SkillSecurityLevel,
    pub granted_level: SkillSecurityLevel,
    pub requires_approval: bool,
    pub approval_reasons: Vec<String>,
    pub policy_template: Option<String>,
    pub sandbox_from: Option<String>,
    pub script: Option<String>,
    pub rationale: String,
    pub next_tools: SkillProposalNextTools,
}

impl SkillProposeOutput {
    fn from_proposal(proposal: SkillAdoptionProposal) -> Self {
        Self {
            skill_id: proposal.skill_id,
            requested_level: proposal.requested_level,
            granted_level: proposal.granted_level,
            requires_approval: proposal.requires_approval,
            approval_reasons: proposal.approval_reasons,
            policy_template: proposal.policy_template,
            sandbox_from: proposal.sandbox_from,
            script: proposal.script,
            rationale: proposal.rationale,
            next_tools: SkillProposalNextTools {
                observe: Vec::new(),
                propose: vec![
                    "cognition_identity_remember".to_string(),
                    "medousa skill-import".to_string(),
                ],
                sandbox: vec![
                    "cognition_skill_probe".to_string(),
                    "cognition_openshell_sandbox_run".to_string(),
                ],
                deny: Vec::new(),
            },
        }
    }
}

#[medousa_tool(id = COGNITION_SKILL_PROPOSE_ID)]
impl CognitionSkillProposeTool {
    /// Evaluate adopting or executing a skill at a requested security level (observe|propose|sandbox|deny). Maps script risk to AutonomyScope-style approval hints before sandbox execution.
    async fn invoke_typed(
        &self,
        input: SkillProposeInput,
    ) -> stasis::prelude::Result<SkillProposeOutput> {
        let manuscript_id = input.manuscript_id.trim();
        if manuscript_id.is_empty() {
            return Err(StasisError::PortFailure(
                "cognition_skill_propose: manuscript_id is required".to_string(),
            ));
        }
        let requested = SkillSecurityLevel::from(input.security_level);
        let script = input
            .script
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let discovery = discover_skill_for_manuscript(manuscript_id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let manuscript = if requested == SkillSecurityLevel::Observe {
            None
        } else {
            Some(
                build_manuscript_context(manuscript_id)
                    .map_err(|err| StasisError::PortFailure(err.to_string()))?,
            )
        };
        let proposal = evaluate_skill_adoption(&discovery, manuscript.as_ref(), requested, script);
        Ok(SkillProposeOutput::from_proposal(proposal))
    }
}

pub struct CognitionSkillProbeTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionSkillProbeTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

fn default_skill_probe_check_grapheme() -> bool {
    true
}

fn default_skill_probe_operator_approved() -> bool {
    false
}

#[derive(Debug, JsonSchema)]
pub struct SkillProbeInput {
    #[schemars(required, with = "String")]
    manuscript_id: Option<String>,
    /// Relative script path (default: first discovered script)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    script: Option<String>,
    /// Run grapheme --version before skill script (H6)
    #[schemars(with = "bool", default = "default_skill_probe_check_grapheme")]
    check_grapheme: Option<bool>,
    /// Set true when operator approved a proposal with requires_approval
    #[schemars(with = "bool", default = "default_skill_probe_operator_approved")]
    operator_approved: Option<bool>,
}

impl<'de> Deserialize<'de> for SkillProbeInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            manuscript_id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            script: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
            )]
            check_grapheme: Option<bool>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
            )]
            operator_approved: Option<bool>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            manuscript_id: input.manuscript_id,
            script: input.script,
            check_grapheme: input.check_grapheme,
            operator_approved: input.operator_approved,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SkillProbeJobOutput {
    GraphemeVersion {
        job_id: String,
        stage: String,
    },
    SkillScript {
        job_id: String,
        stage: String,
        script: String,
        assets_dir: Option<String>,
    },
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SkillProbeOutput {
    PolicyRejected {
        status: String,
        reason: String,
        proposal: SkillAdoptionProposal,
    },
    ProposalRequired {
        status: String,
        proposal: SkillAdoptionProposal,
        next: String,
    },
    ApprovalRequired {
        status: String,
        proposal: SkillAdoptionProposal,
    },
    GatewayRejected {
        status: String,
        reason: String,
        gateway_url: String,
    },
    Enqueued {
        status: String,
        proposal: SkillAdoptionProposal,
        jobs: Vec<SkillProbeJobOutput>,
    },
}

#[medousa_tool(id = COGNITION_SKILL_PROBE_ID)]
impl CognitionSkillProbeTool {
    /// H6/H7 validation: optionally run grapheme --version in sandbox, then upload and execute an imported skill script when policy grants sandbox level. Host filesystem stays untouched.
    async fn invoke_typed(
        &self,
        input: SkillProbeInput,
    ) -> stasis::prelude::Result<SkillProbeOutput> {
        let manuscript_id = input
            .manuscript_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_skill_probe: manuscript_id is required".to_string(),
                )
            })?;
        let check_grapheme = input.check_grapheme.unwrap_or(true);
        let operator_approved = input.operator_approved.unwrap_or(false);

        let discovery = discover_skill_for_manuscript(manuscript_id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let manuscript = build_manuscript_context(manuscript_id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        let script = input
            .script
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                discovery
                    .scripts
                    .first()
                    .map(|entry| entry.relative_path.clone())
            })
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_skill_probe: no script provided and none discovered".to_string(),
                )
            })?;

        let proposal = evaluate_skill_adoption(
            &discovery,
            Some(&manuscript),
            SkillSecurityLevel::Sandbox,
            Some(&script),
        );
        if proposal.granted_level == SkillSecurityLevel::Deny {
            return Ok(SkillProbeOutput::PolicyRejected {
                status: "rejected".to_string(),
                reason: "policy_denied".to_string(),
                proposal,
            });
        }
        if proposal.granted_level == SkillSecurityLevel::Propose {
            return Ok(SkillProbeOutput::ProposalRequired {
                status: "proposal_required".to_string(),
                proposal,
                next: "Re-run with operator_approved=true after review, or use cognition_skill_propose."
                    .to_string(),
            });
        }
        if proposal.requires_approval && !operator_approved {
            return Ok(SkillProbeOutput::ApprovalRequired {
                status: "approval_required".to_string(),
                proposal,
            });
        }

        let gateway = tokio::task::spawn_blocking(collect_openshell_doctor_report)
            .await
            .map_err(|err| {
                StasisError::PortFailure(format!("openshell preflight join error: {err}"))
            })?;
        if !gateway.readyz_ok {
            return Ok(SkillProbeOutput::GatewayRejected {
                status: "rejected".to_string(),
                reason: "gateway_unhealthy".to_string(),
                gateway_url: gateway.gateway_url,
            });
        }

        let mut job_ids = Vec::new();
        if check_grapheme {
            let grapheme_payload = OpenshellSandboxRunPayload {
                command: vec!["grapheme".to_string(), "--version".to_string()],
                sandbox_from: manuscript.openshell_sandbox_from.clone(),
                policy_template: manuscript.openshell_policy_template.clone(),
                destroy_on_complete: true,
                workdir: Some("/sandbox".to_string()),
                timeout_secs: Some(120),
                manuscript_id: Some(manuscript_id.to_string()),
                correlation_id: Some(format!("probe-grapheme-{manuscript_id}")),
                skill_assets_dir: None,
                skill_upload_dest: None,
                skill_script: None,
            };
            let job_id = enqueue_openshell_job(
                &self.runtime,
                &self.event_tx,
                &self.turn_scope,
                grapheme_payload,
                "cognition_skill_probe",
            )
            .await?;
            job_ids.push(SkillProbeJobOutput::GraphemeVersion {
                job_id,
                stage: "h6_grapheme_version".to_string(),
            });
        }

        let skill_payload = build_sandbox_payload_for_skill(
            manuscript_id,
            &script,
            &manuscript,
            Some(format!("probe-skill-{manuscript_id}")),
        )
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let skill_job_id = enqueue_openshell_job(
            &self.runtime,
            &self.event_tx,
            &self.turn_scope,
            skill_payload,
            "cognition_skill_probe",
        )
        .await?;
        job_ids.push(SkillProbeJobOutput::SkillScript {
            job_id: skill_job_id,
            stage: "h7_skill_script".to_string(),
            script,
            assets_dir: resolve_skill_assets_dir(manuscript_id)
                .map(|path| path.display().to_string())
                .ok(),
        });

        Ok(SkillProbeOutput::Enqueued {
            status: "enqueued".to_string(),
            proposal,
            jobs: job_ids,
        })
    }
}

async fn enqueue_openshell_job(
    runtime: &Arc<RuntimeComposition>,
    event_tx: &mpsc::Sender<TuiEvent>,
    turn_scope: &Arc<RwLock<Option<TurnContinuationScope>>>,
    payload: OpenshellSandboxRunPayload,
    causation: &str,
) -> StasisResult<String> {
    let payload_ref = payload.to_payload_ref()?;
    let job_id = format!("skill-probe-{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let mut job = ToolJobSpec::new(
        job_id.clone(),
        "default",
        OPENSHELL_SANDBOX_RUN_JOB_TYPE,
        payload_ref,
        causation,
        "sttp:in:skill:probe",
        now,
    )
    .build();
    if let Some(scope) = turn_scope.read().await.clone() {
        wire_turn_child_job(
            &mut job,
            &scope,
            COGNITION_SKILL_PROBE,
            OPENSHELL_SANDBOX_RUN_JOB_TYPE,
            ContinuationAwaitMode::Async,
        )
        .await;
    }
    runtime.enqueue_job(job).await?;
    let _ = event_tx
        .send(TuiEvent::JobEnqueued {
            job_id: job_id.clone(),
            job_type: OPENSHELL_SANDBOX_RUN_JOB_TYPE.to_string(),
        })
        .await;
    Ok(job_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_tool_names_are_prefixed() {
        assert!(is_skill_cognition_tool(COGNITION_SKILL_PROBE));
        assert!(!is_skill_cognition_tool("cognition_memory_recall"));
    }
}
