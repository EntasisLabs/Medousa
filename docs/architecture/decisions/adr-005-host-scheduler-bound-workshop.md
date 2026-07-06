# ADR-005: Host scheduler and bound workshop turns

**Status:** Accepted  
**Date:** 2026-07-02

## Context

The host console tool loop tried to be both a conversational partner and an execution engine. Prose-terminates and interim heuristics (`update_user`, `prose_requires_finish`) fought the model's natural "think then tool" rhythm. Workshop workers did not exhibit this because role is explicit: execute `WORKER_TASK`, call tools, `cognition_turn_finish`.

## Decision

1. **Host = scheduler** — hot lane for memory, identity, runtime, vault read, `cognition_turn_begin_work`, and parallel `cognition_spawn_turn_worker`. No environment/canvas/web/grapheme execution on host.
2. **Bound workshop** — `cognition_turn_begin_work(message, goal, intent?)` enqueues one async execution turn per session (reuses `run_worker_turn` + synthesis). Host ends with ack; principal sees ack → synthesis on same thread.
3. **Parallel worker** — unchanged (`cognition_spawn_turn_worker`) for heavy multi-topic research.
4. **Steering** — principal can inject messages into the active bound workshop via `POST /v1/sessions/{id}/workshop/steer`; workshop loop reads `[MEDOUSA_WORKSHOP_STEER]` each round.
5. **Deprecate** `cognition_turn_update_user` — workshop internal monologue replaces mid-turn host status tools.
6. **Host FSM** — cooperative prose on host (`host_scheduler_lane`); worker/workshop FSM unchanged.

## Consequences

**Positive**

- Canvas and multi-tool local work no longer fight host turn control.
- One Medousa voice; role split is scheduling, not personality.
- Composer stays open during bound workshop (handoff phase).

**Tradeoffs**

- Extra latency vs inline host execution (async job).
- One bound workshop per session at a time.
- Host must call `begin_work` with a concrete `goal` for execution work.

## Code anchors

| Area | Path |
|------|------|
| Workshop disposition + steer | `src/agent_runtime/turn_worker/store.rs` |
| Enter bound workshop | `src/agent_runtime/turn_worker/run.rs` |
| begin_work tool | `src/turn_control_tools.rs` |
| Host exit | `src/medousa_tool_loop.rs`, `src/agent_runtime/turn_orchestrator.rs` |
| Host allowlist | `src/agent_runtime/turn_worker/policy.rs` |
| Host FSM | `src/agent_runtime/turn_completion_fsm.rs` |
| Steer HTTP | `src/daemon/workshop_steer.rs` |
| Ticket phase | `crates/medousa-types/src/turn_ticket.rs` |
