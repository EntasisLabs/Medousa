use serde::{Deserialize, Serialize};

#[cfg(target_os = "ios")]
use medousa_types::{TurnStreamEventV2, TurnSurfaceContext};
#[cfg(target_os = "ios")]
use tauri::Manager;

const OPENAI_REALTIME_CALLS_URL: &str = "https://api.openai.com/v1/realtime/calls";
const OPENAI_LIVE_SESSIONS_URL: &str = "https://api.openai.com/v1/live/sessions";
const MAX_SDP_BYTES: usize = 256 * 1024;
const PERSONAL_CONTEXT_HANDOFF: &str = "\nBackend tools: the workshop can look up permitted saved memory, conversation history, journal or vault notes, and connected sources when configured.\nDelegate to the backend when: the user asks to recall or summarize their personal activity, work, decisions, plans, or progress beyond facts explicitly available in this conversation. Examples: 'what I've been up to this week', 'what have I been working on lately?', 'what did we decide last time?', and 'summarize my week'. These are personal-context retrieval requests, not small talk. Delegate before answering; the small seeded history is not a complete activity log. Do not guess, claim you have no memory, or ask the user to recount their week before the backend checks its permitted sources. If the backend finds insufficient information, say so honestly. A new time range or topic requires a new lookup even after an earlier result.\nDo not delegate to the backend when: the user is simply telling you about their week, sharing feelings, greeting you, thanking you, or asking to repeat or explain a verified result already provided for that same scope. Respond naturally to those conversational turns.";

#[cfg(target_os = "ios")]
#[derive(Default)]
struct NativeLiveTimeline {
    fragments: Vec<NativeLiveFragment>,
    seen: std::collections::HashSet<String>,
    last_delegation_offset: Option<f64>,
}

#[cfg(target_os = "ios")]
struct NativeLiveFragment {
    id: String,
    role: &'static str,
    text: String,
    start_ms: f64,
    end_ms: f64,
}

#[cfg(target_os = "ios")]
impl NativeLiveTimeline {
    fn accept(&mut self, event: &serde_json::Value) {
        let role = match event.get("type").and_then(serde_json::Value::as_str) {
            Some("session.input_transcript.delta") => "user",
            Some("session.output_transcript.delta") => "assistant",
            _ => return,
        };
        let Some(text) = event.get("delta").and_then(serde_json::Value::as_str) else { return };
        let Some(start_ms) = event.get("start_ms").and_then(serde_json::Value::as_f64) else { return };
        let Some(end_ms) = event.get("end_ms").and_then(serde_json::Value::as_f64) else { return };
        if text.is_empty() || !start_ms.is_finite() || !end_ms.is_finite() || start_ms < 0.0 || end_ms < start_ms { return; }
        let id = event.get("event_id").and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{role}-{start_ms}-{end_ms}-{text}"));
        if !self.seen.insert(id.clone()) { return; }
        self.fragments.push(NativeLiveFragment { id, role, text: text.to_string(), start_ms, end_ms });
        if self.fragments.len() > 512 {
            let removed = self.fragments.remove(0);
            self.seen.remove(&removed.id);
        }
    }

    fn request(&mut self, offset_ms: f64) -> String {
        let previous = self.last_delegation_offset.replace(offset_ms);
        let mut fragments: Vec<_> = self.fragments.iter()
            .filter(|fragment| fragment.role == "user" && fragment.start_ms <= offset_ms
                && previous.is_none_or(|value| fragment.end_ms > value))
            .collect();
        fragments.sort_by(|left, right| left.start_ms.total_cmp(&right.start_ms));
        if previous.is_none() {
            let mut grouped = String::new();
            let mut latest_end = -1.0;
            for fragment in fragments {
                if latest_end >= 0.0 && fragment.start_ms - latest_end > 1200.0 { grouped.clear(); }
                grouped.push_str(&fragment.text);
                latest_end = latest_end.max(fragment.end_ms);
            }
            return grouped.trim().to_string();
        }
        fragments.into_iter().map(|fragment| fragment.text.as_str()).collect::<String>().trim().to_string()
    }
}

