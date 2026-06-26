//! Structured tool-run streaming helpers (P1 presentation layer).

use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
use uuid::Uuid;

use crate::daemon_api::StreamToolArtifactRef;
use crate::payload_receipt::ArtifactReceiptMeta;

use super::prompt_prep::truncate_text_for_budget;
use super::stream_sink::SharedAgentStreamSink;
use super::turn_context;

const SUMMARY_MAX_CHARS: usize = 160;

pub fn new_tool_run_id() -> String {
    format!("tr-{}", Uuid::new_v4().simple())
}

pub fn summarize_tool_input(tool_name: &str, tool_input: &serde_json::Value) -> String {
    if crate::turn_control_tools::is_finish_turn_tool_name(tool_name) {
        return "Final answer".to_string();
    }
    if crate::turn_control_tools::is_checkpoint_turn_tool_name(tool_name) {
        return tool_input
            .get("message")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS))
            .unwrap_or_else(|| "Checkpoint".to_string());
    }
    if crate::turn_control_tools::is_begin_work_tool_name(tool_name) {
        return tool_input
            .get("message")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS))
            .unwrap_or_else(|| "Starting work".to_string());
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

pub fn summarize_tool_output(tool_name: &str, tool_output: &serde_json::Value) -> Option<String> {
    if crate::turn_control_tools::is_finish_turn_tool_name(tool_name) {
        return tool_output
            .get("reason")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| truncate_text_for_budget(value, SUMMARY_MAX_CHARS))
            .or_else(|| Some("Committed final answer".to_string()));
    }
    if crate::turn_control_tools::is_checkpoint_turn_tool_name(tool_name) {
        return Some("Checkpoint sent".to_string());
    }
    if crate::turn_control_tools::is_begin_work_tool_name(tool_name) {
        return Some("Progress noted".to_string());
    }
    if crate::ui_present_tools::is_ui_present_cognition_tool(tool_name)
        || tool_name == crate::artifact_tools::COGNITION_ARTIFACT_WRITE
    {
        if matches!(tool_output.get("ok").and_then(|value| value.as_bool()), Some(false)) {
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
    if let Some(receipt) = input_receipt {
        if let Ok(record) = crate::artifact_store::persist_tool_artifact(
            session_id,
            tool_name,
            "input",
            &receipt.hash64,
            receipt.byte_size,
            tool_input,
        ) {
            for item in refs.iter_mut().filter(|item| item.role == "input") {
                item.artifact_id = Some(record.artifact_id.clone());
                item.label = Some(format!("{tool_name} input"));
            }
        }
    }
    if let Some(receipt) = output_receipt {
        if let Ok(record) = crate::artifact_store::persist_tool_artifact(
            session_id,
            tool_name,
            "output",
            &receipt.hash64,
            receipt.byte_size,
            tool_output,
        ) {
            for item in refs.iter_mut().filter(|item| item.role == "output") {
                item.artifact_id = Some(record.artifact_id.clone());
                item.label = Some(format!("{tool_name} output"));
            }
        }
    }
    refs
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

pub async fn emit_tool_run_started(
    sink: &SharedAgentStreamSink,
    tool_run_id: &str,
    tool_name: &str,
    tool_input: &serde_json::Value,
    tool_round: usize,
) {
    let input_summary = summarize_tool_input(tool_name, tool_input);
    sink.tool_run_started(
        tool_run_id.to_string(),
        tool_name.to_string(),
        input_summary,
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
            "cognition_turn_finish",
            &json!({"message": "Hello world", "reason": "done"}),
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
    fn tool_status_marks_error_outputs_failed() {
        assert_eq!(
            tool_status_from_output(&json!({"ok": false, "error": "nope"})),
            "failed"
        );
        assert_eq!(tool_status_from_output(&json!({"ok": true})), "succeeded");
    }
}
