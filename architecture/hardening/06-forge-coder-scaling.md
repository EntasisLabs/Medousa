# H06 — Incremental Forge and Coder runtime

> **Status:** Draft for Forge/Coder review
>
> **Accountable owner:** Forge and Coder runtime maintainers
>
> **Reviewers:** daemon API, persistence, Git/platform, agent runtime, observability, release engineering
>
> **Audit findings:** PERF-002 (Critical), PERF-004 (Critical), ASYNC-001 (High)
>
> **Release gate:** Gate C — bounded hot paths
>
> **Required decisions:** [ADR-016](../../docs/architecture/decisions/adr-016-transactional-store-ownership.md), [ADR-017](../../docs/architecture/decisions/adr-017-request-scoped-runtime-context.md)
>
> **Dependencies:** H02 filesystem authority; H03 durable turn pipeline; H05 request context and cancellation
>
> **Verification:** [Performance budgets P04/P05](verification/performance-budgets.md), [crash/concurrency matrix](verification/crash-concurrency-matrix.md)

## Outcome

Forge mutations perform work proportional to their event batch, ordinary loads
replay only a validated snapshot tail, and listings read a bounded catalog
projection rather than opening every history. Coder persists cheap logical
boundaries without inspecting the repository and performs workspace integrity
observation only when mutation/resume semantics require it. All filesystem,
Git, hashing, sync, and subprocess work runs through bounded, cancellable
services outside Tokio request threads.

H06 owns Forge event-store scaling, Forge/Git blocking execution, and Coder
checkpoint/observation cost. H04 supplies shared transaction, receipt, and
retention vocabulary. H03 owns the canonical turn journal; H06 may reference
it but must not create a second full transcript history. H05 owns the request
context, capability, task group, and cancellation root passed into this work.

## Current cost and ownership failures

### Forge event storage

`EventStore::append` calls `last_seq`, and `last_seq` calls full `replay`.
Replay first collects the complete JSONL file into `Vec<String>`, then parses it
into a second `Vec<TransitionEvent>`. Every accepted event therefore rereads
all prior events and performs `sync_all`.

The caller discards information the store already computed. `transition`
appends an event, ignores its returned sequence, replays again to recover that
sequence, clones the full `WorkItem`, pretty-serializes a snapshot, syncs the
temporary file, renames, and syncs the directory. `persist_fresh` also replays.
Lease generation replays history merely to count acquisitions.

`Forge::load` replays the whole log before checking whether the snapshot is
current. `Forge::list` loads every item, and registration lists every item to
collect slugs. The result is O(history) append, O(history) cached load, and
O(all histories) list; a growing item pays O(n²) parsing over its lifetime.

### Async daemon surface

The Forge HTTP module declares async handlers around synchronous `Forge` and
`GitEngine` methods. Those methods scan directories, parse/serialize logs,
sync files, inspect indexes, and wait on `std::process::Command::output`,
including network fetch/pull/push. A few isolated `spawn_blocking` calls do not
bound aggregate work. Tokio workers can be occupied while unrelated health,
stream, cancel, and UI requests wait.

Long operations and domain transitions are also too tightly coupled. Holding
an item/repository lock while a network subprocess runs prevents progress and
makes cancellation ambiguous; releasing it without an operation generation
allows stale completion to publish over newer state.

### Coder checkpointing

Start, logical boundaries, status changes, and resume checks all call
`refresh_runtime_metadata`. One refresh loads Forge, resolves/canonicalizes the
worktree, spawns Git commands for root/branch/HEAD/status, materializes a full
binary diff, and hashes every byte of every untracked regular file. A model-only
round pays the same repository audit as a filesystem-mutating tool.

Checkpoint saving clones and bounds large transcript/invocation collections,
repeatedly serializes them to discover size, and removes index zero until they
fit. It then pretty-serializes and replaces the entire checkpoint. A safe
logical recovery fence is consequently coupled to repository file count,
dirty-byte volume, and repeated O(n) shifts/serialization.

## Invariants

1. One per-item owner assigns Forge event sequence, lease generation, item
   generation, operation fences, snapshot generation, and commit order.
