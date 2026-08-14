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
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};

pub fn calendar_surface() -> DeclaredRouter {
    use axum::routing::{delete, get, post, put};

    DeclaredRouter::default()
        .methods([
            (
                calendar_read_policy("/v1/calendar/events"),
                get(list_calendar_events),
            ),
            (
                calendar_write_policy(
                    axum::http::Method::POST,
                    "/v1/calendar/events",
                    256 * 1024,
                ),
                post(create_calendar_event),
            ),
        ])
        .methods([
            (
                calendar_write_policy(
                    axum::http::Method::PUT,
                    "/v1/calendar/events/{uid}",
                    256 * 1024,
                ),
                put(update_calendar_event),
            ),
            (
                calendar_write_policy(
                    axum::http::Method::DELETE,
                    "/v1/calendar/events/{uid}",
                    1024,
                ),
                delete(delete_calendar_event),
            ),
        ])
        .route(
            calendar_write_policy(
                axum::http::Method::POST,
                "/v1/calendar/import",
                8 * 1024 * 1024,
            ),
            post(import_calendar),
        )
        .route(
            calendar_read_policy("/v1/calendar/export"),
            get(export_calendar),
        )
}

fn calendar_read_policy(path: &'static str) -> RoutePolicy {
    calendar_policy(
        axum::http::Method::GET,
        path,
        crate::request_principal::Capability::ContentRead,
        1024,
        RateLimitClass::Read,
    )
}

fn calendar_write_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    calendar_policy(
        method,
        path,
        crate::request_principal::Capability::ContentWrite,
        body_limit,
        RateLimitClass::Mutation,
    )
}

fn calendar_policy(
    method: axum::http::Method,
    path: &'static str,
    required_capability: crate::request_principal::Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
}

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
