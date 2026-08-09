//! Host-bus delegation tools (spawn / status / cancel).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::domain::errors::StasisError;

use crate::agent_runtime::turn_worker::{
    SpawnTurnWorkerOutput, TurnWorkRecord, TurnWorkStatus, TurnWorkerIntent, turn_worker_store,
};
use crate::semantic_values::TrimmedText;
use crate::typed_tools::{ToolId, medousa_tool};
use std::sync::Arc;

pub const COGNITION_SPAWN_TURN_WORKER: &str = "cognition_spawn_turn_worker";
pub const COGNITION_TURN_WORKER_STATUS: &str = "cognition_turn_worker_status";
pub const COGNITION_TURN_WORKER_CANCEL: &str = "cognition_turn_worker_cancel";
pub const COGNITION_WORKSHOP_STEER: &str = "cognition_workshop_steer";

const COGNITION_TURN_WORKER_CANCEL_ID: ToolId = ToolId::new(COGNITION_TURN_WORKER_CANCEL);
const COGNITION_TURN_WORKER_STATUS_ID: ToolId = ToolId::new(COGNITION_TURN_WORKER_STATUS);
const COGNITION_SPAWN_TURN_WORKER_ID: ToolId = ToolId::new(COGNITION_SPAWN_TURN_WORKER);
const COGNITION_WORKSHOP_STEER_ID: ToolId = ToolId::new(COGNITION_WORKSHOP_STEER);

pub fn is_spawn_turn_worker_tool_name(name: &str) -> bool {
    name.trim() == COGNITION_SPAWN_TURN_WORKER
}

pub fn worker_spawn_from_invocations(
    invocations: &[stasis::application::orchestration::tool_loop_pipeline::ToolInvocation],
) -> Option<(String, String)> {
    invocations.iter().rev().find_map(|inv| {
        if !is_spawn_turn_worker_tool_name(&inv.tool_name) {
            return None;
        }
        let spawned = inv
            .tool_output
            .get("worker_spawned")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !spawned {
            return None;
        }
        let work_id = inv.tool_output.get("work_id")?.as_str()?.to_string();
        let ack = inv
            .tool_output
            .get("user_ack")
            .and_then(|v| v.as_str())
            .or_else(|| inv.tool_output.get("message").and_then(|v| v.as_str()))
            .unwrap_or("Working on that in the background.")
            .to_string();
        Some((work_id, ack))
    })
}

pub struct CognitionSpawnTurnWorkerTool {
    scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
}

impl CognitionSpawnTurnWorkerTool {
    pub fn new(scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>) -> Self {
        Self { scheduler }
    }
}

#[derive(Debug, JsonSchema)]
pub struct SpawnTurnWorkerInput {
    /// Worker profile: memory.avec_calibrate | memory.context | research | general
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    intent: Option<String>,
    /// Focused task for the worker: capability id, module.op, URLs, and constraints. Include what the host already resolved so the worker does not rediscover.
    #[schemars(required, with = "String")]
    task: Option<String>,
    /// Short message for the user while the worker runs
    #[schemars(required, with = "String")]
    user_ack: Option<String>,
    /// Optional YAML specialty (voice, tools, worker intent, spec.worker.stage_role, spec.worker.model_hint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    manuscript_id: Option<String>,
    /// Optional StageRoutingMatrix role (extractor, verifier, chunker, …)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    stage_role: Option<String>,
    /// Optional. Prefer omit or 'auto' to use user stage-routing / host prefs. Only set provider:model when explicitly requested (e.g. deepseek:deepseek-v4-flash).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    model_hint: Option<String>,
}

impl<'de> Deserialize<'de> for SpawnTurnWorkerInput {
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
            intent: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            task: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            user_ack: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            manuscript_id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            stage_role: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            model_hint: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            intent: input.intent,
            task: input.task,
            user_ack: input.user_ack,
            manuscript_id: input.manuscript_id,
            stage_role: input.stage_role,
            model_hint: input.model_hint,
        })
    }
}

#[derive(Debug)]
struct SpawnTurnWorkerCommand {
    explicit_intent: Option<TurnWorkerIntent>,
    task: String,
    user_ack: String,
    manuscript_id: Option<String>,
    stage_role: Option<String>,
    model_hint: Option<String>,
}

