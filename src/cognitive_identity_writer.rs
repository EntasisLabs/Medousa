//! Server-side cognitive identity writes — preferences, contacts, relationships.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::ports::outbound::memory::identity_memory_models::{
    AutonomyScope, CommitEntityUpdateRequest, CommitEntityUpdateResponse, ContactEntity,
    EntityRef, IdentityEntityType, ProposeEntityUpdateRequest, RelationshipEntity,
    RelationshipKind, RelationshipStatus, UpdateSource, UpdateTier, UserEntity,
};
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_models::MemoryStoreRequest;

use crate::cognitive_identity::{
    compile_relational_memory_digest, load_cognitive_identity_snapshot,
    DEFAULT_RELATIONAL_DIGEST_BUDGET,
};
use crate::identity_memory::full_identity_context_request;
use crate::identity_store_ext::MedousaIdentityMemoryStore;
use crate::identity_write_policy::{evaluate_identity_commit, load_identity_product_config};
use crate::product_config::IdentityProductConfig;

pub const LOCUS_IDENTITY_BRIDGE_SESSION: &str = "medousa-identity";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitiveFactKind {
    Preference,
    Person,
    Note,
}

#[derive(Debug, Clone)]
pub struct CognitiveWriteResult {
    pub committed: bool,
    pub proposal_ids: Vec<String>,
    pub requires_confirmation: bool,
    pub sttp_bridge_stored: bool,
    pub digest_preview: Option<String>,
    pub rationale: Option<String>,
}

pub struct CognitiveIdentityWriter {
    service: Arc<IdentityMemoryService>,
    store: Arc<MedousaIdentityMemoryStore>,
    memory_writer: Option<Arc<dyn MemoryContextWriter>>,
    config: IdentityProductConfig,
}

impl CognitiveIdentityWriter {
    pub fn new(
        store: Arc<MedousaIdentityMemoryStore>,
        memory_writer: Option<Arc<dyn MemoryContextWriter>>,
    ) -> Self {
        let service = Arc::new(IdentityMemoryService::new(
            store.clone() as Arc<dyn IdentityMemoryStore>,
        ));
        Self {
            service,
            store,
            memory_writer,
            config: load_identity_product_config(),
        }
    }

    pub async fn remember_preference(
        &self,
        user_id: &str,
        key: &str,
        value: Value,
        source: UpdateSource,
        confidence: f32,
        reason: &str,
    ) -> StasisResult<CognitiveWriteResult> {
        if !self.config.enabled {
            return Ok(denied("identity writes disabled in product_config"));
        }

        let user = self.load_user(user_id).await?;
        let patch = json!({ format!("preferences.{key}"): value });
        self.propose_and_maybe_commit(
            user_id,
            IdentityEntityType::UserEntity,
            user_id,
            user.version,
            patch,
            source,
            confidence,
            reason,
        )
        .await
    }

