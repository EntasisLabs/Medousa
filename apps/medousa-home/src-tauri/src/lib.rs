mod workshop_runtime;
mod paths;
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
mod packages;
mod pairing;
mod pairing_client;
mod push;
mod workshop_registry;
mod workshop_transport;
mod capabilities;
mod mcp_gateway;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod browser_host;
#[cfg(not(target_os = "ios"))]
mod human_browser;
#[cfg(any(target_os = "ios", target_os = "android"))]
mod browser_host_mobile;
#[cfg(target_os = "android")]
mod human_browser_android;
#[cfg(target_os = "ios")]
mod human_browser_ios;
mod provider_catalog;
mod providers;
mod tray;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod window;
mod wizard;
#[cfg(target_os = "ios")]
mod live_activity;

use daemon::DaemonState;
use tauri::Manager;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter,
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
        .plugin(tauri_plugin_mobile_push::init())
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

            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            {
                human_browser::init_app_handle(app.handle().clone());
                browser_host::start_browser_host_background();
            }
            #[cfg(target_os = "android")]
            {
                human_browser_android::init_app_handle(app.handle().clone());
            }
            #[cfg(target_os = "ios")]
            {
                human_browser_ios::init_app_handle(app.handle().clone());
            }

            Ok(())
        });

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        // Quit when the main window closes. Previously we hid to tray (prevent_close),
        // which left medousa-home running invisibly — including when launched from Cursor.
        // The hidden chat-popout window would also keep the process alive if we only
        // destroyed main; exit(0) tears down the whole app. Use tray → Hide to background.
        builder = builder.on_window_event(|window, event| {
            if window.label() == "browser" {
                if let tauri::WindowEvent::Resized { .. } = event {
                    human_browser::on_browser_window_resized(window.app_handle());
                }
            }
            if window.label() == "main" {
                if let tauri::WindowEvent::Resized { .. } = event {
                    human_browser::on_main_window_resized(window.app_handle());
                }
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                match window.label() {
                    "main" => {
                        window.app_handle().exit(0);
                    }
                    "browser" => {
                        // Hide instead of destroy so Web nav can reopen the window.
                        api.prevent_close();
                        let app = window.app_handle();
                        let _ = window.hide();
                        let _ = app.emit("browser-window-visibility", false);
                    }
                    "chat-popout" => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    _ => {}
                }
            }
        });
    }

    #[cfg(target_os = "android")]
    {
        builder = builder.on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::Resized { .. } = event {
                    human_browser_android::on_main_window_resized(window.app_handle());
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
            daemon_service::engine_diagnose,
            daemon_service::engine_clear_stale_lock,
            daemon_service::daemon_wait_healthy,
            daemon_service::workshop_ensure_engine,
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
            push::push_register_apns_token,
            push::push_clear_apns_token,
            pairing::bonjour_status,
            workshop_registry::workshops_load,
            workshop_registry::workshops_set_active,
            workshop_registry::workshops_add_local,
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
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_host_search,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_host_status,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_host_restart,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_host_resume_session,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_host_register_client,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_bridge_create_tab_group,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_bridge_get_tab_group,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_bridge_open_tab,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_bridge_navigate_tab,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_bridge_activate_tab,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_bridge_close_tab,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_bridge_set_control,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_bridge_link_work_card,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            browser_host::browser_bridge_snapshot,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            browser_host_mobile::browser_host_search,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            browser_host_mobile::browser_host_register_client,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            browser_host_mobile::browser_host_status,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            browser_host_mobile::browser_host_restart,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            browser_host_mobile::browser_host_resume_session,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_navigate,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_popout_navigate,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_embed_activate_tab,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_embed_close_tab,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_popout_activate_tab,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_popout_close_tab,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_report_new_window,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_reload,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_popout_reload,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_go_back,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_popout_go_back,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_go_forward,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_popout_go_forward,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_embed_apply_layout,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_embed_apply_mobile_layout,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_embed_set_bounds,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_embed_show,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_embed_hide,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_embed_read_bounds,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_embed_coord_probe,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_set_mobile_shell_active,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_report_title,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_report_snapshot,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_snapshot_html,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_snapshot_markdown,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_snapshot_search,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_stop,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_popout_stop,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_query_nav_state,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_popout_query_nav_state,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_report_nav_state,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_report_favicon,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_find_in_page,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_popout_find_in_page,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            human_browser::human_browser_report_find_result,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_navigate,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_reload,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_go_back,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_go_forward,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_embed_apply_layout,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_embed_apply_mobile_layout,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_embed_set_bounds,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_embed_show,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_embed_hide,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_embed_read_bounds,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_set_mobile_shell_active,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_report_title,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_report_snapshot,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_snapshot_html,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_snapshot_markdown,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_snapshot_search,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_stop,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_query_nav_state,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_report_nav_state,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_report_favicon,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_find_in_page,
            #[cfg(target_os = "android")]
            human_browser_android::human_browser_report_find_result,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_navigate,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_reload,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_go_back,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_go_forward,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_embed_apply_layout,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_embed_apply_mobile_layout,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_embed_set_bounds,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_embed_show,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_embed_hide,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_embed_read_bounds,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_set_mobile_shell_active,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_report_title,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_report_snapshot,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_snapshot_html,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_snapshot_markdown,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_snapshot_search,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_stop,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_query_nav_state,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_report_nav_state,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_report_favicon,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_find_in_page,
            #[cfg(target_os = "ios")]
            human_browser_ios::human_browser_report_find_result,
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
            daemon::vault::vault_list_tags,
            daemon::vault::vault_list_roots,
            daemon::vault::vault_set_active_root,
            daemon::vault::vault_add_root,
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
            daemon::session::session_delete,
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
            daemon::runtime::runtime_get_tui_defaults,
            daemon::runtime::runtime_put_tui_defaults,
            daemon::runtime::migrate_global_tui_defaults_to_engine,
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
            daemon::locus::locus_list_tags,
            daemon::locus::locus_get_node,
            daemon::artifact::artifact_command,
            daemon::artifact::artifact_fetch,
            daemon::artifact::artifact_list_ui,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            window::window_show_chat_popout,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            window::window_hide_chat_popout,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            window::window_show_browser,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            window::window_hide_browser,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            window::window_focus_browser,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            window::browser_window_present,
            tray::tray_update_blocked_count,
            #[cfg(target_os = "ios")]
            live_activity::live_activity_is_available,
            #[cfg(target_os = "ios")]
            live_activity::live_activity_sync,
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
            packages::packages_status,
            packages::packages_open_installer,
            daemon::local_inference::local_inference_hardware,
            daemon::local_inference::local_inference_catalog,
            daemon::local_inference::local_inference_models,
            daemon::local_inference::local_inference_start_download,
            daemon::local_inference::local_inference_download_status,
            daemon::local_inference::local_inference_spawn_engine,
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
    let web = MenuItem::with_id(app, "web", "Open Web", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &chat, &web, &hide, &quit])?;

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
                "web" => {
                    let _ = window::window_show_browser(app.clone());
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
