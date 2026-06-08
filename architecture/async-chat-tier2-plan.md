# Async chat Tier 2 — Unified TurnTicket

Tier 1 gave us session-scoped reconnect + cancel. Tier 2 collapses interactive turns and `/ask` jobs into one **TurnTicket** type with a shared SSE contract.

---

## Phase 2a — Daemon foundation ✅

- `turn_ticket.rs` — durable registry (`by_id` + interactive mutex per session)
- `POST /v1/turns` — `mode: interactive | background`
- `GET /v1/turns/{id}` — ticket snapshot
- `GET /v1/sessions/{id}/turns?active=true` — active tickets for reconnect
- Background mode registers `ask_job_store` + workspace card, streams via same SSE path
- `POST /v1/interactive/turn` — thin wrapper over interactive ticket
- Tier 1 `active-turn` endpoints preserved (interactive ticket only)

---

## Phase 2b — Home turn-centric store ✅ (this sprint)

- `Map<turnId, TurnTicketState>` in chat store
- Messages linked via `turnId` on assistant bubbles
- `/ask` → `POST /v1/turns` with `mode: background` + SSE attach
- Reattach prefers live interactive ticket; background tickets bump pulse

---

## Phase 2c — Composer queue (next)

- Composer always accepts input; queue or fork when interactive turn in flight
- Multiplexed session SSE or multi-listener Tauri bridge (today: single `interactive_cancel` slot)
- Notifications on terminal / blocked / worker done (mobile)

---

## Phase 2d — Legacy path retirement (next)

- `POST /v1/jobs/ask` → wrapper over background TurnTicket for API clients
- Deprecate poll-only ask UX in Home (keep job result for deep links)

---

## Key files

| Area | Path |
|------|------|
| Turn ticket model | `src/turn_ticket.rs` |
| Daemon routes | `src/bin/medousa_daemon.rs` |
| Stream + ask mirror | `src/agent_runtime/daemon_interactive_turn.rs` |
| Chat turn map | `apps/medousa-home/src/lib/stores/chat.svelte.ts` |
| API types | `src/daemon_api.rs` |
