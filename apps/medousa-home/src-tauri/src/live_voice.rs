use serde::{Deserialize, Serialize};

const OPENAI_LIVE_SESSIONS_URL: &str = "https://api.openai.com/v1/live/sessions";
const MAX_SDP_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LiveVoiceStatus {
    pub available: bool,
    pub active: bool,
    pub muted: bool,
    pub phase: String,
    pub workshop_name: Option<String>,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSessionAnswer {
    live_session_id: String,
    sdp: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiLiveResponse {
    session: OpenAiLiveSession,
    transport: OpenAiLiveTransport,
}

#[derive(Debug, Deserialize)]
struct OpenAiLiveSession {
    id: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiLiveTransport {
    sdp: String,
}

#[tauri::command]
pub async fn live_voice_create_session(
    sdp: String,
    session_id: String,
) -> Result<LiveSessionAnswer, String> {
    let sdp = sdp.trim();
    if sdp.is_empty() || session_id.trim().is_empty() {
        return Err("sdp and sessionId are required".into());
    }
    if sdp.len() > MAX_SDP_BYTES {
        return Err("The Live audio offer is too large".into());
    }
    if !sdp.starts_with("v=0") {
        return Err("The iPhone created an invalid Live audio offer".into());
    }

    let api_key =
        tokio::task::spawn_blocking(|| crate::integration_secrets::load_provider_secret("openai"))
            .await
            .map_err(|_| "Could not read the iPhone's OpenAI credential".to_string())?
            .ok_or_else(|| {
                "Configure an OpenAI API key on this iPhone to use Medousa Live".to_string()
            })?;

    let response = reqwest::Client::new()
        .post(OPENAI_LIVE_SESSIONS_URL)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "session": {
                "model": "gpt-live-1",
                "instructions": "You are Medousa's live voice. Be concise and conversational. Delegate requests that need tools or durable work to the Medousa application."
            },
            "transport": {
                "type": "webrtc",
                "sdp": sdp
            }
        }))
        .send()
        .await
        .map_err(|error| format!("Could not reach OpenAI Live from this iPhone: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "OpenAI rejected the API key stored on this iPhone".into(),
            429 => "OpenAI Live is temporarily rate limited".into(),
            code => format!("OpenAI Live could not start the session (HTTP {code})"),
        });
    }

    let decoded: OpenAiLiveResponse = response
        .json()
        .await
        .map_err(|_| "OpenAI Live returned an invalid session response".to_string())?;
    if decoded.session.id.trim().is_empty() || decoded.transport.sdp.trim().is_empty() {
        return Err("OpenAI Live returned an incomplete session response".into());
    }
    Ok(LiveSessionAnswer {
        live_session_id: decoded.session.id,
        sdp: decoded.transport.sdp,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveVoiceStartRequest<'a> {
    workshop_name: &'a str,
    session_id: &'a str,
}

#[tauri::command]
pub fn live_voice_start(
    workshop_name: String,
    session_id: String,
) -> Result<LiveVoiceStatus, String> {
    let workshop_name = workshop_name.trim();
    let session_id = session_id.trim();
    if workshop_name.is_empty() || session_id.is_empty() {
        return Err("workshopName and sessionId are required".into());
    }
    ios::start(LiveVoiceStartRequest {
        workshop_name,
        session_id,
    })
}

#[tauri::command]
pub fn live_voice_set_muted(muted: bool) -> Result<LiveVoiceStatus, String> {
    ios::set_muted(muted)
}

#[tauri::command]
pub fn live_voice_stop() -> Result<LiveVoiceStatus, String> {
    ios::stop()
}

#[tauri::command]
pub fn live_voice_status() -> Result<LiveVoiceStatus, String> {
    ios::status()
}

#[cfg(target_os = "ios")]
mod ios {
    use super::{LiveVoiceStartRequest, LiveVoiceStatus};
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    extern "C" {
        fn medousa_live_voice_start(json: *const c_char) -> *mut c_char;
        fn medousa_live_voice_set_muted(muted: bool) -> *mut c_char;
        fn medousa_live_voice_stop() -> *mut c_char;
        fn medousa_live_voice_status() -> *mut c_char;
        fn medousa_live_activity_free_string(ptr: *mut c_char);
    }

    fn decode(raw: *mut c_char) -> Result<LiveVoiceStatus, String> {
        if raw.is_null() {
            return Err("Medousa Live native bridge returned null".into());
        }
        let json = unsafe {
            let json = CStr::from_ptr(raw).to_string_lossy().into_owned();
            medousa_live_activity_free_string(raw);
            json
        };
        serde_json::from_str(&json).map_err(|error| format!("decode Medousa Live status: {error}"))
    }

    pub fn start(request: LiveVoiceStartRequest<'_>) -> Result<LiveVoiceStatus, String> {
        let json = serde_json::to_string(&request).map_err(|error| error.to_string())?;
        let json = CString::new(json).map_err(|_| "voice request contained a null byte")?;
        decode(unsafe { medousa_live_voice_start(json.as_ptr()) })
    }

    pub fn set_muted(muted: bool) -> Result<LiveVoiceStatus, String> {
        decode(unsafe { medousa_live_voice_set_muted(muted) })
    }

    pub fn stop() -> Result<LiveVoiceStatus, String> {
        decode(unsafe { medousa_live_voice_stop() })
    }

    pub fn status() -> Result<LiveVoiceStatus, String> {
        decode(unsafe { medousa_live_voice_status() })
    }
}
