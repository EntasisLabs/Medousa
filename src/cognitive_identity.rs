//! Cognitive identity memory — load `IdentityContextMode::Cognitive` and compile prompt digests.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use stasis::ports::outbound::memory::identity_memory_models::{
    ContactEntity, GetIdentityContextRequest, IdentityContextMode, RelationshipEntity, UserEntity,
};
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;

use crate::identity_memory::{
    resolve_identity_channel_id, resolve_identity_persona_id,
};

pub const DEFAULT_RELATIONAL_DIGEST_BUDGET: usize = 800;

#[derive(Debug, Clone)]
pub struct CognitiveIdentitySnapshot {
    pub user_id: String,
    pub user: Option<UserEntity>,
    pub contacts: Vec<ContactEntity>,
    pub relationships: Vec<RelationshipEntity>,
    pub error: Option<String>,
}

pub fn build_identity_context_request(
    user_id: impl Into<String>,
    persona_id: impl Into<String>,
    channel_id: impl Into<String>,
    relationship_limit: usize,
    mode: IdentityContextMode,
) -> GetIdentityContextRequest {
    GetIdentityContextRequest {
        user_id: user_id.into(),
        persona_id: persona_id.into(),
        channel_id: channel_id.into(),
        relationship_limit,
        mode,
    }
}

pub async fn load_cognitive_identity_snapshot(
    store: Option<&Arc<dyn IdentityMemoryStore>>,
    user_id: &str,
    policy_profile: Option<&str>,
    relationship_limit: usize,
) -> CognitiveIdentitySnapshot {
    let Some(store) = store else {
        return CognitiveIdentitySnapshot {
            user_id: user_id.to_string(),
            user: None,
            contacts: Vec::new(),
            relationships: Vec::new(),
            error: None,
        };
    };

    let request = build_identity_context_request(
        user_id,
        resolve_identity_persona_id(),
        resolve_identity_channel_id(policy_profile),
        relationship_limit,
        IdentityContextMode::Cognitive,
    );

    match store.get_identity_context(&request).await {
        Ok(context) => CognitiveIdentitySnapshot {
            user_id: user_id.to_string(),
            user: context.user,
            contacts: context.contacts,
            relationships: context.relationships,
            error: None,
        },
        Err(err) => CognitiveIdentitySnapshot {
            user_id: user_id.to_string(),
            user: None,
            contacts: Vec::new(),
            relationships: Vec::new(),
            error: Some(err.to_string()),
        },
    }
}

pub fn compile_relational_memory_digest(
    snapshot: &CognitiveIdentitySnapshot,
    budget: usize,
) -> String {
    if let Some(err) = &snapshot.error {
        return format!("[MEDOUSA_RELATIONAL_MEMORY]\nstatus=error\nerror={err}");
    }

    let preference_line = format_preferences(snapshot.user.as_ref());
    let people_lines = format_people(snapshot);

    if preference_line.is_none() && people_lines.is_empty() {
        return "[MEDOUSA_RELATIONAL_MEMORY]\nstatus=empty".to_string();
    }

    let mut body = String::from("[MEDOUSA_RELATIONAL_MEMORY]\nstatus=ready");
    if let Some(prefs) = preference_line {
        body.push_str("\npreferences: ");
        body.push_str(&prefs);
    }
    for line in people_lines {
        body.push_str("\npeople: ");
        body.push_str(&line);
    }

    truncate_to_budget(&body, budget)
}

pub fn cognitive_identity_diagnostics(snapshot: &CognitiveIdentitySnapshot) -> String {
    let preference_count = snapshot
        .user
        .as_ref()
        .map(|user| user.preferences.len())
        .unwrap_or(0);
    format!(
        "mode=cognitive contacts={} preferences={} relationships={}",
        snapshot.contacts.len(),
        preference_count,
        snapshot.relationships.len()
    )
}

fn format_preferences(user: Option<&UserEntity>) -> Option<String> {
    let user = user?;
    if user.preferences.is_empty() {
        return None;
    }

    let rendered = user
        .preferences
        .iter()
        .map(|(key, value)| format!("{key}={}", value_to_plain(value)))
        .collect::<Vec<_>>()
        .join("; ");
    Some(rendered)
}

fn format_people(snapshot: &CognitiveIdentitySnapshot) -> Vec<String> {
    let contact_names = snapshot
        .contacts
        .iter()
        .map(|contact| (contact.contact_id.as_str(), contact.display_name.as_str()))
        .collect::<BTreeMap<_, _>>();

    let mut lines = Vec::new();
    for relationship in &snapshot.relationships {
        let contact_id = contact_id_for_relationship(relationship);
        let display_name = contact_id
            .and_then(|id| contact_names.get(id).copied())
            .unwrap_or_else(|| contact_id.unwrap_or("unknown"));

        let kind = relationship.relationship_kind.as_str();
        let detail = relationship
            .last_transition_reason
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                if relationship.policy_tags.is_empty() {
                    None
                } else {
                    Some(relationship.policy_tags.join(", "))
                }
            })
            .unwrap_or_else(|| "no details".to_string());

        lines.push(format!(
            "{display_name} — {detail} ({kind}, conf={:.2})",
            relationship.confidence
        ));
    }

    lines.sort();
    lines
}

fn contact_id_for_relationship(relationship: &RelationshipEntity) -> Option<&str> {
    if relationship.target_entity_ref.entity_type == "ContactEntity" {
        return Some(relationship.target_entity_ref.entity_id.as_str());
    }
    if relationship.source_entity_ref.entity_type == "ContactEntity" {
        return Some(relationship.source_entity_ref.entity_id.as_str());
    }
    None
}

