//! Portable policy for bounded parallel model tool-call batches.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepExecutionClass {
    ReadOnly,
    Mutating,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParallelExecutionSettings {
    #[serde(default = "default_parallel_tool_calls_enabled")]
    pub parallel_tool_calls_enabled: bool,
    #[serde(default = "default_max_parallel_tool_calls")]
    pub max_parallel_tool_calls: usize,
    #[serde(default = "default_max_concurrent_workflow_steps")]
    pub max_concurrent_workflow_steps: usize,
    #[serde(default)]
    pub allow_mutating_parallel: bool,
    #[serde(default = "default_workflow_strategy")]
    pub default_workflow_strategy: String,
}

impl Default for ParallelExecutionSettings {
    fn default() -> Self {
        Self {
            parallel_tool_calls_enabled: default_parallel_tool_calls_enabled(),
            max_parallel_tool_calls: default_max_parallel_tool_calls(),
            max_concurrent_workflow_steps: default_max_concurrent_workflow_steps(),
            allow_mutating_parallel: false,
            default_workflow_strategy: default_workflow_strategy(),
        }
    }
}

/// Composition-time source for execution settings that may change between
/// turns. Embedded nodes can use the static default; the daemon injects its
/// product-config and environment-backed loader.
pub trait ParallelExecutionSettingsProvider: Send + Sync {
    fn load(&self) -> ParallelExecutionSettings;
}

impl<F> ParallelExecutionSettingsProvider for F
where
    F: Fn() -> ParallelExecutionSettings + Send + Sync,
{
    fn load(&self) -> ParallelExecutionSettings {
        self()
    }
}

fn default_parallel_tool_calls_enabled() -> bool {
    true
}

fn default_max_parallel_tool_calls() -> usize {
    4
}

fn default_max_concurrent_workflow_steps() -> usize {
    8
}

fn default_workflow_strategy() -> String {
    "sequential".to_string()
}

pub fn classify_tool_call(tool_name: &str, input: &Value) -> StepExecutionClass {
    match tool_name {
        "cognition_mcp_invoke" => {
            let read_only = input
                .get("effect_class")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("external_read"));
            if read_only {
                StepExecutionClass::ReadOnly
            } else {
                StepExecutionClass::Mutating
            }
        }
        "cognition_capability" => {
            let action = input.get("action").and_then(Value::as_str).unwrap_or("");
            if action.ends_with(".find")
                || (action == "mcp.invoke"
                    && input
                        .get("effect_class")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.eq_ignore_ascii_case("external_read")))
            {
                StepExecutionClass::ReadOnly
            } else {
                StepExecutionClass::Mutating
            }
        }
        "cognition_turn" => {
            let action = input.get("action").and_then(Value::as_str).unwrap_or("");
            match action {
                "turn.finish" | "turn.checkpoint" | "turn.request_more_rounds" => {
                    StepExecutionClass::ReadOnly
                }
                _ => StepExecutionClass::Mutating,
            }
        }
        "cognition_workshop_mutate" => {
            let action = input.get("action").and_then(Value::as_str).unwrap_or("");
            if action == "workshop.spawn" {
                StepExecutionClass::ReadOnly
            } else {
                StepExecutionClass::Mutating
            }
        }
        "cognition_runtime_mutate"
        | "cognition_memory_mutate"
        | "cognition_identity_mutate"
        | "cognition_calendar_mutate"
        | "cognition_openshell_sandbox_run" => StepExecutionClass::Mutating,
        "cognition_openshell_status" | "cognition_skill_discover" | "cognition_skill_propose" => {
            StepExecutionClass::ReadOnly
        }
        "cognition_skill_probe" => StepExecutionClass::Mutating,
        "cognition_memory_query"
        | "cognition_identity_query"
        | "cognition_calendar_query"
        | "cognition_workshop_query"
        | "cognition_store_read"
        | "cognition_runtime_query"
        | "cognition_schema"
        | "cognition_web_search" => StepExecutionClass::ReadOnly,
        _ if tool_name.contains("modules") || tool_name.contains("examples") => {
            StepExecutionClass::ReadOnly
        }
        _ => StepExecutionClass::Mutating,
    }
}

pub fn parallel_tool_batch_allowed(
    calls: &[(String, Value)],
    settings: &ParallelExecutionSettings,
) -> Result<(), String> {
    if !settings.parallel_tool_calls_enabled {
        return Err(
            "parallel tool calls disabled by runtime.workflow.parallel_tool_calls_enabled"
                .to_string(),
        );
    }
    if calls.len() <= 1 {
        return Ok(());
    }
    if calls.len() > settings.max_parallel_tool_calls {
        return Err(format!(
            "tool batch size {} exceeds max_parallel_tool_calls ({})",
            calls.len(),
            settings.max_parallel_tool_calls
        ));
    }

    if !settings.allow_mutating_parallel {
        let mutating: Vec<_> = calls
            .iter()
            .filter(|(name, input)| classify_tool_call(name, input) == StepExecutionClass::Mutating)
            .map(|(name, _)| name.clone())
            .collect();
        if !mutating.is_empty() {
            return Err(format!(
                "parallel tool batch blocked mutating tool(s): {}. \
                 Retry sequentially or enable runtime.workflow.allow_mutating_parallel.",
                mutating.join(", ")
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_only_batches_are_portable_but_mutations_remain_sequential() {
        let settings = ParallelExecutionSettings::default();
        let reads = vec![
            ("cognition_memory_query".to_string(), json!({})),
            ("cognition_store_read".to_string(), json!({})),
        ];
        assert!(parallel_tool_batch_allowed(&reads, &settings).is_ok());

        let mutation = vec![
            ("cognition_memory_query".to_string(), json!({})),
            ("cognition_memory_mutate".to_string(), json!({})),
        ];
        assert!(parallel_tool_batch_allowed(&mutation, &settings).is_err());
    }

    #[test]
    fn workshop_spawn_is_parallel_safe_but_cancel_is_not() {
        assert_eq!(
            classify_tool_call(
                "cognition_workshop_mutate",
                &json!({ "action": "workshop.spawn" })
            ),
            StepExecutionClass::ReadOnly
        );
        assert_eq!(
            classify_tool_call(
                "cognition_workshop_mutate",
                &json!({ "action": "workshop.cancel" })
            ),
            StepExecutionClass::Mutating
        );
    }
}
