# Channel worker notifications — review plan

> **Status:** Implemented (2026-06-07)  
> **Symptom:** WhatsApp / Telegram / Discord user sees nothing when a turn worker spawns or finishes; only Home (SSE + Work Hub) shows activity.

---

## Problem

When Medousa delegates to a background turn worker from an **external channel** ingest turn:

1. The worker **does** spawn and run (visible in Home Work Hub).
2. The channel user gets **no** “working on it” message at spawn time.
3. The channel user often gets **no** synthesis answer when the worker completes.
4. They only learn what happened by opening Home.

Home must **not** regress: worker handoff stays on SSE + workspace cards + optional Tauri mobile push (`notifyWorkerHandoff`). No duplicate OS pushes on desktop Home.

---

## Current behavior (as of review)

### Home / interactive turn (`POST /v1/interactive/turn`)

| Event | Sink | Channel push |
|-------|------|--------------|
| `worker_ack` | `DaemonInteractiveTurnSink` | **None** — SSE `worker_ack` only |
| Worker synthesis | Same sink via host continuation | **None** — SSE `final` + session |
| Budget pause | Tool loop + `turn_budget_notify` | **External only** (Telegram/Discord/Slack/WhatsApp) |

Home UI: `chat.applyStreamEvent` unlocks composer on `worker_ack`; workspace `turn_worker` cards drive synthesis bubble updates. Tauri **mobile** optionally fires `notifyWorkerHandoff` — not desktop, not external channels.

**This is intentional and must stay.**

### Ingest channels (`POST /v1/ingest` → WhatsApp adapter, etc.)

Host turn uses `IngestAgentStreamSink` (`medousa_daemon.rs`):

| Event | Current sink behavior | Channel push |
|-------|----------------------|--------------|
| `worker_ack` | Trait default → `agent_response` | *Should* push `user_ack` text, but also marks job **delivered**, publishes SSE **`final`**, removes `channel_deliveries` |
| Normal `agent_response` | Explicit dispatch + mark delivered | ✅ Push final answer |
| `turn_budget_approval` | Tool loop calls `turn_budget_notify` | ✅ External push |

Worker execution (Stasis job) uses **`DurableWorkerStreamSink`** (`turn_worker_job.rs`):

```265:274:Medousa/src/agent_runtime/turn_worker_job.rs
    async fn agent_response(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        let turn = ConversationTurn::plain(
            "assistant",
            text,
            Utc::now(),
            tool_names,
            None,
        );
        append_turn(&self.session_id, &turn);
    }
```

**No `dispatch_channel_message`.** Synthesis is session-only. External users never see the worker result unless they open Home.

`TurnWorkRecord` **does** carry `delivery_target` (copied from host bus session at spawn), but the durable sink ignores it.

### Reference pattern: budget approval

`turn_budget_notify.rs` already solves “notify external channels only”:

- Skip `is_home_channel`
- Only push `is_external_push_channel` (telegram, discord, slack, whatsapp)
- Use `channel_delivery::dispatch_channel_message`
- Home relies on workspace blocked cards + SSE

Mirror this for worker spawn and (separately) synthesis completion.

---

## Root causes

1. **Synthesis gap (primary):** `DurableWorkerStreamSink` persists assistant text to session but never pushes to `record.delivery_target`.
2. **Spawn gap (secondary):** No dedicated `agent_worker_ack` on `IngestAgentStreamSink`. Trait default treats handoff as terminal `agent_response` — wrong SSE semantics, premature `mark_job_delivery_success`, and no formatted “worker started” message. In practice users still report silence (likely conflated with missing synthesis + no distinct spawn copy).
3. **Home isolation (working):** `DaemonInteractiveTurnSink.agent_worker_ack` overrides default and avoids channel push — **keep this**.

---

## Proposed fix (engine-only, no adapter changes)

### A. `turn_worker_notify.rs` (new module, mirror `turn_budget_notify.rs`)

```rust
pub struct TurnWorkerSpawnNotifyPayload {
    pub work_id: String,
    pub user_ack: String,
    pub intent: Option<String>,
}

pub fn compose_turn_worker_spawn_text(payload: &TurnWorkerSpawnNotifyPayload) -> String;

pub async fn notify_turn_worker_spawned(
    client: &Client,
    delivery_target: &ChannelDeliveryTarget,
    payload: TurnWorkerSpawnNotifyPayload,
) -> Result<()>;
// Guards: skip home; only external push channels
```

Optional follow-up: `notify_turn_worker_synthesis` or reuse `format_channel_delivery_text` at synthesis time (see B).

Suggested spawn copy (plain language, channel-safe):

```
{user_ack}

(Background worker {work_id} — I'll message you here when it's done.)
```

Keep `user_ack` as the model-provided short line; append work_id only if useful for support (Telegram/Discord tolerate it; WhatsApp keep short).

### B. `IngestAgentStreamSink::agent_worker_ack` (override)

Replace trait default. On worker handoff:

