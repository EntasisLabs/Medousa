# Turn worker bus — Phase 1 (implemented)

> **Durability track:** [durable-turn-worker-plan.md](durable-turn-worker-plan.md) — Stasis job queue, disk-backed records, restart reconciliation (Phase 1a–1c landing).

Phase 1 of [turn-worker-bus-plan.md](turn-worker-bus-plan.md): in-process **host → worker → synthesis** on the daemon agent runtime (all comms adapters).

## Behavior (product mode C)

1. **Host turn** runs the normal tool loop (full registry by default).
2. Model calls **`cognition_spawn_turn_worker`** with `intent`, `task`, `user_ack`.
3. Host turn **ends immediately** with `user_ack` (`termination_reason=worker_spawned`).
4. **Worker** runs in `tokio::spawn` with a **filtered tool allowlist** and `WORKER_SYSTEM_PROMPT`.
5. On worker success, a **synthesis** prompt-only pass publishes the final answer on the same `AgentStreamSink` (second message for Telegram/API; TUI event + session append).

## Tools (host bus)

| Tool | Role |
|------|------|
| `cognition_spawn_turn_worker` | Delegate work; starts background worker |
| `cognition_turn_worker_status` | List/fetch `TurnWorkRecord` by session or `work_id` |
| `cognition_turn_worker_cancel` | Best-effort cancel pending/running work |

## Worker intents (allowlists)

| Intent | Tools (subset) |
|--------|----------------|
| `memory.avec_calibrate` | memory schema/moods/calibrate/context/list/recall/store + prepare_final + utilities |
| `memory.context` | memory tools without calibrate requirement |
| `general` | memory + capability_invoke + mcp_invoke |

Defined in `src/agent_runtime/turn_worker/policy.rs`.

## Host bus env (updated in Phase 2)

| `MEDOUSA_TURN_HOST_BUS` | Behavior |
|-------------------------|----------|
| *(unset)* / `auto` | Slim host only when route is `delegate:*` (default) |
| `force` / `1` / `true` | Slim host on every tool turn |
| `off` / `0` / `false` | Full host registry; spawn still available |

See [turn-worker-phase2.md](turn-worker-phase2.md).

## State

- **In-memory** `TurnWorkerStore` (`turn_worker_store()`) — `TurnWorkRecord` by `work_id`
- **Ledger** JSONL events: `work_delegated`, `work_completed` (see [turn-ledger-phase0.md](turn-ledger-phase0.md))
- **Bus session** per `execute_local_turn`: `ActiveWorkerBusSession` on `TurnWorkerScheduler` (sink, session, correlation, delivery target)

## Code map

| Path | Role |
|------|------|
| `src/agent_runtime/turn_worker/` | store, policy, registry, prompts, run |
| `src/agent_runtime/turn_worker_tools.rs` | Stasis tool definitions |
| `src/medousa_tool_loop.rs` | End host turn on spawn |
| `src/agent_runtime/turn_orchestrator.rs` | Bus session + optional host allowlist pipeline |
| `src/tui/runtime_services.rs` | Register worker tools; `TuiRuntime.worker_scheduler` |
| `src/runtime/platform.rs` | Attach scheduler context on daemon platform build |

## Observability

Notices (all adapters):

- `◈ work_delegated work_id=… intent=…`
- `◈ work_running work_id=…`
- `◈ work_completed work_id=…` / `◈ work_failed …`
- `◈ work_synthesis work_id=… delivering final answer`

## Verify

```bash
cargo check
cargo test turn_worker
```

Example spawn (model or manual tool call):

```json
{
  "intent": "memory.avec_calibrate",
  "task": "Pull focused AVEC preset, run cognition_memory_calibrate, return stability and receipt summary.",
  "user_ack": "On it — calibrating your focused AVEC in the background."
}
```

## Worker failures

On worker error (e.g. tool policy failure), the bus emits `◈ work_failed` and a **failure notify** turn posts a user-visible assistant message (same sink as synthesis) — not only obs.

Worker memory tools receive automatic `session_id` injection via `WorkerSessionToolRegistry` when the model passes null.

## Not in Phase 1

- Durable Stasis worker jobs (Phase 3)
- Structured SSE event types beyond `notice` (Phase 4)
- True multi-message chat while worker runs (mode B)
