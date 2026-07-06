//! Large tool payload receipt metadata carried on the stream sink port.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactReceiptMeta {
    pub content_type: String,
    pub inline: bool,
    pub byte_size: usize,
    pub max_inline_bytes: usize,
    pub hash64: String,
}
