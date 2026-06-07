//! Medousa extension for Stasis identity store: Persona/User/Contact commit overlay.
//!
//! Stasis 0.4.0 implements relationship commit natively; this wrapper delegates those commits
//! to the inner store and applies persona/user/contact commits locally.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::infrastructure::memory::in_memory_identity_memory_store::InMemoryIdentityMemoryStore;
use stasis::infrastructure::memory::surreal_identity_memory_store::SurrealIdentityMemoryStore;
use stasis::ports::outbound::memory::identity_memory_models::{
    CommitEntityUpdateRequest, CommitEntityUpdateResponse, CommitOutcomeCode, ContactEntity,
    RelationshipEntity,
    EntityUpdateProposalRecord, GetIdentityContextRequest, GetIdentityContextResponse,
    IdentityEntityType, ListEntityHistoryRequest, ListEntityHistoryResponse, PersonaEntity,
    ProposalState, ProposeEntityUpdateRequest, ProposeEntityUpdateResponse,
    RollbackEntityVersionRequest, RollbackEntityVersionResponse, UpdateSource, UpdateTier,
    UserEntity,
};
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;

use crate::identity_memory::{
    full_identity_context_request, is_identity_user_preferences_decode_error,
    repair_surreal_identity_user_preferences, repair_surreal_identity_user_preferences_for_id,
    resolve_identity_channel_id, resolve_identity_persona_id, resolve_identity_user_id,
};

const PROPOSAL_TABLE: &str = "identity_entity_update_proposal";
const PERSONA_TABLE: &str = "identity_persona";
const USER_TABLE: &str = "identity_user";
const CONTACT_TABLE: &str = "identity_contact";

pub fn wrap_in_memory(store: Arc<InMemoryIdentityMemoryStore>) -> Arc<MedousaIdentityMemoryStore> {
    Arc::new(MedousaIdentityMemoryStore {
        backing: Backing::InMemory {
            store,
            committed_proposals: Mutex::new(HashSet::new()),
            proposal_cache: Mutex::new(HashMap::new()),
        },
    })
}

pub fn wrap_surreal(
    store: Arc<SurrealIdentityMemoryStore>,
    db: Surreal<Any>,
) -> Arc<MedousaIdentityMemoryStore> {
    Arc::new(MedousaIdentityMemoryStore {
        backing: Backing::Surreal {
            store,
            db,
            proposal_cache: Mutex::new(HashMap::new()),
        },
    })
}

enum Backing {
    InMemory {
        store: Arc<InMemoryIdentityMemoryStore>,
        committed_proposals: Mutex<HashSet<String>>,
        proposal_cache: Mutex<HashMap<String, CachedProposal>>,
    },
    Surreal {
        store: Arc<SurrealIdentityMemoryStore>,
        db: Surreal<Any>,
        proposal_cache: Mutex<HashMap<String, CachedProposal>>,
    },
}

#[derive(Clone)]
struct CachedProposal {
    entity_type: IdentityEntityType,
    entity_id: String,
    patch: Value,
    tier: UpdateTier,
    #[allow(dead_code)]
    source: UpdateSource,
}

pub struct MedousaIdentityMemoryStore {
    backing: Backing,
}

impl MedousaIdentityMemoryStore {
    pub async fn upsert_contact_entity(&self, contact: ContactEntity) -> StasisResult<()> {
        match &self.backing {
            Backing::InMemory { store, .. } => store.upsert_contact(contact),
            Backing::Surreal { store, .. } => store.upsert_contact(contact).await,
        }
    }

    pub async fn upsert_relationship_entity(
        &self,
        relationship: RelationshipEntity,
    ) -> StasisResult<()> {
        match &self.backing {
            Backing::InMemory { store, .. } => store.upsert_relationship(relationship),
            Backing::Surreal { store, .. } => store.upsert_relationship(relationship).await,
        }
    }
}

fn port_err(prefix: &str, err: impl std::fmt::Display) -> StasisError {
    StasisError::PortFailure(format!("{prefix}: {err}"))
}

fn is_stasis_unimplemented_commit(response: &CommitEntityUpdateResponse) -> bool {
    !response.committed
        && response
            .rationale
            .as_deref()
            .is_some_and(|r| r.contains("not implemented yet"))
}

fn patch_requires_approval(tier: UpdateTier) -> bool {
    matches!(tier, UpdateTier::ApprovalRequired)
}

fn apply_persona_patch(persona: &mut PersonaEntity, patch: &Value) -> StasisResult<()> {
    let map = patch.as_object().ok_or_else(|| {
        StasisError::PortFailure("identity patch must be an object".to_string())
    })?;
    for (path, value) in map {
        match path.as_str() {
            "display_name" => {
                persona.display_name = value.as_str().ok_or_else(|| {
                    StasisError::PortFailure("display_name must be a string".to_string())
                })?.to_string();
            }
            "status" => {
                persona.status = value.as_str().ok_or_else(|| {
                    StasisError::PortFailure("status must be a string".to_string())
                })?.to_string();
            }
            other => {
                return Err(StasisError::PortFailure(format!(
                    "unsupported persona patch field: {other}"
                )));
            }
        }
    }
    persona.version = persona.version.saturating_add(1);
    persona.updated_at = Utc::now();
    Ok(())
}