impl TryFrom<SpawnTurnWorkerInput> for SpawnTurnWorkerCommand {
    type Error = StasisError;

    fn try_from(input: SpawnTurnWorkerInput) -> Result<Self, Self::Error> {
        let explicit_intent = input
            .intent
            .and_then(|value| TrimmedText::new(value).ok().map(TrimmedText::into_string))
            .map(|raw| {
                TurnWorkerIntent::parse(&raw).ok_or_else(|| {
                    StasisError::PortFailure(format!(
                        "cognition_spawn_turn_worker: unknown intent '{raw}'"
                    ))
                })
            })
            .transpose()?;
        let task = required_worker_text(input.task, "task")?;
        let user_ack = required_worker_text(input.user_ack, "user_ack")?;

        Ok(Self {
            explicit_intent,
            task,
            user_ack,
            manuscript_id: optional_worker_text(input.manuscript_id),
            stage_role: optional_worker_text(input.stage_role),
            model_hint: optional_worker_text(input.model_hint),
        })
    }
}

fn required_worker_text(value: Option<String>, field: &str) -> Result<String, StasisError> {
    TrimmedText::new(value.unwrap_or_default())
        .map(TrimmedText::into_string)
        .map_err(|_| {
            StasisError::PortFailure(format!(
                "cognition_spawn_turn_worker: {field} is required"
            ))
        })
}

fn optional_worker_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| TrimmedText::new(value).ok().map(TrimmedText::into_string))
}

#[medousa_tool(id = COGNITION_SPAWN_TURN_WORKER_ID)]
impl CognitionSpawnTurnWorkerTool {
    /// Delegate heavy work to a background turn worker (web/Grapheme execution, memory rituals). Returns immediately; the worker runs tools with a focused policy, then a synthesis pass delivers the final user-facing answer. Intents: memory.avec_calibrate | memory.context | research | general. Optional manuscript_id loads a YAML specialty (voice, tool allowlist, identity pins, OpenShell/skill tools). Manuscript spec.worker.stage_role selects a StageRoutingMatrix route (extractor, verifier, …); spec.worker.model_hint overrides provider/model. Spawn-time stage_role/model_hint win over manuscript defaults. Prefer omitting model_hint (or set model_hint=auto) so workshop StageRoutingMatrix / host preferences choose provider+model. Only pass provider:model when the user explicitly asked for that combo. Bare model ids infer the provider when unambiguous; otherwise they inherit the host turn provider — never the process default. Use manuscript_id=echo-skill or openshell-researcher for sandbox script execution. Put resolved capability/module/op and any host evidence into task — workers do not see parent chat.
    async fn invoke_typed(
        &self,
        input: SpawnTurnWorkerInput,
    ) -> stasis::prelude::Result<SpawnTurnWorkerOutput> {
        let command = SpawnTurnWorkerCommand::try_from(input)?;
        let manuscript = command
            .manuscript_id
            .as_deref()
            .map(crate::identity_manuscript::build_manuscript_context)
            .transpose()
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        let intent = match (
            command.explicit_intent,
            manuscript
                .as_ref()
                .and_then(|ctx| ctx.worker_intent.as_deref()),
        ) {
            (Some(intent), _) => intent,
            (None, Some(ms_intent)) => TurnWorkerIntent::parse(ms_intent).ok_or_else(|| {
                StasisError::PortFailure(format!(
                    "cognition_spawn_turn_worker: manuscript worker intent '{ms_intent}' is invalid"
                ))
            })?,
            (None, None) => {
                return Err(StasisError::PortFailure(
                    "cognition_spawn_turn_worker: intent is required (or provide manuscript_id with spec.worker.intent)".to_string(),
                ));
            }
        };

        self.scheduler
            .spawn_worker(
                intent,
                &command.task,
                &command.user_ack,
                None,
                manuscript,
                command.stage_role.as_deref(),
                command.model_hint.as_deref(),
            )
            .await
    }
}

pub struct CognitionTurnWorkerStatusTool {
    scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
}

