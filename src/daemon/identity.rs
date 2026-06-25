//! Identity memory and user profile HTTP handlers.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::ports::outbound::memory::identity_memory_models::{
    CommitEntityUpdateRequest, CommitEntityUpdateResponse, GetIdentityContextResponse,
    ListEntityHistoryRequest, ListEntityHistoryResponse, ProposeEntityUpdateRequest,
    ProposeEntityUpdateResponse, RollbackEntityVersionRequest, RollbackEntityVersionResponse,
};
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use crate::identity_memory::{
    build_identity_context_request, full_identity_context_request, parse_identity_context_mode_label,
    resolve_identity_channel_id, resolve_identity_persona_id,
};
use crate::user_profiles::ProfileRecord;
use crate::daemon_api::{
    CreateUserProfileRequest, CreateUserProfileResponse, ExportUserProfileRequest,
    ExportUserProfileResponse, IdentityContextRequest, IdentityDigestPreviewResponse,
    IdentityExportMarkdownRequest, IdentityExportMarkdownResponse, IdentityRememberRequest,
    IdentityRememberResponse, ImportUserProfileRequest, ImportUserProfileResponse,
    ListUserProfilesResponse, SetActiveUserProfileRequest, SetActiveUserProfileResponse,
    UserProfileRecordDto,
};

use crate::daemon::http::internal_error;
use crate::daemon::state::{AgentTurnJobRecord, AppState};

pub(crate) struct ResolvedIdentityContext {
    pub user_id: String,
    pub summary: String,
}
pub async fn resolve_identity_context_for_request(
    state: &AppState,
    user_id_override: Option<&str>,
    persona_id_override: Option<&str>,
    channel_id_override: Option<&str>,
    policy_profile: Option<&str>,
    relationship_limit: usize,
) -> Result<ResolvedIdentityContext, (StatusCode, String)> {
    let user_id = normalize_optional_text(user_id_override)
        .unwrap_or_else(|| state.workshop_identity_user_id());
    let persona_id = normalize_optional_text(persona_id_override)
        .unwrap_or_else(resolve_identity_persona_id);
    let channel_id = normalize_optional_text(channel_id_override)
        .unwrap_or_else(|| resolve_identity_channel_id(policy_profile));

    let response = state
        .identity_service
        .get_identity_context(&full_identity_context_request(
            user_id.clone(),
            persona_id,
            channel_id,
            relationship_limit.clamp(1, 64),
        ))
        .await
        .map_err(internal_error)?;

    Ok(ResolvedIdentityContext {
        user_id,
        summary: summarize_identity_context(&response),
    })
}

fn summarize_identity_context(context: &GetIdentityContextResponse) -> String {
    let continuity_links = context
        .relationships
        .iter()
        .filter(|relationship| relationship.derived_from_relationship_id.is_some())
        .count();
    let continuity_receipts = context
        .relationships
        .iter()
        .filter(|relationship| relationship.transition_receipt_id.is_some())
        .count();
    let preference_count = context
        .user
        .as_ref()
        .map(|user| user.preferences.len())
        .unwrap_or(0);

    format!(
        "persona_present={} user_present={} channel_present={} contacts={} preferences={} relationships={} policies={} depth={} continuity_links={} continuity_receipts={}",
        context.persona.is_some(),
        context.user.is_some(),
        context.channel.is_some(),
        context.contacts.len(),
        preference_count,
        context.relationships.len(),
        context.policy_profiles.len(),
        context.graph_depth_used,
        continuity_links,
        continuity_receipts,
    )
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(ToString::to_string)
}

fn profile_record_to_dto(record: &ProfileRecord) -> UserProfileRecordDto {
    UserProfileRecordDto {
        profile_id: record.profile_id.clone(),
        display_name: record.display_name.clone(),
        created_at: record.created_at,
        is_default: record.is_default,
        archived: record.archived,
    }
}