2. Steady append/mutation never replays historical events to discover current
   tail metadata or a value returned by the append itself.
3. A snapshot is accepted only with a verified log anchor; load replays from
   its byte offset, not from event one.
4. A list/slug lookup reads a bounded catalog projection, not every item log.
5. One domain command commits its event batch under one declared durability
   fence and returns the assigned sequences/generation.
6. A stale lease, operation completion, snapshot, catalog update, or checkpoint
   observation cannot replace a newer generation.
7. Async request/turn tasks perform no blocking filesystem, Git, sync, hashing,
   or subprocess wait inline.
8. Blocking work reserves bounded count/byte/process capacity before dispatch;
   queue full/closed never falls back to inline execution.
9. Long Git operations have explicit timeout, cancellation, output bounds, and
   child-process cleanup. An abandoned result cannot publish state.
10. Logical checkpoint cost scales with its delta and is independent of
    repository size when workspace generation is unchanged.
11. Workspace observation states whether it is exact, conservatively dirty, or
    incomplete. Incomplete observation cannot authorize automatic resume.
12. No tool call with uncertain completion is replayed automatically.
13. Checkpoint transcript/invocation retention is byte-accounted incrementally;
    bounding does not repeatedly serialize or front-remove a `Vec`.
14. Compaction and recovery preserve a complete committed prefix and do not
    globally stall unrelated work items.

## Non-goals

- abandoning event sourcing or Git-governed worktrees;
- weakening lease fencing, evidence sealing, or resume safety for speed;
- promising constant-time cold recovery after corruption;
- treating a filesystem watcher as infallible authority;
- retaining all event/checkpoint payloads in hot storage forever;
- moving unlimited sync work into `spawn_blocking` and calling it fixed;
- redesigning project-task output retention owned by H04; or
- duplicating H03's canonical turn stream inside Coder checkpoints.

## Forge item ownership

### Per-item owner and registry

Introduce a `ForgeItemRegistry` mapping validated `WorkId` to a lightweight
`Arc<ForgeItemHandle>`. Creation is single-flight; the registry lock is held
only for lookup/admission. Each item owner serializes typed commands:

```text
Register
AppendBatch { expected_generation?, events, durability }
Transition { expected_state/generation, to, reason, operation? }
AcquireLease / ReleaseLease
BeginOperation / CommitOperation / AbortOperation
ReadProjection { minimum_generation? }
CheckpointSnapshot / Compact
```

The owner retains current folded projection, next sequence, last record offset
and hash, item/lease/operation generations, dirty snapshot state, and compact
summary. It receives ADR-016 byte/count permits before enqueue. Unrelated item
owners progress independently; repository-mutating commands additionally use
the explicit keyed repository lane described below.

Callers submit intent, not a cloned `WorkItem`, path, or pre-serialized event.
The owner validates state/lease/generation, constructs the minimal event batch,
appends it, applies the fold once, publishes catalog/snapshot dirtiness, and
returns:

```text
ForgeCommitReceipt {
  work_id, item_generation,
  first_seq, last_seq, log_offset,
  durability, operation_generation?
}
```

Every event of one compound transition remains individually auditable, but the
batch has one append/write and one requested sync fence. Event payload failure
cannot leave an externally acknowledged half-transition. Use returned sequence
and generation everywhere; delete replay-to-rediscover calls.

### Log v2 and tail authority

Use a versioned framed append format with record length, sequence, schema,
payload, and checksum/hash-chain link. JSON payloads are acceptable inside the
frame. The framing must distinguish a partial final record from middle
corruption and allow bounded reverse-tail recovery. Newline position alone is
not a sufficient commit marker.

On open, recover tail metadata once from the last valid frame or a verified
tail sidecar/sparse index. The live owner then advances it in memory with each
append. If metadata is missing or inconsistent, scan/repair once under bounded
recovery admission; do not repeat the scan per command.

Snapshot envelopes contain at least schema/model version, applied event
sequence, next log byte offset, anchor record hash, item generation, and folded
projection integrity. Load validates the anchor at/preceding the recorded
offset and streams only later frames directly into the fold. It never builds a
`Vec<String>` or full tail `Vec<Event>` merely to fold.

