# H04 — Persistence ownership and crash consistency

> **Status:** Draft for storage/recovery review
>
> **Accountable owner:** daemon persistence maintainers
>
> **Reviewers:** workspace, feeds, Forge/task runtime, platform/filesystem, release engineering
>
> **Audit findings:** STORE-001 (Critical), STORE-002 (High), MEM-001 (Critical)
>
> **Release gates:** Gate B — trustworthy state; Gate C — bounded hot paths
>
> **Required decision:** [ADR-016](../../docs/architecture/decisions/adr-016-transactional-store-ownership.md)
>
> **Verification:** [Crash/concurrency matrix](verification/crash-concurrency-matrix.md), [performance budgets](verification/performance-budgets.md)

## Outcome

Feed, workspace, and project-task mutations have a single ordered owner,
truthful commit receipts, crash-safe authority, bounded admission, and enforced
retention. Incremental work reaches the persistence owner before expensive
serialization, so debounce and batching eliminate work instead of merely
discarding already-built snapshots.

H04 owns STORE-001, STORE-002, and MEM-001. H03 owns session-history
persistence. H06 owns Forge event replay/checkpoints/blocking Git. H07 owns
vault compare-and-write and indexes. Shared primitives should converge, but
finding closure remains with those plans.

## Current ownership failures

### Feed channels

`FeedStore` has one async `RwLock<HashMap<profile, HashMap<feed, state>>>`.
Cold load awaits full file I/O and parsing under the global write lock. Append
mutates a `Vec`, front-removes at 200 entries, clones the entire channel, drops
the lock, and rewrites/truncates the JSONL file. Older snapshot A can finish
after newer A+B and erase B. Direct write can leave a torn sole copy.

Reads clone more than necessary: count calls tail, tail clones events, and
latest-good clones the tail before scanning. The read cursor is memory-only and
is not part of a durable generation.

### Workspace

Card states, associations, ask jobs, and turn workers mutate in independent
locks. Callers clone/pretty-serialize their entire maps and enqueue strings.
The persistence task debounces only those strings, then directly overwrites
the only snapshots. Revision and feed are separate operations, so a crash can
expose a revision, event, and projections from different logical generations.

On full, closed, or uninitialized queues, `try_enqueue` runs synchronous file
I/O on the caller. Errors are printed and erased. `Flush` acknowledges a queue
barrier after best-effort work, not a durable generation.

Worker and ask-job stores have count/time pruning logic, but pruning occurs
while preparing another whole snapshot. High-frequency interim/output-tail
updates repeatedly serialize data the writer will supersede.

### Project-task runs

`PROJECT_TASK_RUNS` never evicts completed runs. A run duplicates up to 256 KiB
each of stdout/stderr across cumulative strings, replay event chunks, terminal
result, broadcast, and response clones. Front-draining a capped UTF-8 `String`
can memmove most of the buffer for each new chunk. A single global write lock is
held while output is appended, locations parsed, results cloned, and broadcast
events constructed for every run.

The 400-event replay deque has no byte cap. Lagged SSE receivers simply continue
after a gap, so retained replay does not provide reliable reconnect semantics.

## Invariants

1. One owner assigns the commit order for each mutable dataset/key.
2. Registry locks locate owners only; unrelated keys never hold one another
   across disk I/O or owner waits.
3. A caller sends a typed mutation before whole-state clone/serialization.
4. An acknowledged generation never disappears beneath an older late write.
5. `accepted`, `written`, and `synced` are distinct and observable.
6. Errors, short writes, sync failures, rename failures, conflicts, overload,
   cancellation, and shutdown are never converted to success.
7. Recovery returns the last complete committed generation or a later complete
   generation, never torn JSON or a silently older acknowledged snapshot.
8. Derived snapshots/indexes declare their authoritative generation and are
   discarded/rebuilt on mismatch.
9. Every queue, registry, log, ring, spool, and snapshot has count/byte/time
   bounds or a documented active/recovery exemption.
10. Replaceable coalescing never drops nonreplaceable transitions/appends.
11. Retention cannot evict active, cancelling, unacknowledged, or recovery-needed
    records without first rejecting/cancelling their authority explicitly.
