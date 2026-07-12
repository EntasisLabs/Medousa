//! MirV1 host bridge for Grapheme `medousa.*` capabilities.
//!
//! Three ops only:
//! - `medousa.digest` — bounded context compile (no LLM)
//! - `medousa.synthesize` — single-shot model pass (no tool loop)
//! - `medousa.deliver` — terminal effect (work, channel, locus, quiet)

use std::future::IntoFuture;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chrono::Utc;
use grapheme_runtime::host::{CapabilityCall, HostCallError};
use grapheme_sdk::{GraphemeEngine, GraphemeEngineBuilder, GraphemeSdkError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline, PromptExecutionRequest,
};
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::infrastructure::runtime::grapheme_sdk_workflow_engine::GraphemeWorkflowGuardrails;
use stasis::ports::outbound::ai_chat_client::AiChatClient;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_models::MemoryStoreRequest;
use stasis::ports::outbound::runtime::workflow_engine::{WorkflowEngine, WorkflowExecutionOutput};
use tokio::runtime::Handle;
use uuid::Uuid;

use crate::channel_delivery::{self, ChannelDeliveryTarget, normalize_channel_surface};
use crate::cognitive_identity::{
    DEFAULT_RELATIONAL_DIGEST_BUDGET, DigestCompileOptions, compile_relational_memory_digest_with_options,
    load_cognitive_identity_snapshot,
};
use crate::daemon_api::{WorkspaceEvent, WorkspaceEventActor, WorkspaceEventKind, WorkspaceEventRef};
use crate::engine_context::{ContextCompilerInput, EngineExecutionLane, RecallReadiness, compile_context_prompt};
use crate::identity_manuscript::{
    build_manuscript_context, digest_options_for_manuscript, format_manuscript_prompt_block,
};
use crate::identity_memory::resolve_identity_user_id;
use crate::locus_memory::resolve_workshop_locus_session;
use crate::workspace::event::new_event_id;
use crate::workspace::store::workspace_store;

const MEDOUSA_MODULE: &str = "medousa";
const DEFAULT_DIGEST_BUDGET: usize = 8_000;
const DEFAULT_SYNTHESIS_POLICY: &str = "scheduled";

#[derive(Clone)]
pub struct MedousaBridgeDeps {
    pub chat_client: Arc<dyn AiChatClient>,
    pub identity_store: Option<Arc<dyn IdentityMemoryStore>>,
    pub memory_writer: Option<Arc<dyn MemoryContextWriter>>,
}

static BRIDGE_DEPS: OnceLock<Arc<MedousaBridgeDeps>> = OnceLock::new();

pub fn init_medousa_bridge(deps: MedousaBridgeDeps) {
    let _ = BRIDGE_DEPS.set(Arc::new(deps));
}

pub fn medousa_bridge_configured() -> bool {
    BRIDGE_DEPS.get().is_some()
}

fn bridge_deps() -> Option<Arc<MedousaBridgeDeps>> {
    BRIDGE_DEPS.get().cloned()
}

pub fn configure_grapheme_engine_builder(builder: GraphemeEngineBuilder) -> GraphemeEngineBuilder {
    builder
        .configure_module_registry(|registry| {
            register_medousa_host_module(registry);
            crate::shell_grapheme::register_shell_host_module(registry);
        })
        .with_default_hotload_store()
        .with_capability_interceptor(medousa_and_shell_capability_interceptor())
}

fn medousa_and_shell_capability_interceptor(
) -> impl Fn(&CapabilityCall) -> Option<Result<Value, HostCallError>> + Send + Sync + 'static {
    move |call: &CapabilityCall| {
        if let Some(result) = try_medousa_call(call) {
            return Some(result);
        }
        crate::shell_grapheme::intercept_shell_call(call)
    }
}

