//! Android native browser — Tauri child webview with DOM-measured bounds from BrowserCompositor.
//!
//! Uses the same embed path as desktop (`human_browser` add_child on the main window).

pub use crate::human_browser::{
    human_browser_embed_apply_layout, human_browser_embed_apply_mobile_layout,
    human_browser_embed_hide, human_browser_embed_read_bounds, human_browser_embed_set_bounds,
    human_browser_embed_show, human_browser_find_in_page, human_browser_go_back,
    human_browser_go_forward, human_browser_navigate, human_browser_query_nav_state,
    human_browser_reload, human_browser_report_favicon, human_browser_report_find_result,
    human_browser_report_nav_state, human_browser_report_snapshot, human_browser_report_title,
    human_browser_set_mobile_shell_active, human_browser_snapshot_html,
    human_browser_snapshot_markdown, human_browser_snapshot_search, human_browser_stop,
    init_app_handle, on_main_window_resized,
};
