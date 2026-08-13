# Crash and concurrency verification matrix

> **Status:** Draft baseline contract
> **Program:** [Medousa hardening](../README.md)
> **Primary findings:** DUR-001, STORE-001, STORE-002, CONSIST-001,
> CONC-001, CONC-002, MEM-001, DATA-001
> **Required decisions:** ADR-015, ADR-016, ADR-017 (planned)

This matrix defines what “durable,” “single writer,” “atomic,” “isolated,” and
“deleted” must mean under failure and concurrency. A happy-path unit test is not
evidence for any of those words.

## Invariants

### Acknowledgement and durability

1. A success acknowledgement identifies a committed generation/sequence whose
   promised durability level has actually completed.
2. A failed write, flush, sync, rename, directory sync, or serialization is a
   visible failure. It is never counted as a successful commit.
3. Recovery yields either the last complete committed state or a later complete
   state. Torn JSON, partially replaced snapshots, and silently missing
   acknowledged events are forbidden.
4. Durability levels are named. `accepted`, `written`, and `synced` must not be
   conflated.

### Concurrency and ownership

1. Each mutable store has one serialized commit owner or a transactional
   primitive with equivalent semantics.
2. Concurrent mutations do not persist stale snapshots out of order.
3. Compare-and-write preconditions are checked in the same transaction/critical
   section as the commit.
4. Turn, worker, request, browser surface, workshop, and profile context cannot
   bleed between concurrent operations.
5. Cancellation has a documented sequence fence: callers can determine which
   work committed before cancellation and what was rejected afterward.

### Retention and deletion

1. Every in-memory and on-disk collection has a documented cap or lifecycle.
2. Eviction cannot remove state required for acknowledged replay/recovery.
3. Deletion has an enumerated inventory and is verified from a fresh process.
4. Cleanup failure is reported; an API cannot return `deleted: true` while
   known session-owned data remains.

## Deterministic test controls

The implementation plans must introduce test-only fault points at meaningful
storage and orchestration boundaries. Tests must not depend on racing
millisecond sleeps.

| Fault point | Meaning |
| --- | --- |
| `before_serialize` / `after_serialize` | State selected versus bytes created |
| `before_enqueue` / `after_enqueue` | Producer admission versus writer ownership |
| `before_write` / `after_partial_write` / `after_write` | File content transition |
| `before_flush` / `after_flush` | Userspace buffering boundary |
| `before_sync_data` / `after_sync_data` | File durability boundary |
| `before_rename` / `after_rename` | Snapshot publication boundary |
| `before_sync_parent` / `after_sync_parent` | Directory-entry durability boundary |
| `before_ack` / `after_ack` | Externally observed success boundary |
| `before_index_update` / `after_index_update` | File versus derived-index consistency |
| `before_terminal` / `after_terminal` | Stream terminal ordering boundary |
| `before_cleanup_member` / `after_cleanup_member` | Deletion inventory progress |

Fault actions include returned errors, short writes, forced task failure, closed
channels, blocked consumers, cancellation, and immediate process termination.
Production code does not branch on arbitrary test names; inject a filesystem,
clock, commit sink, or failpoint interface at the owning boundary.

## Process crash matrix

Each scenario starts from a known committed generation, performs one mutation,
terminates at every applicable fault point, restarts a fresh process, and checks
the authoritative store plus all derived indexes.

| ID | Surface | Mutation | Required recovery result |
| --- | --- | --- | --- |
| CR-001 | Turn journal | Content/reasoning batch append | Replay contains exactly the committed sequence prefix; no malformed event |
| CR-002 | Turn journal | Terminal commit | Terminal is absent and turn recoverable, or present exactly once with all prior events |
| CR-003 | Session history | Append user/assistant turn | Acknowledged turn survives; failed turn is not reported durable |
| CR-004 | Feed store | Append and retention eviction | Old complete channel or new complete channel; never truncated/stale overwrite |
| CR-005 | Feed cursor | Mark read concurrent with append | Cursor and event generation form a valid committed pair |
| CR-006 | Workspace/worker snapshot | Record update | Last acknowledged generation survives; pretty JSON is never torn |
| CR-007 | Forge event store | State-changing event append | Replay produces one valid state transition; no duplicate side effect |
| CR-008 | Coder checkpoint | Protocol-safe boundary | Last complete checkpoint loads; uncertain tool call is never replayed automatically |
| CR-009 | Vault note | Create/update | Old or new complete note plus matching index generation |
| CR-010 | Vault trash/restore | Rename transition | Note exists in exactly one intended location and index repairs deterministically |
| CR-011 | Derived vault/link index | Rebuild/publication | Corrupt/stale cache is rejected and rebuilt from authoritative notes |
| CR-012 | Project-task registry | Completion/eviction | Retention metadata and recoverable result remain consistent |
| CR-013 | Session deletion | Each inventory member | Retry resumes safely; API reports incomplete deletion until inventory is gone |