Existing JSONL stores migrate per item under an exclusive owner:

1. stream-validate/fold v1 once and record the last complete sequence;
2. write v2 log/snapshot/tail metadata beside v1;
3. sync files and directory, then atomically publish the store generation;
4. retain v1 for rollback until the release/migration fence; and
5. resume idempotently after a crash using the generation marker.

### Snapshot and compaction

Snapshot is a derived cache. The owner marks a generation dirty after commit
and coalesces snapshot work by item. The snapshot worker serializes an immutable
projection for an exact generation; publication compare-checks that generation.
A late snapshot may remain a valid older cache but cannot be labeled current or
replace newer metadata.

Trigger snapshot/segment compaction by tail event count and bytes, not every
transition. Initial triggers are 1,000 tail events or 8 MiB, whichever comes
first, then tune under P04. Compaction reserves separate blocking/I/O permits
and yields to foreground commits. Canonical audit history may move to sealed,
checksummed segments according to retention policy; it is not silently deleted.

### Catalog projection

Maintain a rebuildable `ForgeCatalog` keyed by `WorkId` with slug, title,
state, owner display reference, updated time, active-attempt summary, item
generation, and snapshot/log anchor. It supports list/filter/sort/pagination and
a unique-slug index without loading item histories.

The item commit publishes its catalog delta after the authoritative log receipt.
Catalog generation identifies its source item generation. On missing/stale
entries, return an explicit rebuilding/stale diagnostic or repair from validated
item projections in bounded background work. Registration reserves the slug in
the same catalog transaction/fence as item creation so concurrent registration
cannot allocate duplicates.

## Bounded Forge/Git execution

### Service classes

Create a daemon-owned `ForgeExecutionService` with separate bounded classes:

| Class | Examples | Scheduling rule |
| --- | --- | --- |
| Store I/O | open/recover/append/sync/snapshot | per-item order; bounded blocking pool |
| Repository metadata | root/HEAD/status/branch | coalesce reads by repo generation |
| Local mutation | index/worktree/commit/worktree create/remove | exclusive keyed repo lane |
| Network Git | fetch/pull/push | strict global + per-remote/per-repo permit |
| Observation/hash | diff/untracked/checkpoint validation | low-priority byte/time budget |
| Compaction/migration | log/snapshot/catalog repair | background permits; foreground yields |

Admission reserves command count, estimated retained bytes, subprocess slot,
and relevant keyed lane before blocking dispatch. Defaults are configurable and
measured, with conservative starting caps: 64 queued commands globally, 8
blocking jobs, 2 network Git processes, 1 mutating operation per repository,
and 2 observation jobs. Per-principal/session limits from H01/H05 prevent one
caller from consuming the global budget.

No async/global registry lock is held while awaiting this service. Local
synchronous library calls run in bounded blocking workers. Network/long-lived
Git uses `tokio::process::Command` (or an owned child supervisor) with
`kill_on_drop`, explicit deadline, process-group/tree termination where
supported, bounded stdout/stderr capture, `GIT_TERMINAL_PROMPT=0`, and the
existing hidden-window behavior on Windows.

### Long-operation transaction

Provision/fetch/sync/attempt cleanup uses a fenced three-phase protocol:

1. item owner validates state and commits `OperationStarted` with operation and
   expected item/repository generations;
2. execution service runs under the keyed repository lane without holding the
   item owner, streaming bounded progress; and
3. completion submits `CommitOperation`/`AbortOperation` with the exact fence.

The owner accepts the completion only if the operation is still current and
observed repository identity satisfies its contract. Cancellation requests
child termination and records a cancelling state; if OS/process cleanup is
uncertain, the operation remains recovery-required rather than publishing
success. Retry uses operation IDs to avoid duplicate side effects.

## Coder checkpoint design

### Separate logical state from workspace observation

Replace the monolithic snapshot with two generation-linked records:

```text
LogicalCheckpoint {
  checkpoint_generation, turn_handle/id, safe_boundary,
  counters/scratch deltas, transcript_cursor,
  completed_tool_boundary, outstanding_boundary,
  forge work/attempt/lease generations,
  required_workspace_generation,
  latest_observation_generation?
}

WorkspaceObservation {
  observation_generation, workspace_generation,
  worktree identity, Forge environment generation,
  HEAD/index identity, dirty state/digest,
  changed-path summary, completeness, limits hit
}
```

