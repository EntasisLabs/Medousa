//! Resolve selected and retained workshop authority.
//!
//! The main shell follows the registry's selected workshop. Native work windows
//! retain an explicit registry identity without changing that selection. Both
//! resolve exact local/paired transport credentials; mobile Personal stays
//! in-process. Client registry identities are never execution runtime IDs.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri::Manager;
use crate::daemon::types::DEFAULT_DAEMON_URL;
use crate::pairing_client::WorkshopTransportConfig;
use crate::workshop_registry::{
    ensure_migrated, is_peer_kind, WorkshopRegistry, WorkshopServer, PERSONAL_WORKSHOP_ID,
};
use crate::workshop_runtime::{resolve_workshop_data_dir, resolve_workshop_url};

#[derive(Debug, Clone)]
pub enum ActiveWorkshopTarget {
    EmbeddedPersonal,
    Transport {
        workshop: WorkshopServer,
        base_url: String,
    },
}

impl ActiveWorkshopTarget {
    pub fn display_url(&self) -> &str {
        match self {
            Self::EmbeddedPersonal => DEFAULT_DAEMON_URL,
            Self::Transport { base_url, .. } => base_url,
        }
    }
}

fn resolve_for_registry(
    registry: &WorkshopRegistry,
    embedded_personal: bool,
) -> Result<ActiveWorkshopTarget, String> {
    resolve_scope_for_registry(registry, &registry.active_workshop_id, embedded_personal)
}

fn resolve_scope_for_registry(
    registry: &WorkshopRegistry,
    workshop_id: &str,
    embedded_personal: bool,
) -> Result<ActiveWorkshopTarget, String> {
    let workshop = registry
        .workshops
        .iter()
        .find(|workshop| workshop.id == workshop_id)
        .cloned()
        .ok_or_else(|| "Workshop is no longer available on this device".to_string())?;

    if is_peer_kind(&workshop.kind) {
        return Err("Peer connections cannot be selected as Home's active workshop".to_string());
    }

    if embedded_personal && workshop.id == PERSONAL_WORKSHOP_ID && workshop.kind == "local" {
        return Ok(ActiveWorkshopTarget::EmbeddedPersonal);
    }

    if !matches!(workshop.kind.as_str(), "local" | "portal" | "paired") {
        return Err(format!(
            "Unsupported active workshop kind '{}'",
            workshop.kind
        ));
    }

    let base_url = resolve_workshop_url(&workshop)
        .trim()
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return Err("Active workshop address is empty".to_string());
    }
    Ok(ActiveWorkshopTarget::Transport { workshop, base_url })
}

pub fn resolve() -> Result<ActiveWorkshopTarget, String> {
    let registry = ensure_migrated()?;
    resolve_for_registry(
        &registry,
        cfg!(any(target_os = "ios", target_os = "android")),
    )
}

/// Resolve a retained view's client workshop scope without changing the active
/// selection. A registry id is not a daemon execution runtime id.
pub fn resolve_scope(workshop_id: &str) -> Result<ActiveWorkshopTarget, String> {
    resolve_scope_for_registry(
        &ensure_migrated()?,
        workshop_id.trim(),
        cfg!(any(target_os = "ios", target_os = "android")),
    )
}

async fn metadata_read<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
    static EXECUTION: std::sync::OnceLock<ForgeExecutionService> = std::sync::OnceLock::new();
    EXECUTION
        .get_or_init(ForgeExecutionService::new)
        .run(ExecutionClass::Observation, 64 * 1024, move || Ok(work()))
        .await
        .map_err(|error| error.to_string())?
}

pub async fn resolve_async() -> Result<ActiveWorkshopTarget, String> {
    metadata_read(resolve).await
}

pub async fn resolve_scope_async(workshop_id: String) -> Result<ActiveWorkshopTarget, String> {
    metadata_read(move || resolve_scope(&workshop_id)).await
}

pub async fn transport_for_target_async(
    target: ActiveWorkshopTarget,
) -> Result<WorkshopTransportConfig, String> {
    metadata_read(move || transport_config_for_target(target)).await
}

pub fn consumer_generation(window: &tauri::Webview) -> Option<u64> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        crate::desktop_windows::generation_for(window.app_handle(), window.label())
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = window;
        None
    }
}

/// Check under the consumer-registry lock. No native UI query is made while
/// holding that lock; the destruction callback uses the same registry.
pub fn ensure_window_consumer(
    window: &tauri::Webview,
    generation: Option<u64>,
) -> Result<(), String> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if crate::desktop_windows::is_resource_window(window.label())
        && (generation.is_none() || generation != consumer_generation(window))
    {
        return Err("The work window was closed".into());
    }
    let _ = (window, generation);
    Ok(())
}