#[cfg(target_os = "ios")]
fn native_delegation(event: &serde_json::Value) -> Option<(String, f64)> {
    if event.get("type")?.as_str()? != "session.delegation.created" { return None; }
    let delegation = event.get("delegation")?;
    if delegation.get("target")?.as_str()? != "client" { return None; }
    let id = delegation.get("id")?.as_str()?.trim();
    let offset = event.get("offset_ms")?.as_f64()?;
    (!id.is_empty() && offset.is_finite()).then(|| (id.to_string(), offset))
}

#[cfg(target_os = "ios")]
fn bounded_live_result(status: &str, text: &str) -> String {
    let mut excerpt = String::new();
    for character in text.chars() {
        if excerpt.len() + character.len_utf8() > 360 { break; }
        excerpt.push(character);
    }
    let prefix = if status == "completed" { "" } else { status };
    format!("{}{}{}", if prefix.is_empty() { "" } else { prefix }, if prefix.is_empty() { "" } else { ": " }, excerpt)
        + if excerpt.len() < text.len() { "… Full result is in the chat." } else { "" }
}

#[cfg(target_os = "ios")]
async fn execute_native_delegation(
    app: &tauri::AppHandle,
    session_id: &str,
    delegation_id: &str,
    request: String,
) -> Result<(), String> {
    if request.is_empty() { return Err("The request's speech context was unavailable. Please repeat it.".into()); }
    let embedded = app.state::<crate::embedded_daemon::EmbeddedDaemonState>();
    embedded.resume_for_background_execution().await?;
    let client = embedded.client_if_active().await?
        .ok_or_else(|| "Live tools require the Personal workshop".to_string())?;
    let accepted = client.start_turn_with_options(
        session_id,
        request,
        None,
        TurnSurfaceContext {
            channel_surface: Some("home-ios-live".to_string()),
            channel_id: Some(session_id.to_string()),
            user_id: None,
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            browser_driver_id: None,
            selected_worlds: Vec::new(),
        },
        None,
        Some("This answer returns to an ongoing Medousa voice conversation. Lead with a brief, complete spoken summary of the verified result and status, then any useful details. Do not announce internal routing.".to_string()),
        "standard".to_string(),
        "default".to_string(),
        Vec::new(),
        Vec::new(),
        None,
    ).await.map_err(|error| error.to_string())?;
    let mut stream = client.subscribe_turn(&accepted.turn_id, 0).await.map_err(|error| error.to_string())?;
    while let Some(envelope) = stream.recv().await.map_err(|error| error.to_string())? {
        let outcome = match envelope.event {
            TurnStreamEventV2::Final { text, .. } | TurnStreamEventV2::WorkerSynthesis { text, .. } => Some(("completed", text)),
            TurnStreamEventV2::NeedsInput { text, .. } | TurnStreamEventV2::Checkpoint { text, .. } => Some(("needs input", text)),
            TurnStreamEventV2::Error { operator_message, .. } => Some(("failed", operator_message)),
            _ => None,
        };
        if let Some((status, text)) = outcome {
            ios::send_event(serde_json::json!({
                "type": "session.commentary.append",
                "event_id": format!("result-{delegation_id}"),
                "delegation_id": delegation_id,
                "content": bounded_live_result(status, &text),
            }))?;
            return Ok(());
        }
    }
    Err("Medousa's response stream ended early".to_string())
}

