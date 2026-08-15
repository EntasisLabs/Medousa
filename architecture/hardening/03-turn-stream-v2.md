# H03 — Bounded durable turn stream v2

> **Status:** Implemented; release validation and retained benchmark evidence pending
>
> **Accountable owner:** engine runtime maintainers
>
> **Reviewers:** daemon/SSE, persistence, SDK, Home/Tauri, frontend performance
>
> **Audit findings:** PERF-001 (Critical), DUR-001 (Critical), MEM-002 (High), TYPE-001 (High), PERF-005 (Critical)
>
> **Release gates:** Gate B for durability; Gate C for bounded hot paths; Gate D for generated protocol
>
> **Required decision:** [ADR-015](../../docs/architecture/decisions/adr-015-bounded-durable-turn-pipeline.md)
>
> **Dependencies:** H05 request-scoped context for complete concurrent isolation; H10 generated contract for final enforcement

## Outcome

A provider fragment crosses one bounded sequencing owner, one buffered journal
encoder, one live/replay projection, one native bridge representation, and one
frame-batched UI reducer. Successful terminal output is acknowledged durable;
slow or failed consumers have bounded memory and visible failure semantics.

H03 owns the turn output/persistence pipeline. H05 owns unrelated process-global
turn/request state. H10 owns the final authoritative API generator. H03 may
introduce the v2 schema and generate immediate client types without waiting for
the repo-wide H10 migration.

## Current flow and amplification

```text
provider fragment (owned String)
  -> unbounded attempt queue
  -> unbounded turn queue
  -> InteractiveTurnStreamSink
     -> cumulative String append
     -> no-op parts mutex for content
     -> nullable v1 DTO allocation
     -> clone into TurnEvent
     -> JSON serialize + write + flush under std Mutex
     -> clone into unbounded replay Vec
     -> clone into broadcast
  -> Axum SSE serialization
  -> Tauri String buffer suffix copies + JSON decode to unused T
  -> emit original JSON string
  -> JavaScript JSON.parse
  -> promise chain + transcript scan + message/array copy
  -> full Markdown parse/sanitize/DOM replacement/hydration
```

The current terminal path is not a dependable fence. `SessionStore::append_turn`
returns `()`, backend errors are printed and swallowed, the writer counts the
attempt as success, and overflow/shutdown falls back to blocking persistence on
the async caller. `TurnEventLog::mark_committed` ignores flush/marker errors.

## Invariants

1. One owner assigns every sequence and terminal state for a turn.
2. Sequences are monotonic; no semantic content publishes after terminal.
3. Concatenating content/reasoning batches preserves admitted fragment order.
4. Every queue/ring/frame/registry has both count and byte/lifetime bounds.
5. Overload waits, coalesces adjacent compatible data, or fails visibly; it
   never silently drops semantic events or allocates without limit.
6. A published event has a complete successfully appended journal record.
7. A published successful terminal has a synced journal and acknowledged
   required history commit.
8. Replay plus live tail yields each `seq > since` exactly once to a conforming
   client, including at the join fence.
9. Persistence failure is observable and cannot increment success metrics.
10. Cancellation has one accepted sequence fence and finite completion time.
11. Stream v2 cannot represent impossible variant-field combinations.
12. UI work during streaming is proportional to changed tail/batches, not the
    accumulated answer times fragment count.

## Non-goals

- preserving provider callback boundaries or exposing token counts;
- making every non-terminal batch power-loss durable at per-token latency;
- CRDT/multi-writer editing of one assistant message;
- redesigning provider inference internals beyond async/bounded emission;
- solving global request context and cross-turn tool state owned by H05;
- retaining v1 indefinitely.

## Turn pipeline state machine

```text
Starting
  -> Streaming <-> AwaitingTool / AwaitingApproval / AttemptBoundary
  -> Finalizing
       -> journal sync
       -> history commit receipt
       -> terminal publish
  -> Committed

Any active state
  -> Cancelling -> Cancelled
  -> Failing -> Failed or DurabilityDegraded
```

Only the pipeline actor changes state. Producer, sink, recovery, and HTTP tasks
send commands and await receipts; they do not publish directly.

### Commands

Use a typed command enum with variant-specific data:

