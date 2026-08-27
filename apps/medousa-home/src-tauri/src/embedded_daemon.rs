//! Tauri ownership for the in-process iOS deployment of `medousa_daemon`.

#[cfg(target_os = "ios")]
use std::sync::Arc;

#[cfg(target_os = "ios")]
use medousa::embedded_daemon::{
    CredentialProvider, EmbeddedDaemon, EmbeddedDaemonClient, EmbeddedDaemonConfig,
    ProviderCredential, ProviderCredentialError,
};

#[derive(Clone)]
pub struct EmbeddedDaemonState {
    #[cfg(target_os = "ios")]
    daemon: Arc<tokio::sync::OnceCell<Arc<EmbeddedDaemon>>>,
}

impl EmbeddedDaemonState {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "ios")]
            daemon: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    #[cfg(target_os = "ios")]
    pub async fn client_if_active(&self) -> Result<Option<EmbeddedDaemonClient>, String> {
        if !embedded_workshop_selected().await? {
            return Ok(None);
        }

        let daemon = self
            .daemon
            .get_or_try_init(boot_embedded_daemon)
            .await?
            .clone();

        // Workshop selection may have changed while the persistent runtime was
        // booting. Never route a newly issued client through stale selection.
        if !embedded_workshop_selected().await? {
            return Ok(None);
        }
        Ok(Some(daemon.local_client()))
    }

    #[cfg(target_os = "ios")]
    pub async fn client_if_active_for_route(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Result<Option<EmbeddedDaemonClient>, String> {
        let Some(client) = self.client_if_active().await? else {
            return Ok(None);
        };
        let configured_provider = client.inference_provider();
        let configured_model = client.inference_model();
        if let Some(provider) = provider.map(str::trim).filter(|value| !value.is_empty()) {
            if !provider.eq_ignore_ascii_case(&configured_provider) {
                return Err(format!(
                    "the embedded daemon is configured for provider '{}' (requested '{provider}')",
                    configured_provider
                ));
            }
        }
        if let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) {
            if model != configured_model {
                return Err(format!(
                    "the embedded daemon is configured for model '{}' (requested '{model}')",
                    configured_model
                ));
            }
        }
        Ok(Some(client))
    }

    #[cfg(target_os = "ios")]
    pub fn validate_inference_defaults(
        &self,
        defaults: &crate::medousa_paths::TuiDefaultsDto,
    ) -> Result<(), String> {
        let _ = inference_route_from_defaults(defaults)?;
        Ok(())
    }

    #[cfg(target_os = "ios")]
    pub async fn reconfigure_active(
        &self,
        defaults: &crate::medousa_paths::TuiDefaultsDto,
    ) -> Result<(), String> {
        let (provider, model, base_url) = inference_route_from_defaults(defaults)?;
        let Some(client) = self.client_if_active().await? else {
            return Err("Embedded Personal is no longer the selected workshop".to_string());
        };
        client
            .reconfigure_inference(provider, model, base_url)
            .map_err(|error| format!("reconfigure embedded daemon inference: {error:#}"))?;
        Ok(())
    }

    #[cfg(target_os = "ios")]
    fn background_if_booted(&self) -> usize {
        let Some(daemon) = self.daemon.get().cloned() else {
            return 0;
        };
        daemon.enter_background()
    }

    #[cfg(target_os = "ios")]
    fn resume_if_booted(&self) {
        let Some(daemon) = self.daemon.get().cloned() else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            match daemon.resume().await {
                Ok(report) if report.materialized > 0 || !report.processed_job_ids.is_empty() => {
                    eprintln!(
                        "[medousa-home] embedded daemon wake reconciled {} schedule(s) and {} job(s)",
                        report.materialized,
                        report.processed_job_ids.len()
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("[medousa-home] embedded daemon resume failed: {error:#}");
                }
            }
        });
    }
}

impl Default for EmbeddedDaemonState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "ios")]
async fn embedded_workshop_selected() -> Result<bool, String> {
    tokio::task::spawn_blocking(|| {
        Ok(matches!(
            crate::active_workshop::resolve()?,
            crate::active_workshop::ActiveWorkshopTarget::EmbeddedPersonal
        ))
    })
    .await
    .map_err(|_| "embedded workshop selection task failed".to_string())?
}

#[cfg(target_os = "ios")]
fn inference_route_from_defaults(
    defaults: &crate::medousa_paths::TuiDefaultsDto,
) -> Result<(String, String, Option<String>), String> {
    let provider = defaults
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("openai")
        .to_string();
    let model = defaults
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("gpt-5.4-mini")
        .to_string();
    let base_url = crate::integration_secrets::load_connection_base_url(&provider)
        .or_else(|| {
            defaults
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            crate::provider_catalog::find_provider(&provider)
                .and_then(|entry| entry.default_base_url.map(str::to_string))
        });
    medousa::embedded_daemon::validate_credentialed_inference_route(
        provider.clone(),
        model.clone(),
        base_url.clone(),
    )
    .map_err(|error| format!("configure embedded daemon inference: {error:#}"))?;
    Ok((provider, model, base_url))
}

