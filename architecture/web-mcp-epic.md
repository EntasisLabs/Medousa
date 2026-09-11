# Web MCP and MCP Apps epic

Status: active  
Branch: `codex/web-mcp`  
Owner: Medousa MCP gateway + chat surfaces

## Outcome

Medousa connects reliably to hosted MCP servers on desktop and mobile, keeps
remote credentials out of configuration and UI responses, preserves modern MCP
tool contracts, and can eventually render MCP Apps as isolated interactive chat
parts.

Liquid remains Medousa's trusted, native declarative UI. MCP Apps are untrusted
third-party HTML and must always cross a sandboxed host bridge; arbitrary app
HTML is never translated into or mounted as Liquid.

## Decisions

- Use the official Rust SDK for stdio and Streamable HTTP framing, lifecycle,
  negotiated protocol headers, request-scoped SSE, pagination, redirects, and
  expired-session recovery.
- Retain the hand-written SSE client only as an explicitly legacy transport.
- Target the SDK's stable `2025-11-25` lifecycle before opting into the
  `2026-07-28` discovery lifecycle. Add the latter after public-server
  interoperability coverage exists.
- Treat server tool annotations as untrusted hints. They may raise Medousa's
  effect classification, but never lower a conservative classification.
- Store bearer and OAuth credentials in Medousa's typed secret store. Config
  and UI receive presence booleans only.
- Require HTTPS for internet endpoints. Plain HTTP is limited to loopback and
  unauthenticated private-network development servers.
- Route every MCP App tool request through the existing gateway policy and
  approval boundary.

## Delivery slices

### Slice 0 — contract and test matrix

- Record architecture, security boundaries, protocol targets, and checkpoints.
- Establish unit tests for URL policy, tool metadata preservation, and legacy
  SSE framing.
- Update the operator setup guide.

### Slice 1 — official transports

- Replace hand-written Streamable HTTP with the official SDK transport.
- Replace hand-written stdio framing with the official child-process transport.
- Negotiate the stable current protocol, paginate `tools/list`, preserve rich
  tool results, recover expired sessions, and terminate sessions cleanly.
- Keep legacy SSE compatibility isolated.

### Slice 2 — remote security and setup UX

- Move static bearer tokens to typed secret storage and migrate plaintext
  values during config load.
- Return only `bearer_token_configured` to management surfaces.
- Enforce URL/HTTPS policy before OAuth or token attachment.
- Show secure-token presence and explicit removal in Settings.

Checkpoint: connect unauthenticated, bearer, and OAuth servers on macOS, iPhone,
and iPad; exercise JSON and request-scoped SSE responses; verify no secrets in
TOML, UI payloads, or logs.

### Slice 3 — durable runtime sessions

- Add per-server connection actors shared by discovery and invocation.
- Reconnect with bounded backoff and invalidate connections on credential or
  configuration changes.
- Consume `tools/list_changed` and retain the periodic refresh as fallback.

### Slice 4 — richer policy and guidance

- Carry input/output schemas, annotations, icons, and `_meta` through the
  catalog and agent discovery contract.
- Use annotations only to promote risk; expose idempotence/open-world hints for
  planning and approval copy.
- Improve setup and invocation errors for auth, scope, protocol, and transport
  failures.

Checkpoint: long-running/stateful servers, catalog changes, token refresh, and
reconnects remain reliable across sleep/wake and network changes.

### Slice 5 — MCP Apps contracts

- Advertise the `io.modelcontextprotocol/ui` client extension.
- Add resources list/read support and preserve `_meta.ui.resourceUri`.
- Define a durable, secret-free MCP App message-part descriptor.

### Slice 6 — sandboxed read-only rendering

- Render app resources in a sandboxed iframe/webview chat part.
- Enforce declared CSP origins, message-source validation, and immutable
  resource digests.
- Present inline/resizable on desktop and iPad; use an expandable sheet on
  phone layouts.

Checkpoint: official example apps render safely across platforms with no host
tool calls enabled.

### Slice 7 — interactive host bridge

- Implement the MCP Apps bridge for tool calls, chat messages, model-context
  updates, links, theme, locale, and fullscreen requests.
- Proxy calls through Medousa's policy engine and explicit approval UI.

### Slice 8 — durability and production hardening

- Persist/rehydrate app descriptors without tokens or executable host state.
- Add CSP, bridge, malformed-resource, navigation, and permission tests.
- Audit mobile memory, backgrounding, restoration, accessibility, and export
  behavior.

Checkpoint: adversarial app audit plus complete desktop/iOS/Android workflow.

## Slice 0 interoperability matrix

| Server | Authentication | Response | Platforms |
|---|---|---|---|
| Local fixture | none | JSON | macOS, CI |
| Local fixture | bearer | request SSE | macOS, CI |
| Public hosted fixture | none | JSON/SSE | macOS, iOS, Android |
| Public hosted fixture | OAuth | JSON/SSE | macOS, iOS, Android |
| Legacy fixture | optional bearer | SSE + message POST | macOS, iOS, Android |

Each fixture must cover initialization, multi-page tool discovery, one tool
call, structured and media content, tool errors, timeout/cancellation, and
credential redaction. Stateful fixtures additionally cover session expiry and
clean termination.
