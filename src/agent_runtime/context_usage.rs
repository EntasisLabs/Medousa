//! Turn-start context budget telemetry (Cursor-style layer breakdown).

use std::sync::Arc;

use stasis::application::orchestration::tool_registry::ToolRegistry;

use super::turn_services::PriorMessageBuild;
use crate::daemon_api::{ContextUsageLayer, ContextUsageReport};

/// The current catalog conflates separate input ceilings and shared context
/// limits. Until it distinguishes them, reserve output against both: conservative
/// on separate-input providers, necessary for shared-window targets.
pub fn inference_window_policy(
    provider: &str,
    model: &str,
) -> medousa_runtime::inference_window::InferenceWindowPolicy {
    let record = crate::model_capability_registry::registry()
        .resolve(provider, model)
        .model;
    policy_for_record(provider, model, record.as_ref())
}

fn policy_for_record(
    provider: &str,
    model: &str,
    record: Option<&crate::model_capability_registry::types::ModelCapabilityRecord>,
) -> medousa_runtime::inference_window::InferenceWindowPolicy {
    // Older cached account catalogs contain names/reasoning but no limits. Keep
    // this an explicit policy estimate rather than inventing catalog metadata.
    let codex = provider.trim().eq_ignore_ascii_case("openai-codex");
    let codex_fallback = codex && (model.starts_with("gpt-5") || model.starts_with("gpt-6"));
    let input = record
        .and_then(|r| r.max_input_tokens)
        .map(|limit| limit.min(u32::MAX as u64) as u32);
    let output = record
        .and_then(|r| r.max_output_tokens)
        .map(|limit| limit.min(u32::MAX as u64) as u32)
        .unwrap_or(medousa_runtime::inference_window::DEFAULT_OUTPUT_RESERVE);
    medousa_runtime::inference_window::InferenceWindowPolicy {
        target: format!("{provider}:{model}"),
        input_tokens: input,
        shared_tokens: if record
            .is_some_and(|r| matches!(r.source.as_str(), "anthropic.models" | "google.models"))
        {
            None
        } else {
            Some(input.unwrap_or(if codex_fallback {
                272_000
            } else {
                medousa_runtime::inference_window::UNKNOWN_INPUT_TOKENS
            }))
        },
        // ChatGPT plan bridge deliberately rejects max_output_tokens. Reserving
        // the catalog maximum keeps its generation behavior unchanged; unknown
        // limits remain explicit estimates, never a claimed hard guarantee.
        enforce_output_limit: !codex,
        output_reserve: output,
        ..Default::default()
    }
}

pub const ESTIMATOR_LABEL: &str = "chars_div_4";

pub fn chars_to_tokens(chars: usize) -> u32 {
    medousa_runtime::context::ContextCost::text(chars).tokens_estimate()
}

fn layer(id: &str, label: &str, chars: usize) -> ContextUsageLayer {
    let chars = u32::try_from(chars).unwrap_or(u32::MAX);
    ContextUsageLayer {
        id: id.to_string(),
        label: label.to_string(),
        chars,
        tokens_estimate: chars_to_tokens(chars as usize),
    }
}

pub struct ContextUsageInput<'a> {
    pub session_id: &'a str,
    pub system_prompt_chars: usize,
    pub prompt_ledger: &'a super::context_assembly::PromptLedger,
    pub resolved_prompt: &'a str,
    pub prompt_for_request: &'a str,
    pub current_user_message: &'a genai::chat::ChatMessage,
    /// Initial refresh snapshot only; the runtime re-reads this protected lane.
    pub sticky_chars: usize,
    pub prior_build: &'a PriorMessageBuild,
    pub tool_count: usize,
    pub tool_schema_chars: usize,
    pub tool_schema_known: bool,
    pub context_limit_tokens: Option<u32>,
}

pub struct ContextUsageCompilation {
    pub usage: ContextUsageReport,
    pub decisions: medousa_runtime::context::ContextDecisionReport,
    /// Reconciliation of the preparation snapshot only, NOT a ChatRequest.
    pub assembly_reconciled: bool,
    /// Always false here: runtime-owned layers and effective schemas are pending.
    pub reconciled: bool,
}

