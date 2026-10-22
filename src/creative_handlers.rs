//! Create uses the workshop's native content boundary and bounded blocking I/O.
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::request_principal::Capability;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{Method, StatusCode},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use medousa_create::{CreativeError, CreativeStore};
use medousa_forge::execution::ForgeExecutionService;
use medousa_types::creative::*;
use std::sync::Arc;

mod execution;

#[derive(Clone)]
pub struct CreativeApiState {
    pub store: Arc<CreativeStore>,
    pub execution: Arc<ForgeExecutionService>,
}
static SERVICE: once_cell::sync::OnceCell<CreativeApiState> = once_cell::sync::OnceCell::new();
pub fn install(state: CreativeApiState) -> anyhow::Result<()> {
    SERVICE
        .set(state)
        .map_err(|_| anyhow::anyhow!("Create service is already installed"))
}
pub(crate) fn service() -> Result<CreativeApiState, stasis::prelude::StasisError> {
    SERVICE.get().cloned().ok_or_else(|| {
        stasis::prelude::StasisError::PortFailure(
            "Create requires a running workshop daemon".into(),
        )
    })
}
type HttpError = (StatusCode, String);
fn error(error: CreativeError) -> HttpError {
    let status = match &error {
        CreativeError::Invalid(_) => StatusCode::BAD_REQUEST,
        CreativeError::NotFound => StatusCode::NOT_FOUND,
        CreativeError::Conflict(_) | CreativeError::Unsupported => StatusCode::CONFLICT,
        CreativeError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string())
}
pub(crate) fn policy(method: Method, path: &'static str, body_limit: usize) -> RoutePolicy {
    let read = method == Method::GET;
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(if read {
            Capability::ContentRead
        } else {
            Capability::ContentWrite
        }),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class: if read {
            RateLimitClass::Read
        } else {
            RateLimitClass::Mutation
        },
    }
}
pub fn surface() -> DeclaredRouter<CreativeApiState> {
    const JSON_LIMIT: usize = MAX_CREATIVE_SCENE_BYTES + 64 * 1024;
    DeclaredRouter::default()
        .methods([
            (policy(Method::GET, "/v1/create/projects", 1024), get(list)),
            (
                policy(Method::POST, "/v1/create/projects", JSON_LIMIT),
                post(create),
            ),
        ])
        .route(
            policy(Method::GET, "/v1/create/projects/{id}", 1024),
            get(read),
        )
        .route(
            RoutePolicy {
                required_capability: Some(Capability::ContentRead),
                rate_limit_class: RateLimitClass::Read,
                ..policy(Method::POST, "/v1/create/projects/{id}/context", 128 * 1024)
            },
            post(context),
        )
        .route(
            policy(
                Method::POST,
                "/v1/create/projects/{id}/transactions",
                JSON_LIMIT,
            ),
            post(transact),
        )
        .route(
            policy(Method::POST, "/v1/create/projects/{id}/history", 1024),
            post(history),
        )
        .route(
            policy(Method::POST, "/v1/create/projects/{id}/delete", 1024),
            post(delete),
        )
        .route(
            policy(
                Method::POST,
                "/v1/create/projects/{id}/assets",
                (MAX_CREATIVE_ASSET_BYTES * 4).div_ceil(3) + 1024,
            ),
            post(import_asset),
        )
        .route(
            policy(
                Method::GET,
                "/v1/create/projects/{id}/assets/{asset_id}",
                1024,
            ),
            get(asset),
        )
        .route(
            policy(Method::POST, "/v1/create/projects/{id}/promote-media", 4096),
            post(promote_media),
        )
        .methods([
            (
                policy(Method::GET, "/v1/create/projects/{id}/proposals", 1024),
                get(proposals),
            ),
            (
                policy(
                    Method::POST,
                    "/v1/create/projects/{id}/proposals",
                    2 * 1024 * 1024,
                ),
                post(propose),
            ),
        ])
        .route(
            policy(
                Method::POST,
                "/v1/create/projects/{id}/proposals/{proposal_id}/decision",
                1024,
            ),
            post(decide),
        )
}
pub(crate) async fn run<T: Send + 'static>(
    state: CreativeApiState,
    work: impl FnOnce(&CreativeStore) -> medousa_create::Result<T> + Send + 'static,
) -> Result<Json<T>, HttpError> {
    // Startup loads the rail, scene, and jobs together. Wait briefly for
    // admission without reducing the shared byte reservation or replaying work.
    execution::run(state.execution, move || work(&state.store))
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?
        .map(Json)
        .map_err(error)
}
async fn list(
    State(state): State<CreativeApiState>,
    Query(query): Query<CreativeListQuery>,
) -> Result<Json<CreativeListResponse>, HttpError> {
    run(state, move |store| store.list(query)).await
}
async fn create(
    State(state): State<CreativeApiState>,
    Json(request): Json<CreativeCreateRequest>,
) -> Result<Json<CreativeDocument>, HttpError> {
    run(state, move |store| store.create(request)).await
}
async fn read(
    State(state): State<CreativeApiState>,
    Path(id): Path<String>,
) -> Result<Json<CreativeMutationResponse>, HttpError> {
    run(state, move |store| store.get(&id)).await
}
async fn transact(
    State(state): State<CreativeApiState>,
    Path(id): Path<String>,
    Json(request): Json<CreativeTransaction>,
) -> Result<Json<CreativeMutationResponse>, HttpError> {
    run(state, move |store| store.transact(&id, request)).await
}
async fn history(
    State(state): State<CreativeApiState>,
    Path(id): Path<String>,
    Json(request): Json<CreativeHistoryRequest>,
) -> Result<Json<CreativeMutationResponse>, HttpError> {
    run(state, move |store| store.history(&id, request)).await
}
async fn delete(
    State(state): State<CreativeApiState>,
    Path(id): Path<String>,
    Json(request): Json<CreativeDeleteRequest>,
) -> Result<Json<CreativeDeleteResponse>, HttpError> {
    run(state, move |store| {
        store.delete(&id, request.expected_revision)?;
        Ok(CreativeDeleteResponse { deleted: true })
    })
    .await
}
async fn import_asset(
    State(state): State<CreativeApiState>,
    Path(id): Path<String>,
    Json(request): Json<CreativeAssetImportRequest>,
) -> Result<Json<CreativeAsset>, HttpError> {
    run(state, move |store| {
        if request.data_base64.len() > MAX_CREATIVE_ASSET_BYTES.div_ceil(3) * 4 {
            return Err(CreativeError::Invalid("encoded asset is too large".into()));
        }
        let bytes = STANDARD
            .decode(request.data_base64)
            .map_err(|_| CreativeError::Invalid("invalid base64 asset".into()))?;
        store.import_asset(&id, &bytes)
    })
    .await
}
async fn asset(
    State(state): State<CreativeApiState>,
    Path((id, asset_id)): Path<(String, String)>,
) -> Result<Json<CreativeAssetData>, HttpError> {
    run(state, move |store| {
        let (asset, bytes) = store.asset(&id, &asset_id)?;
        Ok(CreativeAssetData {
            asset,
            data_base64: STANDARD.encode(bytes),
        })
    })
    .await
}
async fn promote_media(
    State(state): State<CreativeApiState>,
    Path(id): Path<String>,
    Json(request): Json<CreativePromoteMediaRequest>,
) -> Result<Json<CreativeAsset>, HttpError> {
    run(state, move |store| {
        promote_media_into_project(store, &id, &request)
    })
    .await
}
pub(crate) fn promote_media_into_project(
    store: &CreativeStore,
    id: &str,
    request: &CreativePromoteMediaRequest,
) -> medousa_create::Result<CreativeAsset> {
    copy_media(store, id, request, None)
}
pub(crate) fn promote_media_for_job(
    store: &CreativeStore,
    id: &str,
    request: &CreativePromoteMediaRequest,
    job: &medousa_create::CreativeDispatch,
) -> medousa_create::Result<CreativeAsset> {
    copy_media(store, id, request, Some(job))
}
fn copy_media(
    store: &CreativeStore,
    id: &str,
    request: &CreativePromoteMediaRequest,
    job: Option<&medousa_create::CreativeDispatch>,
) -> medousa_create::Result<CreativeAsset> {
    let (_, lease) = crate::session_deletion::acquire_mutation_for_str(&request.session_id)
        .map_err(CreativeError::Invalid)?;
    let record = crate::media_store::get_media_record(&request.session_id, &request.media_id)
        .ok_or(CreativeError::NotFound)?;
    let bytes = crate::media_store::open_media_payload(&record).map_err(CreativeError::Storage)?;
    let asset = match job {
        Some(job) => store.import_job_asset(id, &job.job_id, &job.turn_id, &bytes)?,
        None => store.import_asset(id, &bytes)?,
    };
    drop(lease);
    Ok(asset)
}

