//! Structured tool-run streaming helpers (P1 presentation layer).

use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
use uuid::Uuid;

use crate::daemon_api::StreamToolArtifactRef;
use crate::payload_receipt::ArtifactReceiptMeta;

use super::prompt_prep::truncate_text_for_budget;
use super::stream_sink::{SharedAgentStreamSink, ToolInputParam};
use super::turn_context;

const SUMMARY_MAX_CHARS: usize = 160;
const PARAM_VALUE_MAX_CHARS: usize = 120;
const PARAM_MAX_KEYS: usize = 6;

/// Keys surfaced first in tool evidence — the ones that usually carry intent.
const PARAM_PRIORITY_KEYS: &[&str] = &[
    "query",
    "task",
    "prompt",
    "action",
    "intent",
    "module",
    "capability",
    "reference",
    "title",
    "message",
    "path",
    "url",
];

pub fn new_tool_run_id() -> String {
    format!("tr-{}", Uuid::new_v4().simple())
}

pub fn summarize_tool_input(tool_name: &str, tool_input: &serde_json::Value) -> String {
    if crate::turn_control_tools::is_finish_turn_tool_name(tool_name, tool_input) {
        return "Final answer".to_string();
    }
    if crate::turn_control_tools::is_checkpoint_turn_tool_name(tool_name, tool_input) {
        return tool_input
            .get("message")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS))
            .unwrap_or_else(|| "Checkpoint".to_string());
    }
    if crate::turn_control_tools::is_begin_work_tool_name(tool_name, tool_input) {
        return tool_input
            .get("message")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS))
            .unwrap_or_else(|| "Starting work".to_string());
    }
    if crate::turn_control_tools::is_update_user_tool_name(tool_name, tool_input) {
        return tool_input
            .get("message")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS))
            .unwrap_or_else(|| "Update".to_string());
    }
    if crate::ui_present_tools::is_ui_present_cognition_tool(tool_name)
        || crate::artifact_tools::is_artifact_cognition_tool(tool_name)
    {
        return tool_input
            .get("title")
            .or_else(|| tool_input.get("artifact_id"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS))
            .unwrap_or_else(|| "HTML artifact".to_string());
    }
    if crate::ui_scene_tools::is_ui_scene_cognition_tool(tool_name) {
        let count = tool_input
            .get("ops")
            .and_then(|value| value.as_array())
            .map(|ops| ops.len())
            .unwrap_or(0);
        return format!("Scene · {count} ops");
    }
    if crate::ui_build_tools::is_ui_build_cognition_tool(tool_name) {
        let verb = tool_input
            .get("verb")
            .or_else(|| tool_input.get("op"))
            .and_then(|value| value.as_str())
            .unwrap_or("build");
        return format!("Liquid · {verb}");
    }

    for key in [
        "query",
        "task",
        "prompt",
        "action",
        "intent",
        "module",
        "capability",
        "reference",
        "title",
    ] {
        if let Some(value) = tool_input.get(key).and_then(|entry| entry.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return truncate_text_for_budget(trimmed, SUMMARY_MAX_CHARS);
            }
        }
    }
    truncate_text_for_budget(
        &serde_json::to_string(tool_input).unwrap_or_else(|_| tool_input.to_string()),
        SUMMARY_MAX_CHARS,
    )
}

/// Redacted key/value preview of a tool's arguments for chat-adjacent evidence.
///
/// This is what lets the UI say `query: "newest Qwen models"` instead of just
/// naming the tool. Intent-bearing keys come first, the rest are alphabetical so
/// the order is stable across rounds.
pub fn preview_tool_input(tool_input: &serde_json::Value) -> Vec<ToolInputParam> {
    let redacted = crate::settings_guard::redact_json_value(tool_input);
    let Some(object) = redacted.as_object() else {
        return Vec::new();
    };

    let mut ordered: Vec<&String> = Vec::with_capacity(object.len());
    for key in PARAM_PRIORITY_KEYS {
        if let Some((found, _)) = object.get_key_value(*key) {
            ordered.push(found);
        }
    }
    let mut rest: Vec<&String> = object
        .keys()
        .filter(|key| !PARAM_PRIORITY_KEYS.contains(&key.as_str()))
        .collect();
    rest.sort();
    ordered.extend(rest);

    let mut params = Vec::new();
    for key in ordered {
        if params.len() >= PARAM_MAX_KEYS {
            break;
        }
        let Some(value) = object.get(key).and_then(param_value_text) else {
            continue;
        };
        let truncated = value.chars().count() > PARAM_VALUE_MAX_CHARS;
        params.push(ToolInputParam {
            key: key.clone(),
            value: truncate_text_for_budget(&value, PARAM_VALUE_MAX_CHARS),
            truncated,
        });
    }
    params
}

