# Agent tools

**Audience:** integrator, contributor

Medousa exposes **cognition** tools to the LLM during turns. Tools are split between **host** (interactive, fast) and **worker** (durable) lanes.

Deep dive: [turn-runtime-and-lanes.md](../../architecture/turn-runtime-and-lanes.md)

---

## Lanes

| Lane | Typical tools | When |
|------|---------------|------|
| **Host** | Bootstrap, vault read, artifact read, MCP | Interactive chat, TUI |
| **Worker** | Long jobs, sandboxed skills | Background ask jobs |

Worker allowlists can strip UI-only tools when `supports_ui_artifacts=false`.

---

## Discover domains

`cognition_tools_discover` returns grouped tool catalogs. Domains include:

- **documents** — vault + artifact list/read/grep/write
- **presentation** — artifact presentation tools
- Standard rings: bootstrap, MCP, finish, etc.

Source: `src/tool_bootstrap.rs`

---

## Built-in cognition families

| Family | Examples |
|--------|----------|
| Vault | `cognition_vault_*` — [vault.md](vault.md) |
| Artifacts | `cognition_artifact_*` — [artifacts.md](artifacts.md) |
| MCP | `cognition.mcp.*` — [mcp-gateway-setup.md](../mcp-gateway-setup.md) |
| UI present | `cognition_ui_present` — emits `ui_artifact` on stream |
| Finish | `cognition_finish` — ends tool loop |

---

## MCP vs built-ins

MCP tools are proxied through the gateway (`http://127.0.0.1:7420` default). Policy evaluation: `POST /v1/mcp/policy/evaluate`.

Capabilities catalog: `GET /v1/capabilities` — SDK `capabilities().list()`.

---

## Integrator guidance

- **HTTP-only clients** do not invoke tools directly; they send prompts via interactive turn or jobs API.
- **Custom UIs** should handle stream events (`tool_*`, `ui_artifact`, `artifact_updated`) — [custom-chat-ui.md](../cookbook/custom-chat-ui.md).
