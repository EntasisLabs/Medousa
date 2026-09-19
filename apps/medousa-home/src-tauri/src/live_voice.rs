use serde::{Deserialize, Serialize};

const OPENAI_REALTIME_CALLS_URL: &str = "https://api.openai.com/v1/realtime/calls";
const OPENAI_LIVE_SESSIONS_URL: &str = "https://api.openai.com/v1/live/sessions";
const MAX_SDP_BYTES: usize = 256 * 1024;
const PERSONAL_CONTEXT_HANDOFF: &str = "\nBackend tools: the workshop can look up permitted saved memory, conversation history, journal or vault notes, and connected sources when configured.\nDelegate to the backend when: the user asks to recall or summarize their personal activity, work, decisions, plans, or progress beyond facts explicitly available in this conversation. Examples: 'what I've been up to this week', 'what have I been working on lately?', 'what did we decide last time?', and 'summarize my week'. These are personal-context retrieval requests, not small talk. Delegate before answering; the small seeded history is not a complete activity log. Do not guess, claim you have no memory, or ask the user to recount their week before the backend checks its permitted sources. If the backend finds insufficient information, say so honestly. A new time range or topic requires a new lookup even after an earlier result.\nDo not delegate to the backend when: the user is simply telling you about their week, sharing feelings, greeting you, thanking you, or asking to repeat or explain a verified result already provided for that same scope. Respond naturally to those conversational turns.";

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn live_voice_append_transcript(
    embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    session_id: String,
    live_session_id: String,
    item_id: String,
    role: String,
    text: String,
    attachment: Option<bool>,
    target_turn_id: Option<String>,
) -> Result<(), String> {
    let client = embedded_state.client_if_active().await?
        .ok_or_else(|| "Live transcript persistence requires the Personal workshop".to_string())?;
    client.append_live_transcript(&session_id, &live_session_id, &item_id, &role, &text, attachment.unwrap_or(false), target_turn_id.as_deref())
        .await.map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LiveVoiceStatus {
    pub available: bool,
    pub active: bool,
    pub muted: bool,
    pub phase: String,
    pub workshop_name: Option<String>,
    pub session_id: Option<String>,
    pub live_session_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSessionAnswer {
    protocol: &'static str,
    live_session_id: String,
    sdp: String,
    seed_history: Vec<LiveSeedMessage>,
}

#[derive(Debug, Serialize)]
struct LiveSeedMessage {
    role: String,
    content: String,
}

#[tauri::command]
pub async fn live_voice_create_session(
    embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    sdp: String,
    session_id: String,
    workshop_name: String,
    protocol: Option<String>,
) -> Result<LiveSessionAnswer, String> {
    let validated_sdp = sdp.trim();
    let session_id = session_id.trim();
    let workshop_name = workshop_name.trim();
    if validated_sdp.is_empty() || session_id.is_empty() || workshop_name.is_empty() {
        return Err("sdp, sessionId, and workshopName are required".into());
    }
    if sdp.len() > MAX_SDP_BYTES {
        return Err("The Live audio offer is too large".into());
    }
    if !validated_sdp.starts_with("v=0") {
        return Err("The iPhone created an invalid Live audio offer".into());
    }

    let context = match embedded_state.client_if_active().await? {
        Some(client) => Some(client.live_context(session_id).await.map_err(|error| error.to_string())?),
        None => None,
    };
    let use_live = match protocol.as_deref().unwrap_or("realtime") {
        "live" => true,
        "realtime" => false,
        _ => return Err("Unsupported Live voice protocol".into()),
    };
    let mut voice_instructions = context.as_ref().map(|packet| packet.voice_instructions.clone())
        .unwrap_or_else(|| format!("You are Medousa in {workshop_name}. Speak naturally and briefly. Delegate work requiring research, MCP tools, files, images or durable actions to the workshop backend before answering. Never invent results."));
    voice_instructions.push_str("\nDo not delegate to the backend when: the user gives a conversational acknowledgment such as 'okay, no worries', thanks you, or asks to hear an already provided result. These acknowledgments do not cancel work or mean its answer should be suppressed. Yield speech to interruptions, then present verified work results when available unless the user explicitly asks not to hear them. Use current returned facts for follow-up questions instead of starting another lookup. Distinguish stopping speech from cancelling a task; never claim cancellation without backend confirmation.");
    voice_instructions.push_str(PERSONAL_CONTEXT_HANDOFF);
    let mut instructions = context.as_ref().map(|packet| packet.instructions.clone()).unwrap_or_else(|| {
        format!("You are Medousa in the user's {workshop_name} workshop. Be direct and conversational. Use hand_off_to_medousa for requests requiring tools or durable work; never invent execution results.")
    });
    instructions.push_str(PERSONAL_CONTEXT_HANDOFF);
    let seed_history: Vec<LiveSeedMessage> = context.map(|packet| packet.recent_history.into_iter().map(|turn| LiveSeedMessage {
        role: turn.role,
        content: turn.content,
    }).collect()).unwrap_or_default();

    let api_key =
        tokio::task::spawn_blocking(|| crate::integration_secrets::load_provider_secret("openai"))
            .await
            .map_err(|_| "Could not read the iPhone's OpenAI credential".to_string())?
            .ok_or_else(|| {
                "Configure an OpenAI API key on this iPhone to use Medousa Live".to_string()
            })?;

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30))
        .build().map_err(|error| error.to_string())?;
    let request = if use_live {
        client.post(OPENAI_LIVE_SESSIONS_URL).bearer_auth(&api_key).json(&serde_json::json!({
            "session": {
                "model": "gpt-live-1", "instructions": voice_instructions,
                "delegation": { "type": "client" }, "store": false,
                "audio": { "output": { "voice": "marin" } },
                "client": { "data_channel": { "allowed_client_events": [
                    "session.thinking.append", "session.commentary.append", "session.close"
                ] } },
                "input": seed_history.iter().map(|message| serde_json::json!({
                    "role": message.role,
                    "content": [{ "type": if message.role == "user" { "input_text" } else { "output_text" }, "text": message.content }]
                })).collect::<Vec<_>>()
            },
            "transport": { "type": "webrtc", "sdp": sdp }
        }))
    } else {
        client
        .post(OPENAI_REALTIME_CALLS_URL)
        .bearer_auth(api_key)
        .multipart(
            reqwest::multipart::Form::new()
                .part(
                    "sdp",
                    // Preserve the browser-generated CRLF line endings, including
                    // the final terminator required by strict SDP parsers.
                    reqwest::multipart::Part::bytes(sdp.into_bytes())
                        .mime_str("application/sdp")
                        .map_err(|error| error.to_string())?,
                )
                .part(
                    "session",
                    reqwest::multipart::Part::text(
                        serde_json::json!({
                            "type": "realtime",
                            "model": "gpt-realtime",
                            "instructions": instructions,
                            "audio": {
                                "input": {
                                    "noise_reduction": { "type": "near_field" },
                                    "transcription": { "model": "gpt-4o-mini-transcribe" }
                                },
                                "output": { "voice": "marin" }
                            },
                            "tool_choice": "auto",
                            "tools": [{
                                "type": "function",
                                "name": "hand_off_to_medousa",
                                "description": "Send a request that needs tools, research, files, durable work, or more time to the user's active Medousa chat. Tell the user briefly that you are handing it off, then call this function.",
                                "parameters": {
                                    "type": "object",
                                    "properties": {
                                        "request": {
                                            "type": "string",
                                            "description": "A self-contained description of the work Medousa should perform, including relevant context from the live conversation."
                                        }
                                    },
                                    "required": ["request"],
                                    "additionalProperties": false
                                }
                            }]
                        })
                        .to_string(),
                    )
                    .mime_str("application/json")
                    .map_err(|error| error.to_string())?,
                ),
        )
    };
    let response = request.send()
        .await
        .map_err(|error| format!("Could not reach OpenAI Live from this iPhone: {error}"))?;
    let status = response.status();
    let live_session_id = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit('/').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("realtime-call")
        .to_string();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let provider_message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            });
        return Err(match status.as_u16() {
            401 | 403 => "OpenAI rejected the API key stored on this iPhone".into(),
            429 => "OpenAI Live is temporarily rate limited".into(),
            code => provider_message.unwrap_or_else(|| {
                format!("OpenAI Live could not start the session (HTTP {code})")
            }),
        });
    }
    let answer_body = response
        .text()
        .await
        .map_err(|error| format!("Could not read OpenAI's Live audio answer: {error}"))?;
    let (live_session_id, answer_sdp) = if use_live {
        let body: serde_json::Value = serde_json::from_str(&answer_body)
            .map_err(|_| "OpenAI Live returned an invalid session response".to_string())?;
        let id = body.pointer("/session/id").and_then(serde_json::Value::as_str)
            .filter(|id| !id.is_empty()).ok_or("OpenAI Live returned no session ID")?;
        let sdp = body.pointer("/transport/sdp").and_then(serde_json::Value::as_str)
            .ok_or("OpenAI Live returned no audio answer")?;
        (id.to_string(), sdp.to_string())
    } else { (live_session_id, answer_body) };
    if !answer_sdp.trim().starts_with("v=0") {
        return Err("OpenAI Live returned an incomplete session response".into());
    }
    Ok(LiveSessionAnswer {
        protocol: if use_live { "live" } else { "realtime" },
        live_session_id,
        sdp: answer_sdp,
        seed_history: if use_live { Vec::new() } else { seed_history },
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveVoiceStartRequest<'a> {
    workshop_name: &'a str,
    session_id: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeLiveBootstrap<'a> {
    workshop_name: &'a str,
    session_id: &'a str,
    authorization: &'a str,
    configuration: serde_json::Value,
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
pub async fn live_voice_start_native(
    embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    workshop_name: String,
    session_id: String,
) -> Result<LiveVoiceStatus, String> {
    let workshop_name = workshop_name.trim();
    let session_id = session_id.trim();
    if workshop_name.is_empty() || session_id.is_empty() {
        return Err("workshopName and sessionId are required".into());
    }
    let client = embedded_state.client_if_active().await?
        .ok_or_else(|| "Native Live currently requires the Personal workshop".to_string())?;
    let context = client.live_context(session_id).await.map_err(|error| error.to_string())?;
    let api_key = tokio::task::spawn_blocking(|| {
        crate::integration_secrets::load_provider_secret("openai")
    }).await.map_err(|_| "Could not read the iPhone's OpenAI credential".to_string())?
      .ok_or_else(|| "Configure an OpenAI API key on this iPhone to use Medousa Live".to_string())?;

    let mut instructions = context.voice_instructions;
    instructions.push_str(PERSONAL_CONTEXT_HANDOFF);
    let input = context.recent_history.into_iter().map(|turn| {
        let content_type = if turn.role == "user" { "input_text" } else { "output_text" };
        serde_json::json!({
            "role": turn.role,
            "content": [{ "type": content_type, "text": turn.content }]
        })
    }).collect::<Vec<_>>();
    let configuration = serde_json::json!({
        "model": "gpt-live-1",
        "instructions": instructions,
        "delegation": { "type": "client" },
        "store": false,
        "audio": {
            "format": { "type": "audio/pcm", "rate": 24_000 },
            "output": { "voice": "marin" }
        },
        "input": input
    });
    ios::start_native(NativeLiveBootstrap {
        workshop_name,
        session_id,
        authorization: &api_key,
        configuration,
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CarPlayLiveSnapshot {
    owner: String,
    active: bool,
    muted: bool,
    phase: String,
    can_control: bool,
}

#[derive(Serialize, Deserialize)]
pub struct CarPlayLiveAction {
    owner: String,
    action: String,
}

#[derive(Serialize, Deserialize)]
pub struct CarPlayLiveExchange {
    enabled: bool,
    action: Option<CarPlayLiveAction>,
}

#[tauri::command]
pub fn live_voice_carplay_exchange(
    snapshot: CarPlayLiveSnapshot,
) -> Result<CarPlayLiveExchange, String> {
    if snapshot.owner.is_empty() || snapshot.owner.len() > 512
        || !matches!(
            snapshot.phase.as_str(),
            "idle" | "connecting" | "listening" | "thinking" | "speaking" | "muted" | "failed"
        )
    {
        return Err("Invalid CarPlay Live state".into());
    }
    ios::carplay_exchange(snapshot)
}

#[cfg(target_os = "ios")]
mod ios {
    use super::{CarPlayLiveExchange, CarPlayLiveSnapshot, LiveVoiceStartRequest, LiveVoiceStatus, NativeLiveBootstrap};
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    extern "C" {
        fn medousa_live_voice_start(json: *const c_char) -> *mut c_char;
        fn medousa_live_voice_start_native(json: *const c_char) -> *mut c_char;
        fn medousa_live_voice_set_muted(muted: bool) -> *mut c_char;
        fn medousa_live_voice_stop() -> *mut c_char;
        fn medousa_live_voice_status() -> *mut c_char;
        fn medousa_carplay_live_exchange(json: *const c_char) -> *mut c_char;
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

    pub fn start_native(request: NativeLiveBootstrap<'_>) -> Result<LiveVoiceStatus, String> {
        let json = serde_json::to_string(&request).map_err(|error| error.to_string())?;
        let json = CString::new(json).map_err(|_| "native Live bootstrap contained a null byte")?;
        decode(unsafe { medousa_live_voice_start_native(json.as_ptr()) })
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

    pub fn carplay_exchange(
        snapshot: CarPlayLiveSnapshot,
    ) -> Result<CarPlayLiveExchange, String> {
        let json = serde_json::to_string(&snapshot).map_err(|error| error.to_string())?;
        let json = CString::new(json).map_err(|_| "Invalid CarPlay Live state")?;
        let raw = unsafe { medousa_carplay_live_exchange(json.as_ptr()) };
        if raw.is_null() {
            return Err("CarPlay Live bridge unavailable".into());
        }
        let result = unsafe {
            let json = CStr::from_ptr(raw).to_string_lossy().into_owned();
            medousa_live_activity_free_string(raw);
            json
        };
        serde_json::from_str(&result).map_err(|error| error.to_string())
    }
}