```rust
enum TurnPipelineCommand {
    AppendContent(Bytes),
    AppendReasoning(Bytes),
    AttemptStarted { attempt: AttemptId, provider: ProviderId, model: ModelId },
    AttemptEnded { attempt: AttemptId, outcome: AttemptOutcome },
    ToolStarted(ToolStarted),
    ToolFinished(ToolFinished),
    Status(StatusEvent),
    ScratchReset,
    ApprovalRequired(ApprovalEvent),
    Finalize(FinalTurn),
    Fail(TurnFailure),
    Cancel(CancelReason),
    Flush(oneshot::Sender<FlushReceipt>),
}
```

Use `Bytes`/owned chunks where it eliminates rematerialization, but do not force
zero-copy theater across JSON/IPC boundaries. The measurable objective is one
owned payload through the core and one deliberate encoding per transport.

## Admission and coalescing

### Initial safety limits

These are implementation starting ceilings, not final performance claims; P01
may tighten them with retained evidence:

| Resource | Initial bound |
| --- | --- |
| Commands per turn | 256 queued |
| Payload bytes per turn queue | 1 MiB |
| Adjacent content/reasoning batch | 32 KiB maximum |
| Coalescing deadline | 16 ms foreground; 25 ms absolute normal maximum |
| Live replay ring | 512 events and 2 MiB, whichever first |
| SSE frame | 1 MiB hard maximum; smaller per variant where possible |
| Subscribers | explicit per-turn/global configuration, default-deny over cap |
| Cancellation/drain | 2 s normal turn deadline |
| Graceful shutdown pipeline drain | 5 s global deadline with incomplete report |

The byte budget uses permits acquired before enqueue and released only when the
actor consumes/drops the payload. Message capacity alone is not a memory bound.
Global limits cap active pipeline count, total queued bytes, live-ring bytes,
replay readers, and subscribers.

### Provider integration

- Make the primary stream emitter async so `send().await` propagates
  backpressure and observes cancellation.
- Collapse `AttemptStreamBridge` and `TurnStreamBridge` into commands on the
  same actor; attempt metadata preserves fallback visibility/order.
- Where a third-party callback cannot await, append only adjacent text into a
  fixed byte ring guarded by nonblocking admission. The drain task wakes once,
  not once per token.
- Overflow closes the attempt/turn with `stream_overflow`; never truncate text
  and later publish an apparently successful terminal.
- Cancellation closes admission and wakes blocked senders immediately.

### Semantic flush boundaries

Flush pending content/reasoning before tool start/finish, scratch reset, attempt
transition, approval, terminal, error, cancellation, or explicit replay/sync
fence. Content and reasoning retain independent order as observed by their
commands; do not reorder one lane around a semantic event to make larger batches.

## Journal and commit protocol

### Record format

Use a versioned append format that detects partial tails and supports bounded
scanning. JSONL v2 is acceptable if each line has schema version, sequence,
variant, and checksum/length validation; length-delimited records are also
acceptable. Recovery must reject/truncate only an incomplete tail and must not
skip malformed data in the middle.

Maintain sparse `seq -> byte offset` checkpoints in memory and optionally in a
rebuildable side index. The journal is authoritative; the index is verified or
rebuilt.

### Writer ownership

The actor hands encoded batches to one per-turn or sharded journal writer in a
bounded blocking pool. It does not hold an async/global lock across I/O. One
writer call may append multiple records. Results carry last sequence and byte
offset.

Initial sync policy:

- flush buffered records on semantic boundary, 64 KiB, or 25 ms;
- `sync_data` at terminal and at a bounded periodic interval no greater than
  250 ms while actively streaming;
- write the commit marker/metadata atomically only after terminal history
  commit, then sync its parent as required by the platform contract; and
- record the last synced sequence so recovery and diagnostics do not confuse
  `written` with `synced`.

Tune intervals from P01, but never remove the finite fences.

### Terminal transaction

1. Stop normal admission and flush coalesced text.
2. Append the canonical terminal event after all prior sequences.
3. Flush and `sync_data` the journal; receive `Synced { through_seq }`.
4. Derive the history turn from canonical terminal data, not accumulated UI
   fragments.
5. Await `SessionStore::append_batch`/transaction `CommitReceipt`.
6. Atomically record committed journal/history generation.
7. Publish the successful terminal to live clients.
8. Close live admission and enter retention/replay lifecycle.

