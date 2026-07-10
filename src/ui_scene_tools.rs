//! Liquid UI scene tool (`cognition_ui_scene`) for surfaces that opt in via
//! `TurnSurfaceContext.supports_ui_artifacts`.
//!
//! Where `cognition_ui_present` ships an opaque HTML blob, this tool lets the
//! model author a *native, structured* scene as a batch of scene operations
//! (`plan_layout` / `fill_slot` / …). Ops are echoed back verbatim and forwarded
//! as a `ui_scene` stream event; the client decodes + validates them (the daemon
//! never inspects their shape). The model can go bones-first: emit a
//! `plan_layout` with skeleton slots, then follow up with `fill_slot` batches.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::turn_continuation::TurnContinuationScope;

pub const COGNITION_UI_SCENE: &str = "cognition_ui_scene";

pub const UI_SCENE_COGNITION_TOOLS: &[&str] = &[COGNITION_UI_SCENE];

pub fn is_ui_scene_cognition_tool(name: &str) -> bool {
    name == COGNITION_UI_SCENE
}

pub fn register_ui_scene_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionUiSceneTool::new(turn_scope))?;
    Ok(())
}

pub struct CognitionUiSceneTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionUiSceneTool {
    pub fn new(turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>) -> Self {
        Self { turn_scope }
    }

    async fn active_surface_supports_ui_artifacts(&self) -> bool {
        self.turn_scope
            .read()
            .await
            .as_ref()
            .is_some_and(|scope| scope.supports_ui_artifacts)
    }
}

#[async_trait]
impl StasisTool for CognitionUiSceneTool {
    fn name(&self) -> &'static str {
        COGNITION_UI_SCENE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Author a native, streamable scene (structure-then-fill) when the client advertises supports_ui_artifacts. \
             Preferred over cognition_ui_present for interactive UI — the model composes typed nodes, not HTML. \
             Emit ops in the scene-op JSON shape; go bones-first: send a plan_layout with skeleton slots, then follow up \
             with fill_slot batches (call again in the same turn) so structure paints before content streams in. \
             Keep each call small (plan_layout first, then 1–3 fill_slot ops per follow-up) — ops must be valid JSON. \
             Ops: plan_layout, fill_slot, patch_props, set_binding, set_fill_state, precompute, remove. \
             Each op is an object with a string op field; nodes carry id (stable reconciliation key), type (archetype), and props.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["ops"],
            "properties": {
                "ops": {
                    "type": "array",
                    "description": "Ordered scene operations. Each item is an object with a string op field (plan_layout, fill_slot, patch_props, set_binding, set_fill_state, precompute, remove). Keep batches small — plan_layout first, then fill_slot in follow-up calls.",
                    "minItems": 1,
                    "maxItems": 12,
                    "items": { "type": "object" }
                },
                "surface_id": {
                    "type": "string",
                    "description": "Optional scene surface id. Defaults to the chat turn surface."
                },
                "rev": {
                    "type": "integer",
                    "description": "Owning plan_layout revision for ordering (optional)."
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        if !self.active_surface_supports_ui_artifacts().await {
            return Ok(json!({
                "ok": false,
                "unsupported_surface": true,
                "error": "This channel does not support UI scenes (supports_ui_artifacts=false). Answer in markdown instead.",
            }));
        }

        let ops = input
            .get("ops")
            .and_then(Value::as_array)
            .ok_or_else(|| StasisError::PortFailure("ops must be an array".to_string()))?;
        if ops.is_empty() {
            return Err(StasisError::PortFailure("ops must not be empty".to_string()));
        }
        if ops.len() > 12 {
            return Err(StasisError::PortFailure(
                "ops batch too large (max 12) — send plan_layout first, then fill_slot in follow-up calls"
                    .to_string(),
            ));
        }
        for (index, op) in ops.iter().enumerate() {
            let has_op = op
                .get("op")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty());
            if !has_op {
                return Err(StasisError::PortFailure(format!(
                    "ops[{index}] must be an object with a string `op` field"
                )));
            }
        }

        let mut response = json!({
            "ok": true,
            "ops": ops.clone(),
            "op_count": ops.len(),
        });
        if let Some(surface_id) = input
            .get("surface_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            response["surface_id"] = json!(surface_id);
        }
        if let Some(rev) = input.get("rev").and_then(Value::as_i64) {
            response["rev"] = json!(rev);
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_api::TurnSurfaceContext;

    fn scope(supports: bool) -> Arc<RwLock<Option<TurnContinuationScope>>> {
        Arc::new(RwLock::new(Some(TurnContinuationScope {
            turn_correlation_id: "turn-1".to_string(),
            session_id: "medousa-home".to_string(),
            original_prompt: "hi".to_string(),
            delivery_target: None,
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            response_depth_mode: "standard".to_string(),
            supports_ui_artifacts: supports,
            supports_browser_host: false,
            channel_surface: Some("home-desktop".to_string()),
        })))
    }

    #[test]
    fn surface_context_flag_default_is_false() {
        assert!(!TurnSurfaceContext::tui().supports_ui_artifacts);
    }

    #[tokio::test]
    async fn rejects_unsupported_surface() {
        let tool = CognitionUiSceneTool::new(scope(false));
        let out = tool
            .invoke(json!({ "ops": [{ "op": "plan_layout" }] }))
            .await
            .expect("invoke");
        assert_eq!(out.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(out.get("unsupported_surface").and_then(Value::as_bool), Some(true));
    }

    #[tokio::test]
    async fn echoes_ops_on_supported_surface() {
        let tool = CognitionUiSceneTool::new(scope(true));
        let out = tool
            .invoke(json!({
                "ops": [{ "op": "plan_layout", "surfaceId": "s", "rev": 1, "root": { "id": "r", "type": "stack" } }],
                "surface_id": "chat:turn-1",
                "rev": 1
            }))
            .await
            .expect("invoke");
        assert_eq!(out.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(out.get("op_count").and_then(Value::as_u64), Some(1));
        assert_eq!(out.get("surface_id").and_then(Value::as_str), Some("chat:turn-1"));
    }

    #[tokio::test]
    async fn rejects_ops_without_op_field() {
        let tool = CognitionUiSceneTool::new(scope(true));
        let err = tool
            .invoke(json!({ "ops": [{ "notop": true }] }))
            .await
            .expect_err("should reject");
        assert!(err.to_string().contains("op"));
    }

    #[tokio::test]
    async fn rejects_oversized_ops_batch() {
        let tool = CognitionUiSceneTool::new(scope(true));
        let ops: Vec<Value> = (0..13)
            .map(|i| json!({ "op": "patch_props", "nodeId": format!("n{i}") }))
            .collect();
        let err = tool
            .invoke(json!({ "ops": ops }))
            .await
            .expect_err("should reject");
        assert!(err.to_string().contains("max 12"));
    }
}
