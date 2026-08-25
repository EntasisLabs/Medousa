//! Resolve the one daemon Home is allowed to address.
//!
//! The registry is a client-side list of known workshops. Only its explicit
//! active selection is routing authority: iOS Personal is in-process, while a
//! selected local desktop or paired portal is reached through its own exact
//! transport credentials.

use crate::daemon::types::DEFAULT_DAEMON_URL;
use crate::pairing_client::WorkshopTransportConfig;
use crate::workshop_registry::{
    PERSONAL_WORKSHOP_ID, WorkshopRegistry, WorkshopServer, active_workshop, ensure_migrated,
    is_peer_kind,
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
    let workshop = active_workshop(registry)
        .cloned()
        .ok_or_else(|| "No active workshop in registry".to_string())?;

    if is_peer_kind(&workshop.kind) {
        return Err("Peer connections cannot be selected as Home's active workshop".to_string());
    }

    if embedded_personal
        && workshop.id == PERSONAL_WORKSHOP_ID
        && workshop.kind == "local"
    {
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
    resolve_for_registry(&registry, cfg!(target_os = "ios"))
}

pub fn display_url() -> Result<String, String> {
    Ok(resolve()?.display_url().to_string())
}

pub fn transport_config() -> Result<WorkshopTransportConfig, String> {
    match resolve()? {
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
                    phone_id: String::new(),
                    workshop_device_id: String::new(),
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

pub fn transport_base_url() -> Result<String, String> {
    Ok(transport_config()?.lan_base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workshop_registry::{WorkshopPairingRef, default_registry, now_iso};

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
}
