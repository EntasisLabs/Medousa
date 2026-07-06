//! Embedded Grapheme SDK for in-process compile and execution workflows.
//!
//! The SDK wraps compiler and runtime crates with an ergonomic builder API,
//! structured output formatting, and AOT helper entrypoints.

use grapheme_artifact::mir::MirFunctionKind;
use grapheme_artifact::{
    build_stage_b_container_from_aot, AotEnvelope, ArtifactEnvelope, CapabilityPolicy,
    ExecutionResult,
};
use grapheme_compiler::ast::{ScalarKind, TypeRef};
use grapheme_compiler::hir::HirExecutableKind;
use grapheme_compiler::verifier::LintWarning;
use grapheme_compiler::{CompiledScript, Compiler, CompilerError, CompilerOptions};
use grapheme_runtime::{
    apply_hotload_store, default_hotload_store_path, discover_wasm_modules,
    discovered_module_to_load_request, load_hotload_store, save_hotload_store, ActivationResult,
    CapabilityCall, CapabilityHost, HostCallError, HotloadError, LoadModuleRequest,
    ModuleLifecycleEvent, ModuleLoadError, PolicyGuard, RuntimeEngine, RuntimeError,
    RuntimeOptions, TracePolicy,
};
use grapheme_signatures::{
    find_op_spec, op_output_object_fields, op_output_type, op_stability, op_stability_label,
    ArgType,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

type HostFactory = Arc<dyn Fn() -> Box<dyn CapabilityHost + Send> + Send + Sync>;
type CapabilityObserver = Arc<dyn Fn(&CapabilityCall) + Send + Sync>;
type CapabilityInterceptor =
    Arc<dyn Fn(&CapabilityCall) -> Option<Result<JsonValue, HostCallError>> + Send + Sync>;

/// Structured output mode for SDK formatting helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuredMode {
    /// YAML output.
    Yaml,
    /// JSON output.
    Json,
}

/// Top-level execute payload returned by SDK execution entrypoints.
#[derive(Debug, Clone, Serialize)]
pub struct ExecuteResultPayload {
    /// Executed artifact id.
    pub artifact_id: String,
    /// Runtime execution result summary.
    pub execution: ExecutionResult,
    /// Final runtime state as JSON.
    pub final_state: JsonValue,
    /// Non-fatal compiler warnings collected during compile path.
    pub lint_warnings: Vec<LintWarning>,
}

/// Errors returned by SDK compile/execute/format operations.
#[derive(Debug, Error)]
pub enum GraphemeSdkError {
    #[error(transparent)]
    Compiler(#[from] CompilerError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error("aot contract error: {0}")]
    Contract(String),
    #[error("serialize output: {0}")]
    Serialization(String),
    #[error("module lifecycle error: {0}")]
    ModuleLifecycle(String),
}

/// Builder for configuring `GraphemeEngine` runtime behavior.
pub struct GraphemeEngineBuilder {
    runtime_options: RuntimeOptions,
    compiler_options: CompilerOptions,
    module_bindings: HashMap<String, PathBuf>,
    host_factory: Option<HostFactory>,
    capability_observer: Option<CapabilityObserver>,
    capability_interceptor: Option<CapabilityInterceptor>,
}

impl Default for GraphemeEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphemeEngineBuilder {
    /// Create a new builder with runtime defaults.
    pub fn new() -> Self {
        Self {
            runtime_options: RuntimeOptions::default(),
            compiler_options: CompilerOptions::default(),
            module_bindings: HashMap::new(),
            host_factory: None,
            capability_observer: None,
            capability_interceptor: None,
        }
    }

    /// Set policy guard for runtime admission checks.
    pub fn with_policy_guard(mut self, guard: PolicyGuard) -> Self {
        self.runtime_options.policy_guard = guard;
        self
    }

    /// Set trace policy shaping runtime pipeline history.
    pub fn with_trace_policy(mut self, policy: TracePolicy) -> Self {
        self.runtime_options.trace_policy = policy;
        self
    }

    /// Enable or disable artifact integrity verification.
    pub fn with_verify_integrity(mut self, enabled: bool) -> Self {
        self.runtime_options.verify_integrity = enabled;
        self
    }

    /// Enable or disable streaming plain step output.
    pub fn with_stream_step_output(mut self, enabled: bool) -> Self {
        self.runtime_options.stream_step_output = enabled;
        self
    }

    /// Enable or disable strict Stage B container execution mode.
    pub fn with_strict_stage_b_container_execution(mut self, enabled: bool) -> Self {
        self.runtime_options.strict_stage_b_container_execution = enabled;
        self
    }

    /// Set compiler options used by source-based compile helpers.
    pub fn with_compiler_options(mut self, options: CompilerOptions) -> Self {
        self.compiler_options = options;
        self
    }

    /// Set capability policy used by runtime activation/dispatch checks.
    pub fn with_capability_policy(mut self, policy: CapabilityPolicy) -> Self {
        self.runtime_options.capability_policy = policy;
        self
    }

    /// Set optional maximum step count.
    pub fn with_max_steps(mut self, max_steps: Option<usize>) -> Self {
        self.runtime_options.max_steps = max_steps;
        self
    }

    /// Set optional maximum nested call depth.
    pub fn with_max_call_depth(mut self, max_call_depth: Option<usize>) -> Self {
        self.runtime_options.max_call_depth = max_call_depth;
        self
    }

    /// Seed the runtime with an initial `state.current` value before the first step executes.
    pub fn with_initial_state_current(mut self, initial_current: JsonValue) -> Self {
        self.runtime_options.initial_state_current = Some(initial_current);
        self
    }

    /// Bind a module id to a Wasm path for runtime resolution.
    pub fn with_module_path(mut self, module: &str, path: impl Into<PathBuf>) -> Self {
        self.module_bindings
            .insert(module.to_lowercase(), path.into());
        self
    }

    /// Configure the runtime module registry before engine build.
    pub fn configure_module_registry(
        mut self,
        configure: impl FnOnce(&mut grapheme_runtime::ModuleRegistry),
    ) -> Self {
        configure(&mut self.runtime_options.module_registry);
        self
    }

    /// Hydrate module manager/registry state from `.grapheme/modules/hotload.json` when present.
    pub fn with_default_hotload_store(mut self) -> Self {
        if let Ok(Some(store)) = load_hotload_store(&default_hotload_store_path()) {
            apply_hotload_store(
                &store,
                &mut self.runtime_options.module_manager,
                &mut self.runtime_options.module_registry,
            );
        }
        self
    }

    /// Register an observer called for each capability invocation.
    pub fn with_capability_observer<F>(mut self, observer: F) -> Self
    where
        F: Fn(&CapabilityCall) + Send + Sync + 'static,
    {
        self.capability_observer = Some(Arc::new(observer));
        self
    }

    /// Register an interceptor that can override capability call results.
    pub fn with_capability_interceptor<F>(mut self, interceptor: F) -> Self
    where
        F: Fn(&CapabilityCall) -> Option<Result<JsonValue, HostCallError>> + Send + Sync + 'static,
    {
        self.capability_interceptor = Some(Arc::new(interceptor));
        self
    }

    /// Provide a custom host factory for full capability dispatch control.
    pub fn with_host_factory<F>(mut self, host_factory: F) -> Self
    where
        F: Fn() -> Box<dyn CapabilityHost + Send> + Send + Sync + 'static,
    {
        self.host_factory = Some(Arc::new(host_factory));
        self
    }

    /// Build the configured `GraphemeEngine`.
    pub fn build(self) -> GraphemeEngine {
        GraphemeEngine {
            runtime_options: self.runtime_options,
            compiler_options: self.compiler_options,
            module_bindings: self.module_bindings,
            host_factory: self.host_factory,
            capability_observer: self.capability_observer,
            capability_interceptor: self.capability_interceptor,
        }
    }
}

/// High-level embedded engine for compile/execute and AOT helper flows.
pub struct GraphemeEngine {
    runtime_options: RuntimeOptions,
    compiler_options: CompilerOptions,
    module_bindings: HashMap<String, PathBuf>,
    host_factory: Option<HostFactory>,
    capability_observer: Option<CapabilityObserver>,
    capability_interceptor: Option<CapabilityInterceptor>,
}

impl GraphemeEngine {
    /// Create a builder for `GraphemeEngine`.
    pub fn builder() -> GraphemeEngineBuilder {
        GraphemeEngineBuilder::new()
    }

    /// Create a stateful runtime session preserving module lifecycle state.
    pub fn runtime_session(&self) -> GraphemeRuntimeSession {
        let mut options = self.runtime_options.clone();
        for (module, path) in &self.module_bindings {
            options
                .module_registry
                .set_wasm_path(module.as_str(), path.clone());
        }

        GraphemeRuntimeSession {
            runtime: RuntimeEngine::new(options),
            compiler_options: self.compiler_options.clone(),
            host_factory: self.host_factory.clone(),
            capability_observer: self.capability_observer.clone(),
            capability_interceptor: self.capability_interceptor.clone(),
        }
    }

    /// Compile and execute source in one call.
    pub fn execute_source(&self, source: &str) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        let compiled = Compiler::compile_source(source, self.compiler_options.clone())?;
        self.execute_compiled(&compiled)
    }

    /// Compile and execute source, applying an optional per-call `state.current`
    /// seed without rebuilding the engine.
    ///
    /// This lets callers cache a single configured `GraphemeEngine` and still
    /// vary the initial state per execution (instead of rebuilding the whole
    /// engine via `GraphemeEngineBuilder::with_initial_state_current`). Passing
    /// `None` preserves whatever initial state the engine was configured with.
    pub fn execute_source_with_initial_state(
        &self,
        source: &str,
        initial_state_current: Option<JsonValue>,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        let compiled = Compiler::compile_source(source, self.compiler_options.clone())?;
        self.execute_artifact_with_lints_and_state(
            &compiled.artifact,
            compiled.compilation.lint_warnings.clone(),
            initial_state_current,
        )
    }

    /// Compile source into a Stage A AOT envelope.
    pub fn compile_source_to_aot(&self, source: &str) -> Result<AotEnvelope, GraphemeSdkError> {
        let compiled = Compiler::compile_source_to_aot(source, self.compiler_options.clone())?;
        Ok(compiled.aot)
    }

    /// Compile source into Stage B AOT using provided workflow bytes/imports.
    pub fn compile_source_to_aot_stage_b(
        &self,
        source: &str,
        workflow_wasm: &[u8],
        allowed_imports: &[String],
    ) -> Result<AotEnvelope, GraphemeSdkError> {
        let stage_a = self.compile_source_to_aot(source)?;
        build_stage_b_container_from_aot(&stage_a, workflow_wasm, allowed_imports)
            .map_err(|e| GraphemeSdkError::Contract(e.to_string()))
    }

    /// Execute a prebuilt artifact envelope.
    pub fn execute_artifact(
        &self,
        artifact: &ArtifactEnvelope,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        self.execute_artifact_with_lints(artifact, Vec::new())
    }