fn apply_user_patch(user: &mut UserEntity, patch: &Value) -> StasisResult<()> {
    let map = patch.as_object().ok_or_else(|| {
        StasisError::PortFailure("identity patch must be an object".to_string())
    })?;
    for (path, value) in map {
        if let Some(key) = path.strip_prefix("preferences.") {
            user.preferences.insert(key.to_string(), value.clone());
            continue;
        }
        match path.as_str() {
            "preferences" => {
                let Some(obj) = value.as_object() else {
                    return Err(StasisError::PortFailure(
                        "preferences must be an object".to_string(),
                    ));
                };
                for (key, pref_value) in obj {
                    user.preferences.insert(key.clone(), pref_value.clone());
                }
            }
            "timezone" => {
                user.timezone = value.as_str().ok_or_else(|| {
                    StasisError::PortFailure("timezone must be a string".to_string())
                })?.to_string();
            }
            "language_variant" => {
                user.language_variant = value.as_str().map(|v| v.to_string());
            }
            "status" => {
                user.status = value.as_str().ok_or_else(|| {
                    StasisError::PortFailure("status must be a string".to_string())
                })?.to_string();
            }
            other => {
                return Err(StasisError::PortFailure(format!(
                    "unsupported user patch field: {other}"
                )));
            }
        }
    }
    user.version = user.version.saturating_add(1);
    user.updated_at = Utc::now();
    Ok(())
}

fn parse_string_array(value: &Value, field: &str) -> StasisResult<Vec<String>> {
    let Some(items) = value.as_array() else {
        return Err(StasisError::PortFailure(format!("{field} must be an array")));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(item.as_str().ok_or_else(|| {
            StasisError::PortFailure(format!("{field} entries must be strings"))
        })?.to_string());
    }
    Ok(out)
}

fn apply_contact_patch(contact: &mut ContactEntity, patch: &Value) -> StasisResult<()> {
    let map = patch.as_object().ok_or_else(|| {
        StasisError::PortFailure("identity patch must be an object".to_string())
    })?;
    for (path, value) in map {
        match path.as_str() {
            "display_name" => {
                contact.display_name = value.as_str().ok_or_else(|| {
                    StasisError::PortFailure("display_name must be a string".to_string())
                })?.to_string();
            }
            "aliases" => {
                contact.aliases = parse_string_array(value, "aliases")?;
            }
            "status" => {
                contact.status = value.as_str().ok_or_else(|| {
                    StasisError::PortFailure("status must be a string".to_string())
                })?.to_string();
            }
            other => {
                return Err(StasisError::PortFailure(format!(
                    "unsupported contact patch field: {other}"
                )));
            }
        }
    }
    contact.version = contact.version.saturating_add(1);
    contact.updated_at = Utc::now();
    Ok(())
}

impl MedousaIdentityMemoryStore {
    fn cache_proposals(&self, request: &ProposeEntityUpdateRequest, response: &ProposeEntityUpdateResponse) {
        let cache = match &self.backing {
            Backing::InMemory { proposal_cache, .. } => proposal_cache,
            Backing::Surreal { proposal_cache, .. } => proposal_cache,
        };
        let Ok(mut guard) = cache.lock() else {
            return;
        };
        for (idx, proposal_id) in response.proposal_ids.iter().enumerate() {
            let tier = response.tiers.get(idx).copied().unwrap_or(UpdateTier::AutoCommit);
            guard.insert(
                proposal_id.clone(),
                CachedProposal {
                    entity_type: request.entity_type.clone(),
                    entity_id: request.entity_id.clone(),
                    patch: request.patch.clone(),
                    tier,
                    source: request.source,
                },
            );
        }
    }

    fn cached_proposal(&self, proposal_id: &str) -> Option<CachedProposal> {
        let cache = match &self.backing {
            Backing::InMemory { proposal_cache, .. } => proposal_cache,
            Backing::Surreal { proposal_cache, .. } => proposal_cache,
        };
        cache.lock().ok()?.get(proposal_id).cloned()
    }

