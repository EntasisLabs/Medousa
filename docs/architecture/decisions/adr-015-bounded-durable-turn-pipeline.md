# ADR-015: Bounded single-writer durable turn pipeline

> **Status:** Proposed
>
> **Date:** 2026-08-13
>
> **Decision owners:** engine runtime and Home streaming maintainers
>
> **Related:** [ADR-004](adr-004-durable-turn-spine.md), [H03 execution plan](../../../architecture/hardening/03-turn-stream-v2.md)

## Context

Medousa's most latency-sensitive path carries small provider fragments through
multiple independently stateful layers: provider callbacks, nested unbounded
queues, the agent sink, a synchronous JSONL journal, an in-memory replay vector,
broadcast, Axum SSE, Tauri SSE parsing, JavaScript event reduction, Markdown
parsing/sanitization, and DOM hydration.

One content fragment is currently copied and serialized repeatedly. Journal I/O
runs synchronously on an async worker and flushes every event without providing
power-loss durability. Replay retains every string-heavy event. Home parses the
same JSON after Rust already parsed and discarded a typed value, then replaces
reactive transcript structures and reparses the complete accumulated Markdown
for every fragment. Nested unbounded channels allow these slow consumers to
turn a bursty provider into unbounded process memory.

The public stream event is also a string discriminator plus dozens of nullable
fields. Impossible event combinations are representable, producers allocate
constant strings, and clients cannot switch exhaustively.

The separate session-history writer makes a stronger claim than its API can
prove. Store append returns `()`, errors are swallowed, queued attempts are
counted as successful commits, queue overflow performs blocking persistence on
the async caller, and shutdown has no acknowledged drain. A UI can therefore
show a successful terminal answer that is absent after restart.

ADR-004 correctly established the durable journal, monotonic sequence, and
`?since=` replay model. This decision preserves those product semantics while
replacing per-provider-fragment work and false durability with one bounded
owner.

## Decision

### 1. One actor owns each turn's output ordering

Each active turn has one `TurnPipeline` actor. It owns:

- provider-fragment admission and coalescing;
- attempt boundaries, event ordering, and sequence assignment;
- journal encoding, buffered append, flush, and sync fences;
- the bounded live replay ring and sparse journal index;
- live subscriber publication;
- terminal session-history commit acknowledgement; and
- cancellation, failure, close, and shutdown state.

Nested attempt/turn forwarding queues, direct sink-to-journal calls, and
independent terminal publication are removed. Producers send typed commands to
this owner; no other component stamps a sequence or publishes a terminal event.

### 2. Admission is bounded in messages and bytes

The pipeline has hard per-turn message and byte limits. Async producers wait
for capacity and cancellation-aware admission. A provider API that cannot await
uses a bounded coalescing adapter; exceeding its byte budget fails the turn
visibly rather than allocating without limit or dropping semantic events.

Adjacent content fragments and adjacent reasoning fragments are coalesced until
the first of a small latency deadline, byte threshold, semantic boundary,
flush request, or terminal command. Tool, approval, status, attempt, reset, and
terminal events are never merged across their semantic boundary.

Sequences identify emitted semantic events/batches, not raw provider callback
fragments. Clients must not infer token counts from sequence numbers.

### 3. The journal is authoritative and written once

The pipeline serializes each sequenced v2 event once into a buffered append-only
journal. Disk work runs in a dedicated bounded blocking/writer context, not
while holding a `std::sync::Mutex` on a Tokio worker. A successful live publish
occurs only after the corresponding complete record has been appended
successfully.

Durability levels are explicit:

| Level | Meaning |
| --- | --- |
| `accepted` | Pipeline owns the event in bounded memory; no durability promise |
| `written` | Complete journal record was accepted by the filesystem; process-crash replay contract |
| `synced` | Journal data reached the configured file synchronization fence |
| `committed` | Terminal journal is synced and required session-history transaction acknowledged |

Non-terminal batches are flushed by bounded time/bytes and synced periodically
according to a documented policy. A power failure may lose records after the
last reported sync fence; a normal process crash must recover the complete
written prefix. Terminal success is not published until all prior events and
the terminal record are synced and the history store returns a commit receipt.

Write, serialization, flush, sync, or history failure is a first-class pipeline
failure. It increments failure metrics and yields a durable/degraded terminal
outcome when possible; it is never counted as a successful commit.

### 4. Replay is journal plus a bounded live ring

ADR-004's `?since=<seq>` behavior remains. The live ring holds a bounded number
of already encoded/typed batches and bytes. Older replay comes from the journal
using sparse sequence-to-offset checkpoints in a bounded blocking reader.

The pipeline takes a replay fence, emits journal/ring events after `since` up to
that fence, then attaches the live tail without a gap or duplicate. A lagged
subscriber resumes from its last sequence through the same replay path; it does
not silently skip broadcast events. A disconnected client cannot pin unbounded
memory or journal handles.

Committed journals follow an explicit retention/compaction policy. Uncommitted
journals remain available to startup recovery. Deleting the in-memory registry
after a grace period does not delete recovery authority prematurely.

### 5. Stream v2 is a discriminated union

The v2 wire shape contains a small common envelope and exactly one typed event:

```json
{
  "schema_version": 2,
  "turn_id": "...",
  "seq": 42,
  "emitted_at_utc": "...",
  "event": { "type": "content_append", "text": "..." }
}
```

