use std::path::PathBuf;

/// Resolve Medousa data directory (`MEDOUSA_DATA_DIR` or platform default).
pub fn medousa_data_dir() -> PathBuf {
    std::env::var("MEDOUSA_DATA_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("medousa")
        })
}