impl CognitionTurnWorkerStatusTool {
    pub fn new(scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>) -> Self {
        Self { scheduler }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnWorkerStatusInput {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    work_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TurnWorkerStatusOutput {
    Record {
        ok: bool,
        #[schemars(with = "serde_json::Value")]
        record: Box<TurnWorkRecord>,
    },
    List {
        ok: bool,
        session_id: String,
        active_count: usize,
        #[schemars(with = "Vec<serde_json::Value>")]
        records: Vec<TurnWorkRecord>,
    },
}

#[medousa_tool(id = COGNITION_TURN_WORKER_STATUS_ID)]
impl CognitionTurnWorkerStatusTool {
    /// List or fetch status of background turn workers. On an active host turn, omit session_id to use the current session.
    async fn invoke_typed(
        &self,
        input: TurnWorkerStatusInput,
    ) -> stasis::prelude::Result<TurnWorkerStatusOutput> {
        let store = turn_worker_store();
        if let Some(work_id) = input.work_id.as_deref() {
            let record = store
                .get(work_id)
                .ok_or_else(|| StasisError::PortFailure(format!("work_id not found: {work_id}")))?;
            return Ok(TurnWorkerStatusOutput::Record {
                ok: true,
                record: Box::new(record),
            });
        }
        let session_id = match input
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(session_id) => session_id.to_string(),
            None => self
                .scheduler
                .active_bus_session_id()
                .await
                .ok_or_else(|| {
                    StasisError::PortFailure(
                        "cognition_turn_worker_status: session_id required when no host turn is active"
                            .to_string(),
                    )
                })?,
        };
        let records = store.list_for_session(&session_id);
        let active = records
            .iter()
            .filter(|record| {
                matches!(
                    record.status,
                    TurnWorkStatus::Pending | TurnWorkStatus::Running
                )
            })
            .count();
        Ok(TurnWorkerStatusOutput::List {
            ok: true,
            session_id,
            active_count: active,
            records,
        })
    }
}

pub struct CognitionTurnWorkerCancelTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TurnWorkerCancelInput {
    pub work_id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TurnWorkerCancelOutput {
    pub ok: bool,
    /// Evolving durable worker record; its wire projection remains owned by the worker store.
    #[schemars(with = "serde_json::Value")]
    pub record: TurnWorkRecord,
}

#[medousa_tool(id = COGNITION_TURN_WORKER_CANCEL_ID)]
impl CognitionTurnWorkerCancelTool {
    /// Mark a pending or running turn worker as cancelled (best-effort; in-flight worker may still finish).
    async fn invoke_typed(
        &self,
        input: TurnWorkerCancelInput,
    ) -> stasis::prelude::Result<TurnWorkerCancelOutput> {
        let work_id = input.work_id;
        let store = turn_worker_store();
        let updated = store
            .update(&work_id, |r| {
                if matches!(r.status, TurnWorkStatus::Pending | TurnWorkStatus::Running) {
                    r.status = TurnWorkStatus::Cancelled;
                }
            })
            .ok_or_else(|| StasisError::PortFailure(format!("work_id not found: {work_id}")))?;
        Ok(TurnWorkerCancelOutput {
            ok: true,
            record: updated,
        })
    }
}

pub fn register_turn_worker_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionSpawnTurnWorkerTool::new(scheduler.clone()))?;
    registry.register_typed_tool(CognitionWorkshopSteerTool::new(scheduler.clone()))?;
    registry.register_typed_tool(CognitionTurnWorkerStatusTool::new(scheduler))?;
    registry.register_typed_tool(CognitionTurnWorkerCancelTool)?;
    Ok(())
}

pub struct CognitionWorkshopSteerTool {
    scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
}

impl CognitionWorkshopSteerTool {
    pub fn new(scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>) -> Self {
        Self { scheduler }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkshopSteerInput {
    /// Steer text for the bound workshop
    message: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum WorkshopSteerOutput {
    Failure {
        ok: bool,
        error: String,
    },
    Queued {
        ok: bool,
        work_id: String,
        queued: usize,
        speaker_profile_id: Option<String>,
    },
}

impl WorkshopSteerOutput {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Queued { .. })
    }
}

#[medousa_tool(id = COGNITION_WORKSHOP_STEER_ID)]
impl CognitionWorkshopSteerTool {
    /// Forward a principal steer message into the active bound workshop for this session. Use when the operator adds guidance while workshop execution is in flight.
    async fn invoke_typed(
        &self,
        input: WorkshopSteerInput,
    ) -> stasis::prelude::Result<WorkshopSteerOutput> {
        let message = input.message.trim();
        if message.is_empty() {
            return Err(StasisError::PortFailure(
                "cognition_workshop_steer: message required".to_string(),
            ));
        }
        let session_id = self
            .scheduler
            .active_bus_session_id()
            .await
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_workshop_steer: no active host turn session".to_string(),
                )
            })?;
        let speaker = crate::user_profiles::resolve_workshop_identity_user_id();
        steer_bound_workshop_for_session(&session_id, message, Some(speaker))
    }
}

