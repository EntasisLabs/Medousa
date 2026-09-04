//! Tauri ownership for the in-process mobile deployment of `medousa_daemon`.

#[cfg(any(target_os = "ios", target_os = "android"))]
use std::sync::{Arc, OnceLock};

#[cfg(any(target_os = "ios", target_os = "android"))]
use medousa::chatgpt_oauth::{ChatGptCredentialStore, ChatGptOAuthBroker};
#[cfg(any(target_os = "ios", target_os = "android"))]
use medousa::delegated_task::{
    DelegatedTaskControlObservation, DelegatedTaskControlRequest, DelegatedTaskError,
    DelegatedTaskObservation, DelegatedTaskRequest, DelegatedTaskTransport, delegated_work_id,
    validate_task_control_observation,
};
#[cfg(any(target_os = "ios", target_os = "android"))]
use medousa::embedded_daemon::{
    CredentialProvider, EmbeddedDaemon, EmbeddedDaemonClient, EmbeddedDaemonConfig,
    EmbeddedNativeChatRequest, EmbeddedNativeChatResponse, EmbeddedNativeInference,
    EmbeddedNativeInferenceEvent, ProviderCredential, ProviderCredentialError,
};

#[derive(Clone)]
pub struct EmbeddedDaemonState {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    daemon: Arc<tokio::sync::OnceCell<Arc<EmbeddedDaemon>>>,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    native_inference: Arc<OnceLock<Arc<dyn EmbeddedNativeInference>>>,
}

impl EmbeddedDaemonState {
    pub fn new() -> Self {
        Self {
            #[cfg(any(target_os = "ios", target_os = "android"))]
            daemon: Arc::new(tokio::sync::OnceCell::new()),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            native_inference: Arc::new(OnceLock::new()),
        }
    }

