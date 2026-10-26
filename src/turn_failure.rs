//! Structured turn failures — operator-facing copy vs debug detail.
//!
//! Phase 0 of [inference-profiles-and-model-catalog-plan.md](../architecture/inference-profiles-and-model-catalog-plan.md):
//! raw provider / runtime errors must not land in chat transcripts or model context.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnFailureCategory {
    Auth,
    RateLimit,
    ModelNotFound,
    ProviderDown,
    RuntimeBusy,
    Timeout,
    Cancelled,
    Validation,
    Unknown,
}

impl TurnFailureCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::RateLimit => "rate_limit",
            Self::ModelNotFound => "model_not_found",
            Self::ProviderDown => "provider_down",
            Self::RuntimeBusy => "runtime_busy",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Validation => "validation",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnFailure {
    pub category: TurnFailureCategory,
    pub operator_message: String,
    pub debug_message: String,
    pub retryable: bool,
}

impl TurnFailure {
    pub fn from_debug(raw: &str) -> Self {
        let debug_message = raw.trim().to_string();
        if debug_message.is_empty() {
            return Self::unknown("(empty error)");
        }

        let category = classify_category(&debug_message);
        Self {
            operator_message: operator_message_for(category, &debug_message),
            debug_message,
            retryable: category_is_retryable(category),
            category,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            category: TurnFailureCategory::Cancelled,
            operator_message: "Turn cancelled.".to_string(),
            debug_message: "interactive turn cancelled".to_string(),
            retryable: false,
        }
    }

    pub fn validation(
        operator_message: impl Into<String>,
        debug_message: impl Into<String>,
    ) -> Self {
        Self {
            category: TurnFailureCategory::Validation,
            operator_message: operator_message.into(),
            debug_message: debug_message.into(),
            retryable: false,
        }
    }

    fn unknown(debug_message: &str) -> Self {
        Self {
            category: TurnFailureCategory::Unknown,
            operator_message: operator_message_for(TurnFailureCategory::Unknown, debug_message),
            debug_message: debug_message.to_string(),
            retryable: false,
        }
    }

    pub fn category_label(&self) -> &'static str {
        self.category.as_str()
    }
}

pub fn is_error_turn_excluded_from_model_context(turn: &crate::session::ConversationTurn) -> bool {
    turn.answer_state.as_deref() == Some("error")
}

// Only interpret status-shaped text as HTTP codes, never incidental byte
// counts, paths, request IDs, or model names.
fn has_http_status(text: &str, code: &str) -> bool {
    [
        "http ",
        "http error: ",
        "status ",
        "status: ",
        "status code ",
        "status code: ",
        "status_code=",
        "status=",
        "statuscode(",
    ]
    .iter()
    .any(|prefix| {
        text.match_indices(prefix).any(|(offset, matched)| {
            let rest = text[offset + matched.len()..].trim_start();
            rest.strip_prefix(code)
                .is_some_and(|tail| !tail.starts_with(|c: char| c.is_ascii_digit()))
        })
    })
}

/// Only return fixed, safe copy here: runtime errors can contain paths, prompts,
/// credentials, or provider response bodies. Local blockers take precedence over
/// provider heuristics and must not trigger provider fallback or blind retries.
pub(crate) fn runtime_blocker_message(raw: &str) -> Option<&'static str> {
    let text = raw.to_ascii_lowercase();
    if text.contains("cannot begin attempt") && text.contains("invalid transition") {
        if text.contains(" is accepted,") {
            return Some(
                "Coder couldn't start because this work is already accepted. Create a new undertaking for the next changes; retrying this assignment won't reopen it.",
            );
        }
        if text.contains(" is discarded,") {
            return Some(
                "Coder couldn't start because this work was discarded. Create a new undertaking to continue.",
            );
        }
        if text.contains(" is awaiting_review,") {
            return Some(
                "Coder couldn't start because this work is awaiting review. Review it or request changes before assigning more work.",
            );
        }
        return Some(
            "Coder couldn't start because this work isn't executable in its current state. Inspect the undertaking's state before assigning more work.",
        );
    }
    if text.contains("no space left on device") || text.contains("disk quota exceeded") {
        return Some(
            "Medousa couldn't save runtime data because the workshop disk is full. Free disk space, then check the work state before retrying.",
        );
    }
    if text.contains("cannot acquire coder authority")
        || text.contains("coder forge lease rejected")
    {
        return Some(
            "Coder couldn't obtain permission to execute this work. Check the undertaking's state and active executor in the runtime details before retrying.",
        );
    }
    None
}

