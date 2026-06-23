use axum::{http::StatusCode, routing::put, Json, Router};

use crate::inference_profiles::{self, InferenceProfilesConfig};
use crate::session::{load_tui_defaults, save_tui_defaults};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PutInferenceProfilesRequest {
    pub inference_profiles: InferenceProfilesConfig,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PutInferenceProfilesResponse {
    pub inference_profiles: InferenceProfilesConfig,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
}

pub fn routes() -> Router {
    Router::new().route("/v1/runtime/inference-profiles", put(put_inference_profiles))
}

async fn put_inference_profiles(
    Json(request): Json<PutInferenceProfilesRequest>,
) -> Result<Json<PutInferenceProfilesResponse>, (StatusCode, String)> {
    let mut defaults = load_tui_defaults();
    inference_profiles::apply_profiles(&mut defaults, request.inference_profiles)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))?;
    save_tui_defaults(&defaults);
    let main = inference_profiles::main_target(&defaults);
    Ok(Json(PutInferenceProfilesResponse {
        inference_profiles: defaults
            .inference_profiles
            .clone()
            .unwrap_or_default(),
        provider: main.provider,
        model: main.model,
        base_url: main.base_url,
    }))
}
