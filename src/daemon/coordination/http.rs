//! Native operator adapter. Proposals are immutable; callers never send grants.
use super::{LocalPeerDispatcher, local_coordination_host};
use crate::{
    daemon::route_policy::{
        BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
    },
    request_principal::{Capability, RequestPrincipal},
};
use axum::{
    Json,
    extract::{Extension, Path, Query},
    http::{Method, StatusCode},
    routing::{get, post},
};
use medousa_types::{SessionId, coordination::*};
use std::sync::Arc;

pub fn surface() -> DeclaredRouter {
    DeclaredRouter::default()
        .route(
            policy(Method::GET, "/v1/coordination/proposals"),
            get(inbox),
        )
        .route(
            policy(
                Method::POST,
                "/v1/coordination/channels/{channel_id}/proposals/{proposal_id}/approve",
            ),
            post(approve),
        )
        .route(
            policy(
                Method::POST,
                "/v1/coordination/channels/{channel_id}/proposals/{proposal_id}/deny",
            ),
            post(deny),
        )
        .route(
            policy(
                Method::POST,
                "/v1/coordination/channels/{channel_id}/proposals/{proposal_id}/dispatch",
            ),
            post(dispatch),
        )
}
fn policy(method: Method, path: &'static str) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(Capability::AdminExecute),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: 1024,
        rate_limit_class: RateLimitClass::Administration,
    }
}
type HttpError = (StatusCode, String);
fn host() -> Result<Arc<LocalPeerDispatcher>, HttpError> {
    local_coordination_host().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Coordination is not available on this workshop".into(),
    ))
}
fn conflict(error: anyhow::Error) -> HttpError {
    (StatusCode::CONFLICT, error.to_string())
}
fn channel(channel_id: String) -> Result<CoordinationChannelRef, HttpError> {
    Ok(CoordinationChannelRef {
        authority_id: crate::workshop_authority::current()
            .map_err(|_| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Workshop authority unavailable".into(),
                )
            })?
            .clone(),
        channel_id,
    })
}
#[derive(serde::Deserialize)]
struct InboxQuery {
    session_id: SessionId,
    after: Option<String>,
}
async fn inbox(
    Extension(principal): Extension<RequestPrincipal>,
    Query(query): Query<InboxQuery>,
) -> Result<Json<PeerProposalInboxResponse>, HttpError> {
    host()?
        .proposal_inbox(&principal, query.session_id, query.after)
        .await
        .map(Json)
        .map_err(conflict)
}
async fn decide(
    principal: RequestPrincipal,
    ids: (String, String),
    approved: bool,
) -> Result<Json<PeerProposalActionResponse>, HttpError> {
    let (channel_id, proposal_id) = ids;
    host()?
        .decide_proposal(
            &principal,
            channel(channel_id)?,
            proposal_id.clone(),
            approved,
        )
        .await
        .map_err(conflict)?;
    Ok(Json(PeerProposalActionResponse {
        proposal_id,
        binding: None,
    }))
}
async fn approve(
    Extension(principal): Extension<RequestPrincipal>,
    Path(ids): Path<(String, String)>,
    Json(_): Json<PeerProposalActionRequest>,
) -> Result<Json<PeerProposalActionResponse>, HttpError> {
    decide(principal, ids, true).await
}
async fn deny(
    Extension(principal): Extension<RequestPrincipal>,
    Path(ids): Path<(String, String)>,
    Json(_): Json<PeerProposalActionRequest>,
) -> Result<Json<PeerProposalActionResponse>, HttpError> {
    decide(principal, ids, false).await
}
async fn dispatch(
    Extension(principal): Extension<RequestPrincipal>,
    Path((channel_id, proposal_id)): Path<(String, String)>,
    Json(_): Json<PeerProposalActionRequest>,
) -> Result<Json<PeerProposalActionResponse>, HttpError> {
    let binding = host()?
        .dispatch_approved_proposal(&principal, channel(channel_id)?, proposal_id.clone())
        .await
        .map_err(conflict)?;
    Ok(Json(PeerProposalActionResponse {
        proposal_id,
        binding: Some(binding),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_coordination_routes_are_native_operator_only() {
        let router = surface();
        let inventory = router.inventory();
        let entries: Vec<_> = inventory.entries().collect();
        assert_eq!(entries.len(), 4);
        assert!(
            entries
                .iter()
                .all(|entry| entry.required_capability == Some("admin.execute")
                    && entry.browser_policy == BrowserPolicy::NativeOnly
                    && !entry.bootstrap_public)
        );
    }

    #[tokio::test]
    async fn assignment_overrides_are_rejected_before_the_grant_service() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let principal = RequestPrincipal::local_app(
            Arc::from("test-native-operator"),
            crate::request_principal::TransportClass::Loopback,
        );
        for action in ["approve", "deny", "dispatch"] {
            let response = surface()
                .into_router()
                .layer(Extension(principal.clone()))
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(format!(
                            "/v1/coordination/channels/channel/proposals/proposal/{action}"
                        ))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            r#"{"instructions":"override","owner_principal_id":"spoofed"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
}
