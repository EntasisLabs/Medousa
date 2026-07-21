# Medousa MCP server (bones)

**Audience:** operator connecting Cursor / Codex / other MCP hosts to Medousa space

Expose vault, calendar, and artifacts to an external agentic runtime. This is the “keep the house” half of hot-swappable runtimes — see [ADR-008](../architecture/decisions/adr-008-hot-swappable-agent-runtime.md).

## Binary

```bash
cargo run -p medousa-mcp-server --bin medousa_mcp_server
```

Stdio JSON-RPC (MCP `2024-11-05`). Logs go to stderr.

## Tools (allowlist)

| Tool | Purpose |
|------|---------|
| `vault_list` / `vault_read` / `vault_write` / `vault_search` | Workshop vault |
| `calendar_list` | Calendar events |
| `artifacts_list` / `artifacts_fetch` | Artifacts |

**Denied:** spawn / turn / worker / host orchestration / OpenShell — never registered.

## Status (0.4.0)

Protocol surface + stubs land first. Workshop-bound I/O wires next; until then tools return a bones acknowledgement.

## Example host config (Cursor)

Point an MCP server entry at the `medousa_mcp_server` binary (stdio). Exact UI varies by host — treat this cookbook as the Medousa side of the contract.
