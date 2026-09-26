//! Canonical digest for committed conversation ranges shared by derivation and peers.

use crate::{SessionRef, TranscriptEntryRef};
use sha2::{Digest, Sha256};

pub fn conversation_range_digest<'a>(
    selection: &SessionRef,
    entries: impl IntoIterator<Item = (&'a TranscriptEntryRef, &'a str)>,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"medousa/conversation-range/v1\0");
    digest.update(selection.authority_id.as_str().as_bytes());
    digest.update(selection.session_id.as_str().as_bytes());
    for (source, content_digest) in entries {
        digest.update(source.entry_seq.to_be_bytes());
        digest.update(source.entry_id.as_str().as_bytes());
        digest.update(content_digest.as_bytes());
    }
    format!("sha256:{:x}", digest.finalize())
}
