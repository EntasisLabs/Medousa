//! Tauri ownership for the in-process iOS deployment of `medousa_daemon`.

#[cfg(target_os = "ios")]
use std::sync::Arc;
#[cfg(target_os = "ios")]
use std::time::Duration;

#[cfg(target_os = "ios")]
use medousa::embedded_daemon::{
    CredentialProvider, EmbeddedDaemon, EmbeddedDaemonClient, EmbeddedDaemonConfig,
    ProviderCredential, ProviderCredentialError,
};

#[cfg(target_os = "ios")]
const IOS_SUSPEND_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(target_os = "ios")]
enum IosBackgroundTaskState {
    Starting,
    Active(objc2_ui_kit::UIBackgroundTaskIdentifier),
    Finished,
}

#[cfg(target_os = "ios")]
type IosBackgroundTask = Arc<std::sync::Mutex<IosBackgroundTaskState>>;

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
        if let Some(provider) = provider.map(str::trim).filter(|value| !value.is_empty()) {
            if !provider.eq_ignore_ascii_case(client.inference_provider()) {
                return Err(format!(
                    "the embedded daemon is configured for provider '{}' (requested '{provider}')",
                    client.inference_provider()
                ));
            }
        }
        if let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) {
            if model != client.inference_model() {
                return Err(format!(
                    "the embedded daemon is configured for model '{}' (requested '{model}')",
                    client.inference_model()
                ));
            }
        }
        Ok(Some(client))
    }

    #[cfg(target_os = "ios")]
    fn suspend_if_booted(&self, app: &tauri::AppHandle) -> usize {
        let Some(daemon) = self.daemon.get().cloned() else {
            return 0;
        };
        let cancellation_requested = daemon.suspend();
        let background_task = begin_ios_background_task();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let report = daemon
                .drain_suspended(cancellation_requested, IOS_SUSPEND_DRAIN_TIMEOUT)
                .await;
            if report.timed_out {
                eprintln!(
                    "[medousa-home] embedded daemon suspension deadline expired with {} turn(s) still live",
                    report.remaining_turns
                );
            }
            if let Some(background_task) = background_task {
                finish_ios_background_task(&app, background_task);
            }
        });
        cancellation_requested
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

#[cfg(target_os = "ios")]
fn begin_ios_background_task() -> Option<IosBackgroundTask> {
    use block2::RcBlock;
    use objc2::MainThreadMarker;
    use objc2_ui_kit::{UIApplication, UIBackgroundTaskInvalid};

    let mtm = MainThreadMarker::new()?;
    let state = Arc::new(std::sync::Mutex::new(IosBackgroundTaskState::Starting));
    let expiration_state = state.clone();
    let expiration = RcBlock::new(move || {
        finish_ios_background_task_on_main(expiration_state.clone());
    });
    let application = UIApplication::sharedApplication(mtm);
    let identifier = application.beginBackgroundTaskWithExpirationHandler(Some(&expiration));
    if identifier == unsafe { UIBackgroundTaskInvalid } {
        return None;
    }
    let end_immediately = {
        let mut guard = state.lock().expect("iOS background task state");
        match *guard {
            IosBackgroundTaskState::Starting => {
                *guard = IosBackgroundTaskState::Active(identifier);
                false
            }
            IosBackgroundTaskState::Finished => true,
            IosBackgroundTaskState::Active(_) => false,
        }
    };
    if end_immediately {
        application.endBackgroundTask(identifier);
    }
    Some(state)
}

#[cfg(target_os = "ios")]
fn finish_ios_background_task(app: &tauri::AppHandle, state: IosBackgroundTask) {
    if let Err(error) = app.run_on_main_thread(move || {
        finish_ios_background_task_on_main(state);
    }) {
        eprintln!("[medousa-home] could not release iOS background task: {error}");
    }
}

#[cfg(target_os = "ios")]
fn finish_ios_background_task_on_main(state: IosBackgroundTask) {
    use objc2::MainThreadMarker;
    use objc2_ui_kit::UIApplication;

    let identifier = {
        let mut guard = state.lock().expect("iOS background task state");
        match std::mem::replace(&mut *guard, IosBackgroundTaskState::Finished) {
            IosBackgroundTaskState::Active(identifier) => Some(identifier),
            IosBackgroundTaskState::Starting | IosBackgroundTaskState::Finished => None,
        }
    };
    if let (Some(identifier), Some(mtm)) = (identifier, MainThreadMarker::new()) {
        UIApplication::sharedApplication(mtm).endBackgroundTask(identifier);
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
async fn boot_embedded_daemon() -> Result<Arc<EmbeddedDaemon>, String> {
    let (installation_id, provider, model, base_url, root) = tokio::task::spawn_blocking(|| {
        let installation_id = crate::integration_secrets::ensure_secrets_bootstrapped()?;
        let defaults = crate::medousa_paths::load_tui_defaults_summary();
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
        let base_url = crate::integration_secrets::load_connection_base_url(&provider);
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
    .map_err(|error| format!("configure embedded daemon inference: {error:#}"))?;
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
            let cancelled = background_app
                .state::<EmbeddedDaemonState>()
                .suspend_if_booted(&background_app);
            if cancelled > 0 {
                eprintln!(
                    "[medousa-home] suspended embedded daemon; cancellation requested for {cancelled} turn(s)"
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