fn classify_category(raw: &str) -> TurnFailureCategory {
    let text = raw.to_ascii_lowercase();
    if runtime_blocker_message(raw).is_some() {
        return TurnFailureCategory::Validation;
    }

    if text.contains(
        "undertaking workspace is held by an executor outside coder shared-space coordination",
    ) {
        return TurnFailureCategory::Validation;
    }
    if text.contains("cancelled") || text.contains("canceled") {
        return TurnFailureCategory::Cancelled;
    }
    // Check local admission errors before provider status heuristics: byte
    // counts can contain digits such as 401, 404, or 429.
    if text.contains("forge command queue is full")
        || text.contains("forge queued-byte budget is exhausted")
        || text.contains("repository-lane registry is full")
    {
        return TurnFailureCategory::RuntimeBusy;
    }
    if text.contains("estimated retained bytes") && text.contains("class budget") {
        return TurnFailureCategory::Validation;
    }
    if text.contains("session_id")
        && (text.contains("required") || text.contains("must not be empty"))
        || text.contains("prompt are required")
        || text.contains("prompt is required")
    {
        return TurnFailureCategory::Validation;
    }
    if has_http_status(&text, "401")
        || has_http_status(&text, "403")
        || text.contains("invalid api key")
        || text.contains("invalid_api_key")
        || text.contains("incorrect api key")
        || text.contains("authentication")
        || text.contains("unauthorized")
        || (text.contains("chatgpt") && text.contains("sign-in"))
    {
        return TurnFailureCategory::Auth;
    }
    if has_http_status(&text, "429")
        || text.contains("rate limit")
        || text.contains("rate_limit")
        || text.contains("too many requests")
        || text.contains("quota")
    {
        return TurnFailureCategory::RateLimit;
    }
    if has_http_status(&text, "404")
        || text.contains("model not found")
        || (text.contains("model") && text.contains("does not exist"))
        || text.contains("unknown model")
        || text.contains("invalid model")
    {
        return TurnFailureCategory::ModelNotFound;
    }
    if text.contains("timeout") || text.contains("timed out") || text.contains("deadline exceeded")
    {
        return TurnFailureCategory::Timeout;
    }
    if has_http_status(&text, "400") || text.contains("bad request") {
        return TurnFailureCategory::Validation;
    }
    if text.contains("connection")
        || text.contains("transport")
        || text.contains("temporar")
        || text.contains("unavailable")
        || has_http_status(&text, "502")
        || has_http_status(&text, "503")
        || has_http_status(&text, "504")
        || text.contains("5xx")
        || (text.contains("queue") && (text.contains("busy") || text.contains("full")))
    {
        return TurnFailureCategory::ProviderDown;
    }

    TurnFailureCategory::Unknown
}

fn category_is_retryable(category: TurnFailureCategory) -> bool {
    matches!(
        category,
        TurnFailureCategory::RateLimit
            | TurnFailureCategory::ProviderDown
            | TurnFailureCategory::RuntimeBusy
            | TurnFailureCategory::Timeout
    )
}

