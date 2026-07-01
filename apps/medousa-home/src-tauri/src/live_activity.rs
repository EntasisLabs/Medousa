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
pub struct LiveActivityStatus {
    pub available: bool,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[tauri::command]
pub fn live_activity_is_available() -> LiveActivityStatus {
    #[cfg(target_os = "ios")]
    {
        let available = unsafe { ios::native_is_available() };
        return LiveActivityStatus {
            available,
            active: false,
            error: if available {
                None
            } else {
                Some(
                    "Live Activities disabled in iOS Settings or Widget Extension not installed"
                        .into(),
                )
            },
        };
    }

    #[cfg(not(target_os = "ios"))]
    LiveActivityStatus {
        available: false,
        active: false,
        error: Some("Live Activity is iOS-only".into()),
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
        })
    }
}

#[cfg(all(target_os = "ios", live_activity_native))]
mod ios {
    use super::{LiveActivityPayload, LiveActivityStatus};
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    extern "C" {
        fn medousa_live_activity_is_available() -> bool;
        fn medousa_live_activity_sync(json: *const c_char) -> *mut c_char;
        fn medousa_live_activity_free_string(ptr: *mut c_char);
    }

    pub unsafe fn native_is_available() -> bool {
        medousa_live_activity_is_available()
    }

    pub fn sync(payload: LiveActivityPayload) -> Result<LiveActivityStatus, String> {
        let json = serde_json::to_string(&payload).map_err(|err| err.to_string())?;
        let c_json = CString::new(json).map_err(|_| "payload contained null byte".to_string())?;

        let available = unsafe { medousa_live_activity_is_available() };
        if !available && payload.mood == "quiet" {
            return Ok(LiveActivityStatus {
                available: false,
                active: false,
                error: Some(
                    "Live Activities disabled in iOS Settings or Widget Extension not installed"
                        .into(),
                ),
            });
        }

        let raw = unsafe { medousa_live_activity_sync(c_json.as_ptr()) };
        if raw.is_null() {
            return Ok(LiveActivityStatus {
                available,
                active: false,
                error: Some("Live Activity bridge returned null".into()),
            });
        }

        let result = unsafe {
            let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
            medousa_live_activity_free_string(raw);
            text
        };

        serde_json::from_str(&result).map_err(|err| format!("decode live activity status: {err}"))
    }
}

#[cfg(all(target_os = "ios", not(live_activity_native)))]
mod ios {
    use super::{LiveActivityPayload, LiveActivityStatus};

    pub unsafe fn native_is_available() -> bool {
        false
    }

    pub fn sync(payload: LiveActivityPayload) -> Result<LiveActivityStatus, String> {
        let _ = payload;
        Ok(LiveActivityStatus {
            available: false,
            active: false,
            error: Some(
                "Live Activity native bridge not built (run on macOS with Xcode)".into(),
            ),
        })
    }
}
