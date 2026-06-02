use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use stasis::application::runtime::runtime_factory::RuntimeFactory;
use stasis::infrastructure::memory::in_memory_identity_memory_store::InMemoryIdentityMemoryStore;
use stasis::infrastructure::memory::surreal_identity_memory_store::SurrealIdentityMemoryStore;
use stasis::ports::outbound::memory::identity_memory_models::{
    AutonomyScope, ChannelProfileEntity, EntityRef, EscalationPolicy, InterruptionPolicy,
    PersonaEntity, PolicyProfileEntity, RelationshipEntity, RelationshipStatus, UpdateSource,
    UserEntity,
};
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, StasisError};

use crate::engine_context::{EngineExecutionLane, default_policy_profile_for_lane};
use crate::identity_store_ext::{wrap_in_memory, wrap_surreal};

const DEFAULT_PERSONA_ID: &str = "persona:default";
const DEFAULT_USER_ID: &str = "user:default";
const DEFAULT_CHANNEL_ID: &str = "channel:default";
const DEFAULT_PERSONA_DISPLAY_NAME: &str = "Medousa";

pub fn resolve_identity_persona_id() -> String {
    resolve_non_empty_env("MEDOUSA_IDENTITY_PERSONA_ID")
        .or_else(|| resolve_non_empty_env("STASIS_DEFAULT_PERSONA_ID"))
        .unwrap_or_else(|| DEFAULT_PERSONA_ID.to_string())
}

pub fn resolve_identity_user_id(explicit: Option<&str>) -> String {
    if let Some(value) = explicit.and_then(trimmed_non_empty) {
        return value.to_string();
    }

    resolve_non_empty_env("MEDOUSA_IDENTITY_USER_ID")
        .or_else(|| resolve_non_empty_env("STASIS_DEFAULT_USER_ID"))
        .unwrap_or_else(|| DEFAULT_USER_ID.to_string())
}

pub fn resolve_identity_channel_id(policy_profile: Option<&str>) -> String {
    if let Some(profile) = policy_profile.and_then(trimmed_non_empty) {
        return format!("channel:{}", profile.to_ascii_lowercase());
    }

    resolve_non_empty_env("MEDOUSA_IDENTITY_CHANNEL_ID")
        .or_else(|| resolve_non_empty_env("STASIS_DEFAULT_CHANNEL_ID"))
        .unwrap_or_else(|| DEFAULT_CHANNEL_ID.to_string())
}

pub fn build_seeded_identity_memory_store() -> Result<Arc<dyn IdentityMemoryStore>> {
    let store = Arc::new(InMemoryIdentityMemoryStore::default());
    seed_baseline_identity_store(store.as_ref())?;
    Ok(wrap_in_memory(store))
}

pub async fn build_identity_memory_store_for_backend(
    backend: &RuntimeBackend,
) -> Result<Arc<dyn IdentityMemoryStore>> {
    match backend {
        RuntimeBackend::SurrealKv { .. } | RuntimeBackend::SurrealWs { .. } => {
            build_seeded_surreal_identity_memory_store(backend).await
        }
        _ => build_seeded_identity_memory_store(),
    }
}

/// Build a seeded identity store from an already-open runtime (no second SurrealKV connection).
pub async fn build_seeded_identity_memory_store_for_runtime(
    runtime: &RuntimeComposition,
) -> Result<Arc<dyn IdentityMemoryStore>> {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            let db = rt.job_store.db();
            let store = Arc::new(SurrealIdentityMemoryStore::new(db.clone()));
            store.ensure_schema().await?;
            seed_baseline_surreal_identity_store(store.as_ref()).await?;
            Ok(wrap_surreal(store, db))
        }
        _ => build_seeded_identity_memory_store(),
    }
}

async fn build_seeded_surreal_identity_memory_store(
    backend: &RuntimeBackend,
) -> Result<Arc<dyn IdentityMemoryStore>> {
    let runtime = RuntimeFactory::build(backend.clone()).await?;
    let RuntimeComposition::Surreal(rt) = runtime else {
        return build_seeded_identity_memory_store();
    };

    let db = rt.job_store.db();
    let store = Arc::new(SurrealIdentityMemoryStore::new(db.clone()));
    store.ensure_schema().await?;
    seed_baseline_surreal_identity_store(store.as_ref()).await?;
    Ok(wrap_surreal(store, db))
}