/// Accept the invoking webview even when its native window hosts browser children.
/// Retained desktop views have immutable workshop ownership. Other windows use
/// the normal selected workshop; an explicit execution destination wins.
pub async fn transport_for_webview(
    window: &tauri::Webview,
    execution_runtime_id: Option<&str>,
) -> Result<WorkshopTransportConfig, String> {
    if let Some(id) = execution_runtime_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return transport_config_for_runtime_id(id);
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if crate::desktop_windows::is_resource_window(window.label()) {
        let scope = crate::desktop_windows::scope_for(window.app_handle(), window.label())
            .ok_or("The work window was closed")?;
        return metadata_read(move || transport_config_for_target(resolve_scope(&scope)?)).await;
    }
    let _ = window;
    transport_config()
}

pub fn display_url() -> Result<String, String> {
    Ok(resolve()?.display_url().to_string())
}

pub fn display_url_for_webview(window: &tauri::Webview) -> Result<String, String> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if let Some(scope) = crate::desktop_windows::scope_for(window.app_handle(), window.label()) {
        return Ok(resolve_scope(&scope)?.display_url().to_string());
    }
    let _ = window;
    display_url()
}

pub fn personal_transport_config() -> Result<WorkshopTransportConfig, String> {
    let registry = ensure_migrated()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|workshop| workshop.id == PERSONAL_WORKSHOP_ID)
        .ok_or_else(|| "Personal workshop is missing".to_string())?;
    if workshop.kind != "local" {
        return Err("Personal workshop is not a local engine".to_string());
    }
    let session_token =
        medousa_local_credential::load_home_local_secret(&resolve_workshop_data_dir(workshop))
            .map_err(|error| format!("load Personal engine credential: {error}"))?
            .token()
            .to_string();
    Ok(WorkshopTransportConfig {
        lan_base: resolve_workshop_url(workshop)
            .trim()
            .trim_end_matches('/')
            .to_string(),
        iroh_ticket: None,
        session_token: Some(session_token),
        pairing_id: String::new(),
        session_expires_at: None,
        phone_id: String::new(),
        workshop_device_id: String::new(),
        daemon_public_key: None,
    })
}

pub fn transport_config() -> Result<WorkshopTransportConfig, String> {
    transport_config_for_target(resolve()?)
}

pub fn transport_config_for_target(
    target: ActiveWorkshopTarget,
) -> Result<WorkshopTransportConfig, String> {
    match target {
        ActiveWorkshopTarget::EmbeddedPersonal => Err(
            "Personal is routed through the embedded daemon; remote transport is unavailable"
                .to_string(),
        ),
        ActiveWorkshopTarget::Transport { workshop, base_url } => {
            if workshop.kind == "local" {
                let session_token = medousa_local_credential::load_home_local_secret(
                    &resolve_workshop_data_dir(&workshop),
                )
                .ok()
                .map(|secret| secret.token().to_string());
                return Ok(WorkshopTransportConfig {
                    lan_base: base_url,
                    iroh_ticket: None,
                    session_token,
                    pairing_id: String::new(),
                    session_expires_at: None,
                    phone_id: String::new(),
                    workshop_device_id: String::new(),
                    daemon_public_key: None,
                });
            }

            let config = crate::pairing_client::load_workshop_transport_config_for_id(
                &workshop.id,
                &base_url,
            )
            .ok_or_else(|| {
                format!(
                    "Selected workshop '{}' has no pairing credentials; pair it again",
                    workshop.label
                )
            })?;
            if config
                .session_token
                .as_deref()
                .is_none_or(|token| token.trim().is_empty())
            {
                return Err(format!(
                    "Selected workshop '{}' has no authenticated session; pair it again",
                    workshop.label
                ));
            }
            Ok(config)
        }
    }
}

/// Resolve an exact paired workshop by its daemon-authored runtime identity.
/// This never treats a URL or client-side workshop id as execution authority.
pub fn transport_config_for_runtime_id(
    runtime_id: &str,
) -> Result<WorkshopTransportConfig, String> {
    let runtime_id = runtime_id.trim();
    if runtime_id.is_empty() {
        return Err("Execution runtime id is required".to_string());
    }
    let registry = ensure_migrated()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|workshop| {
            workshop
                .pairing
                .as_ref()
                .is_some_and(|pairing| pairing.workshop_device_id.trim() == runtime_id)
        })
        .ok_or_else(|| {
            format!("The selected execution workshop '{runtime_id}' is not paired on this device")
        })?;
    if !matches!(workshop.kind.as_str(), "portal" | "paired") {
        return Err("Selected execution runtime is not a paired workshop".to_string());
    }
    let config = crate::pairing_client::load_workshop_transport_config_for_id(
        &workshop.id,
        &resolve_workshop_url(workshop),
    )
    .ok_or_else(|| {
        format!(
            "Selected workshop '{}' has no pairing credentials; pair it again",
            workshop.label
        )
    })?;
    if config.workshop_device_id.trim() != runtime_id {
        return Err("Selected workshop transport identity changed; pair it again".to_string());
    }
    if config
        .session_token
        .as_deref()
        .is_none_or(|token| token.trim().is_empty())
    {
        return Err(format!(
            "Selected workshop '{}' has no authenticated session; pair it again",
            workshop.label
        ));
    }
    Ok(config)
}

