# External coding agents

**Audience:** engineers wiring external coding agents through the SDK and daemon

Any Medousa **channel** talks to the daemon **through the Medousa SDK** (`client.agents()`). The daemon owns agent runtimes — Medousa-native (`/v1/turns`) or external coding agents (`/v1/agents`). External agents reach Medousa space via the [MCP server](mcp-server-setup.md).

See [ADR-008](../architecture/decisions/adr-008-hot-swappable-agent-runtime.md).

## SDK

```rust
let runtimes = client.agents().list_runtimes().await?;
let session = client
    .agents()
    .create_session(&CreateAgentSessionRequest {
        session_id: "…".into(),
        runtime: "hermes".into(),
        prompt: Some("hello".into()),
        cwd: None,
        command: None,
        args: None,
        surface: None,
        work_id: None,
        resume_provider_token: None,
        code_context: None,
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

## Runtime transports

`crates/medousa-acp-client` owns the launch profiles and transport. ACP agents
use `ExternalAcpClient`; Codex uses its native app-server protocol and Muse Code
uses its native server client. All use the same daemon agent-session interface:

| Runtime | Default launch | Notes |
|---------|----------------|-------|
| **Cursor** | `agent acp` | Override: `MEDOUSA_ACP_CURSOR_COMMAND` / `MEDOUSA_ACP_CURSOR_ARGS` |
| **Codex** | `codex app-server` | Native protocol from the installed Codex CLI; no ACP adapter or npm bridge. `MEDOUSA_ACP_CODEX_COMMAND` remains a compatibility override for the Codex executable |
| **Claude Code** | `claude-agent-acp`, or pinned `npx -y @agentclientprotocol/claude-agent-acp@0.94.0` | Uses Claude Code account/settings; configure on the workshop host |
| **Devin** | `devin acp` | Uses the configured Devin CLI |
| **Pi** | `pi-acp`, or pinned `npx -y pi-acp@0.0.34` | Requires Pi v0.81.0+ and native provider setup |
| **Muse Code** | `muse serve` | Native server bridge; runtime ID `muse_code`, separate from Muse WhatsApp |
| **Hades** | `hades acp` | Linux workshop; uses native Hades configuration |
| **Hermes** | `hermes-acp`, or `hermes acp` | Sets `HERMES_ACP_SKIP_CONFIGURED_MCP=1` so Medousa owns MCP via `session/new`. Override: `MEDOUSA_ACP_HERMES_COMMAND` / `MEDOUSA_ACP_HERMES_ARGS` |

For ACP runtimes, the protocol sequence below applies. Codex and Muse Code map
their native server events to the same daemon session interface. Home and SDK
callers remain on `/v1/agents`; they do not speak vendor wire protocols directly.
Codex model and reasoning choices are normalized into the existing session config
options instead of a hardcoded Home model list. Protocol fixtures should follow
the installed Codex CLI's app-server schema when updating compatibility tests.

Codex reuses the installed CLI's saved ChatGPT account. Startup checks
`account/read` without requesting token refresh and rejects API-key accounts
before creating a thread; it never starts login or logout implicitly. Legacy
`MEDOUSA_ACP_CODEX_COMMAND` / `MEDOUSA_ACP_CODEX_ARGS` overrides must now launch
native `codex app-server --stdio`, rather than an ACP wrapper. Command/file
approvals preserve the native choices. Structured questions, MCP elicitation,
and additional permission-grant forms fail explicitly until their dedicated UI
is supported. Resuming a thread that already has an active turn also reports a
blocker instead of silently losing its pending events.

Missing CLI → clear setup error (no silent stub). Spawn/handshake failures return errors. Handshake: `initialize` → `session/new` → optional `session/set_config_option` → `session/prompt`; streams `session/update` chunks and replies to `session/request_permission`. Force stub: `MEDOUSA_ACP_FORCE_STUB=1`. Demo permissions: `MEDOUSA_ACP_STUB_PERMISSION=1`. Permission wait timeout (default-deny): `MEDOUSA_ACP_PERMISSION_TIMEOUT_SECS` (default `300`).

## Account sign-in (0.7.0)

External runtimes need the vendor CLI signed in / configured **before** `create_session`. For its existing Codex, Cursor, and Hermes integrations, Medousa orchestrates the official login — credentials stay in the vendor's own store (`~/.codex/…`, Cursor's store, `~/.hermes/…`), never in Medousa.

| Runtime | Install | Sign-in | Sign-out |
|---------|---------|---------|----------|
| **ChatGPT / Codex** | External Agents → **Install** (official Codex installer). Includes native `codex app-server`; no separate adapter | `codex login` (browser) or `codex login --device-auth` | `codex logout` |
| **Cursor** | External Agents → **Install** (official Cursor Agent installer) | Prefer `cursor agent login` (falls back to `agent login`); same auth store | `cursor agent logout` / `agent logout` |
| **Hermes** | External Agents → **Install** (official Hermes installer). Ensures ACP extras (`hermes acp` / `hermes-acp`) | `hermes acp --setup` (or `hermes model`) — multi-provider config in `~/.hermes` | No vendor logout; **Reconfigure** opens `hermes model` (Medousa never deletes `~/.hermes`) |

Home: **Settings → External Agents** retains install and sign-in actions for Codex, Cursor, and Hermes on a co-located desktop workshop. New runtime CLIs are installed and configured on the workshop host. Runtime discovery uses the daemon API without reading tokens. The daemon surfaces it on each runtime:

```jsonc
// GET /v1/agents/runtimes → AgentRuntimeInfo
{
  "runtime": "hermes",
  "binary_present": true,
  "auth_status": "signed_in",   // signed_out | signed_in | unknown
  "auth_detail": null
}
```

`POST /v1/agents/sessions` returns **401** when the binary is present but signed out — surface a sign-in CTA rather than a generic spawn error. Cursor auth is probed via `agent status --format json` (tokens live in the OS keychain, not a file Medousa can read); Codex still uses `~/.codex` / `codex login status`; Hermes uses `hermes status` when available plus presence of `~/.hermes/.env` / `config.yaml`. Raw tokens are never read into Medousa logs.

## Thinking / reasoning traces

`agent_thought_chunk` updates are routed to the chat stream's `reasoning_delta` channel (event_type `reasoning_delta`, phase `thinking`), separate from `content_delta`. Home accumulates them into `ChatMessage.reasoning` and renders the collapsed **thinking tray** (`AssistantThinking.svelte`) above the answer — same treatment as native Medousa reasoning. Missing CLI → `create_session` errors loudly (no stub fallback).

## Session history (transcript)

Each external-agent prompt also writes durable Medousa session turns (user at pump start; assistant on `Done` / idle complete / `Error`) via `session_writer` — same store as native `/v1/turns`. Reopen or daemon restart → `GET /v1/sessions/{id}/history` restores the transcript. Live SSE is unchanged.

**Forge resume ≠ transcript.** Codex `thread/resume`, or ACP `session/resume` / `session/load`, reattaches the vendor session inside a governed worktree; command-log staging is audit metadata. Chat text lives in Medousa session history only.

## Stasis waitable turns (0.8)

External-agent sessions still enter through `/v1/agents` (SDK façade). When a Stasis job uses `workflow.stasis.agent_turn.waitable`, the daemon parks on a **process-local** `TurnWaitStore` until ACP completion feeds `AgentEventIngress`:

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

Set `work_id` on `POST /v1/agents/sessions` to run the provider session inside a
Forge undertaking's governed worktree instead of a bare chat:

```text
register + provision via /v1/forge   →   work item Ready with a worktree
POST /v1/agents/sessions { work_id, runtime: "cursor" | "codex" | "hermes" | "claude" | "devin" | "pi" | "muse_code" | "hades" }
        → isolated lease acquired first, provider cwd forced to its worktree
