//! Ephemeral model-facing tool observation budgets.
//!
//! Authoritative tool outputs remain unchanged for execution receipts and UI
//! delivery. This module only compiles the copy placed back into the model's
//! current tool loop, and never persists payloads.

use std::collections::HashMap;

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub const PERCEPTION_ROUND_MAX_CHARS: usize = 96 * 1024;
pub const PERCEPTION_CONTEXT_RESERVE_CHARS: usize = 24 * 1024;
pub const PERCEPTION_TOOL_RESULTS_MAX_CHARS: usize =
    PERCEPTION_ROUND_MAX_CHARS - PERCEPTION_CONTEXT_RESERVE_CHARS;
pub const PERCEPTION_RESULT_MAX_CHARS: usize = 48 * 1024;

const MIN_ACTIONABLE_RESULT_CHARS: usize = 1_024;
const PRIORITY_FIELDS: &[&str] = &[
    "ok",
    "error",
    "hint",
    "recoverable",
    "read_status",
    "path",
    "root",
    "bytes",
    "total_lines",
    "digest",
    "coverage",
    "orientation",
    "encoding",
    "status",
    "exit_code",
    "artifact_id",
    "reference",
    "next",
];

#[derive(Debug, Default)]
pub struct ToolPerceptionGovernor {
    failure_occurrences: HashMap<String, usize>,
}

impl ToolPerceptionGovernor {
    /// Allocate one deterministic result ceiling from the fixed round pool.
    /// Equal allocation means parallel completion order cannot change budgets.
    pub fn result_budget_for_batch(&self, result_count: usize) -> usize {
        let result_count = result_count.max(1);
        (PERCEPTION_TOOL_RESULTS_MAX_CHARS / result_count).min(PERCEPTION_RESULT_MAX_CHARS)
    }

    /// Compile one authoritative tool output into its ephemeral model-facing
    /// observation. The input value is never mutated or retained.
    pub fn observe(&mut self, tool_name: &str, output: &Value, max_chars: usize) -> Value {
        if max_chars < MIN_ACTIONABLE_RESULT_CHARS {
            return minimal_observation(tool_name, output, max_chars);
        }
        if is_failure(output) {
            let signature = failure_signature(tool_name, output);
            let occurrences = self
                .failure_occurrences
                .entry(signature.clone())
                .or_insert(0);
            *occurrences = occurrences.saturating_add(1);
            if *occurrences > 1 {
                return fit_failure_cluster(tool_name, output, &signature, *occurrences, max_chars);
            }
        }

        let rendered = output.to_string();
        if rendered.chars().count() <= max_chars {
            return output.clone();
        }
        fit_bounded_observation(tool_name, output, rendered, max_chars)
    }

    /// Hard backstop for mode-owned world refreshes. Providers are expected to
    /// compile focused context themselves; this only prevents an anomalous
    /// refresh from escaping the global round envelope.
    pub fn observe_round_context(&self, context: &str) -> String {
        let original_chars = context.chars().count();
        if original_chars <= PERCEPTION_CONTEXT_RESERVE_CHARS {
            return context.to_string();
        }
        let mut preview_chars = PERCEPTION_CONTEXT_RESERVE_CHARS / 3;
        loop {
            let bounded = json!({
                "perception_status": "bounded_round_context",
                "reason": "mode_context_exceeds_round_reserve",
                "original_chars": original_chars,
                "context_limit_chars": PERCEPTION_CONTEXT_RESERVE_CHARS,
                "preview_head": take_chars(context, preview_chars),
                "preview_tail": take_last_chars(context, preview_chars),
                "next_decision": "Use the visible pointers and focused discovery tools to resolve omitted world context; do not request the same broad refresh again.",
            })
            .to_string();
            if bounded.chars().count() <= PERCEPTION_CONTEXT_RESERVE_CHARS {
                return bounded;
            }
            preview_chars /= 2;
        }
    }
}

fn minimal_observation(tool_name: &str, output: &Value, max_chars: usize) -> Value {
    let value = json!({
        "ok": output.get("ok").cloned().unwrap_or(Value::Bool(true)),
        "perception_status": "round_batch_too_wide",
        "tool": tool_name,
        "next_decision": "Reduce the number of tools in the next batch and retry only the focused calls still needed.",
    });
    if value.to_string().chars().count() <= max_chars {
        value
    } else {
        json!({
            "perception_status": "round_batch_too_wide",
            "next_decision": "Use a smaller tool batch."
        })
    }
}

