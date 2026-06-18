use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::IntoResponse;
use axum::{Json, routing::{delete, get, post}, Router};

use crate::pairing::{
    PairInitRequest, PairVerifyRequest, PairingService,
};

#[derive(Clone)]
pub struct PairingApiState {
    pub service: Arc<PairingService>,
}

pub fn routes() -> Router<PairingApiState> {
    Router::new()
        .route("/pair/status", get(pair_status))
        .route("/pair/iroh-ticket", get(get_iroh_ticket))
        .route("/qr", get(get_qr))
        .route("/qr/image", get(get_qr_image))
        .route("/qr.png", get(get_qr_png))
        .route("/pair/code", get(get_pair_code))
        .route("/pair/init", post(pair_init))
        .route("/pair/verify", post(pair_verify))
        .route("/pair/heartbeat", get(pair_heartbeat))
        .route("/pair/{pairing_id}", delete(revoke_pairing))
}

async fn pair_status(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .pair_status()
        .await
        .map(|response| Json(serde_json::to_value(response).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_iroh_ticket(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.service.iroh_ticket() {
        Some(response) => Ok(Json(serde_json::to_value(response).unwrap_or_default())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_qr(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .current_qr()
        .await
        .map(|response| Json(serde_json::to_value(response).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_qr_png(
    State(state): State<PairingApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let qr = state
        .service
        .current_qr()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let png = state
        .service
        .render_qr_png(&qr.url)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(axum::http::header::CONTENT_TYPE, "image/png")], png))
}

async fn get_qr_image(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .current_qr_image()
        .await
        .map(|response| Json(serde_json::to_value(response).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_pair_code(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .current_short_code()
        .await
        .map(|short_code| Json(serde_json::json!({ "shortCode": short_code })))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn pair_init(
    State(state): State<PairingApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<PairInitRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let source_ip = addr.ip().to_string();
    let response = state
        .service
        .pair_init(body, &source_ip)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = if response.status == "challenge" {
        StatusCode::OK
    } else if response.reason.as_deref() == Some("token_already_used") {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_REQUEST
    };
    Ok((status, Json(serde_json::to_value(response).unwrap_or_default())))
}

async fn pair_verify(
    State(state): State<PairingApiState>,
    Json(body): Json<PairVerifyRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    match state.service.pair_verify(body).await {
        Ok(response) => {
            let status = if response.status == "paired" {
                StatusCode::OK
            } else {
                StatusCode::BAD_REQUEST
            };
            Ok((status, Json(serde_json::to_value(response).unwrap_or_default())))
        }
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn pair_heartbeat(
    State(state): State<PairingApiState>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let token = bearer_token(&headers);
    let response = state
        .service
        .pair_heartbeat(token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = if response.status == "ok" {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    };
    Ok((status, Json(serde_json::to_value(response).unwrap_or_default())))
}

async fn revoke_pairing(
    State(state): State<PairingApiState>,
    Path(pairing_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state
        .service
        .revoke_pairing(&pairing_id)
        .await
        .map(|removed| {
            if removed {
                StatusCode::NO_CONTENT
            } else {
                StatusCode::NOT_FOUND
            }
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
