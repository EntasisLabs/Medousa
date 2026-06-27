# Agent Browser Host

**Status:** Accepted (v1 in-process)

## Problem

Normie-friendly web search without API keys, Docker, or Grapheme discovery friction. Telegram/TUI must not advertise browser-only tools. CAPTCHA and rate limits need human handoff, not turn failure spirals.

## Decision

Browser is a **provider backend + client UX**, not a parallel tool ecosystem:

| Tool | Visibility |
|------|------------|
| `cognition_web_search` | All surfaces (binding chain differs) |
| `cognition_browser_fetch` | `supports_browser_host=true` only |
| CAPTCHA handoff | SSE `browser_challenge` + UI panel (not a tool) |

Gating mirrors `TurnSurfaceContext.supports_ui_artifacts`: clients advertise `supports_browser_host`; the daemon never infers from channel name.

## Execution chain (`mode=search`)

```mermaid
flowchart LR
  A[cognition_web_search] --> B{supports_browser_host?}
  B -->|no| D[browser_host_lite DDG HTML]
  B -->|yes desktop| C[BrowserHost :7422]
  B -->|yes iOS| E[client WebView + session complete]
  C --> D
  D --> F[Grapheme web.* last resort]
```

1. **BrowserHost** — Home desktop in-process HTTP on `127.0.0.1:7422`
2. **Client-executed** — `home-ios`: daemon session + SSE navigate/challenge → WebView → `POST /v1/browser/sessions/{id}/complete`
3. **Lite fallback** — `medousa-browser-lite` DDG HTML parse + 30m cache (daemon, always available)
4. **Grapheme** — existing capability bindings (discovery ops filtered from fallback chain)

## Session model

- Daemon stores in-memory `BrowserSession` records (`src/browser_sessions.rs`)
- Challenge: tool returns `challenge_required`; SSE emits `browser_challenge` with `browser_session_id` + `browser_challenge_url`
- Resume: desktop → BrowserHost `POST /v1/sessions/{id}/resume`; iOS → daemon `POST /v1/browser/sessions/{id}/complete`

## Surfaces

| Surface | `supports_browser_host` | Browser execution |
|---------|-------------------------|-------------------|
| `home-desktop` | true when `:7422/health` ok | Local BrowserHost |
| `home-ios` | true | Client WebView |
| `telegram`, `tui`, ingest | false | Lite + Grapheme only |

## Out of scope (v1)

- Separate browser sidecar binary
- `cognition_browser_act` / form automation
- Playwright/CDP, SearXNG, Google-first SERP

## References

- Shared lite crate: `crates/medousa-browser-lite`
- Home service: `apps/medousa-home/src-tauri/src/browser_host.rs`
- Daemon routes: `src/browser_handlers.rs`
- Tool gating: `src/agent_runtime/turn_worker/registry.rs`, `src/tool_bootstrap.rs`
