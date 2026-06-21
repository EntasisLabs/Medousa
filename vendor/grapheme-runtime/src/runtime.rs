use grapheme_artifact::mir::{MirCompareOp, MirMatchTarget};
use grapheme_artifact::{
    validate_aot_host_interface_boundary, AotEnvelope, AotStage, ArtifactEnvelope, Capability,
    CapabilityPolicy, ExecutionOutcome, ExecutionResult, MirFunction, MirInst, MirLoopMergeMode,
    TraceSummary,
};
use grapheme_signatures::module_ops;
use serde_json::{json, Map, Value as JsonValue};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::error::RuntimeError as GraphemeError;
use crate::host::{CapabilityCall, CapabilityHost, HostCallError};
use crate::module_manager::{
    ActivationResult, LoadModuleRequest, ModuleLifecycleEvent, ModuleLifecycleEventKind,
    ModuleLoadError, ModuleManager,
};
use crate::module_manifest::ModuleAbi;
use crate::module_registry::ModuleRegistry;
#[cfg(feature = "wasix-runtime")]
use crate::module_registry::ResolvedModuleCall;
use crate::policy::PolicyGuard;
use crate::state::{AgentState, StepContext, TracePolicy};
#[cfg(feature = "wasix-runtime")]
use crate::wasix_backend::WasixBackend;

const DEFAULT_MAX_CALL_DEPTH: usize = 16;

/// Runtime configuration for policy, module resolution, and execution limits.
#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    /// Capability admission policy used by verifier/runtime contract checks.
    pub capability_policy: CapabilityPolicy,
    /// Environment and host-allowlist policy guard.
    pub policy_guard: PolicyGuard,
    /// Module manifest/binding registry used for capability dispatch.
    pub module_registry: ModuleRegistry,
    /// Runtime module generation manager for activation/rollback.
    pub module_manager: ModuleManager,
    /// When true, validates artifact integrity hash before execution.
    pub verify_integrity: bool,
    /// Trace shaping policy for `AgentState` pipeline history.
    pub trace_policy: TracePolicy,
    /// Stream plain-mode step output as steps execute.
    pub stream_step_output: bool,
    /// Enforce Stage B direct container execution without parity fallback.
    pub strict_stage_b_container_execution: bool,
    /// Optional max executed steps across full run.
    pub max_steps: Option<usize>,
    /// Optional max call depth for nested function execution.
    pub max_call_depth: Option<usize>,
    /// Optional initial value assigned to `state.current` before first step.
    pub initial_state_current: Option<JsonValue>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            capability_policy: CapabilityPolicy::default(),
            policy_guard: PolicyGuard::default(),
            module_registry: ModuleRegistry::default(),
            module_manager: ModuleManager::new(),
            verify_integrity: true,
            trace_policy: TracePolicy::default(),
            stream_step_output: false,
            strict_stage_b_container_execution: default_strict_stage_b_container_execution(),
            max_steps: Some(100_000),
            max_call_depth: Some(DEFAULT_MAX_CALL_DEPTH),
            initial_state_current: None,
        }
    }
}

fn default_strict_stage_b_container_execution() -> bool {
    if let Some(explicit) = parse_bool_env("GRAPHEME_STRICT_STAGE_B") {
        return explicit;
    }

    // Production policy: release builds default to strict Stage B container-first execution.
    !cfg!(debug_assertions)
}

fn parse_bool_env(var: &str) -> Option<bool> {
    let value = std::env::var(var).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg_attr(not(feature = "wasix-runtime"), allow(dead_code))]
enum StageBContainerExecution {
    Executed(JsonValue),
    Unavailable(String),
}

struct LoopFrame<'a> {
    function: &'a MirFunction,
    max_iterations: usize,
    merge_mode: MirLoopMergeMode,
    input_snapshot: JsonValue,
    each_inputs: Option<Vec<JsonValue>>,
    iteration_outputs: Vec<JsonValue>,
}

struct TemplateScope<'a> {
    current: &'a JsonValue,
    state: &'a JsonValue,
    item: Option<&'a JsonValue>,
    loop_meta: Option<JsonValue>,
}

impl<'a> LoopFrame<'a> {
    fn new(function: &'a MirFunction, state: &AgentState) -> Self {
        let input_snapshot = state.current.clone();
        let each_inputs = function
            .loop_config
            .as_ref()
            .and_then(|cfg| cfg.each.as_deref())
            .map(|selector| resolve_each_inputs(selector, &input_snapshot));
        let configured_max = function
            .loop_config
            .as_ref()
            .and_then(|cfg| cfg.max)
            .map(|max| max as usize)
            .unwrap_or_else(|| {
                if function.loop_config.is_some() {
                    usize::MAX
                } else {
                    1
                }
            });
        let max_iterations = each_inputs
            .as_ref()
            .map(|inputs| configured_max.min(inputs.len()))
            .unwrap_or(configured_max);

        Self {
            function,
            max_iterations,
            merge_mode: function
                .loop_config
                .as_ref()
                .map(|cfg| cfg.merge.clone())
                .unwrap_or(MirLoopMergeMode::Replace),
            input_snapshot,
            each_inputs,
            iteration_outputs: Vec::new(),
        }
    }

    fn apply_iteration_input(&self, state: &mut AgentState, iteration: usize) {
        let Some(inputs) = &self.each_inputs else {
            return;
        };

        if let Some(input) = inputs.get(iteration) {
            state.current = input.clone();
            state.diff = None;
        }
    }

    fn template_scope<'b>(&'b self, state: &'b AgentState, iteration: usize) -> TemplateScope<'b> {
        let in_each = self.each_inputs.is_some();
        let state_scope = if in_each {
            &self.input_snapshot
        } else {
            &state.current
        };
        let item_scope = self.each_inputs.as_ref().and_then(|items| items.get(iteration));

        let loop_meta = if self.function.loop_config.is_some() {
            Some(json!({
                "index": iteration,
                "count": iteration + 1,
                "max": self.max_iterations,
                "is_first": iteration == 0,
                "is_last": iteration + 1 >= self.max_iterations,
            }))
        } else {
            None
        };

        TemplateScope {
            current: &state.current,
            state: state_scope,
            item: item_scope,
            loop_meta,
        }
    }

    fn iteration_index(&self, iteration: usize) -> Option<usize> {
        self.function.loop_config.as_ref().map(|_| iteration)
    }

    fn until_satisfied(&self, state: &AgentState) -> bool {
        let Some(loop_cfg) = self.function.loop_config.as_ref() else {
            return false;
        };

        let Some(until) = loop_cfg.until.as_ref() else {
            return false;
        };

        state
            .current
            .as_object()
            .and_then(|obj| obj.get(&until.field))
            .map(|value| value == &until.eq)
            .unwrap_or(false)
    }

    fn apply_merge(&self, state: &mut AgentState) {
        match self.merge_mode {
            // Replace is the current behavior: final loop iteration value is already in state.current.
            MirLoopMergeMode::Replace => {}
            MirLoopMergeMode::Append => {
                state.apply_loop_merge_current(JsonValue::Array(self.iteration_outputs.clone()));
            }
            MirLoopMergeMode::Reduce => {
                let reduced = reduce_iteration_outputs(&self.iteration_outputs);
                state.apply_loop_merge_current(reduced);
            }
            MirLoopMergeMode::None => {
                state.apply_loop_merge_current(self.input_snapshot.clone());
            }
        }
    }

    fn record_iteration(&mut self, state: &AgentState) {
        match self.merge_mode {
            MirLoopMergeMode::Append | MirLoopMergeMode::Reduce => {
                self.iteration_outputs.push(state.current.clone());
            }
            MirLoopMergeMode::Replace | MirLoopMergeMode::None => {}
        }
    }
}

pub struct RuntimeEngine {
    options: RuntimeOptions,
    #[cfg(feature = "wasix-runtime")]
    wasix_backend: WasixBackend,
}

impl RuntimeEngine {
    /// Construct a runtime engine from explicit options.
    pub fn new(options: RuntimeOptions) -> Self {
        Self {
            options,
            #[cfg(feature = "wasix-runtime")]
            wasix_backend: WasixBackend::new(),
        }
    }

    /// Activate a new module generation and switch registry bindings to it.
    pub fn activate_module_generation(
        &mut self,
        request: LoadModuleRequest,
    ) -> Result<ActivationResult, ModuleLoadError> {
        if !self.options.module_registry.has_module(&request.module_id) {
            return Err(ModuleLoadError::UnknownModule {
                module_id: request.module_id,
            });
        }

        self.validate_activation_contract(&request.module_id)?;

        let module_id = request.module_id.clone();
        let activation = self.options.module_manager.load_and_activate(request)?;
        let active = self
            .options
            .module_manager
            .active_generation_record(&module_id)
            .ok_or_else(|| ModuleLoadError::NoActiveGeneration {
                module_id: module_id.clone(),
            })?;

        self.options.module_registry.set_wasm_generation(
            &module_id,
            active.wasm_path,
            active.generation_id,
            active.content_hash,
        );

        Ok(activation)
    }

    fn validate_activation_contract(&self, module_id: &str) -> Result<(), ModuleLoadError> {
        let manifest = self
            .options
            .module_registry
            .manifest_for(module_id)
            .ok_or_else(|| ModuleLoadError::UnknownModule {
                module_id: module_id.to_string(),
            })?;

        validate_required_signature_ops(module_id, &manifest.exported_ops).map_err(
            |missing_ops| ModuleLoadError::MissingRequiredOps {
                module_id: module_id.to_string(),
                missing_ops,
            },
        )?;

        validate_required_capabilities_admitted(
            module_id,
            &manifest.required_capabilities,
            &self.options.capability_policy,
        )
        .map_err(
            |denied_capabilities| ModuleLoadError::PolicyDeniedCapabilities {
                module_id: module_id.to_string(),
                denied_capabilities,
            },
        )?;

        Ok(())
    }

