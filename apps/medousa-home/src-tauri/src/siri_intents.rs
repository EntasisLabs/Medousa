use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSiriAsk {
    pub request_id: String,
    pub prompt: String,
    pub workshop_id: String,
    pub created_at: f64,
}

#[cfg(target_os = "ios")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SiriWorkshopSummary {
    id: String,
    name: String,
    is_active: bool,
}

#[tauri::command]
pub fn siri_sync_workshop_snapshot() -> Result<(), String> {
    #[cfg(target_os = "ios")]
    {
        let registry = crate::workshop_registry::ensure_migrated()?;
        let active_workshop_id = registry.active_workshop_id.clone();
        let summaries = registry
            .workshops
            .into_iter()
            .filter(|workshop| crate::workshop_registry::is_portal_kind(&workshop.kind))
            .map(|workshop| SiriWorkshopSummary {
                is_active: workshop.id == active_workshop_id,
                id: workshop.id,
                name: workshop.label,
            })
            .collect::<Vec<_>>();
        return ios::store_workshops(&summaries);
    }

    #[cfg(not(target_os = "ios"))]
    Ok(())
}

#[tauri::command]
pub fn siri_consume_pending_ask(request_id: String) -> Result<PendingSiriAsk, String> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 64 {
        return Err("Invalid Siri request receipt".into());
    }

    #[cfg(target_os = "ios")]
    {
        return ios::consume(request_id);
    }

    #[cfg(not(target_os = "ios"))]
    Err("Siri requests are only available on iOS".into())
}

#[cfg(target_os = "ios")]
mod ios {
    use super::PendingSiriAsk;
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    #[cfg(live_activity_native)]
    unsafe extern "C" {
        fn medousa_siri_consume_pending_ask(request_id: *const c_char) -> *mut c_char;
        fn medousa_siri_store_workshops(json: *const c_char) -> bool;
        fn medousa_live_activity_free_string(ptr: *mut c_char);
    }

    pub fn consume(request_id: &str) -> Result<PendingSiriAsk, String> {
        #[cfg(live_activity_native)]
        {
            let request_id =
                CString::new(request_id).map_err(|_| "Invalid Siri request receipt".to_string())?;
            let raw = unsafe { medousa_siri_consume_pending_ask(request_id.as_ptr()) };
            if raw.is_null() {
                return Err("Siri request expired or was already consumed".into());
            }
            let encoded = unsafe {
                let encoded = CStr::from_ptr(raw).to_string_lossy().into_owned();
                medousa_live_activity_free_string(raw);
                encoded
            };
            let pending: PendingSiriAsk = serde_json::from_str(&encoded)
                .map_err(|error| format!("Invalid Siri request payload: {error}"))?;
            let prompt = pending.prompt.trim();
            if pending.request_id != request_id.to_string_lossy()
                || prompt.is_empty()
                || prompt.len() > 4_000
            {
                return Err("Invalid Siri request payload".into());
            }
            return Ok(PendingSiriAsk {
                prompt: prompt.to_string(),
                ..pending
            });
        }

        #[cfg(not(live_activity_native))]
        Err("Siri native bridge is unavailable".into())
    }

    pub fn store_workshops(summaries: &[super::SiriWorkshopSummary]) -> Result<(), String> {
        #[cfg(live_activity_native)]
        {
            let encoded = serde_json::to_string(summaries).map_err(|error| error.to_string())?;
            let encoded = CString::new(encoded)
                .map_err(|_| "Workshop snapshot contained a null byte".to_string())?;
            if unsafe { medousa_siri_store_workshops(encoded.as_ptr()) } {
                return Ok(());
            }
            return Err("Could not store the Siri workshop snapshot".into());
        }

        #[cfg(not(live_activity_native))]
        Err("Siri native bridge is unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_receipt_before_platform_dispatch() {
        assert!(siri_consume_pending_ask("".into()).is_err());
        assert!(siri_consume_pending_ask("x".repeat(65)).is_err());
    }
}
