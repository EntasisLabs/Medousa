use once_cell::sync::Lazy;
use std::sync::Arc;

use tokio::sync::{oneshot, RwLock};
use tokio::task::JoinHandle;

pub const DEFAULT_LOCAL_ENGINE_BIND: &str = "127.0.0.1:7421";
pub const DEFAULT_LOCAL_ENGINE_BASE_URL: &str = "http://127.0.0.1:7421/v1";

#[derive(Debug, Clone)]
pub struct LocalEngineConfig {
    pub bind: String,
    pub model_repo: String,
    pub model_alias: String,
    pub from_uqff: Option<String>,
    pub in_situ_quant: Option<String>,
    pub cpu_only: bool,
}

impl Default for LocalEngineConfig {
    fn default() -> Self {
        Self {
            bind: DEFAULT_LOCAL_ENGINE_BIND.to_string(),
            model_repo: "google/gemma-4-E4B-it".to_string(),
            model_alias: "gemma-4-e4b-it".to_string(),
            from_uqff: None,
            in_situ_quant: Some("4".to_string()),
            cpu_only: false,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEngineStatus {
    pub feature_enabled: bool,
    pub loaded: bool,
    pub base_url: String,
    pub bind: Option<String>,
    pub model_repo: Option<String>,
    pub model_alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_backend: Option<String>,
    pub message: String,
}

impl LocalEngineStatus {
    pub fn idle() -> Self {
        Self {
            feature_enabled: cfg!(feature = "embedded-inference")
                || super::process::medousa_local_binary_available(),
            loaded: false,
            base_url: DEFAULT_LOCAL_ENGINE_BASE_URL.to_string(),
            bind: None,
            model_repo: None,
            model_alias: None,
            inference_backend: None,
            message: if super::process::medousa_local_binary_available() {
                "Local engine not loaded".to_string()
            } else {
                "Offline brain package not installed".to_string()
            },
        }
    }
}

pub struct LocalEngineManager {
    status: Arc<RwLock<LocalEngineStatus>>,
    server_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    shutdown_tx: Arc<RwLock<Option<oneshot::Sender<()>>>>,
}

impl LocalEngineManager {
    pub fn new() -> Self {
        Self {
            status: Arc::new(RwLock::new(LocalEngineStatus::idle())),
            server_task: Arc::new(RwLock::new(None)),
            shutdown_tx: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn status(&self) -> LocalEngineStatus {
        let cached = self.status.read().await.clone();
        if cached.loaded {
            return cached;
        }

        #[cfg(not(feature = "embedded-inference"))]
        {
            let external = super::process::external_engine_status().await;
            if external.loaded {
                return external;
            }
        }

        cached
    }

    pub async fn load(&self, config: LocalEngineConfig) -> Result<LocalEngineStatus, String> {
        self.unload().await?;

        #[cfg(feature = "embedded-inference")]
        {
            let probe = super::hardware::probe_hardware();
            let device = super::backends::resolve_inference_device(&probe);
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
                inference_backend: Some(device.as_str().to_string()),
                message: format!("Local Gemma engine ready ({})", device.label()),
            };
            *self.status.write().await = status.clone();
            Ok(status)
        }

        #[cfg(not(feature = "embedded-inference"))]
        {
            let status = super::process::spawn_external_local_engine(config).await?;
            *self.status.write().await = status.clone();
            Ok(status)
        }
    }

    pub async fn unload(&self) -> Result<(), String> {
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.server_task.write().await.take() {
            task.abort();
        }
        #[cfg(not(feature = "embedded-inference"))]
        {
            super::process::stop_external_local_engine().await;
        }
        *self.status.write().await = LocalEngineStatus::idle();
        Ok(())
    }
}

impl Default for LocalEngineManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "embedded-inference")]
struct LoadedEngine {
    server_task: JoinHandle<()>,
    shutdown_tx: oneshot::Sender<()>,
}

#[cfg(feature = "embedded-inference")]
async fn load_embedded_engine(config: LocalEngineConfig) -> Result<LoadedEngine, String> {
    use std::net::SocketAddr;

    use mistralrs_core::{initialize_logging, TokenSource};
    use mistralrs_server_core::mistralrs_for_server_builder::{
        configure_paged_attn_from_flags, MistralRsForServerBuilder,
    };
    use mistralrs_server_core::mistralrs_server_router_builder::MistralRsServerRouterBuilder;

    initialize_logging();

    let model = build_model_selected(&config)?;
    let paged_attn = configure_paged_attn_from_flags(false, false).map_err(|err| err.to_string())?;
    let cpu_only = config.cpu_only;
    let mut builder = MistralRsForServerBuilder::new()
        .with_model(model)
        .with_token_source(TokenSource::CacheToken)
        .with_cpu(cpu_only)
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
                    eprintln!("medousa local engine server error: {err}");
                }
            }
            _ = &mut shutdown_rx => {}
        }
    });

    Ok(LoadedEngine {
        server_task,
        shutdown_tx,
    })
}

#[cfg(feature = "embedded-inference")]
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

pub static LOCAL_ENGINE: Lazy<Arc<LocalEngineManager>> =
    Lazy::new(|| Arc::new(LocalEngineManager::new()));

pub async fn load_recommended_engine(bind: Option<String>) -> Result<LocalEngineStatus, String> {
    #[cfg(not(feature = "embedded-inference"))]
    {
        return super::process::spawn_external_recommended(bind).await;
    }

    #[cfg(feature = "embedded-inference")]
    {
        let profile = super::hardware::read_hardware_profile().unwrap_or_else(|| {
            super::hardware::build_hardware_profile(super::hardware::probe_hardware())
        });
        let entry = super::catalog::recommended_model_for_tier(profile.tier)
            .ok_or_else(|| "no recommended model for hardware tier".to_string())?;
        let config = config_from_catalog_entry(&entry, bind);
        LOCAL_ENGINE.as_ref().load(config).await
    }
}

pub fn config_from_catalog_entry(
    entry: &super::catalog::CatalogModelEntry,
    bind: Option<String>,
) -> LocalEngineConfig {
    let probe = super::hardware::probe_hardware();
    config_from_catalog_entry_with_probe(entry, bind, &probe)
}

pub fn config_from_catalog_entry_with_probe(
    entry: &super::catalog::CatalogModelEntry,
    bind: Option<String>,
    probe: &super::hardware::HardwareProbe,
) -> LocalEngineConfig {
    let model_repo =
        super::store::local_repo_if_installed(&entry.id).unwrap_or_else(|| entry.repo.clone());
    let uqff_file = entry
        .engine_args
        .get("uqffFile")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let in_situ_quant = entry
        .engine_args
        .get("fromUqff")
        .and_then(|value| value.as_u64())
        .map(|level| level.to_string());
    let use_uqff = uqff_file.is_some();
    LocalEngineConfig {
        bind: bind.unwrap_or_else(|| DEFAULT_LOCAL_ENGINE_BIND.to_string()),
        model_repo,
        model_alias: entry.id.clone(),
        from_uqff: uqff_file,
        in_situ_quant: if use_uqff {
            None
        } else {
            in_situ_quant.or_else(|| Some("4".to_string()))
        },
        cpu_only: super::backends::resolve_cpu_only(probe),
    }
}
