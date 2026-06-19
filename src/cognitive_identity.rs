//! Cognitive identity memory — load `IdentityContextMode::Cognitive`, compile ranked digests, recall.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use stasis::ports::outbound::memory::identity_memory_models::{
    ContactEntity, GetIdentityContextRequest, IdentityContextMode, RelationshipEntity, UserEntity,
};
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;

use crate::identity_memory::{
    resolve_identity_channel_id, resolve_identity_persona_id,
};
use crate::identity_write_policy::load_identity_product_config;

pub const DEFAULT_RELATIONAL_DIGEST_BUDGET: usize = 800;

#[derive(Debug, Clone)]
pub struct CognitiveIdentitySnapshot {
    pub user_id: String,
    pub user: Option<UserEntity>,
    pub contacts: Vec<ContactEntity>,
    pub relationships: Vec<RelationshipEntity>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DigestCompileOptions {
    pub budget: usize,
    pub max_preferences: usize,
    pub max_people: usize,
    pub pinned_preferences: Vec<String>,
    pub pinned_contact_ids: Vec<String>,
    pub query_hints: Option<String>,
}

impl Default for DigestCompileOptions {
    fn default() -> Self {
        Self::from_product_config(DEFAULT_RELATIONAL_DIGEST_BUDGET)
    }
}

impl DigestCompileOptions {
    pub fn from_product_config(budget: usize) -> Self {
        let config = load_identity_product_config();
        Self {
            budget,
            max_preferences: config.digest_max_preferences,
            max_people: config.digest_max_people,
            pinned_preferences: config.digest_pinned_preferences,
            pinned_contact_ids: Vec::new(),
            query_hints: None,
        }
    }

