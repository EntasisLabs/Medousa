# ACP external agents (bones)

**Audience:** engineers wiring channels to Cursor / Codex via the SDK + daemon

Any Medousa **channel** talks to the daemon **through the Medousa SDK** (`client.agents()`). The daemon owns agent runtimes — Medousa-native (`/v1/turns`) or external ACP (`/v1/agents`). External agents reach Medousa space via the [MCP server](mcp-server-setup.md).

See [ADR-008](../architecture/decisions/adr-008-hot-swappable-agent-runtime.md).

## SDK

```rust
let runtimes = client.agents().list_runtimes().await?;
let session = client
    .agents()
    .create_session(&CreateAgentSessionRequest {
        session_id: "…".into(),
        runtime: "cursor".into(),
        prompt: Some("hello".into()),
        cwd: None,
        command: None,
        args: None,
        surface: None,
    })
    .await?;
// SSE: client.agents().stream(session.stream_url)
```

Python: `client.agents().list_runtimes()` / `create_session(...)`.

Home: Tauri commands → `client().agents()` only (`daemon/agents.rs`).

## Daemon routes

| Method | Path |
|--------|------|
| GET | `/v1/agents/runtimes` |
| POST | `/v1/agents/sessions` |
| POST | `/v1/agents/sessions/{id}/prompt` |
| GET | `/v1/agents/sessions/{id}/stream` |
| POST | `/v1/agents/sessions/{id}/cancel` |
| GET | `/v1/agents/permission-requests` |
| POST | `/v1/agents/permission-requests/{id}/approve\|deny` |

## ACP crate

`crates/medousa-acp-client` — `ExternalAcpClient` spawns Cursor (`agent acp`) or the Codex ACP adapter (`codex-acp`, or `npx -y @agentclientprotocol/codex-acp` — stock `codex` has no `acp` subcommand). Missing CLI → stub bridge. Spawn/handshake failures return errors (no silent stub). Handshake: `initialize` → `session/new` → `session/prompt`; streams `session/update` chunks and replies to `session/request_permission`. Force stub: `MEDOUSA_ACP_FORCE_STUB=1`. Demo permissions: `MEDOUSA_ACP_STUB_PERMISSION=1`. Permission wait timeout (default-deny): `MEDOUSA_ACP_PERMISSION_TIMEOUT_SECS` (default `300`).

## Account sign-in (0.7.0)

External runtimes need the vendor CLI signed in **before** `create_session`. Medousa orchestrates the official login — credentials stay in the vendor's own store (`~/.codex/…`, Cursor's store), never in Medousa.

| Runtime | Install | Sign-in | Sign-out |
|---------|---------|---------|----------|
| **ChatGPT / Codex** | Connections → **Install** (official Codex installer). ACP uses `codex-acp` / `npx -y @agentclientprotocol/codex-acp` | `codex login` (browser) or `codex login --device-auth` | `codex logout` |
| **Cursor** | Connections → **Install** (official Cursor Agent installer) | Prefer `cursor agent login` (falls back to `agent login`); same auth store | `cursor agent logout` / `agent logout` |

Home: **Settings → Connections** installs missing CLIs via the vendor installers, runs login via Tauri (`account_connections.rs`), and probes status without reading tokens. The daemon surfaces it on each runtime:

```jsonc
// GET /v1/agents/runtimes → AgentRuntimeInfo
{
  "kind": "codex",
  "binary_present": true,
  "auth_status": "signed_in",   // signed_out | signed_in | unknown
  "auth_detail": null
}
```

`POST /v1/agents/sessions` returns **401** when the binary is present but signed out — surface a sign-in CTA rather than a generic spawn error. Cursor auth is probed via `agent status --format json` (tokens live in the OS keychain, not a file Medousa can read); Codex still uses `~/.codex` file presence only. Raw tokens are never read into Medousa logs.

## Thinking / reasoning traces

`agent_thought_chunk` updates are routed to the chat stream's `reasoning_delta` channel (event_type `reasoning_delta`, phase `thinking`), separate from `content_delta`. Home accumulates them into `ChatMessage.reasoning` and renders the collapsed **thinking tray** (`AssistantThinking.svelte`) above the answer — same treatment as native Medousa reasoning. Missing CLI → `create_session` errors loudly (no stub fallback).

## Stasis waitable turns (0.8)

External ACP sessions still enter through `/v1/agents` (SDK façade). When a Stasis job uses `workflow.stasis.agent_turn.waitable`, the daemon parks on a **process-local** `TurnWaitStore` until ACP completion feeds `AgentEventIngress`:

```text
/v1/agents prompt/stream
        → medousa-acp-client
        → on Done / Error / Cancel → AgentEnvelope (AcpAgentMessageCodec)
        → WaitCorrelatingAgentEventIngress → TurnWaitStore.complete
        → waitable job unparks (Deferred → success/fail)
```

Limits:

- Wait store is **not durable** across daemon restarts (Stasis 0.8).
- Correlate waitable `turn_id` with the Medousa `agent_session_id` when enqueueing so ACP terminals complete the right wait.
- Native Medousa remains on `/v1/turns`; do not move the local tool-loop onto waitable turns.

MCP: external agents reach vault/context via [Medousa MCP server](mcp-server-setup.md). Stasis builder allowlists read-oriented export names (`vault_list`, `vault_read`, `vault_search`, …) to limit recursion.

## Forge-bound sessions (0.8)

Set `work_id` on `POST /v1/agents/sessions` to run the ACP session inside a
Forge undertaking's governed worktree instead of a bare chat:

```text
register + provision via /v1/forge   →   work item Ready with a worktree
POST /v1/agents/sessions { work_id, runtime: "cursor" | "codex" }
        → cwd forced to the worktree, lease begins on first prompt
prompt(s) → heartbeats + command-log staging beside the chat SSE stream
POST /v1/forge/leases/{id}/complete → seal + evidence (explicit — never auto)
cancel    → interrupt_attempt; ACP *wire* sessionId stashed as ResumeSupported
resume    → POST /v1/agents/sessions { work_id } (or resume_provider_token)
            → session/resume|load, fall back to session/new; response.resumed
error     → fail_attempt; work returns to Ready
```

Undertaking-backed sessions are opt-in: plain chats (no `work_id`) never
touch Forge. Seal is always an explicit caller decision — the adapter only
makes the executor honest. See [Forge — ACP binding](../engine/forge.md#acp-binding-cursorcodex-executors).

## Cut line

| In 0.6 Dynamic | Later |
|----------------|--------|
| SDK + daemon + Home Runtime select | Polished pickers on every channel |
| Cursor + Codex ACP pump (`session/*`) | Broader ACP vendor quirks |
| Permission approve/deny + Home bar + timeout | UX parity with native tool cards |
| Stasis 0.8 ingress + waitable correlation | Durable turn wait store |
