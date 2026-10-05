use super::*;
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use axum::{
    Json,
    extract::{Extension, Path},
    http::{Method, StatusCode},
    routing::{delete, get, post},
};
type HttpError = (StatusCode, String);

pub fn surface() -> DeclaredRouter {
    DeclaredRouter::default()
        .route(policy(Method::GET, "/v1/ssh/targets"), get(list))
        .route(
            policy(Method::POST, "/v1/ssh/inspect"),
            post(inspect_target),
        )
        .route(policy(Method::POST, "/v1/ssh/targets"), post(save))
        .route(
            policy(Method::POST, "/v1/ssh/targets/{id}/access"),
            post(access),
        )
        .route(
            policy(Method::DELETE, "/v1/ssh/targets/{id}"),
            delete(remove),
        )
        .route(policy(Method::POST, "/v1/ssh/terminal"), post(terminal))
        .route(policy(Method::POST, "/v1/ssh/test"), post(test_connection))
}
fn policy(method: Method, path: &'static str) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(Capability::AdminRuntime),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: 64 * 1024,
        rate_limit_class: RateLimitClass::Administration,
    }
}
fn error(e: impl std::fmt::Display) -> HttpError {
    (StatusCode::BAD_REQUEST, e.to_string())
}
fn request_owner(principal: &RequestPrincipal) -> Result<String, HttpError> {
    if !principal.capabilities().contains(Capability::AdminRuntime) {
        return Err((
            StatusCode::FORBIDDEN,
            "SSH setup requires operator access".into(),
        ));
    }
    let active = crate::user_profiles::resolve_workshop_active_profile_id();
    owner(principal, Some(&active)).map_err(error)
}
async fn list(
    Extension(p): Extension<RequestPrincipal>,
) -> Result<Json<serde_json::Value>, HttpError> {
    Ok(Json(
        local_host()
            .map_err(error)?
            .targets(&request_owner(&p)?, false)
            .await,
    ))
}
async fn inspect_target(
    Extension(p): Extension<RequestPrincipal>,
    Json(config): Json<TargetConfig>,
) -> Result<Json<serde_json::Value>, HttpError> {
    request_owner(&p)?;
    Ok(Json(inspect(config).await.map_err(error)?))
}
async fn save(
    Extension(p): Extension<RequestPrincipal>,
    Json(input): Json<SaveTarget>,
) -> Result<Json<serde_json::Value>, HttpError> {
    Ok(Json(
        local_host()
            .map_err(error)?
            .save(request_owner(&p)?, input)
            .await
            .map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessInput {
    enabled: bool,
}
async fn access(
    Extension(p): Extension<RequestPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<AccessInput>,
) -> Result<Json<serde_json::Value>, HttpError> {
    local_host()
        .map_err(error)?
        .set_access(&request_owner(&p)?, &id, input.enabled)
        .await
        .map_err(error)?;
    Ok(Json(serde_json::json!({"ok": true})))
}
async fn remove(
    Extension(p): Extension<RequestPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    local_host()
        .map_err(error)?
        .remove(&request_owner(&p)?, &id)
        .await
        .map_err(error)?;
    Ok(Json(serde_json::json!({"ok": true})))
}
async fn terminal(
    Extension(p): Extension<RequestPrincipal>,
    Json(input): Json<TerminalInput>,
) -> Result<Json<serde_json::Value>, HttpError> {
    Ok(Json(
        local_host()
            .map_err(error)?
            .open_terminal(&request_owner(&p)?, input, false)
            .await
            .map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TestInput {
    target_id: String,
}
async fn test_connection(
    Extension(p): Extension<RequestPrincipal>,
    Json(input): Json<TestInput>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let host = local_host().map_err(error)?;
    let target = host
        .access(&request_owner(&p)?, &input.target_id, false)
        .await
        .map_err(error)?;
    let known_hosts = process::prepare_known_hosts(&host.root, &target)
        .await
        .map_err(error)?;
    let args = process::argv(&target, &known_hosts, Some("true"));
    let result = host
        .execution
        .run_async(
            ExecutionClass::WorkEnvironment,
            256 * 1024,
            None,
            async move {
                Ok(async {
                    let mut child = process::ssh_command(&args).spawn()?;
                    let stderr = process::drain(child.stderr.take().expect("piped stderr"));
                    let stdout = process::drain(child.stdout.take().expect("piped stdout"));
                    let (status, out, err) = tokio::join!(child.wait(), stdout, stderr);
                    let status = status?;
                    out?;
                    let (detail, _) = err?;
                    if !status.success() {
                        bail!("SSH connection failed: {}", detail.trim());
                    }
                    Ok::<_, anyhow::Error>(serde_json::json!({"ok": true, "message": "Connected"}))
                }
                .await)
            },
        )
        .await
        .map_err(error)?
        .map_err(error)?;
    Ok(Json(result))
}
