# Async chat Tier 3 — Turn worker bus adapter

Tier 3 connects Home chat to the **daemon turn worker bus** (Phases 0–2 already in `agent_runtime/turn_worker/`). Chat observes **workspace + session** for synthesis delivery, not only live interactive SSE.

---

## Problem

After `worker_ack`, the composer is open (Tier 0–2). Synthesis still runs on the host stream, but:

- WebView refresh drops SSE
- User may fork another turn (Tier 2c)
- Workspace card reaches `wrapping_up` / `done` while chat still shows the handoff stub

---

## Phase 3a — Worker link + workspace bridge ✅

- `WorkerLink` map in chat store (`workId → parentTurnId, messageId, sessionId`)
- Workspace `turn_worker` card upserts link via `correlation_id` (= parent daemon turn id)
- `wrapping_up` → update handoff bubble status ("Synthesizing…")
- `done` → deliver synthesis from `result_excerpt` or session history tail
- `noteBackgroundSettled` when worker terminal

---

## Phase 3b — SSE work_id on worker_ack ✅

- `InteractiveTurnStreamEvent.work_id` optional field
- Immediate `linkWorkerFromStream` on handoff without waiting for workspace sync

---

## Phase 3c — Session hydrate recovery ✅

- After `ensureSessionHydrated` / reconnect: scan workspace turn_worker cards + deliver pending syntheses

---

## Out of scope (daemon Phase 3 Stasis)

- Durable Stasis worker jobs — see [turn-worker-bus-plan.md](turn-worker-bus-plan.md) Phase 3
- Telegram-specific outbox — adapters share the same bus events

---

## Key files

| Area | Path |
|------|------|
| Chat worker bridge | `apps/medousa-home/src/lib/stores/chat.svelte.ts` |
| Workspace → chat | `apps/medousa-home/src/lib/stores/workspace.svelte.ts` |
| SSE work_id | `src/interactive_turn_runtime.rs`, `stream_sink.rs`, `turn_orchestrator.rs` |
| Worker cards | `src/workspace/card.rs` (`TurnWorker`) |