fn operator_message_for(category: TurnFailureCategory, raw: &str) -> String {
    match category {
        TurnFailureCategory::Auth => {
            if raw.to_ascii_lowercase().contains("chatgpt") {
                return "Reconnect ChatGPT in Settings → Medousa Agent → Models, then submit a new request."
                    .into();
            }
            "The model provider rejected the API key. Check Settings → Models and try again."
                .to_string()
        }
        TurnFailureCategory::RateLimit => {
            "The model provider is rate-limiting requests. Wait a moment and try again.".to_string()
        }
        TurnFailureCategory::ModelNotFound => {
            "That model isn't available right now. Choose a different model in Settings."
                .to_string()
        }
        TurnFailureCategory::ProviderDown => {
            "Couldn't reach the model provider. Try again in a moment.".to_string()
        }
        TurnFailureCategory::RuntimeBusy => {
            "Medousa's work queue is full. Wait for active work to finish, then try again."
                .to_string()
        }
        TurnFailureCategory::Timeout => {
            "The turn timed out. Check whether any work completed before retrying.".to_string()
        }
        TurnFailureCategory::Cancelled => "Turn cancelled.".to_string(),
        TurnFailureCategory::Validation => validation_operator_message(raw),
        TurnFailureCategory::Unknown => {
            "Medousa couldn't complete this turn. Check the error details before retrying."
                .to_string()
        }
    }
}