/// Flatten one argument to display text, dropping anything with nothing to show.
fn param_value_text(value: &serde_json::Value) -> Option<String> {
    let text = match value {
        serde_json::Value::Null => return None,
        serde_json::Value::String(raw) => raw.trim().to_string(),
        serde_json::Value::Array(items) if items.is_empty() => return None,
        serde_json::Value::Object(fields) if fields.is_empty() => return None,
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    };
    (!text.is_empty()).then_some(text)
}

pub fn summarize_tool_output(tool_name: &str, tool_output: &serde_json::Value) -> Option<String> {
    if matches!(
        tool_output
            .get("finish_turn")
            .and_then(|value| value.as_bool()),
        Some(true)
    ) {
        return tool_output
            .get("reason")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS))
            .or_else(|| Some("Committed final answer".to_string()));
    }
    if matches!(
        tool_output
            .get("checkpoint_turn")
            .and_then(|value| value.as_bool()),
        Some(true)
    ) {
        return Some("Checkpoint sent".to_string());
    }
    if matches!(
        tool_output
            .get("workshop_entered")
            .and_then(|value| value.as_bool()),
        Some(true)
    ) || matches!(
        tool_output
            .get("begin_work")
            .and_then(|value| value.as_bool()),
        Some(true)
    ) {
        return Some("Progress noted".to_string());
    }
    if matches!(
        tool_output
            .get("update_user")
            .and_then(|value| value.as_bool()),
        Some(true)
    ) {
        return Some("Update sent".to_string());
    }
    if crate::ui_scene_tools::is_ui_scene_cognition_tool(tool_name) {
        if matches!(
            tool_output.get("ok").and_then(|value| value.as_bool()),
            Some(false)
        ) {
            return tool_output
                .get("error")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS));
        }
        return Some("Scene updated".to_string());
    }
    if crate::ui_build_tools::is_ui_build_cognition_tool(tool_name) {
        if matches!(
            tool_output.get("ok").and_then(|value| value.as_bool()),
            Some(false)
        ) {
            return tool_output
                .get("error")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS));
        }
        return tool_output
            .get("preview")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS))
            .or_else(|| Some("Liquid updated".to_string()));
    }
    if crate::ui_present_tools::is_ui_present_cognition_tool(tool_name)
        || tool_name == crate::artifact_tools::COGNITION_ARTIFACT_WRITE
    {
        if matches!(
            tool_output.get("ok").and_then(|value| value.as_bool()),
            Some(false)
        ) {
            return tool_output
                .get("error")
                .and_then(|value| value.as_str())
                .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS));
        }
        return tool_output
            .get("label")
            .and_then(|value| value.as_str())
            .map(|label| {
                if tool_name == crate::artifact_tools::COGNITION_ARTIFACT_WRITE
                    && tool_output
                        .get("previous_artifact_id")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| !value.trim().is_empty())
                {
                    format!("Updated {label}")
                } else {
                    format!("Presented {label}")
                }
            });
    }

    if let Some(hint) = turn_context::compact_tool_receipt_hint(tool_name, tool_output) {
        return Some(truncate_text_for_budget(&hint, SUMMARY_MAX_CHARS));
    }
    if let Some(error) = tool_output.get("error").and_then(|value| value.as_str()) {
        return Some(truncate_text_for_budget(error, SUMMARY_MAX_CHARS));
    }
    if tool_output.is_string() {
        return tool_output
            .as_str()
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS));
    }
    None
}

pub fn tool_status_from_output(tool_output: &serde_json::Value) -> &'static str {
    if matches!(
        tool_output.get("ok").and_then(|value| value.as_bool()),
        Some(false)
    ) {
        return "failed";
    }
    if tool_output.get("error").is_some() {
        return "failed";
    }
    if matches!(
        tool_output
            .get("persisted_verified")
            .and_then(|value| value.as_bool()),
        Some(false)
    ) && matches!(
        tool_output
            .get("committed")
            .and_then(|value| value.as_bool()),
        Some(true)
    ) {
        return "failed";
    }
    if matches!(
        tool_output
            .get("committed")
            .and_then(|value| value.as_bool()),
        Some(false)
    ) && !matches!(
        tool_output
            .get("requires_confirmation")
            .and_then(|value| value.as_bool()),
        Some(true)
    ) {
        return "failed";
    }
    "succeeded"
}

