//! Closed native dispatch for Code Intelligence HTTP operations.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use super::{DaemonState, workshop_http};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeReadOperation {
    LanguageRoot,
    LanguageSessions,
    LanguageMatrix,
    Symbols,
    WorkspaceSymbols,
    WorkspaceDiagnostics,
    Capabilities,
    Conventions,
}

impl CodeReadOperation {
    fn path(self) -> &'static str {
        match self {
            Self::LanguageRoot => "/v1/code/language-root",
            Self::LanguageSessions => "/v1/code/language-sessions",
            Self::LanguageMatrix => "/v1/code/language-matrix",
            Self::Symbols => "/v1/code/symbols",
            Self::WorkspaceSymbols => "/v1/code/workspace-symbols",
            Self::WorkspaceDiagnostics => "/v1/code/workspace-diagnostics",
            Self::Capabilities => "/v1/code/capabilities",
            Self::Conventions => "/v1/code/conventions",
        }
    }
}

#[tauri::command]
pub async fn code_read(
    state: State<'_, DaemonState>,
    operation: CodeReadOperation,
    query: HashMap<String, String>,
    execution_runtime_id: Option<String>,
) -> Result<Value, String> {
    let query = query
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect::<Vec<_>>();
    let path = workshop_http::path_with_query(operation.path(), &query);
    let config = match execution_runtime_id
        .as_deref()
        .map(str::trim)
        .filter(|runtime_id| !runtime_id.is_empty())
    {
        Some(runtime_id) => crate::active_workshop::transport_config_for_runtime_id(runtime_id)?,
        None => workshop_http::transport_config(&state)?,
    };
    crate::workshop_transport::workshop_get_json(&config, &path).await
}

#[tauri::command]
pub async fn code_request(
    state: State<'_, DaemonState>,
    body: Value,
    execution_runtime_id: Option<String>,
) -> Result<Value, String> {
    let config = match execution_runtime_id
        .as_deref()
        .map(str::trim)
        .filter(|runtime_id| !runtime_id.is_empty())
    {
        Some(runtime_id) => crate::active_workshop::transport_config_for_runtime_id(runtime_id)?,
        None => workshop_http::transport_config(&state)?,
    };
    crate::workshop_transport::workshop_post_json(&config, "/v1/code/request", &body).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_operations_are_a_closed_path_inventory() {
        let operations = [
            CodeReadOperation::LanguageRoot,
            CodeReadOperation::LanguageSessions,
            CodeReadOperation::LanguageMatrix,
            CodeReadOperation::Symbols,
            CodeReadOperation::WorkspaceSymbols,
            CodeReadOperation::WorkspaceDiagnostics,
            CodeReadOperation::Capabilities,
            CodeReadOperation::Conventions,
        ];
        assert!(
            operations
                .iter()
                .all(|operation| operation.path().starts_with("/v1/code/"))
        );
        assert_eq!(operations.len(), 8);
    }
}
