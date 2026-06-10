#[tauri::command]
pub fn write_file_bytes(path: String, bytes: Vec<u8>) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path is required".to_string());
    }
    std::fs::write(trimmed, bytes).map_err(|err| err.to_string())
}