Rust uses a serde-tagged enum with variant-specific fields and enums for bounded
states. The internal pipeline event can share turn-scoped identity and owned
payload bytes; conversion to the public representation happens once at the
transport boundary. The generated TypeScript discriminated union is imported
directly and handled with exhaustive switches.

V1 remains behind an explicit projection adapter during migration. It receives
v2 semantic batches and preserves sequence/replay behavior; it is not an
independent event construction path. New variants are v2-first and require a
declared v1 behavior or explicit unsupported result.

### 6. Tauri forwards each payload across one representation boundary

The native bridge parses SSE incrementally over bytes with bounded frame and
buffer sizes. Consumed bytes are advanced/drained without copying the entire
suffix. It performs exactly one of these operations per configured bridge:

- validate/decode a typed event and emit that typed serializable value; or
- validate framing/size and forward the raw JSON bytes/string for JavaScript to
  decode once.

It never deserializes a payload into an unused `T` and then emits the original
JSON string. Cancellation closes parsing promptly and partial/oversized frames
produce bounded errors.

### 7. Home renders stable completed blocks and one streaming tail

Home buffers append events per turn and publishes reactive state at most once
per animation frame, with a maximum latency fallback for throttled/background
surfaces. A turn/message index avoids transcript-wide searches. Plain content
events do not copy or deduplicate tool arrays.

While streaming, completed Markdown blocks are parsed, sanitized, and hydrated
once, then retained as stable keyed nodes. Only the incomplete tail uses a
cheap escaped-text or incremental parser path. At terminal, the final tail is
parsed once and the canonical terminal body reconciles the stream without
replacing unchanged completed subtrees.

Code highlighting, Mermaid, Liquid, draw, image resolution, and embed hydration
run only for newly completed or changed blocks. Markdown rendering state is
request-local; module-global mutable parse options/counters are removed.

### 8. History persistence returns truthful receipts

The session store exposes an asynchronous result-bearing batch/transaction
operation. A commit receipt identifies the session, committed generation or
turn, and durability level. The history writer has bounded admission, does not
fall back to blocking work on an async caller, retries only classified retryable
errors with bounded backoff, and owns a dead-letter/degraded path.

Terminal pipeline state awaits the required receipt. Graceful shutdown closes
admission, drains with a deadline, syncs required journals/stores, and reports
uncommitted work. Metrics distinguish accepted, written, synced, committed,
failed, retried, and abandoned-at-deadline work.

## Consequences

### Positive

- Cost scales with bytes and semantic batches rather than provider fragments
  multiplied by accumulated response/transcript size.
- Slow disk, subscribers, or UI cannot grow turn memory without bound.
- Exactly one owner defines sequence, terminal order, cancellation fence, and
  commit truth.
- Replay survives reconnect without retaining every event clone in memory.
- Rust and TypeScript reject impossible stream variants at compile time.
- A displayed successful terminal answer has an acknowledged durability chain.

### Costs and migration

- Provider callbacks, engine ports, journal/recovery, SSE, SDKs, Tauri, Home,
  and persistence all change as one coordinated protocol migration.
- Coalescing changes the number/timing of deltas while preserving concatenated
  content, semantic boundaries, and monotonic order.
- A v1 adapter must remain until supported clients migrate.
- Incremental Markdown/block rendering requires explicit behavior for open
  fences, tables, lists, Mermaid, Liquid, and edits/reset events.
- Per-turn actors and journal readers need global admission limits so many idle
  turns do not merely move the memory problem upward.

### Superseded or narrowed decisions

- ADR-004's journal authority, `seq`, reconnect, and startup recovery decisions
  remain accepted.
- ADR-004's consequence that “disk I/O per turn event” is an acceptable
  chat-scale tradeoff is superseded. Provider fragments are coalesced into
  semantic batches and written by a buffered single owner.
- ADR-004's no-version-bump migration applied to the original `seq` addition.
  The typed protocol is explicitly v2 with a compatibility adapter.
- ADR-004's in-memory replay implementation is narrowed: memory is a bounded
  cache; the journal remains replay authority.

## Verification

Implementation is governed by P01/P02 in the [performance budgets](../../../architecture/hardening/verification/performance-budgets.md)
and CR-001–CR-003, ISO-001/007–010, BP-001–008, and RET-001 in the
[crash/concurrency matrix](../../../architecture/hardening/verification/crash-concurrency-matrix.md).

## Code anchors

- `src/agent_runtime/turn_orchestrator.rs` — nested unbounded bridges
- `src/agent_runtime/daemon_interactive_turn.rs` — sink accumulation, journal,
  and publication
- `crates/medousa-engine/src/turn_event.rs` — internal typed vocabulary
- `crates/medousa-engine/src/turn_event_log.rs` — synchronous per-event journal
- `src/daemon/turn_stream_registry.rs`, `src/daemon/turn_event_channel.rs`,
  `src/daemon/ingest.rs` — replay/live registry
- `src/session_writer.rs`, `src/session_store.rs` — false commit acknowledgement
- `crates/medousa-types/src/daemon_api.rs` — nullable v1 wire DTO
- `apps/medousa-home/src-tauri/src/daemon/sse.rs` — copying/double-decode bridge
- `apps/medousa-home/src/lib/stores/chat.svelte.ts` — per-event reactive copies
- `apps/medousa-home/src/lib/components/ui/MarkdownContent.svelte` and
  `apps/medousa-home/src/lib/markdown/` — whole-answer render/hydration
