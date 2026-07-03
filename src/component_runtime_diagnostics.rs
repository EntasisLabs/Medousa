//! Build per-component runtime diagnostics for custom_view_doctor.

use medousa_types::component_runtime::{
    ComponentRuntimeDiagnostic, ComponentRuntimeEmbedStatus, ComponentRuntimeIssue,
    ComponentRuntimeProbeBlock, ComponentStaticLintFinding,
    ComponentStoreKeyStatus, ComponentSuggestedAction,
};
use medousa_types::environment::ComponentDef;
use serde_json::Value;

use crate::artifact_html_lint::{lint_artifact_html, lint_store_value};
use crate::component_runtime_store::component_runtime_hub;
use crate::component_store::component_store_service;
use crate::environment_store::resolve_profile_id;

pub struct RuntimeDiagnosticOptions {
    pub profile_id: String,
    pub include_runtime: bool,
    pub include_static_lint: bool,
    pub probe: bool,
    pub session_id: Option<String>,
}

pub async fn build_component_runtime_diagnostic(
    component: &ComponentDef,
    options: &RuntimeDiagnosticOptions,
) -> ComponentRuntimeDiagnostic {
    let component_id = component.id.clone();
    let artifact_id = presentation_artifact_id(&component.config);
    let mut diagnostic = ComponentRuntimeDiagnostic {
        component_id: component_id.clone(),
        artifact_id: artifact_id.clone(),
        embed: None,
        store_keys: Vec::new(),
        logs: Vec::new(),
        static_lint: Vec::new(),
        issues: Vec::new(),
        suggested_actions: Vec::new(),
        probe: None,
        last_error: None,
        store_key_count: 0,
    };

    if options.include_static_lint {
        if let Some(artifact_id) = artifact_id.as_deref() {
            if let Ok(body) = fetch_artifact_html(options.session_id.as_deref(), artifact_id) {
                diagnostic.static_lint = lint_artifact_html(&body);
                diagnostic.embed = Some(ComponentRuntimeEmbedStatus {
                    store_bootstrap_injected: true,
                    metrics_injected: true,
                    runtime_bridge_injected: true,
                    store_client_injected: true,
                });
                for finding in &diagnostic.static_lint {
                    push_issue_from_lint(&mut diagnostic.issues, finding);
                }
            }
        }
    }

    if options.include_runtime {
        if let Ok(store) = component_store_service()
            .get(&options.profile_id, &component_id, None)
            .await
        {
            diagnostic.store_key_count = store.entries.len();
            for (key, value) in store.entries {
                let value_type = crate::artifact_html_lint::json_value_type(&value);
                let store_issues = lint_store_value(&key, &value);
                for issue in &store_issues {
                    diagnostic.issues.push(ComponentRuntimeIssue {
                        code: "STORE_WRONG_TYPE".to_string(),
                        severity: "error".to_string(),
                        message: format!("Store key '{key}' has {issue}"),
                        fix_hint: "Reset with component store API or patch artifact to use Array.isArray guards.".to_string(),
                    });
                    diagnostic.suggested_actions.push(ComponentSuggestedAction {
                        tool: "cognition_artifact_write".to_string(),
                        reason: format!("Fix store read guards for key '{key}'"),
                    });
                }
                diagnostic.store_keys.push(ComponentStoreKeyStatus {
                    key,
                    value_type,
                    issues: store_issues,
                });
            }
        }

        if let Ok(logs) = component_runtime_hub()
            .tail(&options.profile_id, &component_id, 10)
            .await
        {
            diagnostic.last_error = logs
                .iter()
                .rev()
                .find(|event| event.level == "error")
                .map(|event| event.message.clone());
            for event in &logs {
                if event.level == "error" || event.level == "warn" {
                    diagnostic.issues.push(ComponentRuntimeIssue {
                        code: "RUNTIME_LOG".to_string(),
                        severity: event.level.clone(),
                        message: event.message.clone(),
                        fix_hint: "Read artifact source and patch the failing line via cognition_artifact_write.".to_string(),
                    });
                }
            }
            diagnostic.logs = logs;
        }
    }

    if options.probe {
        let probe_request = component_runtime_hub()
            .register_probe(&options.profile_id, &component_id)
            .await;
        let (status, result) = component_runtime_hub()
            .wait_for_probe(&probe_request.probe_id, 2_000)
            .await;
        let status_str = status.as_str().to_string();
        if let Some(result) = result {
            if !result.store_ready {
                diagnostic.issues.push(ComponentRuntimeIssue {
                    code: "PROBE_STORE_NOT_READY".to_string(),
                    severity: "error".to_string(),
                    message: "MedousaStore.ready() is false — widget is not on a canvas surface with componentId.".to_string(),
                    fix_hint: "Open the custom surface in Home; chat-only embeds cannot persist.".to_string(),
                });
            }
            if !result.store_round_trip_ok {
                diagnostic.issues.push(ComponentRuntimeIssue {
                    code: "PROBE_STORE_ROUND_TRIP_FAILED".to_string(),
                    severity: "error".to_string(),
                    message: format!("Store round-trip failed: {}", result.errors.join("; ")),
                    fix_hint: "Check daemon connectivity and component registration.".to_string(),
                });
            }
            diagnostic.probe = Some(ComponentRuntimeProbeBlock {
                status: status_str,
                result: Some(result),
            });
        } else {
            diagnostic.probe = Some(ComponentRuntimeProbeBlock {
                status: status_str,
                result: None,
            });
        }
    }

    if diagnostic.issues.iter().any(|i| i.code.starts_with("STATIC_")) {
        diagnostic.suggested_actions.push(ComponentSuggestedAction {
            tool: "cognition_artifact_write".to_string(),
            reason: "Fix static anti-patterns in HTML (localStorage, store guards)".to_string(),
        });
    }
    diagnostic
        .suggested_actions
        .push(ComponentSuggestedAction {
            tool: "cognition_custom_view_doctor".to_string(),
            reason: "Re-run doctor after fixes until issues is empty".to_string(),
        });

    diagnostic
}

