//! Shared identity coordinates for daemon tools and capability execution.

const DEFAULT_PERSONA_ID: &str = "persona:default";
const DEFAULT_CHANNEL_ID: &str = "channel:default";

pub fn resolve_identity_persona_id() -> String {
    non_empty_env("MEDOUSA_IDENTITY_PERSONA_ID")
        .or_else(|| non_empty_env("STASIS_DEFAULT_PERSONA_ID"))
        .unwrap_or_else(|| DEFAULT_PERSONA_ID.to_string())
}

pub fn resolve_identity_user_id(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| non_empty_env("MEDOUSA_IDENTITY_USER_ID"))
        .or_else(|| non_empty_env("STASIS_DEFAULT_USER_ID"))
        .unwrap_or_else(|| crate::user_profiles::DEFAULT_USER_ID.to_string())
}

pub fn resolve_tool_identity_user_id(session_id: &str, workshop_operator: bool) -> String {
    if workshop_operator {
        crate::user_profiles::resolve_workshop_identity_user_id()
    } else {
        resolve_identity_user_id(Some(session_id))
    }
}

pub fn resolve_identity_channel_id(policy_profile: Option<&str>) -> String {
    if let Some(profile) = policy_profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if profile.eq_ignore_ascii_case("interactive") {
            return workshop_interactive_channel_id();
        }
        return format!("channel:{}", profile.to_ascii_lowercase());
    }
    non_empty_env("MEDOUSA_IDENTITY_CHANNEL_ID")
        .or_else(|| non_empty_env("STASIS_DEFAULT_CHANNEL_ID"))
        .unwrap_or_else(|| DEFAULT_CHANNEL_ID.to_string())
}

pub fn workshop_interactive_channel_id() -> String {
    match crate::user_profiles::profile_slug_from_id(
        &crate::user_profiles::resolve_workshop_active_profile_id(),
    ) {
        Some(slug) if slug != "default" => format!("channel:{slug}"),
        _ => "channel:interactive".to_string(),
    }
}

pub fn profile_channel_id_for_user_id(user_id: &str) -> String {
    match crate::user_profiles::profile_slug_from_id(user_id) {
        Some(slug) if slug != "default" => format!("channel:{slug}"),
        _ => "channel:interactive".to_string(),
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

