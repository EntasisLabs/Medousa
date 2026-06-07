mod daemon;
mod messaging;
mod medousa_paths;
mod tray;
mod window;

use daemon::DaemonState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(DaemonState::new())
        .setup(|app| {
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register_all();
            }
            let show = MenuItem::with_id(app, "show", "Show Medousa Home", true, None::<&str>)?;
            let chat = MenuItem::with_id(app, "chat", "Open Chat", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &chat, &hide, &quit])?;

            if let Some(icon) = app.default_window_icon().cloned() {
                TrayIconBuilder::with_id("main-tray")
                    .icon(icon)
                    .menu(&menu)
                    .tooltip("Medousa Home")
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
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            daemon::daemon_url,
            daemon::set_daemon_url,
            daemon::daemon_health,
            daemon::workspace_stream_start,
            daemon::workspace_stream_stop,
            daemon::interactive_turn_send,
            daemon::interactive_stream_start,
            daemon::interactive_stream_stop,
            daemon::vault::vault_list_notes,
            daemon::vault::vault_get_note,
            daemon::vault::vault_save_note,
            daemon::vault::vault_search,
            daemon::vault::vault_backlinks,
            daemon::workspace_card::workspace_get_card,
            daemon::workspace_card::workspace_cancel_card,
            daemon::workspace_card::workspace_retry_card,
            daemon::session::session_list,
            daemon::session::session_get_history,
            daemon::catalog::catalog_list_manuscripts,
            daemon::catalog::catalog_list_capabilities,
            daemon::catalog::catalog_get_capability,
            daemon::runtime::runtime_get_stats,
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
            daemon::identity::identity_get_context,
            daemon::artifact::artifact_command,
            window::window_show_chat_popout,
            window::window_hide_chat_popout,
            tray::tray_update_blocked_count,
            medousa_paths::medousa_config_paths,
            medousa_paths::load_tui_defaults_summary,
            medousa_paths::load_tui_defaults,
            medousa_paths::persist_tui_defaults,
            medousa_paths::persist_tui_runtime_prefs,
            messaging::messaging_load_product_config_summary,
            messaging::messaging_save_channel_config,
            messaging::messaging_secret_status,
            messaging::messaging_save_secret,
            messaging::messaging_clear_secret,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn hide_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}