12. Shutdown returns the last durable generation or an incomplete inventory.

## Non-goals

- making one actor serialize every store in the daemon;
- choosing one physical backend for all deployments;
- retaining all feed/task output forever;
- making UI notification delivery part of the disk transaction;
- solving session-history persistence owned by H03;
- solving Forge/Coder/vault incremental algorithms owned by H06/H07;
- promising power-loss durability for operations documented as only `written`.

## Common persistence contract

### Mutations and receipts

Define shared dependency-light types:

```rust
enum DurabilityLevel { Accepted, Written, Synced }

struct CommitReceipt<K> {
    store: StoreKind,
    key: K,
    generation: u64,
    durability: DurabilityLevel,
    committed_at: DateTime<Utc>,
}

enum StoreErrorKind {
    Conflict,
    Overloaded,
    RetryableIo,
    PermanentIo,
    Serialization,
    Corruption,
    Cancelled,
    ShuttingDown,
}
```

Receipts never include payload copies. Every public mutation declares the
minimum durability needed before success. UI-only interim state may accept
`accepted`; feed append and ordinary workspace mutation require at least
`written`; terminal transitions, destructive operations, and explicit flush
require `synced` unless a narrower documented product contract is approved.

Domain notifications contain generation and publish after the required receipt.
Consumers compare generations and refetch/project on a gap.

### Owner registry

Use a sharded/dash-map-like registry or a short-held mutex from typed key to
`Arc<OwnerHandle>`. Owner creation is single-flight. A cold owner loads in its
own task; unrelated owners remain available. Idle clean owners can evict under
an LRU/count/byte policy. Dirty, active, or recovery-required owners remain
charged against global capacity and cause new admission to wait/fail when full.

### Bounded admission

Each command reserves permits for its estimated retained bytes before enqueue.
Owners expose command-count and byte limits; the service exposes global owner,
queue-byte, blocking-job, and compaction limits. Async callers await capacity
with cancellation/deadline. There is no sync fallback.

Replaceable dirty signals are keyed/coalesced in bounded owner state. If ten
interim updates for one worker arrive before persistence, the owner serializes
the latest one once. Append and state-transition commands cannot be overwritten
by a later dirty signal.

### File transaction primitive

Create one tested primitive instead of continuing ad hoc `fs::write` helpers:

```text
append_record(root handle, file, record, durability)
replace_snapshot(root handle, file, bytes, durability)
compact_log(root handle, old generation, new snapshot/log)
```

`replace_snapshot` must:

1. create a collision-safe temp file in the same authorized directory;
2. write all bytes and propagate short/error results;
3. set required permissions before publication;
4. flush/sync the temp for the requested level;
5. atomically replace the destination with defined Windows behavior;
6. sync the parent directory where the platform supports/requires it;
7. clean an unpublished temp without following links; and
8. return a receipt only after the requested fence.

`append_record` frames records so recovery can distinguish a partial final
record from middle corruption. It maintains generation/sequence metadata under
the same owner and does not rediscover sequence by replaying the whole file.

Fault injection wraps the filesystem/transaction interface at every matrix
boundary, rather than adding arbitrary test-name branches to production code.

## Feed store design

### Ownership and layout

`FeedRegistry` maps validated `(ProfileId, FeedId)` to a per-feed owner. Each
owner holds:

- next/committed sequence and generation;
- a `VecDeque<Arc<FeedEvent>>` bounded hot tail;
- read cursor clamped to committed sequence;
- append-log handle and bytes/record counters;
- latest-good metadata/reference; and
- compaction/dirty/recovery state.

Use H02 typed IDs/storage keys; raw profile/feed strings never select paths.

### Append

1. Caller reserves command/payload bytes and submits `Append(event)`.
2. Owner assigns the next sequence and encodes one record.
3. Owner appends and obtains the required receipt.
4. Only then update committed metadata/hot tail/latest-good and publish.
5. Return the assigned sequence/generation receipt to the caller.

An append failure does not advance committed sequence. Retry uses an operation
ID/idempotency fence if caller ambiguity is possible.

### Cursor and reads

