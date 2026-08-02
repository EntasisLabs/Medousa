use std::future::IntoFuture;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use http_body::{Body as HttpBody, Frame};
use medousa_types::local::{
    LOCAL_WORKER_STATUS_PATH, LocalEngineStatus, LocalRuntimePhase, LocalWorkerStatus,
};
use tokio::sync::{RwLock, oneshot};
use tokio::task::JoinHandle;

pub const DEFAULT_LOCAL_ENGINE_BIND: &str = "127.0.0.1:7421";

#[derive(Debug, Clone)]
pub struct LocalEngineConfig {
    pub bind: String,
    pub model_repo: String,
    pub model_alias: String,
    pub from_uqff: Option<String>,
    pub in_situ_quant: Option<String>,
    pub cpu_only: bool,
    pub max_seq_len: usize,
    pub max_batch_size: usize,
    pub idle_timeout_secs: u64,
    pub critical_available_mb: u64,
    pub worker: LocalWorkerStatus,
}

pub struct LoadedEngineHandle {
    server_task: JoinHandle<()>,
    shutdown_tx: oneshot::Sender<()>,
}

pub struct LocalEngineRuntime {
    server_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    shutdown_tx: Arc<RwLock<Option<oneshot::Sender<()>>>>,
    status: Arc<RwLock<LocalEngineStatus>>,
}

impl LocalEngineRuntime {
    pub fn new() -> Self {
        Self {
            server_task: Arc::new(RwLock::new(None)),
            shutdown_tx: Arc::new(RwLock::new(None)),
            status: Arc::new(RwLock::new(LocalEngineStatus {
                feature_enabled: true,
                loaded: false,
                phase: LocalRuntimePhase::Cold,
                base_url: format!("http://{DEFAULT_LOCAL_ENGINE_BIND}/v1"),
                bind: None,
                model_repo: None,
                model_alias: None,
                inference_backend: None,
                worker: None,
                message: "Local engine not loaded".to_string(),
            })),
        }
    }

    pub async fn status(&self) -> LocalEngineStatus {
        self.status.read().await.clone()
    }

    pub async fn load(&self, config: LocalEngineConfig) -> Result<LocalEngineStatus, String> {
        self.unload().await?;
        *self.status.write().await = LocalEngineStatus {
            feature_enabled: true,
            loaded: false,
            phase: LocalRuntimePhase::Loading,
            base_url: format!("http://{}/v1", config.bind.trim()),
            bind: Some(config.bind.clone()),
            model_repo: Some(config.model_repo.clone()),
            model_alias: Some(config.model_alias.clone()),
            inference_backend: None,
            worker: Some(config.worker.clone()),
            message: "Loading local model".to_string(),
        };
        let loaded = match load_embedded_engine(config.clone()).await {
            Ok(loaded) => loaded,
            Err(error) => {
                let mut status = self.status.write().await;
                status.phase = LocalRuntimePhase::Failed;
                status.message = format!("Local model failed to load: {error}");
                return Err(error);
            }
        };
        *self.shutdown_tx.write().await = Some(loaded.shutdown_tx);
        *self.server_task.write().await = Some(loaded.server_task);
        let status = LocalEngineStatus {
            feature_enabled: true,
            loaded: true,
            phase: LocalRuntimePhase::Ready,
            base_url: format!("http://{}/v1", config.bind.trim()),
            bind: Some(config.bind),
            model_repo: Some(config.model_repo),
            model_alias: Some(config.model_alias),
            inference_backend: None,
            worker: Some(config.worker),
            message: "Local Gemma engine ready".to_string(),
        };
        *self.status.write().await = status.clone();
        Ok(status)
    }