fn validation_operator_message(raw: &str) -> String {
    // Keep the more specific workspace-custody explanation below.
    let text = raw.to_ascii_lowercase();
    if !text.contains(
        "undertaking workspace is held by an executor outside coder shared-space coordination",
    ) && let Some(message) = runtime_blocker_message(raw)
    {
        return message.to_string();
    }
    if text.contains("estimated retained bytes") && text.contains("class budget") {
        return "This operation exceeds Medousa's per-operation data limit. See the error details for the operation and limit.".to_string();
    }
    if text.contains(
        "undertaking workspace is held by an executor outside coder shared-space coordination",
    ) {
        return "This project's workspace is in use by another executor. Finish or stop that work before starting Coder.".to_string();
    }
    if text.contains("session_id") && text.contains("required") {
        return "Start or select a chat session before sending a message.".to_string();
    }
    if text.contains("prompt") && text.contains("required") {
        return "Type a message or attach a file before sending.".to_string();
    }
    if text.contains("too many attachments") {
        return "Too many attachments on this turn. Remove one and try again.".to_string();
    }
    "That request wasn't valid. Check your message and try again.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forge_saturation_identifies_the_local_queue_and_preserves_details() {
        for raw in [
            "execution overloaded: forge command queue is full",
            "execution overloaded: forge queued-byte budget is exhausted (limit 8388608 bytes, this admission needs 1048576 bytes)",
            "HTTP 503: execution overloaded: repository-lane registry is full",
            "execution overloaded: forge queued-byte budget is exhausted (limit 8388608 bytes, this admission needs 404 bytes)",
        ] {
            let failure = TurnFailure::from_debug(raw);
            assert_eq!(failure.category, TurnFailureCategory::RuntimeBusy);
            assert!(failure.retryable);
            assert!(failure.operator_message.contains("Medousa's work queue"));
            assert_eq!(failure.debug_message, raw);
            assert!(!crate::inference_router::should_advance_fallback(
                failure.category
            ));
        }
    }

    #[test]
    fn forge_operation_limit_is_not_a_transient_provider_failure() {
        let failure = TurnFailure::from_debug(
            "execution overloaded: estimated retained bytes (1048577) exceed StoreIo class budget (1048576)",
        );
        assert_eq!(failure.category, TurnFailureCategory::Validation);
        assert!(!failure.retryable);
        assert!(
            failure
                .operator_message
                .contains("per-operation data limit")
        );
    }

    #[test]
    fn workspace_custody_failure_explains_the_blocker_without_retry_advice() {
        let failure = TurnFailure::from_debug(
            "cannot acquire Coder authority: workspace busy: the undertaking workspace is held by an executor outside Coder shared-space coordination",
        );
        assert_eq!(failure.category, TurnFailureCategory::Validation);
        assert!(!failure.retryable);
        assert!(failure.operator_message.contains("another executor"));
        assert!(!failure.operator_message.contains("Try again"));
    }

    #[test]
    fn chatgpt_reauthentication_is_actionable_and_not_an_api_key_failure() {
        let failure =
            TurnFailure::from_debug("port failure: ChatGPT sign-in must be completed again");
        assert_eq!(failure.category, TurnFailureCategory::Auth);
        assert!(!failure.retryable);
        assert!(failure.operator_message.contains("Reconnect ChatGPT"));
        assert!(!failure.operator_message.contains("API key"));
    }

    #[test]
    fn classifies_rate_limit() {
        let failure = TurnFailure::from_debug("openai HTTP 429: rate limit exceeded");
        assert_eq!(failure.category, TurnFailureCategory::RateLimit);
        assert!(!failure.operator_message.contains("429"));
        assert!(failure.debug_message.contains("429"));
    }

    #[test]
    fn classifies_auth_without_leaking_to_operator() {
        let failure = TurnFailure::from_debug("Invalid API key provided");
        assert_eq!(failure.category, TurnFailureCategory::Auth);
        assert!(
            !failure
                .operator_message
                .to_ascii_lowercase()
                .contains("api key provided")
        );
    }

    #[test]
    fn classifies_model_not_found() {
        let failure = TurnFailure::from_debug("model gpt-5-unicorn does not exist");
        assert_eq!(failure.category, TurnFailureCategory::ModelNotFound);
    }

    #[test]
    fn cancelled_is_not_retryable() {
        let failure = TurnFailure::cancelled();
        assert_eq!(failure.category, TurnFailureCategory::Cancelled);
        assert!(!failure.retryable);
    }

    #[test]
    fn bad_request_is_validation_and_not_retryable() {
        let failure = TurnFailure::from_debug(
            "HTTP 400 Bad Request: {\"detail\":\"Stream must be set to true\"}",
        );
        assert_eq!(failure.category, TurnFailureCategory::Validation);
        assert!(!failure.retryable);
    }

    #[test]
    fn incidental_numbers_and_missing_runtime_resources_are_not_provider_errors() {
        for raw in [
            "capture failed after 140400 bytes",
            "worker 401 exited",
            "snapshot 429000 could not be saved",
            "workspace does not exist",
            "HTTP 4040 is not a valid status",
        ] {
            assert_eq!(
                TurnFailure::from_debug(raw).category,
                TurnFailureCategory::Unknown,
                "{raw}"
            );
        }
        for raw in [
            "HTTP 401: denied",
            "http error: 403 Forbidden",
            "status code: 401",
            "status=403",
            "StatusCode(401)",
        ] {
            assert_eq!(
                TurnFailure::from_debug(raw).category,
                TurnFailureCategory::Auth,
                "{raw}"
            );
        }
    }

    #[test]
    fn runtime_blockers_are_actionable_safe_and_do_not_fallback() {
        for (raw, expected) in [
            (
                "cannot acquire Coder authority: invalid transition: work work-test is accepted, cannot begin attempt",
                "already accepted",
            ),
            (
                "invalid transition: work work-test is awaiting_review, cannot begin attempt",
                "awaiting review",
            ),
            (
                "invalid transition: work work-test is discarded, cannot begin attempt",
                "discarded",
            ),
            (
                "invalid transition: work work-test is failed, cannot begin attempt",
                "current state",
            ),
            (
                "snapshot failed: No space left on device (os error 28)",
                "disk is full",
            ),
            ("disk quota exceeded", "disk is full"),
            (
                "cannot acquire Coder authority: lease expired",
                "permission to execute",
            ),
            (
                "Coder Forge lease rejected: store unavailable",
                "permission to execute",
            ),
        ] {
            let raw = format!("{raw}; secret=sk-do-not-surface HTTP 401");
            let failure = TurnFailure::from_debug(&raw);
            assert_eq!(failure.category, TurnFailureCategory::Validation, "{raw}");
            assert!(!failure.retryable);
            assert!(failure.operator_message.contains(expected), "{failure:?}");
            assert!(!failure.operator_message.contains("sk-do-not-surface"));
            assert!(!crate::inference_router::should_advance_fallback(
                failure.category
            ));
            assert_eq!(failure.debug_message, raw);
        }
    }

    #[test]
    fn unknown_failure_is_not_retryable() {
        let failure = TurnFailure::from_debug("provider returned an unexplained response");
        assert_eq!(failure.category, TurnFailureCategory::Unknown);
        assert!(!failure.retryable);
        assert!(!failure.operator_message.contains("Try again in a moment"));
        assert!(failure.operator_message.contains("error details"));
        assert_eq!(
            failure.debug_message,
            "provider returned an unexplained response"
        );
    }
}
