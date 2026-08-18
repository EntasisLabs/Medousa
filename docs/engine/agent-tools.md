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

Worker allowlists can strip UI-only tools when `supports_ui_artifacts=false`, and browser tools when `supports_browser_host=false`.

See [agent-browser-host.md](../../architecture/agent-browser-host.md) for search/fetch/CAPTCHA design.

---

## Discover domains

`cognition_tools_discover` returns grouped tool catalogs. Domains include:

- **documents** — vault + artifact list/read/grep/write
- **calendar** — personal `.ics` list/create/update/delete/import/export
- **presentation** — artifact presentation tools
- **environment** — environment spec + component canvas ([environment-canvas.md](./environment-canvas.md))
- **browser** — `cognition_browser_fetch` (auto-unlocked on browser-capable clients)
- Standard rings: bootstrap, MCP, finish, etc.

Source: `src/tool_bootstrap.rs`

---

## Built-in cognition families

| Family | Examples |
|--------|----------|
| Store | `cognition_store_read` / `cognition_store_write` (`store=vault\|artifacts\|code\|scripts`) — [vault.md](vault.md), [artifacts.md](artifacts.md) |
| Capability | `cognition_capability` (`op=find\|invoke`, `source=auto\|mcp\|grapheme`) — catalog, MCP, and Grapheme discover/run |
| Schema | `cognition_schema` (`domain` + `types=[...]`) — batched typed action parameter schemas |
| Runtime | `cognition_runtime_query` / `cognition_runtime_mutate` (`action=job.list\|job.enqueue\|workflow.run\|…`) |
| Calendar | `cognition_calendar_*` — [calendar.md](calendar.md) |
| MCP | `cognition.mcp.*` — [mcp-gateway-setup.md](../mcp-gateway-setup.md) |
| UI present | `cognition_ui_present` — emits `ui_artifact` on stream |
| Web | `cognition_web_search` — all surfaces; BrowserHost → lite → Grapheme chain |
| Browser fetch | `cognition_browser_fetch` — gated on `supports_browser_host` |
| Finish | `cognition_finish` — ends tool loop |

---

## MCP vs built-ins

MCP tools are proxied through the gateway (`http://127.0.0.1:7420` default). Policy evaluation: `POST /v1/mcp/policy/evaluate`.

Capabilities catalog: `GET /v1/capabilities` — SDK `capabilities().list()`.

---

## Integrator guidance

- **HTTP-only clients** do not invoke tools directly; they send prompts via interactive turn or jobs API.
- **Custom UIs** should handle stream events (`tool_*`, `ui_artifact`, `artifact_updated`, `browser_challenge`) — [custom-chat-ui.md](../cookbook/custom-chat-ui.md).

---

## Registered client tools

Native integrations can keep a capability in their own runtime while allowing
the daemon's model turn to call it. A client registers definitions with
`POST /v1/clients/register`, including a `channel_surface` such as `browser`,
`vscode`, or `obsidian`. The daemon adds those definitions only to matching
turns and routes model invocations through a pull queue:

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/v1/clients/register` | Register/refresh a client and its tool definitions |
| GET | `/v1/clients/{client_id}/tools/next?wait_ms=…` | Long-poll the next invocation; returns `null` when the wait expires |
| POST | `/v1/clients/{client_id}/tools/{request_id}/result` | Return `{ "output": … }` or `{ "error": "…" }` |

This is intentionally a client-pull protocol: browser and editor hosts do not
need to expose an inbound HTTP server. Registrations expire after a short idle
TTL, and individual calls have a bounded wait. Tool names are validated and
must not collide with daemon-local tools. Hosts should keep the `next` poll
running while their surface is available and complete every request they
receive.

The browser companion currently registers only the read-only
`browser_page_snapshot` tool. The current bridge requires
`effect_class="external_read"`; write and side-effecting tools need an
explicit approval model before they are exposed.
