//! Cognition tools for skill discovery, policy-gated proposals, and sandbox probes (H6–H7).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::runtime::job::{BackoffPolicy, NewJob};
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use crate::events::TuiEvent;
use crate::identity_manuscript::build_manuscript_context;
use crate::openshell_handoff::collect_openshell_doctor_report;
use crate::openshell_sandbox_run::{
    OPENSHELL_SANDBOX_RUN_JOB_TYPE, OpenshellSandboxRunPayload,
};
use crate::skill_execution::{
    SkillSecurityLevel, build_sandbox_payload_for_skill, discover_skill_for_manuscript,
    discovery_report_json, evaluate_skill_adoption, proposal_json, resolve_skill_assets_dir,
    skill_security_level_parse,
};
use crate::skill_import::resolve_skill_source;
use crate::turn_continuation::{
    ContinuationAwaitMode, TurnContinuationScope, wire_turn_child_job,
};

pub const COGNITION_SKILL_DISCOVER: &str = "cognition_skill_discover";
pub const COGNITION_SKILL_PROPOSE: &str = "cognition_skill_propose";
pub const COGNITION_SKILL_PROBE: &str = "cognition_skill_probe";

pub const SKILL_COGNITION_TOOLS: &[&str] = &[
    COGNITION_SKILL_DISCOVER,
    COGNITION_SKILL_PROPOSE,
    COGNITION_SKILL_PROBE,
];

pub fn is_skill_cognition_tool(name: &str) -> bool {
    name.starts_with("cognition_skill_")
}

pub fn register_skill_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionSkillDiscoverTool)?;
    registry.register_tool(CognitionSkillProposeTool)?;
    registry.register_tool(CognitionSkillProbeTool::new(
        runtime,
        event_tx,
        turn_scope,
    ))?;
    Ok(())
}

pub struct CognitionSkillDiscoverTool;

#[async_trait]
impl StasisTool for CognitionSkillDiscoverTool {
    fn name(&self) -> &'static str {
        COGNITION_SKILL_DISCOVER
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Discover runnable scripts in an imported skill manuscript or raw skill directory. \
             Returns risk classes for on-the-fly skill learning (observe before execute).",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "manuscript_id": {
                    "type": "string",
                    "description": "Imported skill manuscript id (preferred)"
                },
                "skill_path": {
                    "type": "string",
                    "description": "Raw skill directory or SKILL.md path before import"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        if let Some(id) = input
            .get("manuscript_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let report = discover_skill_for_manuscript(id)
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            return Ok(discovery_report_json(&report));
        }

        let skill_path = input
            .get("skill_path")
            .and_then(|value| value.as_str())
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
        Ok(json!({
            "skill_id": source.file_name().and_then(|value| value.to_str()).unwrap_or("skill"),
            "assets_dir": source.display().to_string(),
            "has_scripts": !scripts.is_empty(),
            "max_risk_class": max_risk_class,
            "max_risk_score": max_risk_score,
            "scripts": scripts,
            "import_hint": "Run medousa skill-import to adopt this skill as a manuscript specialty.",
        }))
    }
}

pub struct CognitionSkillProposeTool;

#[async_trait]
impl StasisTool for CognitionSkillProposeTool {
    fn name(&self) -> &'static str {
        COGNITION_SKILL_PROPOSE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Evaluate adopting or executing a skill at a requested security level (observe|propose|sandbox|deny). \
             Maps script risk to AutonomyScope-style approval hints before sandbox execution.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["manuscript_id", "security_level"],
            "properties": {
                "manuscript_id": { "type": "string" },
                "security_level": {
                    "type": "string",
                    "enum": ["observe", "propose", "sandbox", "deny"]
                },
                "script": {
                    "type": "string",
                    "description": "Relative script path (e.g. scripts/run.sh) for sandbox evaluation"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let manuscript_id = input
            .get("manuscript_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_skill_propose: manuscript_id is required".to_string(),
                )
            })?;
        let level_raw = input
            .get("security_level")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_skill_propose: security_level is required".to_string(),
                )
            })?;
        let requested = skill_security_level_parse(level_raw).ok_or_else(|| {
            StasisError::PortFailure(format!(
                "cognition_skill_propose: invalid security_level '{level_raw}'"
            ))
        })?;
        let script = input
            .get("script")
            .and_then(|value| value.as_str())
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
        let proposal = evaluate_skill_adoption(
            &discovery,
            manuscript.as_ref(),
            requested,
            script,
        );
        let mut response = proposal_json(&proposal);
        if let Some(obj) = response.as_object_mut() {
            obj.insert(
                "next_tools".to_string(),
                json!({
                    "observe": [],
                    "propose": ["cognition_identity_remember", "medousa skill-import"],
                    "sandbox": [
                        "cognition_skill_probe",
                        "cognition_openshell_sandbox_run"
                    ],
                    "deny": []
                }),
            );
        }
        Ok(response)
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

