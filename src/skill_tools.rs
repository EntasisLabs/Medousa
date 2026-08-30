//! Cognition tools for skill discovery, policy-gated proposals, and sandbox probes (H6–H7).

#[cfg(feature = "full-daemon")]
use std::sync::Arc;

#[cfg(feature = "full-daemon")]
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "full-daemon")]
use stasis::prelude::Result as StasisResult;
#[cfg(feature = "full-daemon")]
use stasis::prelude::RuntimeComposition;
use stasis::prelude::StasisError;
#[cfg(feature = "full-daemon")]
use tokio::sync::mpsc;
#[cfg(feature = "full-daemon")]
use uuid::Uuid;

#[cfg(feature = "full-daemon")]
use crate::events::TuiEvent;
use crate::identity_manuscript::build_manuscript_context;
#[cfg(feature = "full-daemon")]
use crate::openshell_handoff::collect_openshell_doctor_report;
#[cfg(feature = "full-daemon")]
use crate::openshell_sandbox_run::{OPENSHELL_SANDBOX_RUN_JOB_TYPE, OpenshellSandboxRunPayload};
#[cfg(feature = "full-daemon")]
use crate::runtime_composition_ext::RuntimeCompositionExt;
#[cfg(feature = "full-daemon")]
use crate::runtime_job_spec::ToolJobSpec;
use crate::semantic_values::TrimmedText;
use crate::skill_execution::{
    SkillAdoptionProposal, SkillScriptEntry, SkillScriptRiskClass, SkillSecurityLevel,
    discover_skill_for_manuscript, evaluate_skill_adoption,
};
#[cfg(feature = "full-daemon")]
use crate::skill_execution::{build_sandbox_payload_for_skill, resolve_skill_assets_dir};
use crate::skill_import::resolve_skill_source;
#[cfg(feature = "full-daemon")]
use crate::turn_continuation::{ContinuationAwaitMode, wire_turn_child_job};
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

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
    crate::tool_names::is_skill_cognition_tool(name)
}

pub fn register_portable_skill_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionSkillDiscoverTool)?;
    registry.register_typed_tool(CognitionSkillProposeTool)?;
    Ok(())
}

#[cfg(feature = "full-daemon")]
pub fn register_skill_probe_tool(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionSkillProbeTool::new(runtime, event_tx, turn_scope))?;
    Ok(())
}

pub struct CognitionSkillDiscoverTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillDiscoverInput {
    /// Imported skill manuscript id (preferred)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub manuscript_id: CompatOption<String>,
    /// Raw skill directory or SKILL.md path before import
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub skill_path: CompatOption<String>,
}

#[derive(Debug)]
struct SkillDiscoverCommand {
    manuscript_id: Option<TrimmedText>,
    skill_path: Option<TrimmedText>,
}

impl TryFrom<SkillDiscoverInput> for SkillDiscoverCommand {
    type Error = StasisError;

    fn try_from(input: SkillDiscoverInput) -> Result<Self, Self::Error> {
        let manuscript_id = input
            .manuscript_id
            .into_option()
            .and_then(|value| TrimmedText::new(value).ok());
        let skill_path = input
            .skill_path
            .into_option()
            .and_then(|value| TrimmedText::new(value).ok());
        if manuscript_id.is_none() && skill_path.is_none() {
            return Err(StasisError::PortFailure(
                "cognition_skill_discover: manuscript_id or skill_path is required".to_string(),
            ));
        }
        Ok(Self {
            manuscript_id,
            skill_path,
        })
    }
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
        let command = SkillDiscoverCommand::try_from(input)?;
        if let Some(id) = command.manuscript_id.as_ref() {
            let report = discover_skill_for_manuscript(id.as_str())
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

        let skill_path = command
            .skill_path
            .as_ref()
            .expect("discover command validates one source");
        let source = resolve_skill_source(std::path::Path::new(skill_path.as_str()))
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
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub script: CompatOption<String>,
}

#[derive(Debug)]
struct SkillProposeCommand {
    manuscript_id: TrimmedText,
    security_level: SkillSecurityLevel,
    script: Option<TrimmedText>,
}

impl TryFrom<SkillProposeInput> for SkillProposeCommand {
    type Error = StasisError;

