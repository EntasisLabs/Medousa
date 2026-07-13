use crate::daemon::types::{
    CalendarDeleteResponse, CalendarExportResponse, CalendarImportResponse, CalendarListResponse,
    CalendarWriteResponse,
};
use chrono::{DateTime, Utc};
use medousa_types::{
    CalendarExportQuery, CalendarImportRequest, CalendarListQuery, CalendarWriteRequest,
};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[tauri::command]
pub async fn calendar_list_events(
    state: State<'_, DaemonState>,
    from: Option<String>,
    to: Option<String>,
    path: Option<String>,
) -> Result<CalendarListResponse, String> {
    let query = CalendarListQuery {
        from: parse_optional_datetime(from)?,
        to: parse_optional_datetime(to)?,
        path: path.filter(|value| !value.trim().is_empty()),
    };
    client(&state)
        .calendar()
        .list_events(&query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_create_event(
    state: State<'_, DaemonState>,
    request: CalendarWriteRequest,
) -> Result<CalendarWriteResponse, String> {
    client(&state)
        .calendar()
        .create_event(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_update_event(
    state: State<'_, DaemonState>,
    uid: String,
    request: CalendarWriteRequest,
) -> Result<CalendarWriteResponse, String> {
    client(&state)
        .calendar()
        .update_event(uid.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_delete_event(
    state: State<'_, DaemonState>,
    uid: String,
    path: Option<String>,
) -> Result<CalendarDeleteResponse, String> {
    let query = CalendarExportQuery {
        path: path.filter(|value| !value.trim().is_empty()),
    };
    client(&state)
        .calendar()
        .delete_event(uid.trim(), &query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_import_ics(
    state: State<'_, DaemonState>,
    ics: String,
    path: Option<String>,
) -> Result<CalendarImportResponse, String> {
    let request = CalendarImportRequest {
        ics,
        calendar_path: path.filter(|value| !value.trim().is_empty()),
    };
    client(&state)
        .calendar()
        .import_ics(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn calendar_export(
    state: State<'_, DaemonState>,
    path: Option<String>,
) -> Result<CalendarExportResponse, String> {
    let query = CalendarExportQuery {
        path: path.filter(|value| !value.trim().is_empty()),
    };
    client(&state)
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
