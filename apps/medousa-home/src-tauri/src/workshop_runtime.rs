//! Tauri-only LAN settings adapter; process policy lives in the shared host.
use crate::workshop_registry::PERSONAL_WORKSHOP_ID;
pub use medousa_desktop_host::workshop_runtime::*;
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanPairingStatus {
    pub enabled: bool,
    pub bind: String,
    pub url: String,
    pub message: String,
}

pub fn lan_pairing_status() -> Result<LanPairingStatus, String> {
    let registry = crate::workshop_registry::ensure_migrated()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == PERSONAL_WORKSHOP_ID)
        .ok_or_else(|| "Personal workshop not found".to_string())?;
    let bind = resolve_workshop_bind(workshop);
    let enabled = is_public_bind(&bind);
    Ok(LanPairingStatus {
        enabled,
        bind: bind.clone(),
        url: url_from_bind(&bind),
        message: if enabled {
            "LAN pairing is on — engine is reachable on your Wi‑Fi. Turn off when done pairing."
                .to_string()
        } else {
            "LAN pairing is off — engine is loopback-only. Paired phones/peers keep working over the private tunnel."
                .to_string()
        },
    })
}

/// Restart the personal engine with public bind (pairing window) or loopback-only.
pub async fn set_lan_pairing_enabled(
    state: &crate::daemon::DaemonState,
    enabled: bool,
) -> Result<LanPairingStatus, String> {
    let mut registry = crate::workshop_registry::ensure_migrated()?;
    let workshop = registry
        .workshops
        .iter_mut()
        .find(|entry| entry.id == PERSONAL_WORKSHOP_ID)
        .ok_or_else(|| "Personal workshop not found".to_string())?;

    let port =
        parse_bind_port(workshop.bind.as_deref().unwrap_or(DEFAULT_LOCAL_BIND)).unwrap_or(7419);
    let bind = if enabled {
        format!("0.0.0.0:{port}")
    } else {
        format!("127.0.0.1:{port}")
    };
    workshop.bind = Some(bind.clone());
    workshop.url = url_from_bind(&bind);
    workshop.updated_at = crate::workshop_registry::now_iso();
    crate::workshop_registry::save_registry(&registry)?;

    stop_local_engine(PERSONAL_WORKSHOP_ID);
    tokio::time::sleep(Duration::from_millis(750)).await;

    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == PERSONAL_WORKSHOP_ID)
        .ok_or_else(|| "Personal workshop not found".to_string())?;
    let result = ensure_local_engine(workshop, false).await?;
    if !result.ok {
        return Err(result.message);
    }

    let local_url = url_from_bind(&bind);
    crate::daemon::apply_daemon_url(state, &local_url)?;
    crate::workshop_transport::invalidate_all_route_caches();

    Ok(LanPairingStatus {
        enabled,
        bind: bind.clone(),
        url: local_url,
        message: if enabled {
            "Engine restarted with LAN pairing on. Pair phones and peers, then turn this off."
                .to_string()
        } else {
            "Engine restarted on loopback only. Remote clients use the private tunnel.".to_string()
        },
    })
}