fn register_medousa_host_module(registry: &mut grapheme_runtime::ModuleRegistry) {
    use grapheme_runtime::{EffectKind, ExportedOp, ModuleAbi, ModuleManifest, ResourceLimits};

    registry.register_host_module(ModuleManifest {
        module_id: MEDOUSA_MODULE.to_string(),
        version: "0.1.0".to_string(),
        abi: ModuleAbi::MirV1,
        entrypoint: "medousa.host".to_string(),
        exported_ops: vec![
            ExportedOp {
                op: "digest".to_string(),
                input_schema_ref: None,
                output_schema_ref: None,
                effect: EffectKind::Pure,
            },
            ExportedOp {
                op: "synthesize".to_string(),
                input_schema_ref: None,
                output_schema_ref: None,
                effect: EffectKind::Control,
            },
            ExportedOp {
                op: "deliver".to_string(),
                input_schema_ref: None,
                output_schema_ref: None,
                effect: EffectKind::Control,
            },
        ],
        required_capabilities: vec![],
        limits: ResourceLimits {
            max_cpu_ms: 30_000,
            max_memory_mb: 256,
            max_io_bytes: 16 * 1024 * 1024,
            max_network_calls: 8,
        },
    });
}

pub fn medousa_workflow_engine() -> Arc<dyn WorkflowEngine> {
    Arc::new(MedousaWorkflowEngine::new())
}

pub struct MedousaWorkflowEngine {
    guardrails: GraphemeWorkflowGuardrails,
}

impl MedousaWorkflowEngine {
    pub fn new() -> Self {
        Self {
            guardrails: GraphemeWorkflowGuardrails::default(),
        }
    }

    fn build_engine(guardrails: &GraphemeWorkflowGuardrails) -> GraphemeEngine {
        configure_grapheme_engine_builder(GraphemeEngine::builder())
            .with_max_steps(guardrails.max_steps)
            .with_max_call_depth(guardrails.max_call_depth)
            .build()
    }

    /// Reuse a single process-global `GraphemeEngine` instead of rebuilding one
    /// (host-module registration, hotload store, capability interceptor) on every
    /// execution. The per-call `state.current` seed is applied at execution time
    /// via `execute_source_with_initial_state`, so caching the engine does not
    /// change execution behavior. Guardrails are process-constant
    /// (`GraphemeWorkflowGuardrails::default`), so the first initializer wins.
    fn shared_engine(guardrails: &GraphemeWorkflowGuardrails) -> &'static GraphemeEngine {
        static WORKFLOW_ENGINE: OnceLock<GraphemeEngine> = OnceLock::new();
        WORKFLOW_ENGINE.get_or_init(|| Self::build_engine(guardrails))
    }

    fn validate_source(&self, source: &str) -> StasisResult<()> {
        if source.len() > self.guardrails.max_source_bytes {
            return Err(StasisError::PortFailure(format!(
                "grapheme policy violation: source size {} exceeds max {} bytes",
                source.len(),
                self.guardrails.max_source_bytes
            )));
        }

        for import in Self::extract_imports(source) {
            if !self
                .guardrails
                .allowed_imports
                .iter()
                .any(|pattern| Self::import_is_allowed(pattern, &import))
            {
                return Err(StasisError::PortFailure(format!(
                    "grapheme policy violation: import '{import}' is not allowlisted"
                )));
            }
        }

        Ok(())
    }

    fn import_is_allowed(pattern: &str, import: &str) -> bool {
        if let Some(prefix) = pattern.strip_suffix('*') {
            return import.starts_with(prefix);
        }
        pattern == import
    }

    fn extract_imports(source: &str) -> Vec<String> {
        source
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if !trimmed.starts_with("import ") {
                    return None;
                }

                let quote = if trimmed.contains('"') { '"' } else { '\'' };
                let start = trimmed.find(quote)?;
                let tail = &trimmed[(start + 1)..];
                let end = tail.find(quote)?;
                Some(tail[..end].to_string())
            })
            .collect()
    }

    fn map_error(err: GraphemeSdkError) -> StasisError {
        let msg = err.to_string();
        if msg.contains("policy:") {
            return StasisError::PortFailure(format!("grapheme policy violation: {msg}"));
        }
        StasisError::PortFailure(format!("grapheme sdk execution error: {msg}"))
    }
}

