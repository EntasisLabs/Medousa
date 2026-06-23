mod autostart;
mod badge;
mod channel_adapters;
mod connection_prefs;
mod daemon_service;
mod external_desk;
mod files;
mod daemon;
mod messaging;
mod medousa_paths;
mod pairing;
mod pairing_client;
mod workshop_registry;
mod workshop_transport;
mod capabilities;
mod mcp_gateway;
mod provider_catalog;
mod providers;
mod tray;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod window;
mod wizard;

use daemon::DaemonState;
use tauri::Manager;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // UIKit otherwise shrinks WKWebView scroll content and exposes window background
    // as a band below fixed bottom UI (matches env(safe-area-inset-bottom) ~34px).
    #[cfg(target_os = "ios")]
    {
        builder = builder.plugin(tauri_plugin_ios_webview_insets::init());
    }

    builder = builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(DaemonState::new())
        .manage(daemon::local_inference::LocalInferenceStreamState::new());

    builder = builder
        .setup(|app| {
            if let Err(err) = workshop_registry::sync_daemon_state_from_registry(
                &app.state::<DaemonState>(),
            ) {
                eprintln!("workshop registry sync: {err}");
            }

            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register_all();
            }

            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            setup_desktop_tray(app)?;

            Ok(())
        });

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        // Quit when the main window closes. Previously we hid to tray (prevent_close),
        // which left medousa-home running invisibly — including when launched from Cursor.
        // The hidden chat-popout window would also keep the process alive if we only
        // destroyed main; exit(0) tears down the whole app. Use tray → Hide to background.
        builder = builder.on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    window.app_handle().exit(0);
                }
            }
        });
    }

    builder
        .invoke_handler(tauri::generate_handler![
            daemon::daemon_url,
            daemon::set_daemon_url,
            daemon::daemon_health,
            daemon_service::daemon_start,
            daemon_service::daemon_restart,
            daemon_service::daemon_wait_healthy,
            connection_prefs::connection_load_prefs,
            connection_prefs::connection_set_public_bind,
            connection_prefs::connection_set_autostart,
            providers::providers_list,
            providers::providers_probe,
            providers::providers_validate_key,
            providers::providers_list_models,
            pairing::pairing_fetch_qr,
            pairing::pairing_rotate_invite,
            pairing::pairing_fetch_qr_image,
            pairing::pairing_fetch_status,
            pairing::pairing_revoke,
            pairing::pairing_wait_ready,
            pairing::pairing_complete_from_qr,
            pairing::pairing_load_credentials,
            pairing::pairing_send_heartbeat,
            pairing::bonjour_status,
            workshop_registry::workshops_load,
            workshop_registry::workshops_set_active,
            workshop_registry::workshops_rename,
            workshop_registry::workshops_remove,
            workshop_registry::workshops_update_client_state,
            workshop_registry::workshops_update_branding,
            mcp_gateway::mcp_gateway_load_config,
            mcp_gateway::mcp_gateway_status,
            mcp_gateway::mcp_gateway_restart,
            mcp_gateway::mcp_gateway_upsert_server,
            mcp_gateway::mcp_gateway_remove_server,
            mcp_gateway::mcp_gateway_set_server_enabled,
            mcp_gateway::mcp_gateway_apply_server,
            capabilities::capabilities_load_overlay,
            capabilities::capabilities_set_binding_enabled,
            capabilities::capabilities_save_web_search,
            daemon::catalog::catalog_reindex_capabilities,
            daemon::workspace_stream_start,
            daemon::workspace_stream_stop,
            daemon::interactive_turn_send,
            daemon::interactive_stream_start,
            daemon::interactive_stream_stop,
            daemon::interactive_stream_stop_turn,
            daemon::vault::vault_list_notes,
            daemon::vault::vault_get_note,
            daemon::vault::vault_save_note,
            daemon::vault::vault_create_note,
            daemon::vault::vault_delete_note,
            daemon::vault::vault_search,
            daemon::vault::vault_backlinks,
            daemon::workspace_card::workspace_get_card,
            daemon::workspace_card::workspace_fetch_snapshot,
            daemon::workspace_card::workspace_cancel_card,
            daemon::workspace_card::workspace_archive_card,
            daemon::workspace_card::workspace_retry_card,
            daemon::turn_budget::turn_budget_approve,
            daemon::turn_budget::turn_budget_deny,
            daemon::turn_budget::turn_budget_list,
            daemon::session::session_list,
            daemon::session::session_set_display_name,
            daemon::session::session_get_history,
            daemon::session::session_get_active_turn,
            daemon::session::session_cancel_active_turn,
            daemon::session::turn_create,
            daemon::session::turn_list_session,
            daemon::media::media_upload,
            daemon::media::media_upload_path,
            daemon::catalog::catalog_list_manuscripts,
            daemon::catalog::catalog_get_manuscript,
            daemon::catalog::catalog_update_manuscript,
            daemon::catalog::catalog_import_manuscripts,
            daemon::catalog::catalog_list_capabilities,
            daemon::catalog::catalog_get_capability,
            daemon::grapheme::grapheme_list_modules,
            daemon::grapheme::grapheme_get_module,
            daemon::grapheme::grapheme_get_module_ops,
            daemon::grapheme::grapheme_list_scripts,
            daemon::grapheme::grapheme_get_script,
            daemon::grapheme::grapheme_run_source,
            daemon::grapheme::grapheme_get_allowlist,
            daemon::grapheme::grapheme_update_allowlist,
            daemon::grapheme::grapheme_save_script,
            daemon::grapheme::grapheme_compile_source,
            daemon::grapheme::grapheme_load_module,
            daemon::grapheme::grapheme_get_lifecycle,
            daemon::grapheme::grapheme_get_lsp_workspace,
            daemon::runtime::runtime_get_stats,
            daemon::runtime::runtime_get_defaults,
            daemon::runtime::runtime_get_delivery_status,
            daemon::runtime::runtime_get_continuation_status,
            daemon::runtime::runtime_config_command,
            daemon::runtime::runtime_stage_route_command,
            daemon::jobs::job_get_result,
            daemon::jobs::job_enqueue_ask,
            daemon::jobs::job_complete_actions,
            daemon::jobs::job_archive_ask,
            daemon::recurring::recurring_list,
            daemon::recurring::recurring_register_prompt,
            daemon::recurring::recurring_update,
            daemon::recurring::recurring_delete,
            daemon::recurring::recurring_list_runs,
            daemon::recurring::recurring_get_delivery,
            daemon::workflow::workflow_list,
            daemon::workflow::workflow_get,
            daemon::workflow::workflow_run,
            daemon::workflow::workflow_plan,
            daemon::workflow::workflow_schedule,
            daemon::workflow::workflow_list_runs,
            daemon::tool_history::tool_history_list_slices,
            daemon::tool_history::workflow_from_slice,
            daemon::identity::identity_get_context,
            daemon::identity::identity_list_profiles,
            daemon::identity::identity_create_profile,
            daemon::identity::identity_set_active_profile,
            daemon::identity::identity_remember,
            daemon::identity::identity_digest_preview,
            daemon::identity::identity_export_markdown,
            daemon::locus::locus_list_nodes,
            daemon::locus::locus_get_node,
            daemon::artifact::artifact_command,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            window::window_show_chat_popout,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            window::window_hide_chat_popout,
            tray::tray_update_blocked_count,
            medousa_paths::medousa_config_paths,
            medousa_paths::connection_runbook_path,
            medousa_paths::load_tui_defaults_summary,
            medousa_paths::load_tui_defaults,
            medousa_paths::persist_tui_defaults,
            medousa_paths::persist_tui_runtime_prefs,
            medousa_paths::persist_tui_favorite_models,
            medousa_paths::persist_tui_voice_prefs,
            messaging::messaging_load_product_config_summary,
            messaging::messaging_save_channel_config,
            messaging::messaging_secret_status,
            messaging::messaging_save_secret,
            messaging::messaging_clear_secret,
            messaging::messaging_read_secret,
            channel_adapters::messaging_sync_adapters,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            external_desk::external_desk_scan_root,
            external_desk::external_desk_read_file,
            files::write_file_bytes,
            wizard::wizard_bootstrap,
            wizard::wizard_begin_rerun,
            wizard::wizard_advance,
            wizard::wizard_apply_screen1,
            wizard::wizard_complete,
            daemon::local_inference::local_inference_hardware,
            daemon::local_inference::local_inference_catalog,
            daemon::local_inference::local_inference_models,
            daemon::local_inference::local_inference_start_download,
            daemon::local_inference::local_inference_download_status,
            daemon::local_inference::local_inference_load_engine,
            daemon::local_inference::local_inference_engine_status,
            daemon::local_inference::local_inference_remove_model,
            daemon::local_inference::local_inference_stream_download,
            daemon::local_inference::local_inference_stream_download_stop,
            daemon::model_catalog::model_catalog_list,
            daemon::model_catalog::model_catalog_lookup,
            daemon::model_catalog::model_catalog_refresh,
            daemon::stt::composer_stt_status,
            daemon::stt::composer_stt_transcribe,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn setup_desktop_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Medousa", true, None::<&str>)?;
    let chat = MenuItem::with_id(app, "chat", "Open Chat", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &chat, &hide, &quit])?;

    if let Some(icon) = app.default_window_icon().cloned() {
        TrayIconBuilder::with_id("main-tray")
            .icon(icon)
            .menu(&menu)
            .tooltip("Medousa")
            .on_menu_event(|app, event| match event.id.as_ref() {
                "show" => show_main_window(app),
                "chat" => {
                    let _ = window::window_show_chat_popout(app.clone());
                }
                "hide" => hide_main_window(app),
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let app = tray.app_handle();
                    show_main_window(&app);
                }
            })
            .build(app)?;
    }

    Ok(())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn hide_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}
