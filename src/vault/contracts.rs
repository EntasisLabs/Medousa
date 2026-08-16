//! H07 vault mutation contracts (frozen in H07.1a).

use std::fmt;

use medousa_store::{CommitReceipt, DurabilityLevel, PersistenceError, StoreKind, StoreRootError};
use serde::{Deserialize, Serialize};

use crate::vault::note::VaultNoteSource;

/// Opaque product vault root identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VaultRootId(String);

impl VaultRootId {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VaultRootId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Opaque note version / ETag. Clients treat this as opaque.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NoteVersion(String);

const NOTE_VERSION_SCHEMA: &str = "v1";

impl NoteVersion {
    pub fn from_digest(digest: impl Into<String>) -> Self {
        // Legacy helper kept for tests; prefer [`Self::encode`].
        Self(digest.into())
    }

    /// Opaque blob: schema + root + source + note generation + content digest.
    pub fn encode(
        root_id: &str,
        source: &VaultNoteSource,
        note_generation: u64,
        content_digest: &str,
    ) -> Self {
        use base64::Engine;
        let source = match source {
            VaultNoteSource::User => "user",
            VaultNoteSource::ProjectOverlay => "overlay",
        };
        let raw = format!(
            "{NOTE_VERSION_SCHEMA}\n{root_id}\n{source}\n{note_generation}\n{content_digest}"
        );
        Self(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse a client If-Match token (opaque blob or legacy digest).
    pub fn parse(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn is_encoded(&self) -> bool {
        Self::decode_parts(self).is_some()
    }

    /// Content digest embedded in an opaque version, or the raw string for legacy digests.
    pub fn content_digest_owned(&self) -> String {
        if let Some((_, _, _, _, digest)) = Self::decode_parts(self) {
            return digest;
        }
        self.0.clone()
    }

    /// Exact match for encoded tokens. Legacy digest tokens match only a
    /// still-legacy resident version with the same digest string.
    pub fn matches_precondition(&self, expected: &NoteVersion) -> bool {
        if self == expected {
            return true;
        }
        if self.is_encoded() {
            return false;
        }
        !expected.is_encoded() && self.0 == expected.0
    }

    fn decode_parts(version: &NoteVersion) -> Option<(String, String, String, u64, String)> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(version.0.as_bytes())
            .ok()?;
        let text = String::from_utf8(bytes).ok()?;
        let mut parts = text.splitn(5, '\n');
        let schema = parts.next()?;
        if schema != NOTE_VERSION_SCHEMA {
            return None;
        }
        let root = parts.next()?.to_string();
        let source = parts.next()?.to_string();
        let note_generation = parts.next()?.parse().ok()?;
        let digest = parts.next()?.to_string();
        Some((schema.to_string(), root, source, note_generation, digest))
    }
}

impl fmt::Display for NoteVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationPrecondition {
    CreateOnly,
    Match,
    AbsentOrMatch,
    Unconditional,
}

#[derive(Debug, Clone)]
pub enum VaultMutationError {
    StaleVersion {
        current: Option<NoteVersion>,
    },
    Conflict(String),
    Overloaded,
    IndexRepairRequired {
        receipt: CommitReceipt,
        detail: String,
    },
    ExternallyAmbiguous(String),
    Persistence(String),
    Invalid(String),
}

impl fmt::Display for VaultMutationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleVersion { .. } => f.write_str("stale_version"),
            Self::Conflict(message) => write!(f, "conflict: {message}"),
            Self::Overloaded => f.write_str("overloaded"),
            Self::IndexRepairRequired { detail, .. } => {
                write!(f, "index_repair_required: {detail}")
            }
            Self::ExternallyAmbiguous(message) => write!(f, "externally_ambiguous: {message}"),
            Self::Persistence(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for VaultMutationError {}

impl From<PersistenceError> for VaultMutationError {
    fn from(error: PersistenceError) -> Self {
        Self::Persistence(error.to_string())
    }
}

impl From<StoreRootError> for VaultMutationError {
    fn from(error: StoreRootError) -> Self {
        Self::Persistence(error.to_string())
    }
}

impl From<anyhow::Error> for VaultMutationError {
    fn from(error: anyhow::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}

/// Lane identity: ordered `(root_id, source, normalized_path)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VaultLaneKey {
    pub root_id: VaultRootId,
    pub source: VaultNoteSource,
    pub normalized_path: String,
}

impl VaultLaneKey {
    pub fn new(
        root_id: VaultRootId,
        source: VaultNoteSource,
        normalized_path: impl Into<String>,
    ) -> Self {
        Self {
            root_id,
            source,
            normalized_path: normalized_path.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMutationIntent {
    pub operation_id: String,
    pub root_id: String,
    pub path: String,
    pub precondition: MutationPrecondition,
    pub expected_version: Option<String>,
    pub content_digest: String,
    pub vault_generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMutationReceiptRecord {
    pub operation_id: String,
    pub root_id: String,
    pub path: String,
    pub note_version: String,
    pub vault_generation: u64,
    pub bytes: usize,
}

#[derive(Debug, Clone)]
pub struct VaultCommitOutcome {
    pub receipt: CommitReceipt,
    pub note_version: NoteVersion,
    pub vault_generation: u64,
    pub index_repair_required: bool,
}

pub fn vault_receipt(
    key: impl Into<String>,
    generation: u64,
    bytes: usize,
    durability: DurabilityLevel,
) -> CommitReceipt {
    CommitReceipt::new(StoreKind::Vault, key, generation, durability, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_keys_order_for_multi_path_acquire() {
        let root = VaultRootId::new("personal");
        let a = VaultLaneKey::new(root.clone(), VaultNoteSource::User, "a.md");
        let b = VaultLaneKey::new(root, VaultNoteSource::User, "b.md");
        assert!(a < b);
    }
}
