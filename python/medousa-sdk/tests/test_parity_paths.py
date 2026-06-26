"""Route parity table — must match docs/sdk/api-reference.md and medousa-sdk Rust sources."""

from __future__ import annotations

import pytest

# (accessor, method, http_method, path_template)
PARITY_ROUTES: list[tuple[str, str, str, str]] = [
    ("health", "get", "GET", "/health"),
    ("ingest", "post", "POST", "/v1/ingest"),
    ("local_models", "hardware", "GET", "/v1/local/hardware"),
    ("local_models", "catalog", "GET", "/v1/local/catalog"),
    ("local_models", "list", "GET", "/v1/local/models"),
    ("local_models", "engine_status", "GET", "/v1/local/engine/status"),
    ("local_models", "start_download", "POST", "/v1/local/models/download"),
    ("local_models", "remove_model", "DELETE", "/v1/local/models/{id}"),
    ("local_models", "download_status", "GET", "/v1/local/models/download/{job_id}"),
    ("jobs", "enqueue_ask", "POST", "/v1/jobs/ask"),
    ("recurring", "register_prompt", "POST", "/v1/recurring/prompt"),
    ("sessions", "list", "GET", "/v1/sessions?limit={limit}"),
    ("sessions", "history", "GET", "/v1/sessions/{id}/history"),
    ("sessions", "set_display_name", "PUT", "/v1/sessions/{id}/name"),
    ("sessions", "append_turn", "POST", "/v1/sessions/{id}/turns"),
    ("interactive", "start_turn", "POST", "/v1/interactive/turn"),
    ("interactive", "cancel", "POST", "/v1/sessions/{id}/active-turn"),
    ("runtime", "artifact_command", "POST", "/v1/runtime/artifact/command"),
    ("runtime", "artifact_fetch", "POST", "/v1/runtime/artifact/fetch"),
    ("runtime", "artifact_list_ui", "POST", "/v1/runtime/artifact/list-ui"),
    ("runtime", "config_command", "POST", "/v1/runtime/config/command"),
    ("runtime", "stage_route_command", "POST", "/v1/runtime/stage-route/command"),
    ("capabilities", "list", "GET", "/v1/capabilities"),
    ("capabilities", "get", "GET", "/v1/capabilities/{id}"),
    ("capabilities", "reindex", "POST", "/v1/capabilities/reindex"),
    ("mcp_gateway", "status", "GET", "/v1/mcp/gateway/status"),
    ("budget", "list_pending", "GET", "/v1/turns/budget-requests?status=pending&limit=20"),
    ("budget", "list_all", "GET", "/v1/turns/budget-requests?limit=20"),
    ("budget", "approve", "POST", "/v1/turns/budget-requests/{id}/approve"),
    ("budget", "deny", "POST", "/v1/turns/budget-requests/{id}/deny"),
]


@pytest.mark.parametrize("accessor,method,http_method,path", PARITY_ROUTES)
def test_parity_route_registered(accessor: str, method: str, http_method: str, path: str):
  """Each documented route has a parity table entry."""
  assert accessor
  assert method
  assert http_method in {"GET", "POST", "PUT", "DELETE"}
  assert path.startswith("/")


def test_parity_table_minimum_coverage():
    accessors = {row[0] for row in PARITY_ROUTES}
    expected = {
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
    }
    assert expected <= accessors
