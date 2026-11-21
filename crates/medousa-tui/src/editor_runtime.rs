use std::path::{Path, PathBuf};

use super::TuiState;
use medousa_tui::events::TuiEvent;
use tokio::sync::mpsc;

pub(crate) fn save_editor_buffer(state: &mut TuiState, path_override: Option<&str>) {
    if let Some(path_raw) = path_override
        && !path_raw.trim().is_empty()
    {
        state.editor_file_path = Some(PathBuf::from(path_raw.trim()));
    }

    let Some(path) = state.editor_file_path.clone() else {
        state.editor_status = "Save failed: no path. Use /save <path>".to_string();
        super::push_obs_alert(
            state,
            "⚠ save failed: no target path. use /save <path>".to_string(),
        );
        return;
    };

    match write_editor_file(&path, state.editor_buffer.as_text()) {
        Ok(_) => {
            state.editor_dirty = false;
            state.editor_status = format!("Saved {}", path.display());
            super::push_obs_alert(state, format!("✓ saved {}", path.display()));
        }
        Err(err) => {
            state.editor_status = format!("Save failed: {err}");
            super::push_obs_alert(state, format!("⚠ save failed: {err}"));
        }
    }
}

pub(crate) fn load_editor_file(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub(crate) fn write_editor_file(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

pub(crate) fn resolve_editor_run_source(
    path_override: Option<&str>,
    editor_file_path: Option<&Path>,
    editor_text: &str,
) -> std::result::Result<(String, String), String> {
    let source_target = path_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    if let Some(path) = source_target {
        return match load_editor_file(&path) {
            Ok(Some(content)) => Ok((content, format!("file:{}", path.display()))),
            Ok(None) => Err(format!("run failed: file not found {}", path.display())),
            Err(err) => Err(format!("run failed: {err}")),
        };
    }

    let label = editor_file_path
        .map(|path| format!("editor:{}", path.display()))
        .unwrap_or_else(|| "editor:buffer".to_string());
    Ok((editor_text.to_string(), label))
}

pub(crate) fn validate_editor_run_allowlist(
    source: &str,
    allowed_modules_csv: &str,
) -> std::result::Result<Vec<String>, String> {
    let analysis = super::analyze_allowlist_preview(source, allowed_modules_csv);
    if !analysis.invalid_allowlist.is_empty() {
        return Err(format!(
            "run blocked: invalid allowlist entries: {}",
            analysis.invalid_allowlist.join(", ")
        ));
    }

    if !analysis.blocked_ops.is_empty() {
        return Err(format!(
            "run blocked by allowlist: {}",
            analysis.blocked_ops.join(", ")
        ));
    }

    Ok(analysis.referenced_ops)
}

pub(crate) async fn run_editor_source_via_runtime(
    state: &mut TuiState,
    event_tx: &mpsc::Sender<TuiEvent>,
    path_override: Option<&str>,
) {
    let (source, source_label) = match resolve_editor_run_source(
        path_override,
        state.editor_file_path.as_deref(),
        state.editor_buffer.as_text(),
    ) {
        Ok(values) => values,
        Err(message) => {
            super::push_obs_alert(state, format!("⚠ {message}"));
            return;
        }
    };

    if source.trim().is_empty() {
        super::push_obs_alert(state, "⚠ run failed: source is empty".to_string());
        return;
    }

    let referenced_ops =
        match validate_editor_run_allowlist(&source, &state.settings.allowed_modules) {
            Ok(ops) => ops,
            Err(message) => {
                super::push_obs_alert(state, format!("⚠ {message}"));
                return;
            }
        };

    let daemon_url = state.daemon_url.clone();
    let tx = event_tx.clone();
    super::push_obs_alert(state, format!("Running {source_label} on the daemon"));
    tokio::spawn(async move {
        let result = async {
            let client = super::daemon_commands::daemon_client(&daemon_url)?;
            client
                .transport()
                .post_json(
                    &daemon_url,
                    "/v1/grapheme/run",
                    serde_json::json!({ "source": source }),
                )
                .await
                .map_err(anyhow::Error::from)
        }
        .await;
        let event = match result {
            Ok(response) => {
                let mut result = response["result"].clone();
                if let Some(fields) = result.as_object_mut() {
                    fields.insert("source".into(), source_label.clone().into());
                }
                TuiEvent::ToolPayload {
                    tool_name: "editor.gr.run".into(),
                    tool_input: serde_json::json!({ "source": source_label, "referenced_ops": referenced_ops }),
                    tool_output: result,
                    input_receipt: None,
                    output_receipt: None,
                }
            }
            Err(err) => TuiEvent::UiNotice(format!("Grapheme run failed: {err}")),
        };
        let _ = tx.send(event).await;
    });
}
