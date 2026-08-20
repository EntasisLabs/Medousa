//! Typed identifiers that may select durable storage.
//!
//! Logical identifiers remain visible on the wire. Filesystem names are full,
//! domain-separated SHA-256 keys so normalization and platform aliases cannot
//! collapse distinct authorities onto the same object.

use std::fmt;

use sha2::{Digest, Sha256};

const MAX_IDENTIFIER_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierError {
    kind: &'static str,
    reason: &'static str,
}

impl IdentifierError {
    pub(crate) fn new(kind: &'static str, reason: &'static str) -> Self {
        Self { kind, reason }
    }
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid {}: {}", self.kind, self.reason)
    }
}

impl std::error::Error for IdentifierError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageAuthorityKey(String);

impl StorageAuthorityKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn derive(prefix: &'static str, domain: &'static str, value: &str) -> Self {
        Self::derive_many(prefix, domain, &[value])
    }

    pub(crate) fn derive_many(prefix: &'static str, domain: &'static str, values: &[&str]) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"medousa-storage-authority\0");
        digest.update((domain.len() as u64).to_be_bytes());
        digest.update(domain.as_bytes());
        for value in values {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value.as_bytes());
        }
        Self(format!("{prefix}{:x}", digest.finalize()))
    }
}

macro_rules! authority_id {
    ($name:ident, $kind:literal, $prefix:literal, $domain:literal, $validator:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: &str) -> Result<Self, IdentifierError> {
                validate_common(value, $kind)?;
                if !($validator)(value) {
                    return Err(IdentifierError::new($kind, "unsupported_syntax"));
                }
                Ok(Self(value.to_string()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn storage_key(&self) -> StorageAuthorityKey {
                StorageAuthorityKey::derive($prefix, $domain, self.as_str())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

fn validate_common(value: &str, kind: &'static str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(kind, "empty"));
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(IdentifierError::new(kind, "too_long"));
    }
    if !value.is_ascii() {
        return Err(IdentifierError::new(kind, "non_ascii"));
    }
    if value.trim() != value {
        return Err(IdentifierError::new(kind, "surrounding_whitespace"));
    }
    if value.chars().any(char::is_control) {
        return Err(IdentifierError::new(kind, "control_character"));
    }
    Ok(())
}

fn lower_slug(value: &str, allow_dot: bool) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|ch| {
            ch.is_ascii_lowercase()
                || ch.is_ascii_digit()
                || ch == '-'
                || ch == '_'
                || (allow_dot && ch == '.')
        })
}

authority_id!(
    EnvironmentProfileId,
    "environment_profile_id",
    "ep1-",
    "environment-profile",
    |value: &str| {
        lower_slug(value, false)
            || value
                .strip_prefix("user:")
                .is_some_and(|slug| lower_slug(slug, false))
    }
);

pub fn component_store_record_key(
    profile: &EnvironmentProfileId,
    component: &ComponentId,
    key: &ComponentStoreKey,
) -> StorageAuthorityKey {
    StorageAuthorityKey::derive_many(
        "cs1-",
        "component-store-record",
        &[profile.as_str(), component.as_str(), key.as_str()],
    )
}

pub fn component_runtime_event_record_key(
    profile: &EnvironmentProfileId,
    component: &ComponentId,
    nonce: &str,
) -> StorageAuthorityKey {
    StorageAuthorityKey::derive_many(
        "cr1-",
        "component-runtime-event",
        &[profile.as_str(), component.as_str(), nonce],
    )
}
authority_id!(
    ComponentId,
    "component_id",
    "co1-",
    "component",
    |value: &str| lower_slug(value, true)
);
authority_id!(
    ComponentStoreKey,
    "component_store_key",
    "ck1-",
    "component-store-key",
    |value: &str| {
        value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    }
);
authority_id!(PackageId, "package_id", "pk1-", "package", |value: &str| {
    lower_slug(value, false)
});
authority_id!(ModelId, "model_id", "mo1-", "model", |value: &str| {
    lower_slug(value, false)
});
authority_id!(
    PairingDeviceId,
    "pairing_device_id",
    "pd1-",
    "pairing-device",
    |value: &str| value.bytes().all(|byte| matches!(byte, 0x21..=0x7e))
);
authority_id!(FeedId, "feed_id", "fd1-", "feed", |value: &str| {
    lower_slug(value, true)
});
authority_id!(
    ManuscriptId,
    "manuscript_id",
    "ms1-",
    "manuscript",
    |value: &str| lower_slug(value, false)
);
authority_id!(
    ProviderId,
    "provider_id",
    "pv1-",
    "provider",
    |value: &str| { lower_slug(value, true) }
);
authority_id!(
    ManuscriptOverlayProposalId,
    "manuscript_overlay_proposal_id",
    "mp1-",
    "manuscript-overlay-proposal",
    |value: &str| lower_slug(value, false)
);
authority_id!(
    WorkshopScopeId,
    "workshop_scope_id",
    "ws1-",
    "workshop-scope",
    |value: &str| value.bytes().all(|byte| matches!(byte, 0x21..=0x7e))
);
authority_id!(
    TurnEventId,
    "turn_event_id",
    "te1-",
    "turn-event",
    |value: &str| value.bytes().all(|byte| matches!(byte, 0x21..=0x7e))
);
authority_id!(
    GraphemeRefId,
    "grapheme_ref_id",
    "gr1-",
    "grapheme-ref",
    |value: &str| value.bytes().all(|byte| matches!(byte, 0x21..=0x7e))
);
authority_id!(
    GraphemeScriptId,
    "grapheme_script_id",
    "gs1-",
    "grapheme-script",
    |value: &str| lower_slug(value, false)
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossy_aliases_have_distinct_storage_authority() {
        let dotted = ComponentId::parse("reader.v1").unwrap();
        let underscored = ComponentId::parse("reader_v1").unwrap();
        assert_ne!(dotted.storage_key(), underscored.storage_key());
    }

    #[test]
    fn domains_cannot_alias_the_same_logical_text() {
        let component = ComponentId::parse("personal").unwrap();
        let profile = EnvironmentProfileId::parse("personal").unwrap();
        assert_ne!(component.storage_key(), profile.storage_key());
    }

    #[test]
    fn hostile_and_normalized_spellings_are_rejected() {
        for value in [
            "../work",
            "/work",
            "work\\admin",
            " work",
            "work ",
            "CON",
            "wörk",
        ] {
            assert!(EnvironmentProfileId::parse(value).is_err(), "{value:?}");
        }
    }
}
