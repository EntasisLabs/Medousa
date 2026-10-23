use medousa_types::{
    BotListResponse, BotOpenResponse, BotProfile, CreateBotRequest, DuplicateBotRequest,
    SessionBotResponse, SetBotArchivedRequest, SetSessionBotRequest, UpdateBotRequest,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::window_request::WorkshopRequest;
use super::sdk::{client_for_request as client, sdk_error};

fn required_id<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

#[tauri::command]
pub async fn bot_list(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
) -> Result<BotListResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client.list_bots().map_err(|error| error.to_string());
    }
    client(&state).await?.bots().list().await.map_err(sdk_error)
}

#[tauri::command]
pub async fn bot_create(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: CreateBotRequest,
) -> Result<BotOpenResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .create_bot(request.clone())
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .bots()
        .create(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn bot_get(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    bot_id: String,
) -> Result<BotProfile, String> {
    let bot_id = required_id(&bot_id, "bot_id")?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client.get_bot(bot_id).map_err(|error| error.to_string());
    }
    client(&state).await?.bots().get(bot_id).await.map_err(sdk_error)
}

#[tauri::command]
pub async fn bot_update(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    bot_id: String,
    request: UpdateBotRequest,
) -> Result<BotProfile, String> {
    let bot_id = required_id(&bot_id, "bot_id")?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .update_bot(bot_id, request.clone())
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .bots()
        .update(bot_id, &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn bot_set_archived(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    bot_id: String,
    request: SetBotArchivedRequest,
) -> Result<BotProfile, String> {
    let bot_id = required_id(&bot_id, "bot_id")?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .set_bot_archived(bot_id, request.clone())
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .bots()
        .set_archived(bot_id, &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn bot_duplicate(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    bot_id: String,
    request: DuplicateBotRequest,
) -> Result<BotOpenResponse, String> {
    let bot_id = required_id(&bot_id, "bot_id")?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .duplicate_bot(bot_id, request.clone())
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .bots()
        .duplicate(bot_id, &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn bot_open(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    bot_id: String,
) -> Result<BotOpenResponse, String> {
    let bot_id = required_id(&bot_id, "bot_id")?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client.open_bot(bot_id).map_err(|error| error.to_string());
    }
    client(&state).await?.bots().open(bot_id).await.map_err(sdk_error)
}

#[tauri::command]
pub async fn session_get_bot(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: String,
) -> Result<SessionBotResponse, String> {
    let session_id = required_id(&session_id, "session_id")?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .session_bot(session_id)
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .bots()
        .session(session_id)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_set_bot(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: String,
    request: SetSessionBotRequest,
) -> Result<SessionBotResponse, String> {
    let session_id = required_id(&session_id, "session_id")?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .bind_session_bot(session_id, request.clone())
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .bots()
        .bind_session(session_id, &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_clear_bot(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: String,
) -> Result<SessionBotResponse, String> {
    let session_id = required_id(&session_id, "session_id")?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .unbind_session_bot(session_id)
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .bots()
        .unbind_session(session_id)
        .await
        .map_err(sdk_error)
}