async fn seed_baseline_surreal_identity_store(
    store: &SurrealIdentityMemoryStore,
) -> std::result::Result<(), StasisError> {
    let now = Utc::now();
    let persona_id = resolve_identity_persona_id();
    let user_id = resolve_identity_user_id(None);

    let interactive_policy = default_policy_profile_for_lane(EngineExecutionLane::Interactive);
    let scheduled_policy = default_policy_profile_for_lane(EngineExecutionLane::Scheduled);
    let heartbeat_policy = default_policy_profile_for_lane(EngineExecutionLane::Heartbeat);

    let interactive_channel_id = resolve_identity_channel_id(Some(interactive_policy));
    let scheduled_channel_id = resolve_identity_channel_id(Some(scheduled_policy));
    let heartbeat_channel_id = resolve_identity_channel_id(Some(heartbeat_policy));
    let default_channel_id = resolve_identity_channel_id(None);

    store
        .upsert_persona(PersonaEntity {
            persona_id: persona_id.clone(),
            display_name: resolve_non_empty_env("MEDOUSA_IDENTITY_PERSONA_NAME")
                .unwrap_or_else(|| DEFAULT_PERSONA_DISPLAY_NAME.to_string()),
            status: "active".to_string(),
            version: 1,
            updated_at: now,
        })
        .await?;

    store
        .upsert_user(UserEntity {
            user_id: user_id.clone(),
            timezone: resolve_identity_timezone(),
            language_variant: resolve_non_empty_env("MEDOUSA_IDENTITY_USER_LANGUAGE"),
            status: "active".to_string(),
            version: 1,
            updated_at: now,
        })
        .await?;

    store
        .upsert_policy(default_policy(interactive_policy, 2, 0.03, now))
        .await?;
    store
        .upsert_policy(default_policy(scheduled_policy, 2, 0.02, now))
        .await?;
    store
        .upsert_policy(default_policy(heartbeat_policy, 1, 0.01, now))
        .await?;

    store
        .upsert_channel(default_channel(&default_channel_id, "cli", true, now))
        .await?;
    store
        .upsert_channel(default_channel(&interactive_channel_id, "tui", true, now))
        .await?;
    store
        .upsert_channel(default_channel(&scheduled_channel_id, "api", true, now))
        .await?;
    store
        .upsert_channel(default_channel(&heartbeat_channel_id, "api", true, now))
        .await?;

    store
        .upsert_relationship(default_relationship(
            &format!("rel:{}:{}", stable_id_segment(&persona_id), stable_id_segment(&user_id)),
            entity_ref("PersonaEntity", &persona_id),
            entity_ref("UserEntity", &user_id),
            "assistant_user",
            Some(interactive_policy.to_string()),
            AutonomyScope {
                allow: vec![
                    "analysis".to_string(),
                    "planning".to_string(),
                    "drafting".to_string(),
                    "external_read".to_string(),
                ],
                deny: vec![
                    "external_posting".to_string(),
                    "financial_transfer".to_string(),
                ],
                approval_required: vec![
                    "system_config_change".to_string(),
                    "destructive_command".to_string(),
                    "external_write".to_string(),
                    "external_side_effect".to_string(),
                ],
            },
            now,
        ))
        .await?;

    store
        .upsert_relationship(default_relationship(
            &format!(
                "rel:{}:{}",
                stable_id_segment(&user_id),
                stable_id_segment(&interactive_channel_id)
            ),
            entity_ref("UserEntity", &user_id),
            entity_ref("ChannelProfileEntity", &interactive_channel_id),
            "user_channel",
            Some(interactive_policy.to_string()),
            AutonomyScope {
                allow: vec!["interactive_reply".to_string()],
                deny: vec!["silent_background_action".to_string()],
                approval_required: vec!["high_impact_action".to_string()],
            },
            now,
        ))
        .await?;

    store
        .upsert_relationship(default_relationship(
            &format!(
                "rel:{}:{}",
                stable_id_segment(&user_id),
                stable_id_segment(&scheduled_channel_id)
            ),
            entity_ref("UserEntity", &user_id),
            entity_ref("ChannelProfileEntity", &scheduled_channel_id),
            "user_channel",
            Some(scheduled_policy.to_string()),
            AutonomyScope {
                allow: vec!["scheduled_report".to_string(), "scheduled_monitor".to_string()],
                deny: vec!["interactive_interrupt".to_string()],
                approval_required: vec!["external_side_effect".to_string()],
            },
            now,
        ))
        .await?;

    store
        .upsert_relationship(default_relationship(
            &format!(
                "rel:{}:{}",
                stable_id_segment(&user_id),
                stable_id_segment(&heartbeat_channel_id)
            ),
            entity_ref("UserEntity", &user_id),
            entity_ref("ChannelProfileEntity", &heartbeat_channel_id),
            "user_channel",
            Some(heartbeat_policy.to_string()),
            AutonomyScope {
                allow: vec!["heartbeat_notify".to_string()],
                deny: vec!["heartbeat_execute_mutation".to_string()],
                approval_required: vec!["heartbeat_escalation".to_string()],
            },
            now,
        ))
        .await?;

    Ok(())
}