    pub fn with_query_hints(mut self, hints: impl Into<String>) -> Self {
        let text = hints.into();
        if !text.trim().is_empty() {
            self.query_hints = Some(text);
        }
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct DigestCompileStats {
    pub included_preferences: usize,
    pub included_people: usize,
    pub omitted_preferences: usize,
    pub omitted_people: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RankedDigest {
    pub text: String,
    pub preference_lines: Vec<String>,
    pub people_lines: Vec<String>,
    pub stats: DigestCompileStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityRecallHit {
    pub fact_kind: String,
    pub subject: String,
    pub statement: String,
    pub tags: Vec<String>,
    pub score: f32,
    pub entity_type: String,
    pub entity_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityRecallResult {
    pub query: String,
    pub hits: Vec<IdentityRecallHit>,
    pub total_candidates: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DigestLineKind {
    Preference,
    Person,
}

#[derive(Debug, Clone)]
struct DigestLine {
    kind: DigestLineKind,
    text: String,
    score: f32,
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
    compile_relational_memory_digest_with_options(snapshot, DigestCompileOptions::from_product_config(budget)).text
}

pub fn compile_relational_memory_digest_with_options(
    snapshot: &CognitiveIdentitySnapshot,
    options: DigestCompileOptions,
) -> RankedDigest {
    if let Some(err) = &snapshot.error {
        return RankedDigest {
            text: format!("[MEDOUSA_RELATIONAL_MEMORY]\nstatus=error\nerror={err}"),
            ..RankedDigest::default()
        };
    }

    let query_tokens = tokenize_query(options.query_hints.as_deref());
    let total_preferences = snapshot
        .user
        .as_ref()
        .map(|user| user.preferences.len())
        .unwrap_or(0);
    let total_people = snapshot
        .relationships
        .iter()
        .filter(|relationship| contact_id_for_relationship(relationship).is_some())
        .count();
    let mut lines = collect_scored_digest_lines(snapshot, &options, &query_tokens);

    if lines.is_empty() {
        return RankedDigest {
            text: "[MEDOUSA_RELATIONAL_MEMORY]\nstatus=empty".to_string(),
            stats: DigestCompileStats {
                omitted_preferences: total_preferences,
                omitted_people: total_people,
                ..DigestCompileStats::default()
            },
            ..RankedDigest::default()
        };
    }

    lines.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    while !lines.is_empty() {
        let body = render_digest_body(&lines);
        if body.chars().count() <= options.budget {
            let included_preferences = lines
                .iter()
                .filter(|line| line.kind == DigestLineKind::Preference)
                .count();
            let included_people = lines
                .iter()
                .filter(|line| line.kind == DigestLineKind::Person)
                .count();
            return RankedDigest {
                text: body.clone(),
                preference_lines: lines
                    .iter()
                    .filter(|line| line.kind == DigestLineKind::Preference)
                    .map(|line| line.text.clone())
                    .collect(),
                people_lines: lines
                    .iter()
                    .filter(|line| line.kind == DigestLineKind::Person)
                    .map(|line| line.text.clone())
                    .collect(),
                stats: DigestCompileStats {
                    included_preferences,
                    included_people,
                    omitted_preferences: total_preferences.saturating_sub(included_preferences),
                    omitted_people: total_people.saturating_sub(included_people),
                },
            };
        }

        if let Some(lowest_idx) = lines
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                left.score
                    .partial_cmp(&right.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)
        {
            lines.remove(lowest_idx);
        } else {
            break;
        }
    }

    RankedDigest {
        text: "[MEDOUSA_RELATIONAL_MEMORY]\nstatus=empty".to_string(),
        stats: DigestCompileStats {
            omitted_preferences: total_preferences,
            omitted_people: total_people,
            ..DigestCompileStats::default()
        },
        ..RankedDigest::default()
    }
}

pub fn recall_identity_facts(
    snapshot: &CognitiveIdentitySnapshot,
    query: &str,
    fact_kind: Option<&str>,
    limit: usize,
) -> IdentityRecallResult {
    let normalized_kind = fact_kind
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|raw| !raw.is_empty() && raw != "any");
    let query_tokens = tokenize_query(Some(query));
    let mut hits = Vec::new();

    if normalized_kind.as_deref().is_none_or(|kind| kind == "preference" || kind == "note") {
        if let Some(user) = snapshot.user.as_ref() {
            for (key, value) in &user.preferences {
                let is_note = key.starts_with("note_");
                if normalized_kind.as_deref() == Some("preference") && is_note {
                    continue;
                }
                if normalized_kind.as_deref() == Some("note") && !is_note {
                    continue;
                }
                let value_plain = value_to_plain(value);
                let searchable = format!("{key} {value_plain}");
                let score = score_query_match(&query_tokens, &searchable);
                if score <= 0.0 && !query_tokens.is_empty() {
                    continue;
                }
                let fact = if is_note { "note" } else { "preference" };
                hits.push(IdentityRecallHit {
                    fact_kind: fact.to_string(),
                    subject: key.clone(),
                    statement: value_plain,
                    tags: Vec::new(),
                    score: score.max(0.01),
                    entity_type: "UserEntity".to_string(),
                    entity_id: user.user_id.clone(),
                });
            }
        }
    }

    if normalized_kind.as_deref().is_none_or(|kind| kind == "person") {
        let contact_names = snapshot
            .contacts
            .iter()
            .map(|contact| (contact.contact_id.as_str(), contact.display_name.as_str()))
            .collect::<BTreeMap<_, _>>();

        for relationship in &snapshot.relationships {
            let contact_id = contact_id_for_relationship(relationship);
            let display_name = contact_id
                .and_then(|id| contact_names.get(id).copied())
                .unwrap_or_else(|| contact_id.unwrap_or("unknown"));
            let detail = relationship_social_detail(relationship);
            let searchable = format!(
                "{display_name} {kind} {tags} {detail}",
                kind = relationship.relationship_kind.as_str(),
                tags = relationship.policy_tags.join(" "),
            );
            let mut score = score_query_match(&query_tokens, &searchable);
            if score <= 0.0 {
                score = relationship.recency_score * relationship.confidence;
                if !query_tokens.is_empty() {
                    continue;
                }
            } else {
                score = score.max(relationship.recency_score * relationship.confidence);
            }

            hits.push(IdentityRecallHit {
                fact_kind: "person".to_string(),
                subject: display_name.to_string(),
                statement: detail,
                tags: relationship.policy_tags.clone(),
                score,
                entity_type: relationship
                    .target_entity_ref
                    .entity_type
                    .clone(),
                entity_id: relationship.target_entity_ref.entity_id.clone(),
            });
        }
    }

    let total_candidates = hits.len();
    hits.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit.max(1));

    IdentityRecallResult {
        query: query.to_string(),
        hits,
        total_candidates,
    }
}

pub fn cognitive_identity_diagnostics(snapshot: &CognitiveIdentitySnapshot) -> String {
    cognitive_identity_diagnostics_with_stats(snapshot, None)
}

pub fn cognitive_identity_diagnostics_with_stats(
    snapshot: &CognitiveIdentitySnapshot,
    stats: Option<&DigestCompileStats>,
) -> String {
    let preference_count = snapshot
        .user
        .as_ref()
        .map(|user| user.preferences.len())
        .unwrap_or(0);
    let mut line = format!(
        "mode=cognitive contacts={} preferences={} relationships={}",
        snapshot.contacts.len(),
        preference_count,
        snapshot.relationships.len()
    );
    if let Some(stats) = stats {
        line.push_str(&format!(
            " digest_included_prefs={} digest_included_people={} digest_omitted_prefs={} digest_omitted_people={}",
            stats.included_preferences,
            stats.included_people,
            stats.omitted_preferences,
            stats.omitted_people
        ));
    }
    line
}

fn collect_scored_digest_lines(
    snapshot: &CognitiveIdentitySnapshot,
    options: &DigestCompileOptions,
    query_tokens: &[String],
) -> Vec<DigestLine> {
    let mut lines = Vec::new();

    if let Some(user) = snapshot.user.as_ref() {
        let mut preference_entries: Vec<(String, String, f32)> = user
            .preferences
            .iter()
            .map(|(key, value)| {
                let value_plain = value_to_plain(value);
                let mut score = 1.0_f32;
                if options.pinned_preferences.iter().any(|pin| pin == key) {
                    score += 10.0;
                }
                score += score_query_match(query_tokens, &format!("{key} {value_plain}"));
                (key.clone(), value_plain, score)
            })
            .collect();

        preference_entries.sort_by(|left, right| {
            right
                .2
                .partial_cmp(&left.2)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        preference_entries.truncate(options.max_preferences);

        for (key, value_plain, score) in preference_entries {
            lines.push(DigestLine {
                kind: DigestLineKind::Preference,
                text: format!("{key}={value_plain}"),
                score,
            });
        }
    }

    let contact_names = snapshot
        .contacts
        .iter()
        .map(|contact| (contact.contact_id.as_str(), contact.display_name.as_str()))
        .collect::<BTreeMap<_, _>>();

    let mut people_entries: Vec<(String, f32)> = snapshot
        .relationships
        .iter()
        .filter_map(|relationship| {
            let contact_id = contact_id_for_relationship(relationship)?;
            let display_name = contact_names
                .get(contact_id)
                .copied()
                .unwrap_or(contact_id);
            let kind = relationship.relationship_kind.as_str();
            let detail = relationship_social_detail(relationship);

            let mut score = relationship.recency_score * relationship.confidence;
            if options
                .pinned_contact_ids
                .iter()
                .any(|pin| pin == contact_id)
            {
                score += 10.0;
            }
            score += score_query_match(
                query_tokens,
                &format!("{display_name} {} {detail}", relationship.policy_tags.join(" ")),
            );

            Some((
                format!(
                    "{display_name} — {detail} ({kind}, conf={:.2})",
                    relationship.confidence
                ),
                score,
            ))
        })
        .collect();

    people_entries.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    people_entries.truncate(options.max_people);

    for (text, score) in people_entries {
        lines.push(DigestLine {
            kind: DigestLineKind::Person,
            text,
            score,
        });
    }

    lines
}

fn render_digest_body(lines: &[DigestLine]) -> String {
    let preferences: Vec<_> = lines
        .iter()
        .filter(|line| line.kind == DigestLineKind::Preference)
        .map(|line| line.text.as_str())
        .collect();
    let people: Vec<_> = lines
        .iter()
        .filter(|line| line.kind == DigestLineKind::Person)
        .map(|line| line.text.as_str())
        .collect();

    let mut body = String::from("[MEDOUSA_RELATIONAL_MEMORY]\nstatus=ready");
    if !preferences.is_empty() {
        body.push_str("\npreferences: ");
        body.push_str(&preferences.join("; "));
    }
    for person in people {
        body.push_str("\npeople: ");
        body.push_str(person);
    }
    body
}

fn tokenize_query(query: Option<&str>) -> Vec<String> {
    let Some(raw) = query else {
        return Vec::new();
    };
    raw.to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 2)
        .map(str::to_string)
        .collect()
}

fn score_query_match(tokens: &[String], haystack: &str) -> f32 {
    if tokens.is_empty() {
        return 0.0;
    }
    let haystack = haystack.to_ascii_lowercase();
    let mut score = 0.0_f32;
    for token in tokens {
        if haystack.contains(token) {
            score += 1.0;
        }
    }
    score
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

fn format_policy_tag(tag: &str) -> Option<String> {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((prefix, value)) = trimmed.split_once(':') {
        let value = value.trim();
        if value.is_empty() {
            return Some(trimmed.to_string());
        }
        match prefix.trim().to_ascii_lowercase().as_str() {
            "role" => Some(value.replace('_', " ")),
            "employer" => Some(format!("at {value}")),
            _ => Some(value.to_string()),
        }
    } else {
        Some(trimmed.to_string())
    }
}

fn policy_tags_detail(tags: &[String]) -> Option<String> {
    let parts: Vec<String> = tags.iter().filter_map(|tag| format_policy_tag(tag)).collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

fn relationship_kind_label(kind: &str) -> String {
    if kind == "knows" {
        return "knows".to_string();
    }
    kind.split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_ascii_uppercase().to_string() + chars.as_str()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn relationship_social_detail(relationship: &RelationshipEntity) -> String {
    let kind = relationship.relationship_kind.as_str();
    if let Some(tags) = policy_tags_detail(&relationship.policy_tags) {
        if kind == "knows" {
            return tags;
        }
        return format!("{} · {}", relationship_kind_label(kind), tags);
    }
    if kind == "knows" {
        "no details".to_string()
    } else {
        relationship_kind_label(kind)
    }
}

fn value_to_plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
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
                last_transition_reason: Some("patch_applied".to_string()),
                transition_receipt_id: None,
                version: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            error: None,
        }
    }

    fn snapshot_with_many_relationships() -> CognitiveIdentitySnapshot {
        let mut snapshot = sample_snapshot();
        for idx in 0..20 {
            let recency = (idx as f32) / 20.0;
            snapshot.relationships.push(RelationshipEntity {
                relationship_id: format!("rel:person_{idx}"),
                source_entity_ref: EntityRef {
                    entity_type: "UserEntity".to_string(),
                    entity_id: "user:default".to_string(),
                },
                target_entity_ref: EntityRef {
                    entity_type: "ContactEntity".to_string(),
                    entity_id: format!("contact:person_{idx}"),
                },
                relationship_kind: RelationshipKind::Knows,
                status: RelationshipStatus::Active,
                trust_level: 0.5,
                confidence: 0.5 + recency * 0.4,
                strength_score: 0.5,
                recency_score: recency,
                autonomy_scope: Default::default(),
                approval_profile_id: None,
                interruption_policy: Default::default(),
                escalation_policy: Default::default(),
                policy_tags: vec![format!("idx:{idx}")],
                provenance: UpdateSource::ModelInferred,
                parent_relationship_id: None,
                governing_relationship_ids: vec![],
                derived_from_relationship_id: None,
                last_transition_reason: Some("patch_applied".to_string()),
                transition_receipt_id: None,
                version: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
            snapshot.contacts.push(ContactEntity {
                contact_id: format!("contact:person_{idx}"),
                display_name: format!("Person {idx}"),
                aliases: vec![],
                status: "active".to_string(),
                version: 1,
                updated_at: Utc::now(),
            });
        }
        snapshot
    }

    #[test]
    fn relationship_social_detail_uses_kind_and_tags_not_audit_reason() {
        let relationship = RelationshipEntity {
            relationship_id: "rel:blue".to_string(),
            source_entity_ref: EntityRef {
                entity_type: "UserEntity".to_string(),
                entity_id: "user:default".to_string(),
            },
            target_entity_ref: EntityRef {
                entity_type: "ContactEntity".to_string(),
                entity_id: "contact:blue".to_string(),
            },
            relationship_kind: RelationshipKind::parse("partner"),
            status: RelationshipStatus::Active,
            trust_level: 0.9,
            confidence: 0.95,
            strength_score: 0.9,
            recency_score: 0.9,
            autonomy_scope: Default::default(),
            approval_profile_id: None,
            interruption_policy: Default::default(),
            escalation_policy: Default::default(),
            policy_tags: vec![],
            provenance: UpdateSource::UserDirect,
            parent_relationship_id: None,
            governing_relationship_ids: vec![],
            derived_from_relationship_id: None,
            last_transition_reason: Some("patch_applied".to_string()),
            transition_receipt_id: None,
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert_eq!(relationship_social_detail(&relationship), "Partner");
    }

    #[test]
    fn digest_renders_preferences_and_people() {
        let digest = compile_relational_memory_digest(&sample_snapshot(), 800);
        assert!(digest.contains("status=ready"));
        assert!(digest.contains("beverage=matcha"));
        assert!(digest.contains("Mario"));
        assert!(digest.contains("engineer"));
    }

    #[test]
    fn ranked_digest_prefers_high_recency_relationships() {
        let snapshot = snapshot_with_many_relationships();
        let ranked = compile_relational_memory_digest_with_options(
            &snapshot,
            DigestCompileOptions {
                budget: 800,
                max_preferences: 5,
                max_people: 5,
                pinned_preferences: vec!["beverage".to_string()],
                pinned_contact_ids: vec!["contact:mario".to_string()],
                query_hints: None,
            },
        );

        assert!(ranked.text.contains("Mario"));
        assert!(ranked.text.contains("Person 19"));
        assert!(!ranked.text.contains("Person 0"));
        assert!(ranked.stats.omitted_people > 0);
    }

    #[test]
    fn query_hints_boost_matching_people_in_digest() {
        let snapshot = snapshot_with_many_relationships();
        let ranked = compile_relational_memory_digest_with_options(
            &snapshot,
            DigestCompileOptions {
                budget: 800,
                max_preferences: 5,
                max_people: 5,
                pinned_preferences: Vec::new(),
                pinned_contact_ids: Vec::new(),
                query_hints: Some("what does Mario do at Google".to_string()),
            },
        );

        assert!(ranked.text.contains("Mario"));
    }

    #[test]
    fn recall_finds_person_and_preference_hits() {
        let snapshot = sample_snapshot();
        let mario = recall_identity_facts(&snapshot, "Mario engineer", None, 5);
        assert_eq!(mario.hits.first().map(|hit| hit.subject.as_str()), Some("Mario"));

        let matcha = recall_identity_facts(&snapshot, "matcha", Some("preference"), 5);
        assert!(
            matcha
                .hits
                .iter()
                .any(|hit| hit.statement.contains("matcha"))
        );
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
                last_transition_reason: Some("patch_applied".to_string()),
                transition_receipt_id: None,
                version: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .expect("relationship");

        let store = wrap_in_memory(inner);
        let store_dyn = store as Arc<dyn IdentityMemoryStore>;
        let snapshot = load_cognitive_identity_snapshot(
            Some(&store_dyn),
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