`SetReadCursor` executes on the same owner, clamps/rejects values beyond the
committed sequence, persists cursor metadata, and returns its generation.
Append plus mark-read can use one compound command when product semantics need
atomicity.

Tail clones only requested event handles/payloads. Count reads metadata.
Latest-good uses retained metadata and clones one result. Cold load happens once
inside the owner and validates snapshot/log generations without a global lock.

### Retention and compaction

Keep the current 200-event user-visible tail as the initial semantic policy,
but enforce retained bytes as well as count. Large events are rejected or stored
by reference according to the feed schema; count alone is not a memory limit.

When the append log exceeds the retained window by a configured ratio/byte
threshold, the owner writes a compacted generation containing retained events
and metadata, syncs/publishes it atomically, then retires the old generation.
Appends arriving during compaction remain ordered by the owner and cannot be
lost behind the swap.

## Workspace state design

### One domain owner, typed commands

Create `WorkspaceStateOwner` with one monotonic generation and typed commands:

- append workspace event;
- remember/prune card column;
- add/remove card association;
- insert/update/archive ask job;
- insert/update/archive turn worker;
- retention tick; and
- flush/shutdown.

The owner holds authoritative in-memory projections and a mutation journal.
Existing sync store façades migrate to async command APIs; they do not mutate a
map first and notify persistence afterward.

### Mutation journal and snapshot

Every command produces one `WorkspaceMutation { generation, operation_id,
event }`. Cross-record invariants and revision increment occur in that mutation.
The journal is the authority. Periodic snapshot contains schema version,
`applied_generation`, integrity value, card/association/job/worker projections,
and required feed/revision metadata.

Recovery loads the newest valid snapshot and replays the journal tail. If the
snapshot is corrupt or ahead/divergent, discard it and replay authority. Partial
tail is truncated/recovered only if framing proves it is the final incomplete
record; middle corruption is an operator-visible failure.

### Coalescing and lifecycle

Interim ask-job text and worker output/scratch tails are replaceable by record
key until the next flush/semantic transition. Running/succeeded/failed/cancelled,
archive, synthesis-delivered, association, and card movement boundaries are not
discarded.

Debounce happens inside the owner before snapshot serialization. Journal append
still records required transitions; checkpoint serialization happens once at
the expiry/size threshold, not once per upstream update.

Current workspace retention settings remain the product-level starting point:
terminal cards hide after 24 hours and archived records wipe after 7 days by
default, within existing configuration bounds. H04 adds hard record and byte
ceilings so unarchived completed records cannot grow indefinitely. Active and
recovery-required records consume a separate admission budget rather than being
silently archived.

## Project-task run design

### Per-run ownership

Replace the global map of full `ProjectTaskRunStore` values with a bounded
registry of `Arc<ProjectTaskRunHandle>`. Each owner serializes its process
state, sequence, output ring/spool, problem locations, preview readiness,
terminal metadata, and subscribers. The registry lock is held only for lookup,
insert, eviction, and accounting.

### Output representation

Store output bytes once in a circular byte ring or bounded spool file with
sequence/offset metadata. Decode lossy/strict UTF-8 only for response frames.
Terminal result refers to the output range/spool; it does not clone stdout and
stderr into the run, result, terminal event, and broadcast simultaneously.

Replay events carry sequence and byte ranges or shared bytes. Broadcast lag
causes replay from the ring. If `since` predates retained output, emit a typed
`gap { oldest_seq, next_seq, truncated: true }` before current state; never
continue silently.

### Initial safety limits

Validate/tune these starting limits with P10:

| Resource | Initial bound |
| --- | --- |
| Concurrent active runs | 32 globally, with per-workshop/per-principal limit |
| Retained terminal runs | 64 globally |
| Live output per run | 512 KiB total across streams |
| Global retained task output | 32 MiB |
| Replay events per run | 400 plus byte cap |
| Terminal reconnect window | 15 minutes |
| Problem locations | 100 per run |

Long-running ready tasks remain active and are not terminal-evicted; they count
against active capacity. Terminal metadata may persist longer without output if
the product needs history. Preview grants and child handles are revoked/removed
as part of eviction. Fetch after expiry returns a typed not-found/expired result.