pub fn transport_base_url() -> Result<String, String> {
    Ok(transport_config()?.lan_base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workshop_registry::{default_registry, now_iso, WorkshopPairingRef};

    fn remote_canary(id: &str, url: &str) -> WorkshopServer {
        let now = now_iso();
        WorkshopServer {
            id: id.to_string(),
            label: "Remote canary".to_string(),
            kind: "portal".to_string(),
            url: url.to_string(),
            icon: None,
            created_at: now.clone(),
            updated_at: now.clone(),
            last_connected_at: None,
            brand_color: None,
            tagline: None,
            data_dir: None,
            bind: None,
            pairing: Some(WorkshopPairingRef {
                pairing_id: "pair-canary".to_string(),
                phone_id: "phone-canary".to_string(),
                workshop_device_id: "daemon-canary".to_string(),
                paired_at: now,
                credentials_rel_path: None,
                has_iroh_ticket: Some(true),
                workshop_peer_name: Some("Remote canary".to_string()),
            }),
            client_state: None,
        }
    }

    #[test]
    fn embedded_personal_ignores_a_poisoned_remote_url() {
        let mut registry = default_registry();
        registry.workshops[0].url = "https://remote-canary.invalid".to_string();
        registry
            .workshops
            .push(remote_canary("paired-remote", "https://paired.invalid"));

        let target = resolve_for_registry(&registry, true).unwrap();
        assert!(matches!(&target, ActiveWorkshopTarget::EmbeddedPersonal));
        assert_eq!(target.display_url(), DEFAULT_DAEMON_URL);
    }

    #[test]
    fn selected_portal_keeps_its_exact_id_and_address() {
        let mut registry = default_registry();
        registry
            .workshops
            .push(remote_canary("paired-remote", "https://paired.invalid/"));
        registry.active_workshop_id = "paired-remote".to_string();

        let target = resolve_for_registry(&registry, true).unwrap();
        match target {
            ActiveWorkshopTarget::Transport { workshop, base_url } => {
                assert_eq!(workshop.id, "paired-remote");
                assert_eq!(base_url, "https://paired.invalid");
            }
            ActiveWorkshopTarget::EmbeddedPersonal => {
                panic!("selected portal must not resolve to embedded Personal")
            }
        }
    }

    #[test]
    fn desktop_personal_remains_the_local_daemon_transport() {
        let registry = default_registry();
        assert!(matches!(
            resolve_for_registry(&registry, false).unwrap(),
            ActiveWorkshopTarget::Transport { workshop, .. }
                if workshop.id == PERSONAL_WORKSHOP_ID
        ));
    }

    #[test]
    fn retained_scope_resolves_original_workshop_after_active_selection_changes() {
        let mut registry = default_registry();
        registry
            .workshops
            .push(remote_canary("paired-remote", "https://paired.invalid"));
        registry.active_workshop_id = "paired-remote".into();
        let target = resolve_scope_for_registry(&registry, PERSONAL_WORKSHOP_ID, false).unwrap();
        assert!(
            matches!(target, ActiveWorkshopTarget::Transport { workshop, .. }
            if workshop.id == PERSONAL_WORKSHOP_ID && workshop.kind == "local")
        );
        assert_eq!(registry.active_workshop_id, "paired-remote");
    }

    #[test]
    fn client_workshop_scope_is_distinct_from_daemon_runtime_identity() {
        let mut registry = default_registry();
        registry
            .workshops
            .push(remote_canary("paired-remote", "https://paired.invalid"));
        assert!(resolve_scope_for_registry(&registry, "daemon-canary", false).is_err());
        let target = resolve_scope_for_registry(&registry, "paired-remote", false).unwrap();
        assert!(
            matches!(target, ActiveWorkshopTarget::Transport { workshop, .. }
            if workshop.id == "paired-remote")
        );
    }

    #[test]
    fn scoped_requests_cannot_fall_back_from_missing_workshops_or_use_peers() {
        let mut registry = default_registry();
        let mut peer = remote_canary("peer", "https://peer.invalid");
        peer.kind = "peer".into();
        registry.workshops.push(peer);
        assert!(resolve_scope_for_registry(&registry, "missing", false).is_err());
        assert!(resolve_scope_for_registry(&registry, "peer", false).is_err());
    }
}

/// Retained renderers keep their own workshop subscriptions across shell switches.
pub fn is_resource_window(label: &str) -> bool {
    label == "desktop-spotlight" || label.starts_with("desktop-work-")
}
