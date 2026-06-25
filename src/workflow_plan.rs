//! Heuristic workflow planner (Phase D4) — goal → suggested workflow JSON, no execution.
//!
//! Design: docs/internal/runtime-tools-roadmap.md

use serde_json::{Value, json};

use crate::workflow::{WorkflowRunRequest, WorkflowStepSpec};

pub use medousa_types::workflow_plan::{
    WorkflowPlanRequest, WorkflowPlanResponse, WorkflowScheduleSuggestion,
};

fn normalized_goal(goal: &str) -> String {
    goal.trim().to_ascii_lowercase()
}

fn mentions_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn mentions_schedule(goal: &str) -> bool {
    mentions_any(
        goal,
        &[
            "every ",
            "each ",
            "daily",
            "weekday",
            "weekly",
            "monthly",
            "cron",
            "schedule",
            "scheduled",
            "recurring",
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
            "sunday",
            "morning",
            "evening",
            "8:30",
            "9:00",
            "9am",
            "8am",
        ],
    )
}

fn mentions_notify(goal: &str) -> bool {
    mentions_any(
        goal,
        &[
            "notify",
            "ping",
            "telegram",
            "discord",
            "slack",
            "message me",
            "send message",
            "alert",
        ],
    )
}

fn mentions_csv(goal: &str) -> bool {
    mentions_any(goal, &["csv", "spreadsheet", "anomaly", "anomalies", "digest"])
}

fn mentions_research(goal: &str) -> bool {
    mentions_any(
        goal,
        &[
            "research",
            "report",
            "summarize",
            "summary",
            "look up online",
            "web search",
            "articles",
        ],
    )
}

fn mentions_document_search(goal: &str) -> bool {
    mentions_any(
        goal,
        &[
            "notion",
            "confluence",
            "document",
            "wiki",
            "drive",
            "search my",
            "find page",
            "find doc",
        ],
    )
}

fn mentions_http_poll(goal: &str) -> bool {
    mentions_any(goal, &["poll", "monitor url", "check url", "http poll", "endpoint"])
}

fn infer_cron_expr(goal: &str) -> Option<String> {
    let goal = normalized_goal(goal);
    if goal.contains("weekday") {
        if goal.contains("8:30") || goal.contains("8am") {
            return Some("30 8 * * 1-5".to_string());
        }
        if goal.contains("9:00") || goal.contains("9am") {
            return Some("0 9 * * 1-5".to_string());
        }
        return Some("0 9 * * 1-5".to_string());
    }
    if goal.contains("daily") || goal.contains("every day") {
        if goal.contains("8:30") {
            return Some("30 8 * * *".to_string());
        }
        if goal.contains("9:00") || goal.contains("9am") {
            return Some("0 9 * * *".to_string());
        }
        return Some("0 9 * * *".to_string());
    }
    if goal.contains("weekly") {
        return Some("0 9 * * 1".to_string());
    }
    None
}

fn context_url(context: Option<&Value>) -> Option<String> {
    context
        .and_then(|value| value.get("url").or_else(|| value.get("csv_url")))
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn context_topic(context: Option<&Value>, goal: &str) -> String {
    context
        .and_then(|value| value.get("topic").or_else(|| value.get("query")))
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| goal.to_string())
}

fn context_telegram_chat(context: Option<&Value>) -> Value {
    context
        .and_then(|value| value.get("telegram_chat_id"))
        .cloned()
        .unwrap_or(json!("{{chat_id}}"))
}

fn slug_name(goal: &str) -> String {
    let slug: String = goal
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "workflow-plan".to_string()
    } else {
        slug
    }
}

fn csv_fetch_source(url: &str) -> String {
    let escaped = url.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"import core from "grapheme/core"
query FetchCsv {{
  set {{ url: "{escaped}" }}
  |> http.fetch(url: $current.url) {{ status body }}
}}"#
    )
}

fn csv_digest_source() -> String {
    r#"import core from "grapheme/core"
query DigestCsv {
  set { raw: "{{ $steps.fetch_csv.output.body }}" }
  |> core.echo(message: $current.raw) { state { current } }
}"#
        .to_string()
}

