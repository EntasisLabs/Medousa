//! Heuristics for CAPTCHA / rate-limit pages.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeReason {
    CaptchaUrl,
    CaptchaBody,
    RateLimited,
    EmptyResults,
}

pub fn detect_challenge(url: &str, status: u16, body: &str, parsed_result_count: usize) -> Option<ChallengeReason> {
    let url_lower = url.to_ascii_lowercase();
    if url_lower.contains("/sorry")
        || url_lower.contains("captcha")
        || url_lower.contains("challenge-platform")
    {
        return Some(ChallengeReason::CaptchaUrl);
    }
    if status == 429 {
        return Some(ChallengeReason::RateLimited);
    }
    if status == 403 {
        return Some(ChallengeReason::RateLimited);
    }
    let body_lower = body.to_ascii_lowercase();
    let captcha_markers = [
        "unusual traffic",
        "verify you are human",
        "captcha",
        "cf-challenge",
        "challenge-form",
        "recaptcha",
        "hcaptcha",
    ];
    if captcha_markers.iter().any(|m| body_lower.contains(m)) {
        return Some(ChallengeReason::CaptchaBody);
    }
    if parsed_result_count == 0 && body.len() > 512 && !body_lower.contains("result__a") {
        return Some(ChallengeReason::EmptyResults);
    }
    None
}
