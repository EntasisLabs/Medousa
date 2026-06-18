use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, anyhow};
use chrono::Utc;
use serde::Deserialize;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::SurrealValue;
use stasis::infrastructure::memory::in_memory_identity_memory_store::InMemoryIdentityMemoryStore;
use stasis::infrastructure::memory::surreal_identity_memory_store::SurrealIdentityMemoryStore;
use stasis::ports::outbound::memory::identity_memory_models::{
    AutonomyScope, ChannelProfileEntity, EntityRef, EscalationPolicy, GetIdentityContextRequest,
    IdentityContextMode, InterruptionPolicy, PersonaEntity, PolicyProfileEntity,
    RelationshipEntity, RelationshipKind, RelationshipStatus, UpdateSource, UserEntity,
};
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, StasisError};

use crate::engine_context::{EngineExecutionLane, default_policy_profile_for_lane};
use crate::identity_store_ext::{wrap_in_memory, wrap_surreal};
use crate::runtime::surreal_startup::timed_step;

const DEFAULT_PERSONA_ID: &str = "persona:default";
pub const DEFAULT_USER_ID: &str = "user:default";
const DEFAULT_CHANNEL_ID: &str = "channel:default";
const DEFAULT_PERSONA_DISPLAY_NAME: &str = "Medousa";

pub use crate::cognitive_identity::build_identity_context_request;

pub fn policy_identity_context_request(
    user_id: impl Into<String>,
    persona_id: impl Into<String>,
    channel_id: impl Into<String>,
    relationship_limit: usize,
) -> GetIdentityContextRequest {
    build_identity_context_request(
        user_id,
        persona_id,
        channel_id,
        relationship_limit,
        IdentityContextMode::Policy,
    )
}

pub fn cognitive_identity_context_request(
    user_id: impl Into<String>,
    persona_id: impl Into<String>,
    channel_id: impl Into<String>,
    relationship_limit: usize,
) -> GetIdentityContextRequest {
    build_identity_context_request(
        user_id,
        persona_id,
        channel_id,
        relationship_limit,
        IdentityContextMode::Cognitive,
    )
}

pub fn full_identity_context_request(
    user_id: impl Into<String>,
    persona_id: impl Into<String>,
    channel_id: impl Into<String>,
    relationship_limit: usize,
) -> GetIdentityContextRequest {
    build_identity_context_request(
        user_id,
        persona_id,
        channel_id,
        relationship_limit,
        IdentityContextMode::Full,
    )
}

pub fn parse_identity_context_mode_label(raw: Option<&str>) -> IdentityContextMode {
    match raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("policy") => IdentityContextMode::Policy,
        Some("cognitive") => IdentityContextMode::Cognitive,
        Some("full") => IdentityContextMode::Full,
        _ => IdentityContextMode::Full,
    }
}

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

/// Identity principal for cognition tools at runtime assembly time.
///
/// Workshop/daemon surfaces use the canonical operator user (`user:default` or env override).
/// TUI keeps session-scoped identity for power-user isolation.
pub fn resolve_tool_identity_user_id(session_id: &str, workshop_operator: bool) -> String {
    if workshop_operator {
        crate::user_profiles::resolve_workshop_identity_user_id()
    } else {
        resolve_identity_user_id(Some(session_id))
    }
}

pub fn resolve_identity_channel_id(policy_profile: Option<&str>) -> String {
    if let Some(profile) = policy_profile.and_then(trimmed_non_empty) {
        return format!("channel:{}", profile.to_ascii_lowercase());
    }

    resolve_non_empty_env("MEDOUSA_IDENTITY_CHANNEL_ID")
        .or_else(|| resolve_non_empty_env("STASIS_DEFAULT_CHANNEL_ID"))
        .unwrap_or_else(|| DEFAULT_CHANNEL_ID.to_string())
}

pub fn build_seeded_medousa_identity_store() -> Result<Arc<crate::identity_store_ext::MedousaIdentityMemoryStore>> {
    let store = Arc::new(InMemoryIdentityMemoryStore::default());
    seed_baseline_identity_store(store.as_ref())?;
    Ok(wrap_in_memory(store))
}

