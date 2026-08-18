//! HTTP handlers for turn tool-round budget approval requests.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon_api::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestListQuery,
    TurnBudgetRequestListResponse, TurnBudgetRequestRecord, TurnBudgetRequestResponse,
};
use crate::turn_budget_request::{TurnBudgetRequestStatus, turn_budget_request_store};

#[derive(Clone, Default)]
pub struct TurnBudgetHandlerState;

pub fn budget_surface() -> DeclaredRouter<TurnBudgetHandlerState> {
    use axum::routing::{get, post};

    use crate::request_principal::Capability;

    DeclaredRouter::default()
        .route(
            budget_policy(
                axum::http::Method::GET,
                "/v1/turns/budget-requests",
                Capability::WorkshopRead,
                1024,
                RateLimitClass::Read,
            ),
            get(list_turn_budget_requests),
        )
        .route(
            budget_policy(
                axum::http::Method::GET,
                "/v1/turns/budget-requests/{request_id}",
                Capability::WorkshopRead,
                1024,
                RateLimitClass::Read,
            ),
            get(get_turn_budget_request),
        )
        .route(
            budget_policy(
                axum::http::Method::POST,
                "/v1/turns/budget-requests/{request_id}/approve",
                Capability::AdminExecute,
                64 * 1024,
                RateLimitClass::Administration,
            ),
            post(approve_turn_budget_request),
        )
        .route(
            budget_policy(
                axum::http::Method::POST,
                "/v1/turns/budget-requests/{request_id}/deny",
                Capability::AdminExecute,
                64 * 1024,
                RateLimitClass::Administration,
            ),
            post(deny_turn_budget_request),
        )
}

fn budget_policy(
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

pub async fn list_turn_budget_requests(
    Query(query): Query<TurnBudgetRequestListQuery>,
) -> Result<Json<TurnBudgetRequestListResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(50);
    let rows = if query
        .status
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("pending"))
        || query.status.is_none()
    {
        turn_budget_request_store().list_pending(limit)
    } else {
        turn_budget_request_store().list_for_workspace(true)
    };

    Ok(Json(TurnBudgetRequestListResponse {
        requests: rows
            .into_iter()
            .map(TurnBudgetRequestRecord::from)
            .collect(),
    }))
}

pub async fn get_turn_budget_request(
    Path(request_id): Path<String>,
) -> Result<Json<TurnBudgetRequestRecord>, (StatusCode, String)> {
    let request_id = request_id.trim();
    turn_budget_request_store()
        .get(request_id)
        .map(TurnBudgetRequestRecord::from)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("budget request not found: {request_id}"),
            )
        })
        .map(Json)
}

pub async fn approve_turn_budget_request(
    Path(request_id): Path<String>,
    State(_state): State<TurnBudgetHandlerState>,
    Json(body): Json<TurnBudgetApproveRequest>,
) -> Result<Json<TurnBudgetRequestResponse>, (StatusCode, String)> {
    let request_id = request_id.trim();
    let updated = turn_budget_request_store()
        .approve(request_id, body.extra_rounds, body.resolved_by.clone())
        .map_err(|err| (StatusCode::BAD_REQUEST, err))?;
    Ok(Json(TurnBudgetRequestResponse {
        request: TurnBudgetRequestRecord::from(updated),
        message: "approved".to_string(),
    }))
}

pub async fn deny_turn_budget_request(
    Path(request_id): Path<String>,
    State(_state): State<TurnBudgetHandlerState>,
    Json(body): Json<TurnBudgetDenyRequest>,
) -> Result<Json<TurnBudgetRequestResponse>, (StatusCode, String)> {
    let request_id = request_id.trim();
    let updated = turn_budget_request_store()
        .deny(request_id, body.resolved_by.clone())
        .map_err(|err| (StatusCode::BAD_REQUEST, err))?;
    Ok(Json(TurnBudgetRequestResponse {
        request: TurnBudgetRequestRecord::from(updated),
        message: "denied".to_string(),
    }))
}

impl From<crate::turn_budget_request::TurnBudgetRequest> for TurnBudgetRequestRecord {
    fn from(value: crate::turn_budget_request::TurnBudgetRequest) -> Self {
        Self {
            request_id: value.request_id,
            turn_correlation_id: value.turn_correlation_id,
            stream_turn_id: value.stream_turn_id,
            session_id: value.session_id,
            channel: value.channel,
            rounds_executed: value.rounds_executed,
            max_tool_rounds: value.max_tool_rounds,
            requested_rounds: value.requested_rounds,
            granted_rounds: value.granted_rounds,
            reason: value.reason,
            progress_summary: value.progress_summary,
            status: match value.status {
                TurnBudgetRequestStatus::Pending => "pending".to_string(),
                TurnBudgetRequestStatus::Approved => "approved".to_string(),
                TurnBudgetRequestStatus::Denied => "denied".to_string(),
                TurnBudgetRequestStatus::Expired => "expired".to_string(),
            },
            resolved_by: value.resolved_by,
            created_at_utc: value.created_at_utc,
            updated_at_utc: value.updated_at_utc,
            resolved_at_utc: value.resolved_at_utc,
        }
    }
}