fn build_csv_digest_workflow(
    goal: &str,
    context: Option<&Value>,
    scheduled: bool,
) -> (WorkflowRunRequest, WorkflowScheduleSuggestion, String, Vec<String>) {
    let url = context_url(context).unwrap_or_else(|| "https://example.com/data.csv".to_string());
    let mut assumptions = vec![
        format!("CSV URL placeholder set to '{url}' — replace via context.url before execution."),
    ];
    if scheduled {
        assumptions.push(
            "Cron inferred from goal text; verify timezone and cron_expr before scheduling."
                .to_string(),
        );
    }

    let mut steps = vec![WorkflowStepSpec::Grapheme {
        id: "fetch_csv".to_string(),
        source: csv_fetch_source(&url),
    }];

    steps.push(WorkflowStepSpec::Grapheme {
        id: "digest".to_string(),
        source: csv_digest_source(),
    });

    if mentions_notify(&normalized_goal(goal)) {
        steps.push(WorkflowStepSpec::Mcp {
            id: "notify".to_string(),
            server_id: "telegram".to_string(),
            tool_name: "send_message".to_string(),
            args: json!({
                "chat_id": context_telegram_chat(context),
                "text": "CSV digest complete for {{workflow}} — see step output in cognition_runtime_workflow_status."
            }),
            effect_class: Some("external_side_effect".to_string()),
        });
        assumptions.push(
            "Telegram MCP server must be connected; set context.telegram_chat_id for chat routing."
                .to_string(),
        );
    }

    let workflow = WorkflowRunRequest {
        name: Some(slug_name(goal)),
        strategy: "sequential".to_string(),
        mode: "default".to_string(),
        steps,
        on_failure: "stop".to_string(),
        note: Some(goal.to_string()),
        queue: None,
    };

    let cron_expr = infer_cron_expr(goal).unwrap_or_else(|| "30 8 * * 1-5".to_string());
    let schedule = WorkflowScheduleSuggestion {
        cron_expr,
        timezone: context
            .and_then(|value| value.get("timezone"))
            .and_then(|value| value.as_str())
            .unwrap_or("UTC")
            .to_string(),
    };

    let confidence = if context_url(context).is_some() {
        "high".to_string()
    } else {
        "medium".to_string()
    };

    (workflow, schedule, confidence, assumptions)
}

