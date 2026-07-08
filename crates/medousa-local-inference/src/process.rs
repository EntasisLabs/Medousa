//! Re-exports host spawn helpers for callers that still import from `medousa::local_inference`.

pub use medousa_host::{
    is_bind_reachable, medousa_local_binary_available, resolve_medousa_local_binary,
    spawn_and_wait_recommended as spawn_external_recommended,
    spawn_medousa_local_recommended as spawn_external_local_engine,
};

/// Probe-only status for an externally managed `medousa_local` process.
pub async fn external_engine_status() -> medousa_types::local::LocalEngineStatus {
    super::engine::probe_local_engine_status().await
}

/// Spawn `medousa_local` and wait until the bind is reachable.
pub async fn load_external_engine(
    bind: Option<String>,
) -> Result<medousa_types::local::LocalEngineStatus, String> {
    medousa_host::spawn_and_wait_recommended(bind).await
}

/// No-op: `medousa_local` is managed by the desktop app or CLI, not the daemon.
pub async fn stop_external_local_engine() -> Result<(), String> {
    Ok(())
}
