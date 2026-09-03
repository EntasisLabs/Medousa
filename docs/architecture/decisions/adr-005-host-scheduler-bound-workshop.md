# ADR-005: Host scheduler and bound workshop turns

**Status:** Accepted
**Date:** 2026-07-02
**Amended:** 2026-09-03

## Context

The host console tool loop tried to be both a conversational partner and an execution engine. Prose-terminates and interim heuristics (`update_user`, `prose_requires_finish`) fought the model's natural "think then tool" rhythm. Workshop workers did not exhibit this because role is explicit: execute `WORKER_TASK`, call tools, `cognition_turn_finish`.

## Decision

1. **Host = principal-facing operator** — hot lane for memory, identity, runtime, vault, web, and short local diagnostics. The host may call `cognition_shell_status` and `cognition_shell_run` directly when the operator-enabled shell charter admits them.
2. **Bound workshop** — `cognition_turn_begin_work(message, goal, intent?)` enqueues independently scoped, durable, or longer execution work per session (reuses `run_worker_turn` + synthesis). Host ends with ack; principal sees ack → synthesis on the same thread.
3. **Parallel worker** — unchanged (`cognition_spawn_turn_worker`) for heavy multi-topic research.
4. **Steering** — principal can inject messages into one exact bound-workshop generation via `POST /v1/sessions/{id}/workshop/steer` with `work_id`; workshop loop reads `[MEDOUSA_WORKSHOP_STEER]` each round. Stale generations are rejected.
5. **Deprecate** `cognition_turn_update_user` — workshop internal monologue replaces mid-turn host status tools.
6. **Host FSM** — cooperative prose on host (`host_scheduler_lane`); worker/workshop FSM unchanged.

Cancellation also names the exact `work_id` and is authorized against its
owning session. If that generation is live, the same mutation signals its
registered cancellation token immediately; the durable cancelled state remains
the recovery truth across restart.

## Consequences

**Positive**

- Canvas and multi-tool local work no longer fight host turn control.
- Simple system inspection does not pay a worker handoff and synthesis round-trip.
- One Medousa voice; role split is scheduling, not personality.
- Composer stays open during bound workshop (handoff phase).

**Tradeoffs**

- Delegated work still has extra latency versus inline host execution.
- Direct host shell calls broaden the hot lane, but remain opt-in and bounded by the same network, writable-root, binary, timeout, and output ceilings.
- One bound workshop per session at a time, enforced atomically at insertion.
- Host must call `begin_work` with a concrete `goal` for execution work.

## Code anchors

| Area | Path |
|------|------|
| Workshop disposition + steer | `src/agent_runtime/turn_worker/store.rs` |
| Enter bound workshop | `src/agent_runtime/turn_worker/run.rs` |
| begin_work tool | `src/turn_control_tools.rs` |
| Host exit | `crates/medousa-runtime/src/tool_loop.rs`, `src/agent_runtime/turn_orchestrator.rs` |
| Host allowlist | `src/agent_runtime/turn_worker/policy.rs` |
| Host FSM | `crates/medousa-runtime/src/completion_fsm.rs` |
| Steer HTTP | `src/daemon/workshop_steer.rs` |
| Ticket phase | `crates/medousa-types/src/turn_ticket.rs` |