fn presentation_artifact_id(config: &Value) -> Option<String> {
    config
        .get("artifactId")
        .or_else(|| config.get("artifact_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn fetch_artifact_html(session_id: Option<&str>, artifact_id: &str) -> Result<String, String> {
    if let Some(session_id) = session_id {
        if let Some(fetched) = crate::artifact_store::fetch_artifact(session_id, artifact_id) {
            return Ok(fetched.body);
        }
    }
    let records = crate::artifact_store::list_ui_artifacts(None, 100, Some(artifact_id));
    for record in records {
        if let Some(fetched) =
            crate::artifact_store::fetch_artifact(&record.session_id, &record.artifact_id)
        {
            return Ok(fetched.body);
        }
    }
    Err(format!("artifact not found: {artifact_id}"))
}

fn push_issue_from_lint(issues: &mut Vec<ComponentRuntimeIssue>, finding: &ComponentStaticLintFinding) {
    let fix_hint = match finding.code.as_str() {
        "STATIC_LOCALSTORAGE" | "STATIC_SESSIONSTORAGE" => {
            "Replace with MedousaStore.get/set/delete.".to_string()
        }
        "STATIC_STORE_GET_NO_KEY" => {
            "Use MedousaStore.get('key') and Array.isArray guard.".to_string()
        }
        "STATIC_SLICE_WITHOUT_GUARD" => {
            "Wrap store reads: const items = Array.isArray(raw) ? raw : [];".to_string()
        }
        "STATIC_STORE_SYNC_USAGE" => {
            "MedousaStore is async — use async functions and await get/set/delete; see wiki topic artifact_runtime.".to_string()
        }
        _ => "Patch artifact HTML via cognition_artifact_write.".to_string(),
    };
    issues.push(ComponentRuntimeIssue {
        code: finding.code.clone(),
        severity: finding.severity.clone(),
        message: finding.message.clone(),
        fix_hint,
    });
}

pub fn resolve_profile(profile_id: Option<&str>) -> String {
    resolve_profile_id(profile_id)
}
