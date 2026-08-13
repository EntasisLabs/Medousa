# ADR-016: Transactional store ownership and crash consistency

> **Status:** Proposed
>
> **Date:** 2026-08-13
>
> **Decision owners:** daemon persistence maintainers
>
> **Related:** [ADR-015](adr-015-bounded-durable-turn-pipeline.md), [H04 execution plan](../../../architecture/hardening/04-persistence-and-crash-consistency.md)

## Context

Several daemon stores keep authoritative mutable state in a lock, then perform
its persistence later through a different owner or no owner at all.

The feed store protects in-memory mutation with one global lock, clones the
whole retained feed, releases the lock, and truncates/rewrites the file. Two
concurrent appends can persist snapshots in reverse order and lose an event.
Cold I/O occurs while the global map is write-locked, so one feed stalls every
profile and feed.

Workspace card, association, ask-job, and turn-worker callers clone and
serialize complete maps before sending strings to a debouncing writer. The
writer discards most already-expensive snapshots, directly overwrites the only
file, erases errors, and falls back to synchronous filesystem I/O on an async
caller when overloaded. Its flush acknowledgement does not mean writes
succeeded or became durable.

Project-task runs retain every completed run in a process-global map. Output is
duplicated between cumulative strings, replay events, terminal results, and
broadcast values. A fixed per-run text cap therefore does not bound global
memory, and front-draining a full UTF-8 string makes sustained noisy output
expensive. A lagged subscriber skips events even though replay state is retained.

These are instances of one architectural error: mutation order, persistence
order, acknowledgement, recovery, derived state, and retention do not share a
single accountable owner.

## Decision

### 1. Every mutable dataset has one commit owner

Each mutable dataset is assigned one serialized owner at its natural contention
boundary. The owner alone assigns generations/sequences, applies mutations,
writes authoritative storage, publishes derived/live updates, and performs
retention/compaction.

The chosen boundaries are:

- one owner per `(profile_id, feed_id)` channel;
- one workspace state owner for card state, associations, ask jobs, turn
  workers, revision, and their cross-record invariants; and
- one owner per active project-task run, behind a bounded global run registry.

There is no process-wide writer that serializes unrelated feeds, task output,
and workspace state. Registry locks locate owners and enforce global admission;
they are never held across file I/O or waits on an owner.

### 2. Callers submit typed mutations, not snapshots or paths

Callers send a domain command containing the expected generation when needed,
the minimal changed data, and an acknowledgement channel. They do not clone a
whole store, serialize it, choose a path, or publish a successful state before
the owner accepts it.

Replaceable high-frequency mutations, such as an interim text tail, may be
coalesced by key before serialization. Nonreplaceable transitions, appends,
terminal states, cursor changes, and deletion are ordered individually or in
one explicit transaction.

### 3. Stores use explicit commit semantics

Persistence operations return a shared result vocabulary:

```text
CommitReceipt {
  store, key, generation, durability, committed_at
}

durability = accepted | written | synced
```

`accepted` means only that a bounded owner has admitted the mutation. `written`
means a complete authoritative record or atomic snapshot was published to the
filesystem/database. `synced` means the store's declared data and required
directory/transaction durability fence completed.

Failures are classified as conflict, overload, retryable I/O, permanent I/O,
serialization/schema, corruption, cancelled, or shutdown. A failure is never
logged and converted into success. Flush returns the last durable generation or
an error; it is not merely a queue barrier.

### 4. Authoritative storage is incremental; snapshots are rebuildable caches

Append-oriented domains use versioned, checksummed/length-validated records and
append only the mutation. State-oriented domains use a write-ahead mutation
journal plus periodic snapshots, or a transactional database with equivalent
generation and recovery semantics.

Snapshots include schema version, applied journal generation/offset, and an
integrity value. They are written to a new file, flushed/synced according to the
commit policy, atomically replaced, and followed by a parent-directory sync
where required. Recovery loads a valid snapshot, replays only its tail, and
rejects divergence. A corrupt or absent snapshot is rebuilt from authority.

Direct truncate/write of the sole authoritative copy is forbidden. A helper
called `atomic_write` is not sufficient unless it checks every write/sync/rename,
uses collision-safe `create_new`, preserves required permissions, synchronizes
the parent directory, cleans temporary files safely, and has defined Windows
replacement behavior.

### 5. Compound mutations have one transaction boundary

A domain operation that changes several fields/records exposes one generation
and one acknowledgement. Feed sequence assignment plus event append is one
operation. A read cursor cannot advance beyond the owner's committed feed
sequence. Workspace transitions that update a worker/job/card and revision are
one ordered mutation, not separate best-effort files.

