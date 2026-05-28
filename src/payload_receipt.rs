use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const DEFAULT_MAX_INLINE_BYTES: usize = 8 * 1024;
pub const DEFAULT_PREVIEW_CHARS: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactReceiptMeta {
    pub content_type: String,
    pub inline: bool,
    pub byte_size: usize,
    pub max_inline_bytes: usize,
    pub hash64: String,
}

pub fn receipt_meta(value: &Value, max_inline_bytes: usize) -> Option<ArtifactReceiptMeta> {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    let byte_size = serialized.len();
    if byte_size <= max_inline_bytes {
        return None;
    }

    Some(ArtifactReceiptMeta {
        content_type: "application/json".to_string(),
        inline: false,
        byte_size,
        max_inline_bytes,
        hash64: hash_text(&serialized),
    })
}

pub fn inline_or_receipt(
    value: &Value,
    max_inline_bytes: usize,
    preview_chars: usize,
) -> (Value, bool) {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    let byte_size = serialized.len();
    if byte_size <= max_inline_bytes {
        return (value.clone(), false);
    }

    let preview = truncate_chars(&serialized, preview_chars);
    let hash64 = hash_text(&serialized);

    (
        json!({
            "artifact_receipt": {
                "content_type": "application/json",
                "inline": false,
                "byte_size": byte_size,
                "max_inline_bytes": max_inline_bytes,
                "preview": preview,
                "hash64": hash64,
            }
        }),
        true,
    )
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

fn hash_text(text: &str) -> String {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{inline_or_receipt, receipt_meta};

    #[test]
    fn keeps_small_payload_inline() {
        let value = json!({"ok": true, "items": [1, 2, 3]});
        let (mapped, converted) = inline_or_receipt(&value, 1024, 120);

        assert!(!converted);
        assert_eq!(mapped, value);
    }

    #[test]
    fn replaces_large_payload_with_receipt() {
        let value = json!({"data": "x".repeat(5000)});
        let (mapped, converted) = inline_or_receipt(&value, 256, 80);

        assert!(converted);
        let receipt = mapped
            .get("artifact_receipt")
            .expect("artifact_receipt should exist");
        assert_eq!(receipt["inline"], false);
        assert!(
            receipt["byte_size"]
                .as_u64()
                .expect("byte_size should be set")
                > 256
        );
        let preview = receipt["preview"].as_str().expect("preview should be text");
        assert!(preview.len() <= 83);
    }

    #[test]
    fn builds_receipt_meta_for_large_payload() {
        let value = json!({"data": "x".repeat(5000)});
        let meta = receipt_meta(&value, 256).expect("receipt meta should exist");

        assert_eq!(meta.content_type, "application/json");
        assert!(!meta.inline);
        assert!(meta.byte_size > 256);
        assert_eq!(meta.max_inline_bytes, 256);
        assert!(!meta.hash64.is_empty());
    }
}