pub async fn list_user_profiles(
    State(state): State<AppState>,
) -> Result<Json<ListUserProfilesResponse>, (StatusCode, String)> {
    let registry = state
        .profile_registry
        .read()
        .map_err(|_| internal_error("profile registry lock poisoned"))?;
    Ok(Json(ListUserProfilesResponse {
        profiles: registry
            .list_profiles()
            .into_iter()
            .map(|record| profile_record_to_dto(&record))
            .collect(),
        active_profile_id: registry.active_profile_id().to_string(),
        resolved_user_id: registry.resolve_active_user_id(),
    }))
}

pub async fn create_user_profile(
    State(state): State<AppState>,
    Json(request): Json<CreateUserProfileRequest>,
) -> Result<Json<CreateUserProfileResponse>, (StatusCode, String)> {
    let identity_store = state.platform.medousa_identity_store();
    let profile = {
        let mut registry = state
            .profile_registry
            .write()
            .map_err(|_| internal_error("profile registry lock poisoned"))?;
        registry.create_profile(&request.slug, &request.display_name)
    }
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    crate::identity_memory::seed_workshop_profile_user(identity_store.as_ref(), &profile.profile_id)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let registry = state
        .profile_registry
        .read()
        .map_err(|_| internal_error("profile registry lock poisoned"))?;
    let active_profile_id = registry.active_profile_id().to_string();
    let resolved_user_id = registry.resolve_active_user_id();
    Ok(Json(CreateUserProfileResponse {
        profile: profile_record_to_dto(&profile),
        active_profile_id,
        resolved_user_id,
    }))
}

pub async fn set_active_user_profile(
    State(state): State<AppState>,
    Json(request): Json<SetActiveUserProfileRequest>,
) -> Result<Json<SetActiveUserProfileResponse>, (StatusCode, String)> {
    let mut registry = state
        .profile_registry
        .write()
        .map_err(|_| internal_error("profile registry lock poisoned"))?;
    let resolved_user_id = registry
        .set_active_profile(&request.profile_id)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    let active_profile_id = registry.active_profile_id().to_string();
    eprintln!(
        "[medousa] active profile set to {active_profile_id} ({resolved_user_id})"
    );
    Ok(Json(SetActiveUserProfileResponse {
        active_profile_id,
        resolved_user_id,
    }))
}

pub async fn export_user_profile(
    State(state): State<AppState>,
    Json(request): Json<ExportUserProfileRequest>,
) -> Result<Json<ExportUserProfileResponse>, (StatusCode, String)> {
    let registry = state
        .profile_registry
        .read()
        .map_err(|_| internal_error("profile registry lock poisoned"))?
        .clone();
    let identity_store = state.platform.medousa_identity_store();
    let locus_store = state.platform.agent_handle().locus_store.clone();
    let bundle = crate::profile_portability::export_profile_bundle(
        &registry,
        identity_store.as_ref(),
        locus_store,
        &request.profile_id,
        request.session_limit,
        request.node_limit_per_session,
    )
    .await
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(ExportUserProfileResponse { bundle }))
}