pub fn artifact_refs_from_receipts(
    input_receipt: Option<&ArtifactReceiptMeta>,
    output_receipt: Option<&ArtifactReceiptMeta>,
) -> Vec<StreamToolArtifactRef> {
    let mut refs = Vec::new();
    if let Some(receipt) = input_receipt {
        refs.push(StreamToolArtifactRef {
            role: "input".to_string(),
            content_type: receipt.content_type.clone(),
            byte_size: receipt.byte_size,
            hash64: receipt.hash64.clone(),
            artifact_id: None,
            label: None,
        });
    }
    if let Some(receipt) = output_receipt {
        refs.push(StreamToolArtifactRef {
            role: "output".to_string(),
            content_type: receipt.content_type.clone(),
            byte_size: receipt.byte_size,
            hash64: receipt.hash64.clone(),
            artifact_id: None,
            label: None,
        });
    }
    refs
}

pub fn persist_and_enrich_artifact_refs(
    session_id: &str,
    tool_name: &str,
    tool_input: &serde_json::Value,
    tool_output: &serde_json::Value,
    input_receipt: Option<&ArtifactReceiptMeta>,
    output_receipt: Option<&ArtifactReceiptMeta>,
    mut refs: Vec<StreamToolArtifactRef>,
) -> Vec<StreamToolArtifactRef> {
    if tool_payload_is_requeryable(tool_name) {
        for item in &mut refs {
            item.label = Some(format!(
                "{tool_name} {} — re-queryable, not persisted",
                item.role
            ));
        }
        return refs;
    }
    if let Some(receipt) = input_receipt
        && let Ok(record) = crate::artifact_store::persist_tool_artifact(
            session_id,
            tool_name,
            "input",
            &receipt.hash64,
            receipt.byte_size,
            tool_input,
        )
    {
        for item in refs.iter_mut().filter(|item| item.role == "input") {
            item.artifact_id = Some(record.artifact_id.clone());
            item.label = Some(format!("{tool_name} input"));
        }
    }
    if let Some(receipt) = output_receipt
        && let Ok(record) = crate::artifact_store::persist_tool_artifact(
            session_id,
            tool_name,
            "output",
            &receipt.hash64,
            receipt.byte_size,
            tool_output,
        )
    {
        for item in refs.iter_mut().filter(|item| item.role == "output") {
            item.artifact_id = Some(record.artifact_id.clone());
            item.label = Some(format!("{tool_name} output"));
        }
    }
    refs
}

pub fn tool_payload_is_requeryable(tool_name: &str) -> bool {
    crate::code_intelligence_tools::is_code_cognition_tool(tool_name)
        || crate::detamu_tools::is_detamu_cognition_tool(tool_name)
        || tool_name == crate::public_api::COGNITION_STORE_READ
        || tool_name == crate::coding_tools::COGNITION_SHELL_SESSION_STATUS
        || tool_name == super::coder_tools::COGNITION_CODER_EVIDENCE_READ
}

pub fn ui_artifact_from_tool_output(
    tool_output: &serde_json::Value,
) -> Option<crate::daemon_api::StreamUiArtifact> {
    if tool_output.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return None;
    }
    let artifact_id = tool_output
        .get("artifact_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let label = tool_output
        .get("label")
        .and_then(|value| value.as_str())
        .or_else(|| tool_output.get("title").and_then(|value| value.as_str()))
        .unwrap_or("Artifact")
        .to_string();
    let mime = tool_output
        .get("mime")
        .and_then(|value| value.as_str())
        .unwrap_or("text/html")
        .to_string();
    let presentation = tool_output
        .get("presentation")
        .and_then(|value| value.as_str())
        .unwrap_or("inline")
        .to_string();
    let byte_size = tool_output
        .get("byte_size")
        .and_then(|value| value.as_u64());
    let height_px = tool_output
        .get("height_px")
        .or_else(|| tool_output.get("height"))
        .and_then(|value| value.as_u64())
        .map(|value| value as u32);
    Some(crate::daemon_api::StreamUiArtifact {
        artifact_id,
        mime,
        label,
        presentation,
        byte_size,
        height_px,
    })
}

