//! Tool naming helpers: Stasis advertises sanitized snake_case aliases for dotted registry names.

use std::collections::HashSet;

/// Same rules as `stasis::InMemoryToolRegistry::sanitize_tool_name` (non-alnum → `_`).
pub fn sanitize_tool_advertised_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }

    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "tool".to_string()
    } else {
        trimmed.to_string()
    }
}

/// True when `name` matches an allowlist entry exactly or after sanitization (dot vs snake).
pub fn tool_allowed_matches(name: &str, allowlist: &HashSet<String>) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return false;
    }
    if allowlist.contains(trimmed) {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if allowlist.contains(&lower) {
        return true;
    }

    let sanitized = sanitize_tool_advertised_name(trimmed);
    if allowlist.contains(&sanitized) {
        return true;
    }

    for entry in allowlist {
        if entry.eq_ignore_ascii_case(trimmed) {
            return true;
        }
        if sanitize_tool_advertised_name(entry) == sanitized {
            return true;
        }
    }

    false
}

/// Legacy dot-name tools that were renamed to snake_case (invoke either form).
pub fn legacy_tool_name_aliases(name: &str) -> &'static [&'static str] {
    match sanitize_tool_advertised_name(name).as_str() {
        "cognition_util_time_now" => &["cognition_utility_time_now"],
        "cognition_util_time_day_of_week" => &["cognition_utility_day_of_week"],
        "cognition_util_id_uuid" => &["cognition_utility_uuid"],
        _ => &[],
    }
}

pub fn tool_allowed_matches_with_legacy(name: &str, allowlist: &HashSet<String>) -> bool {
    if tool_allowed_matches(name, allowlist) {
        return true;
    }
    for alias in legacy_tool_name_aliases(name) {
        if tool_allowed_matches(alias, allowlist) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_dots_to_underscores() {
        assert_eq!(
            sanitize_tool_advertised_name("cognition.capability.resolve"),
            "cognition_capability_resolve"
        );
        assert_eq!(
            sanitize_tool_advertised_name("cognition.job.enqueue"),
            "cognition_job_enqueue"
        );
    }

    #[test]
    fn allowlist_matches_sanitized_model_name() {
        let allowlist: HashSet<String> = ["cognition.job.enqueue", "cognition.capability.list"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert!(tool_allowed_matches(
            "cognition_job_enqueue",
            &allowlist
        ));
        assert!(tool_allowed_matches(
            "cognition_capability_list",
            &allowlist
        ));
    }

    #[test]
    fn legacy_util_names_match_utility_allowlist() {
        let allowlist: HashSet<String> = ["cognition_utility_time_now"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert!(tool_allowed_matches_with_legacy(
            "cognition_util_time_now",
            &allowlist
        ));
    }
}