pub fn build_seeded_identity_memory_store() -> Result<Arc<dyn IdentityMemoryStore>> {
    Ok(build_seeded_medousa_identity_store()? as Arc<dyn IdentityMemoryStore>)
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

fn parse_env_flag(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[derive(Debug, Deserialize, SurrealValue)]
struct IdentityPersonaRow {
    persona_id: String,
}

/// True when `identity_persona` exists (daemon restart on populated DB).
pub async fn surreal_identity_table_exists(db: &Surreal<Any>) -> bool {
    db.query("INFO FOR TABLE identity_persona").await.is_ok()
}

/// True when default persona row is already present — safe to skip startup seed.
pub async fn surreal_identity_baseline_ready(db: &Surreal<Any>) -> bool {
    if !surreal_identity_table_exists(db).await {
        return false;
    }

    let persona_id = resolve_identity_persona_id();
    let Ok(mut response) = db
        .query("SELECT persona_id FROM identity_persona WHERE persona_id = $persona_id LIMIT 1")
        .bind(("persona_id", persona_id))
        .await
    else {
        return false;
    };

    let rows: Vec<IdentityPersonaRow> = response.take(0).unwrap_or_default();
    !rows.is_empty()
}

async fn identity_baseline_needs_seed(db: &Surreal<Any>) -> Result<bool> {
    if parse_env_flag("MEDOUSA_FORCE_IDENTITY_INIT_ON_DAEMON") == Some(true) {
        return Ok(true);
    }
    let ready = timed_step("identity baseline probe", || async {
        Ok(surreal_identity_baseline_ready(db).await)
    })
    .await?;
    Ok(!ready)
}

async fn timed_identity_upsert<F, Fut>(
    label: &str,
    upsert: F,
) -> std::result::Result<(), StasisError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), StasisError>>,
{
    let started = Instant::now();
    eprintln!("medousa-daemon: identity upsert start label={label}");
    let result = upsert().await;
    match &result {
        Ok(()) => eprintln!(
            "medousa-daemon: identity upsert ok label={label} elapsed_ms={}",
            started.elapsed().as_millis()
        ),
        Err(err) => eprintln!(
            "medousa-daemon: identity upsert err label={label} elapsed_ms={} err={err}",
            started.elapsed().as_millis()
        ),
    }
    result
}

pub fn is_identity_user_preferences_decode_error(err: &StasisError) -> bool {
    let message = err.to_string().to_ascii_lowercase();
    message.contains("decode identity user")
        && message.contains("preferences")
        && message.contains("expected object, got none")
}

/// Legacy `identity_user` rows may store `preferences` as Surreal `NONE` instead of `{}`.
pub async fn repair_surreal_identity_user_preferences(db: &Surreal<Any>) -> Result<()> {
    if !surreal_identity_table_exists(db).await {
        return Ok(());
    }

    timed_step("identity user preferences repair", || async {
        db.query("UPDATE identity_user SET preferences = {} WHERE preferences = NONE")
            .await
            .map_err(|err| anyhow::anyhow!("identity user preferences repair: {err}"))
    })
    .await?;

    Ok(())
}

pub async fn repair_surreal_identity_user_preferences_for_id(
    db: &Surreal<Any>,
    user_id: &str,
) -> Result<()> {
    if !surreal_identity_table_exists(db).await {
        return Ok(());
    }

    let user_id = user_id.trim();
    if user_id.is_empty() {
        return Ok(());
    }

    timed_step("identity user preferences repair (single)", || async {
        db.query("UPDATE type::record($table, $id) SET preferences = {}")
            .bind(("table", "identity_user"))
            .bind(("id", user_id.to_string()))
            .await
            .map_err(|err| {
                anyhow::anyhow!("identity user preferences repair for {user_id}: {err}")
            })
    })
    .await?;

    Ok(())
}

/// Build identity store on an already-connected Surreal handle.
///
/// `RuntimeFactory::connect_surreal_any` already ran `ensure_schema_for_db` — do not repeat
/// ~90 DEFINE round-trips here (duplicate schema bootstrap can wedge on remote DDL locks).
pub async fn build_seeded_medousa_identity_store_for_db(
    db: Surreal<Any>,
) -> Result<Arc<crate::identity_store_ext::MedousaIdentityMemoryStore>> {
    let store = Arc::new(SurrealIdentityMemoryStore::new(db.clone()));
    if identity_baseline_needs_seed(&db).await? {
        eprintln!("medousa-daemon: seeding identity baseline (idempotent upserts)…");
        seed_baseline_surreal_identity_store(store.as_ref()).await?;
        eprintln!("medousa-daemon: identity baseline seed complete");
    } else {
        eprintln!("medousa-daemon: identity baseline already present — seed no-op");
    }
    repair_surreal_identity_user_preferences(&db).await?;
    Ok(wrap_surreal(store, db))
}

pub async fn build_seeded_identity_memory_store_for_db(
    db: Surreal<Any>,
) -> Result<Arc<dyn IdentityMemoryStore>> {
    Ok(build_seeded_medousa_identity_store_for_db(db).await? as Arc<dyn IdentityMemoryStore>)
}

/// Build a seeded identity store from an already-open runtime (no second SurrealKV connection).
pub async fn build_seeded_identity_memory_store_for_runtime(
    runtime: &RuntimeComposition,
) -> Result<Arc<dyn IdentityMemoryStore>> {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            build_seeded_identity_memory_store_for_db(rt.job_store.db()).await
        }
        _ => build_seeded_identity_memory_store(),
    }
}