If steps 3–6 fail, do not publish a successful terminal. Publish a typed
durability failure if it can itself be appended safely, retain the journal for
recovery, and expose retry/operator state. Recovery retries idempotently using
turn ID/generation so it cannot duplicate the history row.

## Session writer and store API

Replace the sync error-swallowing trait surface with an async result:

```rust
async fn append_turn_batch(
    &self,
    session: &SessionId,
    expected_generation: Option<u64>,
    turns: &[ConversationTurn],
) -> Result<CommitReceipt, StoreError>;
```

`CommitReceipt` includes backend, session, generation/turn id, committed count,
and durability level. File and database stores implement an actual atomic
batch/transaction or honestly report the narrower guarantee.

The writer actor:

- has bounded message/byte admission and async backpressure;
- acknowledges each job individually or as part of a batch receipt;
- batches at the storage operation, not only in a temporary `Vec`;
- classifies retryable/permanent errors and uses bounded jittered backoff;
- has no synchronous inline fallback on queue full/closed;
- retains bounded dead-letter metadata without full secret/user payloads; and
- participates in graceful shutdown with a deadline and incomplete receipt.

Remove `block_in_place(Handle::block_on(...))` from the store abstraction.
Success counters advance only from `Ok(CommitReceipt)`.

## Replay and live-tail join

For `?since=N`:

1. Authenticate/authorize before allocating replay work (H01).
2. Validate `N` against terminal/current bounds.
3. Acquire a replay-reader permit.
4. Ask the pipeline/journal owner for a stable high-water fence `F`.
5. Read `N < seq <= F` from sparse offset/journal, bounded by event/byte chunks.
6. Attach live receiver starting after `F`.
7. Deduplicate by sequence at the final projection boundary.

If the broadcast receiver lags, repeat from its last delivered sequence. If the
journal is expired, return a typed replay-expired response directing the client
to canonical session history; never spin or silently jump ahead.

Retention is expressed in time and bytes. Committed journals stay through at
least the supported reconnect window, then compact/delete according to recovery
policy. Uncommitted journals are never removed by live-ring eviction.

## Stream v2 protocol

### Envelope

```rust
struct TurnStreamEnvelopeV2 {
    schema_version: StreamSchemaVersion,
    turn_id: TurnId,
    seq: u64,
    emitted_at_utc: DateTime<Utc>,
    event: TurnStreamEventV2,
}
```

### Event families

At minimum the tagged union contains:

- `content_append { text }`, `reasoning_append { text }`;
- `attempt_started`, `attempt_finished`, `model_selected`;
- `status`, `scratch_reset`;
- `tool_started`, `tool_finished`;
- `artifact_presented`, `artifact_updated`, `ui_scene`;
- `approval_required`, `browser_challenge`, `browser_navigated`;
- `worker_ack`, `workshop_ack`, `checkpoint`, `needs_input`;
- `completed { final_turn }`, `cancelled`, and `failed`.

Terminality is determined by terminal variants, not a separately contradictory
boolean. Phase/status/mode are enums. Fields required for a variant are not
optional; optionality remains only when semantically valid.

Generate Rust schema fixtures and TypeScript union immediately. SDKs either
consume v2 directly or use a generated adapter. Delete the handwritten Home
duplicate. Exhaustive TS/Rust tests fail when a new variant lacks handling.

### V1 compatibility

- Serve v2 at a versioned media type or stream endpoint negotiated explicitly.
- Keep `?since=` and monotonic sequence semantics.
- Project each v2 event/batch into one v1 DTO; constant discriminator strings
  exist only in this adapter.
- Record v1 client usage without identity/high-cardinality labels.
- Freeze v1 variants except required safety fixes.
- Remove v1 after supported Home/SDK/Python/integration versions migrate.

## Native bridge

Replace `String` suffix copying with a bounded byte decoder (`BytesMut` cursor,
`split_to`, or a maintained SSE codec). It must handle CRLF, split UTF-8, comments,
multi-line data, event IDs, retry fields, EOF partial frames, and malicious
oversized/no-delimiter input.

For Tauri's event bus, prefer emitting the validated serializable v2 value so
JavaScript receives an object without `JSON.parse`. If Tauri internally
serializes again, measure it; the boundary still must not parse to unused `T`
and then forward the original string. A direct channel/plugin bridge may replace
the generic event bus only if profiling proves material benefit.