pub fn steer_bound_workshop_for_session(
    session_id: &str,
    message: &str,
    speaker_profile_id: Option<String>,
) -> stasis::prelude::Result<WorkshopSteerOutput> {
    let store = turn_worker_store();
    let Some(record) = store.active_bound_workshop(session_id) else {
        return Ok(WorkshopSteerOutput::Failure {
            ok: false,
            error: "no active bound workshop for session".to_string(),
        });
    };

    let speaker = speaker_profile_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(profile_id) = speaker.as_deref()
        && let Some(row) = crate::shared_session_catalog::get_shared_row(session_id)
        && !row.includes_member(profile_id)
    {
        return Ok(WorkshopSteerOutput::Failure {
            ok: false,
            error: "speaker is not a member of this shared room".to_string(),
        });
    }

    let updated = store
        .push_steer(&record.work_id, message.to_string(), speaker.clone())
        .ok_or_else(|| StasisError::PortFailure("failed to queue steer message".to_string()))?;

    // Surface the steer in the shared transcript so Home can attribute the speaker.
    let turn = crate::turn_parts::user_conversation_turn_with_media_and_speaker(
        message,
        &[],
        speaker.as_deref(),
    );
    crate::session_writer::persist_turn(session_id, turn, None);

    Ok(WorkshopSteerOutput::Queued {
        ok: true,
        work_id: updated.work_id,
        queued: updated.steer_messages.len(),
        speaker_profile_id: speaker,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_command_parses_intent_and_normalizes_identifiers() {
        let command = SpawnTurnWorkerCommand::try_from(SpawnTurnWorkerInput {
            intent: Some("  research  ".to_string()),
            task: Some("  inspect the feed  ".to_string()),
            user_ack: Some("  I am on it  ".to_string()),
            manuscript_id: Some("  research-specialist  ".to_string()),
            stage_role: Some("  verifier  ".to_string()),
            model_hint: Some("  auto  ".to_string()),
        })
        .expect("spawn command");

        assert_eq!(command.explicit_intent, Some(TurnWorkerIntent::Research));
        assert_eq!(command.task, "inspect the feed");
        assert_eq!(command.user_ack, "I am on it");
        assert_eq!(command.manuscript_id.as_deref(), Some("research-specialist"));
        assert_eq!(command.stage_role.as_deref(), Some("verifier"));
        assert_eq!(command.model_hint.as_deref(), Some("auto"));
    }

    #[test]
    fn spawn_command_rejects_blank_required_text_and_unknown_intent() {
        let blank = SpawnTurnWorkerCommand::try_from(SpawnTurnWorkerInput {
            intent: Some("general".to_string()),
            task: Some(" ".to_string()),
            user_ack: Some("ack".to_string()),
            manuscript_id: None,
            stage_role: None,
            model_hint: None,
        })
        .unwrap_err();
        assert!(blank.to_string().contains("task is required"));

        let unknown = SpawnTurnWorkerCommand::try_from(SpawnTurnWorkerInput {
            intent: Some("unknown".to_string()),
            task: Some("task".to_string()),
            user_ack: Some("ack".to_string()),
            manuscript_id: None,
            stage_role: None,
            model_hint: None,
        })
        .unwrap_err();
        assert!(unknown.to_string().contains("unknown intent 'unknown'"));
    }
}