    pub async fn unload(&self) -> Result<(), String> {
        if self.status.read().await.loaded {
            let mut status = self.status.write().await;
            status.phase = LocalRuntimePhase::Unloading;
            status.message = "Unloading local model".to_string();
        }
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            let _ = tx.send(());
        }
        await_server_stop(self.server_task.write().await.take()).await;
        *self.status.write().await = LocalEngineStatus::idle(true);
        Ok(())
    }

    pub async fn wait_until_stopped(&self) {
        loop {
            if self
                .server_task
                .read()
                .await
                .as_ref()
                .is_none_or(tokio::task::JoinHandle::is_finished)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

async fn await_server_stop(task: Option<JoinHandle<()>>) {
    let Some(mut task) = task else {
        return;
    };
    if tokio::time::timeout(Duration::from_secs(10), &mut task)
        .await
        .is_err()
    {
        task.abort();
        let _ = task.await;
    }
}

impl Default for LocalEngineRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalEngineRuntime, RequestActivity};
    use medousa_types::local::LocalRuntimePhase;
    use std::time::Duration;

    #[tokio::test]
    async fn unload_always_returns_runtime_to_cold() {
        let runtime = LocalEngineRuntime::new();
        runtime.unload().await.unwrap();
        let status = runtime.status().await;
        assert!(!status.loaded);
        assert_eq!(status.phase, LocalRuntimePhase::Cold);
    }

    #[test]
    fn active_response_body_blocks_idle_eviction() {
        let activity = RequestActivity::new();
        activity.begin();
        assert!(!activity.is_idle_for(Duration::ZERO));
        activity.finish();
        assert!(activity.is_idle_for(Duration::ZERO));
    }
}

pub async fn load_embedded_engine(config: LocalEngineConfig) -> Result<LoadedEngineHandle, String> {
    use mistralrs_core::{TokenSource, initialize_logging};
    use mistralrs_server_core::mistralrs_for_server_builder::{
        MistralRsForServerBuilder, configure_paged_attn_from_flags,
    };
    use mistralrs_server_core::mistralrs_server_router_builder::MistralRsServerRouterBuilder;

    initialize_logging();

    let model = build_model_selected(&config)?;
    let paged_attn =
        configure_paged_attn_from_flags(false, false).map_err(|err| err.to_string())?;
    let mut builder = MistralRsForServerBuilder::new()
        .with_model(model)
        .with_token_source(TokenSource::CacheToken)
        .with_cpu(config.cpu_only)
        .set_paged_attn(paged_attn);

    if let Some(isq) = config.in_situ_quant.as_deref() {
        builder = builder.with_in_situ_quant(isq.to_string());
    }

    let mistralrs = builder.build().await.map_err(|err| err.to_string())?;
    let activity = Arc::new(RequestActivity::new());
    let worker = Arc::new(config.worker.clone());
    let status_activity = activity.clone();
    let app = MistralRsServerRouterBuilder::new()
        .with_mistralrs(mistralrs)
        .build()
        .await
        .map_err(|err| err.to_string())?
        .route(
            LOCAL_WORKER_STATUS_PATH,
            get(move || {
                let worker = worker.clone();
                let activity = status_activity.clone();
                async move {
                    let mut status = (*worker).clone();
                    status.phase = if activity.has_active_requests() {
                        LocalRuntimePhase::Busy
                    } else {
                        LocalRuntimePhase::Ready
                    };
                    Json(status)
                }
            }),
        )
        .layer(middleware::from_fn_with_state(
            activity.clone(),
            track_request_activity,
        ));

    let addr: SocketAddr = config
        .bind
        .parse()
        .map_err(|err| format!("invalid engine bind address: {err}"))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| format!("failed to bind local engine on {addr}: {err}"))?;

    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let idle_timeout = Duration::from_secs(config.idle_timeout_secs);
    let critical_available_mb = config.critical_available_mb;
    let server_task = tokio::spawn(async move {
        let serve = axum::serve(listener, app).into_future();
        tokio::pin!(serve);
        let mut idle_check = tokio::time::interval(Duration::from_secs(1));
        let mut system = sysinfo::System::new();
        loop {
            tokio::select! {
                result = &mut serve => {
                    if let Err(err) = result {
                        eprintln!("medousa_local engine error: {err}");
                    }
                    break;
                }
                _ = &mut shutdown_rx => break,
                _ = idle_check.tick() => {
                    system.refresh_memory();
                    let available_mb = system.available_memory() / 1024 / 1024;
                    if available_mb < critical_available_mb {
                        eprintln!(
                            "medousa_local critical memory pressure: {available_mb} MiB available; terminating worker"
                        );
                        break;
                    }
                    if !idle_timeout.is_zero() && activity.is_idle_for(idle_timeout) {
                        eprintln!("medousa_local idle timeout reached");
                        break;
                    }
                }
            }
        }
    });

    Ok(LoadedEngineHandle {
        server_task,
        shutdown_tx,
    })
}

