# Async chat unlock — internal plan

Medousa’s runtime (daemon SSE, workspace, jobs, workers, continuations, notifications) is largely **event/async**. Interactive chat is still a **sync contract**: one turn, one SSE, composer blocked until `terminal`.

This plan unlocks chat incrementally without waiting for a full TurnTicket rewrite.

---

## Current sync contract

| Layer | Constraint | Key files |
|-------|------------|-----------|
| **Home chat store** | `isStreaming` gates composer, session switch, reload | `apps/medousa-home/src/lib/stores/chat.svelte.ts` |
| **Tauri SSE** | Single `interactive_cancel` slot — new stream replaces old | `apps/medousa-home/src-tauri/src/daemon/mod.rs` |
| **Agent runtime** | Singleton `turn_scope` — concurrent turns would clobber tool context | `src/tui/runtime_services.rs`, `src/tools.rs` |
| **Non-terminal SSE** | `worker_ack`, `budget_approval` leave chat blocked | `chat.applyStreamEvent` |

**Async escape hatch already exists:** `/ask` and `/daemon ask` enqueue background jobs without blocking the composer (`ChatPanel.svelte`).

---

## Tier 0 — Composer handoff (high ROI) ✅

**Goal:** Stop freezing chat on worker delegation and budget pause; show background activity pulse.

### Changes

1. Split **live stream** vs **background activity** in chat store:
   - `liveStreamActive` — blocks composer only while attached to live token/tool-loop SSE
   - `backgroundActivity` — counter for worker handoff / operator pause; drives pulse, not composer lock

2. **Release composer** on SSE event types:
   - `worker_ack` — host delegated to turn worker; finalize bubble, increment background pulse
   - `budget_approval` — turn paused for round extension; unlock composer, pulse until resolved

3. **Clear pulse** when:
   - Terminal SSE after a handoff (budget resume → complete)
   - Workspace card settles (worker → `wrapping_up`/`done`; budget → no longer `needs approval`)

4. **Orphan stream attach** — if SSE resumes after handoff with no active assistant bubble, attach deltas to a new bubble (budget approve → continue)

5. **UI** — pulse badge in chat header; composer uses `composerBlocked` (= `liveStreamActive`) not raw streaming flag

### Out of scope (Tier 0)

- Daemon cancel API, session turn registry, SSE reconnect
- Parallel turns per session
- Unifying ask jobs and interactive turns

---

## Tier 1 — Session turn registry (medium) ✅

Mirror ingest `active_ingest_jobs`:

```
session_id → { turn_id, stream_url, phase, composer_handoff, started_at }
```

- `GET /v1/sessions/{id}/active-turn` — reconnect after WebView refresh
- `POST /v1/sessions/{id}/active-turn` — daemon-side cancel (best-effort; in-flight model work may finish)
- Per-session turn mutex — `409 Conflict` if a live turn already exists

**Files:** `session_active_turn.rs`, `medousa_daemon.rs`, `daemon_interactive_turn.rs` (session hooks), Home `chat.svelte.ts` + `workshopConnection.ts` reattach on hydrate.

---

## Tier 2 — Unified TurnTicket (sprint) ✅

Collapse interactive turn + ask job into one durable **TurnTicket** — see [async-chat-tier2-plan.md](async-chat-tier2-plan.md).

- Stream to attached clients (SSE), workspace cards, session history incrementally
- Chat store turn-centric; composer always open with **fork policy**
- `/ask` and `POST /v1/jobs/ask` both use background TurnTicket + SSE

---

## Tier 3 — Turn worker bus adapter ✅

Durable host/worker synthesis delivery via **workspace + session**, not only interactive SSE — see [async-chat-tier3-plan.md](async-chat-tier3-plan.md).

- `work_id` on SSE `worker_ack` for immediate worker ↔ turn linking
- Chat `WorkerLink` map; workspace `turn_worker` cards drive synthesizing/done bubble updates
- Session hydrate recovery after reconnect

---

## Event semantics reference

| SSE `event_type` | `terminal` | Tier 0 chat behavior |
|------------------|------------|----------------------|
| `content_delta` | false | Live stream; composer blocked |
| `final` | true | Finish bubble; clear live |
| `worker_ack` | false | Handoff; **unlock composer**; pulse++ |
| `budget_approval` | false | Handoff; **unlock composer**; pulse++ |
| `final_pending` | — | Legacy; redirects to `turn_progress` |
| `needs_input` | true | Finish; unlock |

---

## Rollout

| Tier | Scope | Status |
|------|--------|--------|
| **0** | Composer handoff + pulse + stuck-state fixes | ✅ |
| **1** | Session registry, reconnect, daemon cancel | ✅ |
| **2** | TurnTicket unified (2a–2d) | ✅ |
| **3** | Turn worker bus adapter (workspace + session synthesis) | ✅ |

---

## Key files

| Area | Path |
|------|------|
| Chat state | `apps/medousa-home/src/lib/stores/chat.svelte.ts` |
| Session registry | `src/session_active_turn.rs`, `src/turn_ticket.rs` |
| Chat UI | `apps/medousa-home/src/lib/components/chat/ChatPanel.svelte` |
| SSE wiring | `apps/medousa-home/src/lib/workshopConnection.ts` |
| Stream events | `src/interactive_turn_runtime.rs` |
| Turn execution | `src/agent_runtime/daemon_interactive_turn.rs`, `turn_orchestrator.rs` |
| Workspace pulse clear | `apps/medousa-home/src/lib/stores/workspace.svelte.ts` |
| Prior art | `architecture/interaction-and-state-model.md`, `architecture/turn-worker-bus-plan.md` |
