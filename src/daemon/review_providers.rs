//! Provider-neutral review package discovery and dispatch. Installed executables
//! implement the versioned medousa-types protocol; this module knows no hosts,
//! provider APIs, CLI arguments, or provider-specific review semantics.
mod auth;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::{Method, StatusCode};
use axum::routing::{get, post};
use medousa_forge::error::ForgeError;
use medousa_forge::execution::{
    ExecutionClass, ForgeExecutionService, supervise_command_with_input,
};
use medousa_types::review_provider::*;
use medousa_types::secrets::{ConnectionId, DaemonSecretPath, IntegrationSecretSlot};
use tokio::sync::Mutex;

use super::route_policy::{BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy};
use super::state::AppState;
use crate::request_principal::Capability;

const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_PROVIDERS: usize = 32;
#[derive(Clone)]
struct InstalledProvider {
    descriptor: ReviewProviderDescriptor,
    binary: PathBuf,
}
struct Catalog {
    scanned: Instant,
    directories: Vec<PathBuf>,
    providers: Vec<InstalledProvider>,
    problems: Vec<String>,
}
static CATALOG: LazyLock<Mutex<Option<Catalog>>> = LazyLock::new(|| Mutex::new(None));

pub fn surface() -> DeclaredRouter<AppState> {
    DeclaredRouter::default()
        .route(policy(Method::GET, "/v1/reviews/providers"), get(list))
        .route(policy(Method::POST, "/v1/reviews/query"), post(query))
        .route(
            write_policy("/v1/reviews/mutate", Capability::ContentWrite),
            post(mutate),
        )
        .route(
            write_policy("/v1/reviews/auth", Capability::AdminRuntime),
            post(auth::handle),
        )
}
fn policy(method: Method, path: &'static str) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(Capability::ContentRead),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: 128 * 1024,
        rate_limit_class: RateLimitClass::Read,
    }
}
fn write_policy(path: &'static str, capability: Capability) -> RoutePolicy {
    RoutePolicy {
        required_capability: Some(capability),
        rate_limit_class: RateLimitClass::Mutation,
        ..policy(Method::POST, path)
    }
}
fn provider_error(
    code: ReviewProviderErrorCode,
    message: impl Into<String>,
) -> ReviewProviderError {
    ReviewProviderError {
        code,
        message: message.into(),
    }
}
fn directories() -> Vec<PathBuf> {
    let mut directories = vec![];
    if let Some(sibling) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        directories.push(sibling);
    }
    let installed = crate::paths::medousa_data_dir().join("bin");
    if !directories.contains(&installed) {
        directories.push(installed);
    }
    directories
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
}

async fn binaries(
    execution: &ForgeExecutionService,
    directories: Vec<PathBuf>,
) -> Result<Vec<PathBuf>, ForgeError> {
    execution
        .run(ExecutionClass::RepositoryMetadata, 32 * 1024, move || {
            let mut found = BTreeMap::new();
            for directory in directories {
                let entries = match std::fs::read_dir(directory) {
                    Ok(entries) => entries,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => {
                        return Err(ForgeError::Store(format!(
                            "Cannot read installed review packages: {error}"
                        )));
                    }
                };
                for entry in entries {
                    let entry = entry.map_err(|e| ForgeError::Store(e.to_string()))?;
                    let name = entry.file_name().to_string_lossy().to_string();
                    let stem = name.strip_suffix(".exe").unwrap_or(&name);
                    if let Some(id) = stem
                        .strip_prefix(REVIEW_PROVIDER_BINARY_PREFIX)
                        .filter(|id| valid_id(id))
                    {
                        // Prefer the sibling of current_exe over dataDir/bin.
                        if entry.path().is_file() {
                            found.entry(id.to_string()).or_insert(entry.path());
                        }
                    }
                    if found.len() > MAX_PROVIDERS {
                        return Err(ForgeError::Overloaded(
                            "Too many installed review packages".into(),
                        ));
                    }
                }
            }
            Ok(found.into_values().collect())
        })
        .await
}

