//! Resolve durable conversation ranges and materialize them as derived sessions.

use std::collections::HashSet;

use chrono::Utc;
use medousa_types::daemon_api::{DeriveSessionRequest, DeriveSessionResponse};
use medousa_types::session::{
    ContextManifest, ContextManifestId, DerivationId, ResolvedConversationRange, SessionDerivation,
    SessionId, SessionRef, TranscriptEntryRef,
};
use sha2::{Digest as _, Sha256};

use crate::request_principal::RequestPrincipal;
use crate::session_store::{
    DerivationCommitOutcome, DerivationCommitRequest, StoreError, TranscriptAppend,
};

const MAX_IDEMPOTENCY_KEY_BYTES: usize = 256;
const MAX_DERIVATION_SOURCES: usize = 8;
const MAX_DERIVATION_ENTRIES: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivationErrorKind {
    Invalid,
    Forbidden,
    Conflict,
    Internal,
}

#[derive(Debug, Clone)]
pub struct DerivationError {
    pub kind: DerivationErrorKind,
    pub message: String,
}

impl DerivationError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: DerivationErrorKind::Invalid,
            message: message.into(),
        }
    }

    fn forbidden() -> Self {
        Self {
            kind: DerivationErrorKind::Forbidden,
            message: "one or more source ranges are unavailable".to_string(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            kind: DerivationErrorKind::Conflict,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: DerivationErrorKind::Internal,
            message: message.into(),
        }
    }
}

fn versioned_hash(domain: &[u8], chunks: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for chunk in chunks {
        digest.update((chunk.len() as u64).to_be_bytes());
        digest.update(chunk);
    }
    format!("{:x}", digest.finalize())
}

fn scoped_idempotency_digest(authority: &str, actor: &str, key: &str) -> String {
    format!(
        "sha256:{}",
        versioned_hash(
            b"medousa/session-derivation-idempotency/v1\0",
            &[authority.as_bytes(), actor.as_bytes(), key.as_bytes()],
        )
    )
}

fn deterministic_id(prefix: &str, domain: &[u8], key_digest: &str) -> String {
    let digest = versioned_hash(domain, &[key_digest.as_bytes()]);
    format!("{prefix}{}", &digest[..32])
}

fn request_digest(
    authority: &str,
    actor: &str,
    request: &DeriveSessionRequest,
) -> Result<String, DerivationError> {
    let encoded = serde_json::to_vec(request)
        .map_err(|error| DerivationError::internal(error.to_string()))?;
    Ok(format!(
        "sha256:{}",
        versioned_hash(
            b"medousa/session-derivation-request/v1\0",
            &[authority.as_bytes(), actor.as_bytes(), &encoded],
        )
    ))
}

fn range_digest(selection: &SessionRef, entries: &[TranscriptAppend]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"medousa/conversation-range/v1\0");
    digest.update(selection.authority_id.as_str().as_bytes());
    digest.update(selection.session_id.as_str().as_bytes());
    for entry in entries {
        let source = entry
            .source
            .as_ref()
            .expect("resolved derivation entries always have source coordinates");
        digest.update(source.entry_seq.to_be_bytes());
        digest.update(source.entry_id.as_str().as_bytes());
        digest.update(
            entry
                .expected_digest
                .as_deref()
                .expect("resolved derivation entries always have content digests")
                .as_bytes(),
        );
    }
    format!("sha256:{:x}", digest.finalize())
}

fn validate_idempotency_key(value: &str) -> Result<&str, DerivationError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_IDEMPOTENCY_KEY_BYTES
        || !value.bytes().all(|byte| matches!(byte, 0x21..=0x7e))
    {
        return Err(DerivationError::invalid(
            "Idempotency-Key must be 1-256 visible ASCII characters",
        ));
    }
    Ok(value)
}

fn normalize_request(request: &mut DeriveSessionRequest) -> Result<(), DerivationError> {
    if request.sources.is_empty() || request.sources.len() > MAX_DERIVATION_SOURCES {
        return Err(DerivationError::invalid(format!(
            "sources must contain 1-{MAX_DERIVATION_SOURCES} ranges"
        )));
    }
    request.intent = request.intent.trim().to_string();
    if request.intent.is_empty()
        || request.intent.len() > 64
        || !request
            .intent
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(DerivationError::invalid(
            "intent must be 1-64 identifier characters",
        ));
    }
    let catalog = request.target.catalog.as_deref().unwrap_or("single").trim();
    if catalog != "single" {
        return Err(DerivationError::invalid(
            "context derivation currently supports single-user targets only",
        ));
    }
    request.target.catalog = Some("single".to_string());
    request.target.display_name = request
        .target
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(())
}

fn map_store_error(error: StoreError) -> DerivationError {
    match error {
        StoreError::InvalidInput(message) => DerivationError::conflict(message),
        StoreError::Serialization(message)
        | StoreError::Backend(message)
        | StoreError::Worker(message) => DerivationError::internal(message),
    }
}

fn publish_catalog(target_session_id: &SessionId, display_name: Option<String>, profile_id: &str) {
    let entries =
        crate::session_store::get_session_store().load_transcript_entries(target_session_id);
    let turns = entries
        .into_iter()
        .map(|entry| entry.turn)
        .collect::<Vec<_>>();
    crate::session_catalog::replace_derived_session(
        target_session_id,
        display_name,
        profile_id,
        &turns,
    );
}

