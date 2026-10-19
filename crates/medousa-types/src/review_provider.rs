//! Versioned repository-host review contract. Provider-native IDs remain opaque.
//! Packages implement the stdio request/response protocol; Home and agents use
//! the same DTOs through the daemon. No Forge work item or checkout is required.
use serde::{Deserialize, Serialize};

pub const REVIEW_PROVIDER_PROTOCOL_VERSION: u32 = 1;
pub const REVIEW_PROVIDER_BINARY_PREFIX: &str = "medousa_review_";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ReviewProviderDescriptor {
    pub protocol_version: u32,
    pub id: String,
    pub label: String,
    /// Suggested hosts. An explicit connection may use a self-hosted instance.
    pub hosts: Vec<String>,
    pub operations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ReviewProviderListResponse {
    pub providers: Vec<ReviewProviderDescriptor>,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct HostedReviewRef {
    pub provider_id: String,
    pub host: String,
    pub repository: String,
    pub native_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct HostedReview {
    pub reference: HostedReviewRef,
    pub url: String,
    pub title: String,
    pub body: String,
    pub author: String,
    /// open, draft, merged, or closed. The native label is preserved separately.
    pub state: String,
    pub native_state: String,
    pub base_branch: String,
    pub head_branch: String,
    pub base_revision: String,
    pub head_revision: String,
    pub updated_at: String,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    /// Actions currently available on this review (empty for read-only packages).
    pub actions: Vec<String>,
    #[serde(default)]
    pub requested_reviewers: Vec<String>,
    #[serde(default)]
    pub merge_state: Option<String>,
    #[serde(default)]
    pub merge_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct HostedReviewFile {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct HostedReviewDiscussionEntry {
    pub id: String,
    pub kind: String,
    pub author: String,
    pub body: String,
    pub url: String,
    pub created_at: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub side: Option<String>,
    pub revision: Option<String>,
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct HostedReviewCheck {
    pub id: String,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewProviderOperation {
    Inbox {
        folder: ReviewInboxFolder,
        cursor: Option<String>,
    },
    CheckDetails {
        review: HostedReviewRef,
        revision: String,
        check_id: String,
    },
    ConnectionStatus,
    ResolveRepository {
        remote_url: String,
    },
    List {
        repository: String,
        cursor: Option<String>,
    },
    Resolve {
        url: String,
    },
    Get {
        review: HostedReviewRef,
    },
    Files {
        review: HostedReviewRef,
        revision: String,
        cursor: Option<String>,
    },
    Diff {
        review: HostedReviewRef,
        revision: String,
        path: String,
    },
    Checks {
        review: HostedReviewRef,
        revision: String,
        cursor: Option<String>,
    },
    Discussion {
        review: HostedReviewRef,
        cursor: Option<String>,
    },
}

impl ReviewProviderOperation {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Inbox { .. } => "inbox",
            Self::CheckDetails { .. } => "check_details",
            Self::ConnectionStatus => "connection_status",
            Self::ResolveRepository { .. } => "resolve_repository",
            Self::List { .. } => "list",
            Self::Resolve { .. } => "resolve",
            Self::Get { .. } => "get",
            Self::Files { .. } => "files",
            Self::Diff { .. } => "diff",
            Self::Checks { .. } => "checks",
            Self::Discussion { .. } => "discussion",
        }
    }

    pub fn review_ref(&self) -> Option<&HostedReviewRef> {
        match self {
            Self::CheckDetails { review, .. }
            | Self::Get { review }
            | Self::Files { review, .. }
            | Self::Diff { review, .. }
            | Self::Checks { review, .. }
            | Self::Discussion { review, .. } => Some(review),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ReviewProviderQuery {
    pub provider_id: String,
    pub host: String,
    /// Explicit account identity; omitted to use the package's existing sign-in.
    pub connection_id: Option<String>,
    pub request: ReviewProviderOperation,
}

/// Internal IPC only. Secrets are delivered through stdin, never argv or HTTP
/// responses. Deliberately does not implement Debug or JsonSchema.
#[derive(Serialize, Deserialize)]
pub struct ReviewProviderInvocation {
    pub protocol_version: u32,
    pub query: ReviewProviderQuery,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewProviderResult {
    Updated {
        message: String,
    },
    CheckDetails {
        title: String,
        summary: String,
        text: String,
        annotations: Vec<String>,
        url: Option<String>,
    },
    Repository {
        provider_id: String,
        host: String,
        repository: String,
    },
    Published {
        review: Box<HostedReview>,
        existing: bool,
    },
    Connection {
        authenticated: bool,
        account: Option<String>,
        message: String,
    },
    Reviews {
        reviews: Vec<HostedReview>,
        next_cursor: Option<String>,
    },
    Review {
        review: Box<HostedReview>,
    },
    Files {
        files: Vec<HostedReviewFile>,
        next_cursor: Option<String>,
    },
    Diff {
        path: String,
        patch: Option<String>,
        message: Option<String>,
    },
    Checks {
        checks: Vec<HostedReviewCheck>,
        next_cursor: Option<String>,
    },
    Discussion {
        entries: Vec<HostedReviewDiscussionEntry>,
        next_cursor: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewProviderErrorCode {
    Unavailable,
    NotAuthenticated,
    NotFound,
    InvalidParameter,
    Unsupported,
    RateLimited,
    StaleRevision,
    ProviderError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ReviewProviderError {
    pub code: ReviewProviderErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewProviderOutcome {
    Success { result: ReviewProviderResult },
    Error { error: ReviewProviderError },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ReviewProviderResponse {
    pub protocol_version: u32,
    pub outcome: ReviewProviderOutcome,
}

impl ReviewProviderResponse {
    pub fn from_result(result: Result<ReviewProviderResult, ReviewProviderError>) -> Self {
        Self {
            protocol_version: REVIEW_PROVIDER_PROTOCOL_VERSION,
            outcome: match result {
                Ok(result) => ReviewProviderOutcome::Success { result },
                Err(error) => ReviewProviderOutcome::Error { error },
            },
        }
    }
}

/// Account-relative inboxes are interpreted by the installed provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewInboxFolder {
    Authored,
    ReviewRequested,
    Participating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewDecision {
    Approve,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewLineSide {
    Old,
    New,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewMergeMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewLifecycle {
    Close,
    Reopen,
    Ready,
    Draft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ReviewInlineComment {
    pub path: String,
    pub side: ReviewLineSide,
    pub line: u64,
    pub body: String,
}

/// Writes have a separate HTTP operation and capability from review reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ReviewProviderMutation {
    pub provider_id: String,
    pub host: String,
    pub connection_id: Option<String>,
    pub request: ReviewProviderWrite,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewProviderWrite {
    Comment {
        review: HostedReviewRef,
        body: String,
        reply_to: Option<String>,
    },
    SubmitReview {
        review: HostedReviewRef,
        revision: String,
        decision: ReviewDecision,
        body: String,
        comments: Vec<ReviewInlineComment>,
    },
    Edit {
        review: HostedReviewRef,
        title: String,
        body: String,
    },
    SetState {
        review: HostedReviewRef,
        state: ReviewLifecycle,
    },
    RequestReviewers {
        review: HostedReviewRef,
        reviewers: Vec<String>,
    },
    Merge {
        review: HostedReviewRef,
        revision: String,
        method: ReviewMergeMethod,
    },
    RerunCheck {
        review: HostedReviewRef,
        revision: String,
        check_id: String,
        failed_only: bool,
    },
    CancelCheck {
        review: HostedReviewRef,
        revision: String,
        check_id: String,
    },
    /// Branch publication stays with the workshop's Git service. This operation
    /// looks up an existing open review before creating one, making retries safe.
    Publish {
        repository: String,
        head: String,
        base: String,
        title: String,
        body: String,
        draft: bool,
        /// Forge sharing updates its description; chat Git reuses an existing PR.
        update_existing: bool,
    },
}
impl ReviewProviderWrite {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Publish { .. } => "publish",
            Self::Comment { .. } => "comment",
            Self::SubmitReview { .. } => "submit_review",
            Self::Edit { .. } => "edit",
            Self::SetState { .. } => "set_state",
            Self::RequestReviewers { .. } => "request_reviewers",
            Self::Merge { .. } => "merge",
            Self::RerunCheck { .. } => "rerun_check",
            Self::CancelCheck { .. } => "cancel_check",
        }
    }
    pub fn review_ref(&self) -> Option<&HostedReviewRef> {
        match self {
            Self::Publish { .. } => None,
            Self::Comment { review, .. }
            | Self::SubmitReview { review, .. }
            | Self::Edit { review, .. }
            | Self::SetState { review, .. }
            | Self::RequestReviewers { review, .. }
            | Self::Merge { review, .. }
            | Self::RerunCheck { review, .. }
            | Self::CancelCheck { review, .. } => Some(review),
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct ReviewProviderWriteInvocation {
    pub protocol_version: u32,
    pub mutation: ReviewProviderMutation,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewProviderAuthRequest {
    Begin {
        provider_id: String,
        host: String,
        client_id: Option<String>,
    },
    Poll {
        flow_id: String,
    },
    Cancel {
        flow_id: String,
    },
}
/// No device secrets or OAuth tokens cross the public HTTP boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ReviewProviderAuthResponse {
    Pending {
        flow_id: String,
        user_code: String,
        verification_uri: String,
        expires_in: u64,
        interval: u64,
    },
    Connected {
        connection_id: String,
        account: String,
    },
    Cancelled,
}
/// Private package IPC. Never expose these types in the HTTP schema or logs.
#[derive(Serialize, Deserialize)]
pub struct ReviewProviderAuthInvocation {
    pub protocol_version: u32,
    pub host: String,
    pub request: ReviewProviderAuthOperation,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ReviewProviderAuthOperation {
    Begin { client_id: Option<String> },
    Poll { state: String },
    Refresh { credential: String },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReviewProviderAuthResult {
    Pending {
        state: String,
        user_code: String,
        verification_uri: String,
        expires_in: u64,
        interval: u64,
    },
    Authorized {
        credential: String,
        account: String,
    },
}
#[derive(Serialize, Deserialize)]
pub struct ReviewProviderAuthPackageResponse {
    pub protocol_version: u32,
    pub result: Result<ReviewProviderAuthResult, ReviewProviderError>,
}