1. Persist worker-ack turn slice (same as today’s session append intent).
2. **`notify_turn_worker_spawned`** when `is_external_push_channel(&delivery_target.channel)`.
3. Publish SSE **`worker_ack`** (not `final`) via `worker_ack_stream_event_with_tools`.
4. **Do not** call `mark_job_delivery_success` or remove `channel_deliveries`.

Leave terminal delivery to synthesis `agent_response` (or failure path).

### C. Synthesis push from durable worker sink

Extend `DurableWorkerStreamSink`:

- Hold `Option<ChannelDeliveryTarget>` + `reqwest::Client` (from daemon runtime context when constructing the sink in `turn_worker_job.rs`).
- In `agent_response`: after session append, if target is external push channel, `dispatch_channel_message` with `format_channel_delivery_text`.
- Then `mark_job_delivery_success` for the **ingest job_id** linked from `parent_turn_correlation_id` / delivery records (needs correlation — see open question below).

Alternative: post-synthesis hook in `deliver_synthesis_response` that reads `record.delivery_target` and dispatches once — keeps sink thin.

### D. Trait default `agent_worker_ack`

Change `stream_sink.rs` default from `agent_response` to **no-op** (like `turn_budget_approval_required` default). Forces explicit behavior per sink:

| Sink | worker_ack behavior |
|------|---------------------|
| `DaemonInteractiveTurnSink` | SSE only (unchanged) |
| `IngestAgentStreamSink` | External notify + SSE worker_ack (new) |
| `DurableWorkerStreamSink` | no-op |
| TUI sink | Non-terminal TuiEvent (unchanged) |

Prevents accidental terminal delivery on any future sink.

### E. Worker failure

`run_worker_failure_notify` already calls `sink.agent_response` on the **host** sink during in-process paths. Durable job path should dispatch failure text to external channels the same way as synthesis (reuse B/C machinery).

---

## Home regression checklist

Before merge, verify:

- [ ] `DaemonInteractiveTurnSink.agent_worker_ack` still **only** publishes SSE — no `dispatch_channel_message`.
- [ ] `notifyWorkerHandoff` still gated on `isTauriMobilePlatform()` — no new desktop push.
- [ ] Composer still unlocks on `worker_ack`; background pulse unchanged.
- [ ] Workspace `turn_worker` cards still link via `work_id`.
- [ ] Budget approval external notify unchanged (`turn_budget_notify` tests pass).

---

## Open questions (decide before coding)

1. **Ingest job correlation for synthesis delivery:** Worker record has `parent_turn_correlation_id` (ingest `job_id`). Need to mark the original ingest job delivered only on synthesis (or failure), not on spawn. Confirm `job_delivery_records` keying.
2. **WhatsApp adapter poll loop:** Today adapter waits for first `delivered` poll. Spawn notify must **not** mark job delivered (fix in B). Synthesis push should mark delivered so poll/fallback still works for single-message adapters.
3. **Message count:** Users may prefer two messages (spawn ack + synthesis) vs one edited message — out of scope for v1; document as Phase 4 polish in [turn-worker-bus-plan.md](turn-worker-bus-plan.md).
4. **Slack:** Include in external push set (already in `is_external_push_channel`).

---

## Test plan

| Case | Channel | Expect |
|------|---------|--------|
| Host spawns worker | Home interactive | SSE `worker_ack`, no WhatsApp/Telegram message |
| Host spawns worker | WhatsApp ingest | Push spawn ack; job poll still **pending** |
| Worker completes | WhatsApp ingest | Push synthesis; job poll **delivered** |
| Worker fails | Telegram ingest | Push user-visible error |
| Budget pause mid-turn | Discord ingest | Existing budget notify still fires |

Unit tests: `compose_turn_worker_spawn_text`, home/external channel guards (copy `turn_budget_notify` tests).

Manual: WhatsApp adapter end-to-end — message → spawn → see ack → wait → see synthesis.

---

## Files to touch (implementation)

| Area | Path |
|------|------|
| New notify module | `src/turn_worker_notify.rs`, `src/lib.rs` |
| Ingest sink | `src/bin/medousa_daemon.rs` (`IngestAgentStreamSink::agent_worker_ack`) |
| Durable worker sink | `src/agent_runtime/turn_worker_job.rs` |
| Trait default | `src/agent_runtime/stream_sink.rs` |
| Synthesis hook (optional) | `src/agent_runtime/turn_worker/run.rs` (`deliver_synthesis_response`) |
| Tests | `src/turn_worker_notify.rs`, ingest sink integration test if feasible |

**Out of scope:** adapter changes (WhatsApp/Telegram binaries), Home UI, desktop notifications.

---

## References

- [turn-worker-bus-plan.md](turn-worker-bus-plan.md) — success criterion #1 (Telegram pending ack + synthesis)
- [turn_budget_notify.rs](../src/turn_budget_notify.rs) — pattern to mirror
- [async-chat-unlock-plan.md](async-chat-unlock-plan.md) — Home worker_ack semantics
- [channel_delivery.rs](../src/channel_delivery.rs) — `is_external_push_channel`, dispatch helpers
