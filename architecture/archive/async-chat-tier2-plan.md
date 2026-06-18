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

## Phase 2b — Home turn-centric store ✅

- `Map<turnId, TurnTicketState>` in chat store
- Messages linked via `turnId` on assistant bubbles
- `/ask` → `POST /v1/turns` with `mode: background` + SSE attach
- Reattach prefers live interactive ticket; background tickets bump pulse

---

## Phase 2c — Composer queue ✅

- **Composer always open** — `composerBlocked` is always false
- **Fork policy** — new messages while an interactive turn streams become `background` tickets automatically
- **Turn-id SSE routing** — stream events update the correct bubble via `turns` map, not a single `assistantId`
- **Multi-stream Tauri bridge** — one SSE listener per `turn_id` (`interactive_streams` map)
- **Reattach all active tickets** — `listSessionTurns` + attach each stream on reconnect
- **Mobile notifications** — terminal, worker handoff, budget approval pushes

---

## Phase 2d — Legacy path retirement ✅

- `POST /v1/jobs/ask` → `spawn_turn_ticket` background mode (queue label `turn-ticket`)
- Home uses `createTurnTicket` exclusively (no poll-only ask UX)
- `GET /v1/jobs/{id}/result` retained for API clients and card inspector deep links

---

## Key files

| Area | Path |
|------|------|
| Turn ticket model | `src/turn_ticket.rs` |
| Daemon routes | `src/bin/medousa_daemon.rs` |
| Stream + ask mirror | `src/agent_runtime/daemon_interactive_turn.rs` |
| Chat turn map | `apps/medousa-home/src/lib/stores/chat.svelte.ts` |
| Multi-stream SSE | `apps/medousa-home/src-tauri/src/daemon/mod.rs` |
| API types | `src/daemon_api.rs` |

---

## Next: Tier 3

See [async-chat-unlock-plan.md](async-chat-unlock-plan.md) and [turn-worker-bus-plan.md](turn-worker-bus-plan.md).