    /// Return a hotload snapshot of the current module manager state.
    pub fn hotload_snapshot(&self) -> crate::module_hotload::HotloadStore {
        self.options.module_manager.export_hotload()
    }

    /// Roll back the active module generation for a module.
    pub fn rollback_module_generation(
        &mut self,
        module_id: &str,
    ) -> Result<ActivationResult, ModuleLoadError> {
        let rollback = self.options.module_manager.rollback(module_id)?;
        let active = self
            .options
            .module_manager
            .active_generation_record(module_id)
            .ok_or_else(|| ModuleLoadError::NoActiveGeneration {
                module_id: module_id.to_string(),
            })?;

        self.options.module_registry.set_wasm_generation(
            module_id,
            active.wasm_path,
            active.generation_id,
            active.content_hash,
        );

        Ok(rollback)
    }

    /// Return collected module lifecycle events captured by the runtime.
    pub fn module_lifecycle_events(&self) -> &[ModuleLifecycleEvent] {
        self.options.module_manager.lifecycle_events()
    }

    /// Execute a verified artifact envelope using the provided capability host.
    pub fn execute_artifact(
        &self,
        artifact: &ArtifactEnvelope,
        host: &mut dyn CapabilityHost,
    ) -> Result<(AgentState, ExecutionResult), GraphemeError> {
        verify_artifact_compatibility(artifact)?;
        if self.options.verify_integrity {
            verify_artifact_integrity(artifact)?;
        }

        let functions = &artifact.payload.mir.functions;
        let function_index = build_function_index(functions);
        let entrypoint_index = function_index
            .get(artifact.entrypoint.as_str())
            .copied()
            .ok_or_else(|| {
                GraphemeError::ArtifactCompatibilityError(format!(
                    "entrypoint '{}' not found in artifact MIR",
                    artifact.entrypoint
                ))
            })?;

        let mut state = AgentState::with_trace_policy(self.options.trace_policy.clone());
        if let Some(initial_current) = &self.options.initial_state_current {
            state.current = initial_current.clone();
        }
        // Pin module resolution state for the full execution so future activations
        // do not affect in-flight workflows.
        let pinned_module_registry = self.options.module_registry.clone();
        let mut step_index = 0usize;
        let mut remaining_steps = self.options.max_steps;
        let max_call_depth = self.options.max_call_depth.unwrap_or(usize::MAX);

        if let Some(result) = self.execute_function(
            functions,
            &function_index,
            &pinned_module_registry,
            entrypoint_index,
            host,
            &mut state,
            &mut step_index,
            &mut remaining_steps,
            0,
            max_call_depth,
        )? {
            state.set_runtime_events(module_lifecycle_events_to_json(
                self.options.module_manager.lifecycle_events(),
            ));
            return Ok((state, result));
        }

        state.set_runtime_events(module_lifecycle_events_to_json(
            self.options.module_manager.lifecycle_events(),
        ));

        Ok((
            state,
            ExecutionResult {
                outcome: ExecutionOutcome::Succeeded,
                output_sttp_node_id: None,
                trace_summary: TraceSummary {
                    steps: step_index,
                    failed_step: None,
                },
                message: None,
            },
        ))
    }

    /// Execute an AOT envelope (Stage A or Stage B) using the provided host.
    pub fn execute_aot(
        &self,
        aot: &AotEnvelope,
        host: &mut dyn CapabilityHost,
    ) -> Result<(AgentState, ExecutionResult), GraphemeError> {
        validate_aot_host_interface_boundary(aot)
            .map_err(|e| GraphemeError::ArtifactCompatibilityError(e.to_string()))?;

        match aot.stage {
            AotStage::StageA => self.execute_artifact(&aot.base_artifact, host),
            AotStage::StageB => self.execute_stage_b_scaffold(aot, host),
        }
    }

    fn execute_stage_b_scaffold(
        &self,
        aot: &AotEnvelope,
        host: &mut dyn CapabilityHost,
    ) -> Result<(AgentState, ExecutionResult), GraphemeError> {
        if let Some(container) = aot.payload.workflow_wasm.as_ref() {
            match self.try_execute_stage_b_container(container)? {
                StageBContainerExecution::Executed(container_output) => {
                    let mut state =
                        AgentState::with_trace_policy(self.options.trace_policy.clone());
                    state.advance_in_place(
                        0,
                        format!("aot.stage_b::{}", container.entry_export),
                        container_output,
                    );
                    let mut runtime_events = module_lifecycle_events_to_json(
                        self.options.module_manager.lifecycle_events(),
                    );
                    runtime_events.push(stage_b_container_event(container));
                    state.set_runtime_events(runtime_events);

                    return Ok((
                        state,
                        ExecutionResult {
                            outcome: ExecutionOutcome::Succeeded,
                            output_sttp_node_id: None,
                            trace_summary: TraceSummary {
                                steps: 1,
                                failed_step: None,
                            },
                            message: Some(
                                "stage_b container executed directly via wasix backend".to_string(),
                            ),
                        },
                    ));
                }
                StageBContainerExecution::Unavailable(reason) => {
                    if self.options.strict_stage_b_container_execution {
                        return Err(GraphemeError::ArtifactCompatibilityError(format!(
                            "strict stage_b container execution required: {reason}"
                        )));
                    }
                }
            }
        }

        let (mut state, mut result) = self.execute_artifact(&aot.base_artifact, host)?;
        if let Some(container) = aot.payload.workflow_wasm.as_ref() {
            state
                .runtime_events
                .push(stage_b_container_event(container));
        }
        if result.message.is_none() {
            result.message = Some(
                "stage_b scaffold executed via parity path until wasm container runtime lowering is enabled"
                    .to_string(),
            );
        }
        Ok((state, result))
    }
}

fn stage_b_container_event(container: &grapheme_artifact::AotWorkflowWasmContainer) -> JsonValue {
    serde_json::json!({
        "kind": "aot.stage_b.container_routed",
        "entry_export": container.entry_export,
        "byte_len": container.byte_len,
        "sha256": container.sha256,
        "allowed_imports": container.allowed_imports,
    })
}

#[cfg(feature = "wasix-runtime")]
impl RuntimeEngine {
    fn try_execute_stage_b_container(
        &self,
        container: &grapheme_artifact::AotWorkflowWasmContainer,
    ) -> Result<StageBContainerExecution, GraphemeError> {
        let Some(inline_wasm_hex) = container.inline_wasm_hex.as_ref() else {
            return Ok(StageBContainerExecution::Unavailable(
                "stage_b container metadata has no inline_wasm_hex bytes".to_string(),
            ));
        };

        let wasm_bytes = hex::decode(inline_wasm_hex).map_err(|e| {
            GraphemeError::ArtifactCompatibilityError(format!(
                "stage_b inline_wasm_hex is not valid hex: {e}"
            ))
        })?;

        let mut wasm_path = std::env::temp_dir();
        let now_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| GraphemeError::RuntimeError(format!("system clock error: {e}")))?
            .as_nanos();
        wasm_path.push(format!("grapheme-aot-stage-b-{now_nanos}.wasm"));

        std::fs::write(&wasm_path, &wasm_bytes).map_err(|e| {
            GraphemeError::RuntimeError(format!(
                "write stage_b wasm container '{}': {e}",
                wasm_path.display()
            ))
        })?;

        let resolved = ResolvedModuleCall {
            module_id: "workflow".to_string(),
            op: container.entry_export.clone(),
            abi: ModuleAbi::WasixV1,
            wasm_path: Some(wasm_path.clone()),
            generation_id: None,
            content_hash: Some(container.sha256.clone()),
        };

        let args = serde_json::json!({
            "entry_export": container.entry_export,
            "allowed_imports": container.allowed_imports,
        });

        let output = self
            .wasix_backend
            .execute_call(&wasm_path, &resolved, &args);

        let _ = std::fs::remove_file(&wasm_path);

        match output {
            Ok(value) => Ok(StageBContainerExecution::Executed(value)),
            Err(err) => Ok(StageBContainerExecution::Unavailable(format!(
                "wasix backend could not execute stage_b container: {err}"
            ))),
        }
    }
}

#[cfg(not(feature = "wasix-runtime"))]
impl RuntimeEngine {
    fn try_execute_stage_b_container(
        &self,
        _container: &grapheme_artifact::AotWorkflowWasmContainer,
    ) -> Result<StageBContainerExecution, GraphemeError> {
        Ok(StageBContainerExecution::Unavailable(
            "wasix-runtime feature is not enabled".to_string(),
        ))
    }
}

fn module_lifecycle_events_to_json(events: &[ModuleLifecycleEvent]) -> Vec<JsonValue> {
    events
        .iter()
        .map(|event| {
            serde_json::json!({
                "kind": module_lifecycle_event_name(event.kind),
                "module_id": event.module_id,
                "generation_id": event.generation_id,
                "version": event.version,
                "content_hash": event.content_hash,
                "reason": event.reason,
            })
        })
        .collect()
}

fn module_lifecycle_event_name(kind: ModuleLifecycleEventKind) -> &'static str {
    match kind {
        ModuleLifecycleEventKind::Loaded => "module.loaded",
        ModuleLifecycleEventKind::Validated => "module.validated",
        ModuleLifecycleEventKind::Activated => "module.activated",
        ModuleLifecycleEventKind::ActivationFailed => "module.activation_failed",
        ModuleLifecycleEventKind::Draining => "module.draining",
        ModuleLifecycleEventKind::Retired => "module.retired",
        ModuleLifecycleEventKind::Rollback => "module.rollback",
    }
}

