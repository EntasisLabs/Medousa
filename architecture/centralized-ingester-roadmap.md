# Centralized Ingester/Router — Roadmap & Plan

> Created: 2026-05-30  
> Session: `medousa-ux`  
> Mood: Focused (autonomy=0.90, friction=0.10, logic=0.95, stability=0.95)

## Vision

One centralized ingester/router that picks up from any comms channel outbox and routes to the proper handler — making all channels behave identically to the TUI experience. Continuous single-chat history per channel+user pair until `/new` is sent.

## Core Principle

**Adapters are thin shells.** All business logic, session management, slash command handling, and response generation lives in a single daemon endpoint (`POST /v1/ingest`). Adapters only listen for incoming messages, forward them, and render responses.

## Architecture

```
                    ┌─────────────────────┐
                    │   Channel Adapters   │
                    │  (thin shells only)  │
                    │                      │
  Telegram ─────────┤    medousa_telegram  │
                    │  (listen + forward)  │
  Discord  ─────────┤    medousa_discord   │
                    │  (listen + forward)  │
  CLI      ─────────┤    medousa_cli       │
                    │  (listen + forward)  │
                    └────────┬────────────┘
                             │ POST /v1/ingest
                             ▼
              ┌───────────────────────────┐
              │   CENTRALIZED INGESTER    │
              │   (in medousa_daemon)     │
              │                           │
              │   POST /v1/ingest         │
              │   { channel, user_id,     │
              │     channel_id, text,     │
              │     attachments }         │
              │                           │
              │   Routes internally:      │
              │   ├─ /new     → reset     │
              │   ├─ /help    → respond   │
              │   ├─ /model   → config    │
              │   ├─ /depth   → config    │
              │   ├─ /history → list      │
              │   ├─ /stop    → cancel    │
              │   ├─ /regen   → rerun     │
              │   └─ text     → continuous│
              │         session ask       │
              └────────┬──────────────────┘
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
   Session Mgr    Slash Router   Ask Handler
   (map channel   (parse cmd,    (enqueue job
    + user →        route to      with session
    session id)     handler)      context)
```

## Session Mapping

- **Key**: `{channel_type}:{channel_id}:{user_id}`
- **Value**: Active `session_id` (UUID v4)
- Stored in-memory with optional SurrealDB persistence
- `/new` generates a fresh `session_id` for the same key
- Old sessions remain accessible via `/history`

## Ingester Request/Response

```rust
struct IngestRequest {
    channel: String,          // "telegram" | "discord" | "cli"
    user_id: String,          // "telegram:user:12345"
    channel_id: String,       // "telegram:chat:67890"
    text: String,
    attachments: Vec<Attachment>,
}

struct IngestResponse {
    session_id: String,
    turn_id: String,
    job_id: Option<String>,
    reply: String,            // immediate text or confirmation
    is_new_session: bool,
    stream_id: Option<String>,
    stream_url: Option<String>,  // SSE stream for job-backed asks
    stream_ready: bool,
}
```

## Slash Command Parity Map

| TUI Command        | Ingester Route        | Behavior                                      |
|--------------------|-----------------------|-----------------------------------------------|
| `/new`             | `/new`                | Reset session for this channel/user pair      |
| `/ask <prompt>`    | Plain text            | Continuous session ask (same as text)         |
| (plain text)       | Text                  | Continuous session ask                        |
| `/help`            | `/help`               | Show available commands                       |
| `/history`         | `/history`            | List recent sessions for this user            |
| `/name`            | `/name`               | Show or set global session display name (Surreal) |
| `/name <label>`    | `/name <label>`       | Name persists across TUI, Telegram, daemon API |
| `/model <name>`    | `/model`              | Switch model for this session                 |
| `/depth <mode>`    | `/depth`              | Switch response depth mode                    |
| `/stop`            | `/stop`               | Cancel current processing                     |
| `/regen`           | `/regen`              | Regenerate last response                      |
| `/health`          | `/health`             | Daemon health check                           |
| `/heartbeat`       | `/heartbeat`          | Daemon heartbeat status                       |

## Phased Implementation

### Phase 1 — Foundation ✅
- [x] Add `POST /v1/ingest` endpoint to `medousa_daemon`
- [x] Add session mapping table (channel+user ↔ session_id)
- [x] Implement basic ingester handler:
  - [x] Session lookup/creation
  - [x] `/new` command → reset session
  - [x] `/help` command → return help text
  - [x] Plain text → load session history, enqueue ask job with context
- [x] Create `IngestRequest` / `IngestResponse` types in `daemon_api.rs`
- [x] Expose ingest types from `lib.rs`

### Phase 2 — Adapter Thinning ✅
- [x] Strip `medousa_telegram` down to thin shell
  - [x] Remove command parsing
  - [x] Remove session logic
  - [x] Remove result polling (done by daemon)
  - [x] POST to `/v1/ingest`, render response
- [x] Strip `medousa_discord` down to thin shell (same pattern)
- [x] Strip `medousa_cli daemon-ask` to use ingester

### Phase 3 — Streaming Support ✅
- [x] Add SSE streaming to `/v1/ingest` for real-time responses
- [x] Adapters switch from poll to stream
- [x] Enable typing indicators on Telegram/Discord during processing

### Phase 4 — Full Feature Parity ✅
- [x] `/stop` command → cancel active job for session
- [x] `/regen` command → regenerate last turn
- [x] `/model` + `/depth` → runtime config changes per session

### Phase 5 — Outbox Channel Delivery ✅

See [outbox-channel-delivery-roadmap.md](outbox-channel-delivery-roadmap.md).

Wire Stasis outbox publish → internal webhook → channel dispatch so completed jobs actually deliver replies to Telegram/Discord.

### Phase 6 — Centralized Agent Runtime ✅ (superseded by dedicated track)

See [centralized-agent-runtime-roadmap.md](centralized-agent-runtime-roadmap.md) — **Phases 1–4 complete.**

All interactive surfaces now use `MedousaAgentRuntime` in the daemon. Ingest `agent_session` jobs and the bare LLM interactive-turn shortcut are retired. Remaining Stasis scheduler jobs: `/v1/jobs/prompt` and recurring materialization only.

- [x] `/history` → list & resume past sessions
- [x] Attachment/media support (`IngestAttachment` merged into ask prompts)
- [x] `/health` + `/heartbeat` ingester routes (daemon queries)
- [x] Heartbeat nudge forwarding per channel (optional env-gated adapter background poll)

---

## Design Decisions

1. **Session key uses channel+user**, not channel alone, so multiple users in the same group chat get their own sessions
2. **Old sessions persist** — `/new` just creates a new active mapping, doesn't delete history
3. **No adapter-level config duplication** — all policy, model, depth config lives in the daemon/ingester
4. **`/v1/jobs/ask` API** — same agent runtime as ingest; clients poll `/v1/jobs/{id}/result`
5. **TUI daemon-primary** — chat via `/v1/interactive/turn`; local runtime is offline/dev fallback only

## Check-in Points

- After Phase 1: Verify basic roundtrip (adapter → daemon → session → response)
- After Phase 2: Verify all Telegram/Discord commands work identically
- After Phase 3: Verify streaming latency matches TUI experience
- After Phase 4: Full parity acceptance test