pub fn build_context_usage_report(input: ContextUsageInput<'_>) -> ContextUsageCompilation {
    use super::context_assembly::contribution;
    use medousa_runtime::context::{
        ContextCapacity, ContextCoordinator, ContextCost, ContextOwner, Retention,
    };
    let mut coordinator = ContextCoordinator::default();
    for (id, owner, chars, retention) in [
        (
            "system_prompt",
            ContextOwner::Host,
            input.system_prompt_chars,
            Retention::Required,
        ),
        (
            "tool_definitions",
            ContextOwner::Tool,
            input.tool_schema_chars,
            Retention::Required,
        ),
        (
            "chat_stickies",
            ContextOwner::Chat,
            input.sticky_chars,
            Retention::Protected,
        ),
    ] {
        coordinator.include(contribution(
            id,
            owner,
            input.session_id,
            ContextCost::text(chars),
            retention,
            None,
        ));
    }
    if !input.tool_schema_known {
        coordinator.include(contribution(
            "unknown_tool_schemas",
            ContextOwner::Tool,
            input.session_id,
            ContextCost {
                text_chars: 0,
                unknown_parts: 1,
            },
            Retention::Required,
            None,
        ));
    }
    coordinator
        .contributions
        .extend(input.prior_build.context.contributions.clone());
    input
        .prompt_ledger
        .contribute(input.resolved_prompt, &mut coordinator);
    if let Some(policy) = input.prompt_for_request.strip_prefix(input.resolved_prompt) {
        coordinator.include(contribution(
            "tool_policy",
            ContextOwner::Tool,
            input.session_id,
            ContextCost::text(policy.chars().count()),
            Retention::Required,
            None,
        ));
    } else {
        // This would be a new renderer contract, not permission to invent source
        // attribution. Leave the independent reconciliation visibly false.
        coordinator.include(contribution(
            "unattributed_tool_policy",
            ContextOwner::Tool,
            input.session_id,
            ContextCost {
                text_chars: 0,
                unknown_parts: 1,
            },
            Retention::Required,
            None,
        ));
    }
    let current_cost = ContextCost::messages(std::slice::from_ref(input.current_user_message));
    if current_cost.unknown_parts > 0 {
        coordinator.include(contribution(
            "current_media",
            ContextOwner::Chat,
            input.session_id,
            ContextCost {
                text_chars: 0,
                unknown_parts: current_cost.unknown_parts,
            },
            Retention::Required,
            None,
        ));
    }
    let expected = ContextCost::messages(&input.prior_build.messages)
        .saturating_add(current_cost)
        .saturating_add(ContextCost::text(input.system_prompt_chars))
        .saturating_add(ContextCost::text(input.tool_schema_chars))
        .saturating_add(ContextCost::text(input.sticky_chars))
        .saturating_add(ContextCost {
            text_chars: 0,
            unknown_parts: usize::from(!input.tool_schema_known),
        });
    let mut decisions = coordinator.report(ContextCapacity {
        input_tokens: input.context_limit_tokens,
    });
    let mut prepared = input.prior_build.messages.clone();
    prepared.push(input.current_user_message.clone());
    decisions.prepared_messages_digest = Some(medousa_runtime::context::prepared_messages_digest(
        &prepared,
    ));
    let assembly_reconciled = decisions.cost == expected
        && !decisions
            .contributions
            .iter()
            .any(|item| item.id.starts_with("unattributed"));
    let mut layers: Vec<ContextUsageLayer> = Vec::new();
    // Stable first-source order, merging repeated manuscript/history spans.
    for item in &decisions.contributions {
        if matches!(
            item.decision,
            medousa_runtime::context::ContextDecision::Excluded { .. }
                | medousa_runtime::context::ContextDecision::Omitted { .. }
        ) {
            continue;
        }
        let chars = u32::try_from(item.cost.text_chars).unwrap_or(u32::MAX);
        if let Some(existing) = layers.iter_mut().find(|layer| layer.id == item.id) {
            existing.chars = existing.chars.saturating_add(chars);
            existing.tokens_estimate = chars_to_tokens(existing.chars as usize);
        } else {
            layers.push(layer(&item.id, layer_label(&item.id), item.cost.text_chars));
        }
    }
    ContextUsageCompilation {
        usage: ContextUsageReport {
            hud: None,
            layers,
            total_tokens_estimate: decisions.text_tokens_estimate,
            total_chars: u32::try_from(decisions.cost.text_chars).unwrap_or(u32::MAX),
            context_limit_tokens: input.context_limit_tokens,
            tool_count: u32::try_from(input.tool_count).unwrap_or(u32::MAX),
            estimator: format!(
                "{ESTIMATOR_LABEL}_text_only; framing_unknown; unknown_parts={}; assembly_snapshot_incomplete_unreconciled",
                decisions.cost.unknown_parts
            ),
        },
        decisions,
        assembly_reconciled,
        reconciled: false,
    }
}