Model-only completion, status changes, approval/user waits, and other logical
boundaries append a small typed checkpoint delta and receive a durability
receipt. They do not call Git or inspect files. A tool batch records its
completed calls only after every call has a terminal result, maintaining the
existing protocol-safe boundary.

H03's durable turn journal is the source for canonical messages/tool events.
The checkpoint stores a verified cursor/digest and only the provider-normalized
minimum not reconstructible from H03. If normalized segments must be retained,
store content-addressed/bounded segments once and reference them; do not clone
the growing transcript and invocation history into every checkpoint.

The checkpoint owner maintains byte counts as deltas enter bounded `VecDeque`
or segment indexes. Retention evicts whole safe prefix segments while preserving
tool-call/result pairing. It serializes once per journal/snapshot write; no
`serialized_size` loop or `remove(0)` remains.

### Workspace generation and invalidation

All Coder-authorized filesystem/Git mutation tools mark the governed workspace
generation dirty before execution and resolve it after the result is known.
The repository watcher contributes external-change generations. Watcher
overflow, restart, missed-event suspicion, direct unmediated access, or metadata
ambiguity sets the state to `Unknown`; it never proves cleanliness.

A logical checkpoint reuses an exact observation only when its workspace,
Forge environment, HEAD/index, and dirty generations still match. Mutating tool
completion schedules/coalesces one new observation. Several boundaries while
the same observation is running share its future; they do not queue duplicate
full scans.

Resume always performs or obtains a current exact observation and compares it
with the safe boundary. An unchanged watcher generation is an optimization
hint within the governed process, not sole proof after restart or watcher loss.

### Bounded observation

Observation runs in the low-priority bounded service and:

- validates H02 worktree/root authority before Git/file access;
- obtains repository identity/HEAD/status once per observation;
- streams diff stdout directly into the digest instead of materializing a
  binary patch;
- hashes untracked files in fixed buffers under per-file, aggregate byte, file
  count, and wall-time limits;
- caches untracked digests by canonical authorized path, file identity, size,
  high-resolution timestamps, and workspace generation, with conservative
  invalidation; and
- records every limit/skip/error in the observation completeness state.

Initial exceptional-work ceilings are 100,000 untracked entries, 1 GiB hashed
per file, 4 GiB aggregate, and 30 seconds for explicit resume validation; P05
must tune them across supported machines. Ordinary post-mutation observation
uses a shorter 5-second/512-MiB aggregate budget. Exceeding a limit persists an
`Incomplete` observation and keeps logical recovery data, but automatic resume
is denied with a precise diagnostic. It never silently calls a partial digest
exact or reads an unlimited multi-gigabyte file on the turn thread.

## Durability and recovery

Forge item logs use ADR-016 receipts. State/lease/operation transitions require
at least `written`; evidence seals, destructive publication, and explicit
checkpoint/flush boundaries require `synced` as their product contract dictates.
Snapshot/catalog notifications follow the authoritative receipt and include
generation.

Checkpoint logical deltas use a versioned framed journal or transactional store
with generation and checksum. A safe resume boundary is acknowledged only after
its logical record and referenced H03 cursor/segments reach the required
durability. Workspace observations are immutable generation records and may be
recomputed; the logical boundary records which exact observation it relied on.

Recovery:

1. validates/migrates the item log and loads snapshot plus tail;
2. reconstructs current item/lease/operation generations;
3. identifies started but uncommitted operations as recovery-required;
4. loads the last complete logical checkpoint prefix;
5. verifies referenced H03/provider-normalized segments and tool pairing;
6. obtains an exact current workspace observation; and
7. resumes only if authority, generations, observation, and protocol boundary
   all match. Otherwise it offers explicit inspect/restart/manual recovery.

## Observability

Record per operation, without repository content or credentials:

- item ID hash, item generation, event sequence/tail bytes, events replayed,
  snapshot/catalog generation, append/sync/compaction latency;
