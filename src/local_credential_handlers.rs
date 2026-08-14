//! Operator diagnostics and lifecycle operations for first-party local credentials.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use medousa_local_credential::{LocalCredentialSet, LocalCredentialSummary};
use serde::Serialize;

use crate::credential_lifecycle::{
    CredentialKind, CredentialLifecycle, CredentialLifecycleSnapshot,
};
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::request_principal::Capability;

#[derive(Clone)]
pub struct LocalCredentialApiState {
    pub data_dir: PathBuf,
    pub credentials: Arc<LocalCredentialSet>,
    pub lifecycle: CredentialLifecycle,
    pub operation_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalCredentialDiagnostics {
    credentials: Vec<LocalCredentialSummary>,
    lifecycle: CredentialLifecycleSnapshot,
}

pub fn surface() -> DeclaredRouter<LocalCredentialApiState> {
    DeclaredRouter::default()
        .route(
            policy(axum::http::Method::GET, "/v1/admin/local-credentials"),
            get(list_credentials),
        )
        .route(
            policy(
                axum::http::Method::POST,
                "/v1/admin/local-credentials/{name}/rotate",
            ),
            post(rotate_credential),
        )
        .route(
            policy(
                axum::http::Method::DELETE,
                "/v1/admin/local-credentials/{name}",
            ),
            delete(revoke_credential),
        )
}

fn policy(method: axum::http::Method, path: &'static str) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(Capability::AdminIdentity),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: 1024,
        rate_limit_class: RateLimitClass::Administration,
    }
}

async fn list_credentials(
    State(state): State<LocalCredentialApiState>,
) -> Result<Json<LocalCredentialDiagnostics>, StatusCode> {
    let _guard = state.operation_lock.lock().await;
    let credentials = medousa_local_credential::list_local_credentials(&state.data_dir)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(LocalCredentialDiagnostics {
        credentials,
        lifecycle: state.lifecycle.snapshot(),
    }))
}

async fn rotate_credential(
    State(state): State<LocalCredentialApiState>,
    Path(name): Path<String>,
) -> Result<Json<LocalCredentialSummary>, StatusCode> {
    let _guard = state.operation_lock.lock().await;
    let rotation = medousa_local_credential::rotate_named(&state.data_dir, &name)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let credential_id: Arc<str> = Arc::from(rotation.verifier.credential_id());
    let generation = rotation.verifier.generation();
    state.credentials.replace(rotation.verifier);
    if let Some(revoked_generation) = rotation.revoked_generation {
        state.lifecycle.revoke(
            credential_id.clone(),
            revoked_generation,
            CredentialKind::LocalApp,
            "local_credential_rotated",
        );
    }
    state
        .lifecycle
        .record_rotation(credential_id, generation, CredentialKind::LocalApp);
    let summary = medousa_local_credential::list_local_credentials(&state.data_dir)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|entry| entry.name == name)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(summary))
}

async fn revoke_credential(
    State(state): State<LocalCredentialApiState>,
    Path(name): Path<String>,
) -> Result<Json<LocalCredentialSummary>, StatusCode> {
    let _guard = state.operation_lock.lock().await;
    let summary = medousa_local_credential::revoke_named(&state.data_dir, &name)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    state.credentials.revoke(&name);
    state.lifecycle.revoke(
        Arc::from(summary.credential_id.as_str()),
        summary.generation,
        CredentialKind::LocalApp,
        "local_credential_revoked",
    );
    Ok(Json(summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_credential_inventory_is_admin_native_only() {
        for entry in surface().inventory().entries() {
            assert_eq!(entry.group, RouteGroup::Administration);
            assert_eq!(entry.required_capability, Some("admin.identity"));
            assert_eq!(entry.browser_policy, BrowserPolicy::NativeOnly);
        }
    }
}
