use crate::spec::ContractError;

/// Stable semantic ID from method + path. Never derived from Rust fn names.
///
/// `GET /health` → `liveness.get`
/// `GET /v1/sessions/{id}/history` → `sessions.history.get`
pub fn stable_operation_id(method: &str, path: &str) -> String {
    let method = method.to_ascii_lowercase();
    if path == "/health" {
        return format!("liveness.{method}");
    }
    let trimmed = path
        .strip_prefix("/v1/")
        .or_else(|| path.strip_prefix('/'))
        .unwrap_or(path);
    let parts: Vec<String> = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            if let Some(name) = segment
                .strip_prefix('{')
                .and_then(|rest| rest.strip_suffix('}'))
            {
                format!(
                    "by_{}",
                    name.trim_start_matches('*').replace('-', "_")
                )
            } else {
                segment.replace('-', "_")
            }
        })
        .collect();
    let resource = if parts.is_empty() {
        "root".to_string()
    } else {
        parts.join(".")
    };
    format!("{resource}.{method}")
}

pub fn const_name(operation_id: &str) -> String {
    let mut name = String::new();
    for ch in operation_id.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch.to_ascii_uppercase());
        } else if !name.ends_with('_') {
            name.push('_');
        }
    }
    name.trim_matches('_').to_string()
}

pub fn path_parameters(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            segment
                .strip_prefix('{')
                .and_then(|rest| rest.strip_suffix('}'))
                .map(|name| name.trim_start_matches('*').to_string())
        })
        .collect()
}

/// Percent-encode one path segment. Unreserved characters stay literal.
pub fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub fn expand_path(template: &str, params: &[(&str, &str)]) -> Result<String, ContractError> {
    if template.contains('?') {
        return Err(ContractError::invalid(
            "query text must not be embedded in a path template",
        ));
    }
    let mut path = template.to_string();
    for (name, value) in params {
        let splat = format!("{{*{name}}}");
        let needle = format!("{{{name}}}");
        if path.contains(&splat) {
            path = path.replace(&splat, &encode_path_segment(value));
        } else if path.contains(&needle) {
            path = path.replace(&needle, &encode_path_segment(value));
        } else {
            return Err(ContractError::invalid(format!(
                "path template missing parameter {name}"
            )));
        }
    }
    if path.contains('{') {
        return Err(ContractError::invalid(
            "path template has unbound parameters",
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_semantic_and_stable() {
        assert_eq!(stable_operation_id("GET", "/health"), "liveness.get");
        assert_eq!(
            stable_operation_id("GET", "/v1/sessions/{id}/history"),
            "sessions.by_id.history.get"
        );
        assert_eq!(
            stable_operation_id("GET", "/v1/vault/notes"),
            "vault.notes.get"
        );
        assert_eq!(
            stable_operation_id("GET", "/v1/vault/notes/{path}"),
            "vault.notes.by_path.get"
        );
        assert_eq!(
            stable_operation_id("POST", "/v1/interactive/turn"),
            "interactive.turn.post"
        );
        assert_eq!(
            stable_operation_id("GET", "/v1/vault/notes/{*note_path}"),
            "vault.notes.by_note_path.get"
        );
        assert_eq!(
            const_name("vault.notes.by_note_path.get"),
            "VAULT_NOTES_BY_NOTE_PATH_GET"
        );
    }

    #[test]
    fn awkward_segments_are_percent_encoded_once() {
        assert_eq!(encode_path_segment("a/b"), "a%2Fb");
        assert_eq!(encode_path_segment("a b"), "a%20b");
        assert_eq!(encode_path_segment("café"), "caf%C3%A9");
        assert_eq!(encode_path_segment("%"), "%25");
        assert_eq!(encode_path_segment("?"), "%3F");
        assert_eq!(encode_path_segment("#"), "%23");
        let expanded = expand_path("/v1/vault/notes/{path}", &[("path", "a/b c")]).unwrap();
        assert_eq!(expanded, "/v1/vault/notes/a%2Fb%20c");
        let catch_all =
            expand_path("/v1/vault/files/{*file_path}", &[("file_path", "a/b")]).unwrap();
        assert_eq!(catch_all, "/v1/vault/files/a%2Fb");
    }
}