fn validate_required_signature_ops(
    module_id: &str,
    exported_ops: &[crate::module_manifest::ExportedOp],
) -> Result<(), Vec<String>> {
    let required_ops = module_ops(module_id);
    if required_ops.is_empty() {
        return Ok(());
    }

    let exported = exported_ops
        .iter()
        .map(|op| op.op.as_str())
        .collect::<HashSet<_>>();

    let missing = required_ops
        .into_iter()
        .map(|spec| spec.op.to_string())
        .filter(|op| !exported.contains(op.as_str()))
        .collect::<Vec<_>>();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

fn validate_required_capabilities_admitted(
    _module_id: &str,
    required_capabilities: &[String],
    policy: &CapabilityPolicy,
) -> Result<(), Vec<String>> {
    let denied = required_capabilities
        .iter()
        .filter(|cap| !policy.is_allowed(&Capability((*cap).clone())))
        .cloned()
        .collect::<Vec<_>>();

    if denied.is_empty() {
        Ok(())
    } else {
        Err(denied)
    }
}

impl Default for RuntimeEngine {
    fn default() -> Self {
        Self::new(RuntimeOptions::default())
    }
}

impl RuntimeEngine {
    fn execute_function(
        &self,
        functions: &[MirFunction],
        function_index: &HashMap<String, usize>,
        module_registry: &ModuleRegistry,
        function_idx: usize,
        host: &mut dyn CapabilityHost,
        state: &mut AgentState,
        step_index: &mut usize,
        remaining_steps: &mut Option<usize>,
        call_depth: usize,
        max_call_depth: usize,
    ) -> Result<Option<ExecutionResult>, GraphemeError> {
        let function = &functions[function_idx];
        let intent_goal = function
            .intent_config
            .as_ref()
            .and_then(|cfg| cfg.goal.clone());
        let intent_risk = function
            .intent_config
            .as_ref()
            .and_then(|cfg| cfg.risk.clone());
        let retry_max_attempts = function
            .retry_config
            .as_ref()
            .map(|cfg| cfg.max.max(1) as usize)
            .unwrap_or(1);

        for attempt in 0..retry_max_attempts {
            let state_snapshot = state.clone();
            let step_snapshot = *step_index;
            let remaining_snapshot = *remaining_steps;

            let result = self.execute_function_once(
                functions,
                function_index,
                module_registry,
                function_idx,
                host,
                state,
                step_index,
                remaining_steps,
                call_depth,
                max_call_depth,
            )?;

            if let Some(result_value) = result {
                if matches!(result_value.outcome, ExecutionOutcome::RetryableFailure)
                    && attempt + 1 < retry_max_attempts
                {
                    *state = state_snapshot;
                    *step_index = step_snapshot;
                    *remaining_steps = remaining_snapshot;
                    continue;
                }

                if matches!(result_value.outcome, ExecutionOutcome::RetryableFailure) {
                    if let Some(retry_cfg) = function.retry_config.as_ref() {
                        let base_context = StepContext {
                            function_name: Some(function.name.clone()),
                            call_depth,
                            iteration_index: None,
                            call_target: None,
                            intent_goal: intent_goal.clone(),
                            intent_risk: intent_risk.clone(),
                        };
                        return self.invoke_target(
                            functions,
                            function_index,
                            module_registry,
                            host,
                            state,
                            step_index,
                            remaining_steps,
                            call_depth,
                            max_call_depth,
                            &retry_cfg.on_fail,
                            "runtime.retry",
                            base_context,
                        );
                    }
                }

                return Ok(Some(result_value));
            }

            return Ok(None);
        }

        Ok(None)
    }

    fn execute_function_once(
        &self,
        functions: &[MirFunction],
        function_index: &HashMap<String, usize>,
        module_registry: &ModuleRegistry,
        function_idx: usize,
        host: &mut dyn CapabilityHost,
        state: &mut AgentState,
        step_index: &mut usize,
        remaining_steps: &mut Option<usize>,
        call_depth: usize,
        max_call_depth: usize,
    ) -> Result<Option<ExecutionResult>, GraphemeError> {
        let function = &functions[function_idx];
        let function_name = function.name.clone();
        let intent_goal = function
            .intent_config
            .as_ref()
            .and_then(|cfg| cfg.goal.clone());
        let intent_risk = function
            .intent_config
            .as_ref()
            .and_then(|cfg| cfg.risk.clone());
        let mut loop_frame = LoopFrame::new(function, state);
        let timeout_started = Instant::now();

        for iteration in 0..loop_frame.max_iterations {
            loop_frame.apply_iteration_input(state, iteration);
            let iteration_index = loop_frame.iteration_index(iteration);
            for block in &function.blocks {
                for inst in &block.instructions {
                    let base_context = StepContext {
                        function_name: Some(function_name.clone()),
                        call_depth,
                        iteration_index,
                        call_target: None,
                        intent_goal: intent_goal.clone(),
                        intent_risk: intent_risk.clone(),
                    };

                    if let Some(timeout_cfg) = function.timeout_config.as_ref() {
                        if timeout_started.elapsed().as_millis() >= timeout_cfg.ms as u128 {
                            loop_frame.apply_merge(state);
                            return self.invoke_target(
                                functions,
                                function_index,
                                module_registry,
                                host,
                                state,
                                step_index,
                                remaining_steps,
                                call_depth,
                                max_call_depth,
                                &timeout_cfg.on_timeout,
                                "runtime.timeout",
                                base_context,
                            );
                        }
                    }

                    if !consume_step_budget(remaining_steps) {
                        return Ok(Some(fail_execution(
                            state,
                            *step_index,
                            &Capability::from_module_op("runtime", "step_budget"),
                            "STEP_BUDGET_EXCEEDED",
                            "runtime step budget exhausted".to_string(),
                            ExecutionOutcome::FatalFailure,
                            base_context,
                        )));
                    }

                    match inst {
                        MirInst::Call {
                            module,
                            op,
                            capability,
                            arg_count,
                            args,
                            ..
                        } => {
                            let base_context = StepContext {
                                function_name: Some(function_name.clone()),
                                call_depth,
                                iteration_index,
                                call_target: None,
                                intent_goal: intent_goal.clone(),
                                intent_risk: intent_risk.clone(),
                            };

                            if !self.options.capability_policy.is_allowed(capability) {
                                let message = format!(
                                    "capability '{}' denied by runtime policy",
                                    capability.0
                                );
                                return Ok(Some(fail_execution(
                                    state,
                                    *step_index,
                                    capability,
                                    "CAPABILITY_DENIED",
                                    message,
                                    ExecutionOutcome::FatalFailure,
                                    base_context,
                                )));
                            }

                            if is_call_step(module) {
                                let call_max_depth = resolve_call_max_depth(args, max_call_depth)?;
                                if let Some(result) = self.invoke_target(
                                    functions,
                                    function_index,
                                    module_registry,
                                    host,
                                    state,
                                    step_index,
                                    remaining_steps,
                                    call_depth,
                                    call_max_depth,
                                    op,
                                    &capability.0,
                                    base_context,
                                )? {
                                    return Ok(Some(result));
                                }
                                continue;
                            }

                            let resolved = module_registry
                                .resolve_call(module.as_deref(), op, &capability.0)
                                .ok_or_else(|| {
                                    GraphemeError::RuntimeError(format!(
                                        "module/op not registered for capability '{}': module={:?}, op={}",
                                        capability.0, module, op
                                    ))
                                })?;

                            let scope = loop_frame.template_scope(state, iteration);
                            let call_args = args_with_pipeline_input(args, &state.current, &scope);

                            if let Err(err) = self.options.policy_guard.check(&resolved, &call_args)
                            {
                                return Ok(Some(fail_execution(
                                    state,
                                    *step_index,
                                    capability,
                                    "POLICY_DENIED",
                                    err.to_string(),
                                    ExecutionOutcome::FatalFailure,
                                    base_context.clone(),
                                )));
                            }

                            if let Some(input) = call_args.get("__input") {
                                if let Some(error) = input.get("error") {
                                    return Ok(Some(fail_execution(
                                        state,
                                        *step_index,
                                        capability,
                                        "EXECUTION_ERROR",
                                        error.to_string(),
                                        ExecutionOutcome::FatalFailure,
                                        base_context.clone(),
                                    )));
                                }
                            }

                            let output = match resolved.abi {
                                ModuleAbi::MirV1 => {
                                    let call = CapabilityCall {
                                        module: module.clone(),
                                        op: op.clone(),
                                        capability: capability.0.clone(),
                                        arg_count: *arg_count,
                                        args: call_args.clone(),
                                        step_index: *step_index,
                                    };

                                    match host.call(&call) {
                                        Ok(output) => output,
                                        Err(HostCallError::Retryable(message)) => {
                                            return Ok(Some(fail_execution(
                                                state,
                                                *step_index,
                                                capability,
                                                "RETRYABLE",
                                                message,
                                                ExecutionOutcome::RetryableFailure,
                                                base_context.clone(),
                                            )));
                                        }
                                        Err(HostCallError::Fatal(message)) => {
                                            return Ok(Some(fail_execution(
                                                state,
                                                *step_index,
                                                capability,
                                                "FATAL",
                                                message,
                                                ExecutionOutcome::FatalFailure,
                                                base_context.clone(),
                                            )));
                                        }
                                    }
                                }
                                ModuleAbi::WasixV1 | ModuleAbi::WasixWitV15 => {
                                    #[cfg(feature = "wasix-runtime")]
                                    {
                                        let path =
                                            resolved.wasm_path.as_deref().ok_or_else(|| {
                                                GraphemeError::RuntimeError(format!(
                                                    "module '{}' requires wasm binding for op '{}'",
                                                    resolved.module_id, resolved.op
                                                ))
                                            })?;
                                        self.wasix_backend
                                            .execute_call(path, &resolved, &call_args)?
                                    }

                                    #[cfg(not(feature = "wasix-runtime"))]
                                    {
                                        return Err(GraphemeError::RuntimeError(
                                            "runtime built without wasix-runtime feature"
                                                .to_string(),
                                        ));
                                    }
                                }
                            };

                            if self.options.stream_step_output {
                                emit_streamed_step_output(op, &base_context, &output);
                            }

                            state.advance_in_place_with_context(
                                *step_index,
                                capability.0.clone(),
                                output,
                                base_context,
                            );
                            *step_index += 1;
                        }
                        MirInst::BranchCall {
                            field,
                            cmp,
                            value,
                            then_target,
                            else_target,
                            max_depth,
                        } => {
                            let base_context = StepContext {
                                function_name: Some(function_name.clone()),
                                call_depth,
                                iteration_index,
                                call_target: None,
                                intent_goal: intent_goal.clone(),
                                intent_risk: intent_risk.clone(),
                            };

                            let scope = loop_frame.template_scope(state, iteration);
                            let compare_to = resolve_current_templates(value, &scope);
                            let branch_matches = select_scoped_json_path(&scope, &state.current, field)
                                .map(|current_value| {
                                    branch_compare(current_value, cmp, &compare_to)
                                })
                                .unwrap_or(false);

                            let target = if branch_matches {
                                Some(then_target.as_str())
                            } else {
                                else_target.as_deref()
                            };

                            if let Some(target) = target {
                                if target == "$return" {
                                    loop_frame.apply_merge(state);
                                    return Ok(None);
                                }

                                let call_max_depth =
                                    max_depth.map(|v| v as usize).unwrap_or(max_call_depth);

                                if let Some(result) = self.invoke_target(
                                    functions,
                                    function_index,
                                    module_registry,
                                    host,
                                    state,
                                    step_index,
                                    remaining_steps,
                                    call_depth,
                                    call_max_depth,
                                    target,
                                    "flow.branch",
                                    base_context,
                                )? {
                                    return Ok(Some(result));
                                }
                            }
                        }
                        MirInst::MatchCall {
                            field,
                            cases,
                            default_target,
                            max_depth,
                        } => {
                            let base_context = StepContext {
                                function_name: Some(function_name.clone()),
                                call_depth,
                                iteration_index,
                                call_target: None,
                                intent_goal: intent_goal.clone(),
                                intent_risk: intent_risk.clone(),
                            };

                            let scope = loop_frame.template_scope(state, iteration);

                            let compare_value = select_scoped_json_path(&scope, &state.current, field);
                            let mut chosen = None;

                            if let Some(current_value) = compare_value {
                                for case in cases {
                                    let expected =
                                        resolve_current_templates(&case.eq, &scope);
                                    if current_value == &expected {
                                        chosen = Some(&case.then_target);
                                        break;
                                    }
                                }
                            }

                            let resolved_target = chosen
                                .and_then(|target| resolve_match_target(&state.current, target, &scope))
                                .or_else(|| resolve_match_target(&state.current, default_target, &scope));

                            if let Some(target) = resolved_target {
                                if target == "$return" {
                                    loop_frame.apply_merge(state);
                                    return Ok(None);
                                }

                                let call_max_depth =
                                    max_depth.map(|v| v as usize).unwrap_or(max_call_depth);

                                if let Some(result) = self.invoke_target(
                                    functions,
                                    function_index,
                                    module_registry,
                                    host,
                                    state,
                                    step_index,
                                    remaining_steps,
                                    call_depth,
                                    call_max_depth,
                                    &target,
                                    "flow.match",
                                    base_context,
                                )? {
                                    return Ok(Some(result));
                                }
                            }
                        }
                    }
                }
            }

            loop_frame.record_iteration(state);

            if loop_frame.until_satisfied(state) {
                break;
            }
        }

        loop_frame.apply_merge(state);

        Ok(None)
    }

    fn invoke_target(
        &self,
        functions: &[MirFunction],
        function_index: &HashMap<String, usize>,
        module_registry: &ModuleRegistry,
        host: &mut dyn CapabilityHost,
        state: &mut AgentState,
        step_index: &mut usize,
        remaining_steps: &mut Option<usize>,
        call_depth: usize,
        max_call_depth: usize,
        target: &str,
        capability_label: &str,
        base_context: StepContext,
    ) -> Result<Option<ExecutionResult>, GraphemeError> {
        if target == "$return" {
            return Ok(None);
        }

        if call_depth + 1 > max_call_depth {
            let message = format!(
                "max call depth exceeded while invoking '{}': depth {} > max_depth {}",
                target,
                call_depth + 1,
                max_call_depth
            );
            return Ok(Some(fail_execution(
                state,
                *step_index,
                &Capability::from_module_op("runtime", "call_depth"),
                "MAX_CALL_DEPTH_EXCEEDED",
                message,
                ExecutionOutcome::FatalFailure,
                base_context,
            )));
        }

        let target_index = function_index.get(target).copied().ok_or_else(|| {
            GraphemeError::RuntimeError(format!(
                "call target '{}' not found in artifact MIR",
                target
            ))
        })?;

        if let Some(result) = self.execute_function(
            functions,
            function_index,
            module_registry,
            target_index,
            host,
            state,
            step_index,
            remaining_steps,
            call_depth + 1,
            max_call_depth,
        )? {
            return Ok(Some(result));
        }

        state.record_passthrough_in_place(
            *step_index,
            capability_label.to_string(),
            StepContext {
                call_target: Some(target.to_string()),
                ..base_context
            },
        );
        *step_index += 1;

        Ok(None)
    }
}

fn resolve_match_target(
    current: &JsonValue,
    target: &MirMatchTarget,
    scope: &TemplateScope<'_>,
) -> Option<String> {
    match target {
        MirMatchTarget::Target(target) => Some(target.clone()),
        MirMatchTarget::Nested {
            field,
            cases,
            default_target,
        } => {
            let compare_value = select_scoped_json_path(scope, current, field);
            if let Some(current_value) = compare_value {
                for case in cases {
                    let expected = resolve_current_templates(&case.eq, scope);
                    if current_value == &expected {
                        return resolve_match_target(current, &case.then_target, scope);
                    }
                }
            }

            resolve_match_target(current, default_target, scope)
        }
    }
}

fn branch_compare(current_value: &JsonValue, cmp: &MirCompareOp, compare_to: &JsonValue) -> bool {
    match cmp {
        MirCompareOp::Eq => current_value == compare_to,
        MirCompareOp::Gt => compare_numbers(current_value, compare_to, |a, b| a > b),
        MirCompareOp::Gte => compare_numbers(current_value, compare_to, |a, b| a >= b),
        MirCompareOp::Lt => compare_numbers(current_value, compare_to, |a, b| a < b),
        MirCompareOp::Lte => compare_numbers(current_value, compare_to, |a, b| a <= b),
    }
}

fn select_scoped_json_path<'a>(
    scope: &'a TemplateScope<'_>,
    current: &'a JsonValue,
    field: &str,
) -> Option<&'a JsonValue> {
    if let Some(path) = field.strip_prefix("state.") {
        return select_json_path(scope.state, path);
    }

    if let Some(path) = field.strip_prefix("current.") {
        return select_json_path(scope.current, path);
    }

    if let Some(path) = field.strip_prefix("item.") {
        return scope.item.and_then(|item| select_json_path(item, path));
    }

    if let Some(path) = field.strip_prefix("loop.") {
        return scope
            .loop_meta
            .as_ref()
            .and_then(|meta| select_json_path(meta, path));
    }

    select_json_path(current, field)
}