pub async fn derive_session(
    principal: &RequestPrincipal,
    mut request: DeriveSessionRequest,
    idempotency_key: &str,
) -> Result<DeriveSessionResponse, DerivationError> {
    normalize_request(&mut request)?;
    let idempotency_key = validate_idempotency_key(idempotency_key)?;
    let authority = crate::workshop_authority::current()
        .map_err(DerivationError::internal)?
        .clone();
    let profile_id = principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id);
    let actor = format!("profile:{profile_id}");
    let key_digest = scoped_idempotency_digest(authority.as_str(), &actor, idempotency_key);
    let request_digest = request_digest(authority.as_str(), &actor, &request)?;
    let target_session_id = SessionId::parse(deterministic_id(
        "ses_",
        b"medousa/derived-session/v1\0",
        &key_digest,
    ))
    .map_err(|error| DerivationError::internal(error.to_string()))?;

    let store = crate::session_store::get_session_store();
    if let Some(existing) = store
        .load_derivation(&target_session_id)
        .map_err(map_store_error)?
    {
        if existing.idempotency_key_digest != key_digest
            || existing.request_digest != request_digest
        {
            return Err(DerivationError::conflict(
                "idempotency key was already used for another derivation request",
            ));
        }
        let display_name = request.target.display_name.clone();
        publish_catalog(&target_session_id, display_name.clone(), &profile_id);
        return Ok(DeriveSessionResponse {
            authority_id: authority,
            session_id: target_session_id.to_string(),
            catalog: "single".to_string(),
            display_name,
            derivation: existing.derivation,
            reused: true,
        });
    }

    let mut resolved_ranges = Vec::with_capacity(request.sources.len());
    let mut selected_entries = Vec::new();
    let mut selected_ids = HashSet::new();
    for selection in &request.sources {
        if selection.session.authority_id != authority
            || !crate::session_catalog::session_visible_to_profile(
                selection.session.session_id.as_str(),
                &profile_id,
            )
        {
            return Err(DerivationError::forbidden());
        }
        let after = selection.after_entry_seq.unwrap_or(0);
        if after >= selection.through_entry_seq {
            return Err(DerivationError::invalid(
                "each source range must end after its exclusive lower bound",
            ));
        }
        let expected_len = selection
            .through_entry_seq
            .checked_sub(after)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| DerivationError::invalid("source range is too large"))?;
        let source_entries = store.load_transcript_entries(&selection.session.session_id);
        let range = source_entries
            .into_iter()
            .filter(|entry| {
                entry.entry_seq > after && entry.entry_seq <= selection.through_entry_seq
            })
            .collect::<Vec<_>>();
        if range.len() != expected_len
            || range
                .first()
                .is_none_or(|entry| entry.entry_seq != after + 1)
            || range
                .last()
                .is_none_or(|entry| entry.entry_seq != selection.through_entry_seq)
        {
            return Err(DerivationError::invalid(
                "a source range does not resolve to contiguous committed entries",
            ));
        }
        let mut resolved = Vec::with_capacity(range.len());
        for entry in range {
            if !selected_ids.insert(entry.entry_id.clone()) {
                return Err(DerivationError::invalid(
                    "source ranges cannot select the same transcript entry twice",
                ));
            }
            let source = TranscriptEntryRef {
                session: selection.session.clone(),
                entry_id: entry.entry_id.clone(),
                entry_seq: entry.entry_seq,
            };
            resolved.push(TranscriptAppend {
                turn: entry.turn,
                caused_by: entry.caused_by,
                existing_entry_id: Some(entry.entry_id),
                source: Some(source),
                expected_digest: Some(entry.content_digest),
            });
        }
        if selected_entries.len() + resolved.len() > MAX_DERIVATION_ENTRIES {
            return Err(DerivationError::invalid(format!(
                "a derivation may select at most {MAX_DERIVATION_ENTRIES} entries"
            )));
        }
        resolved_ranges.push(ResolvedConversationRange {
            selection: selection.clone(),
            selection_digest: range_digest(&selection.session, &resolved),
        });
        selected_entries.extend(resolved);
    }

    let manifest_id = ContextManifestId::parse(deterministic_id(
        "ctx_",
        b"medousa/context-manifest/v1\0",
        &key_digest,
    ))
    .map_err(|error| DerivationError::internal(error.to_string()))?;
    let derivation_id = DerivationId::parse(deterministic_id(
        "drv_",
        b"medousa/session-derivation/v1\0",
        &key_digest,
    ))
    .map_err(|error| DerivationError::internal(error.to_string()))?;
    let created_at = Utc::now();
    let target_session = SessionRef {
        authority_id: authority.clone(),
        session_id: target_session_id.clone(),
    };
    let manifest = ContextManifest {
        manifest_id,
        sources: resolved_ranges,
        created_by: actor.clone(),
        created_at,
    };
    let derivation = SessionDerivation {
        derivation_id,
        target_session,
        manifest,
        intent: request.intent,
        caused_by: None,
        created_by: actor,
        created_at,
    };
    let outcome: DerivationCommitOutcome = store
        .materialize_derivation(&DerivationCommitRequest {
            derivation,
            idempotency_key_digest: key_digest,
            request_digest,
            entries: selected_entries,
        })
        .await
        .map_err(map_store_error)?;
    let display_name = request.target.display_name;
    publish_catalog(&target_session_id, display_name.clone(), &profile_id);
    Ok(DeriveSessionResponse {
        authority_id: authority,
        session_id: target_session_id.to_string(),
        catalog: "single".to_string(),
        display_name,
        derivation: outcome.derivation,
        reused: outcome.reused,
    })
}