impl Default for MedousaWorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WorkflowEngine for MedousaWorkflowEngine {
    async fn execute_grapheme_source(
        &self,
        source: &str,
        state_current: Option<&Value>,
    ) -> StasisResult<WorkflowExecutionOutput> {
        self.validate_source(source)?;
        let guardrails = self.guardrails.clone();
        let timeout = guardrails.execution_timeout;
        if timeout.is_zero() {
            return Err(StasisError::PortFailure(
                "grapheme policy violation: execution timeout must be greater than 0ms".to_string(),
            ));
        }

        let source_owned = source.to_string();
        let state_current_owned = state_current.cloned();
        let guardrails_clone = guardrails.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let engine = Self::shared_engine(&guardrails_clone);
            engine.execute_source_with_initial_state(&source_owned, state_current_owned)
        });

        let result = tokio::time::timeout(timeout, handle)
            .await
            .map_err(|_| {
                StasisError::PortFailure(format!(
                    "grapheme policy violation: execution timed out after {} ms",
                    timeout.as_millis()
                ))
            })?
            .map_err(|err| StasisError::PortFailure(format!("grapheme sdk worker join error: {err}")))?
            .map_err(Self::map_error)?;

        Ok(WorkflowExecutionOutput {
            run_id: format!("grapheme:{}", result.artifact_id),
            execution: serde_json::to_value(&result.execution).unwrap_or(Value::Null),
            final_state: result.final_state,
            lint_warnings: serde_json::to_value(&result.lint_warnings).unwrap_or(Value::Null),
        })
    }
}

fn try_medousa_call(call: &CapabilityCall) -> Option<Result<Value, HostCallError>> {
    if !is_medousa_call(call) {
        return None;
    }
    let op = resolve_medousa_op(call);
    Some(match op.as_str() {
        "digest" => handle_digest(&call.args),
        "synthesize" => handle_synthesize(&call.args),
        "deliver" => handle_deliver(&call.args),
        other => Err(HostCallError::Fatal(format!(
            "unsupported medousa op '{other}' (expected digest, synthesize, or deliver)"
        ))),
    })
}

fn is_medousa_call(call: &CapabilityCall) -> bool {
    let module = call
        .module
        .as_deref()
        .or_else(|| call.capability.split('.').next())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    module == MEDOUSA_MODULE
}