Compare-and-write checks occur inside the same owner/transaction as commit.
This policy is reused by H07 for vault mutation but H07 owns that implementation.

### 6. Retention is part of the store contract

Every in-memory registry, replay ring, journal, snapshot, output spool, and
completed record declares count, byte, and time limits; eviction order; active
and recovery exemptions; on-disk behavior; and operator diagnostics.

Owners track retained bytes incrementally. Eviction removes or compacts the
single owned payload instead of cloning it into several terminal/replay forms.
Active, cancelling, recovery-required, or unacknowledged records are protected
from ordinary retention and instead consume explicit admission capacity.

No caller can pin a record indefinitely merely by disconnecting or retaining a
broadcast receiver. When requested replay is outside retained history, the API
returns an explicit gap/expired result.

### 7. Blocking persistence runs in bounded services

Filesystem/database work that can block runs behind a bounded store-specific
worker/semaphore. Async callers await capacity or receive a typed overload
result. A full/closed channel never causes synchronous blocking I/O on the
caller as a fallback.

Shutdown closes admission, drains owners with a deadline, performs required
sync/compaction, and returns an inventory of incomplete generations. Restart
uses journals/transactions to recover; it does not assume graceful shutdown.

## Store-specific choices

### Feeds

Feed files become real append logs. A per-feed owner loads once, assigns the
next sequence, appends one record, and keeps a bounded `VecDeque`/ring for hot
tail reads. Cursor and channel metadata use generation-fenced metadata records.
Compaction writes the retained window to a new generation and atomically swaps
it without racing appends.

### Workspace

Workspace state uses one typed mutation journal and generation. Card states,
associations, ask jobs, turn workers, and revision project from that ordered
state. The owner coalesces replaceable updates before it serializes them and
periodically checkpoints a generation-stamped snapshot. Domain notifications
publish only after the mutation reaches its declared commit level.

### Project-task runs

Each active run owns a bounded byte ring or spool plus sequenced replay events.
The global registry stores lightweight owner handles and terminal metadata, not
duplicated cumulative output. Completed owners expire after a reconnect window
under hard global count/byte limits. Lagged consumers replay from the retained
ring or receive an explicit gap.

## Consequences

### Positive

- Accepted mutations cannot be overwritten by an older late snapshot.
- Append/update work scales with the mutation rather than retained global state.
- Cold or slow unrelated stores progress independently.
- Crash recovery has a complete committed prefix and verifiable snapshot tail.
- Flush, shutdown, metrics, and API success describe real persistence outcomes.
- Completed tasks reach a measurable steady-state memory envelope.

### Costs and migration

- Synchronous store APIs become asynchronous or return explicit admission and
  commit handles.
- Workspace callers and projections must move from shared map mutation to typed
  commands.
- Existing feed/workspace files require versioned migration and rollback.
- Owners, journals, compaction, byte accounting, and fault injection introduce
  operational complexity that must be shared rather than reimplemented casually.
- Task output replay can expire; clients must handle gap/reset responses.

### Relationship to other decisions

- ADR-015 owns the turn pipeline and session-history terminal receipt. ADR-016
  supplies the general store vocabulary and applies it to feed, workspace, and
  task-run state; it does not create a second turn writer.
- ADR-014 owns typed identifiers and handle-relative path authority. All stores
  here consume those types/root capabilities rather than inventing sanitizers.
- H06 may reuse these journal/snapshot primitives for Forge/Coder, but owns its
  incremental state model and blocking Git execution.
- H07 may reuse the transaction/receipt contract for vault compare-and-write,
  but owns vault indexes and external editor reconciliation.

## Verification

Implementation is governed by CR-004–CR-008, CM-001–CM-005, BP-008,
RET-002–RET-005, and relevant deletion cases in the
[crash/concurrency matrix](../../../architecture/hardening/verification/crash-concurrency-matrix.md),
plus P03 and P10 in the [performance budgets](../../../architecture/hardening/verification/performance-budgets.md).

## Code anchors

- `src/feed_store.rs` — global lock, snapshot rewrite, lost-update race
- `src/workspace/persist.rs` — pre-serialized debounce, direct writes, sync fallback
- `src/workspace/store.rs` — card/association full snapshots and separate revision
- `src/workspace/ask_job_store.rs` — full-map serialization
- `src/agent_runtime/turn_worker/store.rs` — full-map serialization and retention
- `src/agent_runtime/turn_worker_job.rs` — high-frequency tail updates
- `src/daemon/forge_api.rs` — unbounded project-task run registry and output copies
- `src/session.rs` — incomplete generic `atomic_write` contract
- `crates/medousa-forge/src/store.rs` — stronger snapshot/sync precedent
