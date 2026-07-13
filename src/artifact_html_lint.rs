//! Static HTML lint for presentation artifacts (doctor / self-heal).

use medousa_types::component_runtime::ComponentStaticLintFinding;

pub fn lint_artifact_html(html: &str) -> Vec<ComponentStaticLintFinding> {
    let mut findings = Vec::new();
    let lower = html.to_lowercase();

    if lower.contains("localstorage") {
        findings.push(finding(
            "STATIC_LOCALSTORAGE",
            "error",
            "Artifact references localStorage — blocked in sandboxed srcdoc iframes.",
            "Use window.MedousaStore.get/set instead of localStorage.",
        ));
    }
    if lower.contains("sessionstorage") {
        findings.push(finding(
            "STATIC_SESSIONSTORAGE",
            "error",
            "Artifact references sessionStorage — blocked in sandboxed srcdoc iframes.",
            "Use window.MedousaStore.get/set instead of sessionStorage.",
        ));
    }
    if lower.contains("indexeddb") {
        findings.push(finding(
            "STATIC_INDEXEDDB",
            "warn",
            "Artifact references indexedDB — unavailable in sandboxed presentation iframes.",
            "Use MedousaStore for durable widget state.",
        ));
    }

    if has_bare_medousa_store_get(html) {
        findings.push(finding(
            "STATIC_STORE_GET_NO_KEY",
            "warn",
            "MedousaStore.get() called without a key returns all entries (object), not an array.",
            "Use MedousaStore.get('thoughts') and guard with Array.isArray(raw) ? raw : [].",
        ));
    }

    if has_slice_without_array_guard(html) {
        findings.push(finding(
            "STATIC_SLICE_WITHOUT_GUARD",
            "warn",
            "Code calls .slice() on a value that may not be an array (common store read bug).",
            "const items = Array.isArray(raw) ? raw : []; before calling .slice().",
        ));
    }

    if lower.contains("medousastore.set") && !lower.contains("medousastore.ready") {
        findings.push(finding(
            "STATIC_STORE_SET_WITHOUT_READY",
            "info",
            "MedousaStore.set without MedousaStore.ready() guard — fails on chat-only embeds.",
            "if (!MedousaStore.ready()) return; before set on canvas-only widgets.",
        ));
    }

    if has_sync_medousa_store_usage(html) {
        findings.push(finding(
            "STATIC_STORE_SYNC_USAGE",
            "error",
            "MedousaStore.get/set/delete return Promises — calling them without await breaks persistence silently.",
            "Use async/await: const raw = await MedousaStore.get('key'); await MedousaStore.set('key', value);",
        ));
    }

    findings
}

pub fn inspect_embed_injection(prepared_html: &str) -> (bool, bool, bool, bool) {
    (
        prepared_html.contains("medousa-store-bootstrap-script"),
        prepared_html.contains("medousa-artifact-metrics-script"),
        prepared_html.contains("medousa-artifact-runtime-script"),
        prepared_html.contains("medousa-store-client-script"),
    )
}

fn finding(code: &str, severity: &str, message: &str, _fix_hint: &str) -> ComponentStaticLintFinding {
    ComponentStaticLintFinding {
        code: code.to_string(),
        severity: severity.to_string(),
        message: message.to_string(),
        line_hint: None,
    }
}

fn has_bare_medousa_store_get(html: &str) -> bool {
    let patterns = [
        "MedousaStore.get()",
        "MedousaStore.get( )",
        "medousastore.get()",
    ];
    patterns.iter().any(|pattern| html.contains(pattern))
}

fn has_slice_without_array_guard(html: &str) -> bool {
    if !html.contains(".slice(") {
        return false;
    }
    let lower = html.to_lowercase();
    lower.contains(".slice(")
        && !lower.contains("array.isarray")
        && (lower.contains("thoughts") || lower.contains("medousastore.get"))
}

fn has_sync_medousa_store_usage(html: &str) -> bool {
    for line in html.lines() {
        if line_uses_medousa_store_without_await(line) {
            return true;
        }
    }
    false
}

fn line_uses_medousa_store_without_await(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if is_medousa_store_type_check(&lower) {
        return false;
    }
    if !lower.contains("medousastore.get(")
        && !lower.contains("medousastore.set(")
        && !lower.contains("medousastore.delete(")
    {
        return false;
    }
    if lower.contains(".then(") || lower.contains(".catch(") {
        return false;
    }
    if has_await_before_store_call(trimmed) {
        return false;
    }
    true
}

fn is_medousa_store_type_check(lower: &str) -> bool {
    lower.contains("typeof ")
        || lower.contains("=== 'function'")
        || lower.contains("medousastore.get ===")
        || lower.contains("medousastore.set ===")
        || lower.contains("medousastore.delete ===")
}

fn has_await_before_store_call(line: &str) -> bool {
    let lower = line.to_lowercase();
    for op in [
        "medousastore.get(",
        "medousastore.set(",
        "medousastore.delete(",
    ] {
        if let Some(idx) = lower.find(op) {
            let before = lower[..idx].trim();
            if before.ends_with("await") || before.contains("await ") {
                return true;
            }
        }
    }
    false
}

pub fn json_value_type(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(_) => "number".to_string(),
        serde_json::Value::String(_) => "string".to_string(),
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    }
}

pub fn lint_store_value(key: &str, value: &serde_json::Value) -> Vec<String> {
    let mut issues = Vec::new();
    let key_lower = key.to_lowercase();
    if (key_lower.contains("thought") || key_lower.ends_with("items") || key_lower.ends_with("list"))
        && !value.is_array() {
            issues.push(format!(
                "expected_array_got_{}",
                json_value_type(value)
            ));
        }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_localstorage_and_bare_get() {
        let html = r#"
        <script>
        localStorage.setItem('x', '1');
        const all = MedousaStore.get();
        thoughts.slice();
        </script>"#;
        let findings = lint_artifact_html(html);
        let codes: Vec<_> = findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"STATIC_LOCALSTORAGE"));
        assert!(codes.contains(&"STATIC_STORE_GET_NO_KEY"));
        assert!(codes.contains(&"STATIC_SLICE_WITHOUT_GUARD"));
    }

    #[test]
    fn flags_sync_medousa_store_usage() {
        let html = r#"
        <script>
        function load() {
          return window.MedousaStore.get('thoughts');
        }
        </script>"#;
        let findings = lint_artifact_html(html);
        let codes: Vec<_> = findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"STATIC_STORE_SYNC_USAGE"));
    }

    #[test]
    fn allows_awaited_medousa_store_usage() {
        let html = r#"
        <script>
        async function load() {
          const raw = await MedousaStore.get('thoughts');
          return Array.isArray(raw) ? raw : [];
        }
        </script>"#;
        let findings = lint_artifact_html(html);
        let codes: Vec<_> = findings.iter().map(|f| f.code.as_str()).collect();
        assert!(!codes.contains(&"STATIC_STORE_SYNC_USAGE"));
    }
}
