//! Host-bus delegation tools (spawn / status / cancel).

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::StasisError;

use crate::agent_runtime::turn_worker::{
    TurnWorkerIntent, TurnWorkStatus, turn_worker_store,
};
use std::sync::Arc;

pub const COGNITION_SPAWN_TURN_WORKER: &str = "cognition_spawn_turn_worker";
pub const COGNITION_TURN_WORKER_STATUS: &str = "cognition_turn_worker_status";
pub const COGNITION_TURN_WORKER_CANCEL: &str = "cognition_turn_worker_cancel";

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

#[async_trait]
impl StasisTool for CognitionSpawnTurnWorkerTool {
    fn name(&self) -> &'static str {
        COGNITION_SPAWN_TURN_WORKER
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Delegate heavy work to a background turn worker (web/Grapheme execution, memory rituals). \
             Returns immediately; the worker runs tools with a focused policy, then a synthesis pass \
             delivers the final user-facing answer. Intents: memory.avec_calibrate | memory.context | research | general. \
             Optional manuscript_id loads a YAML specialty (voice, tool allowlist, identity pins). \
             Put resolved capability/module/op and any host evidence into task — workers do not see parent chat.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "intent": {
                    "type": "string",
                    "description": "Worker profile: memory.avec_calibrate | memory.context | research | general"
                },
                "task": {
                    "type": "string",
                    "description": "Focused task for the worker: capability id, module.op, URLs, and constraints. Include what the host already resolved so the worker does not rediscover."
                },
                "user_ack": {
                    "type": "string",
                    "description": "Short message for the user while the worker runs"
                },
                "manuscript_id": {
                    "type": "string",
                    "description": "Optional YAML identity manuscript id (e.g. morning-brief)"
                }
            },
            "required": ["task", "user_ack"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let manuscript = input
            .get("manuscript_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(crate::identity_manuscript::build_manuscript_context)
            .transpose()
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        let intent_raw = input.get("intent").and_then(|v| v.as_str()).map(str::trim);
        let intent = match (
            intent_raw.filter(|value| !value.is_empty()),
            manuscript
                .as_ref()
                .and_then(|ctx| ctx.worker_intent.as_deref()),
        ) {
            (Some(raw), _) => TurnWorkerIntent::parse(raw).ok_or_else(|| {
                StasisError::PortFailure(format!(
                    "cognition_spawn_turn_worker: unknown intent '{raw}'"
                ))
            })?,
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
        let task = input
            .get("task")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure("cognition_spawn_turn_worker: task is required".to_string())
            })?;
        let user_ack = input
            .get("user_ack")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_spawn_turn_worker: user_ack is required".to_string(),
                )
            })?;

        self.scheduler
            .spawn_worker(intent, task, user_ack, None, manuscript)
            .await
    }
}

pub struct CognitionTurnWorkerStatusTool;

#[async_trait]
impl StasisTool for CognitionTurnWorkerStatusTool {
    fn name(&self) -> &'static str {
        COGNITION_TURN_WORKER_STATUS
    }

    fn description(&self) -> Option<&'static str> {
        Some("List or fetch status of background turn workers for the current session.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": { "type": "string" },
                "session_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let store = turn_worker_store();
        if let Some(work_id) = input.get("work_id").and_then(|v| v.as_str()) {
            let record = store.get(work_id).ok_or_else(|| {
                StasisError::PortFailure(format!("work_id not found: {work_id}"))
            })?;
            return Ok(json!({ "ok": true, "record": record }));
        }
        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let records = if let Some(session_id) = session_id {
            store.list_for_session(session_id)
        } else {
            Vec::new()
        };
        Ok(json!({ "ok": true, "records": records }))
    }
}

pub struct CognitionTurnWorkerCancelTool;

#[async_trait]
impl StasisTool for CognitionTurnWorkerCancelTool {
    fn name(&self) -> &'static str {
        COGNITION_TURN_WORKER_CANCEL
    }

    fn description(&self) -> Option<&'static str> {
        Some("Mark a pending or running turn worker as cancelled (best-effort; in-flight worker may still finish).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": { "type": "string" }
            },
            "required": ["work_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let work_id = input
            .get("work_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure("cognition_turn_worker_cancel: work_id required".to_string())
            })?;
        let store = turn_worker_store();
        let updated = store
            .update(work_id, |r| {
                if matches!(
                    r.status,
                    TurnWorkStatus::Pending | TurnWorkStatus::Running
                ) {
                    r.status = TurnWorkStatus::Cancelled;
                }
            })
            .ok_or_else(|| StasisError::PortFailure(format!("work_id not found: {work_id}")))?;
        Ok(json!({ "ok": true, "record": updated }))
    }
}

pub fn register_turn_worker_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionSpawnTurnWorkerTool::new(scheduler))?;
    registry.register_tool(CognitionTurnWorkerStatusTool)?;
    registry.register_tool(CognitionTurnWorkerCancelTool)?;
    Ok(())
}