    #[cfg(target_os = "ios")]
    pub fn install_native_inference(
        &self,
        inference: Arc<dyn EmbeddedNativeInference>,
    ) -> Result<(), String> {
        self.native_inference
            .set(inference)
            .map_err(|_| "native inference was already installed".to_string())
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub async fn client_if_active(&self) -> Result<Option<EmbeddedDaemonClient>, String> {
        if !embedded_workshop_selected().await? {
            return Ok(None);
        }

        let native_inference = self.native_inference.get().cloned();
        let daemon = self
            .daemon
            .get_or_try_init(|| boot_embedded_daemon(native_inference))
            .await?
            .clone();

        // Workshop selection may have changed while the persistent runtime was
        // booting. Never route a newly issued client through stale selection.
        if !embedded_workshop_selected().await? {
            return Ok(None);
        }
        Ok(Some(daemon.local_client()))
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
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

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub fn validate_inference_defaults(
        &self,
        defaults: &crate::medousa_paths::TuiDefaultsDto,
    ) -> Result<(), String> {
        let _ = inference_route_from_defaults(defaults)?;
        Ok(())
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub async fn reconfigure_active(
        &self,
        defaults: &crate::medousa_paths::TuiDefaultsDto,
    ) -> Result<(), String> {
        let (provider, model, base_url) = inference_route_from_defaults(defaults)?;
        let portable_defaults = portable_tui_defaults(defaults)?;
        let Some(client) = self.client_if_active().await? else {
            return Err("Embedded Personal is no longer the selected workshop".to_string());
        };
        client
            .reconfigure_inference(provider, model, base_url)
            .map_err(|error| format!("reconfigure embedded daemon inference: {error:#}"))?;
        client
            .sync_tui_defaults(portable_defaults)
            .map_err(|error| format!("sync embedded daemon settings: {error:#}"))?;
        Ok(())
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(crate) fn background_if_booted(&self) -> usize {
        let Some(daemon) = self.daemon.get().cloned() else {
            return 0;
        };
        daemon.enter_background()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(crate) fn resume_if_booted(&self) {
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
#[derive(Clone)]
pub struct HomeNativeInference {
    plugin: tauri_plugin_native_inference::NativeInference<tauri::Wry>,
}

#[cfg(target_os = "ios")]
impl HomeNativeInference {
    pub fn new(plugin: tauri_plugin_native_inference::NativeInference<tauri::Wry>) -> Self {
        Self { plugin }
    }
}

#[cfg(target_os = "ios")]
struct NativeRequestGuard {
    plugin: tauri_plugin_native_inference::NativeInference<tauri::Wry>,
    request_id: String,
    done: tokio::sync::mpsc::UnboundedSender<Option<serde_json::Value>>,
    armed: bool,
}

#[cfg(target_os = "ios")]
impl NativeRequestGuard {
    fn finish(&mut self) {
        self.armed = false;
        let _ = self.done.send(None);
    }
}

#[cfg(target_os = "ios")]
impl Drop for NativeRequestGuard {
    fn drop(&mut self) {
        let _ = self.done.send(None);
        if !self.armed {
            return;
        }
        let plugin = self.plugin.clone();
        let request_id = self.request_id.clone();
        tauri::async_runtime::spawn(async move {
            let _ = plugin.cancel(&request_id).await;
        });
    }
}

#[cfg(target_os = "ios")]
#[async_trait::async_trait]
impl EmbeddedNativeInference for HomeNativeInference {
    async fn generate(
        &self,
        request: EmbeddedNativeChatRequest,
        events: Option<tokio::sync::mpsc::Sender<EmbeddedNativeInferenceEvent>>,
    ) -> Result<EmbeddedNativeChatResponse, String> {
        let request = serde_json::to_value(request)
            .map_err(|error| format!("encode native inference request: {error}"))?;
        let request_id =
            tauri_plugin_native_inference::NativeInference::<tauri::Wry>::new_request_id();
        let (bridge_tx, mut bridge_rx) = tokio::sync::mpsc::unbounded_channel();
        let relay = tauri::async_runtime::spawn(async move {
            while let Some(value) = bridge_rx.recv().await {
                let Some(value) = value else { break };
                let Some(events) = events.as_ref() else {
                    continue;
                };
                let event: EmbeddedNativeInferenceEvent = serde_json::from_value(value)
                    .map_err(|error| format!("decode native inference event: {error}"))?;
                events
                    .send(event)
                    .await
                    .map_err(|_| "native inference stream consumer closed".to_string())?;
            }
            Ok::<(), String>(())
        });
        let callback_tx = bridge_tx.clone();
        let mut guard = NativeRequestGuard {
            plugin: self.plugin.clone(),
            request_id: request_id.clone(),
            done: bridge_tx,
            armed: true,
        };
        // Keep Tauri's mobile plugin future alive independently from this
        // request future. `run_mobile_plugin_async` panics if its Rust receiver
        // is dropped before Swift reports cancellation or another error.
        // Dropping this outer future still triggers `NativeRequestGuard`, which
        // cancels Swift, while the detached task safely receives that reply.
        let plugin = self.plugin.clone();
        let call_request_id = request_id.clone();
        let call = tauri::async_runtime::spawn(async move {
            plugin
                .generate(&call_request_id, request, move |event| {
                    let _ = callback_tx.send(Some(event));
                })
                .await
                .map_err(|error| error.to_string())
        });
        let result = call
            .await
            .map_err(|error| format!("join native inference request: {error}"))?;
        guard.finish();
        relay
            .await
            .map_err(|error| format!("join native inference event relay: {error}"))??;
        let response = result?;
        serde_json::from_value(response)
            .map_err(|error| format!("decode native inference response: {error}"))
    }
}

impl Default for EmbeddedDaemonState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[tauri::command]
pub async fn embedded_delegation_binding(
    state: tauri::State<'_, EmbeddedDaemonState>,
) -> Result<Option<medousa::delegation::DelegationBinding>, String> {
    let client = state
        .client_if_active()
        .await?
        .ok_or_else(|| "Select Personal to manage its delegation binding".to_string())?;
    client
        .delegation_binding()
        .await
        .map_err(|error| format!("load delegation binding: {error:#}"))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[tauri::command]
pub async fn embedded_set_delegation_binding(
    state: tauri::State<'_, EmbeddedDaemonState>,
    workshop_id: String,
) -> Result<medousa::delegation::DelegationBinding, String> {
    let workshop_id = workshop_id.trim().to_string();
    let target = tokio::task::spawn_blocking(move || delegation_target_for(&workshop_id))
        .await
        .map_err(|_| "delegation binding lookup failed".to_string())??;
    let client = state
        .client_if_active()
        .await?
        .ok_or_else(|| "Select Personal to manage its delegation binding".to_string())?;
    client
        .set_delegation_binding(target)
        .await
        .map_err(|error| format!("set delegation binding: {error:#}"))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[tauri::command]
pub async fn embedded_clear_delegation_binding(
    state: tauri::State<'_, EmbeddedDaemonState>,
) -> Result<bool, String> {
    let client = state
        .client_if_active()
        .await?
        .ok_or_else(|| "Select Personal to manage its delegation binding".to_string())?;
    client
        .clear_delegation_binding()
        .await
        .map_err(|error| format!("clear delegation binding: {error:#}"))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
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

#[cfg(any(target_os = "ios", target_os = "android"))]
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
    let native_local = cfg!(target_os = "ios")
        && provider
            .eq_ignore_ascii_case(medousa::embedded_daemon::EMBEDDED_NATIVE_LOCAL_PROVIDER_ID);
    let base_url = if provider.eq_ignore_ascii_case("openai-codex") || native_local {
        None
    } else {
        crate::integration_secrets::load_connection_base_url(&provider)
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
            })
    };
    if native_local {
        medousa::embedded_daemon::validate_embedded_native_inference_route(
            model.clone(),
            base_url.clone(),
        )
        .map_err(|error| format!("configure embedded native inference: {error:#}"))?;
    } else {
        medousa::embedded_daemon::validate_credentialed_inference_route(
            provider.clone(),
            model.clone(),
            base_url.clone(),
        )
        .map_err(|error| format!("configure embedded daemon inference: {error:#}"))?;
    }
    Ok((provider, model, base_url))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(crate) fn normalize_inference_defaults(
    mut defaults: crate::medousa_paths::TuiDefaultsDto,
) -> Result<crate::medousa_paths::TuiDefaultsDto, String> {
    let (provider, model, _) = inference_route_from_defaults(&defaults)?;
    defaults.provider = Some(provider);
    defaults.model = Some(model);
    Ok(defaults)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn portable_tui_defaults(
    defaults: &crate::medousa_paths::TuiDefaultsDto,
) -> Result<medousa_types::session::TuiDefaults, String> {
    serde_json::from_value(crate::medousa_paths::tui_defaults_value_from_dto(defaults))
        .map_err(|error| format!("normalize Personal runtime settings: {error}"))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn boot_embedded_daemon(
    native_inference: Option<Arc<dyn EmbeddedNativeInference>>,
) -> Result<Arc<EmbeddedDaemon>, String> {
    let (installation_id, provider, model, base_url, defaults, data_dir, root) =
        tokio::task::spawn_blocking(|| {
            let installation_id = crate::integration_secrets::ensure_secrets_bootstrapped()?;
            let defaults = crate::medousa_paths::load_tui_defaults();
            let (provider, model, base_url) = inference_route_from_defaults(&defaults)?;
            let defaults = portable_tui_defaults(&defaults)?;
            let data_dir = crate::paths::medousa_data_dir();
            let root = data_dir.join("embedded-daemon");
            Ok::<_, String>((
                installation_id,
                provider,
                model,
                base_url,
                defaults,
                data_dir,
                root,
            ))
        })
        .await
        .map_err(|_| "embedded daemon configuration task failed".to_string())??;
    let chatgpt_oauth = Arc::new(ChatGptOAuthBroker::new(Arc::new(
        HomeChatGptCredentialStore,
    )));
    let mcp_oauth_store = medousa_mcp_gateway::SecureMcpOAuthBundleStore::new(data_dir)
        .map_err(|error| format!("initialize MCP OAuth storage: {error}"))?;
    let mcp_oauth = Arc::new(medousa_mcp_gateway::McpOAuthBroker::new(Arc::new(
        mcp_oauth_store,
    )));
    let config = EmbeddedDaemonConfig::credentialed_with_chatgpt_and_native(
        root,
        installation_id,
        provider,
        model,
        base_url,
        Arc::new(HomeCredentialProvider),
        chatgpt_oauth,
        native_inference,
    )
    .map_err(|error| format!("configure embedded daemon inference: {error:#}"))?
    .with_tui_defaults(defaults)
    .with_mcp_oauth(mcp_oauth)
    .with_tool_registry_recipe(Arc::new(
        medousa::mobile_tool_registry::PersonalMobileToolRegistryRecipe,
    ))
    .with_delegated_task_transport(Arc::new(HomeDelegatedTaskTransport));
    EmbeddedDaemon::boot(config)
        .await
        .map_err(|error| format!("boot embedded daemon: {error:#}"))
}

/// Native adapter for the daemon's transport port. The runtime supplies the
/// exact persisted binding; Home only resolves that route and authenticates it.
#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Debug)]
struct HomeDelegatedTaskTransport;

#[cfg(any(target_os = "ios", target_os = "android"))]
#[async_trait::async_trait]
impl DelegatedTaskTransport for HomeDelegatedTaskTransport {
    async fn authorized_targets(
        &self,
    ) -> Result<Vec<medousa::delegation::AuthorizedDelegationTarget>, DelegatedTaskError> {
        let targets = tokio::task::spawn_blocking(|| {
            let registry = crate::workshop_registry::ensure_migrated()?;
            Ok::<_, String>(
                registry
                    .workshops
                    .iter()
                    .filter(|workshop| {
                        workshop.id != crate::workshop_registry::PERSONAL_WORKSHOP_ID
                            && crate::workshop_registry::is_portal_kind(&workshop.kind)
                            && workshop.pairing.is_some()
                    })
                    .filter_map(|workshop| delegation_target_for(&workshop.id).ok())
                    .collect::<Vec<_>>(),
            )
        })
        .await
        .map_err(|_| DelegatedTaskError::transport("workshop inventory lookup failed"))?
        .map_err(DelegatedTaskError::transport)?;

        let mut authorized = Vec::new();
        for target in targets {
            if let Ok(candidate) = probe_delegation_target(target).await {
                if candidate.candidate.user_selectable {
                    authorized.push(candidate);
                }
            }
        }
        authorized.sort_by(|left, right| {
            left.candidate
                .label
                .to_ascii_lowercase()
                .cmp(&right.candidate.label.to_ascii_lowercase())
                .then(
                    left.candidate
                        .runtime_id
                        .cmp(&right.candidate.runtime_id),
                )
        });
        Ok(authorized)
    }

    async fn submit_or_observe(
        &self,
        target: &medousa::delegation::DelegationTarget,
        request: DelegatedTaskRequest,
    ) -> Result<DelegatedTaskObservation, DelegatedTaskError> {
        let target = target.clone();
        let config = tokio::task::spawn_blocking(move || delegation_transport_for(&target))
            .await
            .map_err(|_| DelegatedTaskError::transport("paired transport lookup failed"))?
            .map_err(DelegatedTaskError::transport)?;
        let wrapped = crate::mesh_envelope::wrap_payload_for_workshop(
            &config,
            crate::mesh_envelope::CAP_TASK_REQUEST,
            request.clone(),
        )
        .map_err(DelegatedTaskError::transport)?;
        let response: crate::mesh_envelope::MeshEnvelopedRequest<DelegatedTaskObservation> =
            crate::workshop_transport::workshop_post_json(&config, "/v1/mesh/tasks", &wrapped)
                .await
                .map_err(DelegatedTaskError::transport)?;
        crate::mesh_envelope::verify_payload_from_workshop(
            &config,
            &response,
            crate::mesh_envelope::CAP_TASK_RESULT,
        )
        .map_err(DelegatedTaskError::transport)?;
        let expected_work_id = delegated_work_id(
            &config.phone_id,
            request
                .grant
                .turn_id
                .as_deref()
                .ok_or_else(|| DelegatedTaskError::invalid("delegated turn id is missing"))?,
        );
        if response.payload.work_id != expected_work_id {
            return Err(DelegatedTaskError::transport(
                "delegated observation does not match the authenticated source identity",
            ));
        }
        if let Some(result) = response.payload.result.as_ref() {
            if result.terminal.participant_id.as_deref().map(str::trim)
                != Some(config.workshop_device_id.trim())
            {
                return Err(DelegatedTaskError::transport(
                    "delegated terminal participant does not match the authenticated workshop",
                ));
            }
        }
        Ok(response.payload)
    }

    async fn control(
        &self,
        target: &medousa::delegation::DelegationTarget,
        request: DelegatedTaskControlRequest,
    ) -> Result<DelegatedTaskControlObservation, DelegatedTaskError> {
        let target = target.clone();
        let config = tokio::task::spawn_blocking(move || delegation_transport_for(&target))
            .await
            .map_err(|_| DelegatedTaskError::transport("paired transport lookup failed"))?
            .map_err(DelegatedTaskError::transport)?;
        let wrapped = crate::mesh_envelope::wrap_payload_for_workshop(
            &config,
            crate::mesh_envelope::CAP_TASK_REQUEST,
            request.clone(),
        )
        .map_err(DelegatedTaskError::transport)?;
        let path = format!("/v1/mesh/tasks/{}/control", request.work_id);
        let response: crate::mesh_envelope::MeshEnvelopedRequest<DelegatedTaskControlObservation> =
            crate::workshop_transport::workshop_post_json(&config, &path, &wrapped)
                .await
                .map_err(DelegatedTaskError::transport)?;
        crate::mesh_envelope::verify_payload_from_workshop(
            &config,
            &response,
            crate::mesh_envelope::CAP_TASK_RESULT,
        )
        .map_err(DelegatedTaskError::transport)?;
        validate_task_control_observation(&request, &response.payload)?;
        if response.payload.destination_runtime_id.trim() != config.workshop_device_id.trim() {
            return Err(DelegatedTaskError::transport(
                "worker control response does not match the authenticated workshop",
            ));
        }
        Ok(response.payload)
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn probe_delegation_target(
    target: medousa::delegation::DelegationTarget,
) -> Result<medousa::delegation::AuthorizedDelegationTarget, DelegatedTaskError> {
    let transport_target = target.clone();
    let config = tokio::task::spawn_blocking(move || delegation_transport_for(&transport_target))
        .await
        .map_err(|_| DelegatedTaskError::transport("paired transport lookup failed"))?
        .map_err(DelegatedTaskError::transport)?;
    let request = medousa::workshop_contract::ExecutionTargetProbeRequest::default();
    let wrapped = crate::mesh_envelope::wrap_payload_for_workshop(
        &config,
        crate::mesh_envelope::CAP_TASK_REQUEST,
        request,
    )
    .map_err(DelegatedTaskError::transport)?;
    let response: crate::mesh_envelope::MeshEnvelopedRequest<
        medousa::workshop_contract::ExecutionTargetProbeResponse,
    > = crate::workshop_transport::workshop_post_json(
        &config,
        "/v1/mesh/execution-target",
        &wrapped,
    )
    .await
    .map_err(DelegatedTaskError::transport)?;
    crate::mesh_envelope::verify_payload_from_workshop(
        &config,
        &response,
        crate::mesh_envelope::CAP_TASK_RESULT,
    )
    .map_err(DelegatedTaskError::transport)?;
    if response.payload.schema_version
        != medousa::workshop_contract::EXECUTION_TARGET_INVENTORY_SCHEMA_VERSION
        || response.payload.target.runtime_id.trim() != target.peer_device_id.trim()
    {
        return Err(DelegatedTaskError::transport(
            "execution-target inventory does not match the paired workshop",
        ));
    }
    let candidate = medousa::workshop_contract::ExecutionTargetCandidate::from_inventory_entry(
        response.payload.target,
    );
    Ok(medousa::delegation::AuthorizedDelegationTarget {
        target,
        candidate,
        policy_revision: response.payload.policy_revision,
    })
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn delegation_transport_for(
    target: &medousa::delegation::DelegationTarget,
) -> Result<crate::pairing_client::WorkshopTransportConfig, String> {
    let registry = crate::workshop_registry::ensure_migrated()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|workshop| workshop.id == target.route_ref)
        .ok_or_else(|| "Bound delegation workshop no longer exists".to_string())?;
    if !crate::workshop_registry::is_portal_kind(&workshop.kind) {
        return Err("Bound delegation route is not a portal workshop".to_string());
    }
    let pairing = workshop
        .pairing
        .as_ref()
        .ok_or_else(|| "Bound delegation workshop is no longer paired".to_string())?;
    if pairing.workshop_device_id.trim() != target.peer_device_id.trim() {
        return Err("Bound delegation identity no longer matches the paired workshop".to_string());
    }
    let config =
        crate::pairing_client::load_workshop_transport_config_for_id(&workshop.id, &workshop.url)
            .ok_or_else(|| "Bound workshop transport credentials are unavailable".to_string())?;
    if config.workshop_device_id.trim() != target.peer_device_id.trim() {
        return Err("Stored transport identity does not match the delegation binding".to_string());
    }
    if config.session_token.is_none() {
        return Err("Bound workshop bearer credential is unavailable".to_string());
    }
    if config.daemon_public_key.is_none() {
        return Err(
            "Bound workshop identity is not pinned; pair it again before delegating work"
                .to_string(),
        );
    }
    Ok(config)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn delegation_target_for(
    workshop_id: &str,
) -> Result<medousa::delegation::DelegationTarget, String> {
    let registry = crate::workshop_registry::ensure_migrated()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|workshop| workshop.id == workshop_id)
        .ok_or_else(|| format!("Unknown workshop '{workshop_id}'"))?;
    if workshop.id == crate::workshop_registry::PERSONAL_WORKSHOP_ID
        || !crate::workshop_registry::is_portal_kind(&workshop.kind)
    {
        return Err("Delegation requires a paired portal workshop".to_string());
    }
    let pairing = workshop
        .pairing
        .as_ref()
        .ok_or_else(|| "Delegation workshop is not paired".to_string())?;
    let config =
        crate::pairing_client::load_workshop_transport_config_for_id(&workshop.id, &workshop.url)
            .ok_or_else(|| "Delegation workshop credentials are unavailable".to_string())?;
    if config.session_token.is_none() || config.daemon_public_key.is_none() {
        return Err(
            "Reconnect this workshop before delegation so its bearer and identity are pinned"
                .to_string(),
        );
    }
    if config.workshop_device_id.trim() != pairing.workshop_device_id.trim() {
        return Err("Workshop registry and pairing identity disagree".to_string());
    }
    Ok(medousa::delegation::DelegationTarget {
        route_ref: workshop.id.clone(),
        peer_device_id: pairing.workshop_device_id.clone(),
        label: Some(workshop.label.clone()),
    })
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Debug)]
struct HomeCredentialProvider;

#[cfg(any(target_os = "ios", target_os = "android"))]
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

#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Debug)]
struct HomeChatGptCredentialStore;

#[cfg(any(target_os = "ios", target_os = "android"))]
impl ChatGptCredentialStore for HomeChatGptCredentialStore {
    fn load_bundle(&self) -> Result<Option<String>, String> {
        Ok(crate::integration_secrets::load_kind_secret(
            "chatgpt",
            medousa_types::secrets::IntegrationSecretSlot::OauthBundle,
        ))
    }

    fn save_bundle(&self, bundle: Option<&str>) -> Result<(), String> {
        crate::integration_secrets::save_kind_secret(
            "chatgpt",
            medousa_types::secrets::IntegrationSecretSlot::OauthBundle,
            bundle,
        );
        Ok(())
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
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
