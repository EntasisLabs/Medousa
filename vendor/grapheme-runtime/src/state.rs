/// ─────────────────────────────────────────────────────────────
///  Grapheme  —  AgentState
///  The universal state object that threads through every pipeline step.
///  Every |> step receives the previous AgentState and returns a new one.
///  Immutable by convention — each step produces a fresh snapshot.
/// ─────────────────────────────────────────────────────────────
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceProjection {
    Full,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracePolicy {
    pub max_pipeline_steps: usize,
    pub max_string_bytes: usize,
    pub projection: TraceProjection,
}

impl Default for TracePolicy {
    fn default() -> Self {
        Self::lean_default()
    }
}

impl TracePolicy {
    pub fn lean_default() -> Self {
        Self {
            max_pipeline_steps: 128,
            max_string_bytes: 512,
            projection: TraceProjection::Minimal,
        }
    }

    pub fn debug_default() -> Self {
        Self {
            max_pipeline_steps: 2048,
            max_string_bytes: 4096,
            projection: TraceProjection::Full,
        }
    }
}

// ── Step Result ───────────────────────────────────────────────

/// The outcome of a single pipeline step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// The step index in the pipeline (0-based)
    pub index: usize,
    /// Name of the operation that ran: "Database.query"
    pub op: String,
    /// What this step returned
    pub output: JsonValue,
    /// Whether this step succeeded
    pub ok: bool,
    /// Any error message from this step
    pub error: Option<String>,
    /// Function currently executing this step
    pub function_name: Option<String>,
    /// Call depth for this step (0 = entrypoint)
    pub call_depth: usize,
    /// Loop iteration index when running under @loop (0-based)
    pub iteration_index: Option<usize>,
    /// Optional call target for call.* bookkeeping steps
    pub call_target: Option<String>,
    /// Optional executable intent goal attached by compiler metadata.
    #[serde(default)]
    pub intent_goal: Option<String>,
    /// Optional executable intent risk attached by compiler metadata.
    #[serde(default)]
    pub intent_risk: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StepContext {
    pub function_name: Option<String>,
    pub call_depth: usize,
    pub iteration_index: Option<usize>,
    pub call_target: Option<String>,
    pub intent_goal: Option<String>,
    pub intent_risk: Option<String>,
}

// ── Agent Error ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentError {
    pub step: usize,
    pub code: String,
    pub message: String,
}

// ── AgentState ────────────────────────────────────────────────

