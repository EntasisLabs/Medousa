use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveActivityPayload {
    pub mood: String,
    pub workshop_name: String,
    pub eyebrow: String,
    pub headline: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion_summary: Option<String>,
    pub blocked_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_card_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveActivityDiagnostics {
    pub bridge_linked: bool,
    #[serde(default)]
    pub activities_enabled: bool,
    #[serde(default)]
    pub widget_extension_installed: bool,
    #[serde(default)]
    pub supports_live_activities: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveActivityStatus {
    pub available: bool,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<LiveActivityDiagnostics>,
}

const BRIDGE_MISSING: &str =
    "ActivityKit bridge not linked — delete the app, run npm run ios:prepare, then rebuild";

#[tauri::command]
pub fn live_activity_is_available() -> LiveActivityStatus {
    #[cfg(target_os = "ios")]
    {
        return ios::status_from_diagnostics(false);
    }

    #[cfg(not(target_os = "ios"))]
    LiveActivityStatus {
        available: false,
        active: false,
        error: Some("Live Activity is iOS-only".into()),
        diagnostics: None,
    }
}

#[tauri::command]
pub fn live_activity_sync(payload: LiveActivityPayload) -> Result<LiveActivityStatus, String> {
    sync_impl(payload)
}

fn sync_impl(payload: LiveActivityPayload) -> Result<LiveActivityStatus, String> {
    #[cfg(target_os = "ios")]
    {
        return ios::sync(payload);
    }

    #[cfg(not(target_os = "ios"))]
    {
        let _ = payload;
        Ok(LiveActivityStatus {
            available: false,
            active: false,
            error: Some("Live Activity is iOS-only".into()),
            diagnostics: None,
        })
    }
}

#[cfg(target_os = "ios")]
mod ios {
    use super::{
        LiveActivityDiagnostics, LiveActivityPayload, LiveActivityStatus, BRIDGE_MISSING,
    };
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    #[cfg(live_activity_native)]
    extern "C" {
        fn medousa_live_activity_bridge_version() -> u32;
        fn medousa_live_activity_diagnostics() -> *mut c_char;
        fn medousa_live_activity_is_available() -> bool;
        fn medousa_live_activity_sync(json: *const c_char) -> *mut c_char;
        fn medousa_live_activity_free_string(ptr: *mut c_char);
    }

    fn read_native_json(raw: *mut c_char) -> Option<String> {
        if raw.is_null() {
            return None;
        }
        let text = unsafe {
            let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
            medousa_live_activity_free_string(raw);
            text
        };
        Some(text)
    }

    fn bridge_linked() -> bool {
        #[cfg(live_activity_native)]
        {
            return unsafe { medousa_live_activity_bridge_version() > 0 };
        }
        #[cfg(not(live_activity_native))]
        {
            false
        }
    }

    fn fetch_diagnostics() -> LiveActivityDiagnostics {
        if !bridge_linked() {
            return LiveActivityDiagnostics {
                bridge_linked: false,
                activities_enabled: false,
                widget_extension_installed: false,
                supports_live_activities: false,
                error: Some(BRIDGE_MISSING.into()),
            };
        }

        #[cfg(live_activity_native)]
        {
            let raw = unsafe { medousa_live_activity_diagnostics() };
            let Some(text) = read_native_json(raw) else {
                return LiveActivityDiagnostics {
                    bridge_linked: true,
                    activities_enabled: false,
                    widget_extension_installed: false,
                    supports_live_activities: false,
                    error: Some("Live Activity diagnostics unavailable".into()),
                };
            };
            return serde_json::from_str(&text).unwrap_or(LiveActivityDiagnostics {
                bridge_linked: true,
                activities_enabled: false,
                widget_extension_installed: false,
                supports_live_activities: false,
                error: Some(format!("invalid diagnostics JSON: {text}")),
            });
        }

        #[cfg(not(live_activity_native))]
        LiveActivityDiagnostics {
            bridge_linked: false,
            activities_enabled: false,
            widget_extension_installed: false,
            supports_live_activities: false,
            error: Some(BRIDGE_MISSING.into()),
        }
    }

    fn explain_unavailable(diag: &LiveActivityDiagnostics) -> String {
        if !diag.bridge_linked {
            return diag
                .error
                .clone()
                .unwrap_or_else(|| BRIDGE_MISSING.into());
        }
        if !diag.supports_live_activities {
            return "App Info.plist missing NSSupportsLiveActivities — reinstall after rebuild"
                .into();
        }
        if !diag.widget_extension_installed {
            return "Widget extension not embedded — run npm run ios:prepare and reinstall"
                .into();
        }
        if !diag.activities_enabled {
            return "Live Activities disabled — iOS Settings → Medousa → Live Activities".into();
        }
        diag.error
            .clone()
            .unwrap_or_else(|| "Live Activity unavailable".into())
    }

    pub fn status_from_diagnostics(active: bool) -> LiveActivityStatus {
        let diagnostics = fetch_diagnostics();
        let available = diagnostics.bridge_linked
            && diagnostics.supports_live_activities
            && diagnostics.widget_extension_installed
            && diagnostics.activities_enabled;
        LiveActivityStatus {
            available,
            active,
            error: if available {
                None
            } else {
                Some(explain_unavailable(&diagnostics))
            },
            diagnostics: Some(diagnostics),
        }
    }

    pub fn sync(payload: LiveActivityPayload) -> Result<LiveActivityStatus, String> {
        if !bridge_linked() {
            return Ok(status_from_diagnostics(false));
        }

        let json = serde_json::to_string(&payload).map_err(|err| err.to_string())?;
        let c_json = CString::new(json).map_err(|_| "payload contained null byte".to_string())?;

        #[cfg(live_activity_native)]
        {
            let raw = unsafe { medousa_live_activity_sync(c_json.as_ptr()) };
            let Some(text) = read_native_json(raw) else {
                return Ok(LiveActivityStatus {
                    available: false,
                    active: false,
                    error: Some("Live Activity bridge returned null".into()),
                    diagnostics: Some(fetch_diagnostics()),
                });
            };

            let mut status: LiveActivityStatus =
                serde_json::from_str(&text).map_err(|err| format!("decode live activity status: {err}"))?;
            status.diagnostics = Some(fetch_diagnostics());
            return Ok(status);
        }

        #[cfg(not(live_activity_native))]
        {
            let _ = c_json;
            Ok(status_from_diagnostics(false))
        }
    }
}