pub async fn import_user_profile(
    State(state): State<AppState>,
    Json(request): Json<ImportUserProfileRequest>,
) -> Result<Json<ImportUserProfileResponse>, (StatusCode, String)> {
    let identity_store = state.platform.medousa_identity_store();
    let locus_store = state.platform.agent_handle().locus_store.clone();
    let mut registry = state
        .profile_registry
        .read()
        .map_err(|_| internal_error("profile registry lock poisoned"))?
        .clone();
    let summary = crate::profile_portability::import_profile_bundle(
        &mut registry,
        identity_store.as_ref(),
        locus_store,
        &request.bundle,
        request.dry_run,
    )
    .await
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    if !request.dry_run && summary.created_profile {
        *state
            .profile_registry
            .write()
            .map_err(|_| internal_error("profile registry lock poisoned"))? = registry;
    }
    let message = if summary.dry_run {
        format!(
            "dry-run: would import {} locus nodes across {} sessions for {}",
            summary.locus_nodes_imported, summary.locus_sessions_touched, summary.profile_id
        )
    } else {
        format!(
            "imported {} locus nodes across {} sessions for {}",
            summary.locus_nodes_imported, summary.locus_sessions_touched, summary.profile_id
        )
    };
    Ok(Json(ImportUserProfileResponse {
        dry_run: summary.dry_run,
        profile_id: summary.profile_id,
        created_profile: summary.created_profile,
        identity_user_imported: summary.identity_user_imported,
        contacts_imported: summary.contacts_imported,
        relationships_imported: summary.relationships_imported,
        locus_nodes_imported: summary.locus_nodes_imported,
        locus_sessions_touched: summary.locus_sessions_touched,
        message,
    }))
}

pub async fn identity_get_context(
    State(state): State<AppState>,
    Json(request): Json<IdentityContextRequest>,
) -> Result<Json<GetIdentityContextResponse>, (StatusCode, String)> {
    let user_id = normalize_optional_text(request.user_id.as_deref())
        .unwrap_or_else(|| state.workshop_identity_user_id());
    let persona_id = normalize_optional_text(request.persona_id.as_deref())
        .unwrap_or_else(resolve_identity_persona_id);
    let channel_id = normalize_optional_text(request.channel_id.as_deref())
        .unwrap_or_else(|| resolve_identity_channel_id(request.policy_profile.as_deref()));
    let relationship_limit = request.relationship_limit.unwrap_or(8).clamp(1, 64);
    let mode = parse_identity_context_mode_label(request.mode.as_deref());

    let response = state
        .identity_service
        .get_identity_context(&build_identity_context_request(
            user_id,
            persona_id,
            channel_id,
            relationship_limit,
            mode,
        ))
        .await
        .map_err(internal_error)?;

    Ok(Json(response))
}

pub async fn identity_remember(
    State(state): State<AppState>,
    Json(request): Json<IdentityRememberRequest>,
) -> Result<Json<IdentityRememberResponse>, (StatusCode, String)> {
    let user_id = normalize_optional_text(request.user_id.as_deref())
        .unwrap_or_else(|| state.workshop_identity_user_id());
    let subject = request.subject.trim();
    let statement = request.statement.trim();
    if subject.is_empty() || statement.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "subject and statement are required".to_string(),
        ));
    }

    let source = crate::identity_write_policy::parse_update_source(
        request.source.as_deref().or(Some("user_direct")),
    )
    .map_err(|err| (StatusCode::BAD_REQUEST, err))?;

    let store = state.platform.medousa_identity_store();
    let writer = crate::cognitive_identity_writer::CognitiveIdentityWriter::new(store, None);
    let reason = "home teach medousa";

    let result = match request.fact_kind.trim().to_ascii_lowercase().as_str() {
        "preference" => {
            writer
                .remember_preference(
                    &user_id,
                    subject,
                    serde_json::Value::String(statement.to_string()),
                    source,
                    1.0,
                    reason,
                )
                .await
        }
        "person" => {
            writer
                .remember_contact(
                    &user_id,
                    subject,
                    statement,
                    &request.attributes,
                    &[],
                    source,
                    1.0,
                    reason,
                )
                .await
        }
        "note" => {
            writer
                .remember_note(&user_id, subject, statement, source, 1.0, reason)
                .await
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unsupported fact_kind '{other}', expected preference|person|note"),
            ));
        }
    }
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    let message = if result.committed {
        format!("Remembered {subject}")
    } else if result.requires_confirmation {
        "Saved as a proposal — confirmation may be required".to_string()
    } else {
        "Could not commit this fact".to_string()
    };

    Ok(Json(IdentityRememberResponse {
        committed: result.committed,
        requires_confirmation: result.requires_confirmation,
        proposal_ids: result.proposal_ids,
        digest_preview: result.digest_preview,
        message,
    }))
}