async fn package_call(
    execution: &ForgeExecutionService,
    binary: PathBuf,
    argument: &str,
    input: Vec<u8>,
    timeout: Duration,
    max_output: usize,
) -> Result<Vec<u8>, ForgeError> {
    let argument = argument.to_string();
    // Estimate retained input and expected response size for class validation.
    // The full output ceiling is enforced independently by capture.
    let estimated_bytes = input.len().saturating_add(max_output.min(256 * 1024));
    execution
        .run_async(
            ExecutionClass::NetworkGit,
            estimated_bytes,
            None,
            async move {
                let (output, _, truncated, status) = supervise_command_with_input(
                    binary,
                    None,
                    vec![argument],
                    vec![],
                    input,
                    timeout,
                    max_output,
                )
                .await?;
                if truncated || !status.success() {
                    return Err(ForgeError::Command(
                        "Review package failed or exceeded its output limit".into(),
                    ));
                }
                Ok(output)
            },
        )
        .await
}

async fn discover(
    execution: &ForgeExecutionService,
    force: bool,
) -> Result<(Vec<InstalledProvider>, Vec<String>), ForgeError> {
    let directories = directories();
    // Serialize discovery only; network review requests never hold this lock.
    let mut catalog = CATALOG.lock().await;
    if !force
        && let Some(cache) = catalog.as_ref()
        && cache.directories == directories
        && cache.scanned.elapsed() < Duration::from_secs(30)
    {
        return Ok((cache.providers.clone(), cache.problems.clone()));
    }
    let mut providers = BTreeMap::new();
    let mut problems = vec![];
    for binary in binaries(execution, directories.clone()).await? {
        let metadata = package_call(
            execution,
            binary.clone(),
            "--describe",
            vec![],
            Duration::from_secs(3),
            32 * 1024,
        )
        .await
        .and_then(|bytes| {
            serde_json::from_slice::<ReviewProviderDescriptor>(&bytes)
                .map_err(|_| ForgeError::Command("Invalid review package descriptor".into()))
        });
        match metadata {
            Ok(descriptor)
                if descriptor.protocol_version == REVIEW_PROVIDER_PROTOCOL_VERSION
                    && valid_id(&descriptor.id)
                    && descriptor.label.len() <= 128
                    && descriptor.hosts.len() <= 32
                    && descriptor.operations.len() <= 32 =>
            {
                if providers.contains_key(&descriptor.id) {
                    problems.push(format!("Duplicate review provider: {}", descriptor.id));
                    continue;
                }
                providers.insert(
                    descriptor.id.clone(),
                    InstalledProvider { descriptor, binary },
                );
            }
            _ => problems.push(format!(
                "Could not load review package {}",
                binary.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    let providers: Vec<_> = providers.into_values().collect();
    *catalog = Some(Catalog {
        scanned: Instant::now(),
        directories,
        providers: providers.clone(),
        problems: problems.clone(),
    });
    Ok((providers, problems))
}

async fn list(
    State(state): State<AppState>,
) -> Result<Json<ReviewProviderListResponse>, (StatusCode, String)> {
    let (providers, problems) = discover(&state.forge_execution, true).await.map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Could not discover review packages".into(),
        )
    })?;
    Ok(Json(ReviewProviderListResponse {
        providers: providers.into_iter().map(|p| p.descriptor).collect(),
        problems,
    }))
}
async fn query(
    State(state): State<AppState>,
    Json(query): Json<ReviewProviderQuery>,
) -> Json<ReviewProviderResponse> {
    Json(ReviewProviderResponse::from_result(
        dispatch(&state.forge_execution, query).await,
    ))
}

fn validate_query(query: &ReviewProviderQuery) -> Result<(), ReviewProviderError> {
    if !valid_id(&query.provider_id)
        || query.host.is_empty()
        || query.host.len() > 253
        || query.host.starts_with('-')
        || !query
            .host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
    {
        return Err(provider_error(
            ReviewProviderErrorCode::InvalidParameter,
            "Choose a provider and a hostname without a scheme or path.",
        ));
    }
    if let Some(reference) = query.request.review_ref()
        && (reference.provider_id != query.provider_id || reference.host != query.host)
    {
        return Err(provider_error(
            ReviewProviderErrorCode::InvalidParameter,
            "The review belongs to another provider or host.",
        ));
    }
    Ok(())
}

async fn credential(
    execution: &ForgeExecutionService,
    query: &ReviewProviderQuery,
) -> Result<Option<String>, ReviewProviderError> {
    let Some(id) = &query.connection_id else {
        return Ok(None);
    };
    let id = ConnectionId::parse(id).map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::InvalidParameter,
            "Invalid connection identity.",
        )
    })?;
    let connection = crate::integration_connection::integration_connection_service()
        .get(&id)
        .await
        .map_err(|_| {
            provider_error(
                ReviewProviderErrorCode::Unavailable,
                "Cannot read the selected connection.",
            )
        })?
        .ok_or_else(|| {
            provider_error(
                ReviewProviderErrorCode::NotFound,
                "The selected connection no longer exists.",
            )
        })?;
    let connection_host = connection
        .base_url
        .as_deref()
        .and_then(|url| reqwest::Url::parse(url).ok())
        .and_then(|url| url.host_str().map(str::to_string));
    if connection.kind != query.provider_id || connection_host.as_deref() != Some(&query.host) {
        return Err(provider_error(
            ReviewProviderErrorCode::InvalidParameter,
            "The connection does not belong to this provider and host.",
        ));
    }
    let root = crate::paths::medousa_data_dir();
    execution
        .run(ExecutionClass::StoreIo, 64 * 1024, move || {
            let installation_id = medousa_secrets::ensure_installation_id(&root)
                .map_err(|e| ForgeError::Store(e.to_string()))?;
            for slot in [
                IntegrationSecretSlot::OauthBundle,
                IntegrationSecretSlot::ApiKey,
            ] {
                if !connection.secrets.slot(slot) {
                    continue;
                }
                let path = DaemonSecretPath::Integration {
                    installation_id: installation_id.clone(),
                    connection_id: id.clone(),
                    slot,
                };
                if let Some(secret) = medousa_secrets::load_daemon_secret(&root, &path)
                    .map_err(|e| ForgeError::Store(e.to_string()))?
                {
                    return Ok(Some(secret.value));
                }
            }
            Err(ForgeError::Store(
                "Selected connection has no credential".into(),
            ))
        })
        .await
        .map_err(|_| {
            provider_error(
                ReviewProviderErrorCode::NotAuthenticated,
                "Could not load the selected connection's credential.",
            )
        })
}

