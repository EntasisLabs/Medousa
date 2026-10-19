//! GitHub-specific parsing, transport, and projections. This crate is an optional
//! executable package; the daemon has no dependency on it.
pub mod auth;
mod manage;
mod publish;
use std::time::Duration;

use medousa_forge::execution::{
    ExecutionClass, ForgeExecutionService, supervise_command_with_input,
};
use medousa_types::review_provider::*;
use serde_json::Value;
use url::Url;

pub fn descriptor() -> ReviewProviderDescriptor {
    ReviewProviderDescriptor {
        protocol_version: REVIEW_PROVIDER_PROTOCOL_VERSION,
        id: "github".into(),
        label: "GitHub".into(),
        hosts: vec![
            std::env::var("MEDOUSA_GITHUB_HOST")
                .ok()
                .or_else(|| option_env!("MEDOUSA_GITHUB_HOST").map(str::to_owned))
                .unwrap_or_else(|| "github.com".into()),
        ],
        operations: [
            "connection_status",
            "resolve_repository",
            "publish",
            "device_auth",
            "credential_refresh",
            "list",
            "inbox",
            "check_details",
            "comment",
            "submit_review",
            "edit",
            "set_state",
            "request_reviewers",
            "merge",
            "rerun_check",
            "cancel_check",
            "resolve",
            "get",
            "files",
            "diff",
            "discussion",
            "checks",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    }
}

fn error(code: ReviewProviderErrorCode, message: impl Into<String>) -> ReviewProviderError {
    ReviewProviderError {
        code,
        message: message.into(),
    }
}
fn invalid(message: &str) -> ReviewProviderError {
    error(ReviewProviderErrorCode::InvalidParameter, message)
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}
fn number(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or_default()
}

pub fn validate_repository(repository: &str) -> Result<(), ReviewProviderError> {
    let parts: Vec<_> = repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || !part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
    {
        return Err(invalid("Use a GitHub repository in owner/name format."));
    }
    Ok(())
}

pub fn resolve_url(host: &str, value: &str) -> Result<HostedReviewRef, ReviewProviderError> {
    let url = Url::parse(value).map_err(|_| invalid("Enter a complete pull request URL."))?;
    if url.scheme() != "https"
        || url.host_str() != Some(host)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(invalid(
            "The pull request URL must use the selected GitHub host over HTTPS.",
        ));
    }
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    if parts.len() < 4
        || parts[2] != "pull"
        || parts[3].parse::<u64>().ok().filter(|v| *v > 0).is_none()
    {
        return Err(invalid("Enter a GitHub pull request URL."));
    }
    let repository = format!("{}/{}", parts[0], parts[1]);
    validate_repository(&repository)?;
    Ok(HostedReviewRef {
        provider_id: "github".into(),
        host: host.into(),
        repository,
        native_id: parts[3].into(),
    })
}

pub fn project_review(host: &str, repository: &str, raw: &Value) -> HostedReview {
    let merged = raw["merged_at"].is_string() || raw["merged"].as_bool() == Some(true);
    let state = if merged {
        "merged"
    } else if raw["state"] == "closed" {
        "closed"
    } else if raw["draft"] == true {
        "draft"
    } else {
        "open"
    };
    HostedReview {
        reference: HostedReviewRef {
            provider_id: "github".into(),
            host: host.into(),
            repository: repository.into(),
            native_id: number(raw, "number").to_string(),
        },
        url: text(raw, "html_url"),
        title: text(raw, "title"),
        body: text(raw, "body"),
        author: text(&raw["user"], "login"),
        state: state.into(),
        native_state: text(raw, "state"),
        base_branch: text(&raw["base"], "ref"),
        head_branch: text(&raw["head"], "ref"),
        base_revision: text(&raw["base"], "sha"),
        head_revision: text(&raw["head"], "sha"),
        updated_at: text(raw, "updated_at"),
        additions: number(raw, "additions"),
        deletions: number(raw, "deletions"),
        changed_files: number(raw, "changed_files"),
        actions: vec![],
        requested_reviewers: raw["requested_reviewers"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| text(v, "login"))
            .chain(
                raw["requested_teams"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| format!("team:{}", text(v, "slug"))),
            )
            .collect(),
        merge_state: raw["mergeable_state"].as_str().map(str::to_string),
        merge_methods: vec![],
    }
}

