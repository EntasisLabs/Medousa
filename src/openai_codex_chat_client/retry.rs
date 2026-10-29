//! Bounded pre-output provider retry policy; never a turn/tool replay policy.
use std::time::Duration;

pub(super) fn permanent_usage_error(code: &str) -> bool {
    matches!(
        code,
        "subscription_sharing_usage_limit_exceeded"
            | "insufficient_quota"
            | "billing_hard_limit_reached"
    )
}

pub(super) fn delay(
    attempt: usize,
    retry_after: Option<Duration>,
    jitter_ms: u64,
) -> Option<Duration> {
    let backoff = Duration::from_secs(1u64 << attempt.saturating_sub(1).min(5));
    let delay = backoff
        .max(retry_after.unwrap_or_default())
        .saturating_add(Duration::from_millis(jitter_ms.min(250)));
    (delay <= Duration::from_secs(120)).then_some(delay)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exponential_backoff_is_bounded() {
        assert_eq!(delay(1, None, 0), Some(Duration::from_secs(1)));
        assert_eq!(delay(2, None, 0), Some(Duration::from_secs(2)));
        assert_eq!(
            delay(usize::MAX, None, 999),
            Some(Duration::from_millis(32250))
        );
    }
    #[test]
    fn cooldown_is_a_floor_not_a_cap() {
        assert_eq!(
            delay(1, Some(Duration::from_secs(30)), 250),
            Some(Duration::from_millis(30250))
        );
        assert_eq!(
            delay(1, Some(Duration::ZERO), 0),
            Some(Duration::from_secs(1))
        );
    }
    #[test]
    fn long_cooldown_is_not_retried_early() {
        assert_eq!(delay(1, Some(Duration::from_secs(121)), 0), None);
        assert_eq!(delay(1, Some(Duration::MAX), 250), None);
    }
    #[test]
    fn quota_and_billing_are_not_transient() {
        for code in [
            "insufficient_quota",
            "billing_hard_limit_reached",
            "subscription_sharing_usage_limit_exceeded",
        ] {
            assert!(permanent_usage_error(code));
        }
        assert!(!permanent_usage_error("rate_limit_exceeded"));
    }
}