pub(super) async fn dispatch(
    execution: &Arc<ForgeExecutionService>,
    query: ReviewProviderQuery,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    validate_query(&query)?;
    let (providers, _) = discover(execution, false).await.map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::Unavailable,
            "Could not discover review packages.",
        )
    })?;
    let provider = providers
        .into_iter()
        .find(|p| p.descriptor.id == query.provider_id)
        .ok_or_else(|| {
            provider_error(
                ReviewProviderErrorCode::Unavailable,
                "Install a review provider on the connected workshop (Settings → Packages for local workshops).",
            )
        })?;
    if !provider
        .descriptor
        .operations
        .iter()
        .any(|name| name == query.request.name())
    {
        return Err(provider_error(
            ReviewProviderErrorCode::Unsupported,
            "This package does not support the requested review operation.",
        ));
    }
    let credential = selected_credential(execution, &provider, &query).await?;
    let input = serde_json::to_vec(&ReviewProviderInvocation {
        protocol_version: REVIEW_PROVIDER_PROTOCOL_VERSION,
        query,
        credential,
    })
    .map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::InvalidParameter,
            "Invalid review request.",
        )
    })?;
    let output = package_call(
        execution,
        provider.binary,
        "--invoke",
        input,
        Duration::from_secs(90),
        MAX_OUTPUT,
    )
    .await
    .map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::Unavailable,
            "The review package failed or timed out. Try again.",
        )
    })?;
    let response: ReviewProviderResponse = serde_json::from_slice(&output).map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::ProviderError,
            "The review package returned an invalid response.",
        )
    })?;
    if response.protocol_version != REVIEW_PROVIDER_PROTOCOL_VERSION {
        return Err(provider_error(
            ReviewProviderErrorCode::Unsupported,
            "The review package uses an unsupported protocol version.",
        ));
    }
    match response.outcome {
        ReviewProviderOutcome::Success { result } => Ok(result),
        ReviewProviderOutcome::Error { error } => Err(error),
    }
}