prompt(s) → heartbeats + command-log staging beside the chat SSE stream
POST /v1/forge/leases/{id}/complete → seal + evidence (explicit — never auto)
cancel    → interrupt_attempt; vendor session token stashed as ResumeSupported
resume    → POST /v1/agents/sessions { work_id } (or resume_provider_token)
            → runtime-specific resume; response.resumed
error     → fail_attempt; work returns to Ready
```

Codex resumes its native thread with `thread/resume`; a rejected token is an
explicit error. ACP runtimes can fall back from `session/resume` / `session/load`
to `session/new` when the vendor cannot resume.

Undertaking-backed sessions are opt-in: plain chats (no `work_id`) never
touch Forge. Seal is always an explicit caller decision — the adapter only
makes the executor honest. See [Forge — ACP binding](../engine/forge.md#acp-binding-cursorcodex-executors).

## Existing runtime infrastructure

| Reused infrastructure | Remaining limitations |
|----------------|--------|
| SDK + daemon + Home Runtime select | Polished pickers on every channel |
| ACP pump (`session/*`) plus Codex and Muse Code native event bridges | Vendor-specific capability differences |
| Exact provider permission options + Home bar + timeout | Extension free-text/editor dialogs unsupported |
| Stasis 0.8 ingress + waitable correlation | Durable turn wait store |

## Expanded runtime setup and capabilities

Runtime discovery and authentication status belong to the connected daemon host.
The External Agents screen never substitutes the shell device's local credentials
for a remote workshop. Local installer/login/logout controls require a co-located
desktop workshop. New runtime CLIs are configured on the workshop using their
vendor's native setup; no Medousa release package or API-key fallback is implied.
Unknown auth remains unknown until the vendor can verify it.

The new ACP profiles accept `MEDOUSA_ACP_<RUNTIME>_COMMAND` and
`MEDOUSA_ACP_<RUNTIME>_ARGS` overrides, where runtime is `CLAUDE`, `DEVIN`, `PI`,
`MUSE_CODE`, or `HADES`. Pin bridge versions when installing them separately.
Pi's ACP bridge does not forward configured MCP servers or extension input/editor
dialogs and extension slash commands; reasoning shares its response stream.
Hades does not advertise session loading. A saved conversation therefore does not
promise that the provider execution can resume. Session capabilities returned by
the daemon describe the live connection; absent capabilities are not assumed.

Agent permissions can include exact provider options (`option_id`, `name`, `kind`).
Home preserves those choices rather than collapsing selection dialogs into an
approval boolean. The resolution carries the selected `option_id`; cancel remains
a separate outcome. Workshops without options keep the existing Allow/Deny UI.

All coding-agent chat remains on `/v1/agents`, with the existing project authority,
streaming, cancellation, and stored conversation paths. The current Home sender
accepts text for external runtime sessions and rejects attachments explicitly.