fn github_cli() -> std::path::PathBuf {
    // Finder/LaunchServices often supplies only system paths. Prefer an explicit
    // PATH install (including package/test shims), then common macOS locations.
    if let Some(path) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(if cfg!(windows) { "gh.exe" } else { "gh" }))
            .find(|path| path.is_file())
    }) {
        return path;
    }
    #[cfg(target_os = "macos")]
    for candidate in [
        "/opt/homebrew/bin/gh",
        "/usr/local/bin/gh",
        "/opt/local/bin/gh",
    ] {
        let path = std::path::PathBuf::from(candidate);
        if path.is_file() {
            return path;
        }
    }
    "gh".into()
}

struct GitHub<'a> {
    program: std::path::PathBuf,
    host: &'a str,
    credential: Option<&'a str>,
}
impl GitHub<'_> {
    async fn api(&self, endpoint: &str) -> Result<Value, ReviewProviderError> {
        self.request("GET", endpoint, None).await
    }
    async fn request(
        &self,
        method: &str,
        endpoint: &str,
        body: Option<Value>,
    ) -> Result<Value, ReviewProviderError> {
        if let Some(token) = self.credential {
            return api_http(self.host, token, method, endpoint, body).await;
        }
        // Never inherit a repository from the package's cwd, start
        // an interactive login, or place a token in argv.
        let mut env = vec![
            ("GH_PROMPT_DISABLED".into(), "1".into()),
            ("GH_PAGER".into(), "cat".into()),
        ];
        if let Some(token) = self.credential {
            let variable = if self.host == "github.com" || self.host.ends_with(".ghe.com") {
                "GH_TOKEN"
            } else {
                "GH_ENTERPRISE_TOKEN"
            };
            env.push((variable.into(), token.into()));
        }
        let output = supervise_command_with_input(&self.program, None,
            vec!["api".into(), "--hostname".into(), self.host.into(), "--method".into(), method.into(), endpoint.into()].into_iter().chain(body.as_ref().map(|_| "--input".to_string())).chain(body.as_ref().map(|_| "-".to_string())).collect(),
            env, body.map(|value| value.to_string().into_bytes()).unwrap_or_default(), Duration::from_secs(40), 7 * 1024 * 1024).await
            .map_err(|_| error(ReviewProviderErrorCode::Unavailable, "Connect your GitHub account in Settings → Accounts. The optional GitHub CLI sign-in is unavailable on this workshop."))?;
        let (stdout, stderr, truncated, status) = output;
        if truncated {
            return Err(error(
                ReviewProviderErrorCode::ProviderError,
                "GitHub response exceeded the package's output limit.",
            ));
        }
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr);
            let (code, message) = if stderr.contains("401")
                || stderr.contains("auth login")
                || stderr.contains("GH_TOKEN")
            {
                (
                    ReviewProviderErrorCode::NotAuthenticated,
                    "Connect your GitHub account in Settings → Accounts.",
                )
            } else if stderr.contains("404") {
                (
                    ReviewProviderErrorCode::NotFound,
                    "GitHub could not find this resource with the selected account.",
                )
            } else if stderr.contains("429") || stderr.contains("rate limit") {
                (
                    ReviewProviderErrorCode::RateLimited,
                    "GitHub's rate limit was reached. Try again later.",
                )
            } else {
                (
                    ReviewProviderErrorCode::ProviderError,
                    "GitHub rejected the request. Check the account's repository access.",
                )
            };
            return Err(error(code, message));
        }
        if stdout.iter().all(u8::is_ascii_whitespace) {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&stdout).map_err(|_| {
            error(
                ReviewProviderErrorCode::ProviderError,
                "GitHub returned an invalid response.",
            )
        })
    }
    async fn review(
        &self,
        reference: &HostedReviewRef,
    ) -> Result<HostedReview, ReviewProviderError> {
        let raw = self
            .api(&format!(
                "repos/{}/pulls/{}",
                reference.repository, reference.native_id
            ))
            .await?;
        manage::project_details(self, &reference.repository, &raw).await
    }
    async fn require_revision(
        &self,
        reference: &HostedReviewRef,
        revision: &str,
    ) -> Result<(), ReviewProviderError> {
        let raw = self
            .api(&format!(
                "repos/{}/pulls/{}",
                reference.repository, reference.native_id
            ))
            .await?;
        if text(&raw["head"], "sha") != revision {
            return Err(error(
                ReviewProviderErrorCode::StaleRevision,
                "This review has a newer revision. Refresh it before loading more changes.",
            ));
        }
        Ok(())
    }
    async fn files_page(
        &self,
        review: &HostedReviewRef,
        page: u32,
    ) -> Result<Vec<Value>, ReviewProviderError> {
        let raw = self
            .api(&format!(
                "repos/{}/pulls/{}/files?per_page=100&page={page}",
                review.repository, review.native_id
            ))
            .await?;
        array(raw)
    }
}
fn array(value: Value) -> Result<Vec<Value>, ReviewProviderError> {
    value.as_array().cloned().ok_or_else(|| {
        error(
            ReviewProviderErrorCode::ProviderError,
            "GitHub returned an invalid collection.",
        )
    })
}
fn page(cursor: &Option<String>) -> Result<u32, ReviewProviderError> {
    match cursor {
        None => Ok(1),
        Some(value) => value
            .parse::<u32>()
            .ok()
            .filter(|p| *p > 0 && *p <= 10000)
            .ok_or_else(|| invalid("Invalid page cursor.")),
    }
}
fn next_page(page: u32, length: usize) -> Option<String> {
    (length == 100).then(|| (page + 1).to_string())
}
fn validate_ref(host: &str, reference: &HostedReviewRef) -> Result<(), ReviewProviderError> {
    validate_repository(&reference.repository)?;
    if reference.provider_id != "github"
        || reference.host != host
        || reference
            .native_id
            .parse::<u64>()
            .ok()
            .filter(|n| *n > 0)
            .is_none()
    {
        return Err(invalid(
            "The review does not belong to this GitHub connection.",
        ));
    }
    Ok(())
}

