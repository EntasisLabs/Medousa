# DLQ Replay → Agent Turn Continuation — Plan

> Created: 2026-05-31  
> Status: In progress  
> Related: [centralized-agent-runtime-roadmap.md](centralized-agent-runtime-roadmap.md), [outbox-channel-delivery-roadmap.md](outbox-channel-delivery-roadmap.md)

## Problem

After Phase 2/3, agent turns run as **ephemeral in-process tasks** (`run_agent_turn`), while tools enqueue **durable Stasis child jobs** (grapheme runs, `cognition.job.enqueue`, etc.).

When a child job lands in the dead-letter queue and is replayed from the Stasis dashboard:

1. Stasis correctly resets and re-runs the job → **job succeeds**
2. Outbox may emit `job_succeeded` for the **child** job id
3. No agent turn is listening — the parent turn already finished
4. `channel_deliveries` is keyed by the **ingest synthetic job id**, not child cognition jobs
5. The user never gets a resumed conversation

Stasis `replay_dead_letter` is job-scoped by design. Medousa never bridged replay back to the agent turn layer.

## Current wiring (gap)

```
Ingest → tokio::spawn run_agent_turn          [ephemeral]
       → channel_deliveries[ingest-job-id]

Tool   → enqueue child job                    [durable]
       → correlation_id = child job_id        [no parent link]
       → causation_id = "cognition_tui"

DLQ replay → child job succeeds → outbox → deliver/outbox misses delivery target
```

Medousa `continuation.rs` is **LLM synthesis continuation** (large tool output → second completion). This plan addresses **Stasis job → agent turn continuation** after DLQ replay.

## Chosen approach: Turn continuation registry (Option B)

Keep in-process agent turns for streaming and tool-loop parity. Add a durable **parent-child link** and a **resume hook** when child jobs succeed after failure or dead-letter.

### Principles

1. **Happy path unchanged** — sync tools that succeed in-turn mark records `consumed`; no double reply.
2. **Replay path fixed** — child job DLQ → replay → success triggers a continuation agent turn.
3. **Idempotent resume** — one continuation message per child job id.
4. **Stasis correlation** — child jobs use `correlation_id = turn_correlation_id` for lineage.

## Data model

### `TurnContinuationScope` (per active turn, in-memory on `TuiRuntime`)

| Field | Description |
|-------|-------------|
| `turn_correlation_id` | Ingest job id, API ask job id, or interactive turn id |
| `session_id` | Conversation session |
| `original_prompt` | User message for this turn |
| `delivery_target` | Optional channel dispatch target for resume delivery |
| `provider`, `model`, `response_depth_mode` | Routing for continuation turn |

### `TurnContinuationRecord` (durable, Surreal or in-memory)

| Field | Description |
|-------|-------------|
| `child_job_id` | Primary key |
| `turn_correlation_id` | Parent turn |
| `session_id`, `original_prompt`, `tool_name`, `job_type` | Resume context |
| `await_mode` | `sync` (grapheme) or `async` (fire-and-forget enqueue) |
| `status` | `pending` → `consumed` \| `resumed` \| `abandoned` |
| `turn_finished` | Set when parent turn completes |
| `turn_outcome` | `success` or `error` |
| `child_was_dead_letter` | Set on `job_dead_lettered` outbox event |
| `delivery_target` | Stored copy for channel resume |

## Resume conditions (v1)

Resume when **all** of:

- Record `status == pending`
- Child job just **succeeded**
- One of:
  - `child_was_dead_letter == true` (DLQ replay path)
  - `turn_finished && turn_outcome == error` (sync tool failed during turn)
  - `turn_finished && await_mode == async && child_was_dead_letter` (async DLQ after turn ended)

Do **not** resume when sync tool succeeded in-turn (`status = consumed`).

## Implementation phases

### Phase 1 — Correlate ✅ (this PR)

- [x] `turn_continuation_store` (Surreal + in-memory)
- [x] `TurnContinuationScope` on `TuiRuntime`
- [x] Tools: `cognition.job.enqueue`, `cognition_grapheme_run` register records + correlation ids
- [x] `run_agent_turn`: set scope, track outcome, mark turn finished

### Phase 2 — Resume hook ✅ (this PR)

- [x] Scheduler tick: after `process_once`, check succeeded child jobs
- [x] `deliver/outbox`: `job_dead_lettered` → mark dead letter; `job_succeeded` → try resume before delivery lookup
- [x] `spawn_continuation_agent_turn` in daemon (new ingest job id + delivery registry)

### Phase 3 — Hardening (follow-up)

- [ ] Extend to `CognitionRuntimeWorkflowRunTool` and promote-to-job tools
- [ ] `POST /v1/jobs/{id}/replay-and-resume` explicit operator endpoint
- [ ] Doctor: pending continuations count + last resume
- [ ] Metrics / dashboard lineage view by `turn_correlation_id`

## Non-goals

- Reverting to durable Stasis agent turn jobs for ingest (Option A)
- Changing Stasis `replay_dead_letter` semantics
- Mid-stream SSE resume for closed TUI streams (delivery + session history is sufficient)

## Code anchors

| Area | Path |
|------|------|
| Continuation store + resume logic | `src/turn_continuation.rs` |
| Turn scope + outcome tracking | `src/agent_runtime/daemon_interactive_turn.rs` |
| Tool correlation | `src/tools.rs` |
| Daemon hooks | `src/bin/medousa_daemon.rs` |
| Store init | `src/runtime/platform.rs` |

## Stasis reference

Stasis v1 defines **Continuation** job class: downstream jobs share `correlation_id`, set `causation_id = parent_job_id`, and consume parent `sttp_output_node_id`. Medousa's parent is not a Stasis job today; this plan bridges that gap at the Medousa layer without requiring Stasis handler changes.