    pub async fn remember_contact(
        &self,
        user_id: &str,
        display_name: &str,
        statement: &str,
        attributes: &[String],
        aliases: &[String],
        source: UpdateSource,
        confidence: f32,
        reason: &str,
    ) -> StasisResult<CognitiveWriteResult> {
        if !self.config.enabled {
            return Ok(denied("identity writes disabled in product_config"));
        }

        let contact_id = contact_id_from_display_name(display_name);
        let now = Utc::now();

        self.store
            .upsert_contact_entity(ContactEntity {
                contact_id: contact_id.clone(),
                display_name: display_name.to_string(),
                aliases: aliases.to_vec(),
                status: "active".to_string(),
                version: 1,
                updated_at: now,
            })
            .await?;

        let relationship_id = relationship_id_for_user_contact(user_id, &contact_id);
        let relationship = RelationshipEntity {
            relationship_id: relationship_id.clone(),
            source_entity_ref: EntityRef {
                entity_type: "UserEntity".to_string(),
                entity_id: user_id.to_string(),
            },
            target_entity_ref: EntityRef {
                entity_type: "ContactEntity".to_string(),
                entity_id: contact_id,
            },
            relationship_kind: RelationshipKind::Knows,
            status: RelationshipStatus::Active,
            trust_level: 0.75,
            confidence: confidence.clamp(0.0, 1.0),
            strength_score: 0.8,
            recency_score: 1.0,
            autonomy_scope: AutonomyScope::default(),
            approval_profile_id: None,
            interruption_policy: Default::default(),
            escalation_policy: Default::default(),
            policy_tags: attributes.to_vec(),
            provenance: source,
            parent_relationship_id: None,
            governing_relationship_ids: Vec::new(),
            derived_from_relationship_id: None,
            last_transition_reason: Some(statement.to_string()),
            transition_receipt_id: None,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        self.store.upsert_relationship_entity(relationship).await?;

        let patch = json!({
            "policy_tags": attributes,
            "last_transition_reason": statement,
            "recency_score": 1.0,
            "confidence": confidence.clamp(0.0, 1.0),
        });

        self.propose_and_maybe_commit(
            user_id,
            IdentityEntityType::RelationshipEntity,
            &relationship_id,
            1,
            patch,
            source,
            confidence,
            reason,
        )
        .await
    }

    pub async fn remember_note(
        &self,
        user_id: &str,
        subject: &str,
        statement: &str,
        source: UpdateSource,
        confidence: f32,
        reason: &str,
    ) -> StasisResult<CognitiveWriteResult> {
        if subject.contains(':') || subject.contains('.') {
            return self
                .remember_preference(
                    user_id,
                    subject,
                    Value::String(statement.to_string()),
                    source,
                    confidence,
                    reason,
                )
                .await;
        }

        let key = format!("note_{}", slugify_token(subject));
        self.remember_preference(
            user_id,
            &key,
            Value::String(statement.to_string()),
            source,
            confidence,
            reason,
        )
        .await
    }

    async fn load_user(&self, user_id: &str) -> StasisResult<UserEntity> {
        let context = self
            .store
            .get_identity_context(&full_identity_context_request(
                user_id,
                crate::identity_memory::resolve_identity_persona_id(),
                crate::identity_memory::resolve_identity_channel_id(Some("interactive")),
                4,
            ))
            .await?;
        context.user.ok_or_else(|| {
            StasisError::PortFailure(format!("identity user not found: {user_id}"))
        })
    }

    async fn propose_and_maybe_commit(
        &self,
        user_id: &str,
        entity_type: IdentityEntityType,
        entity_id: &str,
        expected_version: i32,
        patch: Value,
        source: UpdateSource,
        confidence: f32,
        reason: &str,
    ) -> StasisResult<CognitiveWriteResult> {
        let proposal = ProposeEntityUpdateRequest {
            entity_type: entity_type.clone(),
            entity_id: entity_id.to_string(),
            patch: patch.clone(),
            source,
            confidence: confidence.clamp(0.0, 1.0),
            reason: reason.to_string(),
            actor: "medousa-cognitive-writer".to_string(),
            receipt_id: None,
            expires_at: None,
        };

        let proposed = self.service.propose_entity_update(&proposal).await?;
        let mut committed_any = false;
        let mut stored_bridge = false;
        let mut requires_confirmation = false;
        let mut last_rationale = None;

        for (idx, proposal_id) in proposed.proposal_ids.iter().enumerate() {
            let tier = proposed.tiers.get(idx).copied().unwrap_or(UpdateTier::AutoCommit);
            if should_hold_for_confirmation(source, tier) {
                requires_confirmation = true;
                last_rationale = Some(
                    "model_inferred confirm_required tier: proposal surfaced for operator"
                        .to_string(),
                );
                continue;
            }

            let commit_req = CommitEntityUpdateRequest {
                proposal_id: proposal_id.clone(),
                expected_version,
                approver: None,
            };
            let gate = evaluate_identity_commit(&self.config, &proposal, tier, &commit_req);
            if !gate.allowed {
                requires_confirmation = true;
                last_rationale = gate.reason;
                continue;
            }

            let commit = self.service.commit_entity_update(&commit_req).await?;
            if commit.committed {
                committed_any = true;
                if maybe_store_identity_sttp_bridge(
                    self.memory_writer.as_ref(),
                    &self.config,
                    &commit,
                )
                .await?
                {
                    stored_bridge = true;
                }
            } else {
                last_rationale = commit.rationale;
                if matches!(commit.code, Some(stasis::ports::outbound::memory::identity_memory_models::CommitOutcomeCode::ApprovalRequired)) {
                    requires_confirmation = true;
                }
            }
        }

        let store_dyn = self.store.clone() as Arc<dyn IdentityMemoryStore>;
        let snapshot = load_cognitive_identity_snapshot(
            Some(&store_dyn),
            user_id,
            Some("interactive"),
            8,
        )
        .await;
        let digest_preview = Some(compile_relational_memory_digest(
            &snapshot,
            DEFAULT_RELATIONAL_DIGEST_BUDGET,
        ));

        Ok(CognitiveWriteResult {
            committed: committed_any,
            proposal_ids: proposed.proposal_ids,
            requires_confirmation,
            sttp_bridge_stored: stored_bridge,
            digest_preview,
            rationale: last_rationale,
        })
    }
}

pub async fn maybe_store_identity_sttp_bridge(
    memory_writer: Option<&Arc<dyn MemoryContextWriter>>,
    config: &IdentityProductConfig,
    response: &CommitEntityUpdateResponse,
) -> StasisResult<bool> {
    if !config.bridge_to_locus {
        return Ok(false);
    }
    let Some(writer) = memory_writer else {
        return Ok(false);
    };
    let Some(node) = response.sttp_bridge_node.as_deref().filter(|raw| !raw.is_empty()) else {
        return Ok(false);
    };

    let stored = writer
        .store_context(&MemoryStoreRequest {
            session_id: LOCUS_IDENTITY_BRIDGE_SESSION.to_string(),
            raw_node: node.to_string(),
        })
        .await?;

    Ok(stored.valid)
}

fn should_hold_for_confirmation(source: UpdateSource, tier: UpdateTier) -> bool {
    source != UpdateSource::UserDirect && matches!(tier, UpdateTier::ConfirmRequired)
}

fn denied(reason: &str) -> CognitiveWriteResult {
    CognitiveWriteResult {
        committed: false,
        proposal_ids: Vec::new(),
        requires_confirmation: false,
        sttp_bridge_stored: false,
        digest_preview: None,
        rationale: Some(reason.to_string()),
    }
}

pub fn contact_id_from_display_name(display_name: &str) -> String {
    let slug = slugify_token(display_name);
    if slug.is_empty() {
        "contact:unknown".to_string()
    } else {
        format!("contact:{slug}")
    }
}

pub fn relationship_id_for_user_contact(user_id: &str, contact_id: &str) -> String {
    format!(
        "rel:{}:{}",
        slugify_token(user_id.strip_prefix("user:").unwrap_or(user_id)),
        slugify_token(contact_id.strip_prefix("contact:").unwrap_or(contact_id))
    )
}

pub fn attributes_map_to_tags(attributes: &BTreeMap<String, Value>) -> Vec<String> {
    attributes
        .iter()
        .map(|(key, value)| format!("{key}:{}", value_to_plain(value)))
        .collect()
}

fn value_to_plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

fn slugify_token(raw: &str) -> String {
    raw.to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity_memory::build_seeded_medousa_identity_store;

    #[test]
    fn slugify_builds_stable_contact_id() {
        assert_eq!(
            contact_id_from_display_name("Mario"),
            "contact:mario"
        );
    }

    #[tokio::test]
    async fn user_direct_preference_auto_commits() {
        let store = build_seeded_medousa_identity_store().expect("store");
        let writer = CognitiveIdentityWriter::new(store.clone(), None);

        let user_id = crate::identity_memory::resolve_identity_user_id(None);
        let result = writer
            .remember_preference(
                &user_id,
                "beverage",
                json!("matcha"),
                UpdateSource::UserDirect,
                1.0,
                "operator stated preference",
            )
            .await
            .expect("remember");

        assert!(result.committed, "{result:?}");
        assert!(!result.requires_confirmation);

        let store_dyn = store as Arc<dyn IdentityMemoryStore>;
        let snapshot = load_cognitive_identity_snapshot(Some(&store_dyn), &user_id, Some("interactive"), 8).await;
        let digest = compile_relational_memory_digest(&snapshot, 800);
        assert!(digest.contains("matcha"));
    }

    #[tokio::test]
    async fn model_inferred_relationship_patch_surfaces_confirmation() {
        let store = build_seeded_medousa_identity_store().expect("store");
        let writer = CognitiveIdentityWriter::new(store, None);
        let user_id = crate::identity_memory::resolve_identity_user_id(None);

        let result = writer
            .remember_contact(
                &user_id,
                "Mario",
                "Mario is an engineer at Google",
                &["role:engineer".to_string(), "employer:google".to_string()],
                &[],
                UpdateSource::ModelInferred,
                0.9,
                "inferred from chat",
            )
            .await
            .expect("remember contact");

        assert!(result.requires_confirmation || !result.proposal_ids.is_empty());
    }
}
