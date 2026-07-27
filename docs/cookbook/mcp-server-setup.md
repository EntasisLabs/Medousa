# Medousa MCP server

**Audience:** operator connecting Cursor / Codex / other MCP hosts to Medousa space

Expose vault, calendar, and artifacts to an external agentic runtime. This is the “keep the house” half of hot-swappable runtimes — see [ADR-008](../architecture/decisions/adr-008-hot-swappable-agent-runtime.md).

## Binary

```bash
cargo run -p medousa-mcp-server --bin medousa_mcp_server
```

Stdio JSON-RPC (MCP `2024-11-05`). Logs go to stderr.

## Environment

| Var | Default | Purpose |
|-----|---------|---------|
| `MEDOUSA_DAEMON_URL` (or `MEDOUSA_URL`) | `http://127.0.0.1:7419` | Workshop daemon base |
| `MEDOUSA_SESSION_TOKEN` (or `MEDOUSA_BEARER_TOKEN`) | _(empty)_ | Bearer when the daemon is not trusted-local |

## Tools (allowlist)

| Tool | Status |
|------|--------|
| `vault_list` / `vault_read` / `vault_search` | ✅ Live against daemon vault APIs |
| `calendar_list` | ✅ `/v1/calendar/events` |
| `artifacts_list` / `artifacts_fetch` | ✅ `/v1/runtime/artifact/list-ui` + `fetch` |
| `vault_write` | **Denied** (fail closed) |

**Denied:** spawn / turn / worker / host orchestration / OpenShell — never registered.

## Example host config (Cursor)

Point an MCP server entry at the `medousa_mcp_server` binary (stdio), with `MEDOUSA_DAEMON_URL` aimed at your running workshop engine.