async fn installed(
    execution: &ForgeExecutionService,
    id: &str,
    operation: &str,
) -> Result<InstalledProvider, ReviewProviderError> {
    let (providers, _) = discover(execution, false).await.map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::Unavailable,
            "Could not discover review packages.",
        )
    })?;
    providers
        .into_iter()
        .find(|p| p.descriptor.id == id && p.descriptor.operations.iter().any(|op| op == operation))
        .ok_or_else(|| {
            provider_error(
                ReviewProviderErrorCode::Unavailable,
                "Install a review package that supports this action on the connected workshop.",
            )
        })
}
async fn mutate(
    State(state): State<AppState>,
    Json(request): Json<ReviewProviderMutation>,
) -> Json<ReviewProviderResponse> {
    Json(ReviewProviderResponse::from_result(
        publish(&state.forge_execution, request).await,
    ))
}
pub(super) async fn publish(
    execution: &Arc<ForgeExecutionService>,
    mutation: ReviewProviderMutation,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    let query = ReviewProviderQuery {
        provider_id: mutation.provider_id.clone(),
        host: mutation.host.clone(),
        connection_id: mutation.connection_id.clone(),
        request: ReviewProviderOperation::ConnectionStatus,
    };
    validate_query(&query)?;
    if let Some(reference) = mutation.request.review_ref()
        && (reference.provider_id != mutation.provider_id || reference.host != mutation.host)
    {
        return Err(provider_error(
            ReviewProviderErrorCode::InvalidParameter,
            "The review belongs to another provider or host.",
        ));
    }
    let provider = installed(execution, &mutation.provider_id, mutation.request.name()).await?;
    let credential = selected_credential(execution, &provider, &query).await?;
    let input = serde_json::to_vec(&ReviewProviderWriteInvocation {
        protocol_version: REVIEW_PROVIDER_PROTOCOL_VERSION,
        mutation,
        credential,
    })
    .map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::InvalidParameter,
            "Invalid review write request.",
        )
    })?;
    let output = package_call(execution, provider.binary, "--mutate", input, Duration::from_secs(90), MAX_OUTPUT).await
        .map_err(|_| provider_error(ReviewProviderErrorCode::Unavailable, "The review package did not confirm this action. Refresh the review to check its result before submitting again."))?;
    let response: ReviewProviderResponse = serde_json::from_slice(&output).map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::ProviderError,
            "The review package returned an invalid response.",
        )
    })?;
    if response.protocol_version != REVIEW_PROVIDER_PROTOCOL_VERSION {
        return Err(provider_error(
            ReviewProviderErrorCode::Unsupported,
            "Unsupported provider protocol.",
        ));
    }
    match response.outcome {
        ReviewProviderOutcome::Success { result } => Ok(result),
        ReviewProviderOutcome::Error { error } => Err(error),
    }
}

