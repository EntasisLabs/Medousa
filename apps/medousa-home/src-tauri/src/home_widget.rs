use serde::{Deserialize, Serialize};

use crate::live_activity::LiveActivityPayload;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeWidgetSyncResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

const BRIDGE_MISSING: &str =
    "Widget bridge not linked — delete the app, run npm run ios:prepare, then rebuild";

#[tauri::command]
pub fn home_widget_sync(payload: LiveActivityPayload) -> Result<HomeWidgetSyncResult, String> {
    sync_impl(payload)
}

fn sync_impl(payload: LiveActivityPayload) -> Result<HomeWidgetSyncResult, String> {
    #[cfg(target_os = "ios")]
    {
        return ios::sync(payload);
    }

    #[cfg(not(target_os = "ios"))]
    {
        let _ = payload;
        Ok(HomeWidgetSyncResult {
            ok: false,
            error: Some("Home widget is iOS-only".into()),
        })
    }
}

#[cfg(target_os = "ios")]
mod ios {
    use super::{HomeWidgetSyncResult, BRIDGE_MISSING};
    use crate::live_activity::LiveActivityPayload;
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    #[cfg(live_activity_native)]
    extern "C" {
        fn medousa_live_activity_bridge_version() -> u32;
        fn medousa_home_widget_sync(json: *const c_char) -> *mut c_char;
        fn medousa_live_activity_free_string(ptr: *mut c_char);
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

    pub fn sync(payload: LiveActivityPayload) -> Result<HomeWidgetSyncResult, String> {
        if !bridge_linked() {
            return Ok(HomeWidgetSyncResult {
                ok: false,
                error: Some(BRIDGE_MISSING.into()),
            });
        }

        let json = serde_json::to_string(&payload).map_err(|err| err.to_string())?;
        let c_json = CString::new(json).map_err(|_| "payload contained null byte".to_string())?;

        #[cfg(live_activity_native)]
        {
            let raw = unsafe { medousa_home_widget_sync(c_json.as_ptr()) };
            let Some(text) = read_native_json(raw) else {
                return Ok(HomeWidgetSyncResult {
                    ok: false,
                    error: Some("Home widget bridge returned null".into()),
                });
            };
            return serde_json::from_str(&text)
                .map_err(|err| format!("decode home widget status: {err}"));
        }

        #[cfg(not(live_activity_native))]
        {
            let _ = c_json;
            Ok(HomeWidgetSyncResult {
                ok: false,
                error: Some(BRIDGE_MISSING.into()),
            })
        }
    }
}