#[cfg(target_os = "ios")]
pub(crate) fn normalize_inference_defaults(
    mut defaults: crate::medousa_paths::TuiDefaultsDto,
) -> Result<crate::medousa_paths::TuiDefaultsDto, String> {
    let (provider, model, _) = inference_route_from_defaults(&defaults)?;
    defaults.provider = Some(provider);
    defaults.model = Some(model);
    Ok(defaults)
}

#[cfg(target_os = "ios")]
async fn boot_embedded_daemon() -> Result<Arc<EmbeddedDaemon>, String> {
    let (installation_id, provider, model, base_url, root) = tokio::task::spawn_blocking(|| {
        let installation_id = crate::integration_secrets::ensure_secrets_bootstrapped()?;
        let defaults = crate::medousa_paths::load_tui_defaults();
        let (provider, model, base_url) = inference_route_from_defaults(&defaults)?;
        let root = crate::paths::medousa_data_dir().join("embedded-daemon");
        Ok::<_, String>((installation_id, provider, model, base_url, root))
    })
    .await
    .map_err(|_| "embedded daemon configuration task failed".to_string())??;
    let config = EmbeddedDaemonConfig::credentialed(
        root,
        installation_id,
        provider,
        model,
        base_url,
        Arc::new(HomeCredentialProvider),
    )
    .map_err(|error| format!("configure embedded daemon inference: {error:#}"))?
    .with_tool_registry_recipe(Arc::new(
        medousa::mobile_tool_registry::PersonalMobileToolRegistryRecipe,
    ));
    EmbeddedDaemon::boot(config)
        .await
        .map_err(|error| format!("boot embedded daemon: {error:#}"))
}

#[cfg(target_os = "ios")]
#[derive(Debug)]
struct HomeCredentialProvider;

#[cfg(target_os = "ios")]
#[async_trait::async_trait]
impl CredentialProvider for HomeCredentialProvider {
    async fn credential_for(
        &self,
        provider: &str,
    ) -> Result<ProviderCredential, ProviderCredentialError> {
        let provider = provider.to_string();
        let secret = tokio::task::spawn_blocking(move || {
            crate::integration_secrets::load_provider_secret(&provider)
        })
        .await
        .map_err(|_| ProviderCredentialError::Unavailable)?
        .ok_or(ProviderCredentialError::Missing)?;
        ProviderCredential::new(secret)
    }
}

#[cfg(target_os = "ios")]
pub fn prewarm(app: &tauri::AppHandle) {
    use tauri::Manager;

    let state = app.state::<EmbeddedDaemonState>().inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = state.client_if_active().await {
            eprintln!("[medousa-home] embedded daemon prewarm failed: {error}");
        }
    });
}

#[cfg(target_os = "ios")]
pub fn install_lifecycle(app: &tauri::AppHandle) {
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, Ordering};

    use block2::RcBlock;
    use objc2_foundation::{NSNotification, NSNotificationCenter};
    use objc2_ui_kit::{
        UIApplicationDidEnterBackgroundNotification, UIApplicationWillEnterForegroundNotification,
    };
    use tauri::Manager;

    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }

    let background_app = app.clone();
    let foreground_app = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        let center = NSNotificationCenter::defaultCenter();
        let background = RcBlock::new(move |_notification: NonNull<NSNotification>| {
            let live_turns = background_app
                .state::<EmbeddedDaemonState>()
                .background_if_booted();
            if live_turns > 0 {
                eprintln!(
                    "[medousa-home] embedded daemon backgrounded with {live_turns} live turn(s); execution remains OS-managed"
                );
            }
        });
        let foreground = RcBlock::new(move |_notification: NonNull<NSNotification>| {
            foreground_app
                .state::<EmbeddedDaemonState>()
                .resume_if_booted();
        });
        let background_observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(UIApplicationDidEnterBackgroundNotification),
                None,
                None,
                &background,
            )
        };
        let foreground_observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(UIApplicationWillEnterForegroundNotification),
                None,
                None,
                &foreground,
            )
        };
        // NSNotificationCenter owns the callbacks for the process lifetime.
        std::mem::forget(background_observer);
        std::mem::forget(foreground_observer);
    }) {
        eprintln!("[medousa-home] embedded daemon lifecycle install failed: {error}");
    }
}