async fn build_seeded_surreal_identity_memory_store(
    backend: &RuntimeBackend,
) -> Result<Arc<dyn IdentityMemoryStore>> {
    let runtime = stasis::prelude::RuntimeFactory::build(backend.clone()).await?;
    let RuntimeComposition::Surreal(rt) = runtime else {
        return build_seeded_identity_memory_store();
    };

    build_seeded_identity_memory_store_for_db(rt.job_store.db()).await
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

    timed_identity_upsert("persona", || store.upsert_persona(PersonaEntity {
            persona_id: persona_id.clone(),
            display_name: resolve_non_empty_env("MEDOUSA_IDENTITY_PERSONA_NAME")
                .unwrap_or_else(|| DEFAULT_PERSONA_DISPLAY_NAME.to_string()),
            status: "active".to_string(),
            version: 1,
            updated_at: now,
        }))
    .await?;

    timed_identity_upsert("user", || store.upsert_user(UserEntity {
            user_id: user_id.clone(),
            timezone: resolve_identity_timezone(),
            language_variant: resolve_non_empty_env("MEDOUSA_IDENTITY_USER_LANGUAGE"),
            preferences: Default::default(),
            status: "active".to_string(),
            version: 1,
            updated_at: now,
        }))
    .await?;

    timed_identity_upsert("policy:interactive", || {
        store.upsert_policy(default_policy(interactive_policy, 2, 0.03, now))
    })
    .await?;
    timed_identity_upsert("policy:scheduled", || {
        store.upsert_policy(default_policy(scheduled_policy, 2, 0.02, now))
    })
    .await?;
    timed_identity_upsert("policy:heartbeat", || {
        store.upsert_policy(default_policy(heartbeat_policy, 1, 0.01, now))
    })
    .await?;

    timed_identity_upsert("channel:default", || {
        store.upsert_channel(default_channel(&default_channel_id, "cli", true, now))
    })
    .await?;
    timed_identity_upsert("channel:interactive", || {
        store.upsert_channel(default_channel(&interactive_channel_id, "tui", true, now))
    })
    .await?;
    timed_identity_upsert("channel:scheduled", || {
        store.upsert_channel(default_channel(&scheduled_channel_id, "api", true, now))
    })
    .await?;
    timed_identity_upsert("channel:heartbeat", || {
        store.upsert_channel(default_channel(&heartbeat_channel_id, "api", true, now))
    })
    .await?;

    timed_identity_upsert("relationship:persona_user", || {
        store.upsert_relationship(default_relationship(
            &format!("rel:{}:{}", stable_id_segment(&persona_id), stable_id_segment(&user_id)),
            entity_ref("PersonaEntity", &persona_id),
            entity_ref("UserEntity", &user_id),
            RelationshipKind::AssistantUser,
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
    })
    .await?;

    timed_identity_upsert("relationship:user_interactive", || {
        store.upsert_relationship(default_relationship(
            &format!(
                "rel:{}:{}",
                stable_id_segment(&user_id),
                stable_id_segment(&interactive_channel_id)
            ),
            entity_ref("UserEntity", &user_id),
            entity_ref("ChannelProfileEntity", &interactive_channel_id),
            RelationshipKind::UserChannel,
            Some(interactive_policy.to_string()),
            AutonomyScope {
                allow: vec!["interactive_reply".to_string()],
                deny: vec!["silent_background_action".to_string()],
                approval_required: vec!["high_impact_action".to_string()],
            },
            now,
        ))
    })
    .await?;

    timed_identity_upsert("relationship:user_scheduled", || {
        store.upsert_relationship(default_relationship(
            &format!(
                "rel:{}:{}",
                stable_id_segment(&user_id),
                stable_id_segment(&scheduled_channel_id)
            ),
            entity_ref("UserEntity", &user_id),
            entity_ref("ChannelProfileEntity", &scheduled_channel_id),
            RelationshipKind::UserChannel,
            Some(scheduled_policy.to_string()),
            AutonomyScope {
                allow: vec!["scheduled_report".to_string(), "scheduled_monitor".to_string()],
                deny: vec!["interactive_interrupt".to_string()],
                approval_required: vec!["external_side_effect".to_string()],
            },
            now,
        ))
    })
    .await?;

    timed_identity_upsert("relationship:user_heartbeat", || {
        store.upsert_relationship(default_relationship(
            &format!(
                "rel:{}:{}",
                stable_id_segment(&user_id),
                stable_id_segment(&heartbeat_channel_id)
            ),
            entity_ref("UserEntity", &user_id),
            entity_ref("ChannelProfileEntity", &heartbeat_channel_id),
            RelationshipKind::UserChannel,
            Some(heartbeat_policy.to_string()),
            AutonomyScope {
                allow: vec!["heartbeat_notify".to_string()],
                deny: vec!["heartbeat_execute_mutation".to_string()],
                approval_required: vec!["heartbeat_escalation".to_string()],
            },
            now,
        ))
    })
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
        preferences: Default::default(),
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
        RelationshipKind::AssistantUser,
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
        RelationshipKind::UserChannel,
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
        RelationshipKind::UserChannel,
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
        RelationshipKind::UserChannel,
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

/// Seed a workshop profile user row and persona/channel edges (shared persona/channels must exist).
pub async fn seed_workshop_profile_user(
    store: &crate::identity_store_ext::MedousaIdentityMemoryStore,
    user_id: &str,
) -> Result<()> {
    let user_id = user_id.trim();
    if user_id.is_empty() {
        anyhow::bail!("profile user_id must not be empty");
    }

    let now = Utc::now();
    let persona_id = resolve_identity_persona_id();
    let interactive_policy = default_policy_profile_for_lane(EngineExecutionLane::Interactive);
    let scheduled_policy = default_policy_profile_for_lane(EngineExecutionLane::Scheduled);
    let heartbeat_policy = default_policy_profile_for_lane(EngineExecutionLane::Heartbeat);
    let interactive_channel_id = resolve_identity_channel_id(Some(interactive_policy));
    let scheduled_channel_id = resolve_identity_channel_id(Some(scheduled_policy));
    let heartbeat_channel_id = resolve_identity_channel_id(Some(heartbeat_policy));

    store
        .upsert_user_entity(UserEntity {
            user_id: user_id.to_string(),
            timezone: resolve_identity_timezone(),
            language_variant: resolve_non_empty_env("MEDOUSA_IDENTITY_USER_LANGUAGE"),
            preferences: Default::default(),
            status: "active".to_string(),
            version: 1,
            updated_at: now,
        })
        .await
        .map_err(|err| anyhow!("seed profile user {user_id}: {err}"))?;

    store
        .upsert_relationship_entity(default_relationship(
            &format!(
                "rel:{}:{}",
                stable_id_segment(&persona_id),
                stable_id_segment(user_id)
            ),
            entity_ref("PersonaEntity", &persona_id),
            entity_ref("UserEntity", user_id),
            RelationshipKind::AssistantUser,
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
        .await
        .map_err(|err| anyhow!("seed profile persona edge for {user_id}: {err}"))?;

    for (channel_id, policy, allow, deny, approval) in [
        (
            interactive_channel_id.as_str(),
            interactive_policy,
            vec!["interactive_reply".to_string()],
            vec!["silent_background_action".to_string()],
            vec!["high_impact_action".to_string()],
        ),
        (
            scheduled_channel_id.as_str(),
            scheduled_policy,
            vec![
                "scheduled_report".to_string(),
                "scheduled_monitor".to_string(),
            ],
            vec!["interactive_interrupt".to_string()],
            vec!["external_side_effect".to_string()],
        ),
        (
            heartbeat_channel_id.as_str(),
            heartbeat_policy,
            vec!["heartbeat_notify".to_string()],
            vec!["heartbeat_execute_mutation".to_string()],
            vec!["heartbeat_escalation".to_string()],
        ),
    ] {
        store
            .upsert_relationship_entity(default_relationship(
                &format!(
                    "rel:{}:{}",
                    stable_id_segment(user_id),
                    stable_id_segment(channel_id)
                ),
                entity_ref("UserEntity", user_id),
                entity_ref("ChannelProfileEntity", channel_id),
                RelationshipKind::UserChannel,
                Some(policy.to_string()),
                AutonomyScope {
                    allow,
                    deny,
                    approval_required: approval,
                },
                now,
            ))
            .await
            .map_err(|err| anyhow!("seed profile channel edge for {user_id}: {err}"))?;
    }

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
    relationship_kind: RelationshipKind,
    approval_profile_id: Option<String>,
    autonomy_scope: AutonomyScope,
    now: chrono::DateTime<Utc>,
) -> RelationshipEntity {
    RelationshipEntity {
        relationship_id: relationship_id.to_string(),
        source_entity_ref,
        target_entity_ref,
        relationship_kind,
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
    use super::{
        build_seeded_identity_memory_store, full_identity_context_request,
        is_identity_user_preferences_decode_error, resolve_identity_channel_id,
        resolve_identity_persona_id, resolve_identity_user_id, resolve_tool_identity_user_id,
        DEFAULT_USER_ID,
    };
    use stasis::prelude::StasisError;

    #[test]
    fn detects_identity_user_preferences_none_decode_error() {
        let err = StasisError::PortFailure(
            "decode identity user: Failed to deserialize field 'preferences' on type 'UserRow': Expected object, got none".to_string(),
        );
        assert!(is_identity_user_preferences_decode_error(&err));
    }

    #[tokio::test]
    async fn seeded_identity_store_returns_context_for_defaults() {
        let store = build_seeded_identity_memory_store().expect("identity store should build");
        let interactive_channel = resolve_identity_channel_id(Some("interactive"));

        let context = store
            .get_identity_context(&full_identity_context_request(
                resolve_identity_user_id(None),
                resolve_identity_persona_id(),
                interactive_channel,
                8,
            ))
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

    #[test]
    fn workshop_operator_identity_ignores_runtime_session_label() {
        let resolved = resolve_tool_identity_user_id("daemon-agent-runtime", true);
        assert_eq!(resolved, DEFAULT_USER_ID);
    }

    #[test]
    fn tui_session_scoped_identity_uses_session_id() {
        let resolved = resolve_tool_identity_user_id("tui-session-abc", false);
        assert_eq!(resolved, "tui-session-abc");
    }
}