struct RequestActivity {
    active_requests: AtomicUsize,
    idle_since: std::sync::Mutex<Instant>,
}

impl RequestActivity {
    fn new() -> Self {
        Self {
            active_requests: AtomicUsize::new(0),
            idle_since: std::sync::Mutex::new(Instant::now()),
        }
    }

    fn begin(&self) {
        self.active_requests.fetch_add(1, Ordering::AcqRel);
    }

    fn finish(&self) {
        let previous = self.active_requests.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "request activity underflow");
        if previous == 1 {
            *self.idle_since.lock().expect("idle activity lock") = Instant::now();
        }
    }

    fn is_idle_for(&self, timeout: Duration) -> bool {
        self.active_requests.load(Ordering::Acquire) == 0
            && self
                .idle_since
                .lock()
                .expect("idle activity lock")
                .elapsed()
                >= timeout
    }

    fn has_active_requests(&self) -> bool {
        self.active_requests.load(Ordering::Acquire) > 0
    }
}

struct TrackedBody {
    inner: Body,
    activity: Arc<RequestActivity>,
    finished: bool,
}

impl TrackedBody {
    fn finish(&mut self) {
        if !self.finished {
            self.finished = true;
            self.activity.finish();
        }
    }
}

impl Drop for TrackedBody {
    fn drop(&mut self) {
        self.finish();
    }
}

impl HttpBody for TrackedBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let poll = Pin::new(&mut self.inner).poll_frame(cx);
        if matches!(poll, Poll::Ready(None) | Poll::Ready(Some(Err(_)))) {
            self.finish();
        }
        poll
    }
}

async fn track_request_activity(
    State(activity): State<Arc<RequestActivity>>,
    request: Request,
    next: Next,
) -> Response {
    if request.uri().path().starts_with("/_medousa/") {
        return next.run(request).await;
    }
    activity.begin();
    let response = next.run(request).await;
    let (parts, body) = response.into_parts();
    Response::from_parts(
        parts,
        Body::new(TrackedBody {
            inner: body,
            activity,
            finished: false,
        }),
    )
}

fn build_model_selected(
    config: &LocalEngineConfig,
) -> Result<mistralrs_core::ModelSelected, String> {
    use mistralrs_core::{AutoDeviceMapParams, ModelDType, ModelSelected, MultimodalLoaderType};

    Ok(ModelSelected::MultimodalPlain {
        model_id: config.model_repo.clone(),
        tokenizer_json: None,
        arch: Some(MultimodalLoaderType::Gemma4),
        dtype: ModelDType::Auto,
        topology: None,
        organization: None,
        write_uqff: None,
        from_uqff: config.from_uqff.clone(),
        max_edge: None,
        calibration_file: None,
        imatrix: None,
        max_seq_len: config.max_seq_len,
        max_batch_size: config.max_batch_size,
        max_num_images: AutoDeviceMapParams::DEFAULT_MAX_NUM_IMAGES,
        max_image_length: AutoDeviceMapParams::DEFAULT_MAX_IMAGE_LENGTH,
        hf_cache_path: None,
        matformer_config_path: None,
        matformer_slice_name: None,
    })
}