#[async_trait]
impl StasisTool for CognitionSkillProbeTool {
    fn name(&self) -> &'static str {
        COGNITION_SKILL_PROBE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "H6/H7 validation: optionally run grapheme --version in sandbox, then upload and execute \
             an imported skill script when policy grants sandbox level. Host filesystem stays untouched.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "manuscript_id": { "type": "string" },
                "script": {
                    "type": "string",
                    "description": "Relative script path (default: first discovered script)"
                },
                "check_grapheme": {
                    "type": "boolean",
                    "description": "Run grapheme --version before skill script (H6)",
                    "default": true
                },
                "operator_approved": {
                    "type": "boolean",
                    "description": "Set true when operator approved a proposal with requires_approval",
                    "default": false
                }
            },
            "required": ["manuscript_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let manuscript_id = input
            .get("manuscript_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_skill_probe: manuscript_id is required".to_string(),
                )
            })?;
        let check_grapheme = input
            .get("check_grapheme")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let operator_approved = input
            .get("operator_approved")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let discovery = discover_skill_for_manuscript(manuscript_id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let manuscript = build_manuscript_context(manuscript_id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        let script = input
            .get("script")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| discovery.scripts.first().map(|entry| entry.relative_path.clone()))
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
            return Ok(json!({
                "status": "rejected",
                "reason": "policy_denied",
                "proposal": proposal_json(&proposal),
            }));
        }
        if proposal.granted_level == SkillSecurityLevel::Propose {
            return Ok(json!({
                "status": "proposal_required",
                "proposal": proposal_json(&proposal),
                "next": "Re-run with operator_approved=true after review, or use cognition_skill_propose.",
            }));
        }
        if proposal.requires_approval && !operator_approved {
            return Ok(json!({
                "status": "approval_required",
                "proposal": proposal_json(&proposal),
            }));
        }

        let gateway = tokio::task::spawn_blocking(collect_openshell_doctor_report)
            .await
            .map_err(|err| StasisError::PortFailure(format!("openshell preflight join error: {err}")))?;
        if !gateway.readyz_ok {
            return Ok(json!({
                "status": "rejected",
                "reason": "gateway_unhealthy",
                "gateway_url": gateway.gateway_url,
            }));
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
            job_ids.push(json!({
                "job_id": job_id,
                "stage": "h6_grapheme_version",
            }));
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
        job_ids.push(json!({
            "job_id": skill_job_id,
            "stage": "h7_skill_script",
            "script": script,
            "assets_dir": resolve_skill_assets_dir(manuscript_id)
                .map(|path| path.display().to_string())
                .ok(),
        }));

        Ok(json!({
            "status": "enqueued",
            "proposal": proposal_json(&proposal),
            "jobs": job_ids,
        }))
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
    let mut job = NewJob {
        id: job_id.clone(),
        queue: "default".to_string(),
        job_type: OPENSHELL_SANDBOX_RUN_JOB_TYPE.to_string(),
        payload_ref,
        priority: 100,
        max_attempts: 1,
        idempotency_key: format!("idem-{job_id}"),
        correlation_id: job_id.clone(),
        causation_id: causation.to_string(),
        trace_id: job_id.clone(),
        sttp_input_node_id: "sttp:in:skill:probe".to_string(),
        scheduled_at: now,
        backoff_policy: BackoffPolicy::default(),
    };
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
    match &**runtime {
        RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
        RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
    }
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
