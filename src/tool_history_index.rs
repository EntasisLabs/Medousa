//! Cross-session tool-run index and slice → workflow promotion (W4).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde_json::{Value, json};

use crate::session::{ConversationTurn, load_history, list_history_sessions_page};
use crate::turn_parts::TurnPart;
use crate::turn_slice::{format_slice_id, parse_turn_index_from_slice_id, session_turn_index};
use crate::workflow::{WorkflowRunRequest, WorkflowStepSpec, new_workflow_id};

pub use medousa_types::tool_history::{
    ToolHistoryListQuery, ToolHistoryListResponse, ToolHistoryRunEntry, ToolHistorySliceRef,
    WorkflowFromSliceRequest, WorkflowFromSliceResponse,
};

const SECRET_KEY_MARKERS: &[&str] = &[
    "token",
    "secret",
    "password",
    "api_key",
    "apikey",
    "authorization",
    "bearer",
    "credential",
    "private_key",
];

pub fn hash_text(text: &str) -> String {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn looks_like_json(text: &str) -> bool {
    let trimmed = text.trim();
    (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
}

fn redact_value(value: &mut Value) -> bool {
    let mut redacted = false;
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let lower = key.to_ascii_lowercase();
                if SECRET_KEY_MARKERS
                    .iter()
                    .any(|marker| lower.contains(marker))
                {
                    *child = Value::String("[REDACTED]".to_string());
                    redacted = true;
                } else if redact_value(child) {
                    redacted = true;
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                if redact_value(item) {
                    redacted = true;
                }
            }
        }
        Value::String(text) => {
            let lower = text.to_ascii_lowercase();
            if lower.starts_with("bearer ") || lower.starts_with("sk-") {
                *text = "[REDACTED]".to_string();
                redacted = true;
            }
        }
        _ => {}
    }
    redacted
}

pub fn sanitize_tool_input(tool_name: &str, input_summary: &str) -> (Value, bool, String) {
    let mut value = if looks_like_json(input_summary) {
        serde_json::from_str(input_summary).unwrap_or_else(|_| {
            json!({ "summary": input_summary.trim(), "tool_name": tool_name })
        })
    } else {
        json!({ "summary": input_summary.trim(), "tool_name": tool_name })
    };
    let redacted = redact_value(&mut value);
    let serialized = serde_json::to_string(&value).unwrap_or_else(|_| input_summary.to_string());
    (value, redacted, hash_text(&serialized))
}

fn entry_id_for_run(
    session_id: &str,
    slice_id: &str,
    tool_round: usize,
    run_id: &str,
) -> String {
    format!("{session_id}:{slice_id}:r{tool_round}:{run_id}")
}

fn extract_runs_from_turn(
    session_id: &str,
    turns: &[ConversationTurn],
    turn: &ConversationTurn,
    session_preview: Option<&str>,
) -> Vec<ToolHistoryRunEntry> {
    let Some(turn_index) = session_turn_index(turns, turn) else {
        return Vec::new();
    };
    let slice_id = format_slice_id(turn_index);
    let Some(parts) = turn.parts.as_deref() else {
        return Vec::new();
    };

    parts
        .iter()
        .filter_map(|part| {
            let TurnPart::ToolRun {
                run_id,
                tool_name,
                status,
                input_summary,
                output_summary,
                tool_round,
                started_at,
                ..
            } = part
            else {
                return None;
            };
            let round = tool_round.unwrap_or(1);
            let (sanitized_input, redacted, args_hash) =
                sanitize_tool_input(tool_name, input_summary);
            Some(ToolHistoryRunEntry {
                entry_id: entry_id_for_run(session_id, &slice_id, round, run_id),
                session_id: session_id.to_string(),
                slice_id: slice_id.clone(),
                turn_index,
                tool_round: round,
                run_id: run_id.clone(),
                tool_name: tool_name.clone(),
                status: status.clone(),
                input_summary: input_summary.clone(),
                sanitized_input,
                args_hash,
                redacted,
                output_preview: output_summary.clone(),
                timestamp: *started_at,
                session_preview: session_preview.map(str::to_string),
            })
        })
        .collect()
}