fn value_to_plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

fn truncate_to_budget(text: &str, budget: usize) -> String {
    if text.chars().count() <= budget {
        return text.to_string();
    }
    let mut end = budget;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use stasis::infrastructure::memory::in_memory_identity_memory_store::InMemoryIdentityMemoryStore;
    use stasis::ports::outbound::memory::identity_memory_models::{
        EntityRef, RelationshipKind, RelationshipStatus, UpdateSource,
    };

    use super::*;
    use crate::identity_store_ext::wrap_in_memory;

    fn sample_snapshot() -> CognitiveIdentitySnapshot {
        CognitiveIdentitySnapshot {
            user_id: "user:default".to_string(),
            user: Some(UserEntity {
                user_id: "user:default".to_string(),
                timezone: "UTC".to_string(),
                language_variant: None,
                preferences: [("beverage".to_string(), json!("matcha"))]
                    .into_iter()
                    .collect(),
                status: "active".to_string(),
                version: 1,
                updated_at: Utc::now(),
            }),
            contacts: vec![ContactEntity {
                contact_id: "contact:mario".to_string(),
                display_name: "Mario".to_string(),
                aliases: vec![],
                status: "active".to_string(),
                version: 1,
                updated_at: Utc::now(),
            }],
            relationships: vec![RelationshipEntity {
                relationship_id: "rel:mario".to_string(),
                source_entity_ref: EntityRef {
                    entity_type: "UserEntity".to_string(),
                    entity_id: "user:default".to_string(),
                },
                target_entity_ref: EntityRef {
                    entity_type: "ContactEntity".to_string(),
                    entity_id: "contact:mario".to_string(),
                },
                relationship_kind: RelationshipKind::Knows,
                status: RelationshipStatus::Active,
                trust_level: 0.8,
                confidence: 0.86,
                strength_score: 0.8,
                recency_score: 0.8,
                autonomy_scope: Default::default(),
                approval_profile_id: None,
                interruption_policy: Default::default(),
                escalation_policy: Default::default(),
                policy_tags: vec![
                    "role:engineer".to_string(),
                    "employer:google".to_string(),
                ],
                provenance: UpdateSource::UserDirect,
                parent_relationship_id: None,
                governing_relationship_ids: vec![],
                derived_from_relationship_id: None,
                last_transition_reason: Some("Mario is an engineer at Google".to_string()),
                transition_receipt_id: None,
                version: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            error: None,
        }
    }

    #[test]
    fn digest_renders_preferences_and_people() {
        let digest = compile_relational_memory_digest(&sample_snapshot(), 800);
        assert!(digest.contains("status=ready"));
        assert!(digest.contains("beverage=matcha"));
        assert!(digest.contains("Mario"));
        assert!(digest.contains("engineer"));
    }

    #[tokio::test]
    async fn cognitive_snapshot_loads_seeded_store_without_policy_profiles() {
        let store = crate::identity_memory::build_seeded_identity_memory_store().expect("store");
        let snapshot = load_cognitive_identity_snapshot(
            Some(&store),
            &crate::identity_memory::resolve_identity_user_id(None),
            Some("interactive"),
            8,
        )
        .await;

        assert!(snapshot.error.is_none());
        assert!(snapshot.user.is_some());
    }

    #[tokio::test]
    async fn in_memory_cognitive_graph_renders_in_digest() {
        let inner = Arc::new(InMemoryIdentityMemoryStore::default());
        inner
            .upsert_user(UserEntity {
                user_id: "user:default".to_string(),
                timezone: "UTC".to_string(),
                language_variant: None,
                preferences: [("beverage".to_string(), json!("matcha"))]
                    .into_iter()
                    .collect(),
                status: "active".to_string(),
                version: 1,
                updated_at: Utc::now(),
            })
            .expect("user");
        inner
            .upsert_contact(ContactEntity {
                contact_id: "contact:mario".to_string(),
                display_name: "Mario".to_string(),
                aliases: vec![],
                status: "active".to_string(),
                version: 1,
                updated_at: Utc::now(),
            })
            .expect("contact");
        inner
            .upsert_relationship(RelationshipEntity {
                relationship_id: "rel:mario".to_string(),
                source_entity_ref: EntityRef {
                    entity_type: "UserEntity".to_string(),
                    entity_id: "user:default".to_string(),
                },
                target_entity_ref: EntityRef {
                    entity_type: "ContactEntity".to_string(),
                    entity_id: "contact:mario".to_string(),
                },
                relationship_kind: RelationshipKind::Knows,
                status: RelationshipStatus::Active,
                trust_level: 0.8,
                confidence: 0.86,
                strength_score: 0.8,
                recency_score: 0.8,
                autonomy_scope: Default::default(),
                approval_profile_id: None,
                interruption_policy: Default::default(),
                escalation_policy: Default::default(),
                policy_tags: vec!["role:engineer".to_string()],
                provenance: UpdateSource::UserDirect,
                parent_relationship_id: None,
                governing_relationship_ids: vec![],
                derived_from_relationship_id: None,
                last_transition_reason: Some("Mario is an engineer at Google".to_string()),
                transition_receipt_id: None,
                version: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .expect("relationship");

        let store = wrap_in_memory(inner);
        let snapshot = load_cognitive_identity_snapshot(
            Some(&store),
            "user:default",
            Some("interactive"),
            8,
        )
        .await;
        let digest = compile_relational_memory_digest(&snapshot, 800);

        assert!(digest.contains("matcha"));
        assert!(digest.contains("Mario"));
        assert!(!digest.contains("assistant_user"));
    }
}