pub async fn invoke(
    input: ReviewProviderInvocation,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    if input.protocol_version != REVIEW_PROVIDER_PROTOCOL_VERSION
        || input.query.provider_id != "github"
    {
        return Err(error(
            ReviewProviderErrorCode::Unsupported,
            "Unsupported provider protocol.",
        ));
    }
    let host = input.query.host.as_str();
    validate_host(host)?;
    if host.is_empty()
        || host.len() > 253
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
        || host.starts_with('-')
    {
        return Err(invalid("Use a GitHub hostname without a scheme or path."));
    }
    if let Some(reference) = input.query.request.review_ref() {
        validate_ref(host, reference)?;
    }
    let credential = credential_token(input.credential.as_deref())?;
    let program = locate_cli().await?;
    let github = GitHub {
        program,
        host,
        credential: credential.as_deref(),
    };
    match input.query.request {
        ReviewProviderOperation::Inbox { folder, cursor } => {
            manage::inbox(&github, folder, cursor).await
        }
        ReviewProviderOperation::CheckDetails {
            review,
            revision,
            check_id,
        } => manage::check_details(&github, &review, &revision, &check_id).await,
        ReviewProviderOperation::ConnectionStatus => match github.api("user").await {
            Ok(user) => Ok(ReviewProviderResult::Connection {
                authenticated: true,
                account: Some(text(&user, "login")),
                message: "Connected to GitHub.".into(),
            }),
            Err(cause)
                if cause.code == ReviewProviderErrorCode::NotAuthenticated
                    || cause.code == ReviewProviderErrorCode::Unavailable =>
            {
                Ok(ReviewProviderResult::Connection {
                    authenticated: false,
                    account: None,
                    message: cause.message,
                })
            }
            Err(cause) => Err(cause),
        },
        ReviewProviderOperation::ResolveRepository { remote_url } => {
            let repository = publish::repository(host, &remote_url)?;
            Ok(ReviewProviderResult::Repository {
                provider_id: "github".into(),
                host: host.into(),
                repository,
            })
        }
        ReviewProviderOperation::Resolve { url } => {
            let reference = resolve_url(host, &url)?;
            Ok(ReviewProviderResult::Review {
                review: Box::new(github.review(&reference).await?),
            })
        }
        ReviewProviderOperation::Get { review } => Ok(ReviewProviderResult::Review {
            review: Box::new(github.review(&review).await?),
        }),
        ReviewProviderOperation::List { repository, cursor } => {
            validate_repository(&repository)?;
            let page = page(&cursor)?;
            let rows = array(github.api(&format!("repos/{repository}/pulls?state=all&sort=updated&direction=desc&per_page=100&page={page}")).await?)?;
            let next_cursor = next_page(page, rows.len());
            let reviews = rows
                .iter()
                .map(|row| project_review(host, &repository, row))
                .collect();
            Ok(ReviewProviderResult::Reviews {
                reviews,
                next_cursor,
            })
        }
        ReviewProviderOperation::Files {
            review,
            revision,
            cursor,
        } => {
            let page = page(&cursor)?;
            github.require_revision(&review, &revision).await?;
            let rows = github.files_page(&review, page).await?;
            github.require_revision(&review, &revision).await?;
            let next_cursor = if page < 30 {
                next_page(page, rows.len())
            } else {
                None
            };
            let files = rows
                .iter()
                .map(|row| HostedReviewFile {
                    path: text(row, "filename"),
                    previous_path: row["previous_filename"].as_str().map(str::to_string),
                    status: text(row, "status"),
                    additions: number(row, "additions"),
                    deletions: number(row, "deletions"),
                })
                .collect();
            Ok(ReviewProviderResult::Files { files, next_cursor })
        }
        ReviewProviderOperation::Diff {
            review,
            revision,
            path,
        } => {
            github.require_revision(&review, &revision).await?;
            // GitHub caps this endpoint at 3,000 files. Report an unavailable
            // patch honestly; never manufacture a diff or fetch a working copy.
            for page in 1..=30 {
                let rows = github.files_page(&review, page).await?;
                if let Some(row) = rows.iter().find(|row| row["filename"] == path) {
                    let patch = row["patch"].as_str().map(str::to_string);
                    github.require_revision(&review, &revision).await?;
                    let message = patch.is_none().then(|| "GitHub did not provide a text patch for this file. Open the review on GitHub to inspect it.".into());
                    return Ok(ReviewProviderResult::Diff {
                        path,
                        patch,
                        message,
                    });
                }
                if rows.len() < 100 {
                    break;
                }
            }
            Err(error(
                ReviewProviderErrorCode::NotFound,
                "This file was not included in GitHub's available diff.",
            ))
        }
        ReviewProviderOperation::Checks {
            review,
            revision,
            cursor,
        } => {
            if revision.is_empty() || !revision.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err(invalid("Invalid review revision."));
            }
            let (collection, current_page) = match cursor.as_deref() {
                None => ("runs", 1),
                Some(value) => {
                    let (kind, n) = value
                        .split_once(':')
                        .ok_or_else(|| invalid("Invalid checks cursor."))?;
                    (kind, page(&Some(n.into()))?)
                }
            };
            let endpoint = match collection {
                "runs" => format!("repos/{}/commits/{revision}/check-runs", review.repository),
                "statuses" => format!("repos/{}/commits/{revision}/status", review.repository),
                "workflows" => format!(
                    "repos/{}/actions/runs?head_sha={revision}",
                    review.repository
                ),
                _ => return Err(invalid("Invalid checks cursor.")),
            };
            let raw = github
                .api(&format!(
                    "{endpoint}{}per_page=100&page={current_page}",
                    if endpoint.contains('?') { "&" } else { "?" }
                ))
                .await?;
            let rows = array(if collection == "runs" {
                raw["check_runs"].clone()
            } else if collection == "workflows" {
                raw["workflow_runs"].clone()
            } else {
                raw["statuses"].clone()
            })?;
            let next_cursor = if rows.len() == 100 {
                Some(format!("{collection}:{}", current_page + 1))
            } else if collection == "runs" {
                Some("statuses:1".into())
            } else if collection == "statuses" {
                Some("workflows:1".into())
            } else {
                None
            };
            let checks = rows
                .iter()
                .map(|row| HostedReviewCheck {
                    id: format!("{collection}:{}", number(row, "id")),
                    name: text(
                        row,
                        if collection != "statuses" {
                            "name"
                        } else {
                            "context"
                        },
                    ),
                    status: text(
                        row,
                        if collection != "statuses" {
                            "status"
                        } else {
                            "state"
                        },
                    ),
                    conclusion: row["conclusion"].as_str().map(str::to_string),
                    url: row[if collection != "statuses" {
                        "html_url"
                    } else {
                        "target_url"
                    }]
                    .as_str()
                    .map(str::to_string),
                })
                .collect();
            Ok(ReviewProviderResult::Checks {
                checks,
                next_cursor,
            })
        }
        ReviewProviderOperation::Discussion { review, cursor } => {
            let (collection, current_page) = match cursor.as_deref() {
                None => ("comments", 1),
                Some(value) => {
                    let (kind, n) = value
                        .split_once(':')
                        .ok_or_else(|| invalid("Invalid discussion cursor."))?;
                    (kind, page(&Some(n.into()))?)
                }
            };
            let endpoint = match collection {
                "comments" => format!(
                    "repos/{}/issues/{}/comments",
                    review.repository, review.native_id
                ),
                "reviews" => format!(
                    "repos/{}/pulls/{}/reviews",
                    review.repository, review.native_id
                ),
                "inline" => format!(
                    "repos/{}/pulls/{}/comments",
                    review.repository, review.native_id
                ),
                _ => return Err(invalid("Invalid discussion cursor.")),
            };
            let rows = array(
                github
                    .api(&format!("{endpoint}?per_page=100&page={current_page}"))
                    .await?,
            )?;
            let next_cursor = if rows.len() == 100 {
                Some(format!("{collection}:{}", current_page + 1))
            } else {
                match collection {
                    "comments" => Some("reviews:1".into()),
                    "reviews" => Some("inline:1".into()),
                    _ => None,
                }
            };
            let entries = rows
                .iter()
                .map(|row| HostedReviewDiscussionEntry {
                    id: format!("{collection}:{}", number(row, "id")),
                    kind: match collection {
                        "reviews" => format!("review_{}", text(row, "state").to_lowercase()),
                        "inline" => "inline_comment".into(),
                        _ => "comment".into(),
                    },
                    author: text(&row["user"], "login"),
                    body: text(row, "body"),
                    url: text(row, "html_url"),
                    created_at: row["created_at"]
                        .as_str()
                        .or(row["submitted_at"].as_str())
                        .unwrap_or_default()
                        .into(),
                    path: row["path"].as_str().map(str::to_string),
                    line: row["line"].as_u64(),
                    side: row["side"].as_str().map(str::to_string),
                    revision: row["commit_id"].as_str().map(str::to_string),
                    reply_to: row["in_reply_to_id"]
                        .as_u64()
                        .map(|id| format!("inline:{id}")),
                })
                .collect();
            Ok(ReviewProviderResult::Discussion {
                entries,
                next_cursor,
            })
        }
    }
}