// Serialize credential rotation per connection. Discovery and CLI fallback never
// unlock keychain. Retain a rotated bundle only if persistence failed, so retry
// saves it instead of redeeming an already-invalidated refresh token again.
struct PendingCredential {
    provider_id: String,
    host: String,
    value: String,
}
type CredentialLocks = BTreeMap<String, Arc<Mutex<Option<PendingCredential>>>>;
static CREDENTIAL_LOCKS: LazyLock<Mutex<CredentialLocks>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
async fn selected_credential(
    execution: &Arc<ForgeExecutionService>,
    provider: &InstalledProvider,
    query: &ReviewProviderQuery,
) -> Result<Option<String>, ReviewProviderError> {
    let Some(id) = &query.connection_id else {
        return Ok(None);
    };
    let lock = {
        let mut locks = CREDENTIAL_LOCKS.lock().await;
        locks.retain(|_, lock| {
            Arc::strong_count(lock) > 1 || lock.try_lock().map_or(true, |value| value.is_some())
        });
        if let Some(lock) = locks.get(id) {
            lock.clone()
        } else {
            if locks.len() >= 128 {
                return Err(provider_error(
                    ReviewProviderErrorCode::Unavailable,
                    "Finish saving pending account credentials before connecting more accounts.",
                ));
            }
            let lock = Arc::new(Mutex::new(None));
            locks.insert(id.clone(), lock.clone());
            lock
        }
    };
    let mut pending = lock.lock().await;
    if let Some(rotated) = pending.as_ref() {
        let connection = crate::integration_connection::integration_connection_service()
            .get(&ConnectionId::parse(id).map_err(|_| {
                provider_error(
                    ReviewProviderErrorCode::InvalidParameter,
                    "Invalid connection identity.",
                )
            })?)
            .await
            .map_err(|_| {
                provider_error(
                    ReviewProviderErrorCode::Unavailable,
                    "Cannot read the selected connection.",
                )
            })?;
        let valid = connection.is_some_and(|connection| {
            connection.kind == query.provider_id
                && connection
                    .base_url
                    .as_deref()
                    .and_then(|base| reqwest::Url::parse(base).ok())
                    .and_then(|base| base.host_str().map(str::to_owned))
                    .as_deref()
                    == Some(&query.host)
        });
        if !valid || rotated.provider_id != query.provider_id || rotated.host != query.host {
            *pending = None;
            return Err(provider_error(
                ReviewProviderErrorCode::InvalidParameter,
                "The selected connection changed. Connect the account again.",
            ));
        }
        auth::save_secret(execution, id, &rotated.value).await?;
        return Ok(pending.take().map(|saved| saved.value));
    }
    let credential = credential(execution, query).await?;
    if !provider
        .descriptor
        .operations
        .iter()
        .any(|op| op == "credential_refresh")
    {
        return Ok(credential);
    }
    let Some(saved) = credential else {
        return Ok(None);
    };
    match auth::call(
        execution,
        provider,
        &query.host,
        ReviewProviderAuthOperation::Refresh {
            credential: saved.clone(),
        },
    )
    .await?
    {
        ReviewProviderAuthResult::Authorized { credential, .. } => {
            if credential != saved {
                *pending = Some(PendingCredential {
                    provider_id: query.provider_id.clone(),
                    host: query.host.clone(),
                    value: credential.clone(),
                });
                auth::save_secret(execution, id, &credential).await?;
                *pending = None;
            }
            Ok(Some(credential))
        }
        _ => Err(provider_error(
            ReviewProviderErrorCode::ProviderError,
            "The package returned an invalid credential response.",
        )),
    }
}