    /// Execute a prebuilt AOT envelope.
    pub fn execute_aot(&self, aot: &AotEnvelope) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        self.execute_aot_with_lints(aot, Vec::new())
    }

    /// Execute a compiled script produced by compiler APIs.
    pub fn execute_compiled(
        &self,
        compiled: &CompiledScript,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        self.execute_artifact_with_lints(
            &compiled.artifact,
            compiled.compilation.lint_warnings.clone(),
        )
    }

    /// Format execute results as YAML or JSON.
    pub fn format_result(
        &self,
        result: &ExecuteResultPayload,
        mode: StructuredMode,
    ) -> Result<String, GraphemeSdkError> {
        match mode {
            StructuredMode::Json => serde_json::to_string_pretty(result)
                .map_err(|e| GraphemeSdkError::Serialization(e.to_string())),
            StructuredMode::Yaml => serde_yaml::to_string(result)
                .map_err(|e| GraphemeSdkError::Serialization(e.to_string())),
        }
    }

    /// Format an AOT envelope as YAML or JSON.
    pub fn format_aot(
        &self,
        aot: &AotEnvelope,
        mode: StructuredMode,
    ) -> Result<String, GraphemeSdkError> {
        match mode {
            StructuredMode::Json => serde_json::to_string_pretty(aot)
                .map_err(|e| GraphemeSdkError::Serialization(e.to_string())),
            StructuredMode::Yaml => serde_yaml::to_string(aot)
                .map_err(|e| GraphemeSdkError::Serialization(e.to_string())),
        }
    }

    fn execute_artifact_with_lints(
        &self,
        artifact: &ArtifactEnvelope,
        lint_warnings: Vec<LintWarning>,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        self.execute_artifact_with_lints_and_state(artifact, lint_warnings, None)
    }

    fn execute_artifact_with_lints_and_state(
        &self,
        artifact: &ArtifactEnvelope,
        lint_warnings: Vec<LintWarning>,
        initial_state_current: Option<JsonValue>,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        let mut options = self.runtime_options.clone();
        for (module, path) in &self.module_bindings {
            options
                .module_registry
                .set_wasm_path(module.as_str(), path.clone());
        }
        if let Some(state) = initial_state_current {
            options.initial_state_current = Some(state);
        }

        let runtime = RuntimeEngine::new(options);
        let (state, execution) = if let Some(factory) = &self.host_factory {
            let mut host = factory();
            runtime.execute_artifact(artifact, host.as_mut())?
        } else {
            let mut host = StdlibHost {
                capability_observer: self.capability_observer.clone(),
                capability_interceptor: self.capability_interceptor.clone(),
            };
            runtime.execute_artifact(artifact, &mut host)?
        };

        Ok(ExecuteResultPayload {
            artifact_id: artifact.artifact_id.clone(),
            execution,
            final_state: state.to_json(),
            lint_warnings,
        })
    }

    fn execute_aot_with_lints(
        &self,
        aot: &AotEnvelope,
        lint_warnings: Vec<LintWarning>,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        let mut options = self.runtime_options.clone();
        for (module, path) in &self.module_bindings {
            options
                .module_registry
                .set_wasm_path(module.as_str(), path.clone());
        }

        let runtime = RuntimeEngine::new(options);
        let (state, execution) = if let Some(factory) = &self.host_factory {
            let mut host = factory();
            runtime
                .execute_aot(aot, host.as_mut())
                .map_err(map_runtime_aot_error)?
        } else {
            let mut host = StdlibHost {
                capability_observer: self.capability_observer.clone(),
                capability_interceptor: self.capability_interceptor.clone(),
            };
            runtime
                .execute_aot(aot, &mut host)
                .map_err(map_runtime_aot_error)?
        };

        Ok(ExecuteResultPayload {
            artifact_id: aot.base_artifact.artifact_id.clone(),
            execution,
            final_state: state.to_json(),
            lint_warnings,
        })
    }
}

/// Stateful SDK runtime session preserving module activation lifecycle state.
pub struct GraphemeRuntimeSession {
    runtime: RuntimeEngine,
    compiler_options: CompilerOptions,
    host_factory: Option<HostFactory>,
    capability_observer: Option<CapabilityObserver>,
    capability_interceptor: Option<CapabilityInterceptor>,
}

impl GraphemeRuntimeSession {
    /// Compile and execute source while preserving prior module lifecycle state.
    pub fn execute_source(&self, source: &str) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        let compiled = Compiler::compile_source(source, self.compiler_options.clone())?;
        self.execute_compiled(&compiled)
    }

    /// Execute a precompiled script while preserving prior module lifecycle state.
    pub fn execute_compiled(
        &self,
        compiled: &CompiledScript,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        self.execute_artifact_with_lints(
            &compiled.artifact,
            compiled.compilation.lint_warnings.clone(),
        )
    }

    /// Execute a prebuilt artifact envelope while preserving module lifecycle state.
    pub fn execute_artifact(
        &self,
        artifact: &ArtifactEnvelope,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        self.execute_artifact_with_lints(artifact, Vec::new())
    }

    /// Execute a prebuilt AOT envelope while preserving module lifecycle state.
    pub fn execute_aot(&self, aot: &AotEnvelope) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        self.execute_aot_with_lints(aot, Vec::new())
    }

    /// Activate a new module generation in the current session runtime.
    pub fn activate_module_generation(
        &mut self,
        request: LoadModuleRequest,
    ) -> Result<ActivationResult, GraphemeSdkError> {
        self.runtime
            .activate_module_generation(request)
            .map_err(map_module_load_error)
    }

    /// Activate a discovered module by id from scan roots and persist hotload state.
    pub fn activate_discovered_module(
        &mut self,
        module_id: &str,
        scan_roots: &[PathBuf],
    ) -> Result<ActivationResult, GraphemeSdkError> {
        let report = discover_wasm_modules(scan_roots);
        let discovered = report
            .modules
            .into_iter()
            .find(|module| module.module_id == module_id)
            .ok_or_else(|| {
                GraphemeSdkError::ModuleLifecycle(format!(
                    "module '{module_id}' not found in scan roots"
                ))
            })?;
        let request = discovered_module_to_load_request(&discovered);
        let activation = self.activate_module_generation(request)?;
        self.save_default_hotload_store()?;
        Ok(activation)
    }

    /// Roll back the active module generation for a module.
    pub fn rollback_module_generation(
        &mut self,
        module_id: &str,
    ) -> Result<ActivationResult, GraphemeSdkError> {
        let activation = self
            .runtime
            .rollback_module_generation(module_id)
            .map_err(map_module_load_error)?;
        self.save_default_hotload_store()?;
        Ok(activation)
    }

    /// Persist the current session module manager state to `.grapheme/modules/hotload.json`.
    pub fn save_default_hotload_store(&self) -> Result<(), GraphemeSdkError> {
        save_hotload_store(
            &default_hotload_store_path(),
            &self.runtime.hotload_snapshot(),
        )
        .map_err(map_hotload_error)
    }

    /// Return a snapshot of lifecycle events observed in this session runtime.
    pub fn module_lifecycle_events(&self) -> Vec<ModuleLifecycleEvent> {
        self.runtime.module_lifecycle_events().to_vec()
    }

    fn execute_artifact_with_lints(
        &self,
        artifact: &ArtifactEnvelope,
        lint_warnings: Vec<LintWarning>,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        let (state, execution) = if let Some(factory) = &self.host_factory {
            let mut host = factory();
            self.runtime.execute_artifact(artifact, host.as_mut())?
        } else {
            let mut host = StdlibHost {
                capability_observer: self.capability_observer.clone(),
                capability_interceptor: self.capability_interceptor.clone(),
            };
            self.runtime.execute_artifact(artifact, &mut host)?
        };

        Ok(ExecuteResultPayload {
            artifact_id: artifact.artifact_id.clone(),
            execution,
            final_state: state.to_json(),
            lint_warnings,
        })
    }

    fn execute_aot_with_lints(
        &self,
        aot: &AotEnvelope,
        lint_warnings: Vec<LintWarning>,
    ) -> Result<ExecuteResultPayload, GraphemeSdkError> {
        let (state, execution) = if let Some(factory) = &self.host_factory {
            let mut host = factory();
            self.runtime
                .execute_aot(aot, host.as_mut())
                .map_err(map_runtime_aot_error)?
        } else {
            let mut host = StdlibHost {
                capability_observer: self.capability_observer.clone(),
                capability_interceptor: self.capability_interceptor.clone(),
            };
            self.runtime
                .execute_aot(aot, &mut host)
                .map_err(map_runtime_aot_error)?
        };

        Ok(ExecuteResultPayload {
            artifact_id: aot.base_artifact.artifact_id.clone(),
            execution,
            final_state: state.to_json(),
            lint_warnings,
        })
    }
}

fn map_runtime_aot_error(err: RuntimeError) -> GraphemeSdkError {
    match err {
        RuntimeError::ArtifactCompatibilityError(message) => GraphemeSdkError::Contract(message),
        other => GraphemeSdkError::Runtime(other),
    }
}

fn map_module_load_error(err: ModuleLoadError) -> GraphemeSdkError {
    GraphemeSdkError::ModuleLifecycle(err.to_string())
}

fn map_hotload_error(err: HotloadError) -> GraphemeSdkError {
    GraphemeSdkError::ModuleLifecycle(err.to_string())
}

/// Detail tier for module search explain payloads.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModuleSearchDetail {
    /// Full guidance payload with operation and usage hints.
    #[default]
    Full,
    /// Concise payload optimized for ranking and short previews.
    Concise,
}

/// Search options for module discovery payload APIs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleSearchOptions {
    /// Enable explain output shape (when false, returns compact metadata per module hit).
    pub explain: bool,
    /// Explain detail tier when explain mode is active.
    #[serde(default)]
    pub detail: ModuleSearchDetail,
    /// Optional maximum number of ranked results.
    pub top: Option<usize>,
    /// Optional minimum relevance threshold.
    pub min_score: Option<f64>,
    /// Include experimental operations in operation-name matching.
    #[serde(default)]
    pub include_experimental: bool,
}

/// Public module search payload contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleSearchPayload {
    /// Original search query.
    pub query: String,
    /// Explain detail tier label when explain mode is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Optional top-k applied during ranked explain mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<usize>,
    /// Optional minimum relevance threshold applied during ranked explain mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
    /// Stability selection policy applied to operation-name matching.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stability_policy: Option<String>,
    /// Number of rows in `matches`.
    pub count: usize,
    /// Search result rows.
    pub matches: Vec<ModuleSearchMatch>,
}

/// Public module search match row contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleSearchMatch {
    pub module_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why_matched: Option<Vec<String>>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_ops: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avoid_when: Option<String>,
    pub related_examples: Vec<String>,
}

/// Public `modules ops` payload contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleOpsPayload {
    pub query: String,
    pub matches: Vec<ModuleOpRow>,
}

/// Public row contract for `modules ops` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleOpRow {
    pub module_id: String,
    pub op: String,
    pub stability: String,
    pub effect: grapheme_runtime::EffectKind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<OperationArgRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_object_type: Option<OperationObjectType>,
    pub output_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_object_type: Option<OperationObjectType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationArgRow {
    pub name: String,
    pub ty: String,
    pub required: bool,
}