fn validate_host(host: &str) -> Result<(), ReviewProviderError> {
    if host.is_empty()
        || host.len() > 253
        || host.starts_with('-')
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
    {
        return Err(invalid("Use a GitHub hostname without a scheme or path."));
    }
    Ok(())
}
fn http_client() -> Result<reqwest::Client, ReviewProviderError> {
    reqwest::Client::builder()
        .user_agent("Medousa-GitHub/1")
        .timeout(Duration::from_secs(40))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| {
            error(
                ReviewProviderErrorCode::Unavailable,
                "Could not start GitHub transport.",
            )
        })
}
async fn decode_http(response: reqwest::Response) -> Result<Value, ReviewProviderError> {
    decode_http_limited(response, 7 * 1024 * 1024).await
}
async fn decode_http_limited(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Value, ReviewProviderError> {
    let status = response.status();
    if !status.is_success() {
        let (code, message) = match status.as_u16() {
            401 => (
                ReviewProviderErrorCode::NotAuthenticated,
                "Connect your GitHub account again.",
            ),
            403 => (
                ReviewProviderErrorCode::ProviderError,
                "GitHub denied this action. Check repository permissions, organization authorization, and branch requirements.",
            ),
            409 | 405 => (
                ReviewProviderErrorCode::StaleRevision,
                "GitHub could not apply this action to the current PR state. Refresh and check branch requirements.",
            ),
            422 => (
                ReviewProviderErrorCode::InvalidParameter,
                "GitHub could not apply this request. Check reviewer names, comment locations, permissions, and the current PR state.",
            ),
            404 => (
                ReviewProviderErrorCode::NotFound,
                "GitHub could not find this resource with the selected account.",
            ),
            429 => (
                ReviewProviderErrorCode::RateLimited,
                "GitHub's rate limit was reached. Try later.",
            ),
            _ => (
                ReviewProviderErrorCode::ProviderError,
                "GitHub rejected the request. Check the account's repository access.",
            ),
        };
        return Err(error(code, message));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        error(
            ReviewProviderErrorCode::Unavailable,
            "GitHub response was interrupted.",
        )
    })? {
        if bytes.len() + chunk.len() > limit {
            return Err(error(
                ReviewProviderErrorCode::ProviderError,
                "GitHub response exceeded the package's output limit.",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        error(
            ReviewProviderErrorCode::ProviderError,
            "GitHub returned an invalid response.",
        )
    })
}
async fn api_http(
    host: &str,
    token: &str,
    method: &str,
    endpoint: &str,
    body: Option<Value>,
) -> Result<Value, ReviewProviderError> {
    let base = if host == "github.com" {
        "https://api.github.com".to_owned()
    } else {
        format!("https://{host}/api/v3")
    };
    if endpoint == "graphql" && host != "github.com" {
        return api_http_base(
            &format!("https://{host}/api"),
            token,
            method,
            endpoint,
            body,
        )
        .await;
    }
    api_http_base(&base, token, method, endpoint, body).await
}
async fn api_http_base(
    base: &str,
    token: &str,
    method: &str,
    endpoint: &str,
    body: Option<Value>,
) -> Result<Value, ReviewProviderError> {
    let mut request = http_client()?
        .request(
            reqwest::Method::from_bytes(method.as_bytes())
                .map_err(|_| invalid("Invalid request method."))?,
            format!("{base}/{endpoint}"),
        )
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|_| error(ReviewProviderErrorCode::Unavailable, "GitHub did not confirm the request. Refresh to check whether it succeeded before submitting again."))?;
    decode_http(response).await
}

fn credential_token(value: Option<&str>) -> Result<Option<String>, ReviewProviderError> {
    match value {
        Some(value) if value.trim_start().starts_with('{') => {
            let bundle: Value = serde_json::from_str(value)
                .map_err(|_| invalid("Invalid saved GitHub OAuth credential."))?;
            Ok(Some(
                bundle["access_token"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| invalid("Saved GitHub OAuth credential has no access token."))?
                    .to_owned(),
            ))
        }
        value => Ok(value.map(str::to_owned)),
    }
}
async fn locate_cli() -> Result<std::path::PathBuf, ReviewProviderError> {
    ForgeExecutionService::new()
        .run(
            ExecutionClass::RepositoryMetadata,
            1024,
            || Ok(github_cli()),
        )
        .await
        .map_err(|_| {
            error(
                ReviewProviderErrorCode::Unavailable,
                "Could not locate GitHub CLI on this workshop.",
            )
        })
}
pub use publish::invoke as invoke_write;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn url_identity_is_host_and_repository_specific() {
        let review = resolve_url(
            "git.company.test",
            "https://git.company.test/team/repo/pull/42/files?x=1#diff",
        )
        .unwrap();
        assert_eq!(review.host, "git.company.test");
        assert_eq!(review.repository, "team/repo");
        assert_eq!(review.native_id, "42");
        for url in [
            "https://github.com/team/repo/pull/42",
            "https://git.company.test/team/repo/issues/42",
            "https://user@git.company.test/team/repo/pull/42",
            "http://git.company.test/team/repo/pull/42",
        ] {
            assert!(resolve_url("git.company.test", url).is_err(), "{url}");
        }
    }
    #[test]
    fn projects_actual_state_and_revision_without_forge_identity() {
        let raw = json!({"number": 42, "state": "closed", "merged_at": "2026-10-18", "head": {"sha": "abc", "ref": "feature"}, "base": {"sha": "def", "ref": "main"}});
        let review = project_review("github.com", "team/repo", &raw);
        assert_eq!(review.state, "merged");
        assert_eq!(review.head_revision, "abc");
        assert!(review.actions.is_empty());
    }
    #[test]
    fn rejects_path_and_cursor_injection() {
        for repository in [
            "../repo",
            "owner/repo?state=all",
            "owner/repo/extra",
            "-R owner/repo",
        ] {
            assert!(validate_repository(repository).is_err());
        }
        assert!(page(&Some("0".into())).is_err());
        assert!(page(&Some("1&per_page=999".into())).is_err());
    }
}
