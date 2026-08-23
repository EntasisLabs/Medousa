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
| Store | `cognition_store_read` / `cognition_store_write` (`action=vault.read\|artifacts.write\|…`) — [vault.md](vault.md), [artifacts.md](artifacts.md) |
| Capability | `cognition_capability` (`action=capability.find\|grapheme.invoke\|mcp.invoke\|…`) — catalog, MCP, and Grapheme discover/run |
| Schema | `cognition_schema` (`domain` + `types=[...]`) — batched typed action parameter schemas |
| Runtime | `cognition_runtime_query` / `cognition_runtime_mutate` (`action=job.list\|job.enqueue\|workflow.run\|…`) |
| Turn | `cognition_turn` (`action=turn.finish\|turn.checkpoint\|turn.begin_work\|…`) |
| Memory | `cognition_memory_query` / `cognition_memory_mutate` (`action=memory.context\|memory.store\|…`) |
| Identity | `cognition_identity_query` / `cognition_identity_mutate` (`action=identity.recall\|identity.remember\|…`) |
| Calendar | `cognition_calendar_query` / `cognition_calendar_mutate` (`action=calendar.list\|calendar.create\|…`) — [calendar.md](calendar.md) |
| Workshop | `cognition_workshop_query` / `cognition_workshop_mutate` (`action=workshop.status\|workshop.spawn\|workshop.cancel\|workshop.steer`) |
| MCP | `cognition.mcp.*` — [mcp-gateway-setup.md](../mcp-gateway-setup.md) |
| UI present | `cognition_ui_present` — emits `ui_artifact` on stream |
| Web | `cognition_web_search` — all surfaces; BrowserHost → lite → Grapheme chain |
| Browser fetch | `cognition_browser_fetch` — gated on `supports_browser_host` |
| OpenShell secrets | `cognition_openshell_request_secret` — trusted UI prompt; returns an opaque one-use grant, never the credential value |
| Grapheme secrets | `cognition_grapheme_request_secret` — trusted UI prompt; authorizes an ephemeral credential capability for one native run |
| Finish | `cognition_turn action=turn.finish` — ends tool loop |

---

## MCP vs built-ins

MCP tools are proxied through the gateway (`http://127.0.0.1:7420` default). Policy evaluation: `POST /v1/mcp/policy/evaluate`.

Capabilities catalog: `GET /v1/capabilities` — SDK `capabilities().list()`.

---

## Integrator guidance

- **HTTP-only clients** do not invoke tools directly; they send prompts via interactive turn or jobs API.
- **Custom UIs** should handle stream events (`tool_*`, `ui_artifact`, `artifact_updated`, `browser_challenge`, `secret_request`) — [custom-chat-ui.md](../cookbook/custom-chat-ui.md).
- A `secret_request` event contains metadata only. Collect the value outside
  the transcript and submit it to the native-only fulfill endpoint. Never turn
  it into a user chat message or a tool argument.

## OpenShell credential grants

`cognition_openshell_request_secret` accepts a provider profile id, credential
environment key, short label, and reason. On a trusted interactive surface it
publishes `secret_request`, waits for the native UI, and returns an opaque,
short-lived, session-bound `sgrant-*` id. The model passes that id once through
`cognition_openshell_sandbox_run.secret_grant_ids`.

The daemon resolves grants to non-secret OpenShell provider names before it
serializes the sandbox job. Sandbox creation always uses
`--no-auto-providers`, so unrelated daemon-host credentials are not discovered,
and attaches only the providers authorized by those grants. OpenShell supplies
placeholder values inside the sandbox and performs endpoint-bound proxy
substitution. Provider binding does not widen sandbox network policy; both
checks must allow a request. Medousa verifies the gateway-global
`providers_v2_enabled` setting before prompting, provisioning, and consuming a
grant; an absent, false, or unreadable setting fails closed. Before prompting,
it also verifies that the requested credential key belongs to the named
provider profile and that the profile has an endpoint binding.

## Grapheme credential grants

`cognition_grapheme_request_secret` accepts an uppercase credential key, label,
reason, and up to 16 exact HTTPS authorities in `allowed_hosts`. The trusted UI
shows those hosts before collecting the value. On approval, the host receives a
session-bound `sgrant-*` id and passes it once in `secret_grant_ids` on
`cognition_capability` action `grapheme.invoke`.

Secret grants require an inline script. They are rejected for stored templates
and are not remembered as the last Grapheme source, which prevents accidental
promotion into a saved or recurring workflow. Before enqueue, Medousa replaces
each grant in the source with a daemon-only run alias. The durable job's only
credential coordinates are those aliases and a short-lived internal run token;
it contains neither grants nor values. Before execution the workflow engine
removes the token from initial state and moves the zeroizing credential into a
thread-local capability scope.

The native `grapheme/secrets` host module exposes:

- `get_secret_handle(name: "sgrant-…")`, returning a run-only `gsecret-*`
  handle and logical credential name;
- `sign_request(secret: handle, payload: ...)`, returning an HMAC-SHA256
  signature; and
- `medousa.authorized_http(secret: handle, url: "https://…", ...)`, attaching
  bearer or custom-header authentication only for an approved exact host.

`authorized_http` requires HTTPS, disables redirects, allows at most eight
calls per run, bounds response size and time, and redacts the credential from
returned bodies and transport errors.
Attempts with no active scope, a reused grant, the wrong session or runtime, or
an unapproved host fail closed. Raw credential bytes never enter Grapheme
source, state, VM values, tool output, or Stasis persistence.

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