async fn context(
    State(state): State<CreativeApiState>,
    Path(id): Path<String>,
    Json(request): Json<CreativeContextRequest>,
) -> Result<Json<CreativeSelectionContext>, HttpError> {
    run(state, move |store| store.context(&id, request)).await
}
async fn proposals(
    State(state): State<CreativeApiState>,
    Path(id): Path<String>,
) -> Result<Json<CreativeProposalListResponse>, HttpError> {
    run(state, move |store| store.proposals(&id)).await
}
async fn propose(
    State(state): State<CreativeApiState>,
    Path(id): Path<String>,
    Json(request): Json<CreativeProposalRequest>,
) -> Result<Json<CreativeProposal>, HttpError> {
    run(state, move |store| store.propose(&id, request)).await
}
async fn decide(
    State(state): State<CreativeApiState>,
    Path((id, proposal_id)): Path<(String, String)>,
    Json(request): Json<CreativeProposalDecision>,
) -> Result<Json<CreativeMutationResponse>, HttpError> {
    run(state, move |store| store.decide(&id, &proposal_id, request)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_routes_require_native_content_authority() {
        let declared = surface();
        let inventory = declared.inventory();
        let routes: Vec<_> = inventory.entries().collect();
        assert_eq!(routes.len(), 13);
        for route in routes {
            assert!(!route.bootstrap_public);
            assert_eq!(route.browser_policy, BrowserPolicy::NativeOnly);
            assert_eq!(
                route.required_capability,
                Some(
                    if route.method == "GET" || route.path.ends_with("/context") {
                        "content.read"
                    } else {
                        "content.write"
                    }
                )
            );
        }
    }
    #[tokio::test]
    async fn http_edits_retry_and_conflict_without_losing_the_saved_document() {
        use crate::request_principal::{RequestPrincipal, TransportClass};
        use axum::{
            Extension,
            body::{Body, to_bytes},
            http::Request,
        };
        use tower::ServiceExt;
        let dir = tempfile::tempdir().unwrap();
        let state = CreativeApiState {
            store: Arc::new(CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap()),
            execution: Arc::new(ForgeExecutionService::new()),
        };
        let router = surface().with_state(state).into_router().layer(Extension(
            RequestPrincipal::local_app(Arc::from("test-create"), TransportClass::Loopback),
        ));
        let creation = serde_json::json!({ "id":"http_project", "title":"Original", "scene":CreativeScene::default() });
        let request = |path: &str, body: serde_json::Value| {
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        let created = router
            .clone()
            .oneshot(request("/v1/create/projects", creation))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::OK);
        let transaction = serde_json::json!({ "id":"rename_once", "expected_revision":0, "label":"Rename", "operations":[{"op":"rename","title":"Saved"}] });
        let changed = router
            .clone()
            .oneshot(request(
                "/v1/create/projects/http_project/transactions",
                transaction.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(changed.status(), StatusCode::OK);
        let duplicate = router
            .clone()
            .oneshot(request(
                "/v1/create/projects/http_project/transactions",
                transaction,
            ))
            .await
            .unwrap();
        let body: CreativeMutationResponse =
            serde_json::from_slice(&to_bytes(duplicate.into_body(), 4096).await.unwrap()).unwrap();
        assert!(body.duplicate);
        assert_eq!(body.document.revision, 1);
        let stale = serde_json::json!({ "id":"stale", "expected_revision":0, "label":"Overwrite", "operations":[{"op":"rename","title":"Lost"}] });
        let conflict = router
            .clone()
            .oneshot(request(
                "/v1/create/projects/http_project/transactions",
                stale,
            ))
            .await
            .unwrap();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let saved = router
            .oneshot(
                Request::builder()
                    .uri("/v1/create/projects/http_project")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let saved: CreativeMutationResponse =
            serde_json::from_slice(&to_bytes(saved.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(saved.document.title, "Saved");
    }
}