pub fn list_tool_history_runs(query: &ToolHistoryListQuery) -> ToolHistoryListResponse {
    let run_limit = query.limit.unwrap_or(100).clamp(1, 500);
    let session_limit = query.session_limit.unwrap_or(40).clamp(1, 200);
    let session_filter = query
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let tool_filter = query
        .tool_filter
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let keyword = query
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    let mut runs = Vec::new();

    if let Some(session_id) = session_filter {
        let turns = load_history(session_id);
        let preview = turns
            .first()
            .map(|turn| turn.content.chars().take(72).collect::<String>());
        for turn in &turns {
            runs.extend(extract_runs_from_turn(
                session_id,
                &turns,
                turn,
                preview.as_deref(),
            ));
        }
    } else {
        let page = list_history_sessions_page(session_limit, None, None);
        for session in page.sessions {
            let session_id = session.session_id;
            let turns = load_history(&session_id);
            let preview = if session.preview.trim().is_empty() {
                None
            } else {
                Some(session.preview.as_str())
            };
            for turn in &turns {
                runs.extend(extract_runs_from_turn(
                    &session_id,
                    &turns,
                    turn,
                    preview,
                ));
            }
        }
    }

    runs.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));

    if let Some(tool) = tool_filter.as_ref() {
        runs.retain(|entry| entry.tool_name.to_ascii_lowercase().contains(tool.as_str()));
    }
    if let Some(kw) = keyword.as_ref() {
        runs.retain(|entry| {
            let haystack = format!(
                "{} {} {} {}",
                entry.tool_name,
                entry.input_summary,
                entry.slice_id,
                entry.session_id
            )
            .to_ascii_lowercase();
            haystack.contains(kw.as_str())
        });
    }

    runs.truncate(run_limit);
    let count = runs.len();
    ToolHistoryListResponse { count, runs }
}

pub fn resolve_tool_history_refs(
    refs: &[ToolHistorySliceRef],
) -> Result<Vec<ToolHistoryRunEntry>, String> {
    let mut resolved = Vec::new();
    for reference in refs {
        let session_id = reference.session_id.trim();
        let slice_id = reference.slice_id.trim();
        if session_id.is_empty() || slice_id.is_empty() {
            return Err("session_id and slice_id are required for each ref".to_string());
        }
        let turn_index = parse_turn_index_from_slice_id(slice_id)
            .ok_or_else(|| format!("invalid slice_id '{slice_id}'"))?;
        let turns = load_history(session_id);
        let turn = turns.get(turn_index.saturating_sub(1)).ok_or_else(|| {
            format!("turn index {turn_index} out of range for session '{session_id}'")
        })?;
        let mut runs = extract_runs_from_turn(session_id, &turns, turn, None);
        if let Some(round) = reference.tool_round {
            runs.retain(|entry| entry.tool_round == round);
        }
        if let Some(run_id) = reference
            .run_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            runs.retain(|entry| entry.run_id == run_id);
        }
        if runs.is_empty() {
            return Err(format!(
                "no tool runs matched session={session_id} slice={slice_id}"
            ));
        }
        resolved.extend(runs);
    }
    Ok(resolved)
}