fn compare_numbers(
    current_value: &JsonValue,
    compare_to: &JsonValue,
    predicate: impl Fn(f64, f64) -> bool,
) -> bool {
    let Some(a) = current_value.as_f64() else {
        return false;
    };
    let Some(b) = compare_to.as_f64() else {
        return false;
    };
    predicate(a, b)
}

fn reduce_iteration_outputs(outputs: &[JsonValue]) -> JsonValue {
    if outputs.is_empty() {
        return JsonValue::Null;
    }

    if outputs.iter().all(|value| value.is_number()) {
        let sum = outputs
            .iter()
            .filter_map(|value| value.as_f64())
            .fold(0.0, |acc, value| acc + value);
        return serde_json::Number::from_f64(sum)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null);
    }

    if outputs.iter().all(|value| value.is_array()) {
        let merged = outputs
            .iter()
            .filter_map(|value| value.as_array())
            .flat_map(|items| items.iter().cloned())
            .collect::<Vec<_>>();
        return JsonValue::Array(merged);
    }

    if outputs.iter().all(|value| value.is_object()) {
        let mut merged = serde_json::Map::new();
        for object in outputs.iter().filter_map(|value| value.as_object()) {
            for (key, value) in object {
                merged.insert(key.clone(), value.clone());
            }
        }
        return JsonValue::Object(merged);
    }

    outputs.last().cloned().unwrap_or(JsonValue::Null)
}

fn emit_streamed_step_output(op: &str, context: &StepContext, output: &JsonValue) {
    let Some(body) = printable_stream_body(op, output) else {
        return;
    };

    let mut prefix_parts = Vec::new();
    if let Some(iteration_index) = context.iteration_index {
        prefix_parts.push(format!("iter {}", iteration_index + 1));
    }
    if context.call_depth > 0 {
        prefix_parts.push(format!("depth {}", context.call_depth));
    }
    if let Some(risk) = context.intent_risk.as_deref() {
        prefix_parts.push(format!("risk {}", risk));
    }
    if let Some(goal) = context.intent_goal.as_deref() {
        prefix_parts.push(format!("goal {}", goal));
    }
    prefix_parts.push(op.to_string());

    println!("[{}] {}", prefix_parts.join(" | "), body);
}