- owner queue commands/bytes, admission wait/reject, lock/lane hold time;
- blocking class, queue/worker utilization, subprocess count/runtime/exit,
  cancellation/kill outcome, bounded output bytes, executor-delay canary;
- checkpoint generation/delta bytes, serialization passes, retained bytes,
  boundary pause, transcript cursor/segment count;
- workspace generation, observation cache hit/coalescing, Git command count,
  entries statted, bytes diffed/hashed, limits hit, completeness, and drift; and
- recovery scan/repair bytes/events/time and stale fenced completions.

Do not log prompts, diffs, file contents, remote URLs with credentials, command
environment, or raw paths by default. Timeout dumps show operation IDs, hashed
repo/item identity, class, owner state, and child-process cleanup state.

## Migration plan

### H06.0 — Baseline and deterministic controls

- Implement P04/P05 generators and instrumentation before optimizing.
- Add barriers/failpoints for append/snapshot/catalog publication, long Git
  operation completion, checkpoint delta, observation, and cancellation.
- Capture 0/100/10k/1m-event Forge and 1k/100k/1m-file Coder baselines.
- Add an executor-latency canary during slow Forge/Git/checkpoint operations.

### H06.1 — Bounded execution service

- Inventory every sync Forge/Git/filesystem call reachable from async handlers
  and turn orchestration.
- Route them through classified bounded admission; preserve Windows no-window.
- Supervise network Git asynchronously with timeouts/tree cleanup/output caps.
- Add operation fencing before releasing locks around long work.
- Delete direct blocking calls and isolated unbounded `spawn_blocking` wrappers.

### H06.2 — Forge item owners and log v2

- Add owner registry, typed commands, generations, receipts, and in-memory tail.
- Make append return and callers consume sequence/generation.
- Stream fold/replay; implement snapshot anchor/tail seek and one-time recovery.
- Batch compound transitions under one durability fence.
- Migrate v1 JSONL idempotently with rollback artifacts.

### H06.3 — Catalog and compaction

- Build the generation-stamped summary/slug catalog and paginated list API.
- Move registration uniqueness into catalog/item transaction fencing.
- Coalesce snapshots and compact sealed log segments by byte/event thresholds.
- Add stale/missing catalog repair and background-resource limits.
- Delete list-to-load-all and per-transition snapshot writes.

### H06.4 — Logical checkpoint journal

- Define logical delta/receipt schema and H03 transcript cursor contract.
- Replace whole-checkpoint clone/bound/pretty-write with incremental journal and
  bounded referenced segments.
- Preserve safe-boundary and uncertain-tool-call recovery tests.
- Version migration from checkpoint schema v1 and retain rollback support.

### H06.5 — Workspace observation service

- Add workspace generation/invalidation and exact/incomplete observation types.
- Route mutating tools and watcher overflow through conservative invalidation.
- Stream Git diff hashing, add bounded untracked hashing/cache, and coalesce.
- Require exact observation at resume; expose incomplete recovery diagnostics.
- Delete `refresh_runtime_metadata` from ordinary logical boundary paths.

### H06.6 — Close evidence and legacy paths

- Run crash/concurrency and P04/P05 across supported OS/filesystems.
- Demonstrate unrelated request/stream latency while blocking lanes saturate.
- Remove v1 writers, compatibility flags, duplicate snapshots/transcripts,
  replay-to-find-tail calls, and direct handler blocking.
- Update canonical Forge/Coder/API/operator docs with shipped semantics.

H06.1 can start beside H06.2. The logical checkpoint journal can start once H03
defines its cursor contract; observation work can start independently after H05
provides cancellation/task ownership. Do not switch load/mutation to v2 until
recovery and rollback fixtures pass.

## Rollout and rollback

Dual-read/single-write by store generation: while migrating an item, continue
serving v1 until the complete v2 log/snapshot/tail/catalog generation is synced
and atomically selected. Never dual-write independent authorities. Keep v1
read-only rollback data through the release fence and record per-item migration
status/error.

Checkpoint v1 remains readable during migration; the first successful v2 safe
boundary supersedes it atomically. Rollback can resume only from a boundary the
old build understands and must report newer/incomplete records rather than
discarding them.

