use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use serde_json::{json, Value as JsonValue};

use crate::error::RuntimeError;
use crate::module_registry::ResolvedModuleCall;

#[cfg(feature = "wasix-runtime")]
use wasmer::{Engine, Module};
#[cfg(feature = "wasix-runtime")]
use wasmer_types::ModuleHash;
#[cfg(feature = "wasix-runtime")]
use wasmer_wasix::{
    is_wasi_module,
    runners::wasi::{RuntimeOrEngine, WasiRunner},
    virtual_fs::{AsyncReadExt, AsyncWriteExt},
    Pipe,
};

/// Placeholder for the upcoming Wasmer WASIX-backed execution engine.
///
/// This keeps the integration boundary explicit while the MIR interpreter
/// remains the default runtime path.
#[cfg(feature = "wasix-runtime")]
pub struct WasixBackend {
    engine: Engine,
    runtime: tokio::runtime::Runtime,
    module_cache: Mutex<ModuleCache>,
    timing_enabled: bool,
    timing_stats: Mutex<TimingStats>,
}

#[cfg(feature = "wasix-runtime")]
struct ModuleCache {
    max_modules: usize,
    modules: HashMap<PathBuf, Module>,
    order: VecDeque<PathBuf>,
}

#[cfg(feature = "wasix-runtime")]
impl ModuleCache {
    fn new(max_modules: usize) -> Self {
        Self {
            max_modules,
            modules: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&self, path: &Path) -> Option<Module> {
        self.modules.get(path).cloned()
    }

    fn insert(&mut self, path: PathBuf, module: Module) {
        if self.max_modules == 0 {
            return;
        }

        while self.modules.len() >= self.max_modules {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.modules.remove(&oldest);
        }

        self.order.push_back(path.clone());
        self.modules.insert(path, module);
    }
}

#[cfg(feature = "wasix-runtime")]
#[derive(Default)]
struct TimingStats {
    calls: u64,
    cache_hits: u64,
    prepare_ms_total: u128,
    run_ms_total: u128,
    read_ms_total: u128,
    total_ms_total: u128,
    total_ms_max: u128,
}

#[cfg(feature = "wasix-runtime")]
impl TimingStats {
    fn record(
        &mut self,
        cache_hit: bool,
        prepare_ms: u128,
        run_ms: u128,
        read_ms: u128,
        total_ms: u128,
    ) {
        self.calls += 1;
        if cache_hit {
            self.cache_hits += 1;
        }
        self.prepare_ms_total += prepare_ms;
        self.run_ms_total += run_ms;
        self.read_ms_total += read_ms;
        self.total_ms_total += total_ms;
        if total_ms > self.total_ms_max {
            self.total_ms_max = total_ms;
        }
    }
}

#[cfg(feature = "wasix-runtime")]
impl WasixBackend {
    pub fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("init tokio runtime for wasix backend");

        Self {
            engine: Engine::default(),
            runtime,
            module_cache: Mutex::new(ModuleCache::new(read_cache_max_modules())),
            timing_enabled: read_timing_enabled(),
            timing_stats: Mutex::new(TimingStats::default()),
        }
    }

    pub fn validate_environment(&self) -> Result<(), RuntimeError> {
        // This is intentionally conservative for now; we only verify that the
        // feature is enabled and dependency wiring compiles.
        Ok(())
    }

    pub fn execute_call(
        &self,
        wasm_path: &Path,
        call: &ResolvedModuleCall,
        args: &JsonValue,
    ) -> Result<JsonValue, RuntimeError> {
        let started = Instant::now();
        let (module, cache_hit) = self.load_module(wasm_path)?;

        let after_module_prepare = Instant::now();

        let request = json!({
            "module": call.module_id,
            "op": call.op,
            "args": args,
        });

        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| RuntimeError::RuntimeError(format!("serialize wasix request: {e}")))?;

        let (mut stdin_tx, stdin_rx) = Pipe::channel();
        self.runtime
            .block_on(async { stdin_tx.write_all(&request_bytes).await })
            .map_err(|e| RuntimeError::RuntimeError(format!("write wasm stdin: {e}")))?;
        drop(stdin_tx);

        let (stdout_tx, mut stdout_rx) = Pipe::channel();

        {
            let mut runner = WasiRunner::new();
            runner
                .with_stdin(Box::new(stdin_rx))
                .with_stdout(Box::new(stdout_tx));

            runner
                .run_wasm(
                    RuntimeOrEngine::Engine(self.engine.clone()),
                    &call.module_id,
                    module,
                    ModuleHash::random(),
                )
                .map_err(|e| RuntimeError::RuntimeError(format!("run wasm module: {e}")))?;
        }

