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

    pub fn validation(operator_message: impl Into<String>, debug_message: impl Into<String>) -> Self {
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
            retryable: true,
        }
    }

    pub fn category_label(&self) -> &'static str {
        self.category.as_str()
    }
}

pub fn is_error_turn_excluded_from_model_context(turn: &crate::session::ConversationTurn) -> bool {
    turn.answer_state.as_deref() == Some("error")
}

fn classify_category(raw: &str) -> TurnFailureCategory {
    let text = raw.to_ascii_lowercase();

    if text.contains("cancelled") || text.contains("canceled") {
        return TurnFailureCategory::Cancelled;
    }
    if text.contains("session_id")
        && (text.contains("required") || text.contains("must not be empty"))
        || text.contains("prompt are required")
        || text.contains("prompt is required")
    {
        return TurnFailureCategory::Validation;
    }
    if text.contains("401")
        || text.contains("403")
        || text.contains("invalid api key")
        || text.contains("invalid_api_key")
        || text.contains("incorrect api key")
        || text.contains("authentication")
        || text.contains("unauthorized")
    {
        return TurnFailureCategory::Auth;
    }
    if text.contains("429")
        || text.contains("rate limit")
        || text.contains("rate_limit")
        || text.contains("too many requests")
        || text.contains("quota")
    {
        return TurnFailureCategory::RateLimit;
    }
    if text.contains("404")
        || text.contains("model not found")
        || text.contains("does not exist")
        || text.contains("unknown model")
        || text.contains("invalid model")
    {
        return TurnFailureCategory::ModelNotFound;
    }
    if text.contains("timeout")
        || text.contains("timed out")
        || text.contains("deadline exceeded")
    {
        return TurnFailureCategory::Timeout;
    }
    if text.contains("connection")
        || text.contains("transport")
        || text.contains("temporar")
        || text.contains("unavailable")
        || text.contains("502")
        || text.contains("503")
        || text.contains("504")
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
            | TurnFailureCategory::Timeout
            | TurnFailureCategory::Unknown
    )
}

fn operator_message_for(category: TurnFailureCategory, raw: &str) -> String {
    match category {
        TurnFailureCategory::Auth => {
            "The model provider rejected the API key. Check Settings → Models and try again."
                .to_string()
        }
        TurnFailureCategory::RateLimit => {
            "The model provider is rate-limiting requests. Wait a moment and try again."
                .to_string()
        }
        TurnFailureCategory::ModelNotFound => {
            "That model isn't available right now. Choose a different model in Settings."
                .to_string()
        }
        TurnFailureCategory::ProviderDown => {
            "Couldn't reach the model provider. Try again in a moment.".to_string()
        }
        TurnFailureCategory::Timeout => {
            "The model took too long to respond. Try again with a shorter message.".to_string()
        }
        TurnFailureCategory::Cancelled => "Turn cancelled.".to_string(),
        TurnFailureCategory::Validation => validation_operator_message(raw),
        TurnFailureCategory::Unknown => {
            "Something went wrong on this turn. Try again in a moment.".to_string()
        }
    }
}

fn validation_operator_message(raw: &str) -> String {
    let text = raw.to_ascii_lowercase();
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
        assert!(!failure.operator_message.to_ascii_lowercase().contains("api key provided"));
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
}