#[cfg(target_os = "ios")]
pub fn install_background_coordinator(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut timeline = NativeLiveTimeline::default();
        let mut handled = std::collections::HashSet::<String>::new();
        loop {
            let status = ios::status().ok();
            if !status.as_ref().is_some_and(|value| value.active) {
                timeline = NativeLiveTimeline::default();
                handled.clear();
                tokio::time::sleep(std::time::Duration::from_millis(750)).await;
                continue;
            }
            let events = ios::drain_background_events().unwrap_or_default();
            for event in &events { timeline.accept(event); }
            for event in events {
                let Some((delegation_id, offset_ms)) = native_delegation(&event) else { continue; };
                if !handled.insert(delegation_id.clone()) { continue; }
                tokio::time::sleep(std::time::Duration::from_millis(750)).await;
                for late in ios::drain_background_events().unwrap_or_default() { timeline.accept(&late); }
                let request = timeline.request(offset_ms);
                let _ = ios::send_event(serde_json::json!({
                    "type": "session.thinking.append",
                    "event_id": format!("progress-{delegation_id}"),
                    "delegation_id": delegation_id,
                    "content": "The workshop is processing this request. No result is available yet.",
                }));
                if let Err(error) = execute_native_delegation(&app, status.as_ref().and_then(|value| value.session_id.as_deref()).unwrap_or(""), &delegation_id, request).await {
                    let _ = ios::send_event(serde_json::json!({
                        "type": "session.commentary.append",
                        "event_id": format!("result-{delegation_id}"),
                        "delegation_id": delegation_id,
                        "content": bounded_live_result("failed", &error),
                    }));
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    });
}

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
    let client = embedded_state
        .client_if_active()
        .await?
        .ok_or_else(|| "Live transcript persistence requires the Personal workshop".to_string())?;
    client
        .append_live_transcript(
            &session_id,
            &live_session_id,
            &item_id,
            &role,
            &text,
            attachment.unwrap_or(false),
            target_turn_id.as_deref(),
        )
        .await
        .map_err(|error| error.to_string())
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
        Some(client) => Some(
            client
                .live_context(session_id)
                .await
                .map_err(|error| error.to_string())?,
        ),
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
    let seed_history: Vec<LiveSeedMessage> = context
        .map(|packet| {
            packet
                .recent_history
                .into_iter()
                .map(|turn| LiveSeedMessage {
                    role: turn.role,
                    content: turn.content,
                })
                .collect()
        })
        .unwrap_or_default();

    let api_key =
        tokio::task::spawn_blocking(|| crate::integration_secrets::load_provider_secret("openai"))
            .await
            .map_err(|_| "Could not read the iPhone's OpenAI credential".to_string())?
            .ok_or_else(|| {
                "Configure an OpenAI API key on this iPhone to use Medousa Live".to_string()
            })?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
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
    let response = request
        .send()
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
        let id = body
            .pointer("/session/id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or("OpenAI Live returned no session ID")?;
        let sdp = body
            .pointer("/transport/sdp")
            .and_then(serde_json::Value::as_str)
            .ok_or("OpenAI Live returned no audio answer")?;
        (id.to_string(), sdp.to_string())
    } else {
        (live_session_id, answer_body)
    };
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
    let client = embedded_state
        .client_if_active()
        .await?
        .ok_or_else(|| "Native Live currently requires the Personal workshop".to_string())?;
    let context = client
        .live_context(session_id)
        .await
        .map_err(|error| error.to_string())?;
    let api_key =
        tokio::task::spawn_blocking(|| crate::integration_secrets::load_provider_secret("openai"))
            .await
            .map_err(|_| "Could not read the iPhone's OpenAI credential".to_string())?
            .ok_or_else(|| {
                "Configure an OpenAI API key on this iPhone to use Medousa Live".to_string()
            })?;

    let mut instructions = context.voice_instructions;
    instructions.push_str(PERSONAL_CONTEXT_HANDOFF);
    let input = context
        .recent_history
        .into_iter()
        .map(|turn| {
            let content_type = if turn.role == "user" {
                "input_text"
            } else {
                "output_text"
            };
            serde_json::json!({
                "role": turn.role,
                "content": [{ "type": content_type, "text": turn.content }]
            })
        })
        .collect::<Vec<_>>();
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
pub async fn live_voice_prepare_native(
    embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    workshop_name: String,
    session_id: String,
) -> Result<bool, String> {
    let workshop_name = workshop_name.trim();
    let session_id = session_id.trim();
    if workshop_name.is_empty() || session_id.is_empty() {
        return Err("workshopName and sessionId are required".into());
    }
    let client = embedded_state
        .client_if_active()
        .await?
        .ok_or_else(|| "Background Live currently requires the Personal workshop".to_string())?;
    let context = client
        .live_context(session_id)
        .await
        .map_err(|error| error.to_string())?;
    let api_key =
        tokio::task::spawn_blocking(|| crate::integration_secrets::load_provider_secret("openai"))
            .await
            .map_err(|_| "Could not read the iPhone's OpenAI credential".to_string())?
            .ok_or_else(|| {
                "Configure an OpenAI API key on this iPhone to use Medousa Live".to_string()
            })?;

    let mut instructions = context.voice_instructions;
    instructions.push_str(PERSONAL_CONTEXT_HANDOFF);
    let input = context
        .recent_history
        .into_iter()
        .map(|turn| {
            let content_type = if turn.role == "user" {
                "input_text"
            } else {
                "output_text"
            };
            serde_json::json!({
                "role": turn.role,
                "content": [{ "type": content_type, "text": turn.content }]
            })
        })
        .collect::<Vec<_>>();
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
    ios::prepare_native(NativeLiveBootstrap {
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

#[tauri::command]
pub fn live_voice_drain_native_events() -> Result<Vec<serde_json::Value>, String> {
    ios::drain_events()
}

#[tauri::command]
pub fn live_voice_ack_native_events(through_sequence: u64) -> Result<(), String> {
    ios::ack_events(through_sequence)
}

#[tauri::command]
pub fn live_voice_send_native_event(event: serde_json::Value) -> Result<(), String> {
    ios::send_event(event)
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
    if snapshot.owner.is_empty()
        || snapshot.owner.len() > 512
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
    use super::{
        CarPlayLiveExchange, CarPlayLiveSnapshot, LiveVoiceStartRequest, LiveVoiceStatus,
        NativeLiveBootstrap,
    };
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    extern "C" {
        fn medousa_live_voice_start(json: *const c_char) -> *mut c_char;
        fn medousa_live_voice_start_native(json: *const c_char) -> *mut c_char;
        fn medousa_live_voice_prepare_native(json: *const c_char) -> bool;
        fn medousa_live_voice_set_muted(muted: bool) -> *mut c_char;
        fn medousa_live_voice_stop() -> *mut c_char;
        fn medousa_live_voice_status() -> *mut c_char;
        fn medousa_live_voice_drain_events() -> *mut c_char;
        fn medousa_live_voice_drain_background_events() -> *mut c_char;
        fn medousa_live_voice_ack_events(through_sequence: u64) -> bool;
        fn medousa_live_voice_send_event(json: *const c_char) -> bool;
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

    pub fn prepare_native(request: NativeLiveBootstrap<'_>) -> Result<bool, String> {
        let json = serde_json::to_string(&request).map_err(|error| error.to_string())?;
        let json =
            CString::new(json).map_err(|_| "native Live preparation contained a null byte")?;
        Ok(unsafe { medousa_live_voice_prepare_native(json.as_ptr()) })
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

    pub fn drain_events() -> Result<Vec<serde_json::Value>, String> {
        let raw = unsafe { medousa_live_voice_drain_events() };
        if raw.is_null() {
            return Err("Medousa Live native event bridge returned null".into());
        }
        let json = unsafe {
            let json = CStr::from_ptr(raw).to_string_lossy().into_owned();
            medousa_live_activity_free_string(raw);
            json
        };
        serde_json::from_str(&json).map_err(|error| format!("decode native Live events: {error}"))
    }

    pub fn drain_background_events() -> Result<Vec<serde_json::Value>, String> {
        let raw = unsafe { medousa_live_voice_drain_background_events() };
        if raw.is_null() { return Ok(Vec::new()); }
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
        unsafe { medousa_live_activity_free_string(raw) };
        serde_json::from_str(&text).map_err(|error| error.to_string())
    }

    pub fn ack_events(through_sequence: u64) -> Result<(), String> {
        if through_sequence == 0 {
            return Err("Native Live event sequence is invalid".into());
        }
        if unsafe { medousa_live_voice_ack_events(through_sequence) } {
            Ok(())
        } else {
            Err("Native Live could not acknowledge events".into())
        }
    }

    pub fn send_event(event: serde_json::Value) -> Result<(), String> {
        let json = serde_json::to_string(&event).map_err(|error| error.to_string())?;
        if json.len() > 65 * 1024 {
            return Err("Native Live event is too large".into());
        }
        let json = CString::new(json).map_err(|_| "Native Live event contained a null byte")?;
        if unsafe { medousa_live_voice_send_event(json.as_ptr()) } {
            Ok(())
        } else {
            Err("Native Live rejected the event".into())
        }
    }

    pub fn carplay_exchange(snapshot: CarPlayLiveSnapshot) -> Result<CarPlayLiveExchange, String> {
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