        let after_run = Instant::now();

        let mut stdout = String::new();
        self.runtime
            .block_on(async { stdout_rx.read_to_string(&mut stdout).await })
            .map_err(|e| RuntimeError::RuntimeError(format!("read wasm stdout: {e}")))?;

        let finished = Instant::now();

        if self.timing_enabled {
            let prepare_ms = after_module_prepare.duration_since(started).as_millis();
            let run_ms = after_run.duration_since(after_module_prepare).as_millis();
            let read_ms = finished.duration_since(after_run).as_millis();
            let total_ms = finished.duration_since(started).as_millis();

            if let Ok(mut stats) = self.timing_stats.lock() {
                stats.record(cache_hit, prepare_ms, run_ms, read_ms, total_ms);
            }
        }

        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(JsonValue::Null);
        }

        match serde_json::from_str::<JsonValue>(trimmed) {
            Ok(parsed) => Ok(normalize_host_envelope(parsed)),
            Err(_) => Ok(json!({ "stdout": trimmed })),
        }
    }

    fn load_module(&self, wasm_path: &Path) -> Result<(Module, bool), RuntimeError> {
        {
            let cache = self
                .module_cache
                .lock()
                .map_err(|_| RuntimeError::RuntimeError("acquire module cache lock".to_string()))?;

            if let Some(module) = cache.get(wasm_path) {
                return Ok((module, true));
            }
        }

        let wasm_bytes = std::fs::read(wasm_path).map_err(|e| {
            RuntimeError::RuntimeError(format!("read wasm module '{}': {e}", wasm_path.display()))
        })?;

        let module = Module::new(&self.engine, &wasm_bytes).map_err(|e| {
            RuntimeError::RuntimeError(format!(
                "compile wasm module '{}': {e}",
                wasm_path.display()
            ))
        })?;

        if !is_wasi_module(&module) {
            return Err(RuntimeError::ArtifactCompatibilityError(format!(
                "wasm module '{}' is not WASI/WASIX compatible",
                wasm_path.display()
            )));
        }

        let mut cache = self
            .module_cache
            .lock()
            .map_err(|_| RuntimeError::RuntimeError("acquire module cache lock".to_string()))?;

        if let Some(existing) = cache.get(wasm_path) {
            return Ok((existing, true));
        }

        cache.insert(wasm_path.to_path_buf(), module.clone());
        Ok((module, false))
    }
}

#[cfg(feature = "wasix-runtime")]
impl Drop for WasixBackend {
    fn drop(&mut self) {
        if !self.timing_enabled {
            return;
        }

        let Ok(stats) = self.timing_stats.lock() else {
            return;
        };
        if stats.calls == 0 {
            return;
        }

        let calls = stats.calls as u128;
        eprintln!(
            "[timing-summary] calls={} cache_hits={} cache_hit_rate_pct={:.1} avg_prepare_ms={} avg_run_ms={} avg_read_ms={} avg_total_ms={} max_total_ms={}",
            stats.calls,
            stats.cache_hits,
            (stats.cache_hits as f64 / stats.calls as f64) * 100.0,
            stats.prepare_ms_total / calls,
            stats.run_ms_total / calls,
            stats.read_ms_total / calls,
            stats.total_ms_total / calls,
            stats.total_ms_max,
        );
    }
}

#[cfg(feature = "wasix-runtime")]
fn normalize_host_envelope(raw: JsonValue) -> JsonValue {
    if raw
        .as_object()
        .is_some_and(|obj| obj.contains_key("data") && obj.contains_key("meta") && obj.contains_key("error"))
    {
        return raw;
    }

    if raw.get("error").and_then(|v| v.as_str()).is_some() {
        return json!({
            "data": raw.get("data").cloned().unwrap_or(JsonValue::Null),
            "meta": json!({ "legacy_flat": true, "adapter": "wasix" }),
            "error": raw.get("error").and_then(|v| v.as_str()),
        });
    }

    json!({
        "data": raw,
        "meta": json!({ "legacy_flat": true, "adapter": "wasix" }),
        "error": null,
    })
}

#[cfg(feature = "wasix-runtime")]
fn read_cache_max_modules() -> usize {
    std::env::var("GRAPHEME_WASIX_CACHE_MAX_MODULES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(8)
}

#[cfg(feature = "wasix-runtime")]
fn read_timing_enabled() -> bool {
    std::env::var("GRAPHEME_RUNTIME_TIMING")
        .ok()
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}