Where the chosen design uses a database/WAL instead of files, preserve the same
semantic fault boundaries using transaction hooks and kill/restart tests.

## Concurrent mutation matrix

All cases use barriers to establish ordering and repeat enough times under a
deterministic scheduler or high-contention runner to expose unplanned shared
state.

| ID | Concurrent operations | Required result |
| --- | --- | --- |
| CM-001 | N feed appends to one feed | Every accepted event appears once in commit order; cap/eviction deterministic |
| CM-002 | Appends to unrelated feeds/profiles | No global lock held across file I/O; unrelated progress remains bounded |
| CM-003 | Append versus mark-read | Cursor never points beyond committed event generation |
| CM-004 | N worker updates plus persistence debounce | Persisted state equals a real ordered generation, never an older late writer |
| CM-005 | Update versus retention eviction | Active/recoverable record cannot be evicted; memory/disk agree |
| CM-006 | Two vault writes with identical `If-Match` | Exactly one succeeds; loser receives conflict without mutation |
| CM-007 | Vault write versus external editor change | Conflict or documented reconciliation; never silent clobber |
| CM-008 | Vault delete/restore/write same path | One serializable outcome; no duplicate file or orphaned index entry |
| CM-009 | Forge mutations on same item/attempt | Lease and generation fencing admit only valid transition |
| CM-010 | Forge mutations on unrelated items | No unnecessary global serialization or cross-item state |
| CM-011 | Two Coder checkpoints same turn | Monotonic boundary generation; stale observation cannot replace newer state |
| CM-012 | Checkpoint while repository mutation completes | Observation is explicitly before or after mutation, never falsely labeled current |

## Turn and request isolation matrix

Use distinguishable canary values for every dimension: session, turn, worker,
workshop, profile, tool policy, Forge work/attempt, browser surface, model, and
delivery target. Assertions scan all output/persistence for foreign canaries.

| ID | Scenario | Required result |
| --- | --- | --- |
| ISO-001 | Two interactive turns stream simultaneously | Each sink, journal, SSE stream, history row, and terminal belongs to its turn |
| ISO-002 | Host turn plus several workers | Worker bus/scope/tool policy cannot overwrite host or sibling context |
| ISO-003 | Same session receives overlapping request/cancel/steer | Documented admission rule; no cancellation or steering of the wrong generation |
| ISO-004 | Different workshops and profiles operate concurrently | No active-workshop/profile global leaks into path, identity, vault, or delivery |
| ISO-005 | One turn changes model/provider environment while another runs | Provider credentials/routing remain request-scoped |
| ISO-006 | Forge-bound and unbound turns overlap | Filesystem/tool authority remains attached to the correct turn |
| ISO-007 | One sink blocks/fails while another streams | Failure/backpressure isolated; healthy turn remains within latency budget |
| ISO-008 | Retry/fallback attempt emits before failing | Attempt boundary preserves documented visible output without interleaving stale deltas |
| ISO-009 | Reconnect/replay while live tail advances | Client observes each sequence once in monotonic order |
| ISO-010 | Terminal publication races final queued delta | No content arrives after terminal; drain has a bounded failure policy |

## Browser request correlation matrix

These cases complement the authority checks in
[security-abuse-matrix.md](security-abuse-matrix.md).

| ID | Scenario | Required result |
| --- | --- | --- |
| BR-001 | Two snapshot requests | Each response matches request ID, surface, URL, and navigation generation |
| BR-002 | Snapshot and action overlap | No response consumes the other's waiter |
| BR-003 | Embed and pop-out find/nav requests overlap | Surface-scoped correlation and independent timeout/cancellation |
| BR-004 | First caller times out; late response arrives during second call | Late response rejected; second caller remains intact |
| BR-005 | Navigation occurs during snapshot/action | Result rejected or explicitly marked for the prior generation |
| BR-006 | Webview closes during request | All its pending calls fail promptly and only once |
| BR-007 | Malicious unsolicited response | Ignored; cannot complete an honest request |

## Cancellation and backpressure matrix