fn resolve_medousa_op(call: &CapabilityCall) -> String {
    let raw = if call.op.contains('.') {
        call.op.rsplit('.').next().unwrap_or(&call.op)
    } else {
        call.op.as_str()
    };
    raw.trim().to_ascii_lowercase()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDigest {
    pub digest_ref: String,
    pub session_id: String,
    pub created_at_utc: String,
    pub text: String,
    pub token_estimate: usize,
    pub sections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSynthesis {
    pub synthesis_ref: String,
    pub session_id: String,
    pub created_at_utc: String,
    pub outcome: String,
    #[serde(default)]
    pub structured: Option<Value>,
}

fn handle_digest(args: &Value) -> Result<Value, HostCallError> {
    let session_id = arg_string(args, &["session_id", "session"]).unwrap_or_else(|| "default".to_string());
    let query = arg_string(args, &["query", "q"]);
    let manuscript_id = arg_string(args, &["manuscript_id", "manuscript"]);
    let artifact_ref = arg_string(args, &["artifact_ref", "artifact_id", "artifact"]);
    let pack_ref = arg_string(args, &["pack_ref", "context_pack"]);
    let inline_input = arg_input_text(args, &["input", "text", "body"]);
    let max_chars = arg_usize(args, &["max_chars", "budget"]).unwrap_or(DEFAULT_DIGEST_BUDGET);

    let mut sections = Vec::new();
    let mut body = String::new();

    if let Some(manuscript_id) = manuscript_id.as_deref() {
        match build_manuscript_context(manuscript_id) {
            Ok(manuscript) => {
                sections.push("manuscript".to_string());
                body.push_str(&format_manuscript_prompt_block(&manuscript));
                body.push('\n');
            }
            Err(err) => {
                body.push_str(&format!("[MEDOUSA_MANUSCRIPT]\nstatus=error\nerror={err}\n"));
            }
        }
    }

    if let Some(identity_text) = compile_identity_digest_block(query.as_deref(), manuscript_id.as_deref()) {
        if !identity_text.trim().is_empty() {
            sections.push("identity".to_string());
            body.push_str(&identity_text);
            body.push('\n');
        }
    }

    if let Some(pack_selector) = pack_ref.as_deref() {
        if let Some(pack) = crate::context_pack::find_context_pack(&session_id, Some(pack_selector)) {
            sections.push("context_pack".to_string());
            body.push_str(&format!(
                "[MEDOUSA_CONTEXT_PACK]\npack_id={}\nartifact_id={}\nclaims={}\nchunks={}\ntokens={}\n",
                pack.pack_id,
                pack.artifact_id,
                pack.selected_claims.len(),
                pack.selected_chunk_refs.len(),
                pack.total_token_estimate,
            ));
            for claim in pack.selected_claims.iter().take(8) {
                body.push_str("- ");
                body.push_str(claim.statement.trim());
                body.push('\n');
            }
        }
    }

    if let Some(artifact_selector) = artifact_ref.as_deref() {
        if let Some(artifact) = crate::artifact_store::find_artifact(&session_id, Some(artifact_selector)) {
            sections.push("artifact".to_string());
            body.push_str(&format!(
                "[MEDOUSA_ARTIFACT]\nid={}\ntool={}\nbytes={}\n",
                artifact.record.artifact_id,
                artifact.record.tool_name,
                artifact.record.byte_size,
            ));
            if let Some(preview) = artifact_preview(&artifact.payload) {
                body.push_str(&preview);
                body.push('\n');
            }
        }
    }

    if let Some(input) = inline_input {
        sections.push("input".to_string());
        body.push_str("[MEDOUSA_INPUT]\n");
        body.push_str(input.trim());
        body.push('\n');
    }

    if body.trim().is_empty() {
        return Err(HostCallError::Fatal(
            "medousa.digest requires at least one of: input, artifact_ref, pack_ref, manuscript_id"
                .to_string(),
        ));
    }

    let bounded = truncate_chars(body.trim(), max_chars.max(256));
    let token_estimate = bounded.split_whitespace().count();
    let digest_ref = format!("digest:{}:{}", short_session(&session_id), Uuid::new_v4().simple());
    let stored = StoredDigest {
        digest_ref: digest_ref.clone(),
        session_id: session_id.clone(),
        created_at_utc: Utc::now().to_rfc3339(),
        text: bounded.clone(),
        token_estimate,
        sections: sections.clone(),
    };
    persist_digest(&stored)?;

    Ok(json!({
        "digest_ref": digest_ref,
        "session_id": session_id,
        "text": bounded,
        "token_estimate": token_estimate,
        "sections": sections,
    }))
}

fn compile_identity_digest_block(
    query: Option<&str>,
    manuscript_id: Option<&str>,
) -> Option<String> {
    let deps = bridge_deps()?;
    let user_id = resolve_identity_user_id(None);
    let snapshot = block_on(load_cognitive_identity_snapshot(
        deps.identity_store.as_ref(),
        &user_id,
        Some(DEFAULT_SYNTHESIS_POLICY),
        32,
    ));
    let mut options =
        DigestCompileOptions::from_product_config(DEFAULT_RELATIONAL_DIGEST_BUDGET.max(800));
    if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
        options.query_hints = Some(query.trim().to_string());
    }
    if let Some(manuscript_id) = manuscript_id.filter(|value| !value.trim().is_empty()) {
        if let Ok(manuscript) = build_manuscript_context(manuscript_id) {
            options = digest_options_for_manuscript(options, &manuscript);
        }
    }
    let ranked = compile_relational_memory_digest_with_options(&snapshot, options);
    if ranked.text.trim().is_empty() {
        None
    } else {
        Some(format!("{}\n", ranked.text.trim()))
    }
}

fn handle_synthesize(args: &Value) -> Result<Value, HostCallError> {
    let deps = bridge_deps().ok_or_else(|| {
        HostCallError::Fatal(
            "medousa.synthesize requires medousa bridge init (daemon runtime with chat client)"
                .to_string(),
        )
    })?;

    let session_id = arg_string(args, &["session_id", "session"]).unwrap_or_else(|| "default".to_string());
    let instruction = arg_string(args, &["prompt", "instruction", "task"]).ok_or_else(|| {
        HostCallError::Fatal("medousa.synthesize requires prompt/instruction".to_string())
    })?;
    let response_format = arg_string(args, &["response_format", "format"])
        .unwrap_or_else(|| "text".to_string())
        .to_ascii_lowercase();
    let system_appendix = arg_string(args, &["system", "system_prompt"]);

    let digest_text = resolve_digest_text(args, &session_id)?;
    let lane = match arg_string(args, &["lane"]).as_deref() {
        Some("interactive") => EngineExecutionLane::Interactive,
        Some("heartbeat") => EngineExecutionLane::Heartbeat,
        _ => EngineExecutionLane::Scheduled,
    };
    let compiled = compile_context_prompt(ContextCompilerInput {
        lane,
        user_prompt: &instruction,
        response_depth_mode: "standard",
        stage_route: None,
        recall_readiness: RecallReadiness::Verified,
    });

    let mut system_prompt = String::from(
        "You are a single-shot synthesis pass inside a Grapheme automation. \
         Do not request tools. Respond directly to the instruction using the digest context.\n",
    );
    if response_format == "json" {
        system_prompt.push_str("Return valid JSON only with no markdown fences.\n");
    }
    if let Some(extra) = system_appendix {
        system_prompt.push('\n');
        system_prompt.push_str(extra.trim());
    }

    let user_prompt = format!(
        "{}\n\n[MEDOUSA_DIGEST]\n{}\n\n[MEDOUSA_INSTRUCTION]\n{}",
        compiled.compiled_prompt.trim(),
        digest_text.trim(),
        instruction.trim(),
    );

    let pipeline = PromptExecutionPipeline::new(deps.chat_client.clone());
    let request = PromptExecutionRequest::from_user_prompt(user_prompt)
        .with_system_prompt(system_prompt)
        .with_context(PromptExecutionContext {
            policy_profile: Some(DEFAULT_SYNTHESIS_POLICY.to_string()),
            ..PromptExecutionContext::default()
        });

    let response = block_on(pipeline.execute(request)).map_err(|err| {
        HostCallError::Fatal(format!("medousa.synthesize model pass failed: {err}"))
    })?;

    let structured = if response_format == "json" {
        serde_json::from_str::<Value>(response.text.trim()).ok()
    } else {
        None
    };

    let synthesis_ref = format!("synthesis:{}:{}", short_session(&session_id), Uuid::new_v4().simple());
    let stored = StoredSynthesis {
        synthesis_ref: synthesis_ref.clone(),
        session_id: session_id.clone(),
        created_at_utc: Utc::now().to_rfc3339(),
        outcome: response.text.clone(),
        structured: structured.clone(),
    };
    persist_synthesis(&stored)?;

    Ok(json!({
        "synthesis_ref": synthesis_ref,
        "session_id": session_id,
        "outcome": response.text,
        "structured": structured,
        "lane": lane.as_str(),
    }))
}

fn handle_deliver(args: &Value) -> Result<Value, HostCallError> {
    let destination = arg_string(args, &["destination", "target", "channel"])
        .unwrap_or_else(|| "work".to_string())
        .to_ascii_lowercase();
    let session_id = arg_string(args, &["session_id", "session"]).unwrap_or_else(|| "default".to_string());

    if matches!(destination.as_str(), "quiet" | "none" | "noop") {
        return Ok(json!({
            "destination": "quiet",
            "delivered": true,
            "session_id": session_id,
        }));
    }

    let title = arg_string(args, &["title", "summary"]).unwrap_or_else(|| "Grapheme delivery".to_string());
    let body = resolve_delivery_body(args, &session_id)?;

    match destination.as_str() {
        "work" | "board" | "workspace" => deliver_to_work(&title, &body, &session_id),
        "channel" | "push" => deliver_to_channel(args, &session_id, &body),
        "locus" | "memory" => deliver_to_locus(&session_id, &title, &body),
        other => Err(HostCallError::Fatal(format!(
            "unsupported medousa.deliver destination '{other}' (expected work, channel, locus, or quiet)"
        ))),
    }
}

fn deliver_to_work(title: &str, body: &str, session_id: &str) -> Result<Value, HostCallError> {
    let card_id = format!("gph:{}", Uuid::new_v4().simple());
    let event = WorkspaceEvent {
        id: new_event_id(),
        timestamp_utc: Utc::now(),
        kind: WorkspaceEventKind::AgentReplied,
        actor: WorkspaceEventActor::Agent,
        summary: title.trim().to_string(),
        refs: vec![WorkspaceEventRef {
            ref_type: "card".to_string(),
            ref_id: card_id.clone(),
        }],
        detail_line: Some(truncate_chars(body.trim(), 240)),
        context_line: Some(body.trim().to_string()),
        intent: Some("grapheme_medousa_deliver".to_string()),
        tool_names: vec!["medousa.deliver".to_string()],
    };
    workspace_store().append_event(event);
    Ok(json!({
        "destination": "work",
        "delivered": true,
        "card_id": card_id,
        "session_id": session_id,
        "deep_link": channel_delivery::work_deep_link_url(&card_id),
    }))
}

fn deliver_to_channel(args: &Value, session_id: &str, body: &str) -> Result<Value, HostCallError> {
    let channel = arg_string(args, &["channel"]).unwrap_or_else(|| "telegram".to_string());
    let channel_id = arg_string(args, &["channel_id", "chat_id"]).ok_or_else(|| {
        HostCallError::Fatal("medousa.deliver channel destination requires channel_id".to_string())
    })?;
    let user_id = arg_string(args, &["user_id"]).unwrap_or_else(|| session_id.to_string());
    let target = ChannelDeliveryTarget {
        channel: normalize_channel_surface(&channel),
        user_id,
        channel_id,
        session_id: session_id.to_string(),
        stream_id: None,
    };
    let client = reqwest::Client::new();
    block_on(channel_delivery::dispatch_channel_message(&client, &target, body)).map_err(|err| {
        HostCallError::Fatal(format!("medousa.deliver channel dispatch failed: {err}"))
    })?;
    Ok(json!({
        "destination": "channel",
        "delivered": true,
        "channel": target.channel,
        "channel_id": target.channel_id,
        "session_id": session_id,
    }))
}

fn deliver_to_locus(session_id: &str, title: &str, body: &str) -> Result<Value, HostCallError> {
    let deps = bridge_deps().ok_or_else(|| {
        HostCallError::Fatal(
            "medousa.deliver locus requires medousa bridge init with memory writer".to_string(),
        )
    })?;
    let writer = deps
        .memory_writer
        .as_ref()
        .ok_or_else(|| HostCallError::Fatal("memory writer unavailable for locus deliver".to_string()))?;
    let locus_session = resolve_workshop_locus_session(session_id);
    let node = build_deliver_sttp_node(&locus_session, title, body);
    let response = block_on(writer.store_context(&MemoryStoreRequest {
        session_id: locus_session.clone(),
        raw_node: node,
    }))
    .map_err(|err| HostCallError::Fatal(format!("locus store failed: {err}")))?;
    if !response.valid {
        return Err(HostCallError::Fatal(
            response
                .validation_error
                .unwrap_or_else(|| "locus store rejected deliver node".to_string()),
        ));
    }
    Ok(json!({
        "destination": "locus",
        "delivered": true,
        "session_id": session_id,
        "locus_session_id": locus_session,
        "node_id": response.node_id,
    }))
}

fn build_deliver_sttp_node(session_id: &str, title: &str, body: &str) -> String {
    let timestamp = Utc::now().to_rfc3339();
    let escaped_title = title.replace('"', "\\\"");
    let escaped_body = truncate_chars(body, 1200).replace('"', "\\\"");
    format!(
        "⊕⟨ ⏣0{{ trigger: grapheme_deliver, response_format: temporal_node, origin_session: \"{session_id}\", compression_depth: 1, parent_node: null, prime: {{ attractor_config: {{ stability: 0.88, friction: 0.22, logic: 0.96, autonomy: 0.82 }}, context_summary: \"{escaped_title}\", relevant_tier: raw, retrieval_budget: 8 }} }} ⟩\n\
         ⦿⟨ ⏣0{{ timestamp: \"{timestamp}\", tier: raw, session_id: \"{session_id}\", schema_version: \"sttp-1.0\", user_avec: {{ stability: 0.88, friction: 0.22, logic: 0.96, autonomy: 0.82, psi: 2.88 }}, model_avec: {{ stability: 0.88, friction: 0.22, logic: 0.96, autonomy: 0.82, psi: 2.88 }} }} ⟩\n\
         ◈⟨ ⏣0{{ focus(.99): \"{escaped_title}\", decision(.96): {{ summary(.95): \"{escaped_body}\" }} }} ⟩\n\
         ∴"
    )
}

fn resolve_digest_text(args: &Value, session_id: &str) -> Result<String, HostCallError> {
    if let Some(text) = arg_input_text(args, &["digest", "digest_text"]) {
        return Ok(text);
    }
    if let Some(digest_ref) = arg_string(args, &["digest_ref"]) {
        return load_digest_text(&digest_ref).ok_or_else(|| {
            HostCallError::Fatal(format!("digest_ref not found: {digest_ref}"))
        });
    }
    if let Some(piped) = args.get("__input") {
        if let Some(digest_ref) = piped.get("digest_ref").and_then(Value::as_str) {
            return load_digest_text(digest_ref).ok_or_else(|| {
                HostCallError::Fatal(format!("digest_ref not found: {digest_ref}"))
            });
        }
        if let Some(text) = piped.get("text").and_then(Value::as_str) {
            return Ok(text.to_string());
        }
    }
    let _ = session_id;
    Err(HostCallError::Fatal(
        "medousa.synthesize requires digest, digest_ref, or piped digest output".to_string(),
    ))
}

fn resolve_delivery_body(args: &Value, _session_id: &str) -> Result<String, HostCallError> {
    if let Some(body) = arg_input_text(args, &["body", "text", "message"]) {
        return Ok(body);
    }
    if let Some(synthesis_ref) = arg_string(args, &["synthesis_ref"]) {
        return load_synthesis_outcome(&synthesis_ref).ok_or_else(|| {
            HostCallError::Fatal(format!("synthesis_ref not found: {synthesis_ref}"))
        });
    }
    if let Some(digest_ref) = arg_string(args, &["digest_ref"]) {
        return load_digest_text(&digest_ref).ok_or_else(|| {
            HostCallError::Fatal(format!("digest_ref not found: {digest_ref}"))
        });
    }
    if let Some(piped) = args.get("__input") {
        if let Some(synthesis_ref) = piped.get("synthesis_ref").and_then(Value::as_str) {
            return load_synthesis_outcome(synthesis_ref).ok_or_else(|| {
                HostCallError::Fatal(format!("synthesis_ref not found: {synthesis_ref}"))
            });
        }
        if let Some(outcome) = piped.get("outcome").and_then(Value::as_str) {
            return Ok(outcome.to_string());
        }
        if let Some(text) = piped.get("text").and_then(Value::as_str) {
            return Ok(text.to_string());
        }
    }
    Err(HostCallError::Fatal(
        "medousa.deliver requires body, synthesis_ref, digest_ref, or piped synthesis output"
            .to_string(),
    ))
}

fn artifact_preview(payload: &Value) -> Option<String> {
    if let Some(text) = payload.as_str() {
        return Some(truncate_chars(text, 1200));
    }
    payload
        .get("text")
        .or_else(|| payload.get("content"))
        .or_else(|| payload.get("summary"))
        .and_then(|value| value.as_str())
        .map(|text| truncate_chars(text, 1200))
        .or_else(|| {
            serde_json::to_string_pretty(payload)
                .ok()
                .map(|text| truncate_chars(&text, 1200))
        })
}

fn arg_input_text(args: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = args.get(*key) {
            if let Some(text) = value_to_digest_text(value) {
                return Some(text);
            }
        }
    }
    args.get("__input").and_then(value_to_digest_text)
}