    fn try_from(input: SkillProposeInput) -> Result<Self, Self::Error> {
        let manuscript_id = TrimmedText::new(input.manuscript_id).map_err(|_| {
            StasisError::PortFailure(
                "cognition_skill_propose: manuscript_id is required".to_string(),
            )
        })?;
        Ok(Self {
            manuscript_id,
            security_level: input.security_level.into(),
            script: input
                .script
                .into_option()
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
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
                    "cognition_identity_mutate".to_string(),
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
        let command = SkillProposeCommand::try_from(input)?;
        let manuscript_id = command.manuscript_id.as_str();
        let requested = command.security_level;
        let script = command.script.as_ref().map(TrimmedText::as_str);

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

#[cfg(feature = "full-daemon")]
pub struct CognitionSkillProbeTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

#[cfg(feature = "full-daemon")]
impl CognitionSkillProbeTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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
    manuscript_id: CompatOption<String>,
    /// Relative script path (default: first discovered script)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    script: CompatOption<String>,
    /// Run grapheme --version before skill script (H6)
    #[schemars(with = "bool", default = "default_skill_probe_check_grapheme")]
    check_grapheme: CompatOption<bool>,
    /// Set true when operator approved a proposal with requires_approval
    #[schemars(with = "bool", default = "default_skill_probe_operator_approved")]
    operator_approved: CompatOption<bool>,
}

impl<'de> Deserialize<'de> for SkillProbeInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            manuscript_id: CompatOption<String>,
            #[serde(default)]
            script: CompatOption<String>,
            #[serde(default)]
            check_grapheme: CompatOption<bool>,
            #[serde(default)]
            operator_approved: CompatOption<bool>,
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

#[derive(Debug)]
struct SkillProbeCommand {
    manuscript_id: TrimmedText,
    script: Option<TrimmedText>,
    check_grapheme: bool,
    operator_approved: bool,
}

impl TryFrom<SkillProbeInput> for SkillProbeCommand {
    type Error = StasisError;

    fn try_from(input: SkillProbeInput) -> Result<Self, Self::Error> {
        let manuscript_id = TrimmedText::new(input.manuscript_id.into_option().unwrap_or_default())
            .map_err(|_| {
                StasisError::PortFailure(
                    "cognition_skill_probe: manuscript_id is required".to_string(),
                )
            })?;
        Ok(Self {
            manuscript_id,
            script: input
                .script
                .into_option()
                .and_then(|value| TrimmedText::new(value).ok()),
            check_grapheme: input
                .check_grapheme
                .into_option()
                .unwrap_or_else(default_skill_probe_check_grapheme),
            operator_approved: input
                .operator_approved
                .into_option()
                .unwrap_or_else(default_skill_probe_operator_approved),
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

#[cfg(feature = "full-daemon")]
#[medousa_tool(id = COGNITION_SKILL_PROBE_ID)]
impl CognitionSkillProbeTool {
    /// Validate Grapheme availability, then upload and execute an imported skill script in a sandbox.
    async fn invoke_typed(
        &self,
        input: SkillProbeInput,
    ) -> stasis::prelude::Result<SkillProbeOutput> {
        let command = SkillProbeCommand::try_from(input)?;
        let manuscript_id = command.manuscript_id.as_str();
        let check_grapheme = command.check_grapheme;
        let operator_approved = command.operator_approved;

        let discovery = discover_skill_for_manuscript(manuscript_id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let manuscript = build_manuscript_context(manuscript_id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        let script = command
            .script
            .map(TrimmedText::into_string)
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
                providers: Vec::new(),
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

#[cfg(feature = "full-daemon")]
async fn enqueue_openshell_job(
    runtime: &Arc<RuntimeComposition>,
    event_tx: &mpsc::Sender<TuiEvent>,
    turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
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
        now,
    )
    .build();
    if let Some(scope) =
        crate::agent_runtime::execution_context::turn_continuation_scope(turn_scope).await
    {
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
        assert!(!is_skill_cognition_tool("cognition_memory_query"));
    }

    #[test]
    fn skill_commands_normalize_identifiers_and_defaults() {
        let discover = SkillDiscoverCommand::try_from(SkillDiscoverInput {
            manuscript_id: Some(" manuscript-a ".into()).into(),
            skill_path: Some(" \n\t".into()).into(),
        })
        .expect("discover command");
        assert_eq!(
            discover.manuscript_id.as_ref().unwrap().as_str(),
            "manuscript-a"
        );
        assert!(discover.skill_path.is_none());

        let propose = SkillProposeCommand::try_from(SkillProposeInput {
            manuscript_id: " manuscript-a ".into(),
            security_level: SkillSecurityLevelInput::Sandbox,
            script: Some(" scripts/run.sh ".into()).into(),
        })
        .expect("propose command");
        assert_eq!(propose.manuscript_id.as_str(), "manuscript-a");
        assert_eq!(propose.security_level, SkillSecurityLevel::Sandbox);
        assert_eq!(propose.script.as_ref().unwrap().as_str(), "scripts/run.sh");

        let probe = SkillProbeCommand::try_from(SkillProbeInput {
            manuscript_id: Some(" manuscript-a ".into()).into(),
            script: Some(" scripts/run.sh ".into()).into(),
            check_grapheme: None.into(),
            operator_approved: None.into(),
        })
        .expect("probe command");
        assert_eq!(probe.manuscript_id.as_str(), "manuscript-a");
        assert_eq!(probe.script.as_ref().unwrap().as_str(), "scripts/run.sh");
        assert!(probe.check_grapheme);
        assert!(!probe.operator_approved);
    }

    #[test]
    fn skill_discover_command_requires_a_source() {
        let error = SkillDiscoverCommand::try_from(SkillDiscoverInput {
            manuscript_id: Some(" \n\t".into()).into(),
            skill_path: Some(" \n\t".into()).into(),
        })
        .expect_err("source is required");
        assert!(
            error
                .to_string()
                .contains("manuscript_id or skill_path is required")
        );
    }

    #[test]
    fn skill_wire_optionals_remain_lenient_for_legacy_values() {
        let discover: SkillDiscoverInput = serde_json::from_value(serde_json::json!({
            "manuscript_id": 42,
            "skill_path": false,
        }))
        .expect("discover input");
        assert!(discover.manuscript_id.into_option().is_none());
        assert!(discover.skill_path.into_option().is_none());

        let propose: SkillProposeInput = serde_json::from_value(serde_json::json!({
            "manuscript_id": "manuscript-a",
            "security_level": "sandbox",
            "script": [],
        }))
        .expect("propose input");
        assert!(propose.script.into_option().is_none());

        let probe: SkillProbeInput = serde_json::from_value(serde_json::json!({
            "manuscript_id": "manuscript-a",
            "script": 9,
            "check_grapheme": "true",
            "operator_approved": [],
        }))
        .expect("probe input");
        assert!(probe.script.into_option().is_none());
        assert!(probe.check_grapheme.into_option().is_none());
        assert!(probe.operator_approved.into_option().is_none());
    }
}