/// Final telemetry comes from the same footprint retained by inference usage.
pub fn first_request_usage_report(
    footprint: &medousa_runtime::inference_usage::RequestFootprint,
) -> Option<ContextUsageReport> {
    let report = footprint.first_request_context.as_ref()?;
    let mut layers: Vec<ContextUsageLayer> = Vec::new();
    for item in &report.decisions.contributions {
        if matches!(
            item.decision,
            medousa_runtime::context::ContextDecision::Excluded { .. }
                | medousa_runtime::context::ContextDecision::Omitted { .. }
        ) {
            continue;
        }
        if let Some(existing) = layers.iter_mut().find(|layer| layer.id == item.id) {
            existing.chars = existing
                .chars
                .saturating_add(u32::try_from(item.cost.text_chars).unwrap_or(u32::MAX));
            existing.tokens_estimate = chars_to_tokens(existing.chars as usize);
        } else {
            layers.push(layer(&item.id, layer_label(&item.id), item.cost.text_chars));
        }
    }
    Some(ContextUsageReport {
        hud: footprint
            .inference_window
            .as_ref()
            .and_then(|w| w.hud.clone()),
        layers,
        total_chars: u32::try_from(report.decisions.cost.text_chars).unwrap_or(u32::MAX),
        total_tokens_estimate: report.decisions.text_tokens_estimate,
        context_limit_tokens: report.decisions.capacity.input_tokens,
        tool_count: u32::try_from(report.tool_count.unwrap_or(0)).unwrap_or(u32::MAX),
        estimator: format!(
            "{ESTIMATOR_LABEL}_text_only; framing_unknown; unknown_parts={}; first_request; reconciled={}; prepared_provenance_preserved={:?}; tools={}",
            report.decisions.cost.unknown_parts,
            report.reconciled,
            report.prepared_provenance_preserved,
            if report.tool_count.is_some() {
                "catalog_known"
            } else {
                "disabled"
            }
        ),
    })
}

