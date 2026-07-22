# Component: medousa-mcp-gateway

## Role in the Product

`medousa-mcp-gateway` is the **external capability broker** for Medousa.

It is a separate long-running process that:

- connects to third-party **MCP servers** as an MCP **client**
- exposes a narrow HTTP API for the daemon's agent runtime to **discover** and **invoke** external tools
- holds MCP server credentials (not the daemon)
- provides an **instant kill switch** for all external tool access

It is **not** the agent brain. The daemon's `MedousaAgentRuntime` remains the sole turn engine.

## Entry Point

- Binary crate: [`adapters/medousa-mcp-gateway/`](../adapters/medousa-mcp-gateway/) → bin `medousa_mcp_gateway`
- Default bind: `127.0.0.1:7420`
- Contract types: [`crates/medousa-types/src/mcp_gateway_api.rs`](../crates/medousa-types/src/mcp_gateway_api.rs)
- Release package id: `mcp-gateway` (`medousa pull mcp-gateway`)

## Process Model

```
medousa-daemon (7419)          medousa-mcp-gateway (7420)
        │                                │
        │  cognition.mcp.* tools         │
        ├──────────────────────────────►│ discover / invoke
        │◄──────────────────────────────┤ policy evaluate callback
        │                                │
        │                                ├── MCP stdio → notion-mcp
        │                                ├── MCP SSE  → remote server
        │                                └── catalog cache
```

On compromise or incident: stop or disable the gateway. Daemon, TUI, Telegram, and Stasis scheduling continue without external MCP.

## API Surface (gateway)

| Route | Auth | Purpose |
|-------|------|---------|
| `GET /health` | none (loopback) or bearer | Liveness + kill-switch state |
| `POST /v1/mcp/discover` | gateway token | Search cached MCP tool catalog |
| `POST /v1/mcp/invoke` | gateway token + turn token | Execute MCP tool call |
| `POST /v1/admin/invokes/disable` | admin token | Global kill switch |
| `POST /v1/admin/servers/{id}/disable` | admin token | Per-server shutoff |

Full design: [docs/internal/mcp-client-gateway-design.md](../docs/internal/mcp-client-gateway-design.md)

## Daemon Callback

| Route | Caller | Purpose |
|-------|--------|---------|
| `POST /v1/mcp/policy/evaluate` | gateway | Identity + lane policy before invoke |

## Agent Tools (daemon-side)

| Tool | Gateway call |
|------|--------------|
| `cognition.capability.resolve` | daemon `CapabilityRegistry` (aggregates Grapheme + MCP) |
| `cognition.mcp.discover` | `POST /v1/mcp/discover` |
| `cognition.mcp.invoke` | `POST /v1/mcp/invoke` |
| `cognition.mcp.servers` | derived from discover / registry metadata |

Daemon syncs MCP bindings via `GET /v1/mcp/catalog`. See [capability-catalog-design.md](../docs/internal/capability-catalog-design.md).

## Configuration

- Gateway: `~/.config/medousa/mcp-gateway.toml` — server registry, transports, lane allowlists
- Setup guide: [docs/mcp-gateway-setup.md](../docs/mcp-gateway-setup.md)
- Background start: `medousa start mcp-gateway`
- Env: `MEDOUSA_MCP_GATEWAY_URL`, `MEDOUSA_MCP_GATEWAY_TOKEN`, `MEDOUSA_MCP_GATEWAY_ADMIN_TOKEN`
- Daemon: `MEDOUSA_MCP_POLICY_TOKEN` for gateway callback auth

## Relationship to Stasis

Phase D (after gateway MVP): MCP invokes can be promoted to Stasis jobs (`cognition.mcp.promote_to_job`), same pattern as Grapheme promote-to-recurring. Gateway remains the only MCP client — Stasis jobs call gateway HTTP, not MCP directly.

## Code Anchors

- `crates/medousa-types/src/mcp_gateway_api.rs` — shared HTTP contract
- `crates/medousa-types/src/mcp_turn_token.rs` — turn-scoped invoke tokens
- `adapters/medousa-mcp-gateway/` — gateway library + binary (no Surreal / no `medousa` lib)
- `src/mcp_gateway.rs` — thin re-export for daemon/setup helpers
- `src/tools.rs` — `CognitionMcp*Tool` implementations
- `src/bin/medousa_daemon.rs` — `/v1/mcp/policy/evaluate` handler