Batch native-to-webview notifications within the same latency budget when doing
so preserves semantic order. Never merge across terminal/approval/tool/reset
boundaries.

## Home state and rendering

### Reducer

- Maintain `turn_id -> message_id` and `message_id -> index/entity` maps.
- Buffer adjacent append payloads in nonreactive per-turn accumulators.
- Flush once per `requestAnimationFrame`, or by a bounded timer when frames are
  throttled/backgrounded.
- Apply terminal and interactive approval events immediately after flushing
  prior content.
- Update only the target normalized entity/message; avoid two transcript slices.
- Touch tool/artifact arrays only for corresponding variants.
- Preserve sequence dedupe before buffering and reset maps on session/workshop
  lifecycle transitions.

The current Promise-per-event `streamApplyChain` becomes a bounded event pump.
It must preserve ordering across async side effects without creating one promise
closure per content batch.

### Stable-block Markdown

Represent assistant presentation as:

```text
completed sanitized blocks[] + streaming source tail + terminal metadata
```

The incremental splitter recognizes safe completed boundaries outside open
fenced code, tables/lists requiring continuation, HTML, Mermaid, Liquid, and
other supported constructs. Completed block source hash and render context key
cache parsed/sanitized HTML. Stable keyed components hydrate once.

While a block is incomplete, display escaped text or a deliberately limited
tail renderer. Do not repeatedly run Marked, DOMPurify, highlighting, Mermaid,
Liquid, draw, or image mounting on the accumulated answer. At terminal, parse
the remaining tail once and reconcile with canonical final text. A mismatch
falls back to one full terminal rebuild with a metric, not repeated stream-time
rebuilds.

Move `activeRenderOptions`, heading counts, checkbox indexes, and similar parser
state into a render invocation/context so concurrent render calls are safe.

## Cancellation, failure, and shutdown

- Cancellation atomically closes admission and records the highest accepted
  command/sequence fence.
- Blocked producers awaken with cancellation, releasing byte permits.
- The actor flushes only events at/before the accepted fence according to
  policy, emits exactly one cancellation terminal, and stops within 2 seconds.
- A dead writer fails the pipeline; it does not trigger synchronous async-thread
  I/O or infinite drain.
- Subscriber disconnect affects only that subscriber; no client can block the
  journal owner.
- Graceful daemon shutdown stops new turns, closes admission, drains/syncs for
  at most 5 seconds, and emits an operator-visible inventory of uncommitted turn
  IDs/recovery journals.
- Process-crash recovery reads complete records, truncates only a partial tail,
  and resumes/commits terminal history idempotently.

## Observability

Per stage, record low-cardinality histograms/counters:

- admitted fragments/bytes versus emitted batches/bytes;
- coalescing delay and batch size;
- queue messages/bytes, high-water, blocked-send duration, overflow;
- journal encode/write/flush/sync latency, syscalls, bytes, and last synced seq;
- live ring events/bytes, subscribers, lag/replay/reconnect latency;
- bridge input/frame/output bytes, compactions, decode/emit duration;
- UI buffered batches, reactive commits, lookup duration, missed frames;
- Markdown tail/completed parse, sanitize, hydration, block cache hit/miss;
- history accepted/retried/committed/failed receipts; and
- cancellation/terminal/shutdown drain duration.

Do not label metrics with turn/session IDs, model text, tool input, raw error
bodies, or other high-cardinality/user data. Trace correlation samples synthetic
or explicitly opted-in diagnostic turns.

## Delivery slices

### H03.0 — Baseline and truthful persistence

- Implement P01/P02 benchmark fixtures before structural changes.
- Change session-store append to `Result<CommitReceipt, StoreError>`.
- Stop counting attempts as commits; add failure injection and fresh reload.
- Remove inline blocking overflow fallback; add bounded async acknowledgement.
- Add graceful writer drain with deadline.

### H03.1 — Typed v2 protocol

- Define envelope and discriminated event union in shared types.
- Generate TS/schema fixtures and exhaustive reducer/projection tests.
- Build the single v2-to-v1 adapter and freeze handwritten v1 construction.
- Add Rust/TS/Python/SDK round trips for every variant and invalid state.