If the bounded service regresses, reduce admission/disable affected operations
with typed unavailable responses; do not restore inline blocking. If observation
cannot complete, disable automatic resume for that checkpoint rather than
falling back to an unbounded audit on an async thread.

## Verification and exit criteria

### Correctness and crash evidence

- CR-007 at partial append, sync, snapshot, catalog, compaction, and migration
  publication yields one complete event prefix and valid folded state.
- CR-008 at every logical delta/segment/observation publication yields the last
  complete protocol-safe boundary; uncertain calls never replay.
- CM-009 proves leases/operations/generations reject stale same-item work.
- CM-010 proves unrelated items progress while one owner/compaction is blocked.
- CM-011/CM-012 prove monotonic checkpoint generations and correctly ordered
  observation around repository mutation.
- ISO-006 proves Forge authority/context cannot cross concurrent turns.

### Performance and saturation evidence

PERF-002 is validated when P04 shows:

- steady append and common mutation decode zero historical events;
- append cost is O(batch bytes) amortized and returned sequence is reused;
- snapshot load reads/verifies anchor plus tail, not historical prefix;
- list/slug allocation reads the catalog without per-item replay;
- compaction/migration has bounded memory/I/O and unrelated-item latency; and
- crash recovery is measured separately and produces a valid repaired tail.

PERF-004 is validated when P05 shows:

- model-only and unchanged-workspace boundaries run zero Git subprocesses and
  read/hash zero repository content bytes;
- logical checkpoint allocation/write scales with delta and uses one bounded
  serialization/encoding pass;
- mutating boundaries coalesce observation and stream hashing under limits;
- explicit resume validation reports exact or incomplete, never partial-exact;
  and checkpoint pause/turn p99 meet the recorded budget at all fixture sizes.

ASYNC-001 is validated when:

- audited async paths contain no direct blocking Forge/Git/fs/process waits;
- queue/process/byte limits hold under saturation with typed overload;
- cancellation terminates supervised child processes within platform budget or
  reports recovery-required state;
- trivial health and H03 stream p99 remain within their budgets during slow
  loads, syncs, compactions, hashes, and network Git; and
- no async/global lock is held across bounded-service waits.

All findings reach Shipped only after migrations, rollback evidence,
observability, supported-platform verification, and canonical docs ship.

## Canonical documentation at ship time

- Forge engine/API docs: generations, pagination, operation lifecycle,
  durability, overload, migration, and recovery;
- Coder docs: safe logical boundaries, workspace observation, automatic-resume
  denial/incomplete diagnostics, and retention;
- operator runbooks: queue/process saturation, compaction/catalog repair,
  migration failures, stuck Git child cleanup, and observation limits;
- configuration reference: bounded-service, compaction, checkpoint, hash, and
  timeout settings; and
- contributor docs: no direct blocking Forge/Git work from async code.

## Superseded code and concepts to delete

- `EventStore::last_seq` full replay and replay's intermediate `Vec<String>`;
- replay-to-rediscover sequence/lease generation paths;
- full-log replay before snapshot acceptance;
- `Forge::list`/registration loading every work item;
- per-transition full snapshot/sync policy;
- direct synchronous Forge/Git calls from async handlers/orchestration;
- unbounded/isolated `spawn_blocking` without shared admission;
- unsupervised `std::process::Command::output` for long/network Git;
- checkpoint `refresh_runtime_metadata` on every logical boundary;
- allocated full binary diff used only for hashing;
- unlimited untracked-file hashing;
- repeated checkpoint `serialized_size`, `remove(0)`, and whole-snapshot rewrite;
- duplicated full transcript/invocation state already authoritative in H03; and
- v1 migration writers/readers and rollback artifacts after the release fence.

## Code anchors

- `crates/medousa-forge/src/store.rs`
- `crates/medousa-forge/src/forge.rs`
- `crates/medousa-forge/src/git.rs`
- `crates/medousa-forge/src/adapter.rs`
- `src/daemon/forge_api.rs`
- `src/agent_runtime/coder_turn_checkpoint.rs`
- `src/agent_runtime/coder_tools.rs`
- Forge/worktree filesystem watcher and activity registry paths