/// Only transport syntax is parsed here. Each package resolves its own repository
/// identity, including namespace rules and host-specific URL conventions.
pub(super) async fn repository_target(
    execution: &Arc<ForgeExecutionService>,
    remote: &str,
    provider_id: Option<&str>,
) -> Result<(String, String, String), ReviewProviderError> {
    let normalized = if let Some(scp) = remote.strip_prefix("git@") {
        format!("ssh://git@{}", scp.replacen(':', "/", 1))
    } else {
        remote.to_owned()
    };
    let url = reqwest::Url::parse(&normalized).map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::InvalidParameter,
            "Origin must have one SSH or HTTPS push URL.",
        )
    })?;
    if remote.lines().count() != 1 {
        return Err(provider_error(
            ReviewProviderErrorCode::InvalidParameter,
            "Choose one origin push destination.",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| {
            provider_error(
                ReviewProviderErrorCode::InvalidParameter,
                "Origin has no hostname.",
            )
        })?
        .to_string();
    let (providers, _) = discover(execution, false).await.map_err(|_| {
        provider_error(
            ReviewProviderErrorCode::Unavailable,
            "Could not discover review packages.",
        )
    })?;
    let candidates: Vec<_> = providers
        .iter()
        .filter(|p| {
            p.descriptor
                .operations
                .iter()
                .any(|op| op == "resolve_repository")
                && provider_id.is_none_or(|id| id == p.descriptor.id)
        })
        .collect();
    let provider = candidates
        .iter()
        .find(|p| p.descriptor.hosts.contains(&host))
        .copied()
        .or_else(|| (candidates.len() == 1).then(|| candidates[0]))
        .ok_or_else(|| {
            provider_error(
                ReviewProviderErrorCode::Unavailable,
                "Choose an installed review package for this repository host.",
            )
        })?;
    let result = dispatch(
        execution,
        ReviewProviderQuery {
            provider_id: provider.descriptor.id.clone(),
            host: host.clone(),
            connection_id: None,
            request: ReviewProviderOperation::ResolveRepository {
                remote_url: remote.into(),
            },
        },
    )
    .await?;
    match result {
        ReviewProviderResult::Repository {
            provider_id,
            host: result_host,
            repository,
        } if provider_id == provider.descriptor.id && result_host == host => {
            Ok((provider_id, host, repository))
        }
        _ => Err(provider_error(
            ReviewProviderErrorCode::ProviderError,
            "The package returned an invalid repository identity.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_write_and_account_administration_are_distinct_routes() {
        let surface = surface();
        let capability = |path| {
            surface
                .inventory()
                .entries()
                .find(|entry| entry.path == path)
                .unwrap()
                .required_capability
        };
        assert_eq!(capability("/v1/reviews/query"), Some("content.read"));
        assert_eq!(capability("/v1/reviews/mutate"), Some("content.write"));
        assert_eq!(capability("/v1/reviews/auth"), Some("admin.runtime"));
        let write = serde_json::json!({"provider_id":"example","host":"example.test","request":{"operation":"publish","repository":"group/repo","head":"feature","base":"main","title":"Title","body":"","draft":true,"update_existing":false}});
        assert!(serde_json::from_value::<ReviewProviderQuery>(write.clone()).is_err());
        assert!(serde_json::from_value::<ReviewProviderMutation>(write).is_ok());
    }
    #[test]
    fn identity_validation_is_provider_neutral() {
        let mut query = ReviewProviderQuery {
            provider_id: "example_host".into(),
            host: "git.example.test".into(),
            connection_id: None,
            request: ReviewProviderOperation::Get {
                review: HostedReviewRef {
                    provider_id: "example_host".into(),
                    host: "git.example.test".into(),
                    repository: "group/subgroup/repo".into(),
                    native_id: "opaque-native-id".into(),
                },
            },
        };
        assert!(validate_query(&query).is_ok());
        query.host = "different.example.test".into();
        assert!(validate_query(&query).is_err());
        query.host = "--hostname evil".into();
        assert!(validate_query(&query).is_err());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn discovers_and_invokes_an_independent_non_github_package() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("medousa_review_example");
        std::fs::write(&binary, r#"#!/bin/sh
if [ "$1" = "--describe" ]; then
  printf '%s' '{"protocol_version":1,"id":"example","label":"Example","hosts":["git.example.test"],"operations":["connection_status"]}'
else
  cat >/dev/null
  printf '%s' '{"protocol_version":1,"outcome":{"status":"success","result":{"kind":"connection","authenticated":true,"account":"example-user","message":"Connected"}}}'
fi
"#).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let execution = ForgeExecutionService::new();
        let found = binaries(&execution, vec![temp.path().into()])
            .await
            .unwrap();
        assert_eq!(found, vec![binary.clone()]);
        let output = package_call(
            &execution,
            binary.clone(),
            "--describe",
            vec![],
            Duration::from_secs(2),
            4096,
        )
        .await
        .unwrap();
        let descriptor: ReviewProviderDescriptor = serde_json::from_slice(&output).unwrap();
        assert_eq!(descriptor.id, "example");
        let output = package_call(
            &execution,
            binary.clone(),
            "--invoke",
            b"{}".to_vec(),
            Duration::from_secs(2),
            4096,
        )
        .await
        .unwrap();
        let response: ReviewProviderResponse = serde_json::from_slice(&output).unwrap();
        assert!(matches!(
            response.outcome,
            ReviewProviderOutcome::Success { .. }
        ));
        // Two normal review reads must fit concurrently. An 8 MiB capture
        // ceiling is not an 8 MiB queued payload for each tiny request.
        let (first, second) = tokio::join!(
            package_call(
                &execution,
                binary.clone(),
                "--invoke",
                b"{}".to_vec(),
                Duration::from_secs(2),
                MAX_OUTPUT
            ),
            package_call(
                &execution,
                binary,
                "--invoke",
                b"{}".to_vec(),
                Duration::from_secs(2),
                MAX_OUTPUT
            ),
        );
        assert!(first.is_ok());
        assert!(second.is_ok());
    }
}