fn printable_stream_body(op: &str, value: &JsonValue) -> Option<String> {
    if op.eq_ignore_ascii_case("echo") {
        if let Some(message) = value.get("message").and_then(|v| v.as_str()) {
            return Some(message.to_string());
        }
    }

    if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    if let Some(stdout) = value.get("stdout").and_then(|v| v.as_str()) {
        return Some(stdout.to_string());
    }
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }

    None
}

fn build_function_index(functions: &[MirFunction]) -> HashMap<String, usize> {
    functions
        .iter()
        .enumerate()
        .map(|(idx, function)| (function.name.clone(), idx))
        .collect()
}

fn is_call_step(module: &Option<String>) -> bool {
    module
        .as_deref()
        .map(|m| m.eq_ignore_ascii_case("call"))
        .unwrap_or(false)
}

fn resolve_call_max_depth(
    args: &JsonValue,
    inherited_max_depth: usize,
) -> Result<usize, GraphemeError> {
    let Some(map) = args.as_object() else {
        return Ok(inherited_max_depth);
    };

    let Some(raw) = map.get("max_depth") else {
        return Ok(inherited_max_depth);
    };

    let value = raw.as_i64().ok_or_else(|| {
        GraphemeError::RuntimeError("call max_depth must be an integer".to_string())
    })?;

    if value < 1 {
        return Err(GraphemeError::RuntimeError(
            "call max_depth must be >= 1".to_string(),
        ));
    }

    Ok(value as usize)
}

fn fail_execution(
    state: &mut AgentState,
    step_index: usize,
    capability: &Capability,
    code: &str,
    message: String,
    outcome: ExecutionOutcome,
    context: StepContext,
) -> ExecutionResult {
    state.fail_in_place_with_context(
        step_index,
        capability.0.clone(),
        code.to_string(),
        message.clone(),
        context,
    );

    ExecutionResult {
        outcome,
        output_sttp_node_id: None,
        trace_summary: TraceSummary {
            steps: step_index + 1,
            failed_step: Some(step_index),
        },
        message: Some(message),
    }
}

fn verify_artifact_compatibility(artifact: &ArtifactEnvelope) -> Result<(), GraphemeError> {
    if artifact.payload.format != "grapheme.mir.v1" {
        return Err(GraphemeError::ArtifactCompatibilityError(format!(
            "unsupported payload format '{}'",
            artifact.payload.format
        )));
    }

    Ok(())
}

fn verify_artifact_integrity(artifact: &ArtifactEnvelope) -> Result<(), GraphemeError> {
    let mir_bytes = serde_json::to_vec(&artifact.payload.mir).map_err(|e| {
        GraphemeError::RuntimeError(format!("serialize MIR for integrity verification: {e}"))
    })?;

    let hash = Sha256::digest(&mir_bytes);
    let hash_hex = hex::encode(hash);
    let expected = format!("sha256:{hash_hex}");

    if artifact.integrity_hash != expected {
        return Err(GraphemeError::ArtifactIntegrityError(format!(
            "artifact integrity mismatch: expected '{}', got '{}'",
            expected, artifact.integrity_hash
        )));
    }

    for cap in &artifact.required_capabilities {
        let capability = Capability(cap.clone());
        if !artifact.payload.mir.capabilities.contains(&capability) {
            return Err(GraphemeError::ArtifactCompatibilityError(format!(
                "required capability '{}' missing from MIR capability set",
                cap
            )));
        }
    }

    Ok(())
}

fn args_with_pipeline_input(
    args: &JsonValue,
    input: &JsonValue,
    scope: &TemplateScope<'_>,
) -> JsonValue {
    let mut merged = match resolve_current_templates(args, scope) {
        JsonValue::Object(map) => map,
        _ => Map::new(),
    };

    merged.insert("__input".to_string(), input.clone());
    JsonValue::Object(merged)
}

fn consume_step_budget(remaining_steps: &mut Option<usize>) -> bool {
    let Some(remaining) = remaining_steps.as_mut() else {
        return true;
    };

    if *remaining == 0 {
        return false;
    }

    *remaining -= 1;
    true
}

fn resolve_each_inputs(selector: &str, input_snapshot: &JsonValue) -> Vec<JsonValue> {
    if selector == "$state" || selector == "$current" {
        return input_snapshot.as_array().cloned().unwrap_or_default();
    }

    let path = selector
        .strip_prefix("$state.")
        .or_else(|| selector.strip_prefix("$current."));
    let Some(path) = path else {
        return Vec::new();
    };

    let Some(selected) = select_json_path(input_snapshot, path) else {
        return Vec::new();
    };

    selected.as_array().cloned().unwrap_or_default()
}

fn resolve_current_templates(value: &JsonValue, scope: &TemplateScope<'_>) -> JsonValue {
    match value {
        JsonValue::Object(map) => {
            if let Some(var_ref) = variable_ref_from_object(map) {
                return resolve_variable_reference(var_ref, scope);
            }

            let mapped = map
                .iter()
                .map(|(k, v)| (k.clone(), resolve_current_templates(v, scope)))
                .collect::<Map<String, JsonValue>>();
            JsonValue::Object(mapped)
        }
        JsonValue::Array(items) => JsonValue::Array(
            items
                .iter()
                .map(|item| resolve_current_templates(item, scope))
                .collect(),
        ),
        JsonValue::String(s) => resolve_current_string_template(s, scope),
        _ => value.clone(),
    }
}

fn variable_ref_from_object(map: &Map<String, JsonValue>) -> Option<&str> {
    if map.len() != 1 {
        return None;
    }

    map.get("$var")?.as_str()
}

fn resolve_variable_reference(reference: &str, scope: &TemplateScope<'_>) -> JsonValue {
    if reference == "state" {
        return scope.state.clone();
    }

    if reference == "current" {
        return scope.current.clone();
    }

    if let Some(path) = reference
        .strip_prefix("state.")
    {
        return select_json_path(scope.state, path)
            .cloned()
            .unwrap_or(JsonValue::Null);
    }

    if let Some(path) = reference.strip_prefix("current.") {
        return select_json_path(scope.current, path)
            .cloned()
            .unwrap_or(JsonValue::Null);
    }

    if reference == "item" {
        return scope.item.cloned().unwrap_or(JsonValue::Null);
    }

    if let Some(path) = reference.strip_prefix("item.") {
        return scope
            .item
            .and_then(|item| select_json_path(item, path))
            .cloned()
            .unwrap_or(JsonValue::Null);
    }

    if reference == "loop" {
        return scope.loop_meta.clone().unwrap_or(JsonValue::Null);
    }

    if let Some(path) = reference.strip_prefix("loop.") {
        return scope
            .loop_meta
            .as_ref()
            .and_then(|meta| select_json_path(meta, path))
            .cloned()
            .unwrap_or(JsonValue::Null);
    }

    JsonValue::String(format!("${reference}"))
}

fn resolve_current_string_template(template: &str, scope: &TemplateScope<'_>) -> JsonValue {
    if template == "$state" {
        return scope.state.clone();
    }

    if template == "$current" {
        return scope.current.clone();
    }

    if let Some(path) = template
        .strip_prefix("$state.")
    {
        if path.chars().all(is_selector_char) {
            return select_json_path(scope.state, path)
                .cloned()
                .unwrap_or(JsonValue::Null);
        }
    }

    if let Some(path) = template.strip_prefix("$current.") {
        if path.chars().all(is_selector_char) {
            return select_json_path(scope.current, path)
                .cloned()
                .unwrap_or(JsonValue::Null);
        }
    }

    if template == "$item" {
        return scope.item.cloned().unwrap_or(JsonValue::Null);
    }

    if let Some(path) = template.strip_prefix("$item.") {
        if path.chars().all(is_selector_char) {
            return scope
                .item
                .and_then(|item| select_json_path(item, path))
                .cloned()
                .unwrap_or(JsonValue::Null);
        }
    }

    if template == "$loop" {
        return scope.loop_meta.clone().unwrap_or(JsonValue::Null);
    }

    if let Some(path) = template.strip_prefix("$loop.") {
        if path.chars().all(is_selector_char) {
            return scope
                .loop_meta
                .as_ref()
                .and_then(|meta| select_json_path(meta, path))
                .cloned()
                .unwrap_or(JsonValue::Null);
        }
    }

    let mut out = String::new();
    let bytes = template.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && template[i + 1..].starts_with("$") {
            let refs = ["$state", "$current", "$item", "$loop"];
            let mut matched = None;
            for r in refs {
                if template[i + 1..].starts_with(r) {
                    matched = Some(r);
                    break;
                }
            }
            let Some(prefix) = matched else {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            };

            let mut j = i + 1 + prefix.len();
            let mut resolved = None;

            if j < bytes.len() && bytes[j] == b'.' {
                j += 1;
                let path_start = j;
                while j < bytes.len() && is_selector_char(bytes[j] as char) {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'}' {
                    let path = &template[path_start..j];
                    let scoped = match prefix {
                        "$state" => select_json_path(scope.state, path),
                        "$current" => select_json_path(scope.current, path),
                        "$item" => scope.item.and_then(|item| select_json_path(item, path)),
                        "$loop" => scope
                            .loop_meta
                            .as_ref()
                            .and_then(|meta| select_json_path(meta, path)),
                        _ => None,
                    };
                    resolved = Some(scoped.map(json_value_to_inline_string).unwrap_or_default());
                    j += 1;
                }
            } else if j < bytes.len() && bytes[j] == b'}' {
                let scoped = match prefix {
                    "$state" => Some(scope.state),
                    "$current" => Some(scope.current),
                    "$item" => scope.item,
                    "$loop" => scope.loop_meta.as_ref(),
                    _ => None,
                };
                resolved = Some(scoped.map(json_value_to_inline_string).unwrap_or_default());
                j += 1;
            }

            if let Some(text) = resolved {
                out.push_str(&text);
                i = j;
                continue;
            }
        }

        let refs = ["$state", "$current", "$item", "$loop"];
        let mut found = None;
        for r in refs {
            if template[i..].starts_with(r) {
                found = Some(r);
                break;
            }
        }
        if let Some(prefix) = found {
            let mut j = i + prefix.len();
            if j < bytes.len() && bytes[j] == b'.' {
                j += 1;
                while j < bytes.len() && is_selector_char(bytes[j] as char) {
                    j += 1;
                }
                let path = &template[i + prefix.len() + 1..j];
                let scoped = match prefix {
                    "$state" => select_json_path(scope.state, path),
                    "$current" => select_json_path(scope.current, path),
                    "$item" => scope.item.and_then(|item| select_json_path(item, path)),
                    "$loop" => scope
                        .loop_meta
                        .as_ref()
                        .and_then(|meta| select_json_path(meta, path)),
                    _ => None,
                };
                if let Some(value) = scoped {
                    out.push_str(&json_value_to_inline_string(value));
                }
                i = j;
                continue;
            }

            let scoped = match prefix {
                "$state" => Some(scope.state),
                "$current" => Some(scope.current),
                "$item" => scope.item,
                "$loop" => scope.loop_meta.as_ref(),
                _ => None,
            };
            if let Some(value) = scoped {
                out.push_str(&json_value_to_inline_string(value));
            }
            i += prefix.len();
            continue;
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    JsonValue::String(out)
}

fn is_selector_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

fn select_json_path<'a>(root: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    let mut current = root;
    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }
        current = current.get(segment)?;
    }
    Some(current)
}

