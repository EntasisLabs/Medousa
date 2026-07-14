//! HTTP handlers for calendar APIs (`/v1/calendar/*`).

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use medousa_types::{
    CalendarDeleteResponse, CalendarExportQuery, CalendarExportResponse, CalendarImportRequest,
    CalendarImportResponse, CalendarListQuery, CalendarListResponse, CalendarWriteRequest,
    CalendarWriteResponse,
};

use crate::calendar::CalendarService;

fn map_calendar_error(err: anyhow::Error) -> (StatusCode, String) {
    let message = err.to_string();
    if message.contains("not found") {
        (StatusCode::NOT_FOUND, message)
    } else if message.contains("already exists")
        || message.contains("required")
        || message.contains("must")
        || message.contains("invalid")
    {
        (StatusCode::BAD_REQUEST, message)
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

pub async fn list_calendar_events(
    Query(query): Query<CalendarListQuery>,
) -> Result<Json<CalendarListResponse>, (StatusCode, String)> {
    CalendarService::list_events(query.path.as_deref(), query.from, query.to)
        .map(Json)
        .map_err(map_calendar_error)
}

pub async fn create_calendar_event(
    Json(request): Json<CalendarWriteRequest>,
) -> Result<Json<CalendarWriteResponse>, (StatusCode, String)> {
    CalendarService::create_event(&request)
        .map(Json)
        .map_err(map_calendar_error)
}

pub async fn update_calendar_event(
    Path(uid): Path<String>,
    Json(request): Json<CalendarWriteRequest>,
) -> Result<Json<CalendarWriteResponse>, (StatusCode, String)> {
    CalendarService::update_event(&uid, &request)
        .map(Json)
        .map_err(map_calendar_error)
}

pub async fn delete_calendar_event(
    Path(uid): Path<String>,
    Query(query): Query<CalendarExportQuery>,
) -> Result<Json<CalendarDeleteResponse>, (StatusCode, String)> {
    CalendarService::delete_event(&uid, query.path.as_deref())
        .map(Json)
        .map_err(map_calendar_error)
}

pub async fn import_calendar(
    Json(request): Json<CalendarImportRequest>,
) -> Result<Json<CalendarImportResponse>, (StatusCode, String)> {
    CalendarService::import(&request)
        .map(Json)
        .map_err(map_calendar_error)
}

pub async fn export_calendar(
    Query(query): Query<CalendarExportQuery>,
) -> Result<Json<CalendarExportResponse>, (StatusCode, String)> {
    CalendarService::export(query.path.as_deref())
        .map(Json)
        .map_err(map_calendar_error)
}
