//! Route parity table — must match docs/sdk/api-reference.md and medousa-sdk sources.

/// (accessor, method, http_method, path_template)
pub const PARITY_ROUTES: &[(&str, &str, &str, &str)] = &[
    ("health", "get", "GET", "/health"),
    ("ingest", "post", "POST", "/v1/ingest"),
    ("local_models", "hardware", "GET", "/v1/local/hardware"),
    ("local_models", "catalog", "GET", "/v1/local/catalog"),
    ("local_models", "list", "GET", "/v1/local/models"),
    ("local_models", "engine_status", "GET", "/v1/local/engine/status"),
    ("local_models", "start_download", "POST", "/v1/local/models/download"),
    ("local_models", "remove_model", "DELETE", "/v1/local/models/{id}"),
    ("local_models", "download_status", "GET", "/v1/local/models/download/{job_id}"),
    ("local_models", "download_events", "SSE", "/v1/local/models/download/{job_id}/events"),
    ("jobs", "enqueue_ask", "POST", "/v1/jobs/ask"),
    ("jobs", "result", "GET", "/v1/jobs/{id}/result"),
    ("jobs", "report", "GET", "/v1/jobs/{id}/report"),
    ("jobs", "enqueue_report", "POST", "/v1/jobs/report"),
    ("jobs", "enqueue_prompt", "POST", "/v1/jobs/prompt"),
    ("jobs", "complete_actions", "POST", "/v1/jobs/{id}/complete-actions"),
    ("jobs", "archive", "POST", "/v1/jobs/{id}/archive"),
    ("recurring", "register_prompt", "POST", "/v1/recurring/prompt"),
    ("recurring", "list", "GET", "/v1/recurring"),
    ("recurring", "update", "PATCH", "/v1/recurring/{id}"),
    ("recurring", "delete", "DELETE", "/v1/recurring/{id}"),
    ("recurring", "runs", "GET", "/v1/recurring/{id}/runs"),
    ("recurring", "delivery_status", "GET", "/v1/recurring/{id}/delivery"),
    ("sessions", "list", "GET", "/v1/sessions?limit={limit}"),
    ("sessions", "history", "GET", "/v1/sessions/{id}/history"),
    ("sessions", "set_display_name", "PUT", "/v1/sessions/{id}/name"),
    ("sessions", "append_turn", "POST", "/v1/sessions/{id}/turns"),
    ("sessions", "delete", "DELETE", "/v1/sessions/{id}"),
    ("sessions", "list_turns", "GET", "/v1/sessions/{id}/turns"),
    ("sessions", "active_turn", "GET", "/v1/sessions/{id}/active-turn"),
    ("sessions", "cancel_active_turn", "POST", "/v1/sessions/{id}/active-turn"),
    ("interactive", "start_turn", "POST", "/v1/interactive/turn"),
    ("interactive", "cancel", "POST", "/v1/sessions/{id}/active-turn"),
    ("interactive", "stream", "SSE", "/v1/interactive/turn/{id}/stream"),
    ("runtime", "artifact_command", "POST", "/v1/runtime/artifact/command"),
    ("runtime", "artifact_fetch", "POST", "/v1/runtime/artifact/fetch"),
    ("runtime", "artifact_write", "POST", "/v1/runtime/artifact/write"),
    ("runtime", "artifact_delete", "POST", "/v1/runtime/artifact/delete"),
    ("runtime", "artifact_list_ui", "POST", "/v1/runtime/artifact/list-ui"),
    ("runtime", "config_command", "POST", "/v1/runtime/config/command"),
    ("runtime", "stage_route_command", "POST", "/v1/runtime/stage-route/command"),
    ("capabilities", "list", "GET", "/v1/capabilities"),
    ("capabilities", "get", "GET", "/v1/capabilities/{id}"),
    ("capabilities", "reindex", "POST", "/v1/capabilities/reindex"),
    ("mcp_gateway", "status", "GET", "/v1/mcp/gateway/status"),
    ("budget", "list", "GET", "/v1/turns/budget-requests"),
    ("budget", "get", "GET", "/v1/turns/budget-requests/{id}"),
    ("budget", "approve", "POST", "/v1/turns/budget-requests/{id}/approve"),
    ("budget", "deny", "POST", "/v1/turns/budget-requests/{id}/deny"),
    ("vault", "list_roots", "GET", "/v1/vault/roots"),
    ("vault", "add_root", "POST", "/v1/vault/roots"),
    ("vault", "set_active_root", "PUT", "/v1/vault/active"),
    ("vault", "list_notes", "GET", "/v1/vault/notes"),
    ("vault", "create_note", "POST", "/v1/vault/notes"),
    ("vault", "get_note", "GET", "/v1/vault/notes/{path}"),
    ("vault", "update_note", "PUT", "/v1/vault/notes/{path}"),
    ("vault", "delete_note", "DELETE", "/v1/vault/notes/{path}"),
    ("vault", "list_tags", "GET", "/v1/vault/tags"),
    ("vault", "search", "GET", "/v1/vault/search"),
    ("vault", "backlinks", "GET", "/v1/vault/backlinks"),
    ("environment", "get_spec", "GET", "/v1/environment/spec"),
    ("environment", "put_spec", "PUT", "/v1/environment/spec"),
    ("environment", "get_status", "GET", "/v1/environment/status"),
    ("environment", "validate_spec", "POST", "/v1/environment/spec/validate"),
    ("environment", "propose_spec", "POST", "/v1/environment/spec/propose"),
    ("environment", "get_pending", "GET", "/v1/environment/spec/pending"),
    ("environment", "dismiss_pending", "DELETE", "/v1/environment/spec/pending"),
    ("environment", "apply_pending", "POST", "/v1/environment/spec/pending/apply"),
    ("environment", "stream_spec", "SSE", "/v1/environment/spec/stream"),
    ("components", "store_get", "GET", "/v1/components/{component_id}/store"),
    ("components", "store_set", "PUT", "/v1/components/{component_id}/store"),
    ("components", "store_list_keys", "GET", "/v1/components/{component_id}/store/keys"),
    ("components", "store_get_key", "GET", "/v1/components/{component_id}/store/{key}"),
    ("components", "store_put_key", "PUT", "/v1/components/{component_id}/store/{key}"),
    ("components", "store_delete_key", "DELETE", "/v1/components/{component_id}/store/{key}"),
    ("components", "runtime_tail_events", "GET", "/v1/components/{component_id}/runtime/events"),
    ("components", "runtime_append_events", "POST", "/v1/components/{component_id}/runtime/events"),
    (
        "components",
        "runtime_complete_probe",
        "POST",
        "/v1/components/{component_id}/runtime/probe/{probe_id}/result",
    ),
    ("feeds", "list", "GET", "/v1/feeds"),
    ("feeds", "tail", "GET", "/v1/feeds/{feed_id}/tail"),
    ("feeds", "mark_read", "POST", "/v1/feeds/{feed_id}/read"),
    ("feeds", "stream", "SSE", "/v1/feeds/stream"),
    ("workspace", "list_cards", "GET", "/v1/workspace/cards"),
    ("workspace", "get_card", "GET", "/v1/workspace/cards/{id}"),
    ("workspace", "cancel_card", "POST", "/v1/workspace/cards/{id}/cancel"),
    ("workspace", "archive_card", "POST", "/v1/workspace/cards/{id}/archive"),
    ("workspace", "retry_card", "POST", "/v1/workspace/cards/{id}/retry"),
    ("workspace", "link_vault", "POST", "/v1/workspace/cards/{id}/link-vault"),
    ("workspace", "feed", "GET", "/v1/workspace/feed"),
    ("workspace", "snapshot", "GET", "/v1/workspace/snapshot"),
];

#[test]
fn parity_table_minimum_coverage() {
    let accessors: std::collections::HashSet<_> = PARITY_ROUTES.iter().map(|r| r.0).collect();
    let expected = [
        "health",
        "ingest",
        "local_models",
        "jobs",
        "recurring",
        "sessions",
        "interactive",
        "runtime",
        "capabilities",
        "mcp_gateway",
        "budget",
        "vault",
        "environment",
        "components",
        "feeds",
        "workspace",
    ];
    for accessor in expected {
        assert!(
            accessors.contains(accessor),
            "missing accessor in parity table: {accessor}"
        );
    }
}

#[test]
fn parity_routes_are_well_formed() {
    for (accessor, method, http_method, path) in PARITY_ROUTES {
        assert!(!accessor.is_empty());
        assert!(!method.is_empty());
        assert!(
            matches!(*http_method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "SSE"),
            "unexpected http method: {http_method}"
        );
        assert!(path.starts_with('/'));
    }
}