fn json_value_to_inline_string(value: &JsonValue) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }

    serde_json::to_string(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module_manager::{CompatibilityMode, LoadModuleRequest};
    use grapheme_artifact::{
        build_aot_from_artifact, build_artifact_from_mir, build_stage_b_container_from_aot,
        Capability, MirBlock, MirFunction, MirFunctionKind, MirInst, MirIntentConfig,
        MirLoopConfig, MirLoopMergeMode, MirProgram, MirTerminator,
    };
    use serde_json::{json, Map, Value as JsonValue};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestHost {
        mode: HostMode,
    }

    enum HostMode {
        StepIndexNumber,
        VerboseObject,
        LongString,
        Fatal,
    }

    impl CapabilityHost for TestHost {
        fn call(&mut self, call: &CapabilityCall) -> Result<JsonValue, HostCallError> {
            match self.mode {
                HostMode::StepIndexNumber => Ok(JsonValue::from(call.step_index as i64)),
                HostMode::VerboseObject => Ok(json!({
                    "message": "ok",
                    "payload": "abcdefghijklmnopqrstuvwxyz",
                })),
                HostMode::LongString => {
                    Ok(JsonValue::String("abcdefghijklmnopqrstuvwxyz".to_string()))
                }
                HostMode::Fatal => {
                    Err(HostCallError::Fatal("injected runtime failure".to_string()))
                }
            }
        }
    }

    fn runtime_events_snapshot_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("golden")
            .join("module-lifecycle-events.snapshot.json")
    }

    #[cfg_attr(feature = "wasix-runtime", allow(dead_code))]
    fn stage_b_strict_mode_snapshot_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("golden")
            .join("aot-stage-b-strict-mode.snapshot.json")
    }

    #[test]
    fn loop_merge_append_collects_iteration_outputs() {
        let state = execute_loop(
            3,
            MirLoopMergeMode::Append,
            TracePolicy::lean_default(),
            HostMode::StepIndexNumber,
        );
        assert_eq!(state.current, json!([0, 1, 2]));
    }

    #[test]
    fn loop_merge_reduce_sums_numeric_outputs() {
        let state = execute_loop(
            3,
            MirLoopMergeMode::Reduce,
            TracePolicy::lean_default(),
            HostMode::StepIndexNumber,
        );
        assert_eq!(state.current, json!(3.0));
    }

    #[test]
    fn loop_merge_none_restores_pre_loop_state() {
        let state = execute_loop(
            3,
            MirLoopMergeMode::None,
            TracePolicy::lean_default(),
            HostMode::StepIndexNumber,
        );
        assert_eq!(state.current, JsonValue::Null);
    }

    #[test]
    fn trace_policy_limits_pipeline_history() {
        let mut policy = TracePolicy::lean_default();
        policy.max_pipeline_steps = 2;
        policy.projection = crate::state::TraceProjection::Full;

        let state = execute_loop(
            6,
            MirLoopMergeMode::Replace,
            policy,
            HostMode::StepIndexNumber,
        );
        assert_eq!(state.pipeline.len(), 2);
        assert_eq!(state.pipeline[0].output, json!(4));
        assert_eq!(state.pipeline[1].output, json!(5));
    }

    #[test]
    fn trace_policy_minimal_projection_keeps_small_summary() {
        let mut policy = TracePolicy::lean_default();
        policy.max_pipeline_steps = 8;
        policy.max_string_bytes = 8;
        policy.projection = crate::state::TraceProjection::Minimal;

        let state = execute_loop(
            1,
            MirLoopMergeMode::Replace,
            policy,
            HostMode::VerboseObject,
        );
        let output = state
            .pipeline
            .first()
            .expect("pipeline step")
            .output
            .as_object()
            .expect("object output");
        assert_eq!(
            output.get("message"),
            Some(&JsonValue::String("ok".to_string()))
        );
        assert!(output.get("payload").is_none());
        assert_eq!(
            output.get("_kind"),
            Some(&JsonValue::String("object".to_string()))
        );
    }

    #[test]
    fn trace_policy_zero_steps_drops_pipeline_history() {
        let mut policy = TracePolicy::lean_default();
        policy.max_pipeline_steps = 0;

        let state = execute_loop(
            4,
            MirLoopMergeMode::Replace,
            policy,
            HostMode::StepIndexNumber,
        );
        assert!(state.pipeline.is_empty());
        assert_eq!(state.current, json!(3));
    }

    #[test]
    fn trace_policy_truncates_long_strings_in_minimal_mode() {
        let mut policy = TracePolicy::lean_default();
        policy.max_pipeline_steps = 4;
        policy.max_string_bytes = 5;
        policy.projection = crate::state::TraceProjection::Minimal;

        let state = execute_loop(1, MirLoopMergeMode::Replace, policy, HostMode::LongString);
        let output = state
            .pipeline
            .first()
            .expect("pipeline step")
            .output
            .as_str()
            .expect("string output");
        assert_eq!(output, "abcde...");
    }

    #[test]
    fn trace_includes_intent_metadata_when_present() {
        let capability = Capability::from_module_op("core", "echo");
        let instruction = MirInst::Call {
            module: Some("core".to_string()),
            op: "echo".to_string(),
            capability: capability.clone(),
            arg_count: 0,
            args: JsonValue::Object(Map::new()),
            stores_state: true,
        };

        let function = MirFunction {
            name: "Main".to_string(),
            kind: MirFunctionKind::Fragment,
            retry_config: None,
            timeout_config: None,
            intent_config: Some(MirIntentConfig {
                goal: Some("validate canary before 50% rollout".to_string()),
                risk: Some("high".to_string()),
            }),
            loop_config: None,
            blocks: vec![MirBlock {
                id: 0,
                instructions: vec![instruction],
                terminator: MirTerminator::ReturnState,
            }],
        };

        let mir = MirProgram {
            functions: vec![function],
            capabilities: vec![capability],
        };

        let artifact = build_artifact_from_mir(&mir, Some("Main")).expect("artifact builds");
        let runtime = RuntimeEngine::new(RuntimeOptions::default());
        let mut host = TestHost {
            mode: HostMode::StepIndexNumber,
        };

        let (state, result) = runtime
            .execute_artifact(&artifact, &mut host)
            .expect("runtime execution succeeds");

        assert!(matches!(result.outcome, ExecutionOutcome::Succeeded));
        let step = state.pipeline.first().expect("trace has at least one step");
        assert_eq!(
            step.intent_goal.as_deref(),
            Some("validate canary before 50% rollout")
        );
        assert_eq!(step.intent_risk.as_deref(), Some("high"));
    }

    #[test]
    fn loop_each_selector_reads_array_from_current_path() {
        let snapshot = json!({
            "jobs": [
                {"id": "a"},
                {"id": "b"}
            ]
        });

        let items = resolve_each_inputs("$current.jobs", &snapshot);
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].get("id"),
            Some(&JsonValue::String("a".to_string()))
        );
        assert_eq!(
            items[1].get("id"),
            Some(&JsonValue::String("b".to_string()))
        );
    }

    #[test]
    fn execute_artifact_applies_initial_state_current_option() {
        let mir = MirProgram {
            functions: vec![MirFunction {
                name: "Main".to_string(),
                kind: MirFunctionKind::Fragment,
                retry_config: None,
                timeout_config: None,
                intent_config: None,
                loop_config: None,
                blocks: vec![MirBlock {
                    id: 0,
                    instructions: vec![],
                    terminator: MirTerminator::ReturnState,
                }],
            }],
            capabilities: vec![],
        };

        let artifact = build_artifact_from_mir(&mir, Some("Main")).expect("artifact builds");
        let runtime = RuntimeEngine::new(RuntimeOptions {
            initial_state_current: Some(json!({ "seed": 42 })),
            ..RuntimeOptions::default()
        });
        let mut host = TestHost {
            mode: HostMode::StepIndexNumber,
        };

        let (state, result) = runtime
            .execute_artifact(&artifact, &mut host)
            .expect("runtime execution succeeds");

        assert!(matches!(result.outcome, ExecutionOutcome::Succeeded));
        assert_eq!(state.current, json!({ "seed": 42 }));
    }

    #[test]
    fn args_with_pipeline_input_interpolates_current_templates() {
        let args = json!({
            "url": "https://example.com/job/$current.id",
            "payload": "$current",
            "id": "$current.id"
        });
        let current = json!({"id": "123", "status": "ready"});
        let scope = TemplateScope {
            current: &current,
            state: &current,
            item: None,
            loop_meta: None,
        };

        let resolved = args_with_pipeline_input(&args, &current, &scope);
        assert_eq!(
            resolved.get("url"),
            Some(&JsonValue::String(
                "https://example.com/job/123".to_string()
            ))
        );
        assert_eq!(resolved.get("payload"), Some(&current));
        assert_eq!(
            resolved.get("id"),
            Some(&JsonValue::String("123".to_string()))
        );
        assert_eq!(resolved.get("__input"), Some(&current));
    }

    #[test]
    fn args_with_pipeline_input_interpolates_brace_current_templates() {
        let args = json!({
            "message": "fib:{$current.a}",
            "status": "{$current.status}",
            "snapshot": "{$current}"
        });
        let current = json!({"a": 21, "status": "ready"});
        let scope = TemplateScope {
            current: &current,
            state: &current,
            item: None,
            loop_meta: None,
        };

        let resolved = args_with_pipeline_input(&args, &current, &scope);
        assert_eq!(
            resolved.get("message"),
            Some(&JsonValue::String("fib:21".to_string()))
        );
        assert_eq!(
            resolved.get("status"),
            Some(&JsonValue::String("ready".to_string()))
        );
        assert_eq!(
            resolved.get("snapshot"),
            Some(&JsonValue::String(
                "{\"a\":21,\"status\":\"ready\"}".to_string()
            ))
        );
    }

    fn execute_loop(
        max: u32,
        merge: MirLoopMergeMode,
        trace_policy: TracePolicy,
        host_mode: HostMode,
    ) -> AgentState {
        let artifact = loop_artifact(max, merge);
        let mut host = TestHost { mode: host_mode };

        let mut options = RuntimeOptions::default();
        options.trace_policy = trace_policy;

        let runtime = RuntimeEngine::new(options);
        let (state, result) = runtime
            .execute_artifact(&artifact, &mut host)
            .expect("runtime execution succeeds");

        assert!(matches!(result.outcome, ExecutionOutcome::Succeeded));
        state
    }

    fn write_temp_wasm(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        path.push(format!("grapheme-runtime-{tag}-{ts}.wasm"));
        fs::write(&path, bytes).expect("write temp wasm bytes");
        path
    }

    #[test]
    fn activation_updates_registry_generation_metadata() {
        let wasm = write_temp_wasm("activate-runtime", b"runtime-wasm-a");
        let mut runtime = RuntimeEngine::new(RuntimeOptions::default());

        let activation = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("activation should succeed");

        let resolved = runtime
            .options
            .module_registry
            .resolve_call(Some("http"), "get", "http.get")
            .expect("http.get should resolve");

        assert_eq!(resolved.generation_id, Some(activation.generation_id));
        assert_eq!(
            resolved.content_hash.as_deref(),
            Some(activation.content_hash.as_str())
        );

        let _ = fs::remove_file(wasm);
    }

    #[test]
    fn activation_rejects_module_when_required_capabilities_are_denied() {
        let wasm = write_temp_wasm("activate-policy-denied", b"runtime-wasm-policy-denied");
        let mut options = RuntimeOptions::default();
        options
            .capability_policy
            .denied
            .push(Capability("http.get.allowed_domain".to_string()));
        let mut runtime = RuntimeEngine::new(options);

        let err = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect_err("activation should be denied by capability policy");

        assert!(matches!(
            err,
            ModuleLoadError::PolicyDeniedCapabilities { .. }
        ));

        let _ = fs::remove_file(wasm);
    }

    #[test]
    fn rollback_restores_prior_registry_generation_metadata() {
        let wasm_a = write_temp_wasm("rollback-runtime-a", b"runtime-wasm-a");
        let wasm_b = write_temp_wasm("rollback-runtime-b", b"runtime-wasm-b");
        let mut runtime = RuntimeEngine::new(RuntimeOptions::default());

        let first = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.1.0".to_string()),
            })
            .expect("second activation should succeed");

        let rolled_back = runtime
            .rollback_module_generation("http")
            .expect("rollback should succeed");
        assert_eq!(rolled_back.generation_id, first.generation_id);

        let resolved = runtime
            .options
            .module_registry
            .resolve_call(Some("http"), "get", "http.get")
            .expect("http.get should resolve");
        assert_eq!(resolved.generation_id, Some(first.generation_id));

        assert!(runtime
            .module_lifecycle_events()
            .iter()
            .any(|e| e.kind == crate::module_manager::ModuleLifecycleEventKind::Rollback));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn execution_state_includes_module_lifecycle_events() {
        let wasm = write_temp_wasm("events-runtime", b"runtime-wasm-events");
        let mut runtime = RuntimeEngine::new(RuntimeOptions::default());

        runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("activation should succeed");

        let artifact = loop_artifact(1, MirLoopMergeMode::Replace);
        let mut host = TestHost {
            mode: HostMode::StepIndexNumber,
        };

        let (state, result) = runtime
            .execute_artifact(&artifact, &mut host)
            .expect("runtime execution succeeds");

        assert!(matches!(result.outcome, ExecutionOutcome::Succeeded));
        assert!(!state.runtime_events.is_empty());
        assert!(state.runtime_events.iter().any(|event| {
            event.get("kind").and_then(|v| v.as_str()) == Some("module.activated")
                && event.get("module_id").and_then(|v| v.as_str()) == Some("http")
        }));

        let _ = fs::remove_file(wasm);
    }

    #[test]
    fn pinned_module_registry_snapshot_isolated_from_later_activation() {
        let wasm_a = write_temp_wasm("pin-runtime-a", b"runtime-pin-a");
        let wasm_b = write_temp_wasm("pin-runtime-b", b"runtime-pin-b");
        let mut runtime = RuntimeEngine::new(RuntimeOptions::default());

        let first = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let pinned = runtime.options.module_registry.clone();

        let second = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.1.0".to_string()),
            })
            .expect("second activation should succeed");

        let pinned_resolved = pinned
            .resolve_call(Some("http"), "get", "http.get")
            .expect("pinned http.get should resolve");
        let current_resolved = runtime
            .options
            .module_registry
            .resolve_call(Some("http"), "get", "http.get")
            .expect("current http.get should resolve");

        assert_eq!(pinned_resolved.generation_id, Some(first.generation_id));
        assert_eq!(current_resolved.generation_id, Some(second.generation_id));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn execution_a_and_b_split_generation_resolution_after_activation() {
        let wasm_a = write_temp_wasm("split-runtime-a", b"runtime-split-a");
        let wasm_b = write_temp_wasm("split-runtime-b", b"runtime-split-b");
        let mut runtime = RuntimeEngine::new(RuntimeOptions::default());

        let first = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        // Execution A starts now and pins the pre-activation registry snapshot.
        let execution_a_snapshot = runtime.options.module_registry.clone();

        let second = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.1.0".to_string()),
            })
            .expect("second activation should succeed");

        // Execution B starts after activation and resolves through current active generation.
        let execution_b_registry = runtime.options.module_registry.clone();

        let resolved_a = execution_a_snapshot
            .resolve_call(Some("http"), "get", "http.get")
            .expect("execution A should resolve http.get");
        let resolved_b = execution_b_registry
            .resolve_call(Some("http"), "get", "http.get")
            .expect("execution B should resolve http.get");

        assert_eq!(resolved_a.generation_id, Some(first.generation_id));
        assert_eq!(resolved_b.generation_id, Some(second.generation_id));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn failed_activation_does_not_replace_active_generation() {
        let wasm_a = write_temp_wasm("runtime-fail-activation-a", b"runtime-fail-a");
        let wasm_b = write_temp_wasm("runtime-fail-activation-b", b"runtime-fail-b");
        let mut runtime = RuntimeEngine::new(RuntimeOptions::default());

        let first = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let err = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::MirV1,
                version: Some("2.0.0".to_string()),
            })
            .expect_err("incompatible ABI activation should fail");

        assert!(matches!(err, ModuleLoadError::AbiIncompatible { .. }));

        let resolved = runtime
            .options
            .module_registry
            .resolve_call(Some("http"), "get", "http.get")
            .expect("http.get should resolve");
        assert_eq!(resolved.generation_id, Some(first.generation_id));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn runtime_event_payload_contract_is_stable() {
        let wasm_a = write_temp_wasm("runtime-events-contract-a", b"runtime-contract-a");
        let wasm_b = write_temp_wasm("runtime-events-contract-b", b"runtime-contract-b");
        let mut runtime = RuntimeEngine::new(RuntimeOptions::default());

        runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let _ = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::MirV1,
                version: Some("2.0.0".to_string()),
            })
            .expect_err("incompatible activation should fail");

        let artifact = loop_artifact(1, MirLoopMergeMode::Replace);
        let mut host = TestHost {
            mode: HostMode::StepIndexNumber,
        };
        let (state, result) = runtime
            .execute_artifact(&artifact, &mut host)
            .expect("runtime execution succeeds");

        assert!(matches!(result.outcome, ExecutionOutcome::Succeeded));
        assert!(state.runtime_events.iter().all(|event| {
            event.get("kind").is_some()
                && event.get("module_id").is_some()
                && event.get("generation_id").is_some()
                && event.get("version").is_some()
                && event.get("content_hash").is_some()
                && event.get("reason").is_some()
        }));
        assert!(state.runtime_events.iter().all(|event| {
            event
                .get("kind")
                .and_then(|v| v.as_str())
                .map(|kind| kind.starts_with("module."))
                .unwrap_or(false)
        }));
        assert!(state.runtime_events.iter().any(|event| {
            event.get("kind").and_then(|v| v.as_str()) == Some("module.activation_failed")
                && event.get("reason").and_then(|v| v.as_str()) == Some("abi_incompatible")
        }));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn rollback_restores_prior_generation_after_runtime_failure_window() {
        let wasm_a = write_temp_wasm("runtime-window-a", b"runtime-window-a");
        let wasm_b = write_temp_wasm("runtime-window-b", b"runtime-window-b");
        let mut runtime = RuntimeEngine::new(RuntimeOptions::default());

        let first = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.1.0".to_string()),
            })
            .expect("second activation should succeed");

        let mut host = TestHost {
            mode: HostMode::Fatal,
        };
        let artifact = loop_artifact(1, MirLoopMergeMode::Replace);
        let (_state, execution) = runtime
            .execute_artifact(&artifact, &mut host)
            .expect("execution should complete with failure envelope");
        assert!(matches!(execution.outcome, ExecutionOutcome::FatalFailure));

        let rolled_back = runtime
            .rollback_module_generation("http")
            .expect("rollback should succeed after failure window");
        assert_eq!(rolled_back.generation_id, first.generation_id);

        let resolved = runtime
            .options
            .module_registry
            .resolve_call(Some("http"), "get", "http.get")
            .expect("http.get should resolve after rollback");
        assert_eq!(resolved.generation_id, Some(first.generation_id));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn module_lifecycle_event_snapshot_matches_golden_contract() {
        let wasm_a = write_temp_wasm("runtime-snapshot-a", b"runtime-snapshot-a");
        let wasm_b = write_temp_wasm("runtime-snapshot-b", b"runtime-snapshot-b");
        let mut runtime = RuntimeEngine::new(RuntimeOptions::default());

        runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let _ = runtime
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::MirV1,
                version: Some("2.0.0".to_string()),
            })
            .expect_err("incompatible activation should fail");

        let artifact = loop_artifact(1, MirLoopMergeMode::Replace);
        let mut host = TestHost {
            mode: HostMode::StepIndexNumber,
        };
        let (state, result) = runtime
            .execute_artifact(&artifact, &mut host)
            .expect("runtime execution succeeds");
        assert!(matches!(result.outcome, ExecutionOutcome::Succeeded));

        let kinds = state
            .runtime_events
            .iter()
            .filter_map(|e| e.get("kind").and_then(|v| v.as_str()))
            .map(ToOwned::to_owned)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let snapshot = json!({
            "required_fields": ["kind", "module_id", "generation_id", "version", "content_hash", "reason"],
            "kinds": kinds,
        });

        let expected = fs::read_to_string(runtime_events_snapshot_path())
            .expect("read lifecycle snapshot golden")
            .parse::<JsonValue>()
            .expect("parse lifecycle snapshot golden json");
        assert_eq!(snapshot, expected);

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn required_signature_ops_validation_reports_missing_exports() {
        let exported_ops = vec![crate::module_manifest::ExportedOp {
            op: "post".to_string(),
            input_schema_ref: None,
            output_schema_ref: None,
            effect: crate::module_manifest::EffectKind::Network,
        }];

        let missing = validate_required_signature_ops("http", &exported_ops)
            .expect_err("missing signature ops should be reported");

        assert!(missing.contains(&"get".to_string()));
        assert!(!missing.contains(&"post".to_string()));
    }

    #[test]
    fn execute_aot_stage_b_records_container_routing_event() {
        let artifact = loop_artifact(1, MirLoopMergeMode::Replace);
        let stage_a = build_aot_from_artifact(&artifact).expect("stage_a build should succeed");
        let imports = vec![
            "grapheme.runtime.host.v1::state.read".to_string(),
            "grapheme.runtime.host.v1::state.write".to_string(),
        ];
        let stage_b = build_stage_b_container_from_aot(&stage_a, b"\0asmstageb", &imports)
            .expect("stage_b build should succeed");

        let runtime = RuntimeEngine::new(RuntimeOptions::default());
        let mut host = TestHost {
            mode: HostMode::StepIndexNumber,
        };

        let (state, result) = runtime
            .execute_aot(&stage_b, &mut host)
            .expect("stage_b runtime execution should succeed");

        assert!(matches!(result.outcome, ExecutionOutcome::Succeeded));
        assert!(
            result
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("stage_b scaffold executed via parity path")
                || result
                    .message
                    .as_deref()
                    .unwrap_or_default()
                    .contains("stage_b container executed directly via wasix backend")
        );

        let stage_b_event = state.runtime_events.iter().find(|event| {
            event.get("kind").and_then(|v| v.as_str()) == Some("aot.stage_b.container_routed")
        });
        assert!(stage_b_event.is_some());
    }

    #[cfg(not(feature = "wasix-runtime"))]
    #[test]
    fn execute_aot_stage_b_strict_mode_rejects_parity_fallback() {
        let artifact = loop_artifact(1, MirLoopMergeMode::Replace);
        let stage_a = build_aot_from_artifact(&artifact).expect("stage_a build should succeed");
        let imports = vec![
            "grapheme.runtime.host.v1::state.read".to_string(),
            "grapheme.runtime.host.v1::state.write".to_string(),
        ];
        let stage_b = build_stage_b_container_from_aot(&stage_a, b"\0asmstageb", &imports)
            .expect("stage_b build should succeed");

        let mut options = RuntimeOptions::default();
        options.strict_stage_b_container_execution = true;
        let runtime = RuntimeEngine::new(options);
        let mut host = TestHost {
            mode: HostMode::StepIndexNumber,
        };

        let err = runtime.execute_aot(&stage_b, &mut host).expect_err(
            "strict mode should reject parity fallback when wasix runtime is unavailable",
        );

        assert!(matches!(err, GraphemeError::ArtifactCompatibilityError(_)));
        assert!(err
            .to_string()
            .contains("strict stage_b container execution required"));

        let snapshot = json!({
            "error_kind": "artifact_compatibility_error",
            "message": err.to_string(),
        });
        let expected = fs::read_to_string(stage_b_strict_mode_snapshot_path())
            .expect("read stage_b strict mode snapshot golden")
            .parse::<JsonValue>()
            .expect("parse stage_b strict mode snapshot golden json");
        assert_eq!(snapshot, expected);
    }

    #[cfg(feature = "wasix-runtime")]
    #[test]
    fn execute_aot_stage_b_direct_path_with_wasix_feature() {
        let workflow_wasm = wat::parse_str(
            r#"(module
                (import "wasi_snapshot_preview1" "proc_exit" (func $proc_exit (param i32)))
                (memory 1)
                (export "memory" (memory 0))
                (func $_start)
                (export "_start" (func $_start))
            )"#,
        )
        .expect("compile WAT to WASM bytes");

        let artifact = loop_artifact(1, MirLoopMergeMode::Replace);
        let stage_a = build_aot_from_artifact(&artifact).expect("stage_a build should succeed");
        let imports = vec![
            "grapheme.runtime.host.v1::state.read".to_string(),
            "grapheme.runtime.host.v1::state.write".to_string(),
        ];
        let stage_b = build_stage_b_container_from_aot(&stage_a, &workflow_wasm, &imports)
            .expect("stage_b build should succeed");

        let mut options = RuntimeOptions::default();
        options.strict_stage_b_container_execution = true;
        let runtime = RuntimeEngine::new(options);
        let mut host = TestHost {
            mode: HostMode::StepIndexNumber,
        };

        let (state, result) = match runtime.execute_aot(&stage_b, &mut host) {
            Ok(outcome) => outcome,
            Err(err) => panic!("stage_b direct path should succeed with wasix feature: {err}"),
        };

        assert!(matches!(result.outcome, ExecutionOutcome::Succeeded));
        assert!(result
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("stage_b container executed directly via wasix backend"));
        assert!(state.current.is_null());
    }

    fn loop_artifact(max: u32, merge: MirLoopMergeMode) -> ArtifactEnvelope {
        let capability = Capability::from_module_op("core", "echo");
        let instruction = MirInst::Call {
            module: Some("core".to_string()),
            op: "echo".to_string(),
            capability: capability.clone(),
            arg_count: 0,
            args: JsonValue::Object(Map::new()),
            stores_state: true,
        };

        let function = MirFunction {
            name: "Main".to_string(),
            kind: MirFunctionKind::Fragment,
            retry_config: None,
            timeout_config: None,
            intent_config: None,
            loop_config: Some(MirLoopConfig {
                max: Some(max),
                each: None,
                until: None,
                merge,
            }),
            blocks: vec![MirBlock {
                id: 0,
                instructions: vec![instruction],
                terminator: MirTerminator::ReturnState,
            }],
        };

        let mir = MirProgram {
            functions: vec![function],
            capabilities: vec![capability],
        };

        build_artifact_from_mir(&mir, Some("Main")).expect("artifact builds")
    }
}