fn layer_label(id: &str) -> &str {
    match id {
        "system_prompt" => "System prompt",
        "tool_definitions" => "Tool definitions",
        "chat_stickies" => "Chat sticky notes",
        "prior_conversation" => "Conversation",
        "cold_history" => "Summarized history",
        "tool_slices" => "Prior tool slices",
        "grapheme_scripts" => "Grapheme scripts",
        "runtime_learnings" => "Runtime learnings",
        "tool_hints" => "Tool hints",
        "user_message" => "Your message",
        "history_media" => "Prior attachments (text only)",
        "current_media" => "Current attachments (unknown cost)",
        _ => id,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSchemaFootprint {
    /// False means registry enumeration failed, NOT an empty tool catalog.
    pub known: bool,
    pub tool_count: usize,
    /// Sum of the individually serialized tool contracts. Provider-specific
    /// request-envelope punctuation is deliberately excluded.
    pub total_chars: usize,
    /// Largest schemas first, then stable by tool name.
    pub tools: Vec<ToolSchemaEntryFootprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSchemaEntryFootprint {
    pub name: String,
    pub chars: usize,
    pub tokens_estimate: u32,
}

pub async fn measure_tool_schema_footprint(
    registry: &Arc<dyn ToolRegistry>,
) -> ToolSchemaFootprint {
    let tools = match registry.list_tools().await {
        Ok(tools) => tools,
        Err(_) => {
            return ToolSchemaFootprint {
                known: false,
                tool_count: 0,
                total_chars: 0,
                tools: Vec::new(),
            };
        }
    };
    measure_tool_schema_footprint_for_tools(&tools)
}

pub(crate) fn measure_tool_schema_footprint_for_tools(
    tools: &[genai::chat::Tool],
) -> ToolSchemaFootprint {
    let mut entries = tools
        .iter()
        .map(|tool| {
            let chars = estimate_single_tool_chars(tool);
            ToolSchemaEntryFootprint {
                name: tool.name.to_string(),
                chars,
                tokens_estimate: chars_to_tokens(chars),
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .chars
            .cmp(&left.chars)
            .then_with(|| left.name.cmp(&right.name))
    });
    ToolSchemaFootprint {
        known: true,
        tool_count: tools.len(),
        total_chars: entries.iter().map(|entry| entry.chars).sum(),
        tools: entries,
    }
}

fn estimate_single_tool_chars(tool: &genai::chat::Tool) -> usize {
    serde_json::to_string(tool)
        .map(|json| json.chars().count())
        .unwrap_or_else(|_| tool.name.to_string().chars().count() + 64)
}

pub fn operator_summary(report: &ContextUsageReport) -> String {
    let total = format_tokens(report.total_tokens_estimate);
    let limit = report
        .context_limit_tokens
        .map(|limit| format!(" / {}", format_tokens(limit)))
        .unwrap_or_default();
    let system = report
        .layers
        .iter()
        .find(|l| l.id == "system_prompt")
        .map(|l| format_tokens(l.tokens_estimate))
        .unwrap_or_else(|| "?".to_string());
    let tools = report
        .layers
        .iter()
        .find(|l| l.id == "tool_definitions")
        .map(|l| format_tokens(l.tokens_estimate))
        .unwrap_or_else(|| "?".to_string());
    let accounting = format!("Context ~{total}{limit} · system ~{system} · tools ~{tools}");
    let Some(hud) = &report.hud else {
        return accounting;
    };
    let mut parts = Vec::new();
    if let Some(work) = &hud.work {
        parts.push(format!(
            "Work {}: {} (authoritative lifecycle, not task verification)",
            work.work_id, work.phase
        ));
        if let Some(progress) = &work.progress {
            parts.push(progress.clone());
        }
        parts.extend(work.blockers.iter().map(|b| format!("Blocker: {b}")));
        parts.extend(
            work.decisions
                .iter()
                .map(|d| format!("Decision accounting: {d}")),
        );
    }
    if hud.deferred_items > 0 {
        parts.push(format!(
            "{} context items deferred; native retrieval retained where available",
            hud.deferred_items
        ));
    }
    parts.push(accounting);
    parts.join(" · ")
}

/// Latest inference accounting and safe projections, also for non-first rounds.
pub fn inference_usage_report(
    footprint: &medousa_runtime::inference_usage::RequestFootprint,
) -> Option<ContextUsageReport> {
    let window = footprint.inference_window.as_ref()?;
    let mut layers: Vec<ContextUsageLayer> = Vec::new();
    for item in &window.decisions.contributions {
        if matches!(
            item.decision,
            medousa_runtime::context::ContextDecision::Omitted { .. }
                | medousa_runtime::context::ContextDecision::Excluded { .. }
        ) {
            continue;
        }
        if let Some(existing) = layers.iter_mut().find(|l| l.id == item.id) {
            existing.chars = existing
                .chars
                .saturating_add(u32::try_from(item.cost.text_chars).unwrap_or(u32::MAX));
            existing.tokens_estimate = chars_to_tokens(existing.chars as usize);
        } else {
            layers.push(layer(&item.id, layer_label(&item.id), item.cost.text_chars));
        }
    }
    Some(ContextUsageReport {
        hud: window.hud.clone(),
        layers,
        total_chars: u32::try_from(window.decisions.cost.text_chars).unwrap_or(u32::MAX),
        total_tokens_estimate: window.estimate.total_tokens,
        context_limit_tokens: window.policy.input_tokens,
        tool_count: u32::try_from(footprint.tool_count.unwrap_or(0)).unwrap_or(u32::MAX),
        estimator: format!(
            "{}; estimated_total_including_protocol_and_media; layer_estimates_text_only; input_budget={}; reconciled={}",
            window.accounting, window.input_budget, window.reconciled
        ),
    })
}

/// Ephemeral per-turn semantic coalescer, never authoritative work state.
#[derive(Default)]
pub struct ContextUpdateCoalescer {
    previous: Option<medousa_types::context_hud::UserContextProjection>,
    published: bool,
}
impl ContextUpdateCoalescer {
    pub fn should_publish(&mut self, report: &ContextUsageReport) -> bool {
        let changed = !self.published
            || match (&report.hud, &self.previous) {
                (Some(next), Some(previous)) => next.meaningful_change_from(previous),
                (None, None) => false,
                _ => true,
            };
        self.previous = report.hud.clone();
        self.published = true;
        changed
    }
}

/// Fill percentage when `context_limit_tokens` is known.
pub fn usage_fill_percent(report: &ContextUsageReport) -> Option<u32> {
    let limit = report.context_limit_tokens?;
    if limit == 0 {
        return None;
    }
    let pct = (report.total_tokens_estimate as f64 / limit as f64) * 100.0;
    Some(pct.round().clamp(0.0, 100.0) as u32)
}

/// Human-readable multiline breakdown for TUI, WhatsApp, Telegram, etc.
pub fn format_context_usage_text(report: &ContextUsageReport) -> String {
    let mut lines = Vec::new();
    if report.hud.is_some() {
        lines.push(operator_summary(report));
    }
    if let Some(pct) = usage_fill_percent(report) {
        let total = format_tokens(report.total_tokens_estimate);
        let limit = report
            .context_limit_tokens
            .map(format_tokens)
            .unwrap_or_else(|| "?".to_string());
        lines.push(format!("Context {pct}% full · ~{total} / {limit} tokens"));
    } else {
        lines.push(format!(
            "Context ~{} tokens (estimator: {})",
            format_tokens(report.total_tokens_estimate),
            report.estimator
        ));
    }
    lines.push(String::new());
    for layer in &report.layers {
        lines.push(format!(
            "  {} — {}",
            layer.label,
            format_tokens(layer.tokens_estimate)
        ));
    }
    lines.join("\n")
}

fn format_tokens(n: u32) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn codex_account_budget_handles_old_caches_and_advertised_limits() {
        use crate::model_capability_registry::heuristic::{
            chatgpt_catalog_record, infer_capability,
        };
        let old = infer_capability("openai-codex", "gpt-6.1-sol");
        for record in [None, Some(&old)] {
            let policy = super::policy_for_record("openai-codex", "gpt-6.1-sol", record);
            assert_eq!(policy.input_tokens, None); // fallback is not provider metadata
            assert_eq!(policy.input_budget(), 266880);
            assert!(!policy.enforce_output_limit); // bridge rejects output caps
            let mut request =
                genai::chat::ChatRequest::new(vec![genai::chat::ChatMessage::user("hello")])
                    .with_system("s".repeat(9000))
                    .with_tools(vec![genai::chat::Tool::new("read").with_schema(
                        serde_json::json!({
                            "description": "x".repeat(50000), "type": "object"
                        }),
                    )]);
            let report = medousa_runtime::context::ContextCoordinator::compile_inference(
                &mut request,
                &policy,
                0,
                1,
                None,
                &mut medousa_runtime::perception::ToolPerceptionGovernor::default(),
            )
            .unwrap();
            assert!(report.estimate.total_tokens < 20000);
            assert!(report.accounting.contains("o200k_proxy"));
            assert_eq!(report.deferred_history_messages, 0);
        }
        let small = chatgpt_catalog_record(&serde_json::json!({
            "slug": "gpt-6.1-sol", "context_window": 32000,
            "effective_context_window_percent": 100, "max_output_tokens": 8192
        }));
        let policy = super::policy_for_record("openai-codex", "gpt-6.1-sol", Some(&small));
        assert_eq!(policy.input_budget(), 22784);
        assert_eq!(
            super::policy_for_record("other", "unknown", None).input_budget(),
            2880
        );
        assert_eq!(
            super::policy_for_record("openai-codex", "unknown", None).input_budget(),
            2880
        );
    }

    use super::*;

    #[test]
    fn user_updates_coalesce_accounting_but_preserve_work_blockers_and_decisions() {
        use medousa_types::context_hud::{UserContextProjection, WorkContextProjection};
        let mut report = ContextUsageReport {
            hud: Some(UserContextProjection {
                work: Some(WorkContextProjection {
                    work_id: "work-1".into(),
                    phase: "executing".into(),
                    progress: Some("one recorded attempt".into()),
                    blockers: vec![],
                    decisions: vec![],
                }),
                pressure: "normal".into(),
                deferred_items: 0,
                accounting: "estimated".into(),
            }),
            layers: Vec::new(),
            total_chars: 100,
            total_tokens_estimate: 25,
            context_limit_tokens: Some(8000),
            tool_count: 1,
            estimator: "estimated".into(),
        };
        let mut coalescer = ContextUpdateCoalescer::default();
        assert!(coalescer.should_publish(&report));
        report.total_tokens_estimate = 40;
        report.hud.as_mut().unwrap().pressure = "high".into();
        assert!(!coalescer.should_publish(&report));
        report
            .hud
            .as_mut()
            .unwrap()
            .work
            .as_mut()
            .unwrap()
            .blockers
            .push("awaiting_owner_decision".into());
        assert!(coalescer.should_publish(&report));
        assert!(operator_summary(&report).contains("Blocker: awaiting_owner_decision"));
        assert!(!coalescer.should_publish(&report));
        report
            .hud
            .as_mut()
            .unwrap()
            .work
            .as_mut()
            .unwrap()
            .decisions
            .push("review_decision_recorded".into());
        assert!(coalescer.should_publish(&report));
        assert!(!operator_summary(&report).contains("completed"));
        assert_eq!(
            report.hud.as_ref().unwrap().work.as_ref().unwrap().phase,
            "executing"
        );
    }

    #[tokio::test]
    async fn first_request_reports_effective_profile_schemas_and_empty_catalog() {
        use async_trait::async_trait;
        use genai::chat::{
            ChatMessage, ChatOptions, ChatRequest, ChatResponse, MessageContent, Tool,
        };
        use medousa_runtime::context::{ContextCoordinator, ContextCost, ContextOwner, Retention};
        use medousa_runtime::request_context::FirstRequestContextConfig;
        use stasis::application::orchestration::prompt_pipeline::{
            PromptExecutionContext, PromptExecutionPipeline,
        };
        use stasis::application::orchestration::tool_loop_pipeline::{
            ToolCallMode, ToolLoopExecutionRequest,
        };
        use stasis::domain::errors::{Result, StasisError};
        use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta};
        use std::sync::Mutex;
        struct Registry {
            fail: bool,
        }
        #[async_trait]
        impl ToolRegistry for Registry {
            async fn list_tools(&self) -> Result<Vec<Tool>> {
                if self.fail {
                    return Err(StasisError::PortFailure("catalog unavailable".into()));
                }
                Ok(vec![
                    Tool::new("allowed_tool").with_schema(
                        serde_json::json!({"type":"object","properties":{"α":{"type":"string"}}}),
                    ),
                    Tool::new("denied_tool").with_schema(
                        serde_json::json!({"type":"object","properties":{"x":{"type":"integer"}}}),
                    ),
                ])
            }
            async fn invoke_tool(
                &self,
                _: &str,
                _: serde_json::Value,
            ) -> Result<serde_json::Value> {
                unreachable!()
            }
        }
        #[derive(Default)]
        struct Client(Mutex<Vec<ChatRequest>>);
        #[async_trait]
        impl AiChatClient for Client {
            async fn complete(
                &self,
                request: ChatRequest,
                _: Option<&ChatOptions>,
            ) -> Result<ChatResponse> {
                self.0.lock().unwrap().push(request);
                let model_iden = genai::ModelIden::from_static(
                    genai::adapter::AdapterKind::OpenAI,
                    "context-test",
                );
                Ok(ChatResponse {
                    content: MessageContent::from("answer"),
                    reasoning_content: None,
                    model_iden: model_iden.clone(),
                    provider_model_iden: model_iden,
                    stop_reason: None,
                    response_id: None,
                    usage: Default::default(),
                    captured_raw_body: None,
                })
            }
            async fn complete_stream(
                &self,
                request: ChatRequest,
                options: Option<&ChatOptions>,
                _: Option<&tokio::sync::mpsc::Sender<StreamDelta>>,
            ) -> Result<ChatResponse> {
                self.complete(request, options).await
            }
        }
        let underlying: Arc<dyn ToolRegistry> = Arc::new(Registry { fail: false });
        assert_eq!(underlying.list_tools().await.unwrap().len(), 2);
        for permitted in [vec!["allowed_tool".to_string()], vec![]] {
            let registry: Arc<dyn ToolRegistry> = Arc::new(
                crate::agent_runtime::turn_worker::SessionBootstrapToolRegistry::host(
                    underlying.clone(),
                    "context-profile",
                    permitted.iter().cloned().collect(),
                    false,
                    false,
                    None,
                    crate::client_tools::ClientRegistry::new(),
                ),
            );
            let effective = measure_tool_schema_footprint(&registry).await;
            assert!(effective.known);
            assert_eq!(effective.tool_count, permitted.len());
            let client = Arc::new(Client::default());
            let observed = Arc::new(Mutex::new(Vec::new()));
            let captured = observed.clone();
            let mut seed = ContextCoordinator::default();
            for (id, owner, chars) in [
                ("user_message", ContextOwner::Chat, 8),
                ("system_prompt", ContextOwner::Host, 6),
                ("tool_definitions", ContextOwner::Tool, 9999),
            ] {
                seed.include(super::super::context_assembly::contribution(
                    id,
                    owner,
                    "context-profile",
                    ContextCost::text(chars),
                    Retention::Required,
                    None,
                ));
            }
            let pipeline = medousa_runtime::MedousaToolLoopPipeline::new(
                PromptExecutionPipeline::new(client.clone()),
                registry,
            )
            .with_first_request_context(FirstRequestContextConfig {
                inference_observer: None,
                assembly: Some({
                    let mut report = seed.report(Default::default());
                    report.prepared_messages_digest =
                        Some(medousa_runtime::context::prepared_messages_digest(&[
                            genai::chat::ChatMessage::user("question"),
                        ]));
                    report
                }),
                observer: Some(Arc::new(move |footprint| {
                    captured.lock().unwrap().push(footprint);
                    Box::pin(async {})
                })),
            });
            let request = ToolLoopExecutionRequest {
                user_prompt: "question".into(),
                system_prompt: Some("policy".into()),
                context: PromptExecutionContext::default(),
                tool_name: String::new(),
                tool_input: serde_json::Value::Null,
                tool_call_mode: ToolCallMode::Auto,
            };
            let boundary = Arc::new(
                medousa_runtime::execution_boundary::TurnExecutionBoundary::new(
                    tokio_util::sync::CancellationToken::new(),
                    None,
                ),
            );
            let result = medousa_runtime::execution_boundary::with_turn_execution_boundary(
                boundary,
                pipeline.execute_with_prior_messages(request, Vec::new()),
            )
            .await;
            if permitted.is_empty() {
                // Legacy tool-loop semantics: a known empty effective catalog
                // aborts before inference. Do not invent a sent empty request.
                assert!(result.is_err());
                assert!(client.0.lock().unwrap().is_empty());
                assert!(observed.lock().unwrap().is_empty());
                continue;
            }
            result.unwrap();
            let requests = client.0.lock().unwrap();
            let footprints = observed.lock().unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(footprints.len(), 1);
            let report = footprints[0].first_request_context.as_ref().unwrap();
            assert!(report.reconciled, "{report:?}");
            assert_eq!(report.tool_count, Some(permitted.len()));
            assert_eq!(
                requests[0]
                    .tools
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|tool| tool.name.to_string())
                    .collect::<Vec<_>>(),
                permitted
            );
            let actual =
                measure_tool_schema_footprint_for_tools(requests[0].tools.as_deref().unwrap());
            assert_eq!(report.schema_chars, actual.total_chars);
            let usage = first_request_usage_report(&footprints[0]).unwrap();
            assert_eq!(usage.tool_count as usize, actual.tool_count);
            assert_eq!(usage.total_chars as usize, report.decisions.cost.text_chars);
            assert!(usage.estimator.contains("first_request; reconciled=true"));
            assert!(
                report
                    .decisions
                    .contributions
                    .iter()
                    .any(|item| item.id == "user_message"
                        && item.provenance.scope.as_deref() == Some("context-profile"))
            );
        }
        let failed: Arc<dyn ToolRegistry> = Arc::new(Registry { fail: true });
        let unknown = measure_tool_schema_footprint(&failed).await;
        assert!(!unknown.known);
        assert!(measure_tool_schema_footprint_for_tools(&[]).known);
        let failed_client = Arc::new(Client::default());
        let failed_pipeline = medousa_runtime::MedousaToolLoopPipeline::new(
            PromptExecutionPipeline::new(failed_client.clone()),
            failed,
        );
        let request = ToolLoopExecutionRequest {
            user_prompt: "question".into(),
            system_prompt: Some("policy".into()),
            context: PromptExecutionContext::default(),
            tool_name: String::new(),
            tool_input: serde_json::Value::Null,
            tool_call_mode: ToolCallMode::Auto,
        };
        let boundary = Arc::new(
            medousa_runtime::execution_boundary::TurnExecutionBoundary::new(
                tokio_util::sync::CancellationToken::new(),
                None,
            ),
        );
        let result = medousa_runtime::execution_boundary::with_turn_execution_boundary(
            boundary,
            failed_pipeline.execute_with_prior_messages(request, Vec::new()),
        )
        .await;
        assert!(result.is_err());
        assert!(
            failed_client.0.lock().unwrap().is_empty(),
            "failed catalog must not masquerade as an empty request"
        );
        // The request contract also distinguishes an explicit empty schema array
        // from no-tools None, even though the legacy tool loop rejects empty catalogs.
        let empty_request = ChatRequest::from_user("question").with_tools(Vec::<Tool>::new());
        let empty =
            medousa_runtime::inference_usage::RequestFootprint::new(&empty_request, None, None)
                .with_first_request_context(&empty_request, 0, 1, None);
        assert_eq!(
            empty.first_request_context.as_ref().unwrap().tool_count,
            Some(0)
        );
        assert!(empty.first_request_context.as_ref().unwrap().reconciled);
        // Actual no-tools requests use None, versus the effective empty Some([]).
        let client = Client::default();
        let request = ChatRequest::new(vec![
            ChatMessage::system("policy"),
            ChatMessage::user("question"),
        ]);
        let footprint =
            medousa_runtime::inference_usage::RequestFootprint::new(&request, None, None)
                .with_first_request_context(&request, 1, 1, None);
        client.complete(request, None).await.unwrap();
        assert!(client.0.lock().unwrap()[0].tools.is_none());
        let context = footprint.first_request_context.as_ref().unwrap();
        assert_eq!(context.tool_count, None);
        assert_eq!(context.schema_chars, 0);
        assert!(context.reconciled);
        assert!(
            first_request_usage_report(&footprint)
                .unwrap()
                .estimator
                .contains("tools=disabled")
        );
    }

    #[test]
    fn complete_layers_reconcile_with_actual_text_and_unknown_media() {
        use super::super::context_assembly::{PromptLedger, contribution};
        use genai::chat::{ChatMessage, ContentPart, MessageContent};
        use medousa_runtime::context::{ContextCoordinator, ContextCost, ContextOwner, Retention};
        let mut ledger = PromptLedger::new("session-a", "  🦀 request  ");
        let mut resolved = "  🦀 request  ".to_string();
        for (id, owner) in [
            ("context_pack", ContextOwner::Host),
            ("memory_recall", ContextOwner::Host),
            ("identity_manuscript", ContextOwner::Host),
            ("bot_profile", ContextOwner::Host),
            ("world_references", ContextOwner::Host),
            ("voice", ContextOwner::Host),
            ("suggested_capabilities", ContextOwner::Tool),
            ("identity", ContextOwner::Host),
            ("active_work", ContextOwner::UnitOfWork),
            ("compiler_metadata", ContextOwner::Host),
            ("ambient", ContextOwner::Host),
            ("admitted_goal_and_work", ContextOwner::UnitOfWork),
        ] {
            resolved = format!("{}\n\n{id}", resolved.trim());
            ledger.observe(id, owner, &resolved);
        }
        let request = format!("{resolved}\n\npolicy");
        let current = ChatMessage::user(MessageContent::from_parts(vec![
            ContentPart::Text(request.clone()),
            ContentPart::from_binary_base64("image/png", "AAAA", None),
        ]));
        let mut context = ContextCoordinator::default();
        let mut messages = Vec::new();
        for (id, owner) in [
            ("prior_conversation", ContextOwner::Chat),
            ("cold_history", ContextOwner::Chat),
            ("tool_slices", ContextOwner::Tool),
            ("grapheme_scripts", ContextOwner::Tool),
            ("runtime_learnings", ContextOwner::Tool),
            ("tool_hints", ContextOwner::Tool),
            ("history_media", ContextOwner::Chat),
        ] {
            messages.push(ChatMessage::assistant(id));
            context.include(contribution(
                id,
                owner,
                "session-a",
                ContextCost::text(id.chars().count()),
                Retention::Flexible,
                Some(100),
            ));
        }
        let prior = PriorMessageBuild {
            messages,
            context,
            hot_turns_included: 1,
            cold_turns_summarized: 1,
            cold_summary_chars: 0,
            tool_slices_chars: 0,
            script_recall_chars: 0,
            learning_recall_chars: 0,
            tool_hints_chars: 0,
            // Deliberately wrong legacy counters must NOT drive accounting.
            total_chars: 999_999,
        };
        for limit in [None, Some(1), Some(200_000)] {
            let compiled = build_context_usage_report(ContextUsageInput {
                session_id: "session-a",
                system_prompt_chars: 16_000,
                prompt_ledger: &ledger,
                resolved_prompt: &resolved,
                prompt_for_request: &request,
                current_user_message: &current,
                sticky_chars: 250,
                prior_build: &prior,
                tool_count: 28,
                tool_schema_chars: 36_000,
                tool_schema_known: true,
                context_limit_tokens: limit,
            });
            assert!(compiled.assembly_reconciled);
            assert!(!compiled.reconciled);
            let mut prepared = prior.messages.clone();
            prepared.push(current.clone());
            assert_eq!(
                compiled.decisions.prepared_messages_digest,
                Some(medousa_runtime::context::prepared_messages_digest(
                    &prepared
                ))
            );
            let prepared_count = prepared.len();
            prepared.push(ChatMessage::system(format!(
                "{}\ngoal",
                medousa_runtime::turn_context::SCRATCH_PREFIX
            )));
            let actual_request = genai::chat::ChatRequest::new(prepared);
            let footprint = medousa_runtime::inference_usage::RequestFootprint::new(
                &actual_request,
                None,
                None,
            )
            .with_first_request_context(
                &actual_request,
                0,
                prepared_count,
                Some(&compiled.decisions),
            );
            let first = footprint.first_request_context.as_ref().unwrap();
            assert!(first.reconciled);
            assert_eq!(first.prepared_provenance_preserved, Some(true));
            let report = compiled.usage;
            assert_eq!(
                report.layers.iter().map(|layer| layer.chars).sum::<u32>(),
                report.total_chars
            );
            assert_eq!(compiled.decisions.cost.unknown_parts, 1);
            assert!(!compiled.decisions.complete_token_cost_known);
            assert_eq!(
                compiled.decisions.text_estimate_exceeds_capacity,
                limit.map(|limit| report.total_tokens_estimate > limit)
            );
            assert!(
                compiled.decisions.contributions.iter().all(|item| item
                    .provenance
                    .scope
                    .as_deref()
                    == Some("session-a"))
            );
            for id in [
                "system_prompt",
                "tool_definitions",
                "chat_stickies",
                "ambient",
                "admitted_goal_and_work",
                "history_media",
                "current_media",
                "tool_policy",
            ] {
                assert!(
                    report.layers.iter().any(|layer| layer.id == id),
                    "missing {id}"
                );
            }
            assert!(operator_summary(&report).contains("Context"));
            let text = format_context_usage_text(&report);
            assert!(text.contains("System prompt"));
            assert!(text.contains("Tool definitions"));
            assert!(report.estimator.contains("text_only"));
        }
    }

    #[tokio::test]
    async fn failed_catalog_enumeration_is_unknown_not_empty() {
        struct FailingRegistry;
        #[async_trait::async_trait]
        impl ToolRegistry for FailingRegistry {
            async fn list_tools(&self) -> stasis::prelude::Result<Vec<genai::chat::Tool>> {
                Err(stasis::prelude::StasisError::PortFailure(
                    "catalog unavailable".into(),
                ))
            }
            async fn invoke_tool(
                &self,
                _: &str,
                _: serde_json::Value,
            ) -> stasis::prelude::Result<serde_json::Value> {
                unreachable!("accounting does not execute tools")
            }
        }
        let registry: Arc<dyn ToolRegistry> = Arc::new(FailingRegistry);
        let footprint = measure_tool_schema_footprint(&registry).await;
        assert!(!footprint.known);
        assert_eq!(footprint.total_chars, 0);
        assert!(measure_tool_schema_footprint_for_tools(&[]).known);
    }

    #[test]
    fn tool_schema_footprint_ranks_the_largest_contracts() {
        let tools = vec![
            genai::chat::Tool::new("small"),
            genai::chat::Tool::new("large").with_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "A deliberately longer contract for footprint ordering"
                    }
                }
            })),
        ];
        let footprint = measure_tool_schema_footprint_for_tools(&tools);
        assert_eq!(footprint.tool_count, 2);
        assert_eq!(footprint.tools[0].name, "large");
        assert_eq!(
            footprint.total_chars,
            footprint.tools.iter().map(|tool| tool.chars).sum::<usize>()
        );
        assert!(footprint.tools.iter().all(|tool| tool.tokens_estimate > 0));
    }
}