pub fn plan_workflow_from_goal(request: &WorkflowPlanRequest) -> WorkflowPlanResponse {
    let goal = request.goal.trim();
    let normalized = normalized_goal(goal);
    let context = request.context.as_ref();

    if goal.is_empty() {
        return WorkflowPlanResponse {
            goal: String::new(),
            confidence: "low".to_string(),
            execute_with: "none".to_string(),
            suggested_workflow: None,
            suggested_schedule: None,
            suggested_tool_input: None,
            notes: vec![
                "Provide a non-empty goal describing the desired durable workflow.".to_string(),
            ],
            assumptions: Vec::new(),
        };
    }

    if mentions_document_search(&normalized)
        && !mentions_csv(&normalized)
        && !mentions_schedule(&normalized)
        && !mentions_notify(&normalized)
    {
        return WorkflowPlanResponse {
            goal: goal.to_string(),
            confidence: "high".to_string(),
            execute_with: "cognition_capability_invoke".to_string(),
            suggested_workflow: None,
            suggested_schedule: None,
            suggested_tool_input: Some(json!({
                "capability": "document_search",
                "input": { "query": context_topic(context, goal) },
                "try_fallbacks": true
            })),
            notes: vec![
                "Single-shot document search — capability invoke is preferred over a workflow."
                    .to_string(),
            ],
            assumptions: vec![
                "MCP bindings (e.g. notion.search_pages) used when gateway catalog marks them available."
                    .to_string(),
            ],
        };
    }

    if mentions_research(&normalized)
        && !mentions_csv(&normalized)
        && !mentions_schedule(&normalized)
        && !mentions_notify(&normalized)
    {
        let topic = context_topic(context, goal);
        return WorkflowPlanResponse {
            goal: goal.to_string(),
            confidence: "high".to_string(),
            execute_with: "cognition_grapheme_template_run".to_string(),
            suggested_workflow: None,
            suggested_schedule: None,
            suggested_tool_input: Some(json!({
                "template": "research_report",
                "params": { "topic": topic }
            })),
            notes: vec![
                "Single-step web research — template run is sufficient; no durable workflow needed."
                    .to_string(),
            ],
            assumptions: vec![format!("Research topic inferred as '{topic}'.")],
        };
    }

    if mentions_http_poll(&normalized) && !mentions_csv(&normalized) {
        let url = context_url(context).unwrap_or_else(|| "https://example.com/health".to_string());
        return WorkflowPlanResponse {
            goal: goal.to_string(),
            confidence: if context_url(context).is_some() {
                "high".to_string()
            } else {
                "medium".to_string()
            },
            execute_with: if mentions_schedule(&normalized) {
                "cognition_runtime_workflow_schedule".to_string()
            } else {
                "cognition_grapheme_template_run".to_string()
            },
            suggested_workflow: None,
            suggested_schedule: infer_cron_expr(goal).map(|cron_expr| WorkflowScheduleSuggestion {
                cron_expr,
                timezone: "UTC".to_string(),
            }),
            suggested_tool_input: Some(json!({
                "template": "http_poll",
                "params": { "url": url }
            })),
            notes: vec![
                "HTTP poll template suggested; promote to workflow_schedule if recurring monitoring is intended."
                    .to_string(),
            ],
            assumptions: vec![format!("Poll URL placeholder set to '{url}'.")],
        };
    }

    if mentions_csv(&normalized) || (mentions_schedule(&normalized) && mentions_notify(&normalized))
    {
        let scheduled = mentions_schedule(&normalized);
        let (workflow, schedule, confidence, assumptions) =
            build_csv_digest_workflow(goal, context, scheduled);

        return WorkflowPlanResponse {
            goal: goal.to_string(),
            confidence,
            execute_with: if scheduled {
                "cognition_runtime_workflow_schedule".to_string()
            } else {
                "cognition_runtime_workflow_run".to_string()
            },
            suggested_workflow: Some(workflow),
            suggested_schedule: if scheduled {
                Some(schedule)
            } else {
                None
            },
            suggested_tool_input: None,
            notes: vec![
                "Multi-step CSV digest workflow with optional Telegram notify step.".to_string(),
                "Review grapheme sources and MCP args before calling execute_with tool.".to_string(),
            ],
            assumptions,
        };
    }

    if mentions_notify(&normalized) {
        let workflow = WorkflowRunRequest {
            name: Some(slug_name(goal)),
            strategy: "sequential".to_string(),
            mode: "default".to_string(),
            steps: vec![WorkflowStepSpec::Mcp {
                id: "notify".to_string(),
                server_id: "telegram".to_string(),
                tool_name: "send_message".to_string(),
                args: json!({
                    "chat_id": context_telegram_chat(context),
                    "text": goal
                }),
                effect_class: Some("external_side_effect".to_string()),
            }],
            on_failure: "stop".to_string(),
            note: Some(goal.to_string()),
            queue: None,
        };

        return WorkflowPlanResponse {
            goal: goal.to_string(),
            confidence: "medium".to_string(),
            execute_with: if mentions_schedule(&normalized) {
                "cognition_runtime_workflow_schedule".to_string()
            } else {
                "cognition_runtime_workflow_run".to_string()
            },
            suggested_workflow: Some(workflow),
            suggested_schedule: infer_cron_expr(goal).map(|cron_expr| WorkflowScheduleSuggestion {
                cron_expr,
                timezone: "UTC".to_string(),
            }),
            suggested_tool_input: None,
            notes: vec!["Single MCP notify step suggested.".to_string()],
            assumptions: vec![
                "Telegram MCP server must be connected.".to_string(),
                "Set context.telegram_chat_id when chat routing is known.".to_string(),
            ],
        };
    }

    WorkflowPlanResponse {
        goal: goal.to_string(),
        confidence: "low".to_string(),
        execute_with: "manual".to_string(),
        suggested_workflow: Some(WorkflowRunRequest {
            name: Some(slug_name(goal)),
            strategy: "sequential".to_string(),
            mode: "default".to_string(),
            steps: vec![WorkflowStepSpec::Prompt {
                id: "plan".to_string(),
                user_prompt: format!(
                    "Execute the following user goal using available tools and return structured evidence:\n\n{goal}"
                ),
                system_prompt: None,
            }],
            on_failure: "stop".to_string(),
            note: Some(goal.to_string()),
            queue: None,
        }),
        suggested_schedule: None,
        suggested_tool_input: None,
        notes: vec![
            "No strong heuristic match — generic prompt step scaffold returned.".to_string(),
            "Prefer cognition_capability_invoke or cognition_grapheme_template_run when intent is clearer."
                .to_string(),
        ],
        assumptions: vec![
            "Replace or extend steps after reviewing goal with capability catalog.".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_document_search_as_capability_invoke() {
        let plan = plan_workflow_from_goal(&WorkflowPlanRequest {
            goal: "Search my Notion docs for Q1 roadmap".to_string(),
            context: None,
        });
        assert_eq!(plan.execute_with, "cognition_capability_invoke");
        assert_eq!(plan.confidence, "high");
    }

    #[test]
    fn plans_weekday_csv_digest_with_schedule() {
        let plan = plan_workflow_from_goal(&WorkflowPlanRequest {
            goal: "Every weekday 8:30 run CSV anomaly digest and ping Telegram".to_string(),
            context: Some(json!({ "url": "https://example.com/metrics.csv" })),
        });
        assert_eq!(plan.execute_with, "cognition_runtime_workflow_schedule");
        assert!(plan.suggested_workflow.is_some());
        assert!(plan.suggested_schedule.is_some());
        let workflow = plan.suggested_workflow.unwrap();
        assert_eq!(workflow.steps.len(), 3);
    }

    #[test]
    fn plans_research_as_template() {
        let plan = plan_workflow_from_goal(&WorkflowPlanRequest {
            goal: "Research Rust async runtimes and summarize".to_string(),
            context: None,
        });
        assert_eq!(plan.execute_with, "cognition_grapheme_template_run");
    }
}
