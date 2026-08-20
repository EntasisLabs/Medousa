//! Generated `/v1/integrations*` admin surface — status + upsert, never secret values.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post, put};
use chrono::Utc;
use medousa_secrets::{delete_daemon_secret, ensure_installation_id, save_daemon_secret};
use medousa_types::secrets::{
    ConnectionId, CreateIntegrationConnectionRequest, DaemonSecretPath,
    DeleteIntegrationConnectionResponse, IntegrationConnection, IntegrationConnectionListResponse,
    IntegrationSecretSlot, IntegrationSecretStatus, IntegrationSecretWriteResponse,
    PatchIntegrationConnectionRequest, UpsertIntegrationSecretRequest,
};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::integration_connection::{
    ensure_secrets_bootstrapped, integration_connection_service,
};
use crate::paths::medousa_data_dir;
use crate::request_principal::Capability;

const SECRET_BODY_LIMIT: usize = 16 * 1024;

pub fn surface() -> DeclaredRouter<()> {
    DeclaredRouter::default()
        .route(
            admin_policy(axum::http::Method::GET, "/v1/integrations", 1024),
            get(list_integrations),
        )
        .route(
            admin_policy(axum::http::Method::POST, "/v1/integrations", 4 * 1024),
            post(create_integration),
        )
        .route(
            admin_policy(
                axum::http::Method::GET,
                "/v1/integrations/{connection_id}",
                1024,
            ),
            get(get_integration),
        )
        .route(
            admin_policy(
                axum::http::Method::PATCH,
                "/v1/integrations/{connection_id}",
                4 * 1024,
            ),
            patch(patch_integration),
        )
        .route(
            admin_policy(
                axum::http::Method::DELETE,
                "/v1/integrations/{connection_id}",
                1024,
            ),
            delete(delete_integration),
        )
        .route(
            admin_policy(
                axum::http::Method::PUT,
                "/v1/integrations/{connection_id}/secrets/{slot}",
                SECRET_BODY_LIMIT,
            ),
            put(put_secret),
        )
        .route(
            admin_policy(
                axum::http::Method::DELETE,
                "/v1/integrations/{connection_id}/secrets/{slot}",
                1024,
            ),
            delete(delete_secret),
        )
}

fn admin_policy(method: axum::http::Method, path: &'static str, body_limit: usize) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(Capability::AdminRuntime),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class: RateLimitClass::Administration,
    }
}

async fn list_integrations() -> Result<Json<IntegrationConnectionListResponse>, StatusCode> {
    let _ = ensure_secrets_bootstrapped().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let connections = integration_connection_service()
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(IntegrationConnectionListResponse { connections }))
}

async fn create_integration(
    Json(body): Json<CreateIntegrationConnectionRequest>,
) -> Result<Json<IntegrationConnection>, StatusCode> {
    let _ = ensure_secrets_bootstrapped().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let kind = body.kind.trim();
    if kind.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let now = Utc::now();
    let record = IntegrationConnection {
        connection_id: ConnectionId::parse(&uuid::Uuid::new_v4().to_string())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        kind: kind.to_string(),
        label: body.label,
        base_url: body.base_url,
        secrets: IntegrationSecretStatus::default(),
        created_at: now,
        updated_at: now,
    };
    integration_connection_service()
        .upsert_record(record.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(record))
}

async fn get_integration(
    Path(connection_id): Path<String>,
) -> Result<Json<IntegrationConnection>, StatusCode> {
    let id = ConnectionId::parse(&connection_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let record = integration_connection_service()
        .get(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(record))
}

async fn patch_integration(
    Path(connection_id): Path<String>,
    Json(body): Json<PatchIntegrationConnectionRequest>,
) -> Result<Json<IntegrationConnection>, StatusCode> {
    let id = ConnectionId::parse(&connection_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let service = integration_connection_service();
    let mut record = service
        .get(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if let Some(label) = body.label {
        record.label = Some(label);
    }
    if let Some(base_url) = body.base_url {
        record.base_url = Some(base_url);
    }
    if let Some(kind) = body.kind {
        let trimmed = kind.trim();
        if trimmed.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        record.kind = trimmed.to_string();
    }
    record.updated_at = Utc::now();
    service
        .upsert_record(record.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(record))
}

async fn delete_integration(
    Path(connection_id): Path<String>,
) -> Result<Json<DeleteIntegrationConnectionResponse>, StatusCode> {
    let id = ConnectionId::parse(&connection_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let deleted = integration_connection_service()
        .delete_record(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(DeleteIntegrationConnectionResponse { deleted }))
}

async fn put_secret(
    Path((connection_id, slot)): Path<(String, String)>,
    Json(body): Json<UpsertIntegrationSecretRequest>,
) -> Result<Json<IntegrationSecretWriteResponse>, StatusCode> {
    let id = ConnectionId::parse(&connection_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let slot = IntegrationSecretSlot::parse(&slot).map_err(|_| StatusCode::BAD_REQUEST)?;
    let value = body.value.trim();
    if value.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let installation_id =
        ensure_installation_id(&medousa_data_dir()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service = integration_connection_service();
    let _ = service
        .get(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let path = DaemonSecretPath::Integration {
        installation_id,
        connection_id: id.clone(),
        slot,
    };
    save_daemon_secret(&medousa_data_dir(), &path, value)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    service
        .set_slot_presence(&id, slot, true)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(IntegrationSecretWriteResponse {
        connection_id: id,
        slot,
        configured: true,
    }))
}

async fn delete_secret(
    Path((connection_id, slot)): Path<(String, String)>,
) -> Result<Json<IntegrationSecretWriteResponse>, StatusCode> {
    let id = ConnectionId::parse(&connection_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let slot = IntegrationSecretSlot::parse(&slot).map_err(|_| StatusCode::BAD_REQUEST)?;
    let installation_id =
        ensure_installation_id(&medousa_data_dir()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let service = integration_connection_service();
    let _ = service
        .get(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let path = DaemonSecretPath::Integration {
        installation_id,
        connection_id: id.clone(),
        slot,
    };
    delete_daemon_secret(&medousa_data_dir(), &path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    service
        .set_slot_presence(&id, slot, false)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(IntegrationSecretWriteResponse {
        connection_id: id,
        slot,
        configured: false,
    }))
}