pub async fn identity_digest_preview(
    State(state): State<AppState>,
    Json(request): Json<IdentityContextRequest>,
) -> Result<Json<IdentityDigestPreviewResponse>, (StatusCode, String)> {
    let user_id = normalize_optional_text(request.user_id.as_deref())
        .unwrap_or_else(|| state.workshop_identity_user_id());
    let store = state.platform.medousa_identity_store();
    let relationship_limit = request.relationship_limit.unwrap_or(32).clamp(1, 64);
    let mode = parse_identity_context_mode_label(request.mode.as_deref());

    let context = store
        .get_identity_context(&build_identity_context_request(
            user_id.clone(),
            normalize_optional_text(request.persona_id.as_deref())
                .unwrap_or_else(resolve_identity_persona_id),
            normalize_optional_text(request.channel_id.as_deref())
                .unwrap_or_else(|| resolve_identity_channel_id(request.policy_profile.as_deref())),
            relationship_limit,
            mode,
        ))
        .await
        .map_err(internal_error)?;

    let ranked = crate::identity_markdown::compile_identity_digest_preview(
        store.as_ref(),
        Some(user_id.as_str()),
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(IdentityDigestPreviewResponse {
        digest_text: ranked.text,
        preference_count: context
            .user
            .as_ref()
            .map(|user| user.preferences.len())
            .unwrap_or(0),
        contact_count: context.contacts.len(),
        relationship_count: context.relationships.len(),
        claim_count: context.flattened_claims.len(),
    }))
}

pub async fn identity_export_markdown(
    State(state): State<AppState>,
    Json(request): Json<IdentityExportMarkdownRequest>,
) -> Result<Json<IdentityExportMarkdownResponse>, (StatusCode, String)> {
    let user_id = normalize_optional_text(request.user_id.as_deref())
        .unwrap_or_else(|| state.workshop_identity_user_id());
    let dir = request
        .dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(crate::identity_markdown::identity_markdown_export_dir);

    let store = state.platform.medousa_identity_store();
    let written = crate::identity_markdown::write_identity_markdown_export(
        store.as_ref(),
        Some(user_id.as_str()),
        dir.as_path(),
    )
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(IdentityExportMarkdownResponse {
        export_dir: written.display().to_string(),
        files: vec![
            "SOUL.md".to_string(),
            "USER.md".to_string(),
            "PEOPLE.md".to_string(),
            "IDENTITY.md".to_string(),
        ],
    }))
}

pub async fn identity_propose_update(
    State(state): State<AppState>,
    Json(request): Json<ProposeEntityUpdateRequest>,
) -> Result<Json<ProposeEntityUpdateResponse>, (StatusCode, String)> {
    let response = state
        .identity_service
        .propose_entity_update(&request)
        .await
        .map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn identity_commit_update(
    State(state): State<AppState>,
    Json(request): Json<CommitEntityUpdateRequest>,
) -> Result<Json<CommitEntityUpdateResponse>, (StatusCode, String)> {
    let response = state
        .identity_service
        .commit_entity_update(&request)
        .await
        .map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn identity_list_history(
    State(state): State<AppState>,
    Json(request): Json<ListEntityHistoryRequest>,
) -> Result<Json<ListEntityHistoryResponse>, (StatusCode, String)> {
    let response = state
        .identity_service
        .list_entity_history(&request)
        .await
        .map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn identity_rollback_version(
    State(state): State<AppState>,
    Json(request): Json<RollbackEntityVersionRequest>,
) -> Result<Json<RollbackEntityVersionResponse>, (StatusCode, String)> {
    let response = state
        .identity_service
        .rollback_entity_version(&request)
        .await
        .map_err(internal_error)?;
    Ok(Json(response))
}

