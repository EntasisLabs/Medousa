use axum::{
    Json,
    extract::Multipart,
    http::StatusCode,
    routing::{get, post},
};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::stt::{self, SttStatusResponse, SttTranscribeResponse};

pub fn surface() -> DeclaredRouter {
    DeclaredRouter::default()
        .route(
            stt_policy(
                axum::http::Method::GET,
                "/v1/stt/status",
                crate::request_principal::Capability::WorkshopRead,
                1024,
                RateLimitClass::Read,
            ),
            get(stt_status),
        )
        .route(
            stt_policy(
                axum::http::Method::POST,
                "/v1/stt/transcribe",
                crate::request_principal::Capability::WorkshopInteract,
                32 * 1024 * 1024,
                RateLimitClass::Mutation,
            ),
            post(stt_transcribe),
        )
}

fn stt_policy(
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

async fn stt_status() -> Json<SttStatusResponse> {
    Json(stt::stt_status())
}

async fn stt_transcribe(
    mut multipart: Multipart,
) -> Result<Json<SttTranscribeResponse>, (StatusCode, String)> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut mime: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?
    {
        if field.name().unwrap_or_default() == "file" {
            mime = field.content_type().map(str::to_string);
            let bytes = field
                .bytes()
                .await
                .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let bytes = file_bytes.ok_or((
        StatusCode::BAD_REQUEST,
        "multipart field 'file' is required".to_string(),
    ))?;

    let mime_type = mime.as_deref().unwrap_or("audio/webm");
    stt::transcribe_audio(&bytes, mime_type)
        .await
        .map(Json)
        .map_err(|failure| (StatusCode::BAD_REQUEST, failure.operator_message))
}