### H03.2 — Turn pipeline actor

- Introduce per-turn actor, state machine, message+byte admission, and global
  budgets.
- Convert provider emission to async/backpressured commands.
- Collapse attempt/turn unbounded bridges and remove the no-op parts lock.
- Implement adjacent coalescing and semantic flush boundaries.
- Add BP-001–005 and ISO-007/008/010 tests.

### H03.3 — Buffered journal and replay

- Replace synchronous `TurnEventLog::append` and unbounded event `Vec`.
- Add buffered writer, sync/commit receipts, sparse offsets, and bounded ring.
- Implement gap-free replay/live join and lag recovery.
- Add crash points CR-001–003, ISO-009, BP-005/006, and RET-001.

### H03.4 — SSE/Tauri boundary

- Serve/negotiate v2 and stream already sequenced pipeline events.
- Replace copying `String` parser with bounded byte framing.
- Remove unused generic double decode and emit one representation.
- Add parser fragmentation/CRLF/UTF-8/oversize/cancellation tests and bridge
  profiling.

### H03.5 — Home reducer and stable rendering

- Import generated union and make event handling exhaustive.
- Add turn/message indexes and frame-batched append reducer.
- Replace Promise-per-fragment scheduling.
- Implement completed-block plus streaming-tail Markdown components.
- Remove global renderer state and repeated hydration teardown.
- Run P02 in browser harness and packaged app.

### H03.6 — Migration and deletion

- Migrate first-party Rust/Python/TS SDKs, Home, CLI, TUI, and integrations.
- Measure and announce v1 sunset; remove adapter after support window.
- Delete old queues, DTO duplicate, journal vector, bridge parser, reducer paths,
  and whole-stream renderer.
- Ship canonical protocol, durability, reconnect, and operator docs.

## Verification and exit criteria

H03 reaches **Validated** when:

- P01 streams 10,000 fragments with 0/100/1,000-message transcripts and records
  all required allocation, latency, syscall, queue, replay, and cancellation
  metrics;
- work/syscalls scale with semantic batches, not provider fragments;
- stalled disk/UI/provider stress stays below hard queue/ring/global byte caps;
- CR-001–003 pass every injected write/flush/sync/commit/crash boundary;
- BP-001–008, ISO-001/007–010, and RET-001 pass deterministically;
- successful terminal events always have journal and history commit receipts;
- every v2 variant passes schema/round-trip/exhaustiveness tests;
- replay/live join has no gap/duplicate across every batch boundary;
- P02 proves completed blocks hydrate once and no whole-answer replacement runs
  per fragment;
- users can scroll/type/select during the 100k-character packaged-app stream
  within the accepted frame/input budget; and
- full supported CI and doc checks pass.

PERF-001, DUR-001, MEM-002, TYPE-001, and PERF-005 become **Shipped** only after
v1 compatibility removal, packaged release evidence, rollback/recovery proof,
and canonical documentation are released.

## Canonical documents changed at ship time

- `docs/engine/interactive-streaming.md`, HTTP API, and recovery/durability docs;
- Rust/Python/TypeScript SDK streaming and reconnect references;
- generated stream schema/event reference;
- Home app streaming/rendering behavior and troubleshooting;
- configuration reference for queue/replay/retention limits that are operator
  configurable; and
- contributor benchmark and protocol-evolution guidance.

## Removal ledger

Delete after migration:

- `TurnStreamBridge` and per-attempt unbounded forwarding queues;
- provider-facing `UnboundedSender<StreamDelta>` paths;
- content-delta no-op parts mutex acquisition;
- synchronous `TurnEventLog::append`, per-event `flush`, and unbounded replay
  `Vec`;
- direct sink sequence/publication and duplicate wire-to-journal projection;
- `InteractiveTurnStreamEvent` nullable v1 mega-struct and handwritten Home
  duplicate after sunset;
- unused Rust SSE decode plus raw string re-emit and suffix-copy parser;
- Promise-per-event `streamApplyChain`, transcript scans, and whole-array content
  updates for append events;
- whole-answer Markdown parse/DOM replacement/hydration per delta;
- module-global Markdown render state; and
- session writer inline blocking fallback, swallowed errors, false success
  counters, and no-deadline shutdown.