fn is_failure(output: &Value) -> bool {
    matches!(output.get("ok").and_then(Value::as_bool), Some(false))
        || output.get("error").is_some()
}

fn failure_signature(tool_name: &str, output: &Value) -> String {
    let error = output
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("tool returned failure");
    let mut hasher = Sha256::new();
    hasher.update(tool_name.as_bytes());
    hasher.update([0]);
    hasher.update(
        error
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .as_bytes(),
    );
    let digest = format!("{:x}", hasher.finalize());
    format!("sha256:{}", &digest[..16])
}

fn fit_failure_cluster(
    tool_name: &str,
    output: &Value,
    signature: &str,
    occurrences: usize,
    max_chars: usize,
) -> Value {
    let error = output
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("tool returned failure");
    let hint = output.get("hint").and_then(Value::as_str);
    let mut field_budget = max_chars.saturating_sub(512).max(128) / 2;
    loop {
        let value = json!({
            "ok": false,
            "perception_status": "failure_cluster",
            "tool": tool_name,
            "failure_signature": signature,
            "occurrences_this_turn": occurrences,
            "error": truncate_middle(error, field_budget),
            "hint": hint.map(|value| truncate_middle(value, field_budget)),
            "next_decision": "This failure repeated in the current tool loop. Change the arguments or approach before retrying; use the preserved error and hint as the recovery boundary.",
        });
        if value.to_string().chars().count() <= max_chars || field_budget <= 128 {
            return value;
        }
        field_budget /= 2;
    }
}

fn fit_bounded_observation(
    tool_name: &str,
    output: &Value,
    rendered: String,
    max_chars: usize,
) -> Value {
    let original_chars = rendered.chars().count();
    let available_fields = output
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut preview_chars = max_chars.saturating_sub(1_024).max(256) / 2;
    let mut field_chars = (max_chars / PRIORITY_FIELDS.len().max(1)).clamp(128, 4_096);

    loop {
        let preserved = priority_fields(output, field_chars);
        let value = json!({
            "ok": output.get("ok").cloned().unwrap_or(Value::Bool(true)),
            "perception_status": "bounded",
            "tool": tool_name,
            "reason": "tool_result_exceeds_model_context_budget",
            "original_chars": original_chars,
            "result_limit_chars": max_chars,
            "preserved": preserved,
            "available_fields": available_fields,
            "payload_preview": {
                "head": take_chars(&rendered, preview_chars),
                "tail": take_last_chars(&rendered, preview_chars),
            },
            "next_decision": next_decision(output),
        });
        if value.to_string().chars().count() <= max_chars {
            return value;
        }
        if preview_chars > 128 {
            preview_chars /= 2;
        } else if field_chars > 128 {
            field_chars /= 2;
        } else {
            return json!({
                "ok": output.get("ok").cloned().unwrap_or(Value::Bool(true)),
                "perception_status": "bounded",
                "tool": tool_name,
                "reason": "tool_result_exceeds_model_context_budget",
                "original_chars": original_chars,
                "result_limit_chars": max_chars,
                "next_decision": next_decision(output),
            });
        }
    }
}

fn priority_fields(output: &Value, max_field_chars: usize) -> Value {
    let Some(object) = output.as_object() else {
        return Value::Null;
    };
    let mut preserved = Map::new();
    for key in PRIORITY_FIELDS {
        if let Some(value) = object.get(*key) {
            preserved.insert((*key).to_string(), bound_field(value, max_field_chars));
        }
    }
    Value::Object(preserved)
}

fn bound_field(value: &Value, max_chars: usize) -> Value {
    let rendered = value.to_string();
    let original_chars = rendered.chars().count();
    if original_chars <= max_chars {
        return value.clone();
    }
    json!({
        "perception_status": "bounded_field",
        "original_chars": original_chars,
        "preview": truncate_middle(&rendered, max_chars.saturating_sub(96).max(32)),
    })
}

