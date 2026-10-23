//! Authority for legacy native actions, injected by Tauri rather than supplied
//! by JavaScript. Resolve once so every step of an action uses the same engine.
use crate::active_workshop::ActiveWorkshopTarget;
use crate::pairing_client::WorkshopTransportConfig;
use std::future::Future;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri::Manager;
use tauri::ipc::{CommandArg, CommandItem, InvokeError};
use tauri::{State, Wry};
use tokio::sync::OnceCell;

pub struct WorkshopRequest {
    scope: Option<String>,
    target: OnceCell<Result<ActiveWorkshopTarget, String>>,
    transport: OnceCell<Result<WorkshopTransportConfig, String>>,
}

impl<'de> CommandArg<'de, Wry> for WorkshopRequest {
    fn from_command(command: CommandItem<'de, Wry>) -> Result<Self, InvokeError> {
        let webview = command.message.webview_ref();
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let scope = if webview.label().starts_with("desktop-work-") {
            Some(
                crate::desktop_windows::scope_for(webview.app_handle(), webview.label())
                    .ok_or_else(|| InvokeError::from("The work window was closed"))?,
            )
        } else {
            None
        };
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let scope = None;
        let _ = webview;
        Ok(Self {
            scope,
            target: OnceCell::new(),
            transport: OnceCell::new(),
        })
    }
}

impl WorkshopRequest {
    pub async fn target(&self) -> Result<ActiveWorkshopTarget, String> {
        self.target
            .get_or_init(|| async {
                match &self.scope {
                    Some(scope) => crate::active_workshop::resolve_scope_async(scope.clone()).await,
                    None => crate::active_workshop::resolve_async().await,
                }
            })
            .await
            .clone()
    }

    pub async fn config(&self) -> Result<WorkshopTransportConfig, String> {
        self.transport
            .get_or_init(|| async {
                crate::active_workshop::transport_for_target_async(self.target().await?).await
            })
            .await
            .clone()
    }
}

/// Existing main-only callers remain compatible; migrated commands inject a
/// WorkshopRequest instead of reading the mutable main-window selection.
pub trait WorkshopHttpSource: Sync {
    fn config(&self) -> impl Future<Output = Result<WorkshopTransportConfig, String>> + Send;
}

impl WorkshopHttpSource for WorkshopRequest {
    async fn config(&self) -> Result<WorkshopTransportConfig, String> {
        WorkshopRequest::config(self).await
    }
}

impl WorkshopHttpSource for State<'_, super::DaemonState> {
    async fn config(&self) -> Result<WorkshopTransportConfig, String> {
        crate::active_workshop::transport_for_target_async(
            crate::active_workshop::resolve_async().await?,
        )
        .await
    }
}