pub fn promote_run_to_step(entry: &ToolHistoryRunEntry, step_id: &str) -> (WorkflowStepSpec, Vec<String>) {
    let mut notes = Vec::new();
    let tool = entry.tool_name.as_str();
    let input = &entry.sanitized_input;

    if tool.contains("grapheme") {
        let source = input
            .get("source")
            .and_then(Value::as_str)
            .or_else(|| input.get("summary").and_then(Value::as_str))
            .unwrap_or(entry.input_summary.as_str())
            .trim()
            .to_string();
        if source.len() < entry.input_summary.len() && !input.get("source").is_some() {
            notes.push(format!(
                "Grapheme source for {} may be truncated — review before scheduling.",
                entry.tool_name
            ));
        }
        return (
            WorkflowStepSpec::Grapheme {
                id: step_id.to_string(),
                source,
            },
            notes,
        );
    }

    if tool == "cognition_mcp_invoke" {
        let server_id = input
            .get("server_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let tool_name = input
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let args = input
            .get("input")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if server_id.is_empty() || tool_name.is_empty() {
            notes.push(
                "MCP invoke args incomplete — edit server_id and tool_name before run.".to_string(),
            );
        }
        return (
            WorkflowStepSpec::Mcp {
                id: step_id.to_string(),
                server_id,
                tool_name,
                args,
                effect_class: None,
            },
            notes,
        );
    }

    if tool.contains("prompt") || tool == "cognition_vault_search" {
        let prompt = input
            .get("query")
            .and_then(Value::as_str)
            .or_else(|| input.get("summary").and_then(Value::as_str))
            .unwrap_or(entry.input_summary.as_str())
            .trim()
            .to_string();
        return (
            WorkflowStepSpec::Prompt {
                id: step_id.to_string(),
                user_prompt: prompt,
                system_prompt: None,
            },
            notes,
        );
    }

    if entry.redacted {
        notes.push(
            "Replay input had secrets redacted — confirm parameters before running.".to_string(),
        );
    }

    (
        WorkflowStepSpec::ToolReplay {
            id: step_id.to_string(),
            tool_name: entry.tool_name.clone(),
            input: entry.sanitized_input.clone(),
            session_id: Some(entry.session_id.clone()),
            slice_id: Some(entry.slice_id.clone()),
            tool_round: Some(entry.tool_round),
            run_id: Some(entry.run_id.clone()),
            requires_confirm: entry.redacted,
        },
        notes,
    )
}

pub fn build_workflow_from_slice_refs(
    refs: &[ToolHistorySliceRef],
    name: Option<&str>,
) -> Result<(WorkflowRunRequest, Vec<String>), String> {
    let runs = resolve_tool_history_refs(refs)?;
    if runs.is_empty() {
        return Err("no tool runs resolved from refs".to_string());
    }

    let mut notes = Vec::new();
    let mut steps = Vec::new();
    for (index, entry) in runs.iter().enumerate() {
        let step_id = format!("replay-{}", index + 1);
        let (step, step_notes) = promote_run_to_step(entry, &step_id);
        steps.push(step);
        notes.extend(step_notes);
    }

    Ok((
        WorkflowRunRequest {
            name: name.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string),
            strategy: "sequential".to_string(),
            mode: "default".to_string(),
            on_failure: "stop".to_string(),
            note: Some(format!(
                "Promoted from {} tool-history slice(s)",
                refs.len()
            )),
            queue: Some("default".to_string()),
            steps,
        },
        notes,
    ))
}

pub fn new_workflow_id_for_promotion() -> String {
    new_workflow_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_redacts_secret_keys() {
        let raw = r#"{"api_key":"sk-secret","query":"hello"}"#;
        let (value, redacted, _hash) = sanitize_tool_input("cognition_mcp_invoke", raw);
        assert!(redacted);
        assert_eq!(value["api_key"], "[REDACTED]");
        assert_eq!(value["query"], "hello");
    }

    #[test]
    fn promote_mcp_invoke_to_step() {
        let entry = ToolHistoryRunEntry {
            entry_id: "s:turn:1:r1:run".to_string(),
            session_id: "s".to_string(),
            slice_id: "turn:1".to_string(),
            turn_index: 1,
            tool_round: 1,
            run_id: "run".to_string(),
            tool_name: "cognition_mcp_invoke".to_string(),
            status: "succeeded".to_string(),
            input_summary: "server.tool".to_string(),
            sanitized_input: json!({
                "server_id": "github",
                "tool_name": "search",
                "input": { "q": "medousa" }
            }),
            args_hash: "abc".to_string(),
            redacted: false,
            output_preview: None,
            timestamp: Utc::now(),
            session_preview: None,
        };
        let (step, _) = promote_run_to_step(&entry, "step-1");
        match step {
            WorkflowStepSpec::Mcp {
                server_id,
                tool_name,
                ..
            } => {
                assert_eq!(server_id, "github");
                assert_eq!(tool_name, "search");
            }
            other => panic!("expected mcp step, got {other:?}"),
        }
    }
}
