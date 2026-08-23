//! Shared parallel execution policy for workflow strategies and agent tool loops.
//!
//! Keeps workflow concurrent/handoff and multi-tool-call batches under the same rules.

use crate::mcp_gateway_api::McpEffectClass;
use crate::product_config::load_product_config;
use crate::workflow::WorkflowStepSpec;

pub use medousa_runtime::execution_policy::{
    ParallelExecutionSettings, StepExecutionClass, classify_tool_call, parallel_tool_batch_allowed,
};

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
        WorkflowStepSpec::ToolReplay { .. } => StepExecutionClass::Mutating,
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
        WorkflowStepSpec::ToolReplay { input, .. } => vec![input.to_string()],
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
                "cognition_capability".to_string(),
                json!({ "action": "grapheme.invoke", "script": "query {}" }),
            ),
            (
                "cognition_memory_query".to_string(),
                json!({ "action": "memory.recall", "query": "x" }),
            ),
        ];
        let settings = ParallelExecutionSettings::default();
        assert!(parallel_tool_batch_allowed(&calls, &settings).is_err());
    }

    #[test]
    fn spawn_turn_worker_is_parallel_safe() {
        let input = json!({
            "action": "workshop.spawn",
            "intent": "research",
            "task": "look this up",
            "user_ack": "On it."
        });
        assert_eq!(
            classify_tool_call("cognition_workshop_mutate", &input),
            StepExecutionClass::ReadOnly
        );
        let calls = vec![
            ("cognition_workshop_mutate".to_string(), input.clone()),
            ("cognition_workshop_mutate".to_string(), input),
        ];
        let settings = ParallelExecutionSettings::default();
        assert!(parallel_tool_batch_allowed(&calls, &settings).is_ok());
        let cancel = json!({ "action": "workshop.cancel", "work_id": "work-1" });
        assert_eq!(
            classify_tool_call("cognition_workshop_mutate", &cancel),
            StepExecutionClass::Mutating
        );
    }

    #[test]
    fn turn_finish_is_parallel_safe_and_begin_work_is_mutating() {
        let finish = json!({ "action": "turn.finish", "message": "Done." });
        let begin = json!({
            "action": "turn.begin_work",
            "message": "Starting",
            "goal": "inspect"
        });
        assert_eq!(
            classify_tool_call("cognition_turn", &finish),
            StepExecutionClass::ReadOnly
        );
        assert_eq!(
            classify_tool_call("cognition_turn", &begin),
            StepExecutionClass::Mutating
        );
    }

    #[test]
    fn store_read_is_parallel_safe_and_store_write_is_mutating() {
        let read = json!({ "action": "vault.search", "query": "notes" });
        let write = json!({ "action": "vault.write", "path": "a.md", "content": "x" });
        assert_eq!(
            classify_tool_call("cognition_store_read", &read),
            StepExecutionClass::ReadOnly
        );
        assert_eq!(
            classify_tool_call("cognition_store_write", &write),
            StepExecutionClass::Mutating
        );
        let settings = ParallelExecutionSettings::default();
        assert!(
            parallel_tool_batch_allowed(
                &[
                    ("cognition_store_read".to_string(), read.clone()),
                    ("cognition_store_read".to_string(), read),
                ],
                &settings
            )
            .is_ok()
        );
        assert!(
            parallel_tool_batch_allowed(
                &[
                    ("cognition_store_read".to_string(), json!({})),
                    ("cognition_store_write".to_string(), write),
                ],
                &settings
            )
            .is_err()
        );
    }

    #[test]
    fn capability_find_is_parallel_safe_and_invoke_is_mutating() {
        let find = json!({ "action": "grapheme.find", "module": "web" });
        let invoke = json!({ "action": "capability.invoke", "capability": "web_research" });
        let mcp_read = json!({
            "action": "mcp.invoke",
            "server_id": "web",
            "tool_name": "search",
            "effect_class": "external_read"
        });
        assert_eq!(
            classify_tool_call("cognition_capability", &find),
            StepExecutionClass::ReadOnly
        );
        assert_eq!(
            classify_tool_call("cognition_capability", &invoke),
            StepExecutionClass::Mutating
        );
        assert_eq!(
            classify_tool_call("cognition_capability", &mcp_read),
            StepExecutionClass::ReadOnly
        );
    }

    #[test]
    fn runtime_query_is_parallel_safe_and_mutate_is_mutating() {
        let query = json!({ "action": "job.list" });
        let mutate = json!({
            "action": "job.enqueue",
            "job_type": "workflow.grapheme.run",
            "payload_ref": "grapheme:inline:query {}"
        });
        assert_eq!(
            classify_tool_call("cognition_runtime_query", &query),
            StepExecutionClass::ReadOnly
        );
        assert_eq!(
            classify_tool_call("cognition_runtime_mutate", &mutate),
            StepExecutionClass::Mutating
        );
        let settings = ParallelExecutionSettings::default();
        assert!(
            parallel_tool_batch_allowed(
                &[
                    ("cognition_runtime_query".to_string(), query.clone()),
                    ("cognition_runtime_query".to_string(), query),
                ],
                &settings
            )
            .is_ok()
        );
        assert!(
            parallel_tool_batch_allowed(
                &[
                    (
                        "cognition_runtime_query".to_string(),
                        json!({ "action": "delivery.status" })
                    ),
                    ("cognition_runtime_mutate".to_string(), mutate),
                ],
                &settings
            )
            .is_err()
        );
    }

    #[test]
    fn identity_query_is_parallel_safe_and_mutate_is_mutating() {
        let query = json!({ "action": "identity.recall", "query": "Mario" });
        let mutate = json!({
            "action": "identity.remember",
            "fact_kind": "preference",
            "subject": "beverage",
            "statement": "matcha"
        });
        assert_eq!(
            classify_tool_call("cognition_identity_query", &query),
            StepExecutionClass::ReadOnly
        );
        assert_eq!(
            classify_tool_call("cognition_identity_mutate", &mutate),
            StepExecutionClass::Mutating
        );
    }

    #[test]
    fn calendar_query_is_parallel_safe_and_mutate_is_mutating() {
        let query = json!({ "action": "calendar.list", "from": "2026-08-18T00:00:00Z" });
        let mutate = json!({
            "action": "calendar.create",
            "summary": "Standup",
            "dtstart": "2026-08-18T17:00:00Z"
        });
        assert_eq!(
            classify_tool_call("cognition_calendar_query", &query),
            StepExecutionClass::ReadOnly
        );
        assert_eq!(
            classify_tool_call("cognition_calendar_mutate", &mutate),
            StepExecutionClass::Mutating
        );
    }
}
