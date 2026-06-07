mod daemon;

use daemon::DaemonState;
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(DaemonState::new())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