fn seed_baseline_identity_store(store: &InMemoryIdentityMemoryStore) -> std::result::Result<(), StasisError> {
    let now = Utc::now();
    let persona_id = resolve_identity_persona_id();
    let user_id = resolve_identity_user_id(None);

    let interactive_policy = default_policy_profile_for_lane(EngineExecutionLane::Interactive);
    let scheduled_policy = default_policy_profile_for_lane(EngineExecutionLane::Scheduled);
    let heartbeat_policy = default_policy_profile_for_lane(EngineExecutionLane::Heartbeat);

    let interactive_channel_id = resolve_identity_channel_id(Some(interactive_policy));
    let scheduled_channel_id = resolve_identity_channel_id(Some(scheduled_policy));
    let heartbeat_channel_id = resolve_identity_channel_id(Some(heartbeat_policy));
    let default_channel_id = resolve_identity_channel_id(None);

    store.upsert_persona(PersonaEntity {
        persona_id: persona_id.clone(),
        display_name: resolve_non_empty_env("MEDOUSA_IDENTITY_PERSONA_NAME")
            .unwrap_or_else(|| DEFAULT_PERSONA_DISPLAY_NAME.to_string()),
        status: "active".to_string(),
        version: 1,
        updated_at: now,
    })?;

    store.upsert_user(UserEntity {
        user_id: user_id.clone(),
        timezone: resolve_identity_timezone(),
        language_variant: resolve_non_empty_env("MEDOUSA_IDENTITY_USER_LANGUAGE"),
        status: "active".to_string(),
        version: 1,
        updated_at: now,
    })?;

    store.upsert_policy(default_policy(interactive_policy, 2, 0.03, now))?;
    store.upsert_policy(default_policy(scheduled_policy, 2, 0.02, now))?;
    store.upsert_policy(default_policy(heartbeat_policy, 1, 0.01, now))?;

    store.upsert_channel(default_channel(&default_channel_id, "cli", true, now))?;
    store.upsert_channel(default_channel(&interactive_channel_id, "tui", true, now))?;
    store.upsert_channel(default_channel(&scheduled_channel_id, "api", true, now))?;
    store.upsert_channel(default_channel(&heartbeat_channel_id, "api", true, now))?;

    store.upsert_relationship(default_relationship(
        &format!("rel:{}:{}", stable_id_segment(&persona_id), stable_id_segment(&user_id)),
        entity_ref("PersonaEntity", &persona_id),
        entity_ref("UserEntity", &user_id),
        "assistant_user",
        Some(interactive_policy.to_string()),
        AutonomyScope {
            allow: vec![
                "analysis".to_string(),
                "planning".to_string(),
                "drafting".to_string(),
                "external_read".to_string(),
            ],
            deny: vec![
                "external_posting".to_string(),
                "financial_transfer".to_string(),
            ],
            approval_required: vec![
                "system_config_change".to_string(),
                "destructive_command".to_string(),
                "external_write".to_string(),
                "external_side_effect".to_string(),
            ],
        },
        now,
    ))?;

    store.upsert_relationship(default_relationship(
        &format!(
            "rel:{}:{}",
            stable_id_segment(&user_id),
            stable_id_segment(&interactive_channel_id)
        ),
        entity_ref("UserEntity", &user_id),
        entity_ref("ChannelProfileEntity", &interactive_channel_id),
        "user_channel",
        Some(interactive_policy.to_string()),
        AutonomyScope {
            allow: vec!["interactive_reply".to_string()],
            deny: vec!["silent_background_action".to_string()],
            approval_required: vec!["high_impact_action".to_string()],
        },
        now,
    ))?;

    store.upsert_relationship(default_relationship(
        &format!(
            "rel:{}:{}",
            stable_id_segment(&user_id),
            stable_id_segment(&scheduled_channel_id)
        ),
        entity_ref("UserEntity", &user_id),
        entity_ref("ChannelProfileEntity", &scheduled_channel_id),
        "user_channel",
        Some(scheduled_policy.to_string()),
        AutonomyScope {
            allow: vec!["scheduled_report".to_string(), "scheduled_monitor".to_string()],
            deny: vec!["interactive_interrupt".to_string()],
            approval_required: vec!["external_side_effect".to_string()],
        },
        now,
    ))?;

    store.upsert_relationship(default_relationship(
        &format!(
            "rel:{}:{}",
            stable_id_segment(&user_id),
            stable_id_segment(&heartbeat_channel_id)
        ),
        entity_ref("UserEntity", &user_id),
        entity_ref("ChannelProfileEntity", &heartbeat_channel_id),
        "user_channel",
        Some(heartbeat_policy.to_string()),
        AutonomyScope {
            allow: vec!["heartbeat_notify".to_string()],
            deny: vec!["heartbeat_execute_mutation".to_string()],
            approval_required: vec!["heartbeat_escalation".to_string()],
        },
        now,
    ))?;

    Ok(())
}