fn value_to_digest_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
        return None;
    }
    if value.is_null() {
        return None;
    }
    serde_json::to_string_pretty(value)
        .ok()
        .or_else(|| serde_json::to_string(value).ok())
}

fn arg_string(args: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = args.get(*key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    args.get("__input")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn arg_usize(args: &Value, keys: &[&str]) -> Option<usize> {
    for key in keys {
        if let Some(value) = args.get(*key) {
            if let Some(number) = value.as_u64() {
                return Some(number as usize);
            }
            if let Some(text) = value.as_str() {
                if let Ok(number) = text.trim().parse::<usize>() {
                    return Some(number);
                }
            }
        }
    }
    None
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    input.chars().take(max_chars).collect()
}

fn short_session(session_id: &str) -> String {
    let trimmed = session_id.trim();
    if trimmed.len() <= 12 {
        trimmed.to_string()
    } else {
        trimmed.chars().take(12).collect()
    }
}

fn digests_root() -> PathBuf {
    crate::session::medousa_data_dir().join("grapheme/digests")
}

fn syntheses_root() -> PathBuf {
    crate::session::medousa_data_dir().join("grapheme/syntheses")
}

fn persist_digest(digest: &StoredDigest) -> Result<(), HostCallError> {
    let dir = digests_root();
    std::fs::create_dir_all(&dir).map_err(|err| HostCallError::Fatal(err.to_string()))?;
    let path = dir.join(format!("{}.json", digest.digest_ref.replace(':', "_")));
    let raw = serde_json::to_vec_pretty(digest).map_err(|err| HostCallError::Fatal(err.to_string()))?;
    std::fs::write(&path, raw).map_err(|err| HostCallError::Fatal(err.to_string()))?;
    append_index_line(&digests_root().join("index.jsonl"), digest.digest_ref.as_str(), &path)
}

fn persist_synthesis(synthesis: &StoredSynthesis) -> Result<(), HostCallError> {
    let dir = syntheses_root();
    std::fs::create_dir_all(&dir).map_err(|err| HostCallError::Fatal(err.to_string()))?;
    let path = dir.join(format!("{}.json", synthesis.synthesis_ref.replace(':', "_")));
    let raw = serde_json::to_vec_pretty(synthesis).map_err(|err| HostCallError::Fatal(err.to_string()))?;
    std::fs::write(&path, raw).map_err(|err| HostCallError::Fatal(err.to_string()))?;
    append_index_line(
        &syntheses_root().join("index.jsonl"),
        synthesis.synthesis_ref.as_str(),
        &path,
    )
}

fn append_index_line(index_path: &Path, ref_id: &str, output_path: &Path) -> Result<(), HostCallError> {
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| HostCallError::Fatal(err.to_string()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(index_path)
        .map_err(|err| HostCallError::Fatal(err.to_string()))?;
    let line = json!({ "ref_id": ref_id, "output_path": output_path.to_string_lossy() });
    writeln!(
        file,
        "{}",
        serde_json::to_string(&line).map_err(|err| HostCallError::Fatal(err.to_string()))?
    )
    .map_err(|err| HostCallError::Fatal(err.to_string()))?;
    Ok(())
}

fn load_digest_text(digest_ref: &str) -> Option<String> {
    load_ref_payload(&digests_root(), digest_ref).and_then(|value| {
        value
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn load_synthesis_outcome(synthesis_ref: &str) -> Option<String> {
    load_ref_payload(&syntheses_root(), synthesis_ref).and_then(|value| {
        value
            .get("outcome")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn load_ref_payload(root: &Path, ref_id: &str) -> Option<Value> {
    let direct = root.join(format!("{}.json", ref_id.replace(':', "_")));
    if let Ok(raw) = std::fs::read_to_string(&direct) {
        return serde_json::from_str(&raw).ok();
    }
    let index_path = root.join("index.jsonl");
    let Ok(file) = std::fs::File::open(index_path) else {
        return None;
    };
    for line in std::io::BufReader::new(file).lines() {
        let Ok(line) = line else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if record.get("ref_id").and_then(Value::as_str) != Some(ref_id) {
            continue;
        }
        let Some(path) = record.get("output_path").and_then(Value::as_str) else {
            continue;
        };
        if let Ok(raw) = std::fs::read_to_string(path) {
            return serde_json::from_str(&raw).ok();
        }
    }
    None
}

fn block_on<F: IntoFuture>(future: F) -> F::Output {
    tokio::task::block_in_place(move || Handle::current().block_on(future.into_future()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medousa_call_detection_uses_module_or_capability_prefix() {
        let call = CapabilityCall {
            module: Some("medousa".to_string()),
            op: "digest".to_string(),
            capability: "medousa.digest".to_string(),
            arg_count: 0,
            args: json!({}),
            step_index: 0,
        };
        assert!(is_medousa_call(&call));
        assert_eq!(resolve_medousa_op(&call), "digest");
    }

    #[test]
    fn digest_inline_input_persists_ref() {
        let result = handle_digest(&json!({
            "session_id": "sess-test",
            "input": "Weekly sales were up 12%."
        }))
        .expect("digest should succeed");
        assert!(result.get("digest_ref").and_then(Value::as_str).is_some());
        assert!(result
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .contains("Weekly sales"));
    }

    #[test]
    fn deliver_quiet_is_success_without_side_effects() {
        let result = handle_deliver(&json!({ "destination": "quiet" })).expect("quiet deliver");
        assert_eq!(
            result.get("destination").and_then(Value::as_str),
            Some("quiet")
        );
    }

    #[test]
    fn digest_accepts_structured_input() {
        let result = handle_digest(&json!({
            "session_id": "sess-test",
            "input": { "rows": [{ "region": "west", "sales": 12 }] }
        }))
        .expect("digest should accept json input");
        assert!(result
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .contains("west"));
    }

    #[test]
    fn synthesize_requires_bridge_init() {
        let err = handle_synthesize(&json!({
            "digest": "context",
            "prompt": "Summarize"
        }))
        .expect_err("should fail without bridge");
        assert!(format!("{err:?}").contains("bridge init"));
    }
}