fn next_decision(output: &Value) -> &'static str {
    if output.get("orientation").is_some() {
        "Use preserved.orientation to make the next focused range or discovery call."
    } else if output.get("artifact_id").is_some() || output.get("reference").is_some() {
        "Follow the preserved artifact/reference with its focused read or search tool."
    } else if output.get("stdout").is_some() || output.get("stderr").is_some() {
        "Use the head/tail preview to choose a narrower command, search, or diagnostic query; do not repeat the same broad output call."
    } else {
        "Use preserved metadata and the head/tail preview to choose a narrower follow-up query instead of repeating the broad call."
    }
}

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    if max_chars < 24 {
        return take_chars(value, max_chars);
    }
    let marker = "…[bounded]…";
    let remaining = max_chars.saturating_sub(marker.chars().count());
    let head = remaining / 2;
    let tail = remaining.saturating_sub(head);
    format!(
        "{}{}{}",
        take_chars(value, head),
        marker,
        take_last_chars(value, tail)
    )
}

fn take_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn take_last_chars(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    value
        .chars()
        .skip(count.saturating_sub(max_chars))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_observation_is_unchanged() {
        let input = json!({"ok": true, "content": "small"});
        let mut governor = ToolPerceptionGovernor::default();
        assert_eq!(
            governor.observe("cognition_code_read", &input, 4_096),
            input
        );
    }

    #[test]
    fn oversized_observation_preserves_orientation_and_points_forward() {
        let input = json!({
            "ok": true,
            "path": "src/lib.rs",
            "root": "/worktree",
            "coverage": {"line_start": 1, "line_end": 200},
            "orientation": {"next_read": {"line_start": 201, "line_end": 400}},
            "content": "x".repeat(20_000),
        });
        let mut governor = ToolPerceptionGovernor::default();
        let observed = governor.observe("cognition_code_read", &input, 4_096);
        assert_eq!(observed["perception_status"], "bounded");
        assert_eq!(observed["preserved"]["path"], "src/lib.rs");
        assert_eq!(
            observed["preserved"]["orientation"]["next_read"]["line_start"],
            201
        );
        assert!(observed.to_string().chars().count() <= 4_096);
        assert!(
            observed["next_decision"]
                .as_str()
                .is_some_and(|value| value.contains("range"))
        );
    }

    #[test]
    fn repeated_failures_become_a_causal_cluster() {
        let failure = json!({
            "ok": false,
            "error": "compiler exited with status 1",
            "hint": "inspect the first diagnostic",
        });
        let mut governor = ToolPerceptionGovernor::default();
        assert_eq!(
            governor.observe("cognition_shell_session_run", &failure, 4_096),
            failure
        );
        let second = governor.observe("cognition_shell_session_run", &failure, 4_096);
        assert_eq!(second["perception_status"], "failure_cluster");
        assert_eq!(second["occurrences_this_turn"], 2);
        assert!(
            second["failure_signature"]
                .as_str()
                .is_some_and(|value| value.starts_with("sha256:"))
        );
    }

    #[test]
    fn batch_allocation_cannot_exceed_round_tool_pool() {
        let governor = ToolPerceptionGovernor::default();
        for count in [1, 2, 3, 8, 32, 128] {
            let budget = governor.result_budget_for_batch(count);
            assert!(budget <= PERCEPTION_RESULT_MAX_CHARS);
            assert!(budget.saturating_mul(count) <= PERCEPTION_TOOL_RESULTS_MAX_CHARS);
            let mut observations = ToolPerceptionGovernor::default();
            let observed = observations.observe(
                "cognition_test",
                &json!({"ok": true, "content": "x".repeat(100_000)}),
                budget,
            );
            assert!(observed.to_string().chars().count() <= budget);
        }
    }

    #[test]
    fn anomalous_mode_context_is_bounded_with_next_step_guidance() {
        let governor = ToolPerceptionGovernor::default();
        let context = format!(
            "head-pointer\n{}\ntail-pointer",
            "\"escaped\\context\"".repeat(4_000)
        );
        let observed = governor.observe_round_context(&context);
        assert!(observed.chars().count() <= PERCEPTION_CONTEXT_RESERVE_CHARS);
        let value: Value = serde_json::from_str(&observed).expect("bounded context json");
        assert_eq!(value["perception_status"], "bounded_round_context");
        assert!(
            value["preview_head"]
                .as_str()
                .is_some_and(|text| text.starts_with("head-pointer"))
        );
        assert!(
            value["preview_tail"]
                .as_str()
                .is_some_and(|text| text.ends_with("tail-pointer"))
        );
        assert!(value["next_decision"].as_str().is_some());
    }
}