/// Only commands migrated to injected workshop authority bypass the legacy guard.
pub fn supports_command(command: &str) -> bool {
    matches!(
        command,
        "browser_host_register_client"
            | "browser_host_resume_session"
            | "runtime_patch_tui_preferences"
            | "chatgpt_oauth_request"
            | "review_provider_auth"
            | "interactive_turn_send"
            | "ssh_targets"
            | "ssh_action"
            | "ssh_set_access"
            | "ssh_remove"
            | "account_connections_probe"
            | "account_chatgpt_begin_device_login"
            | "account_begin_terminal_login"
            | "account_sign_out"
            | "account_cli_install"
            | "agents_list_runtimes"
            | "agents_create_session"
            | "agents_prompt"
            | "agents_set_config_option"
            | "agents_cancel"
            | "agents_list_permission_requests"
            | "agents_approve_permission"
            | "agents_deny_permission"
            | "agents_list_secret_requests"
            | "agents_fulfill_secret_request"
            | "agents_deny_secret_request"
            | "artifact_command"
            | "artifact_fetch"
            | "artifact_list_ui"
            | "artifact_write"
            | "artifact_delete"
            | "bot_list"
            | "bot_create"
            | "bot_get"
            | "bot_update"
            | "bot_set_archived"
            | "bot_duplicate"
            | "bot_open"
            | "session_get_bot"
            | "session_set_bot"
            | "session_clear_bot"
            | "calendar_list_events"
            | "calendar_create_event"
            | "calendar_update_event"
            | "calendar_delete_event"
            | "calendar_import_ics"
            | "calendar_export"
            | "catalog_list_manuscripts"
            | "catalog_list_capabilities"
            | "catalog_get_capability"
            | "catalog_reindex_capabilities"
            | "catalog_get_manuscript"
            | "catalog_create_manuscript"
            | "catalog_update_manuscript"
            | "catalog_import_manuscripts"
            | "component_runtime_append_events"
            | "component_runtime_tail_events"
            | "component_runtime_complete_probe"
            | "component_store_get"
            | "component_store_set"
            | "component_store_delete"
            | "component_store_list_keys"
            | "environment_get_status"
            | "environment_get_spec"
            | "environment_put_spec"
            | "environment_get_pending"
            | "environment_apply_pending"
            | "environment_dismiss_pending"
            | "feed_tail"
            | "feed_latest_good"
            | "identity_get_context"
            | "identity_list_profiles"
            | "identity_create_profile"
            | "identity_set_active_profile"
            | "identity_remember"
            | "identity_digest_preview"
            | "identity_export_markdown"
            | "identity_export_profile"
            | "identity_import_profile"
            | "job_get_result"
            | "job_enqueue_ask"
            | "job_complete_actions"
            | "job_archive_ask"
            | "liquid_state_get"
            | "liquid_state_put"
            | "media_upload"
            | "media_upload_path"
            | "media_read"
            | "media_read_image_path"
            | "model_catalog_list"
            | "model_catalog_lookup"
            | "model_catalog_refresh"
            | "recurring_list"
            | "recurring_register_prompt"
            | "recurring_update"
            | "recurring_delete"
            | "recurring_list_runs"
            | "recurring_get_delivery"
            | "runtime_get_stats"
            | "runtime_get_execution_targets"
            | "runtime_list_workers"
            | "runtime_pair_worker"
            | "runtime_remove_worker"
            | "runtime_rename_worker"
            | "runtime_select_worker"
            | "runtime_get_tui_defaults"
            | "runtime_put_tui_defaults"
            | "migrate_global_tui_defaults_to_engine"
            | "runtime_get_defaults"
            | "runtime_get_worker_config"
            | "runtime_put_worker_config"
            | "runtime_get_delivery_status"
            | "runtime_get_continuation_status"
            | "runtime_config_command"
            | "runtime_stage_route_command"
            | "session_create"
            | "session_derive"
            | "session_list"
            | "session_set_display_name"
            | "agent_mode_list"
            | "agent_mode_transition_policy_get"
            | "agent_mode_transition_policy_set"
            | "session_get_agent_mode"
            | "session_set_agent_mode"
            | "session_list_agent_mode_proposals"
            | "session_decide_agent_mode_proposal"
            | "session_get_code_binding"
            | "session_set_code_binding"
            | "session_clear_code_binding"
            | "session_start_code_project"
            | "session_delete"
            | "prompt_stash_list"
            | "prompt_stash_create"
            | "prompt_stash_delete"
            | "session_get_history"
            | "session_get_active_turn"
            | "session_cancel_active_turn"
            | "session_steer_bound_workshop"
            | "turn_create"
            | "turn_list_session"
            | "composer_stt_status"
            | "composer_stt_transcribe"
            | "tool_history_list_slices"
            | "workflow_from_slice"
            | "turn_budget_approve"
            | "turn_budget_deny"
            | "turn_budget_list"
            | "workspace_get_card"
            | "workspace_cancel_card"
            | "workspace_archive_card"
            | "workspace_retry_card"
            | "workspace_fetch_snapshot"
            | "workflow_list"
            | "workflow_get"
            | "workflow_run"
            | "workflow_plan"
            | "workflow_schedule"
            | "workflow_list_runs"
            | "locus_list_nodes"
            | "locus_list_tags"
            | "locus_get_node"
            | "shared_mode_status"
            | "shared_mode_set"
            | "vault_list_notes"
            | "vault_list_changes"
            | "vault_list_tags"
            | "vault_get_note"
            | "vault_get_file"
            | "vault_save_note"
            | "vault_create_note"
            | "vault_delete_note"
            | "vault_search"
            | "vault_backlinks"
            | "vault_list_roots"
            | "vault_set_active_root"
            | "vault_add_root"
            | "vault_list_trash"
            | "vault_restore_trash"
            | "vault_git_detect"
            | "vault_git_status"
            | "vault_git_enable"
            | "vault_git_init"
            | "vault_git_install"
            | "vault_git_log"
            | "vault_git_commit"
            | "vault_git_restore"
            | "vault_git_diff"
            | "vault_git_worktrees"
    )
}