/// Extract a Liquid UI scene batch from a `cognition_ui_scene` tool result.
/// Requires `ok == true` and a non-empty `ops` array; ops are forwarded verbatim.
pub fn scene_ops_from_tool_output(
    tool_output: &serde_json::Value,
) -> Option<crate::daemon_api::StreamUiScene> {
    if tool_output.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return None;
    }
    let ops: Vec<serde_json::Value> = tool_output
        .get("ops")
        .and_then(|value| value.as_array())
        .filter(|ops| !ops.is_empty())?
        .clone();
    let surface_id = tool_output
        .get("surface_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let rev = tool_output.get("rev").and_then(|value| value.as_i64());
    Some(crate::daemon_api::StreamUiScene {
        turn_id: None,
        surface_id,
        rev,
        ops,
    })
}

pub async fn emit_tool_run_started(
    sink: &SharedAgentStreamSink,
    tool_run_id: &str,
    tool_name: &str,
    tool_input: &serde_json::Value,
    tool_round: usize,
) {
    let input_summary = summarize_tool_input(tool_name, tool_input);
    let input_params = preview_tool_input(tool_input);
    sink.tool_run_started(
        tool_run_id.to_string(),
        tool_name.to_string(),
        input_summary,
        input_params,
        tool_round,
    )
    .await;
}

pub async fn emit_tool_run_finished(
    sink: &SharedAgentStreamSink,
    tool_run_id: &str,
    tool_round: usize,
    invocation: &ToolInvocation,
    input_receipt: Option<ArtifactReceiptMeta>,
    output_receipt: Option<ArtifactReceiptMeta>,
) {
    let input_summary = summarize_tool_input(&invocation.tool_name, &invocation.tool_input);
    let status = tool_status_from_output(&invocation.tool_output);
    let output_summary = summarize_tool_output(&invocation.tool_name, &invocation.tool_output);
    sink.tool_run_finished(
        tool_run_id.to_string(),
        invocation.tool_name.clone(),
        status.to_string(),
        input_summary,
        output_summary,
        invocation.tool_input.clone(),
        invocation.tool_output.clone(),
        input_receipt,
        output_receipt,
        tool_round,
    )
    .await;
}

#[derive(Clone)]
pub struct DaemonToolRunEventPort {
    sink: SharedAgentStreamSink,
}

impl DaemonToolRunEventPort {
    pub fn new(sink: SharedAgentStreamSink) -> Self {
        Self { sink }
    }
}

impl medousa_runtime::ToolRunEventPort for DaemonToolRunEventPort {
    fn started(
        &self,
        event: medousa_runtime::ToolRunStart,
    ) -> medousa_runtime::RuntimePortFuture<String> {
        let sink = self.sink.clone();
        Box::pin(async move {
            let tool_run_id = new_tool_run_id();
            emit_tool_run_started(
                &sink,
                &tool_run_id,
                &event.tool_name,
                &event.tool_input,
                event.tool_round,
            )
            .await;
            tool_run_id
        })
    }

