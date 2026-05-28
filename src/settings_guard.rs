use std::collections::HashSet;

use serde_json::{Map, Value};

const SENSITIVE_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "authorization",
    "auth",
    "token",
    "secret",
    "password",
    "x-api-key",
];

pub fn redact_json_value(value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut next = Map::new();
            for (key, raw) in obj {
                if is_sensitive_key(key) {
                    next.insert(key.clone(), Value::String("[REDACTED]".to_string()));
                } else {
                    next.insert(key.clone(), redact_json_value(raw));
                }
            }
            Value::Object(next)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(redact_json_value).collect::<Vec<_>>())
        }
        Value::String(text) => Value::String(redact_text(text)),
        _ => value.clone(),
    }
}

pub fn parse_allowed_modules(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut modules = Vec::new();

    for token in raw
        .split(|c: char| c == ',' || c == '\n' || c == '\t' || c == ' ')
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let lowered = token.to_ascii_lowercase();
        if seen.insert(lowered) {
            modules.push(token.to_string());
        }
    }

    modules
}

pub fn invalid_module_ids(modules: &[String]) -> Vec<String> {
    modules
        .iter()
        .filter(|module| !is_valid_module_id(module))
        .cloned()
        .collect()
}

pub fn is_valid_module_id(module: &str) -> bool {
    if module.is_empty() || module.len() > 120 {
        return false;
    }

    if !module.contains('.') {
        return false;
    }

    module
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    SENSITIVE_KEYS
        .iter()
        .any(|candidate| normalized.contains(candidate))
}

fn redact_text(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("bearer ") || lower.starts_with("token ") {
        return "[REDACTED]".to_string();
    }

    text.to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{invalid_module_ids, parse_allowed_modules, redact_json_value};

    #[test]
    fn redacts_nested_sensitive_json_fields() {
        let value = json!({
            "headers": {
                "Authorization": "Bearer secret",
                "X-API-Key": "123"
            },
            "query": "safe"
        });

        let redacted = redact_json_value(&value);
        assert_eq!(redacted["headers"]["Authorization"], "[REDACTED]");
        assert_eq!(redacted["headers"]["X-API-Key"], "[REDACTED]");
        assert_eq!(redacted["query"], "safe");
    }

    #[test]
    fn parses_allowed_modules_as_unique_list() {
        let parsed = parse_allowed_modules("websearch.search, http.fetch websearch.search");
        assert_eq!(parsed, vec!["websearch.search", "http.fetch"]);
    }

    #[test]
    fn validates_module_id_shape() {
        let modules = vec![
            "websearch.search".to_string(),
            "bad module".to_string(),
            "missingdot".to_string(),
        ];

        let invalid = invalid_module_ids(&modules);
        assert_eq!(invalid, vec!["bad module", "missingdot"]);
    }
}