fn stable_id_segment(raw: &str) -> String {
    raw.replace(':', "_").replace('/', "_")
}

fn resolve_identity_timezone() -> String {
    resolve_non_empty_env("MEDOUSA_IDENTITY_USER_TIMEZONE")
        .or_else(|| resolve_non_empty_env("TZ"))
        .unwrap_or_else(|| "UTC".to_string())
}

fn default_policy(
    policy_profile_id: &str,
    graph_max_depth: usize,
    trust_delta_max_per_window: f32,
    now: chrono::DateTime<Utc>,
) -> PolicyProfileEntity {
    PolicyProfileEntity {
        policy_profile_id: policy_profile_id.to_string(),
        graph_max_depth,
        trust_delta_max_per_window,
        status: "active".to_string(),
        version: 1,
        updated_at: now,
    }
}

fn default_channel(
    channel_id: &str,
    channel_type: &str,
    proactive_allowed: bool,
    now: chrono::DateTime<Utc>,
) -> ChannelProfileEntity {
    ChannelProfileEntity {
        channel_id: channel_id.to_string(),
        channel_type: channel_type.to_string(),
        proactive_allowed,
        status: "active".to_string(),
        version: 1,
        updated_at: now,
    }
}

fn entity_ref(entity_type: &str, entity_id: &str) -> EntityRef {
    EntityRef {
        entity_type: entity_type.to_string(),
        entity_id: entity_id.to_string(),
    }
}

fn default_relationship(
    relationship_id: &str,
    source_entity_ref: EntityRef,
    target_entity_ref: EntityRef,
    relationship_kind: &str,
    approval_profile_id: Option<String>,
    autonomy_scope: AutonomyScope,
    now: chrono::DateTime<Utc>,
) -> RelationshipEntity {
    RelationshipEntity {
        relationship_id: relationship_id.to_string(),
        source_entity_ref,
        target_entity_ref,
        relationship_kind: relationship_kind.to_string(),
        status: RelationshipStatus::Active,
        trust_level: 0.78,
        confidence: 0.86,
        strength_score: 0.82,
        recency_score: 0.75,
        autonomy_scope,
        approval_profile_id,
        interruption_policy: InterruptionPolicy {
            quiet_hours: Some("22:00-07:00".to_string()),
            allow_urgent_only: Some(true),
            urgent_threshold: Some(0.75),
        },
        escalation_policy: EscalationPolicy {
            mode: Some("confirm".to_string()),
            fallback: Some("defer".to_string()),
        },
        policy_tags: vec!["medousa-default".to_string()],
        provenance: UpdateSource::SystemEvent,
        parent_relationship_id: None,
        governing_relationship_ids: Vec::new(),
        derived_from_relationship_id: None,
        last_transition_reason: Some("bootstrap_seed".to_string()),
        transition_receipt_id: Some(format!("rcpt:{}", relationship_id)),
        version: 1,
        created_at: now,
        updated_at: now,
    }
}

fn resolve_non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use stasis::ports::outbound::memory::identity_memory_models::GetIdentityContextRequest;

    use super::{
        build_seeded_identity_memory_store, resolve_identity_channel_id,
        resolve_identity_persona_id, resolve_identity_user_id,
    };

    #[tokio::test]
    async fn seeded_identity_store_returns_context_for_defaults() {
        let store = build_seeded_identity_memory_store().expect("identity store should build");
        let interactive_channel = resolve_identity_channel_id(Some("interactive"));

        let context = store
            .get_identity_context(&GetIdentityContextRequest {
                user_id: resolve_identity_user_id(None),
                persona_id: resolve_identity_persona_id(),
                channel_id: interactive_channel,
                relationship_limit: 8,
            })
            .await
            .expect("identity context should resolve");

        assert!(context.persona.is_some());
        assert!(context.user.is_some());
        assert!(context.channel.is_some());
        assert!(!context.relationships.is_empty());
        assert!(!context.policy_profiles.is_empty());
    }

    #[test]
    fn explicit_user_id_wins_over_env_defaults() {
        let resolved = resolve_identity_user_id(Some("user:explicit"));
        assert_eq!(resolved, "user:explicit");
    }
}