    fn finished(
        &self,
        event: medousa_runtime::ToolRunFinish,
    ) -> medousa_runtime::RuntimePortFuture<()> {
        let sink = self.sink.clone();
        Box::pin(async move {
            let safe_input = crate::settings_guard::redact_json_value(&event.invocation.tool_input);
            let safe_output =
                crate::settings_guard::redact_json_value(&event.invocation.tool_output);
            emit_tool_run_finished(
                &sink,
                &event.tool_run_id,
                event.tool_round,
                &event.invocation,
                crate::payload_receipt::receipt_meta(
                    &safe_input,
                    crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                ),
                crate::payload_receipt::receipt_meta(
                    &safe_output,
                    crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                ),
            )
            .await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarize_tool_input_prefers_query_field() {
        let summary = summarize_tool_input(
            "cognition_web_search",
            &json!({"query": "weather in NYC", "limit": 3}),
        );
        assert!(summary.contains("weather in NYC"));
    }

    #[test]
    fn summarize_turn_finish_input_avoids_raw_json() {
        let summary = summarize_tool_input(
            "cognition_turn",
            &json!({"action": "turn.finish", "message": "Hello world", "reason": "done"}),
        );
        assert_eq!(summary, "Final answer");
    }

    #[test]
    fn summarize_ui_present_output_uses_label() {
        let summary = summarize_tool_output(
            "cognition_ui_present",
            &json!({"ok": true, "label": "Session Recap"}),
        );
        assert_eq!(summary.as_deref(), Some("Presented Session Recap"));
    }

    #[test]
    fn scene_ops_from_tool_output_reads_ops_and_meta() {
        let scene = scene_ops_from_tool_output(&json!({
            "ok": true,
            "ops": [{ "op": "plan_layout" }, { "op": "fill_slot" }],
            "surface_id": "chat:turn-1",
            "rev": 2
        }))
        .expect("scene");
        assert_eq!(scene.ops.len(), 2);
        assert_eq!(scene.surface_id.as_deref(), Some("chat:turn-1"));
        assert_eq!(scene.rev, Some(2));
    }

    #[test]
    fn scene_ops_from_tool_output_rejects_not_ok_or_empty() {
        assert!(
            scene_ops_from_tool_output(&json!({ "ok": false, "ops": [{ "op": "x" }] })).is_none()
        );
        assert!(scene_ops_from_tool_output(&json!({ "ok": true, "ops": [] })).is_none());
        assert!(scene_ops_from_tool_output(&json!({ "ok": true })).is_none());
    }

    #[test]
    fn summarize_ui_scene_output() {
        let summary = summarize_tool_output("cognition_ui_scene", &json!({"ok": true}));
        assert_eq!(summary.as_deref(), Some("Scene updated"));
    }

    #[test]
    fn tool_status_marks_error_outputs_failed() {
        assert_eq!(
            tool_status_from_output(&json!({"ok": false, "error": "nope"})),
            "failed"
        );
        assert_eq!(tool_status_from_output(&json!({"ok": true})), "succeeded");
        assert_eq!(
            tool_status_from_output(&json!({"committed": false})),
            "failed"
        );
        assert_eq!(
            tool_status_from_output(&json!({
                "committed": false,
                "requires_confirmation": true
            })),
            "succeeded"
        );
    }

    /// The headline case: chat should show what the agent actually searched for.
    #[test]
    fn preview_tool_input_surfaces_query_first() {
        let params = preview_tool_input(&json!({
            "limit": 3,
            "query": "newest Qwen models"
        }));
        assert_eq!(params[0].key, "query");
        assert_eq!(params[0].value, "newest Qwen models");
        assert!(!params[0].truncated);
        assert_eq!(params[1].key, "limit");
        assert_eq!(params[1].value, "3");
    }

    #[test]
    fn preview_tool_input_truncates_long_values_and_flags_them() {
        let long = "x".repeat(PARAM_VALUE_MAX_CHARS + 40);
        let params = preview_tool_input(&json!({ "prompt": long }));
        assert!(params[0].truncated);
        assert!(params[0].value.chars().count() <= PARAM_VALUE_MAX_CHARS);
    }

    #[test]
    fn preview_tool_input_caps_key_count_and_drops_empties() {
        let params = preview_tool_input(&json!({
            "a": 1, "b": 2, "c": 3, "d": 4, "e": 5, "f": 6, "g": 7, "h": 8
        }));
        assert_eq!(params.len(), PARAM_MAX_KEYS);

        let sparse = preview_tool_input(&json!({
            "query": "ok", "empty": "", "nothing": null, "none": []
        }));
        assert_eq!(sparse.len(), 1);
        assert_eq!(sparse[0].key, "query");
    }

    /// Non-object arguments have no key/value shape; the summary covers those.
    #[test]
    fn preview_tool_input_ignores_non_objects() {
        assert!(preview_tool_input(&json!("just a string")).is_empty());
        assert!(preview_tool_input(&json!([1, 2, 3])).is_empty());
    }

    #[test]
    fn code_and_detamu_payloads_are_requeried_instead_of_persisted() {
        assert!(tool_payload_is_requeryable(
            crate::public_api::COGNITION_STORE_READ
        ));
        assert!(tool_payload_is_requeryable("cognition_code_diagnostics"));
        assert!(tool_payload_is_requeryable("cognition_detamu_impact"));
        assert!(tool_payload_is_requeryable(
            "cognition_shell_session_status"
        ));
        assert!(tool_payload_is_requeryable(
            super::super::coder_tools::COGNITION_CODER_EVIDENCE_READ
        ));
        assert!(!tool_payload_is_requeryable("cognition_shell_session_run"));
    }
}