| ID | Scenario | Required result |
| --- | --- | --- |
| BP-001 | Provider emits faster than journal/UI sink | Queue bytes remain below hard limit; producer is backpressured or turn fails visibly |
| BP-002 | Sink blocks indefinitely | Watchdog/cancellation terminates within budget without unbounded retention |
| BP-003 | Bounded queue reaches capacity | Documented behavior—await, coalesce, or fail; never silent semantic loss |
| BP-004 | Cancel while producer waits for capacity | Producer and consumer release promptly; no orphan task/sender |
| BP-005 | Cancel during journal batch write | Commit fence and replay outcome are deterministic |
| BP-006 | Client disconnects while replay/live stream runs | Server retention follows policy; disconnected client cannot pin memory forever |
| BP-007 | Terminal arrives while cancellation is accepted | Exactly one terminal outcome with a stable reason |
| BP-008 | Persistence actor/channel is closed or full | Caller receives error/degraded state; synchronous blocking fallback is forbidden |

## Retention and lifecycle matrix

Each store publishes limits in records and bytes, active-state exemptions,
eviction order, persistence behavior, and operator diagnostics.

| ID | Surface | Required test |
| --- | --- | --- |
| RET-001 | Live turn replay buffer | Exceed event/byte cap; reconnect semantics follow journal/ring policy |
| RET-002 | Completed project-task runs | Create well beyond cap; oldest eligible runs evict from memory and disk as specified |
| RET-003 | Worker/job records | Active and recoverable records preserved; completed records compacted/evicted |
| RET-004 | Feed events | Cap enforced without O(n) front shifting or lost cursor semantics |
| RET-005 | Checkpoint transcript/history | Byte cap enforced incrementally; recovery retains required safe boundary |
| RET-006 | Browser pending requests | Per-surface/global cap; timeouts remove entries and late responses are harmless |
| RET-007 | Error/log/evidence buffers | Bounded and redacted under repeated failures |

After each case, resident memory must return to its documented steady-state
envelope; “Rust eventually could free it” is not sufficient if owners remain in
registries.

## Deletion inventory

H02 defines the authoritative inventory for a session. At minimum the test
fixture creates:

- history/transcript and session catalog/metadata;
- active-turn ticket, turn journal, checkpoint, ledger, and replay state;
- artifacts, media, extraction, verification, and UI/list metadata;
- workspace/work-card or delivery references owned by the session;
- profile/identity memory references according to retention policy; and
- caches/indexes whose entries reveal the deleted session.

| ID | Case | Required result after fresh-process inspection |
| --- | --- | --- |
| DEL-001 | Complete normal deletion | Every inventory member gone or retained only under an explicit legal/audit policy reported to caller |
| DEL-002 | Failure on each member | API reports incomplete state; retry is idempotent and completes remaining work |
| DEL-003 | Concurrent active turn | Deletion fences/cancels work before removing authority; turn cannot recreate data afterward |
| DEL-004 | Reuse deleted session ID | No old ledger/artifact/memory state attaches to the new session |
| DEL-005 | Hostile identifier | Rejected before cleanup; outside canaries unchanged |
| DEL-006 | Process crash during deletion | Restart discovers and resumes/rolls back according to deletion transaction policy |

## Test execution tiers

| Tier | Frequency | Contents |
| --- | --- | --- |
| PR fast | Every pull request | Deterministic unit/model checks, focused concurrent cases, injected I/O failures |
| PR integration | Required for affected subsystems | Real temp filesystem/database, assembled runtime, process restart, bounded stress |
| Nightly | Supported OS matrix | Repeated high-contention, kill-point sweep, sanitizer/loom-like schedules where applicable |
| Release | Packaged binaries and migration fixtures | Upgrade/crash/recovery/deletion evidence retained with artifact release |

Tests must use explicit per-case timeouts and dump task/store state on timeout.
Running everything serially is allowed only for cases that intentionally model
a singleton external resource; it cannot hide process-global test pollution.

## Evidence record

Each run records:

```text
schema_version, git_revision, dirty_state
os, filesystem, architecture, build profile
test seed and scheduler/fault-point sequence
store schema/version and migration fixture
operation/ack/commit generations
pre-crash and post-restart state hashes
queue/registry/retention high-water marks
case duration, timeout and terminal reason
artifact/log hashes
```

Failed runs retain the minimal disposable store needed to reproduce them, with
synthetic secrets and paths redacted.

## Exit criteria

Gate B and the concurrency portions of Gate C are validated only when:

- CR-001–013 pass at every applicable fault point on supported filesystems;
- CM-001–012 and ISO-001–010 pass repeatedly without foreign canary leakage;
- BR-001–007 prove request-correlated browser behavior;
- BP-001–008 demonstrate bounded memory and cancellation under stalled sinks;
- every RET case enforces a documented record/byte lifecycle;
- DEL-001–006 pass from a fresh process;
- all acknowledged generations survive according to their named durability
  level and no injected failure is counted as success; and
- required CI retains the fault schedule and recovery evidence.

If a plan deliberately changes an invariant, it must update the governing ADR
and this matrix before implementation is called validated.