/// Discovery row returned by SDK example listing/search APIs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExampleDiscoveryItem {
    /// Stable example name.
    pub name: String,
    /// Relative path where the example is typically scaffolded.
    pub path: String,
    /// One-line description.
    pub summary: String,
    /// Guidance on when to choose this example.
    pub use_when: String,
    /// Authoring complexity tier.
    pub complexity: String,
    /// Discovery tags.
    pub tags: Vec<String>,
    /// Whether native modules are required.
    pub requires_native_modules: bool,
    /// Suggested CLI run command.
    pub run: String,
}

struct ExampleCatalogEntry {
    name: &'static str,
    path: &'static str,
    summary: &'static str,
    use_when: &'static str,
    complexity: &'static str,
    tags: &'static [&'static str],
    requires_native_modules: bool,
}

const EXAMPLE_CATALOG: &[ExampleCatalogEntry] = &[
    ExampleCatalogEntry {
        name: "main",
        path: "examples/main.gr",
        summary: "Canonical project entrypoint with glyph-based composition.",
        use_when: "You want the default project root flow and main wiring pattern.",
        complexity: "beginner",
        tags: &["entrypoint", "glyph", "composition"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "hello-world",
        path: "examples/hello-world.gr",
        summary: "Smallest end-to-end query pipeline.",
        use_when: "You need a first successful run to validate install/runtime.",
        complexity: "beginner",
        tags: &["intro", "core", "query"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "core-merge",
        path: "examples/core-merge.gr",
        summary: "Shows object merge semantics in core transforms.",
        use_when: "You need to build/reshape state objects.",
        complexity: "beginner",
        tags: &["core", "transform", "object"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "core-filter",
        path: "examples/core-filter.gr",
        summary: "Filters list items by field equality.",
        use_when: "You need list narrowing before later pipeline steps.",
        complexity: "beginner",
        tags: &["core", "list", "filter"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "core-validate-schema",
        path: "examples/core-validate-schema.gr",
        summary: "Validates required fields in payload-like objects.",
        use_when: "You need fast contract checks before side effects.",
        complexity: "intermediate",
        tags: &["core", "validation", "schema"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "request-transform-output",
        path: "examples/request-transform-output.gr",
        summary: "Transforms request data into a structured output envelope.",
        use_when: "You need to normalize or map inbound payloads.",
        complexity: "intermediate",
        tags: &["transform", "mapping", "output"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "mutation-update-preferences",
        path: "examples/mutation-update-preferences.gr",
        summary: "Mutation flow that updates preference-like state.",
        use_when: "You need write-style workflows with controlled state changes.",
        complexity: "intermediate",
        tags: &["mutation", "state", "core"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "mutation-state-machine-apply",
        path: "examples/mutation-state-machine-apply.gr",
        summary: "State-machine style mutation transitions.",
        use_when: "You need explicit status/lifecycle transitions.",
        complexity: "advanced",
        tags: &["mutation", "state-machine", "transition"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "resilience-composition",
        path: "examples/resilience-composition.gr",
        summary: "Shows retry/timeout/loop resilience composition patterns.",
        use_when: "You need robust flows under transient errors.",
        complexity: "advanced",
        tags: &["resilience", "retry", "timeout"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "subscription-heartbeat-readable",
        path: "examples/subscription-heartbeat-readable.gr",
        summary: "Readable heartbeat subscription-style workflow.",
        use_when: "You need periodic signal/event style patterns.",
        complexity: "intermediate",
        tags: &["subscription", "heartbeat", "loop"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "http-get",
        path: "examples/http-get.gr",
        summary: "Fetch a live page over HTTP and convert it to markdown.",
        use_when: "You want a practical fetch -> transform flow using http and html modules.",
        complexity: "beginner",
        tags: &["http", "html", "markdown", "transform"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "websearch-basic",
        path: "examples/websearch-basic.gr",
        summary: "Search web results, iterate URLs, and normalize each page into markdown.",
        use_when: "You need a practical websearch -> fetch -> parse loop for research workflows.",
        complexity: "intermediate",
        tags: &["websearch", "http", "html", "loop", "research"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "websearch-report",
        path: "examples/websearch-report.gr",
        summary: "Search -> fetch -> clean -> report pipeline.",
        use_when: "You need source-grounded report generation.",
        complexity: "advanced",
        tags: &["websearch", "report", "research"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "web-provider-catalog",
        path: "examples/web-provider-catalog.gr",
        summary: "Discover web providers and render provider ids.",
        use_when: "You need capability-aware provider discovery in-flow.",
        complexity: "beginner",
        tags: &["web", "providers", "discovery"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "web-provider-routing",
        path: "examples/web-provider-routing.gr",
        summary: "Capability-aware provider routing with fallback behavior.",
        use_when: "You need provider-selection control flow with graceful fallback.",
        complexity: "advanced",
        tags: &["web", "routing", "fallback"],
        requires_native_modules: false,
    },
    ExampleCatalogEntry {
        name: "web-xaviv-planned",
        path: "examples/web-xaviv-planned.gr",
        summary: "Planned-provider handling path for unsupported providers.",
        use_when: "You need explicit unsupported-provider branching patterns.",
        complexity: "intermediate",
        tags: &["web", "provider", "planned"],
        requires_native_modules: false,
    },
];

struct ModuleSearchGuidance {
    summary: &'static str,
    use_when: &'static str,
    avoid_when: &'static str,
}

/// Return runtime module manifests currently known to the core runtime catalog.
pub fn discover_module_manifests() -> Vec<grapheme_runtime::ModuleManifest> {
    grapheme_runtime::core_v1_manifests()
}

/// Find a module manifest by id (case-insensitive).
pub fn module_manifest_by_id(module_id: &str) -> Option<grapheme_runtime::ModuleManifest> {
    discover_module_manifests()
        .into_iter()
        .find(|m| m.module_id.eq_ignore_ascii_case(module_id))
}

/// Return curated example paths for a module id.
pub fn curated_examples_for_module(module_id: &str) -> &'static [&'static str] {
    match module_id.to_lowercase().as_str() {
        "http" => &["examples/http-get.gr"],
        "websearch" => &[
            "examples/websearch-basic.gr",
            "examples/websearch-materials.gr",
            "examples/websearch-report.gr",
        ],
        "tcp" => &["examples/tcp-connect.gr"],
        "smtp" => &["examples/smtp-send.gr"],
        "sql" => &[
            "examples/sql-query.gr",
            "examples/sql-query-params.gr",
            "examples/sql-transaction.gr",
            "examples/sql-transaction-rollback.gr",
        ],
        "surreal" => &[
            "examples/surreal-select.gr",
            "examples/surreal-query.gr",
            "examples/surreal-select-filtered.gr",
            "examples/surreal-query-vars.gr",
            "examples/surreal-health.gr",
            "examples/surreal-create.gr",
            "examples/surreal-update.gr",
            "examples/surreal-delete.gr",
        ],
        "email" => &["examples/email-smtp.gr"],
        "data" => &["examples/data-read-csv.gr", "examples/data-filter.gr"],
        "pdf" => &["examples/pdf-generate.gr"],
        "image" => &["examples/image-metadata.gr"],
        "plot" => &["examples/plot-line.gr"],
        "io" => &["examples/io-list.gr"],
        "memory" => &["examples/memory-roundtrip.gr"],
        "secrets" => &["examples/secrets-handle.gr", "examples/secrets-sign.gr"],
        "json" | "csv" | "yaml" | "html" => &["examples/request-transform-output.gr"],
        "core" => &[
            "examples/core-merge.gr",
            "examples/core-filter.gr",
            "examples/core-validate-schema.gr",
            "examples/mutation-update-preferences.gr",
        ],
        _ => &[],
    }
}

/// Search modules and return compact or explainable ranked payloads.
pub fn modules_search_payload(query: &str, options: &ModuleSearchOptions) -> JsonValue {
    let payload = modules_search_contract(query, options);
    serde_json::to_value(payload).unwrap_or(JsonValue::Null)
}

/// Build typed module search contract payload.
pub fn modules_search_contract(query: &str, options: &ModuleSearchOptions) -> ModuleSearchPayload {
    let q = query.to_lowercase();

    if !options.explain {
        let mut matches = Vec::new();

        for manifest in discover_module_manifests() {
            let module_id = manifest.module_id;
            let module_match = module_id.to_lowercase().contains(&q);
            let matching_ops = manifest
                .exported_ops
                .iter()
                .filter_map(|op| {
                    if op.op.to_lowercase().contains(&q)
                        || format!("{}.{}", module_id, op.op)
                            .to_lowercase()
                            .contains(&q)
                    {
                        if !options.include_experimental
                            && matches!(
                                op_stability(&module_id, &op.op),
                                grapheme_signatures::SignatureStability::Experimental
                            )
                        {
                            return None;
                        }
                        Some(op.op.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if !(module_match || !matching_ops.is_empty()) {
                continue;
            }

            let guidance = module_search_guidance(&module_id);
            let effects = manifest
                .exported_ops
                .iter()
                .map(|op| effect_name(&op.effect).to_string())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let related_examples = curated_examples_for_module(&module_id)
                .iter()
                .map(|path| (*path).to_string())
                .collect::<Vec<_>>();

            matches.push(ModuleSearchMatch {
                module_id,
                score: None,
                why_matched: None,
                summary: guidance.summary.to_string(),
                op_count: Some(manifest.exported_ops.len()),
                effects: Some(effects),
                matching_ops: Some(matching_ops),
                use_when: None,
                avoid_when: None,
                related_examples,
            });
        }

        matches.sort_by(|a, b| a.module_id.cmp(&b.module_id));

        return ModuleSearchPayload {
            query: query.to_string(),
            detail: None,
            top: None,
            min_score: None,
            stability_policy: Some(if options.include_experimental {
                "all".to_string()
            } else {
                "stable_preferred".to_string()
            }),
            count: matches.len(),
            matches,
        };
    }

    let mut matches = Vec::new();

    for manifest in discover_module_manifests() {
        let module_id = manifest.module_id;
        let module_match = module_id.to_lowercase().contains(&q);
        let matching_ops = manifest
            .exported_ops
            .iter()
            .filter_map(|op| {
                if op.op.to_lowercase().contains(&q)
                    || format!("{}.{}", module_id, op.op)
                        .to_lowercase()
                        .contains(&q)
                {
                    if !options.include_experimental
                        && matches!(
                            op_stability(&module_id, &op.op),
                            grapheme_signatures::SignatureStability::Experimental
                        )
                    {
                        return None;
                    }
                    Some(op.op.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if !(module_match || !matching_ops.is_empty()) {
            continue;
        }

        let mut why_matched = Vec::new();
        if module_match {
            why_matched.push("module_id".to_string());
        }
        if !matching_ops.is_empty() {
            why_matched.push("op_name".to_string());
        }

        let relevance_score =
            compute_module_relevance_score(&q, &module_id, &matching_ops, module_match);

        let guidance = module_search_guidance(&module_id);
        let related_examples = curated_examples_for_module(&module_id)
            .iter()
            .map(|path| (*path).to_string())
            .collect::<Vec<_>>();

        let row = match options.detail {
            ModuleSearchDetail::Concise => ModuleSearchMatch {
                module_id,
                score: Some(relevance_score),
                why_matched: Some(why_matched),
                summary: guidance.summary.to_string(),
                op_count: None,
                effects: None,
                matching_ops: None,
                use_when: None,
                avoid_when: None,
                related_examples,
            },
            ModuleSearchDetail::Full => ModuleSearchMatch {
                module_id,
                score: Some(relevance_score),
                why_matched: Some(why_matched),
                summary: guidance.summary.to_string(),
                op_count: None,
                effects: None,
                matching_ops: Some(matching_ops),
                use_when: Some(guidance.use_when.to_string()),
                avoid_when: Some(guidance.avoid_when.to_string()),
                related_examples,
            },
        };

        matches.push(row);
    }

    matches.sort_by(|a, b| {
        let a_score = a.score.unwrap_or(0.0);
        let b_score = b.score.unwrap_or(0.0);

        b_score
            .partial_cmp(&a_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.module_id.cmp(&b.module_id))
    });

    if let Some(min_score) = options.min_score {
        matches.retain(|row| row.score.is_some_and(|score| score >= min_score));
    }

    if let Some(top) = options.top {
        matches.truncate(top);
    }

    ModuleSearchPayload {
        query: query.to_string(),
        detail: Some(match options.detail {
            ModuleSearchDetail::Full => "full".to_string(),
            ModuleSearchDetail::Concise => "concise".to_string(),
        }),
        top: options.top,
        min_score: options.min_score,
        stability_policy: Some(if options.include_experimental {
            "all".to_string()
        } else {
            "stable_preferred".to_string()
        }),
        count: matches.len(),
        matches,
    }
}

/// Build `modules ops` payload for a query string.
pub fn modules_ops_payload(query: &str) -> JsonValue {
    let payload = modules_ops_contract(query);
    serde_json::to_value(payload).unwrap_or(JsonValue::Null)
}

/// Build typed `modules ops` contract payload.
pub fn modules_ops_contract(query: &str) -> ModuleOpsPayload {
    let q = query.to_lowercase();
    let mut matches = Vec::new();

    for manifest in discover_module_manifests() {
        let module_id = manifest.module_id;
        let module_match = module_id.to_lowercase().contains(&q);

        for op in manifest.exported_ops {
            let full = format!("{}.{}", module_id, op.op);
            if module_match || op.op.to_lowercase().contains(&q) || full.to_lowercase().contains(&q)
            {
                matches.push(ModuleOpRow {
                    module_id: module_id.clone(),
                    stability: op_stability_label(&module_id, &op.op).to_string(),
                    args: op_arg_rows(&module_id, &op.op),
                    input_object_type: op_input_object_type(&module_id, &op.op),
                    output_type: output_type_label(
                        &module_id,
                        &op.op,
                        op.output_schema_ref.as_deref(),
                    ),
                    output_object_type: op_output_object_type(&module_id, &op.op),
                    op: op.op,
                    effect: op.effect,
                    input_schema_ref: op.input_schema_ref,
                    output_schema_ref: op.output_schema_ref,
                });
            }
        }
    }

    matches.sort_by(|a, b| a.module_id.cmp(&b.module_id).then(a.op.cmp(&b.op)));

    ModuleOpsPayload {
        query: query.to_string(),
        matches,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactModuleOp {
    pub op: String,
    pub stability: String,
    pub effect: grapheme_runtime::EffectKind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<OperationArgRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_object_type: Option<OperationObjectType>,
    pub output_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_object_type: Option<OperationObjectType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationObjectField {
    pub ty: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationObjectType {
    pub kind: String,
    pub required: Vec<String>,
    pub properties: BTreeMap<String, OperationObjectField>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectGroup {
    pub effect: String,
    pub ops: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModuleOpSummary {
    pub total_ops: usize,
    pub typed_ops: usize,
    pub untyped_ops: usize,
    pub input_schema_refs: usize,
    pub output_schema_refs: usize,
}

/// Public `modules info` payload contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfoPayload {
    pub module_id: String,
    pub version: String,
    pub abi: grapheme_runtime::ModuleAbi,
    pub entrypoint: String,
    pub required_capabilities: Vec<String>,
    pub limits: grapheme_runtime::ResourceLimits,
    pub op_summary: ModuleOpSummary,
    pub exported_ops_by_effect: Vec<EffectGroup>,
    pub exported_ops: Vec<CompactModuleOp>,
}

/// Public `modules types` payload contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleTypesPayload {
    pub module_id: String,
    pub type_summary: ModuleOpSummary,
    pub types_by_effect: Vec<EffectGroup>,
    pub types: Vec<CompactModuleOp>,
}

/// Public executable/function reflection kind contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutableReflectionKind {
    Query,
    Mutation,
    Subscription,
    Iterator,
}

/// Public executable/function reflection contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutableReflection {
    pub name: String,
    pub kind: ExecutableReflectionKind,
    /// Optional executable description derived from `@intent(goal: ...)`.
    pub description: Option<String>,
    pub input_type: Option<String>,
    pub output_type: Option<String>,
    pub loop_directive_count: usize,
    pub recursive_directive_count: usize,
    pub retry_directive_count: usize,
    pub timeout_directive_count: usize,
    pub pipeline_count: usize,
    pub step_count: usize,
}

/// Public executable reflection payload contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutablesReflectionPayload {
    pub count: usize,
    pub executables: Vec<ExecutableReflection>,
}

/// Reflect executable/function metadata from Grapheme source.
pub fn executables_reflection_from_source(
    source: &str,
) -> Result<Vec<ExecutableReflection>, GraphemeSdkError> {
    let compilation = grapheme_compiler::compile(source)?;
    Ok(compilation
        .hir
        .executable_defs
        .iter()
        .map(executable_reflection_from_hir)
        .collect())
}

/// Build typed executable reflection payload from Grapheme source.
pub fn executables_reflection_contract_from_source(
    source: &str,
) -> Result<ExecutablesReflectionPayload, GraphemeSdkError> {
    let executables = executables_reflection_from_source(source)?;
    Ok(ExecutablesReflectionPayload {
        count: executables.len(),
        executables,
    })
}

/// Build JSON executable reflection payload from Grapheme source.
pub fn executables_reflection_payload_from_source(
    source: &str,
) -> Result<JsonValue, GraphemeSdkError> {
    let payload = executables_reflection_contract_from_source(source)?;
    serde_json::to_value(payload).map_err(|e| GraphemeSdkError::Serialization(e.to_string()))
}

/// Reflect executable/function metadata from a compiled artifact.
pub fn executables_reflection_from_artifact(
    artifact: &ArtifactEnvelope,
) -> Vec<ExecutableReflection> {
    artifact
        .payload
        .mir
        .functions
        .iter()
        .map(executable_reflection_from_mir)
        .collect()
}

/// Build typed executable reflection payload from a compiled artifact.
pub fn executables_reflection_contract_from_artifact(
    artifact: &ArtifactEnvelope,
) -> ExecutablesReflectionPayload {
    let executables = executables_reflection_from_artifact(artifact);
    ExecutablesReflectionPayload {
        count: executables.len(),
        executables,
    }
}

/// Build JSON executable reflection payload from a compiled artifact.
pub fn executables_reflection_payload_from_artifact(artifact: &ArtifactEnvelope) -> JsonValue {
    let payload = executables_reflection_contract_from_artifact(artifact);
    serde_json::to_value(payload).unwrap_or(JsonValue::Null)
}

fn executable_reflection_from_hir(
    executable: &grapheme_compiler::hir::HirExecutable,
) -> ExecutableReflection {
    let kind = match executable.kind {
        HirExecutableKind::Query => ExecutableReflectionKind::Query,
        HirExecutableKind::Mutation => ExecutableReflectionKind::Mutation,
        HirExecutableKind::Subscription => ExecutableReflectionKind::Subscription,
        HirExecutableKind::Fragment => ExecutableReflectionKind::Iterator,
    };

    let pipeline_count = executable.pipelines.len();
    let step_count = executable.pipelines.iter().map(|p| p.steps.len()).sum();

    ExecutableReflection {
        name: executable.name.clone(),
        kind,
        description: intent_goal_from_hir(executable.intent_args.as_ref()),
        input_type: executable.input_type.as_ref().map(type_ref_label),
        output_type: executable.output_type.as_ref().map(type_ref_label),
        loop_directive_count: executable.loop_directive_count,
        recursive_directive_count: executable.recursive_directive_count,
        retry_directive_count: executable.retry_directive_count,
        timeout_directive_count: executable.timeout_directive_count,
        pipeline_count,
        step_count,
    }
}

fn executable_reflection_from_mir(
    function: &grapheme_artifact::mir::MirFunction,
) -> ExecutableReflection {
    let kind = match function.kind {
        MirFunctionKind::Query => ExecutableReflectionKind::Query,
        MirFunctionKind::Mutation => ExecutableReflectionKind::Mutation,
        MirFunctionKind::Subscription => ExecutableReflectionKind::Subscription,
        MirFunctionKind::Fragment => ExecutableReflectionKind::Iterator,
    };

    let pipeline_count = function.blocks.len();
    let step_count = function.blocks.iter().map(|b| b.instructions.len()).sum();

    ExecutableReflection {
        name: function.name.clone(),
        kind,
        description: function.intent_config.as_ref().and_then(|cfg| cfg.goal.clone()),
        input_type: None,
        output_type: None,
        loop_directive_count: usize::from(function.loop_config.is_some()),
        recursive_directive_count: 0,
        retry_directive_count: usize::from(function.retry_config.is_some()),
        timeout_directive_count: usize::from(function.timeout_config.is_some()),
        pipeline_count,
        step_count,
    }
}

fn intent_goal_from_hir(intent_args: Option<&JsonValue>) -> Option<String> {
    intent_args
        .and_then(|args| args.as_object())
        .and_then(|args| args.get("goal"))
        .and_then(|goal| goal.as_str())
        .map(|goal| goal.to_string())
}

fn type_ref_label(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Named(name, non_null) => {
            if *non_null {
                format!("{}!", name)
            } else {
                name.clone()
            }
        }
        TypeRef::List(inner, non_null) => {
            let inner_label = type_ref_label(inner);
            if *non_null {
                format!("[{}]!", inner_label)
            } else {
                format!("[{}]", inner_label)
            }
        }
        TypeRef::Scalar(kind, non_null) => {
            let base = match kind {
                ScalarKind::String => "String",
                ScalarKind::Int => "Int",
                ScalarKind::Float => "Float",
                ScalarKind::Bool => "Bool",
                ScalarKind::Any => "Any",
                ScalarKind::Json => "Json",
            };
            if *non_null {
                format!("{}!", base)
            } else {
                base.to_string()
            }
        }
    }
}

fn effect_name(effect: &grapheme_runtime::EffectKind) -> &'static str {
    match effect {
        grapheme_runtime::EffectKind::Pure => "pure",
        grapheme_runtime::EffectKind::Network => "network",
        grapheme_runtime::EffectKind::Io => "io",
        grapheme_runtime::EffectKind::State => "state",
        grapheme_runtime::EffectKind::Secrets => "secrets",
        grapheme_runtime::EffectKind::Control => "control",
    }
}

fn arg_type_label(ty: ArgType) -> &'static str {
    match ty {
        ArgType::String => "string",
        ArgType::Number => "number",
        ArgType::Boolean => "boolean",
        ArgType::Object => "object",
        ArgType::Array => "array",
        ArgType::Any => "any",
    }
}

fn output_type_label(module_id: &str, op_name: &str, output_schema_ref: Option<&str>) -> String {
    if let Some(shape) = output_schema_ref {
        return shape.to_string();
    }

    arg_type_label(op_output_type(module_id, op_name)).to_string()
}

fn op_input_object_type(module_id: &str, op_name: &str) -> Option<OperationObjectType> {
    let spec = find_op_spec(module_id, op_name)?;
    if spec.args.is_empty() {
        return None;
    }

    let mut required = Vec::new();
    let mut properties = BTreeMap::new();

    for arg in spec.args {
        if arg.required {
            required.push(arg.name.to_string());
        }

        properties.insert(
            arg.name.to_string(),
            OperationObjectField {
                ty: arg_type_label(arg.ty).to_string(),
                required: arg.required,
            },
        );
    }

    Some(OperationObjectType {
        kind: "object".to_string(),
        required,
        properties,
    })
}

fn op_arg_rows(module_id: &str, op_name: &str) -> Vec<OperationArgRow> {
    let Some(spec) = find_op_spec(module_id, op_name) else {
        return Vec::new();
    };

    spec.args
        .iter()
        .map(|arg| OperationArgRow {
            name: arg.name.to_string(),
            ty: arg_type_label(arg.ty).to_string(),
            required: arg.required,
        })
        .collect()
}

fn op_output_object_type(module_id: &str, op_name: &str) -> Option<OperationObjectType> {
    let fields = op_output_object_fields(module_id, op_name)?;

    let mut required = Vec::new();
    let mut properties = BTreeMap::new();

    for field in fields {
        if field.required {
            required.push(field.name.to_string());
        }

        properties.insert(
            field.name.to_string(),
            OperationObjectField {
                ty: arg_type_label(field.ty).to_string(),
                required: field.required,
            },
        );
    }

    Some(OperationObjectType {
        kind: "object".to_string(),
        required,
        properties,
    })
}

fn compact_module_ops(
    module_id: &str,
    ops: &[grapheme_runtime::ExportedOp],
) -> Vec<CompactModuleOp> {
    ops.iter()
        .map(|op| CompactModuleOp {
            op: op.op.clone(),
            stability: op_stability_label(module_id, &op.op).to_string(),
            effect: op.effect.clone(),
            args: op_arg_rows(module_id, &op.op),
            input_object_type: op_input_object_type(module_id, &op.op),
            output_type: output_type_label(module_id, &op.op, op.output_schema_ref.as_deref()),
            output_object_type: op_output_object_type(module_id, &op.op),
            input_schema_ref: op.input_schema_ref.clone(),
            output_schema_ref: op.output_schema_ref.clone(),
        })
        .collect()
}

fn module_op_summary(ops: &[grapheme_runtime::ExportedOp]) -> ModuleOpSummary {
    let total_ops = ops.len();
    let input_schema_refs = ops
        .iter()
        .filter(|op| op.input_schema_ref.is_some())
        .count();
    let output_schema_refs = ops
        .iter()
        .filter(|op| op.output_schema_ref.is_some())
        .count();
    let typed_ops = ops
        .iter()
        .filter(|op| op.input_schema_ref.is_some() || op.output_schema_ref.is_some())
        .count();

    ModuleOpSummary {
        total_ops,
        typed_ops,
        untyped_ops: total_ops.saturating_sub(typed_ops),
        input_schema_refs,
        output_schema_refs,
    }
}

fn grouped_module_ops(ops: &[grapheme_runtime::ExportedOp]) -> Vec<EffectGroup> {
    let mut groups: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();

    for op in ops {
        groups
            .entry(effect_name(&op.effect))
            .or_default()
            .push(op.op.clone());
    }

    groups
        .into_iter()
        .map(|(effect, ops)| EffectGroup {
            effect: effect.to_string(),
            ops,
        })
        .collect()
}

/// Build `modules info` payload for a module id.
pub fn modules_info_payload(module_id: &str) -> Option<JsonValue> {
    let payload = modules_info_contract(module_id)?;
    serde_json::to_value(payload).ok()
}

/// Build typed `modules info` contract payload.
pub fn modules_info_contract(module_id: &str) -> Option<ModuleInfoPayload> {
    let manifest = module_manifest_by_id(module_id)?;
    let op_summary = module_op_summary(&manifest.exported_ops);
    let module_id = manifest.module_id.clone();
    let payload = ModuleInfoPayload {
        module_id: module_id.clone(),
        version: manifest.version,
        abi: manifest.abi,
        entrypoint: manifest.entrypoint,
        required_capabilities: manifest.required_capabilities,
        limits: manifest.limits,
        op_summary,
        exported_ops_by_effect: grouped_module_ops(&manifest.exported_ops),
        exported_ops: compact_module_ops(&module_id, &manifest.exported_ops),
    };

    Some(payload)
}

/// Build `modules types` payload for a module id.
pub fn modules_types_payload(module_id: &str) -> Option<JsonValue> {
    let payload = modules_types_contract(module_id)?;
    serde_json::to_value(payload).ok()
}

/// Build typed `modules types` contract payload.
pub fn modules_types_contract(module_id: &str) -> Option<ModuleTypesPayload> {
    let manifest = module_manifest_by_id(module_id)?;
    let module_id = manifest.module_id.clone();
    let payload = ModuleTypesPayload {
        module_id: module_id.clone(),
        type_summary: module_op_summary(&manifest.exported_ops),
        types_by_effect: grouped_module_ops(&manifest.exported_ops),
        types: compact_module_ops(&module_id, &manifest.exported_ops),
    };

    Some(payload)
}

/// Build `modules examples` payload for a module id.
pub fn modules_examples_payload(module_id: &str) -> Option<JsonValue> {
    let normalized = module_id.to_lowercase();
    let examples = curated_examples_for_module(&normalized);
    if examples.is_empty() {
        return None;
    }

    Some(serde_json::json!({
        "module_id": normalized,
        "examples": examples,
    }))
}

fn module_search_guidance(module_id: &str) -> ModuleSearchGuidance {
    match module_id.to_lowercase().as_str() {
        "core" => ModuleSearchGuidance {
            summary: "General-purpose transforms, branching helpers, and state shaping.",
            use_when: "You need data reshaping, list operations, path access, or debug helpers.",
            avoid_when: "You need external network/database side effects.",
        },
        "web" | "websearch" => ModuleSearchGuidance {
            summary: "Search and research primitives over web providers.",
            use_when: "You need source discovery, provider routing, or report/material generation.",
            avoid_when: "You already have trusted local content and do not need web fetch/search.",
        },
        "http" | "tcp" | "smtp" | "email" => ModuleSearchGuidance {
            summary: "Network side-effect modules for transport and external I/O.",
            use_when: "You need outbound calls, socket interactions, or email delivery.",
            avoid_when: "You can complete the workflow with local transforms only.",
        },
        "sql" | "surreal" => ModuleSearchGuidance {
            summary: "Database capability modules for read/write and transactional patterns.",
            use_when: "You need persistent state queries and durable updates.",
            avoid_when: "You only need ephemeral in-memory state.",
        },
        "data" => ModuleSearchGuidance {
            summary: "Polars-backed CSV ingest and JSON frame analytics pipeline.",
            use_when: "You need tabular reads, filtering, grouping, or schema introspection on local CSV files.",
            avoid_when: "You only need lightweight string CSV parsing (use csv.to_list) or remote fetch.",
        },
        "pdf" | "image" | "plot" | "media" => ModuleSearchGuidance {
            summary: "Heavy capability modules (PDF, image, plot, media) — Wasm or native; opt-in in SDK, full in CLI.",
            use_when: "You need document generation, image transforms, charts, or ffmpeg media ops.",
            avoid_when: "You can stay with core/json/csv transforms or defer until implementations are enabled.",
        },
        "memory" => ModuleSearchGuidance {
            summary: "In-memory storage/roundtrip examples and lightweight persistence patterns.",
            use_when: "You need temporary memory interactions within bounded runtime scope.",
            avoid_when: "You need durable relational/document persistence.",
        },
        "io" | "docs" | "json" | "csv" | "yaml" | "html" => ModuleSearchGuidance {
            summary: "Document and content transformation helpers.",
            use_when: "You need file/text conversion or structured format transforms.",
            avoid_when: "You need network search or database transactions.",
        },
        "secrets" => ModuleSearchGuidance {
            summary: "Secret handling and signing-oriented capability flows.",
            use_when: "You need governed secret retrieval or signing operations.",
            avoid_when: "Your workflow does not require secret material.",
        },
        _ => ModuleSearchGuidance {
            summary: "Runtime module capability surface.",
            use_when: "You need operation-level capabilities for workflow execution.",
            avoid_when: "No matching capabilities are needed for your task.",
        },
    }
}

fn compute_module_relevance_score(
    query_lower: &str,
    module_id: &str,
    matching_ops: &[String],
    module_match: bool,
) -> f64 {
    let mut score = 0.0;

    if module_id.eq_ignore_ascii_case(query_lower) {
        score += 100.0;
    } else if module_match {
        score += 55.0;
    }

    score += matching_ops.len() as f64 * 7.5;

    if matching_ops
        .iter()
        .any(|op| op.eq_ignore_ascii_case(query_lower))
    {
        score += 35.0;
    }

    // Prefer stable operations during ranking so selection defaults to mature surfaces.
    for op in matching_ops {
        match op_stability(module_id, op) {
            grapheme_signatures::SignatureStability::Stable => score += 2.0,
            grapheme_signatures::SignatureStability::Experimental => score -= 2.0,
            grapheme_signatures::SignatureStability::Deprecated => score -= 4.0,
        }
    }

    score
}

fn example_catalog_row(entry: &ExampleCatalogEntry) -> ExampleDiscoveryItem {
    let mut run = format!("grapheme run {}", entry.path);
    if entry.requires_native_modules {
        run.push_str(" --native-modules");
    }

    ExampleDiscoveryItem {
        name: entry.name.to_string(),
        path: entry.path.to_string(),
        summary: entry.summary.to_string(),
        use_when: entry.use_when.to_string(),
        complexity: entry.complexity.to_string(),
        tags: entry.tags.iter().map(|t| (*t).to_string()).collect(),
        requires_native_modules: entry.requires_native_modules,
        run,
    }
}

/// Discover examples with optional filtering by query/tag/complexity/native requirement.
pub fn discover_examples(
    query: Option<&str>,
    tag: Option<&str>,
    complexity: Option<&str>,
    native_only: bool,
) -> Vec<ExampleDiscoveryItem> {
    let query = query.map(|q| q.to_lowercase());
    let tag = tag.map(|t| t.to_lowercase());
    let complexity = complexity.map(|c| c.to_lowercase());

    EXAMPLE_CATALOG
        .iter()
        .map(example_catalog_row)
        .filter(|item| {
            if native_only && !item.requires_native_modules {
                return false;
            }

            if let Some(ref c) = complexity {
                if item.complexity.to_lowercase() != *c {
                    return false;
                }
            }

            if let Some(ref t) = tag {
                if !item.tags.iter().any(|entry| entry.eq_ignore_ascii_case(t)) {
                    return false;
                }
            }

            if let Some(ref q) = query {
                let tags = item.tags.join(" ").to_lowercase();
                if !(item.name.to_lowercase().contains(q)
                    || item.summary.to_lowercase().contains(q)
                    || item.use_when.to_lowercase().contains(q)
                    || tags.contains(q))
                {
                    return false;
                }
            }

            true
        })
        .collect()
}

/// Get one example discovery row by stable name.
pub fn example_by_name(name: &str) -> Option<ExampleDiscoveryItem> {
    EXAMPLE_CATALOG
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(name))
        .map(example_catalog_row)
}

struct StdlibHost {
    capability_observer: Option<CapabilityObserver>,
    capability_interceptor: Option<CapabilityInterceptor>,
}

impl StdlibHost {
    fn resolve_module(call: &CapabilityCall) -> String {
        call.module
            .as_deref()
            .map(|m| m.to_lowercase())
            .or_else(|| call.capability.split('.').next().map(|m| m.to_lowercase()))
            .unwrap_or_default()
    }
}

impl CapabilityHost for StdlibHost {
    fn call(&mut self, call: &CapabilityCall) -> Result<JsonValue, HostCallError> {
        if let Some(observer) = &self.capability_observer {
            observer(call);
        }

        if let Some(interceptor) = &self.capability_interceptor {
            if let Some(outcome) = interceptor(call) {
                return outcome;
            }
        }

        let module = Self::resolve_module(call);
        if let Some(out) = grapheme_stdlib::registry::dispatch(&module, &call.op, &call.args) {
            return Ok(out);
        }

        Err(HostCallError::Fatal(format!(
            "unsupported capability dispatch: module='{}' op='{}'",
            module, call.op
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grapheme_artifact::Capability;
    use grapheme_artifact::ExecutionOutcome;
    use grapheme_runtime::{CompatibilityMode, ModuleAbi, ModuleLifecycleEventKind, PolicyGuard};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_temp_wasm(tag: &str, bytes: &[u8]) -> PathBuf {
        let mut path = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        path.push(format!("grapheme-sdk-{tag}-{ts}.wasm"));
        fs::write(&path, bytes).expect("write temp wasm bytes");
        path
    }

    #[test]
    fn modules_search_payload_concise_returns_ranked_matches() {
        let payload = modules_search_payload(
            "web",
            &ModuleSearchOptions {
                explain: true,
                detail: ModuleSearchDetail::Concise,
                top: Some(1),
                min_score: Some(100.0),
                include_experimental: false,
            },
        );

        assert_eq!(payload.get("count").and_then(|v| v.as_u64()), Some(1));
        let first = payload
            .get("matches")
            .and_then(|v| v.as_array())
            .and_then(|items| items.first())
            .expect("first ranked match");
        assert_eq!(first.get("module_id").and_then(|v| v.as_str()), Some("web"));
    }

    #[test]
    fn modules_search_payload_default_includes_compact_module_metadata() {
        let payload = modules_search_payload("html", &ModuleSearchOptions::default());

        assert_eq!(payload.get("query").and_then(|v| v.as_str()), Some("html"));
        assert_eq!(payload.get("count").and_then(|v| v.as_u64()), Some(1));

        let first = payload
            .get("matches")
            .and_then(|v| v.as_array())
            .and_then(|items| items.first())
            .expect("first compact match");

        assert_eq!(
            first.get("module_id").and_then(|v| v.as_str()),
            Some("html")
        );
        assert!(first
            .get("summary")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty()));
        assert!(first.get("op_count").and_then(|v| v.as_u64()).is_some());
        assert!(first
            .get("effects")
            .and_then(|v| v.as_array())
            .is_some_and(|effects| !effects.is_empty()));
        assert_eq!(
            payload.get("stability_policy").and_then(|v| v.as_str()),
            Some("stable_preferred")
        );
    }

    #[test]
    fn modules_search_payload_experimental_ops_are_opt_in() {
        let default_payload = modules_search_payload("xaviv", &ModuleSearchOptions::default());
        assert_eq!(
            default_payload.get("count").and_then(|v| v.as_u64()),
            Some(0)
        );

        let include_payload = modules_search_payload(
            "xaviv",
            &ModuleSearchOptions {
                include_experimental: true,
                ..ModuleSearchOptions::default()
            },
        );

        assert_eq!(
            include_payload.get("count").and_then(|v| v.as_u64()),
            Some(1)
        );
        assert_eq!(
            include_payload
                .get("stability_policy")
                .and_then(|v| v.as_str()),
            Some("all")
        );
    }

    #[test]
    fn modules_ops_payload_includes_core_ops_for_core_query() {
        let payload = modules_ops_payload("core");
        let matches = payload
            .get("matches")
            .and_then(|v| v.as_array())
            .expect("matches array");

        assert!(matches
            .iter()
            .any(|row| { row.get("module_id").and_then(|v| v.as_str()) == Some("core") }));
        assert!(matches
            .iter()
            .all(|row| { row.get("stability").and_then(|v| v.as_str()) == Some("stable") }));
        assert!(matches.iter().any(|row| {
            row.get("op").and_then(|v| v.as_str()) == Some("echo")
                && row.get("input_object_type").is_some()
                && row.get("output_type").and_then(|v| v.as_str()) == Some("object")
                && row
                    .get("args")
                    .and_then(|v| v.as_array())
                    .is_some_and(|args| {
                        args.iter().any(|arg| {
                            arg.get("name").and_then(|v| v.as_str()) == Some("message")
                                && arg.get("ty").and_then(|v| v.as_str()) == Some("string")
                                && arg.get("required").and_then(|v| v.as_bool()) == Some(false)
                        })
                    })
        }));
    }

    #[test]
    fn modules_info_payload_groups_ops_and_compacts_null_schema_refs() {
        let payload = modules_info_payload("web").expect("web module payload");

        assert_eq!(
            payload.get("module_id").and_then(|v| v.as_str()),
            Some("web")
        );
        assert_eq!(
            payload
                .get("op_summary")
                .and_then(|v| v.get("total_ops"))
                .and_then(|v| v.as_u64()),
            Some(5)
        );

        let groups = payload
            .get("exported_ops_by_effect")
            .and_then(|v| v.as_array())
            .expect("effect groups");
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0].get("effect").and_then(|v| v.as_str()),
            Some("control")
        );
        assert_eq!(
            groups[1].get("effect").and_then(|v| v.as_str()),
            Some("network")
        );

        let exported_ops = payload
            .get("exported_ops")
            .and_then(|v| v.as_array())
            .expect("exported ops");
        let first = exported_ops.first().expect("first exported op");
        assert_eq!(first.get("op").and_then(|v| v.as_str()), Some("duckduckgo"));
        assert_eq!(
            first.get("stability").and_then(|v| v.as_str()),
            Some("stable")
        );
        assert_eq!(
            first.get("output_type").and_then(|v| v.as_str()),
            Some("object")
        );
        assert_eq!(
            first
                .get("output_object_type")
                .and_then(|v| v.get("properties"))
                .and_then(|v| v.get("results"))
                .and_then(|v| v.get("ty"))
                .and_then(|v| v.as_str()),
            Some("array")
        );
        assert_eq!(
            first
                .get("input_object_type")
                .and_then(|v| v.get("properties"))
                .and_then(|v| v.get("query"))
                .and_then(|v| v.get("ty"))
                .and_then(|v| v.as_str()),
            Some("string")
        );
        assert_eq!(
            first
                .get("args")
                .and_then(|v| v.as_array())
                .and_then(|args| args.first())
                .and_then(|arg| arg.get("name"))
                .and_then(|v| v.as_str()),
            Some("query")
        );
        assert!(first.get("input_schema_ref").is_none());
        assert!(first.get("output_schema_ref").is_none());
    }

    #[test]
    fn modules_types_payload_reports_type_summary_and_compacts_null_schema_refs() {
        let payload = modules_types_payload("web").expect("web module payload");

        assert_eq!(
            payload.get("module_id").and_then(|v| v.as_str()),
            Some("web")
        );
        assert_eq!(
            payload
                .get("type_summary")
                .and_then(|v| v.get("typed_ops"))
                .and_then(|v| v.as_u64()),
            Some(0)
        );

        let types = payload
            .get("types")
            .and_then(|v| v.as_array())
            .expect("types array");
        assert_eq!(types.len(), 5);
        assert!(types
            .iter()
            .all(|row| row.get("output_type").and_then(|v| v.as_str()) == Some("object")));
        let duckduckgo = types
            .iter()
            .find(|row| row.get("op").and_then(|v| v.as_str()) == Some("duckduckgo"))
            .expect("duckduckgo op row");
        assert!(duckduckgo.get("input_object_type").is_some());
        assert!(duckduckgo
            .get("args")
            .and_then(|v| v.as_array())
            .is_some_and(|args| !args.is_empty()));
        assert_eq!(
            duckduckgo.get("stability").and_then(|v| v.as_str()),
            Some("stable")
        );

        let providers = types
            .iter()
            .find(|row| row.get("op").and_then(|v| v.as_str()) == Some("providers"))
            .expect("providers op row");
        assert!(providers.get("input_object_type").is_none());
        assert!(providers.get("args").is_none());
        assert_eq!(
            providers
                .get("output_object_type")
                .and_then(|v| v.get("properties"))
                .and_then(|v| v.get("providers"))
                .and_then(|v| v.get("ty"))
                .and_then(|v| v.as_str()),
            Some("array")
        );
        assert!(types
            .iter()
            .all(|row| row.get("input_schema_ref").is_none()));
        assert!(types
            .iter()
            .all(|row| row.get("output_schema_ref").is_none()));
    }

    #[test]
    fn modules_info_contract_serializes_with_payload_parity() {
        let typed = modules_info_contract("web").expect("typed contract");
        let typed_json = serde_json::to_value(typed).expect("serialize typed info contract");
        let payload_json = modules_info_payload("web").expect("payload json");
        assert_eq!(typed_json, payload_json);
    }

    #[test]
    fn modules_types_contract_serializes_with_payload_parity() {
        let typed = modules_types_contract("web").expect("typed contract");
        let typed_json = serde_json::to_value(typed).expect("serialize typed types contract");
        let payload_json = modules_types_payload("web").expect("payload json");
        assert_eq!(typed_json, payload_json);
    }

    #[test]
    fn modules_ops_contract_serializes_with_payload_parity() {
        let typed = modules_ops_contract("core");
        let typed_json = serde_json::to_value(typed).expect("serialize typed ops contract");
        let payload_json = modules_ops_payload("core");
        assert_eq!(typed_json, payload_json);
    }

    #[test]
    fn modules_search_contract_serializes_with_payload_parity() {
        let options = ModuleSearchOptions {
            explain: true,
            detail: ModuleSearchDetail::Full,
            top: Some(2),
            min_score: Some(50.0),
            include_experimental: false,
        };
        let typed = modules_search_contract("web", &options);
        let typed_json = serde_json::to_value(typed).expect("serialize typed search contract");
        let payload_json = modules_search_payload("web", &options);
        assert_eq!(typed_json, payload_json);
    }

    #[test]
    fn executable_reflection_contract_serialization_is_stable() {
        let row = ExecutableReflection {
            name: "Heartbeat".to_string(),
            kind: ExecutableReflectionKind::Subscription,
            description: Some("Heartbeat status stream".to_string()),
            input_type: Some("HeartbeatIn".to_string()),
            output_type: Some("HeartbeatOut".to_string()),
            loop_directive_count: 1,
            recursive_directive_count: 0,
            retry_directive_count: 1,
            timeout_directive_count: 1,
            pipeline_count: 2,
            step_count: 6,
        };

        let value = serde_json::to_value(row).expect("serialize executable reflection");
        assert_eq!(
            value.get("name").and_then(|v| v.as_str()),
            Some("Heartbeat")
        );
        assert_eq!(
            value.get("kind").and_then(|v| v.as_str()),
            Some("subscription")
        );
        assert_eq!(
            value.get("description").and_then(|v| v.as_str()),
            Some("Heartbeat status stream")
        );
        assert_eq!(
            value.get("pipeline_count").and_then(|v| v.as_u64()),
            Some(2)
        );
        assert_eq!(value.get("step_count").and_then(|v| v.as_u64()), Some(6));
    }

    #[test]
    fn executables_reflection_from_source_reports_query_and_iterator_metadata() {
        let source = r#"
query Hello on String -> String @intent(goal: "Say hello") {
    core.echo(message: "hello") {
        state { current }
    }
}

iterator Normalize on Json -> Json {
    core.echo(message: "normalize") {
        state { current }
    }
}
"#;

        let rows = executables_reflection_from_source(source).expect("source reflection");
        assert_eq!(rows.len(), 2);

        let hello = rows
            .iter()
            .find(|r| r.name == "Hello")
            .expect("Hello reflection row");
        assert_eq!(hello.kind, ExecutableReflectionKind::Query);
        assert_eq!(hello.description.as_deref(), Some("Say hello"));
        assert_eq!(hello.input_type.as_deref(), Some("String"));
        assert_eq!(hello.output_type.as_deref(), Some("String"));
        assert_eq!(hello.pipeline_count, 1);
        assert_eq!(hello.step_count, 1);

        let normalize = rows
            .iter()
            .find(|r| r.name == "Normalize")
            .expect("Normalize reflection row");
        assert_eq!(normalize.kind, ExecutableReflectionKind::Iterator);
        assert_eq!(normalize.description, None);
        assert_eq!(normalize.input_type.as_deref(), Some("Json"));
        assert_eq!(normalize.output_type.as_deref(), Some("Json"));
    }

    #[test]
    fn executables_reflection_contract_from_source_matches_payload() {
        let source = r#"
query Hello on String -> String {
    core.echo(message: "hello") {
        state { current }
    }
}
"#;

        let contract =
            executables_reflection_contract_from_source(source).expect("source contract payload");
        let contract_json = serde_json::to_value(contract).expect("serialize source contract");
        let payload_json =
            executables_reflection_payload_from_source(source).expect("source payload json");
        assert_eq!(contract_json, payload_json);
    }

    #[test]
    fn executables_reflection_from_artifact_reports_runtime_shape() {
        let source = r#"
query Hello @intent(goal: "Artifact hello") {
    core.echo(message: "hello") {
        state { current }
    }
}
"#;

        let artifact = grapheme_compiler::compile_to_artifact(source, Some("Hello"))
            .expect("compile artifact");
        let rows = executables_reflection_from_artifact(&artifact);
        assert_eq!(rows.len(), 1);
        let row = rows.first().expect("artifact row");
        assert_eq!(row.name, "Hello");
        assert_eq!(row.kind, ExecutableReflectionKind::Query);
        assert_eq!(row.description.as_deref(), Some("Artifact hello"));
        assert_eq!(row.pipeline_count, 1);
        assert_eq!(row.step_count, 1);
    }

    #[test]
    fn executables_reflection_contract_from_artifact_matches_payload() {
        let source = r#"
query Hello {
    core.echo(message: "hello") {
        state { current }
    }
}
"#;

        let artifact = grapheme_compiler::compile_to_artifact(source, Some("Hello"))
            .expect("compile artifact");
        let contract = executables_reflection_contract_from_artifact(&artifact);
        let contract_json = serde_json::to_value(contract).expect("serialize artifact contract");
        let payload_json = executables_reflection_payload_from_artifact(&artifact);
        assert_eq!(contract_json, payload_json);
    }

    #[test]
    fn modules_examples_payload_returns_none_for_unknown_module() {
        assert!(modules_examples_payload("unknown").is_none());
    }

    #[test]
    fn discover_examples_filters_query_tag_and_complexity() {
        let matches = discover_examples(Some("fallback"), Some("routing"), Some("advanced"), false);

        assert!(matches.iter().any(|row| row.name == "web-provider-routing"));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn example_by_name_returns_expected_run_hint() {
        let row = example_by_name("main").expect("main example exists");
        assert_eq!(row.path, "examples/main.gr");
        assert_eq!(row.run, "grapheme run examples/main.gr");
    }

    #[test]
    fn execute_source_runs_core_echo() {
        let source = r#"import core from "grapheme/core"

query Hello {
    core.echo(message: "hello from sdk") {
    state { current }
  }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let result = engine
            .execute_source(source)
            .expect("execution should succeed");

        assert_eq!(
            result
                .final_state
                .get("current")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str()),
            Some("hello from sdk")
        );
    }

    #[test]
    fn format_result_supports_yaml_and_json() {
        let source = r#"import core from "grapheme/core"

query Hello {
    core.echo(message: "hello") {
    state { current }
  }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let result = engine
            .execute_source(source)
            .expect("execution should succeed");

        let yaml = engine
            .format_result(&result, StructuredMode::Yaml)
            .expect("yaml format should succeed");
        let json = engine
            .format_result(&result, StructuredMode::Json)
            .expect("json format should succeed");

        assert!(yaml.contains("artifact_id:"));
        assert!(json.contains("\"artifact_id\""));
    }

    #[test]
    fn execute_artifact_preserves_lint_warnings_from_compilation_path() {
        let source = r#"import core from "grapheme/core"

query Q {
    core.echo(message: "hi") {
    state { current }
  }
}
"#;

        let compiled = Compiler::compile_source(source, CompilerOptions::default())
            .expect("compile should succeed");
        let engine = GraphemeEngine::builder().build();
        let result = engine
            .execute_compiled(&compiled)
            .expect("compiled execution should succeed");

        assert_eq!(
            result.lint_warnings.len(),
            compiled.compilation.lint_warnings.len()
        );
    }

    #[test]
    fn capability_observer_receives_host_calls() {
        let source = r#"import core from "grapheme/core"

query Hello {
    core.echo(message: "observe") {
    state { current }
  }
}
"#;

        let observed = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_ref = Arc::clone(&observed);

        let engine = GraphemeEngine::builder()
            .with_capability_observer(move |call| {
                observed_ref.lock().expect("lock observer").push(format!(
                    "{}.{}",
                    call.module.clone().unwrap_or_default(),
                    call.op
                ));
            })
            .build();

        let result = engine
            .execute_source(source)
            .expect("execution should succeed");
        assert_eq!(
            result
                .final_state
                .get("current")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str()),
            Some("observe")
        );

        let calls = observed.lock().expect("lock observer snapshot");
        assert!(calls.iter().any(|v| v.ends_with(".echo")));
    }

    #[test]
    fn capability_interceptor_can_override_stdlib_dispatch() {
        let source = r#"import core from "grapheme/core"

query Hello {
    core.echo(message: "original") {
    state { current }
  }
}
"#;

        let engine = GraphemeEngine::builder()
            .with_capability_interceptor(|call| {
                if call.op == "echo" {
                    return Some(Ok(serde_json::json!({"message": "intercepted"})));
                }
                None
            })
            .build();

        let result = engine
            .execute_source(source)
            .expect("execution should succeed");
        assert_eq!(
            result
                .final_state
                .get("current")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str()),
            Some("intercepted")
        );
    }

    #[test]
    fn runtime_session_hotmodule_activate_and_rollback_updates_lifecycle() {
        let wasm_a = write_temp_wasm("session-hot-a", b"sdk-hot-a");
        let wasm_b = write_temp_wasm("session-hot-b", b"sdk-hot-b");

        let engine = GraphemeEngine::builder().build();
        let mut session = engine.runtime_session();

        let first = session
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let second = session
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.1".to_string()),
            })
            .expect("second activation should succeed");

        assert_ne!(first.generation_id, second.generation_id);

        let rolled_back = session
            .rollback_module_generation("http")
            .expect("rollback should succeed");
        assert_eq!(rolled_back.generation_id, first.generation_id);

        let events = session.module_lifecycle_events();
        assert!(events
            .iter()
            .any(|e| e.kind == ModuleLifecycleEventKind::Activated));
        assert!(events
            .iter()
            .any(|e| e.kind == ModuleLifecycleEventKind::Rollback));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn runtime_session_execute_source_preserves_engine_behavior() {
        let source = r#"import core from "grapheme/core"

query Hello {
    core.echo(message: "hello from session") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let session = engine.runtime_session();
        let result = session
            .execute_source(source)
            .expect("session execution should succeed");

        assert_eq!(
            result
                .final_state
                .get("current")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str()),
            Some("hello from session")
        );
    }

    #[test]
    fn runtime_session_execution_payloads_reflect_activation_window_boundaries() {
        let wasm_a = write_temp_wasm("session-pin-a", b"sdk-pin-a");
        let wasm_b = write_temp_wasm("session-pin-b", b"sdk-pin-b");

        let engine = GraphemeEngine::builder().build();
        let mut session = engine.runtime_session();

        let first = session
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let source = r#"import core from "grapheme/core"

query Hello {
    core.echo(message: "window-a") {
        state { current }
    }
}
"#;

        let execution_a = session
            .execute_source(source)
            .expect("execution A should succeed");

        let second = session
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.1.0".to_string()),
            })
            .expect("second activation should succeed");

        let execution_b = session
            .execute_source(source)
            .expect("execution B should succeed");

        let gen_ids_a = execution_a
            .final_state
            .get("runtime_events")
            .and_then(|v| v.as_array())
            .map(|events| {
                events
                    .iter()
                    .filter_map(|event| event.get("generation_id").and_then(|v| v.as_u64()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let gen_ids_b = execution_b
            .final_state
            .get("runtime_events")
            .and_then(|v| v.as_array())
            .map(|events| {
                events
                    .iter()
                    .filter_map(|event| event.get("generation_id").and_then(|v| v.as_u64()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        assert!(gen_ids_a.contains(&first.generation_id));
        assert!(!gen_ids_a.contains(&second.generation_id));
        assert!(gen_ids_b.contains(&second.generation_id));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn runtime_session_failed_activation_does_not_replace_active_generation() {
        let wasm_a = write_temp_wasm("session-fail-a", b"sdk-fail-a");
        let wasm_b = write_temp_wasm("session-fail-b", b"sdk-fail-b");

        let engine = GraphemeEngine::builder().build();
        let mut session = engine.runtime_session();

        let first = session
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let err = session
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::MirV1,
                version: Some("2.0.0".to_string()),
            })
            .expect_err("incompatible abi activation should fail");

        assert!(err.to_string().contains("incompatible ABI update"));

        let rollback_err = session
            .rollback_module_generation("http")
            .expect_err("failed activation should not create rollback candidate");
        assert!(rollback_err
            .to_string()
            .contains("has no prior generation to roll back to"));

        let events = session.module_lifecycle_events();
        assert!(events.iter().any(|e| {
            e.kind == ModuleLifecycleEventKind::Activated && e.generation_id == first.generation_id
        }));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn runtime_session_activation_denied_by_capability_policy() {
        let wasm = write_temp_wasm("session-policy-denied", b"sdk-policy-denied");

        let mut policy = CapabilityPolicy::default();
        policy
            .denied
            .push(Capability("http.get.allowed_domain".to_string()));

        let engine = GraphemeEngine::builder()
            .with_capability_policy(policy)
            .build();
        let mut session = engine.runtime_session();

        let err = session
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect_err("activation should be denied by capability policy");

        assert!(err
            .to_string()
            .contains("activation denied by capability policy"));

        let _ = fs::remove_file(wasm);
    }

    #[test]
    fn runtime_session_lifecycle_event_contract_has_required_fields() {
        let wasm_a = write_temp_wasm("session-events-a", b"sdk-events-a");
        let wasm_b = write_temp_wasm("session-events-b", b"sdk-events-b");

        let engine = GraphemeEngine::builder().build();
        let mut session = engine.runtime_session();

        session
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let _ = session
            .activate_module_generation(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::MirV1,
                version: Some("2.0.0".to_string()),
            })
            .expect_err("incompatible activation should fail");

        let events = session.module_lifecycle_events();
        assert!(!events.is_empty());
        assert!(events.iter().all(|e| {
            !e.module_id.is_empty()
                && e.generation_id > 0
                && !e.version.is_empty()
                && !e.content_hash.is_empty()
        }));
        assert!(events
            .iter()
            .any(|e| e.kind == ModuleLifecycleEventKind::ActivationFailed));
        assert!(events
            .iter()
            .any(|e| e.kind == ModuleLifecycleEventKind::ActivationFailed
                && e.reason.as_deref() == Some("abi_incompatible")));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn sql_runtime_flow_denied_by_policy_when_not_allowlisted() {
        let source = r#"import sql from "grapheme/sql"

query SqlDenied {
    sql.query(connection: "sqlite::memory:", sql: "select 1 as ok") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let result = engine
            .execute_source(source)
            .expect("execution should complete");

        assert!(matches!(
            result.execution.outcome,
            ExecutionOutcome::FatalFailure
        ));
        assert!(result
            .execution
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("sql module is disabled"));
    }

    #[test]
    fn sql_runtime_flow_succeeds_when_connection_allowlisted() {
        let source = r#"import sql from "grapheme/sql"

query SqlAllowed {
    sql.query(connection: "sqlite::memory:", sql: "select 1 as ok") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder()
            .with_policy_guard(PolicyGuard {
                allowed_sql_connections: vec!["sqlite::memory:".to_string()],
                ..PolicyGuard::default()
            })
            .build();
        let result = engine
            .execute_source(source)
            .expect("execution should complete");

        assert!(matches!(
            result.execution.outcome,
            ExecutionOutcome::Succeeded
        ));
        assert_eq!(
            result
                .final_state
                .get("current")
                .and_then(|v| v.get("ok"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn surreal_runtime_flow_denied_by_policy_when_not_allowlisted() {
        let source = r#"import surreal from "grapheme/surreal"

query SurrealDenied {
    surreal.query(connection: "local", query: "return true;") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let result = engine
            .execute_source(source)
            .expect("execution should complete");

        assert!(matches!(
            result.execution.outcome,
            ExecutionOutcome::FatalFailure
        ));
        assert!(result
            .execution
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("surreal module is disabled"));
    }

    #[test]
    fn surreal_runtime_flow_reaches_module_when_connection_allowlisted() {
        let source = r#"import surreal from "grapheme/surreal"

query SurrealAllowed {
    surreal.query(connection: "local", query: "return true;") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder()
            .with_policy_guard(PolicyGuard {
                allowed_surreal_connections: vec!["local".to_string()],
                ..PolicyGuard::default()
            })
            .build();
        let result = engine
            .execute_source(source)
            .expect("execution should complete");

        assert!(matches!(
            result.execution.outcome,
            ExecutionOutcome::Succeeded
        ));
        assert_eq!(
            result
                .final_state
                .get("current")
                .and_then(|v| v.get("error"))
                .and_then(|v| v.get("code"))
                .and_then(|v| v.as_str()),
            Some("surreal_connection_unresolved")
        );
    }

    #[test]
    fn execute_aot_matches_base_artifact_execution_parity() {
        let source = r#"import core from "grapheme/core"

query HelloAotParity {
    core.echo(message: "hello-aot-parity") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let compiled = Compiler::compile_source_to_aot(source, CompilerOptions::default())
            .expect("compile to aot should succeed");

        let interpreted = engine
            .execute_artifact(&compiled.artifact)
            .expect("interpreted execution should succeed");
        let staged = engine
            .execute_aot(&compiled.aot)
            .expect("aot-backed execution should succeed");

        assert!(matches!(
            (&interpreted.execution.outcome, &staged.execution.outcome),
            (ExecutionOutcome::Succeeded, ExecutionOutcome::Succeeded)
                | (
                    ExecutionOutcome::RetryableFailure,
                    ExecutionOutcome::RetryableFailure
                )
                | (
                    ExecutionOutcome::FatalFailure,
                    ExecutionOutcome::FatalFailure
                )
        ));
        assert_eq!(interpreted.final_state, staged.final_state);
    }

    #[test]
    fn format_aot_supports_yaml_and_json() {
        let source = r#"import core from "grapheme/core"

query HelloAot {
    core.echo(message: "hello-aot") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let aot = engine
            .compile_source_to_aot(source)
            .expect("compile source to aot should succeed");

        let yaml = engine
            .format_aot(&aot, StructuredMode::Yaml)
            .expect("yaml aot formatting should succeed");
        let json = engine
            .format_aot(&aot, StructuredMode::Json)
            .expect("json aot formatting should succeed");

        assert!(yaml.contains("stage: stage_a"));
        assert!(json.contains("\"stage\": \"stage_a\""));
    }

    #[test]
    fn compile_source_to_aot_stage_b_emits_container_metadata() {
        let source = r#"import core from "grapheme/core"

query HelloAot {
    core.echo(message: "hello-aot") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let imports = vec![
            "grapheme.runtime.host.v1::state.read".to_string(),
            "grapheme.runtime.host.v1::state.write".to_string(),
        ];
        let aot = engine
            .compile_source_to_aot_stage_b(source, b"\0asmstageb", &imports)
            .expect("compile source to stage_b aot should succeed");

        assert!(matches!(aot.stage, grapheme_artifact::AotStage::StageB));
        assert_eq!(aot.payload.format, "grapheme.aot.stage_b.v1");
        assert!(aot.payload.workflow_wasm.is_some());
    }

    #[test]
    fn execute_aot_rejects_stage_b_outside_host_boundary() {
        let source = r#"import core from "grapheme/core"

query HelloAot {
    core.echo(message: "hello-aot") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let mut stage_a = engine
            .compile_source_to_aot(source)
            .expect("compile source to aot should succeed");

        stage_a.stage = grapheme_artifact::AotStage::StageB;
        stage_a.payload.format = "grapheme.aot.stage_b.v1".to_string();
        stage_a.payload.workflow_wasm = Some(grapheme_artifact::AotWorkflowWasmContainer {
            byte_len: 8,
            sha256: "sha256:deadbeef".to_string(),
            entry_export: "_start".to_string(),
            allowed_imports: vec!["wasi_snapshot_preview1::fd_write".to_string()],
            inline_wasm_hex: None,
        });

        let err = engine
            .execute_aot(&stage_a)
            .expect_err("stage_b boundary escape should be rejected");

        assert!(matches!(err, GraphemeSdkError::Contract(_)));
        assert!(err.to_string().contains("outside host interface boundary"));
    }

    #[test]
    fn execute_aot_stage_b_routes_through_runtime_stage_b_path() {
        let source = r#"import core from "grapheme/core"

query HelloAot {
    core.echo(message: "hello-aot") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder().build();
        let imports = vec![
            "grapheme.runtime.host.v1::state.read".to_string(),
            "grapheme.runtime.host.v1::state.write".to_string(),
        ];
        let stage_b = engine
            .compile_source_to_aot_stage_b(source, b"\0asmstageb", &imports)
            .expect("compile source to stage_b should succeed");

        let result = engine
            .execute_aot(&stage_b)
            .expect("stage_b execution should succeed");

        assert!(
            result
                .execution
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("stage_b scaffold executed via parity path")
                || result
                    .execution
                    .message
                    .as_deref()
                    .unwrap_or_default()
                    .contains("stage_b container executed directly via wasix backend")
        );

        let stage_b_event_found = result
            .final_state
            .get("runtime_events")
            .and_then(|events| events.as_array())
            .map(|events| {
                events.iter().any(|event| {
                    event.get("kind").and_then(|v| v.as_str())
                        == Some("aot.stage_b.container_routed")
                })
            })
            .unwrap_or(false);

        assert!(stage_b_event_found);
    }

    #[cfg(not(feature = "wasix-runtime"))]
    #[test]
    fn execute_aot_stage_b_strict_mode_rejects_when_container_runtime_unavailable() {
        let source = r#"import core from "grapheme/core"

query HelloAot {
    core.echo(message: "hello-aot") {
        state { current }
    }
}
"#;

        let engine = GraphemeEngine::builder()
            .with_strict_stage_b_container_execution(true)
            .build();
        let imports = vec![
            "grapheme.runtime.host.v1::state.read".to_string(),
            "grapheme.runtime.host.v1::state.write".to_string(),
        ];
        let stage_b = engine
            .compile_source_to_aot_stage_b(source, b"\0asmstageb", &imports)
            .expect("compile source to stage_b should succeed");

        let err = engine
            .execute_aot(&stage_b)
            .expect_err("strict mode should reject fallback when container runtime is unavailable");

        assert!(matches!(err, GraphemeSdkError::Contract(_)));
        assert!(err
            .to_string()
            .contains("strict stage_b container execution required"));
    }
}
