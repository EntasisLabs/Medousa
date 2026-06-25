use std::net::SocketAddr;
use std::sync::Arc;

use medousa_types::local::LocalEngineStatus;
use tokio::sync::{oneshot, RwLock};
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
                base_url: format!("http://{DEFAULT_LOCAL_ENGINE_BIND}/v1"),
                bind: None,
                model_repo: None,
                model_alias: None,
                inference_backend: None,
                message: "Local engine not loaded".to_string(),
            })),
        }
    }

    pub async fn status(&self) -> LocalEngineStatus {
        self.status.read().await.clone()
    }

    pub async fn load(&self, config: LocalEngineConfig) -> Result<LocalEngineStatus, String> {
        self.unload().await?;
        let loaded = load_embedded_engine(config.clone()).await?;
        *self.shutdown_tx.write().await = Some(loaded.shutdown_tx);
        *self.server_task.write().await = Some(loaded.server_task);
        let status = LocalEngineStatus {
            feature_enabled: true,
            loaded: true,
            base_url: format!("http://{}/v1", config.bind.trim()),
            bind: Some(config.bind),
            model_repo: Some(config.model_repo),
            model_alias: Some(config.model_alias),
            inference_backend: None,
            message: "Local Gemma engine ready".to_string(),
        };
        *self.status.write().await = status.clone();
        Ok(status)
    }

    pub async fn unload(&self) -> Result<(), String> {
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.server_task.write().await.take() {
            task.abort();
        }
        Ok(())
    }
}

impl Default for LocalEngineRuntime {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn load_embedded_engine(config: LocalEngineConfig) -> Result<LoadedEngineHandle, String> {
    use mistralrs_core::{initialize_logging, TokenSource};
    use mistralrs_server_core::mistralrs_for_server_builder::{
        configure_paged_attn_from_flags, MistralRsForServerBuilder,
    };
    use mistralrs_server_core::mistralrs_server_router_builder::MistralRsServerRouterBuilder;

    initialize_logging();

    let model = build_model_selected(&config)?;
    let paged_attn = configure_paged_attn_from_flags(false, false).map_err(|err| err.to_string())?;
    let mut builder = MistralRsForServerBuilder::new()
        .with_model(model)
        .with_token_source(TokenSource::CacheToken)
        .with_cpu(config.cpu_only)
        .set_paged_attn(paged_attn);

    if let Some(isq) = config.in_situ_quant.as_deref() {
        builder = builder.with_in_situ_quant(isq.to_string());
    }

    let mistralrs = builder.build().await.map_err(|err| err.to_string())?;
    let app = MistralRsServerRouterBuilder::new()
        .with_mistralrs(mistralrs)
        .build()
        .await
        .map_err(|err| err.to_string())?;

    let addr: SocketAddr = config
        .bind
        .parse()
        .map_err(|err| format!("invalid engine bind address: {err}"))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| format!("failed to bind local engine on {addr}: {err}"))?;

    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let server_task = tokio::spawn(async move {
        let serve = axum::serve(listener, app);
        tokio::select! {
            result = serve => {
                if let Err(err) = result {
                    eprintln!("medousa_local engine error: {err}");
                }
            }
            _ = &mut shutdown_rx => {}
        }
    });

    Ok(LoadedEngineHandle {
        server_task,
        shutdown_tx,
    })
}

fn build_model_selected(config: &LocalEngineConfig) -> Result<mistralrs_core::ModelSelected, String> {
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
        max_seq_len: AutoDeviceMapParams::DEFAULT_MAX_SEQ_LEN,
        max_batch_size: AutoDeviceMapParams::DEFAULT_MAX_BATCH_SIZE,
        max_num_images: AutoDeviceMapParams::DEFAULT_MAX_NUM_IMAGES,
        max_image_length: AutoDeviceMapParams::DEFAULT_MAX_IMAGE_LENGTH,
        hf_cache_path: None,
        matformer_config_path: None,
        matformer_slice_name: None,
    })
}