/// Threads through every |> step in a pipeline.
/// Queries only produce new state; mutations may also have side effects.
///
/// The AI can inspect this at any point via:
///   state { current diff errors pipeline proposed }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// The current output value — result of the last completed step
    pub current: JsonValue,

    /// Diff between the previous step's output and this step's output
    /// (null if this is the first step or nothing changed)
    pub diff: Option<JsonValue>,

    /// All errors encountered so far in this pipeline run
    pub errors: Vec<AgentError>,

    /// The full history of step results in this pipeline
    pub pipeline: Vec<StepResult>,

    /// Runtime lifecycle events (for example module activation/rollback) captured
    /// during or before this execution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_events: Vec<JsonValue>,

    /// Modules the AI has proposed but which haven't been approved yet
    pub proposed: Vec<ProposedModule>,

    #[serde(skip, default)]
    trace_policy: TracePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedModule {
    pub proposal: JsonValue,
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalStatus {
    Pending,
    Approved,
    Rejected(String),
}

impl AgentState {
    /// Create a fresh empty state at the start of a pipeline
    pub fn new() -> Self {
        Self::with_trace_policy(TracePolicy::default())
    }

    pub fn with_trace_policy(trace_policy: TracePolicy) -> Self {
        AgentState {
            current: JsonValue::Null,
            diff: None,
            errors: vec![],
            pipeline: vec![],
            runtime_events: vec![],
            proposed: vec![],
            trace_policy,
        }
    }

    /// Advance state after a successful step
    pub fn advance(&self, index: usize, op: String, output: JsonValue) -> Self {
        let mut next = self.clone();
        next.advance_in_place(index, op, output);
        next
    }

    /// Advance state after a failed step (errors accumulate, current unchanged)
    pub fn fail(&self, index: usize, op: String, code: String, message: String) -> Self {
        let mut next = self.clone();
        next.fail_in_place(index, op, code, message);
        next
    }

    /// Advance state after a successful step without cloning full history.
    pub fn advance_in_place(&mut self, index: usize, op: String, output: JsonValue) {
        self.advance_in_place_with_context(index, op, output, StepContext::default());
    }

    /// Advance state after a successful step with control-flow context.
    pub fn advance_in_place_with_context(
        &mut self,
        index: usize,
        op: String,
        output: JsonValue,
        context: StepContext,
    ) {
        self.diff = compute_diff(&self.current, &output);
        self.push_step_result(StepResult {
            index,
            op,
            output: project_output_for_trace(&output, &self.trace_policy),
            ok: true,
            error: None,
            function_name: context.function_name,
            call_depth: context.call_depth,
            iteration_index: context.iteration_index,
            call_target: context.call_target,
            intent_goal: context.intent_goal,
            intent_risk: context.intent_risk,
        });
        self.current = output;
    }

    /// Advance state after a failed step without cloning full history.
    pub fn fail_in_place(&mut self, index: usize, op: String, code: String, message: String) {
        self.fail_in_place_with_context(index, op, code, message, StepContext::default());
    }

    /// Advance state after a failed step with control-flow context.
    pub fn fail_in_place_with_context(
        &mut self,
        index: usize,
        op: String,
        code: String,
        message: String,
        context: StepContext,
    ) {
        self.errors.push(AgentError {
            step: index,
            code,
            message: message.clone(),
        });
        self.push_step_result(StepResult {
            index,
            op,
            output: JsonValue::Null,
            ok: false,
            error: Some(message),
            function_name: context.function_name,
            call_depth: context.call_depth,
            iteration_index: context.iteration_index,
            call_target: context.call_target,
            intent_goal: context.intent_goal,
            intent_risk: context.intent_risk,
        });
        self.diff = None;
    }

    /// Record a successful pipeline step that forwards existing current output.
    ///
    /// This is used for call-wrapper bookkeeping to avoid re-applying a current assignment.
    pub fn record_passthrough_in_place(&mut self, index: usize, op: String, context: StepContext) {
        self.diff = None;
        self.push_step_result(StepResult {
            index,
            op,
            output: project_output_for_trace(&self.current, &self.trace_policy),
            ok: true,
            error: None,
            function_name: context.function_name,
            call_depth: context.call_depth,
            iteration_index: context.iteration_index,
            call_target: context.call_target,
            intent_goal: context.intent_goal,
            intent_risk: context.intent_risk,
        });
    }

    /// Apply a loop merge result to current state without recording an additional step.
    pub fn apply_loop_merge_current(&mut self, merged: JsonValue) {
        self.diff = compute_diff(&self.current, &merged);
        self.current = merged;
    }

    fn push_step_result(&mut self, step: StepResult) {
        self.pipeline.push(step);
        if self.trace_policy.max_pipeline_steps == 0 {
            self.pipeline.clear();
            return;
        }

        let len = self.pipeline.len();
        if len > self.trace_policy.max_pipeline_steps {
            let to_drop = len - self.trace_policy.max_pipeline_steps;
            self.pipeline.drain(0..to_drop);
        }
    }

    /// Register a module proposal from the AI
    pub fn propose_json(&self, proposal: JsonValue) -> Self {
        let mut proposed = self.proposed.clone();
        proposed.push(ProposedModule {
            proposal,
            status: ProposalStatus::Pending,
        });
        AgentState {
            proposed,
            ..self.clone()
        }
    }

    /// Returns true if there are any errors in the current run
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Serialize the state to JSON for returning to the AI agent
    pub fn to_json(&self) -> JsonValue {
        serde_json::to_value(self).unwrap_or(JsonValue::Null)
    }

    pub fn set_runtime_events(&mut self, events: Vec<JsonValue>) {
        self.runtime_events = events;
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Diff Helper ───────────────────────────────────────────────

/// Compute a simple structural diff between two JSON values.
/// Returns None if they are identical.
fn compute_diff(prev: &JsonValue, next: &JsonValue) -> Option<JsonValue> {
    if prev == next {
        return None;
    }

    match (prev, next) {
        (JsonValue::Object(p), JsonValue::Object(n)) => {
            let mut diff = serde_json::Map::new();

            // Keys added or changed
            for (k, v) in n {
                match p.get(k) {
                    None => {
                        diff.insert(format!("+{k}"), v.clone());
                    }
                    Some(pv) if pv != v => {
                        diff.insert(format!("~{k}"), v.clone());
                    }
                    _ => {}
                }
            }
            // Keys removed
            for k in p.keys() {
                if !n.contains_key(k) {
                    diff.insert(format!("-{k}"), JsonValue::Null);
                }
            }

            if diff.is_empty() {
                None
            } else {
                Some(JsonValue::Object(diff))
            }
        }
        // For non-objects just show before/after
        _ => Some(serde_json::json!({ "from": prev, "to": next })),
    }
}

fn project_output_for_trace(value: &JsonValue, policy: &TracePolicy) -> JsonValue {
    match policy.projection {
        TraceProjection::Full => value.clone(),
        TraceProjection::Minimal => minimal_trace_projection(value, policy.max_string_bytes),
    }
}

fn minimal_trace_projection(value: &JsonValue, max_string_bytes: usize) -> JsonValue {
    match value {
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => value.clone(),
        JsonValue::String(s) => JsonValue::String(truncate_utf8(s, max_string_bytes)),
        JsonValue::Array(items) => serde_json::json!({
            "_kind": "array",
            "len": items.len()
        }),
        JsonValue::Object(map) => {
            let mut out = serde_json::Map::new();
            for key in ["message", "text", "stdout", "error", "status", "done"] {
                if let Some(v) = map.get(key) {
                    out.insert(
                        key.to_string(),
                        minimal_trace_projection(v, max_string_bytes),
                    );
                }
            }
            out.insert("_kind".to_string(), JsonValue::String("object".to_string()));
            out.insert("_keys".to_string(), JsonValue::from(map.len()));
            JsonValue::Object(out)
        }
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = 0usize;
    for (idx, _) in value.char_indices() {
        if idx > max_bytes {
            break;
        }
        end = idx;
    }

    let head = &value[..end];
    format!("{head}...")
}