## Crash recovery and compaction

Every journal/snapshot store follows:

```text
load newest valid snapshot S at generation G
  -> verify schema/integrity/authority offset
  -> replay complete records G+1..N
  -> reject middle corruption or sequence regression
  -> repair/truncate only provable incomplete tail
  -> expose generation N
```

Compaction is an owner command:

1. fence current generation and select retained state;
2. create/sync new snapshot and compacted log generation;
3. include source generation/hash and tail join point;
4. atomically publish manifest/generation;
5. sync parent/transaction;
6. continue/join mutations admitted after the fence; and
7. retire old files only after the new generation survives reopen validation.

Interrupted compaction leaves either the prior complete generation or the new
complete generation. Temporary/orphan files are detected and safely cleaned at
startup under H02 path authority.

## Concurrency and publication

- Per-key owner mailboxes define mutation order; callers get receipts in commit
  order, not task scheduling order.
- Unrelated feed owners and task runs progress independently.
- Workspace uses one owner because its revision and projections cross records.
- Domain notifications/SSE carry committed generation and occur after the
  operation's declared durability.
- Readers may use immutable `Arc` projections stamped with generation; they do
  not hold the mutation owner while formatting responses.
- A stale reader can return its stamped generation or retry; it cannot publish
  a mutation based on stale state without an expected-generation check.
- Cancellation before admission commits nothing. After owner acceptance, the
  receipt/final query resolves whether the mutation committed; ambiguous caller
  cancellation does not roll back an already durable record.

## Shutdown and failure behavior

On daemon shutdown:

1. close external mutation admission;
2. issue flush fences to all active owners;
3. finish required appends/snapshots/compactions under a global deadline;
4. return/record each store's last synced generation and pending operation IDs;
5. terminate task children according to task policy; and
6. leave recoverable journals for anything incomplete.

Default graceful drain target is 5 seconds, subject to measurement and explicit
operator configuration. Reaching the deadline never causes a success receipt;
diagnostics identify incomplete store/key generations without user payload.

## Observability

Record bounded, low-cardinality metrics for:

- owners active/idle/dirty/recovering and registry count/bytes;
- command count/byte depth, wait/overload/cancellation, and high-water;
- mutation size, commit latency/level, generation, retry/failure class;
- append/write/sync/rename/parent-sync calls and bytes;
- snapshot serialization time/bytes and compaction amplification;
- feed hot-tail count/bytes, cursor generation, latest-good cache hits;
- workspace journal/snapshot generation and coalesced update count;
- task active/terminal count, output/ring/spool bytes, replay gaps, evictions;
- recovery source/tail records/corruption/repair; and
- shutdown drained/incomplete owner counts.

Do not label with raw profile/feed/job/work/run IDs, event bodies, output text,
paths, or arbitrary errors. Local diagnostics may expose hashed/opaque keys and
escaped filenames under explicit operator action.

## Delivery slices

### H04.0 — Shared receipts, atomic publication, and fault points

- Define durability/receipt/error/generation types aligned with ADR-015.
- Build and cross-platform test append-record and atomic-snapshot primitives.
- Add injected failures/short writes at the crash matrix boundaries.
- Remove false flush success and synchronous async-path fallback.
- Establish P03/P10 baseline fixtures.

### H04.1 — Feed per-key owners

- Add typed feed/profile keys and single-flight owner registry.
- Convert cold load, append, cursor, tail/count/latest-good to owner state.
- Migrate to true append log and bounded `VecDeque`/bytes.
- Add fenced compaction, legacy migration, CM-001–003 and CR-004/005.

### H04.2 — Workspace typed journal owner

- Define mutation enum, generation, projections, and async command API.
- Move card/association/ask-job/turn-worker mutation into the owner.
- Coalesce replaceable updates before serialization.
- Add journal-tail recovery and atomic generation snapshot.
- Remove pre-serialized snapshot queue methods and direct revision file.

### H04.3 — Workspace retention and migration

