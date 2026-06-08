//! Shared parallel execution policy for workflow strategies and agent tool loops.
//!
//! Keeps workflow concurrent/handoff and multi-tool-call batches under the same rules.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mcp_gateway_api::McpEffectClass;
use crate::product_config::load_product_config;
use crate::workflow::WorkflowStepSpec;

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

pub fn load_parallel_execution_settings() -> ParallelExecutionSettings {
    let config = load_product_config();
    let workflow = config.runtime.workflow;
    let mut settings = ParallelExecutionSettings {
        parallel_tool_calls_enabled: workflow.parallel_tool_calls_enabled,
        max_parallel_tool_calls: workflow.max_parallel_tool_calls,
        max_concurrent_workflow_steps: workflow.max_concurrent_workflow_steps,
        allow_mutating_parallel: workflow.allow_mutating_parallel,
        default_workflow_strategy: workflow.default_strategy,
    };

    if let Ok(value) = std::env::var("MEDOUSA_PARALLEL_TOOL_CALLS_ENABLED") {
        settings.parallel_tool_calls_enabled = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }
    if let Ok(value) = std::env::var("MEDOUSA_ALLOW_MUTATING_PARALLEL") {
        settings.allow_mutating_parallel = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }

    settings
}

pub fn classify_workflow_step(step: &WorkflowStepSpec) -> StepExecutionClass {
    match step {
        WorkflowStepSpec::Prompt { .. } => StepExecutionClass::ReadOnly,
        WorkflowStepSpec::Grapheme { .. } => StepExecutionClass::Mutating,
        WorkflowStepSpec::Mcp { effect_class, .. } => match effect_class {
            Some(class) if class.eq_ignore_ascii_case("external_read") => {
                StepExecutionClass::ReadOnly
            }
            _ => StepExecutionClass::Mutating,
        },
    }
}

pub fn step_references_prior_outputs(step: &WorkflowStepSpec) -> bool {
    let haystacks: Vec<String> = match step {
        WorkflowStepSpec::Grapheme { source, .. } => vec![source.clone()],
        WorkflowStepSpec::Prompt {
            user_prompt,
            system_prompt,
            ..
        } => {
            let mut values = vec![user_prompt.clone()];
            if let Some(system_prompt) = system_prompt.clone() {
                values.push(system_prompt);
            }
            values
        }
        WorkflowStepSpec::Mcp { args, .. } => vec![args.to_string()],
    };

    haystacks
        .iter()
        .any(|value| value.contains("$steps.") || value.contains("$handoff."))
}

pub fn validate_concurrent_workflow(
    steps: &[WorkflowStepSpec],
    settings: &ParallelExecutionSettings,
) -> Result<(), String> {
    if steps.is_empty() {
        return Err("workflow requires at least one step".to_string());
    }
    if steps.len() > settings.max_concurrent_workflow_steps {
        return Err(format!(
            "concurrent workflow exceeds max_concurrent_workflow_steps ({})",
            settings.max_concurrent_workflow_steps
        ));
    }

    for step in steps {
        if step_references_prior_outputs(step) {
            return Err(format!(
                "concurrent workflow step '{}' references prior outputs; use sequential or handoff strategy",
                step.id()
            ));
        }
    }

    if !settings.allow_mutating_parallel {
        let mutating: Vec<_> = steps
            .iter()
            .filter(|step| classify_workflow_step(step) == StepExecutionClass::Mutating)
            .map(|step| step.id().to_string())
            .collect();
        if !mutating.is_empty() {
            return Err(format!(
                "concurrent workflow blocked mutating step(s): {}. \
                 Set runtime.workflow.allow_mutating_parallel=true or mark MCP steps with effect_class=external_read.",
                mutating.join(", ")
            ));
        }
    }

    Ok(())
}

pub fn classify_tool_call(tool_name: &str, input: &Value) -> StepExecutionClass {
    match tool_name {
        "cognition_mcp_invoke" => {
            let read_only = input
                .get("effect_class")
                .and_then(|value| value.as_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("external_read"));
            if read_only {
                StepExecutionClass::ReadOnly
            } else {
                StepExecutionClass::Mutating
            }
        }
        "cognition_capability_invoke" => StepExecutionClass::Mutating,
        "cognition_memory_store"
        | "cognition_memory_calibrate"
        | "cognition_runtime_workflow_run"
        | "cognition_runtime_workflow_schedule"
        | "cognition_runtime_recurring_register"
        | "cognition_mcp_promote_to_job"
        | "cognition_grapheme_promote_to_job"
        | "cognition_job_enqueue"
        | "cognition_openshell_sandbox_run" => StepExecutionClass::Mutating,
        "cognition_openshell_status" | "cognition_skill_discover" | "cognition_skill_propose" => {
            StepExecutionClass::ReadOnly
        }
        "cognition_skill_probe" => StepExecutionClass::Mutating,
        "cognition_memory_recall"
        | "cognition_memory_context"
        | "cognition_memory_list"
        | "cognition_memory_schema"
        | "cognition_memory_moods"
        | "cognition_runtime_jobs_list"
        | "cognition_runtime_jobs_status"
        | "cognition_runtime_delivery_status"
        | "cognition_mcp_discover"
        | "cognition_capability_search"
        | "cognition_grapheme_modules"
        | "cognition_grapheme_template_run"
        | "cognition_turn_prepare_final"
        | "cognition.turn.prepare_final"
        | "cognition_turn_finish"
        | "cognition.turn.finish" => StepExecutionClass::ReadOnly,
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
        return Err("parallel tool calls disabled by runtime.workflow.parallel_tool_calls_enabled".to_string());
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

pub fn mcp_effect_class_from_str(raw: &str) -> Option<McpEffectClass> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "external_read" => Some(McpEffectClass::ExternalRead),
        "external_write" => Some(McpEffectClass::ExternalWrite),
        "external_side_effect" => Some(McpEffectClass::ExternalSideEffect),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::WorkflowStepSpec;
    use serde_json::json;

    #[test]
    fn concurrent_rejects_step_refs() {
        let steps = vec![WorkflowStepSpec::Prompt {
            id: "b".to_string(),
            user_prompt: "use $steps.a.output".to_string(),
            system_prompt: None,
        }];
        let settings = ParallelExecutionSettings::default();
        assert!(validate_concurrent_workflow(&steps, &settings).is_err());
    }

    #[test]
    fn concurrent_allows_read_only_batch() {
        let steps = vec![
            WorkflowStepSpec::Prompt {
                id: "a".to_string(),
                user_prompt: "one".to_string(),
                system_prompt: None,
            },
            WorkflowStepSpec::Mcp {
                id: "b".to_string(),
                server_id: "notion".to_string(),
                tool_name: "search".to_string(),
                args: json!({}),
                effect_class: Some("external_read".to_string()),
            },
        ];
        let settings = ParallelExecutionSettings::default();
        assert!(validate_concurrent_workflow(&steps, &settings).is_ok());
    }

    #[test]
    fn parallel_tool_batch_blocks_mutating_without_flag() {
        let calls = vec![
            (
                "cognition_grapheme_run".to_string(),
                json!({ "source": "query {}" }),
            ),
            ("cognition_memory_recall".to_string(), json!({ "query": "x" })),
        ];
        let settings = ParallelExecutionSettings::default();
        assert!(parallel_tool_batch_allowed(&calls, &settings).is_err());
    }
}