    async fn commit_overlay_entity(
        &self,
        request: &CommitEntityUpdateRequest,
        proposal: CachedProposal,
    ) -> StasisResult<CommitEntityUpdateResponse> {
        if patch_requires_approval(proposal.tier) && request.approver.is_none() {
            return Ok(CommitEntityUpdateResponse {
                committed: false,
                code: Some(CommitOutcomeCode::ApprovalRequired),
                rationale: Some("approval required for this proposal tier".to_string()),
                ..Default::default()
            });
        }

        match &self.backing {
            Backing::InMemory {
                store,
                committed_proposals,
                ..
            } => {
                if committed_proposals
                    .lock()
                    .map(|set| set.contains(&request.proposal_id))
                    .unwrap_or(false)
                {
                    return Ok(CommitEntityUpdateResponse {
                        committed: true,
                        code: Some(CommitOutcomeCode::Ok),
                        entity_type: Some(proposal.entity_type.clone()),
                        entity_id: Some(proposal.entity_id.clone()),
                        rationale: Some("proposal already committed".to_string()),
                        ..Default::default()
                    });
                }

                match proposal.entity_type {
                    IdentityEntityType::PersonaEntity => {
                        let ctx = store
                            .get_identity_context(&full_identity_context_request(
                                resolve_identity_user_id(None),
                                proposal.entity_id.clone(),
                                resolve_identity_channel_id(None),
                                1,
                            ))
                            .await?;
                        let mut persona = ctx.persona.ok_or_else(|| {
                            StasisError::PortFailure("target persona not found".to_string())
                        })?;
                        if persona.version != request.expected_version {
                            return Ok(CommitEntityUpdateResponse {
                                committed: false,
                                code: Some(CommitOutcomeCode::StaleState),
                                entity_type: Some(IdentityEntityType::PersonaEntity),
                                entity_id: Some(persona.persona_id.clone()),
                                rationale: Some(format!(
                                    "stale_state expected_version={} current_version={}",
                                    request.expected_version, persona.version
                                )),
                                ..Default::default()
                            });
                        }
                        apply_persona_patch(&mut persona, &proposal.patch)?;
                        let new_version = persona.version;
                        store.upsert_persona(persona)?;
                        if let Ok(mut set) = committed_proposals.lock() {
                            set.insert(request.proposal_id.clone());
                        }
                        Ok(CommitEntityUpdateResponse {
                            committed: true,
                            code: Some(CommitOutcomeCode::Ok),
                            entity_type: Some(IdentityEntityType::PersonaEntity),
                            entity_id: Some(proposal.entity_id),
                            new_version: Some(new_version),
                            ..Default::default()
                        })
                    }
                    IdentityEntityType::UserEntity => {
                        let ctx = store
                            .get_identity_context(&full_identity_context_request(
                                proposal.entity_id.clone(),
                                resolve_identity_persona_id(),
                                resolve_identity_channel_id(None),
                                1,
                            ))
                            .await?;
                        let mut user = ctx.user.ok_or_else(|| {
                            StasisError::PortFailure("target user not found".to_string())
                        })?;
                        if user.version != request.expected_version {
                            return Ok(CommitEntityUpdateResponse {
                                committed: false,
                                code: Some(CommitOutcomeCode::StaleState),
                                entity_type: Some(IdentityEntityType::UserEntity),
                                entity_id: Some(user.user_id.clone()),
                                rationale: Some(format!(
                                    "stale_state expected_version={} current_version={}",
                                    request.expected_version, user.version
                                )),
                                ..Default::default()
                            });
                        }
                        apply_user_patch(&mut user, &proposal.patch)?;
                        let new_version = user.version;
                        store.upsert_user(user)?;
                        if let Ok(mut set) = committed_proposals.lock() {
                            set.insert(request.proposal_id.clone());
                        }
                        Ok(CommitEntityUpdateResponse {
                            committed: true,
                            code: Some(CommitOutcomeCode::Ok),
                            entity_type: Some(IdentityEntityType::UserEntity),
                            entity_id: Some(proposal.entity_id),
                            new_version: Some(new_version),
                            ..Default::default()
                        })
                    }
                    IdentityEntityType::ContactEntity => {
                        let ctx = store
                            .get_identity_context(&full_identity_context_request(
                                resolve_identity_user_id(None),
                                resolve_identity_persona_id(),
                                resolve_identity_channel_id(None),
                                64,
                            ))
                            .await?;
                        let mut contact = ctx
                            .contacts
                            .into_iter()
                            .find(|contact| contact.contact_id == proposal.entity_id)
                            .ok_or_else(|| {
                                StasisError::PortFailure("target contact not found".to_string())
                            })?;
                        if contact.version != request.expected_version {
                            return Ok(CommitEntityUpdateResponse {
                                committed: false,
                                code: Some(CommitOutcomeCode::StaleState),
                                entity_type: Some(IdentityEntityType::ContactEntity),
                                entity_id: Some(contact.contact_id.clone()),
                                rationale: Some(format!(
                                    "stale_state expected_version={} current_version={}",
                                    request.expected_version, contact.version
                                )),
                                ..Default::default()
                            });
                        }
                        apply_contact_patch(&mut contact, &proposal.patch)?;
                        let new_version = contact.version;
                        store.upsert_contact(contact)?;
                        if let Ok(mut set) = committed_proposals.lock() {
                            set.insert(request.proposal_id.clone());
                        }
                        Ok(CommitEntityUpdateResponse {
                            committed: true,
                            code: Some(CommitOutcomeCode::Ok),
                            entity_type: Some(IdentityEntityType::ContactEntity),
                            entity_id: Some(proposal.entity_id),
                            new_version: Some(new_version),
                            ..Default::default()
                        })
                    }
                    _ => Ok(CommitEntityUpdateResponse {
                        committed: false,
                        code: Some(CommitOutcomeCode::InvalidPatch),
                        entity_type: Some(proposal.entity_type),
                        entity_id: Some(proposal.entity_id),
                        rationale: Some(
                            "commit path for this entity type is not implemented yet".to_string(),
                        ),
                        ..Default::default()
                    }),
                }
            }
            Backing::Surreal { store, db, .. } => {
                surreal_commit_overlay_entity(store, db, request, proposal).await
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct PersonaRow {
    persona_id: String,
    display_name: String,
    status: String,
    version: i32,
    updated_at: chrono::DateTime<Utc>,
}

impl From<PersonaEntity> for PersonaRow {
    fn from(value: PersonaEntity) -> Self {
        Self {
            persona_id: value.persona_id,
            display_name: value.display_name,
            status: value.status,
            version: value.version,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct UserRow {
    user_id: String,
    timezone: String,
    language_variant: Option<String>,
    #[serde(default)]
    preferences: std::collections::BTreeMap<String, Value>,
    status: String,
    version: i32,
    updated_at: chrono::DateTime<Utc>,
}

impl From<UserEntity> for UserRow {
    fn from(value: UserEntity) -> Self {
        Self {
            user_id: value.user_id,
            timezone: value.timezone,
            language_variant: value.language_variant,
            preferences: value.preferences,
            status: value.status,
            version: value.version,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct ContactRow {
    contact_id: String,
    display_name: String,
    #[serde(default)]
    aliases: Vec<String>,
    status: String,
    version: i32,
    updated_at: chrono::DateTime<Utc>,
}

impl From<ContactEntity> for ContactRow {
    fn from(value: ContactEntity) -> Self {
        Self {
            contact_id: value.contact_id,
            display_name: value.display_name,
            aliases: value.aliases,
            status: value.status,
            version: value.version,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct ProposalRow {
    proposal_id: String,
    entity_type: String,
    entity_id: String,
    patch_json: String,
    tier: String,
    source: String,
    confidence: f32,
    reason: String,
    state: String,
    approver: Option<String>,
    actor: String,
    receipt_id: Option<String>,
    expires_at: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

fn parse_identity_entity_type(value: &str) -> StasisResult<IdentityEntityType> {
    match value {
        "persona_entity" => Ok(IdentityEntityType::PersonaEntity),
        "user_entity" => Ok(IdentityEntityType::UserEntity),
        "contact_entity" => Ok(IdentityEntityType::ContactEntity),
        "channel_profile_entity" => Ok(IdentityEntityType::ChannelProfileEntity),
        "policy_profile_entity" => Ok(IdentityEntityType::PolicyProfileEntity),
        "relationship_entity" => Ok(IdentityEntityType::RelationshipEntity),
        other => Err(StasisError::PortFailure(format!(
            "invalid identity entity type: {other}"
        ))),
    }
}

fn parse_update_tier(value: &str) -> StasisResult<UpdateTier> {
    match value {
        "auto_commit" => Ok(UpdateTier::AutoCommit),
        "confirm_required" => Ok(UpdateTier::ConfirmRequired),
        "approval_required" => Ok(UpdateTier::ApprovalRequired),
        other => Err(StasisError::PortFailure(format!("invalid update tier: {other}"))),
    }
}

fn parse_proposal_state(value: &str) -> StasisResult<ProposalState> {
    match value {
        "proposed" => Ok(ProposalState::Proposed),
        "committed" => Ok(ProposalState::Committed),
        "rejected" => Ok(ProposalState::Rejected),
        "expired" => Ok(ProposalState::Expired),
        other => Err(StasisError::PortFailure(format!("invalid proposal state: {other}"))),
    }
}

fn parse_update_source(value: &str) -> StasisResult<UpdateSource> {
    match value {
        "user_direct" => Ok(UpdateSource::UserDirect),
        "model_inferred" => Ok(UpdateSource::ModelInferred),
        "system_event" => Ok(UpdateSource::SystemEvent),
        other => Err(StasisError::PortFailure(format!("invalid update source: {other}"))),
    }
}

fn proposal_state_str(value: ProposalState) -> &'static str {
    match value {
        ProposalState::Proposed => "proposed",
        ProposalState::Committed => "committed",
        ProposalState::Rejected => "rejected",
        ProposalState::Expired => "expired",
    }
}

impl TryFrom<ProposalRow> for EntityUpdateProposalRecord {
    type Error = StasisError;

    fn try_from(value: ProposalRow) -> StasisResult<Self> {
        let patch: Value = serde_json::from_str(&value.patch_json)
            .map_err(|e| StasisError::PortFailure(format!("decode proposal patch json: {e}")))?;
        Ok(Self {
            proposal_id: value.proposal_id,
            entity_type: parse_identity_entity_type(&value.entity_type)?,
            entity_id: value.entity_id,
            patch,
            tier: parse_update_tier(&value.tier)?,
            source: parse_update_source(&value.source)?,
            confidence: value.confidence,
            reason: value.reason,
            state: parse_proposal_state(&value.state)?,
            approver: value.approver,
            actor: value.actor,
            receipt_id: value.receipt_id,
            expires_at: value.expires_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

impl From<EntityUpdateProposalRecord> for ProposalRow {
    fn from(value: EntityUpdateProposalRecord) -> Self {
        let patch_json = serde_json::to_string(&value.patch).unwrap_or_else(|_| "{}".to_string());
        Self {
            proposal_id: value.proposal_id,
            entity_type: match value.entity_type {
                IdentityEntityType::PersonaEntity => "persona_entity",
                IdentityEntityType::UserEntity => "user_entity",
                IdentityEntityType::ContactEntity => "contact_entity",
                IdentityEntityType::ChannelProfileEntity => "channel_profile_entity",
                IdentityEntityType::PolicyProfileEntity => "policy_profile_entity",
                IdentityEntityType::RelationshipEntity => "relationship_entity",
            }
            .to_string(),
            entity_id: value.entity_id,
            patch_json,
            tier: match value.tier {
                UpdateTier::AutoCommit => "auto_commit",
                UpdateTier::ConfirmRequired => "confirm_required",
                UpdateTier::ApprovalRequired => "approval_required",
            }
            .to_string(),
            source: match value.source {
                UpdateSource::UserDirect => "user_direct",
                UpdateSource::ModelInferred => "model_inferred",
                UpdateSource::SystemEvent => "system_event",
            }
            .to_string(),
            confidence: value.confidence,
            reason: value.reason,
            state: proposal_state_str(value.state).to_string(),
            approver: value.approver,
            actor: value.actor,
            receipt_id: value.receipt_id,
            expires_at: value.expires_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

async fn load_surreal_proposal(
    db: &Surreal<Any>,
    proposal_id: &str,
) -> StasisResult<Option<EntityUpdateProposalRecord>> {
    let mut proposal_resp = db
        .query("SELECT * FROM type::record($table, $id)")
        .bind(("table", PROPOSAL_TABLE))
        .bind(("id", proposal_id.to_string()))
        .await
        .map_err(|e| port_err("load identity proposal", e))?;

    let proposal_row: Option<ProposalRow> = proposal_resp
        .take(0)
        .map_err(|e| port_err("decode identity proposal", e))?;

    proposal_row.map(EntityUpdateProposalRecord::try_from).transpose()
}

async fn surreal_commit_overlay_entity(
    store: &SurrealIdentityMemoryStore,
    db: &Surreal<Any>,
    request: &CommitEntityUpdateRequest,
    cached: CachedProposal,
) -> StasisResult<CommitEntityUpdateResponse> {
    let Some(mut proposal) = load_surreal_proposal(db, &request.proposal_id).await? else {
        return Ok(CommitEntityUpdateResponse {
            committed: false,
            code: Some(CommitOutcomeCode::NotFound),
            rationale: Some("proposal not found".to_string()),
            ..Default::default()
        });
    };

    if proposal.state != ProposalState::Proposed {
        return Ok(CommitEntityUpdateResponse {
            committed: false,
            code: Some(CommitOutcomeCode::InvalidPatch),
            rationale: Some("proposal is not in proposed state".to_string()),
            ..Default::default()
        });
    }

    if let Some(expires_at) = proposal.expires_at
        && Utc::now() > expires_at
    {
        proposal.state = ProposalState::Expired;
        proposal.updated_at = Utc::now();
        db.query("UPSERT type::record($table, $id) CONTENT $data")
            .bind(("table", PROPOSAL_TABLE))
            .bind(("id", proposal.proposal_id.clone()))
            .bind(("data", ProposalRow::from(proposal)))
            .await
            .map_err(|e| port_err("expire identity proposal", e))?;
        return Ok(CommitEntityUpdateResponse {
            committed: false,
            code: Some(CommitOutcomeCode::ExpiredProposal),
            rationale: Some("proposal expired".to_string()),
            ..Default::default()
        });
    }

    if patch_requires_approval(proposal.tier) && request.approver.is_none() {
        return Ok(CommitEntityUpdateResponse {
            committed: false,
            code: Some(CommitOutcomeCode::ApprovalRequired),
            rationale: Some("approval required for this proposal tier".to_string()),
            ..Default::default()
        });
    }

    let patch = cached.patch;
    match proposal.entity_type {
        IdentityEntityType::PersonaEntity => {
            let mut persona_resp = db
                .query("SELECT * FROM type::record($table, $id)")
                .bind(("table", PERSONA_TABLE))
                .bind(("id", proposal.entity_id.clone()))
                .await
                .map_err(|e| port_err("load persona for proposal", e))?;
            let persona_row: Option<PersonaRow> = persona_resp
                .take(0)
                .map_err(|e| port_err("decode persona for proposal", e))?;
            let Some(persona_row) = persona_row else {
                return Ok(CommitEntityUpdateResponse {
                    committed: false,
                    code: Some(CommitOutcomeCode::NotFound),
                    rationale: Some("target persona not found".to_string()),
                    ..Default::default()
                });
            };
            let mut persona = PersonaEntity {
                persona_id: persona_row.persona_id,
                display_name: persona_row.display_name,
                status: persona_row.status,
                version: persona_row.version,
                updated_at: persona_row.updated_at,
            };
            if persona.version != request.expected_version {
                proposal.state = ProposalState::Rejected;
                proposal.updated_at = Utc::now();
                db.query("UPSERT type::record($table, $id) CONTENT $data")
                    .bind(("table", PROPOSAL_TABLE))
                    .bind(("id", proposal.proposal_id.clone()))
                    .bind(("data", ProposalRow::from(proposal)))
                    .await
                    .map_err(|e| port_err("mark stale proposal", e))?;
                return Ok(CommitEntityUpdateResponse {
                    committed: false,
                    code: Some(CommitOutcomeCode::StaleState),
                    entity_type: Some(IdentityEntityType::PersonaEntity),
                    entity_id: Some(persona.persona_id),
                    rationale: Some(format!(
                        "stale_state expected_version={} current_version={}",
                        request.expected_version, persona.version
                    )),
                    ..Default::default()
                });
            }
            if let Err(err) = apply_persona_patch(&mut persona, &patch) {
                proposal.state = ProposalState::Rejected;
                proposal.updated_at = Utc::now();
                db.query("UPSERT type::record($table, $id) CONTENT $data")
                    .bind(("table", PROPOSAL_TABLE))
                    .bind(("id", proposal.proposal_id.clone()))
                    .bind(("data", ProposalRow::from(proposal)))
                    .await
                    .map_err(|e| port_err("reject invalid proposal", e))?;
                return Ok(CommitEntityUpdateResponse {
                    committed: false,
                    code: Some(CommitOutcomeCode::PolicyDenied),
                    entity_type: Some(IdentityEntityType::PersonaEntity),
                    entity_id: Some(persona.persona_id),
                    rationale: Some(err.to_string()),
                    ..Default::default()
                });
            }
            let new_version = persona.version;
            store.upsert_persona(persona).await?;
            proposal.state = ProposalState::Committed;
            proposal.approver = request.approver.clone();
            proposal.updated_at = Utc::now();
            db.query("UPSERT type::record($table, $id) CONTENT $data")
                .bind(("table", PROPOSAL_TABLE))
                .bind(("id", proposal.proposal_id.clone()))
                .bind(("data", ProposalRow::from(proposal)))
                .await
                .map_err(|e| port_err("mark proposal committed", e))?;
            Ok(CommitEntityUpdateResponse {
                committed: true,
                code: Some(CommitOutcomeCode::Ok),
                entity_type: Some(IdentityEntityType::PersonaEntity),
                entity_id: Some(cached.entity_id),
                new_version: Some(new_version),
                ..Default::default()
            })
        }
        IdentityEntityType::UserEntity => {
            let mut user_resp = db
                .query("SELECT * FROM type::record($table, $id)")
                .bind(("table", USER_TABLE))
                .bind(("id", proposal.entity_id.clone()))
                .await
                .map_err(|e| port_err("load user for proposal", e))?;
            let user_row: Option<UserRow> = user_resp
                .take(0)
                .map_err(|e| port_err("decode user for proposal", e))?;
            let Some(user_row) = user_row else {
                return Ok(CommitEntityUpdateResponse {
                    committed: false,
                    code: Some(CommitOutcomeCode::NotFound),
                    rationale: Some("target user not found".to_string()),
                    ..Default::default()
                });
            };
            let mut user = UserEntity {
                user_id: user_row.user_id,
                timezone: user_row.timezone,
                language_variant: user_row.language_variant,
                preferences: user_row.preferences,
                status: user_row.status,
                version: user_row.version,
                updated_at: user_row.updated_at,
            };
            if user.version != request.expected_version {
                proposal.state = ProposalState::Rejected;
                proposal.updated_at = Utc::now();
                db.query("UPSERT type::record($table, $id) CONTENT $data")
                    .bind(("table", PROPOSAL_TABLE))
                    .bind(("id", proposal.proposal_id.clone()))
                    .bind(("data", ProposalRow::from(proposal)))
                    .await
                    .map_err(|e| port_err("mark stale proposal", e))?;
                return Ok(CommitEntityUpdateResponse {
                    committed: false,
                    code: Some(CommitOutcomeCode::StaleState),
                    entity_type: Some(IdentityEntityType::UserEntity),
                    entity_id: Some(user.user_id),
                    rationale: Some(format!(
                        "stale_state expected_version={} current_version={}",
                        request.expected_version, user.version
                    )),
                    ..Default::default()
                });
            }
            if let Err(err) = apply_user_patch(&mut user, &patch) {
                proposal.state = ProposalState::Rejected;
                proposal.updated_at = Utc::now();
                db.query("UPSERT type::record($table, $id) CONTENT $data")
                    .bind(("table", PROPOSAL_TABLE))
                    .bind(("id", proposal.proposal_id.clone()))
                    .bind(("data", ProposalRow::from(proposal)))
                    .await
                    .map_err(|e| port_err("reject invalid proposal", e))?;
                return Ok(CommitEntityUpdateResponse {
                    committed: false,
                    code: Some(CommitOutcomeCode::PolicyDenied),
                    entity_type: Some(IdentityEntityType::UserEntity),
                    entity_id: Some(user.user_id),
                    rationale: Some(err.to_string()),
                    ..Default::default()
                });
            }
            let new_version = user.version;
            store.upsert_user(user).await?;
            proposal.state = ProposalState::Committed;
            proposal.approver = request.approver.clone();
            proposal.updated_at = Utc::now();
            db.query("UPSERT type::record($table, $id) CONTENT $data")
                .bind(("table", PROPOSAL_TABLE))
                .bind(("id", proposal.proposal_id.clone()))
                .bind(("data", ProposalRow::from(proposal)))
                .await
                .map_err(|e| port_err("mark proposal committed", e))?;
            Ok(CommitEntityUpdateResponse {
                committed: true,
                code: Some(CommitOutcomeCode::Ok),
                entity_type: Some(IdentityEntityType::UserEntity),
                entity_id: Some(cached.entity_id),
                new_version: Some(new_version),
                ..Default::default()
            })
        }
        IdentityEntityType::ContactEntity => {
            let mut contact_resp = db
                .query("SELECT * FROM type::record($table, $id)")
                .bind(("table", CONTACT_TABLE))
                .bind(("id", proposal.entity_id.clone()))
                .await
                .map_err(|e| port_err("load contact for proposal", e))?;
            let contact_row: Option<ContactRow> = contact_resp
                .take(0)
                .map_err(|e| port_err("decode contact for proposal", e))?;
            let Some(contact_row) = contact_row else {
                return Ok(CommitEntityUpdateResponse {
                    committed: false,
                    code: Some(CommitOutcomeCode::NotFound),
                    rationale: Some("target contact not found".to_string()),
                    ..Default::default()
                });
            };
            let mut contact = ContactEntity {
                contact_id: contact_row.contact_id,
                display_name: contact_row.display_name,
                aliases: contact_row.aliases,
                status: contact_row.status,
                version: contact_row.version,
                updated_at: contact_row.updated_at,
            };
            if contact.version != request.expected_version {
                proposal.state = ProposalState::Rejected;
                proposal.updated_at = Utc::now();
                db.query("UPSERT type::record($table, $id) CONTENT $data")
                    .bind(("table", PROPOSAL_TABLE))
                    .bind(("id", proposal.proposal_id.clone()))
                    .bind(("data", ProposalRow::from(proposal)))
                    .await
                    .map_err(|e| port_err("mark stale proposal", e))?;
                return Ok(CommitEntityUpdateResponse {
                    committed: false,
                    code: Some(CommitOutcomeCode::StaleState),
                    entity_type: Some(IdentityEntityType::ContactEntity),
                    entity_id: Some(contact.contact_id),
                    rationale: Some(format!(
                        "stale_state expected_version={} current_version={}",
                        request.expected_version, contact.version
                    )),
                    ..Default::default()
                });
            }
            if let Err(err) = apply_contact_patch(&mut contact, &patch) {
                proposal.state = ProposalState::Rejected;
                proposal.updated_at = Utc::now();
                db.query("UPSERT type::record($table, $id) CONTENT $data")
                    .bind(("table", PROPOSAL_TABLE))
                    .bind(("id", proposal.proposal_id.clone()))
                    .bind(("data", ProposalRow::from(proposal)))
                    .await
                    .map_err(|e| port_err("reject invalid proposal", e))?;
                return Ok(CommitEntityUpdateResponse {
                    committed: false,
                    code: Some(CommitOutcomeCode::PolicyDenied),
                    entity_type: Some(IdentityEntityType::ContactEntity),
                    entity_id: Some(contact.contact_id),
                    rationale: Some(err.to_string()),
                    ..Default::default()
                });
            }
            let new_version = contact.version;
            store.upsert_contact(contact).await?;
            proposal.state = ProposalState::Committed;
            proposal.approver = request.approver.clone();
            proposal.updated_at = Utc::now();
            db.query("UPSERT type::record($table, $id) CONTENT $data")
                .bind(("table", PROPOSAL_TABLE))
                .bind(("id", proposal.proposal_id.clone()))
                .bind(("data", ProposalRow::from(proposal)))
                .await
                .map_err(|e| port_err("mark proposal committed", e))?;
            Ok(CommitEntityUpdateResponse {
                committed: true,
                code: Some(CommitOutcomeCode::Ok),
                entity_type: Some(IdentityEntityType::ContactEntity),
                entity_id: Some(cached.entity_id),
                new_version: Some(new_version),
                ..Default::default()
            })
        }
        _ => Ok(CommitEntityUpdateResponse {
            committed: false,
            code: Some(CommitOutcomeCode::InvalidPatch),
            entity_type: Some(proposal.entity_type),
            entity_id: Some(proposal.entity_id),
            rationale: Some("commit path for this entity type is not implemented yet".to_string()),
            ..Default::default()
        }),
    }
}

#[async_trait]
impl IdentityMemoryStore for MedousaIdentityMemoryStore {
    async fn get_identity_context(
        &self,
        request: &GetIdentityContextRequest,
    ) -> StasisResult<GetIdentityContextResponse> {
        match &self.backing {
            Backing::InMemory { store, .. } => store.get_identity_context(request).await,
            Backing::Surreal { store, db, .. } => {
                match store.get_identity_context(request).await {
                    Ok(response) => Ok(response),
                    Err(err) if is_identity_user_preferences_decode_error(&err) => {
                        if let Err(repair_err) =
                            repair_surreal_identity_user_preferences_for_id(db, &request.user_id)
                                .await
                        {
                            eprintln!(
                                "medousa-daemon: identity user preferences repair failed user_id={} err={repair_err}",
                                request.user_id
                            );
                        } else if let Err(repair_err) =
                            repair_surreal_identity_user_preferences(db).await
                        {
                            eprintln!(
                                "medousa-daemon: identity user preferences bulk repair failed err={repair_err}"
                            );
                        }
                        store.get_identity_context(request).await
                    }
                    Err(err) => Err(err),
                }
            }
        }
    }

    async fn propose_entity_update(
        &self,
        request: &ProposeEntityUpdateRequest,
    ) -> StasisResult<ProposeEntityUpdateResponse> {
        let response = match &self.backing {
            Backing::InMemory { store, .. } => store.propose_entity_update(request).await?,
            Backing::Surreal { store, .. } => store.propose_entity_update(request).await?,
        };
        self.cache_proposals(request, &response);
        Ok(response)
    }

    async fn commit_entity_update(
        &self,
        request: &CommitEntityUpdateRequest,
    ) -> StasisResult<CommitEntityUpdateResponse> {
        let inner = match &self.backing {
            Backing::InMemory { store, .. } => store.commit_entity_update(request).await?,
            Backing::Surreal { store, .. } => store.commit_entity_update(request).await?,
        };

        if inner.committed || !is_stasis_unimplemented_commit(&inner) {
            return Ok(inner);
        }

        let proposal = if let Some(cached) = self.cached_proposal(&request.proposal_id) {
            cached
        } else if let Backing::Surreal { db, .. } = &self.backing {
            let loaded = load_surreal_proposal(db, &request.proposal_id).await?;
            let Some(record) = loaded else {
                return Ok(inner);
            };
            CachedProposal {
                entity_type: record.entity_type,
                entity_id: record.entity_id,
                patch: record.patch,
                tier: record.tier,
                source: record.source,
            }
        } else {
            return Ok(inner);
        };

        match proposal.entity_type {
            IdentityEntityType::PersonaEntity
            | IdentityEntityType::UserEntity
            | IdentityEntityType::ContactEntity => self.commit_overlay_entity(request, proposal).await,
            _ => Ok(inner),
        }
    }

    async fn list_entity_history(
        &self,
        request: &ListEntityHistoryRequest,
    ) -> StasisResult<ListEntityHistoryResponse> {
        match &self.backing {
            Backing::InMemory { store, .. } => store.list_entity_history(request).await,
            Backing::Surreal { store, .. } => store.list_entity_history(request).await,
        }
    }

    async fn rollback_entity_version(
        &self,
        request: &RollbackEntityVersionRequest,
    ) -> StasisResult<RollbackEntityVersionResponse> {
        match &self.backing {
            Backing::InMemory { store, .. } => store.rollback_entity_version(request).await,
            Backing::Surreal { store, .. } => store.rollback_entity_version(request).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stasis::ports::outbound::memory::identity_memory_models::{
        ProposeEntityUpdateRequest, UpdateSource,
    };

    #[tokio::test]
    async fn persona_display_name_commit_on_in_memory_store() {
        let store = Arc::new(InMemoryIdentityMemoryStore::default());
        store
            .upsert_persona(PersonaEntity {
                persona_id: "persona:test".to_string(),
                display_name: "Before".to_string(),
                status: "active".to_string(),
                version: 1,
                updated_at: Utc::now(),
            })
            .expect("seed persona");

        let wrapped = wrap_in_memory(store) as Arc<dyn IdentityMemoryStore>;
        let proposed = wrapped
            .propose_entity_update(&ProposeEntityUpdateRequest {
                entity_type: IdentityEntityType::PersonaEntity,
                entity_id: "persona:test".to_string(),
                patch: json!({ "display_name": "After" }),
                source: UpdateSource::UserDirect,
                confidence: 1.0,
                reason: "test".to_string(),
                actor: "test".to_string(),
                receipt_id: None,
                expires_at: None,
            })
            .await
            .expect("propose");

        let commit = wrapped
            .commit_entity_update(&CommitEntityUpdateRequest {
                proposal_id: proposed.proposal_ids[0].clone(),
                expected_version: 1,
                approver: None,
            })
            .await
            .expect("commit");

        assert!(commit.committed, "{commit:?}");

        let ctx = wrapped
            .get_identity_context(&full_identity_context_request(
                "user:default",
                "persona:test",
                "channel:default",
                1,
            ))
            .await
            .expect("context");
        assert_eq!(
            ctx.persona.as_ref().map(|p| p.display_name.as_str()),
            Some("After")
        );
    }
}
