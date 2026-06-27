//! Mobile stubs for desktop embedded native webviews (Android only).
//! iOS uses [`crate::human_browser_ios`]. Browser UI on Android uses iframe fallback.

use medousa_browser_lite::SearchResponse;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedMobileLayoutParams {
    pub bottom_chrome_height: f64,
    pub content_bounds: Option<EmbedBoundsDto>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedLayoutParams {
    pub activity_width: f64,
    pub activity_collapsed: bool,
    pub work_rail_visible: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedBoundsDto {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedBoundsReadback {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub window_width: f64,
    pub window_height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotReport {
    pub url: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHtmlDto {
    pub url: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMarkdownDto {
    pub url: String,
    pub title: String,
    pub markdown: String,
}

#[tauri::command]
pub async fn human_browser_navigate(_app: AppHandle, _url: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn human_browser_reload(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn human_browser_go_back(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn human_browser_go_forward(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn human_browser_embed_apply_layout(
    _app: AppHandle,
    _params: EmbedLayoutParams,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn human_browser_embed_apply_mobile_layout(
    _app: AppHandle,
    _params: EmbedMobileLayoutParams,
) -> Result<bool, String> {
    Ok(false)
}

#[tauri::command]
pub fn human_browser_embed_set_bounds(
    _app: AppHandle,
    _bounds: EmbedBoundsDto,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn human_browser_embed_show(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn human_browser_embed_hide(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn human_browser_embed_read_bounds(_app: AppHandle) -> Result<EmbedBoundsReadback, String> {
    Ok(EmbedBoundsReadback {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        window_width: 0.0,
        window_height: 0.0,
    })
}

#[tauri::command]
pub fn human_browser_set_mobile_shell_active(_active: bool) {}

#[tauri::command]
pub fn human_browser_report_title(
    _app: AppHandle,
    _url: String,
    _title: String,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn human_browser_report_snapshot(_payload: SnapshotReport) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn human_browser_snapshot_html(_app: AppHandle) -> Result<SnapshotHtmlDto, String> {
    Err("native browser snapshot unavailable on mobile".to_string())
}

#[tauri::command]
pub async fn human_browser_snapshot_markdown(
    _app: AppHandle,
    _max_chars: Option<usize>,
) -> Result<SnapshotMarkdownDto, String> {
    Err("native browser snapshot unavailable on mobile".to_string())
}

#[tauri::command]
pub async fn human_browser_snapshot_search(
    _app: AppHandle,
    _query: String,
    _max_results: Option<usize>,
) -> Result<SearchResponse, String> {
    Err("native browser snapshot unavailable on mobile".to_string())
}
