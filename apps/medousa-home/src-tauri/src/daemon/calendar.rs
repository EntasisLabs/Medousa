use crate::daemon::types::{
    CalendarDeleteResponse, CalendarExportResponse, CalendarImportResponse, CalendarListResponse,
    CalendarWriteResponse,
};
use chrono::{DateTime, Utc};
use medousa_types::{
    CalendarExportQuery, CalendarImportRequest, CalendarListQuery, CalendarWriteRequest,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::DaemonState;
use super::sdk::{client, sdk_error};

#[tauri::command]
pub async fn calendar_list_events(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    from: Option<String>,
    to: Option<String>,
    path: Option<String>,
) -> Result<CalendarListResponse, String> {
    let query = CalendarListQuery {
        from: parse_optional_datetime(from)?,
        to: parse_optional_datetime(to)?,
        path: path.filter(|value| !value.trim().is_empty()),
    };
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_calendar_events(query)
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .calendar()
        .list_events(&query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_create_event(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: CalendarWriteRequest,
) -> Result<CalendarWriteResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .create_calendar_event(&request)
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .calendar()
        .create_event(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_update_event(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    uid: String,
    request: CalendarWriteRequest,
) -> Result<CalendarWriteResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .update_calendar_event(uid.trim(), &request)
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .calendar()
        .update_event(uid.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_delete_event(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    uid: String,
    path: Option<String>,
) -> Result<CalendarDeleteResponse, String> {
    let path = path.filter(|value| !value.trim().is_empty());
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .delete_calendar_event(uid.trim(), path.as_deref())
            .map_err(|error| error.to_string());
    }
    let query = CalendarExportQuery { path };
    client(&state)?
        .calendar()
        .delete_event(uid.trim(), &query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_import_ics(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    ics: String,
    path: Option<String>,
) -> Result<CalendarImportResponse, String> {
    let request = CalendarImportRequest {
        ics,
        calendar_path: path.filter(|value| !value.trim().is_empty()),
    };
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .import_calendar(&request)
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .calendar()
        .import_ics(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_export(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    path: Option<String>,
) -> Result<CalendarExportResponse, String> {
    let path = path.filter(|value| !value.trim().is_empty());
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .export_calendar(path.as_deref())
            .map_err(|error| error.to_string());
    }
    let query = CalendarExportQuery { path };
    client(&state)?
        .calendar()
        .export(&query)
        .await
        .map_err(sdk_error)
}

fn parse_optional_datetime(raw: Option<String>) -> Result<Option<DateTime<Utc>>, String> {
    let Some(value) = raw.filter(|v| !v.trim().is_empty()) else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(value.trim())
        .map(|dt| Some(dt.with_timezone(&Utc)))
        .map_err(|err| format!("invalid datetime: {err}"))
}
