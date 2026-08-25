//! Target-neutral recognition and redaction of opaque Grapheme secret grants.

const GRANT_PREFIX: &[u8] = b"sgrant-";
const GRANT_HEX_LEN: usize = 32;

/// Find concrete opaque grant ids embedded in source. Placeholders such as
/// `sgrant-…` are intentionally ignored; real ids are the prefix plus a UUID's
/// 32 hexadecimal digits.
pub fn secret_grant_references(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let total_len = GRANT_PREFIX.len() + GRANT_HEX_LEN;
    let mut references = Vec::new();
    let mut index = 0;
    while index + total_len <= bytes.len() {
        if &bytes[index..index + GRANT_PREFIX.len()] != GRANT_PREFIX {
            index += 1;
            continue;
        }
        let end = index + total_len;
        let hex = &bytes[index + GRANT_PREFIX.len()..end];
        let boundary_ok = end == bytes.len() || !bytes[end].is_ascii_hexdigit();
        if hex.iter().all(u8::is_ascii_hexdigit) && boundary_ok {
            let grant = source[index..end].to_string();
            if !references.contains(&grant) {
                references.push(grant);
            }
            index = end;
        } else {
            index += GRANT_PREFIX.len();
        }
    }
    references
}

pub fn source_contains_secret_grant(source: &str) -> bool {
    !secret_grant_references(source).is_empty()
}

pub fn redact_secret_grant_references(text: &str) -> String {
    secret_grant_references(text)
        .into_iter()
        .fold(text.to_string(), |redacted, grant| {
            redacted.replace(&grant, "[REDACTED_GRANT]")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_concrete_grants_and_redacts_them() {
        let grant = "sgrant-0123456789abcdef0123456789abcdef";
        assert_eq!(secret_grant_references(grant), vec![grant]);
        assert!(!source_contains_secret_grant("get(name: \"sgrant-…\")"));
        assert_eq!(
            redact_secret_grant_references(&format!("before {grant} after")),
            "before [REDACTED_GRANT] after"
        );
    }
}