- Add hard count/byte budgets alongside hide/wipe policy.
- Migrate legacy JSON snapshots and feed/revision into generation zero/new log.
- Quarantine corrupt/ambiguous inputs; retain rollback copies.
- Add CR-006, CM-004/005, BP-008, RET-003, and shutdown tests.

### H04.4 — Project-task run ownership

- Extract per-run actor/handle from `forge_api.rs`.
- Introduce byte ring/spool, sequence ranges, gap repair, and lightweight result.
- Add global count/byte/TTL admission and eviction.
- Remove completed children/preview grants/output at lifecycle boundaries.
- Add thousands-of-noisy-runs soak and lag/reconnect tests.

### H04.5 — Closure and shared primitive adoption

- Run P03/P10 and supported-platform crash/concurrency matrices.
- Delete compatibility writers/readers after rollback window.
- Offer the proven primitives to H06/H07 without expanding H04 ownership.
- Ship canonical storage, retention, recovery, configuration, and upgrade docs.

## Migration and rollback

Each store migration is versioned and restartable:

1. stop/serialize mutations for that key/store;
2. inventory and parse the legacy source read-only;
3. write new generation zero/journal through the new primitive;
4. reopen and compare semantic state/counts/digests;
5. atomically mark the active layout version;
6. retain the legacy source through the rollback window; and
7. resume the owner and record migration evidence.

Migration never deletes corrupt input or treats an unreadable file as an empty
successful store. Rollback selects the last validated layout before new writes
or uses an explicit reverse/export tool; it does not allow two layouts to accept
concurrent mutations.

Project-task runs are currently ephemeral. Upgrade may expire existing in-memory
runs on daemon restart, but the product must report this; no unsafe disk
migration is invented for state that was never durable.

## Verification and exit criteria

H04 reaches **Validated** when:

- CR-004–CR-008, CM-001–CM-005, BP-008, RET-002–RET-005, and applicable
  deletion cases pass with deterministic barriers/fault points;
- concurrent feed appends survive forced reverse completion and fresh reopen;
- workspace restart yields a valid snapshot plus exact journal tail generation;
- no snapshot write can expose torn JSON as authority;
- queue full/closed/absent never performs blocking fallback on an async caller;
- flush/shutdown reports real synced generations and injected failures;
- P03 shows append/update work is O(delta) amortized and debounce occurs before
  whole-state serialization at 1/100/500 records and 1–20 producers;
- P10 crosses retention thresholds repeatedly and RSS/registry/spool bytes
  return to the declared steady-state envelope;
- task replay repairs lag or emits an explicit gap, and terminal runs evict by
  count/bytes/TTL while active runs remain safe;
- migrations survive interruption at every publication boundary; and
- supported CI, platform, docs, and secret/path-redaction checks pass.

STORE-001, STORE-002, and MEM-001 become **Shipped** only after migration,
rollback, packaged soak/crash evidence, operator diagnostics, and canonical
documentation are released.

## Canonical documents changed at ship time

- feed/workspace/project-task engine HTTP and SDK documentation;
- persistence, data-directory, backup, recovery, and upgrade runbooks;
- configuration reference for queue, retention, spool, and compaction limits;
- Home/TUI behavior for expired output, replay gaps, overload, and degraded
  persistence; and
- contributor guidance for store receipts, owner boundaries, fault injection,
  and performance benchmarks.

## Removal ledger

Delete after migration:

- global feed map lock across cold I/O and whole-channel snapshot rewrites;
- `Vec::remove(0)` feed retention and clone-based count/latest-good reads;
- memory-only/unfenced feed cursor;
- `queue_snapshot_*` APIs accepting pre-serialized `String` bodies;
- direct sole-file workspace `fs::write` and best-effort flush;
- `apply_sync_fallback` and all full/closed/uninitialized sync I/O fallbacks;
- caller-owned workspace maps as mutation authority and separate revision write;
- full-map serialization in ask-job and turn-worker update paths;
- unbounded `PROJECT_TASK_RUNS` full-value registry;
- front-drained stdout/stderr strings and duplicated terminal/replay payloads;
- lagged-task-stream silent `continue`; and
- incomplete generic `atomic_write` uses where the new transaction primitive is
  required.
