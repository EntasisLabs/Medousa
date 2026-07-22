# ADR-008: Hot-swappable agentic runtime (bones in 0.4.0)

## Status

Accepted

## Context

Medousa Home already has a strong vault, context, and native turn loop. Users who love that **space** may still want a different **agentic runtime** for a given chat (Cursor for coding, Codex, later others) — without abandoning Medousa’s workshop.

We must not marry the product to a single model **or** a single agent loop. The durable constant is Medousa space; the loop is swappable.

If ACP were wired only inside Home chat, Telegram / TUI / CLI / Discord would each reimplement agent spawning, streaming, and permission gates. That fights the daemon’s role as the workshop’s single control plane (already true for native interactive turns and the MCP **client** gateway pattern).

## Decision

1. **North star:** Any Medousa **channel** messages through the **daemon** via the **Medousa SDK** (`AgentsApi`). The daemon exposes `/v1/agents/…` for external ACP runtimes (Cursor, Codex). Native Medousa stays on `/v1/turns`. External runtimes reach vault/context via **Medousa MCP server**.
2. **SDK-only clients:** Home Tauri calls `client().agents()` only — no raw ACP sockets, no ad-hoc `workshop_http` path strings for agents. Python SDK mirrors the same accessor. TUI/CLI/Telegram use the same SDK surface.
3. **Where ACP lives:** In-process daemon library [`medousa-acp-client`](../../../crates/medousa-acp-client) (`ExternalAcpClient` + stub fallback). Optional sidecar later for crash isolation.
4. **Permissions:** ACP `PermissionRequest` → SSE `permission_request` + `/v1/agents/permission-requests/{id}/approve|deny` (budget-approval pattern).
5. **0.4.0 bones:** list runtimes, create/prompt/stream/cancel, Cursor/Codex spawn-when-available (else stub), thin Home runtime select. Not polished multi-channel pickers.

```text
Channel → Medousa SDK AgentsApi → Daemon /v1/agents
                                      ├── ExternalAcpClient → Cursor / Codex
                                      └── (native) /v1/turns → Medousa loop
External agent → Medousa MCP server → vault / context / calendar
```

## Consequences

- Native turns remain default; ACP is opt-in per session (Home: Runtime select).
- Contract parity includes `agents` accessor rows in `sdk-contract/manifest.yaml`.
- Stream events reuse `InteractiveTurnStreamEvent` with `agent_session_id` / `permission_request_id` fields.

## Code anchors

- `crates/medousa-acp-client/`
- `crates/medousa-mcp-server/`
- `crates/medousa-sdk/src/agents.rs` — `MedousaClient::agents()`
- `python/medousa-sdk/src/medousa/agents.py`
- `src/daemon/agents.rs` — `/v1/agents/…`
- `src/agent_permission_request.rs`
- `apps/medousa-home/src-tauri/src/daemon/agents.rs`
- `apps/medousa-home/src/lib/utils/sessionAgentRuntime.ts`
- `docs/cookbook/mcp-server-setup.md`
- `docs/cookbook/acp-external-agents.md`
