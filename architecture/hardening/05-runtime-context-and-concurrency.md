# H05 — Request-scoped runtime context and concurrency

> **Status:** Implementing — H05.1/H05.3 execution and worker ownership underway
>
> **Accountable owner:** daemon runtime maintainers
>
> **Reviewers:** tool runtime, worker/workshop, daemon/SSE, Home/Tauri browser, security, persistence
>
> **Audit findings:** CONC-001 (Critical), CONC-002 (High)
>
> **Release gate:** Gate B — trustworthy state
>
> **Required decision:** [ADR-017](../../docs/architecture/decisions/adr-017-request-scoped-runtime-context.md)
>
> **Dependencies:** H01 authenticated principal; H02 typed identifiers/authority
>
> **Coordinates with:** H03 turn pipeline; H08 browser authority
>
> **Verification:** [Crash/concurrency matrix](verification/crash-concurrency-matrix.md), [security abuse matrix](verification/security-abuse-matrix.md)

## Outcome

Every turn, tool invocation, worker, cancellation, continuation, and browser
request carries an explicit immutable context and an exact lifecycle handle.
Process services may be shared; request identity, authority, routing, sinks,
reply channels, and cleanup state may not. Unrelated sessions and browser
surfaces progress concurrently without foreign canaries crossing output,
storage, tools, delivery, or callbacks.

H05 owns runtime correlation, propagation, cancellation, task ownership, and
browser reply correlation. H03 owns sequencing/backpressure inside a turn's
output pipeline. H08 owns webview origin isolation and the minimum permitted
IPC bridge. H01/H02 own the authority placed into the context.

## Implementation progress

H05.0 started from merged H01/H02 on `codex/h05-runtime-context`. The first
deterministic two-turn fixture covers the ambient tool sink: two barrier-aligned
turn futures install distinct canaries, yield in parallel, and prove each sees
only its own sink. The process-global `ACTIVE_TOOL_SINK` mutable slot and its
set/clear API are removed. A Tokio task-local compatibility scope now carries
the sink only across the owning turn future and returns absence outside that
scope. It is not authorization and must be deleted when H05.2 gives the
upstream tool invocation boundary an explicit context.

H05.1 now owns daemon ticket executions with a daemon-issued `TurnHandle`, an
immutable context carrying the typed session, authenticated `RequestPrincipal`,
provider route, surface capabilities, deadline, cancellation root, and a frozen
legacy-scope projection. Admission reserves one of 256 live registry entries
before stream/ticket allocation and returns an explicit overload response at
the boundary. The spawned task retains an RAII lease; terminal drop removes
only the exact `Arc` generation. Registry tests cover capacity/high-water,
same-session concurrency, foreign cancel rejection, cancellation isolation,
task-local isolation, steady-state release, and stale-lease replacement safety.
Each context also owns a 64-per-turn child-task group. Child spawns inherit the
same task-local context and cancellation root, reserve before spawning, release
their permit on exit, and are aborted when the exact execution lease closes.

Authenticated principals from interactive and background HTTP entry points are
now carried through admission instead of being discarded after identity
rewriting. The existing session cancel endpoint also signals the matching
execution's cancellation root while the public API remains on its legacy
session/turn identifier contract.
The daemon turn future races its cancellation root and admitted deadline;
cancellation or expiry drops the in-flight provider/tool future and emits a
terminal error instead of waiting for cooperative polling at a later round.

Daemon turns no longer mirror their continuation scope into
`TuiRuntime::turn_scope`. A task-local compatibility read derives the old scope
from the immutable admitted context. The first authority-sensitive tool group
(history/session, client surfaces, browser, shell, skill, bridge, runtime, and
workflow tools) has moved to that read. TUI, recurring/ingest, and worker entry
surfaces still use the shared fallback until they receive their own admission
contexts; existing detached spawn sites still need conversion to the tracked
task API, plus cancellation/deadline propagation into every blocking leaf,
before H05.1 is complete.

H05.3 has started at the host/worker boundary. `TurnWorkerScheduler` no longer
stores a process-wide `runtime_ctx` or `bus_session`. Each local turn receives
an opaque `WorkerParentHandle`; the scheduler admits a bounded parent record
containing that turn's runtime services, sink, session, route, delivery,
handoff, continuity, and surface capabilities. Worker tools resolve only the
handle scoped to their invoking turn. An RAII lease compare-removes the exact
`Arc` generation on every return path, replacing ten unconditional clear calls
that could erase a newer sibling. The scheduler reports live/high-water counts,
and barrier tests prove concurrent parents cannot cross sessions while stale
lease cleanup cannot remove a replacement generation.

Worker execution no longer installs and restores canvas scope through the
shared runtime lock. Every durable worker record now snapshots the delegating
identity alongside its session/route/surface data, and worker execution scopes
that frozen projection task-locally for both canvas and non-canvas lanes. The
worker execution path no longer reads or writes the shared scope; retained tool
constructors see only the task-local compatibility projection. A two-worker barrier fixture
proves distinct session canaries remain isolated while the shared fallback
stays empty.

Bound-workshop admission is now one atomic store operation: check and insert
share the same lock, so two simultaneous begin requests for one session cannot
both win. Steering carries the exact `work_id` generation from Home through
Tauri and HTTP into a compare-before-mutate store operation. A stale steer
reports the replacement generation and cannot append to it. Concurrent
admission and stale-replacement tests freeze both races. Worker cancellation
also names the exact `work_id` and verifies that its durable record belongs to
the active host session before mutation; a cross-session regression test freezes
that authority boundary.

Durable child-job continuations now write an explicit format version and the
originating profile reference. Resume fails closed for legacy authority-less
records, revoked session visibility, and cross-session delivery targets. The
resume claim is single-winner (including the Surreal update path), and an
accepted replay is admitted into the bounded execution registry with a fresh
member-scoped principal, cancellation root, deadline, task owner, and exact
lease instead of running as a detached authority-less turn.

Provider completions, tool catalog reads, sequential tool calls, fallback
synthesis, and parallel tool calls now cross a shared execution leaf boundary
that races the operation against the exact turn cancellation root and absolute
deadline. Parallel `JoinSet` tasks reinstall their parent's immutable context
before invoking tools. Turn and inference-attempt stream pumps now abort on
owner drop (including with live sender clones), closing the detached-delta leak;
normal completion still drains in order.

Durable workers now register a bounded live cancellation token before entering
execution. Exact session-authorized cancellation changes the durable record and
signals that generation under the same records-then-live lock order. Workers
revalidate their durable profile/session authority, reconstruct a member-scoped
`TurnExecutionContext`, and run provider/tool leaves under the live token and an
absolute deadline. Duplicate execution admission is rejected, stale lease drop
cannot remove a replacement token, and worker delta pumps abort on owner drop;
durable status polling remains a restart/recovery backstop.

The current H05.0 request-state inventory is:

| State | Classification | Current action |
| --- | --- | --- |
| Ambient tool sink | Per-turn request state | Task-scoped compatibility bridge; global slot deleted; H05.2 deletion remains |
| Shared `TurnContinuationScope` | Per-turn request state retained by tool/runtime modules and installed by non-daemon save/restore paths | Daemon writes removed; authority-sensitive reads prefer immutable task context; TUI/worker/ingest removal remains |
| Worker scheduler `runtime_ctx` and `bus_session` | Per-parent/worker request state formerly held in two process-wide `Option` slots | Slots deleted; bounded keyed parent registry and exact leases landed; child execution and bound-workshop generation ownership remain |
| Browser `SNAPSHOT_TX`, `ACT_TX`, `NAV_STATE_TX`, `FIND_TX` | Per-request reply state in four process-wide singleton mailboxes | Open; freeze overlap, reverse completion, timeout, and navigation races before H05.5 |
| `LAST_GRAPHEME_SOURCE` | Per-invocation source selected through one global last-value slot | Open; key to invocation context in H05.2 |
| Continuation last-resume value | Process diagnostic snapshot, not execution authority | Retain only as bounded diagnostics; never select runtime behavior from it |
| Parent stream and ingest delivery bridge registrations | Process service references with keyed APIs | Retain as shared services; audit registration lifecycle separately |
| Browser placement/viewport and per-surface active-tab slots | UI/surface state, not reply mailboxes | Move under `BrowserHostState` surface records in H05.5 where identity matters |

This classification is intentionally narrower than a textual search for every
`Mutex<Option<_>>`: shutdown senders, cached routes, device tokens, and service
registrations are not request identity merely because they use `Option`.

## Current ownership failures

### Turn and tool context

`TuiRuntime` owns one `Arc<RwLock<Option<TurnContinuationScope>>>` and passes it
to dozens of long-lived tool objects at registration. `run_agent_turn` saves
the old scope, writes the incoming turn, runs, and restores it. The TUI and
canvas worker paths repeat the pattern. Tool invocation reads the lock later,
so it observes whichever turn wrote most recently, not necessarily its caller.

`engine_adapters::ACTIVE_TOOL_SINK` is described as per-turn but is one
process-global `RwLock<Option<_>>`. Two turns A and B can interleave as:

```text
A installs scope/sink A
B installs scope/sink B
A invokes a tool             -> tool/output uses B
A finishes and clears/restores
B invokes a tool             -> no sink or stale predecessor scope
```

The locks are behaving correctly; the ownership model is not.

### Worker context

`TurnWorkerScheduler` has one `runtime_ctx` and one `bus_session`. The
orchestrator overwrites both from the current turn, and many exit branches call
an unconditional `clear_bus_session`. A host turn, bound workshop, or sibling
worker can therefore inherit another turn's provider, model, scope, output
sink, delivery target, capabilities, handoff slot, or parent correlation. Late
cleanup can erase a newer session.

ADR-005 intends one bound workshop per session, but the implementation slot is
process-wide. That over-serializes unrelated work without actually providing
correct same-session generation fencing.

### Browser reply mailboxes

`human_browser.rs` declares global `SNAPSHOT_TX`, `ACT_TX`, `NAV_STATE_TX`, and
`FIND_TX` singleton senders. Each command replaces the sender before evaluating
JavaScript; callbacks contain no request ID and sometimes no surface. Overlap,
timeout, navigation, embed/pop-out use, and late callbacks can steal or close
another caller's response. Evaluation and timeout errors also do not reliably
remove the exact registered sender.

## Invariants

1. Admitted execution identity, authority, routing, capabilities, sink, and
   cancellation never change for that execution.
2. Every tool call receives its caller context; absence is a typed error, not a
   bootstrap/global fallback.
3. Shared services expose keyed APIs. No turn/request state is selected through
   a process-global `Option`, active/current accessor, or save/restore guard.
4. A daemon-issued opaque handle identifies each live execution generation.
5. Worker context derives from one named parent and can only reduce authority.
6. Cleanup, cancel, steer, completion, and timeout remove or mutate the exact
   handle/request; stale operations cannot affect a replacement.
7. Cancellation is hierarchical, idempotent, sequence-fenced, and bounded.
8. Every spawned task retaining context is tracked by its execution owner.
9. Browser results match request ID, concrete surface, response kind, and
   navigation generation before completion.
10. Every execution/pending-request registry has count, byte, and lifetime
    limits, with visible overload behavior.
11. Unrelated sessions and surfaces make progress when one sink, provider,
    tool, or webview blocks/fails.
12. Logs and metrics identify handles/correlation safely without recording
    prompts, credentials, page HTML, or filesystem paths by default.

## Non-goals

- serializing every daemon turn to hide races;
- changing ADR-005's host/scheduler product model;
- defining the H03 stream/journal event schema;
- making request IDs authorize browser IPC;
- granting a child worker more authority than its parent;
- preserving detached best-effort tasks after their execution owner exits;
- using one global mutex as the target architecture.

## Target ownership model

```text
Daemon/App process
  RuntimeServices
    context-free ToolRegistry
    provider/persistence/metrics services
    TurnExecutionRegistry[TurnHandle]
      TurnExecution
        immutable TurnExecutionContext
        H03 TurnPipeline + sink
        cancellation root + deadline
        tracked TaskGroup
        WorkerRegistry[WorkerHandle]

Home Tauri AppState
  BrowserHostState
    SurfaceRegistry[BrowserSurfaceId]
      surface instance + navigation generation
      PendingRequests[BrowserRequestId]
```

Registry locks cover lookup/admission only. They are not held across provider,
tool, pipeline, browser, or disk awaits. Execution-owned state can use a local
actor/lock because its key and lifecycle are already fixed.

## Runtime context contract

### Types

Introduce dependency-light semantic types near the runtime boundary:

```rust
struct TurnHandle(/* daemon-generated opaque value */);
struct WorkerHandle(/* child generation */);

struct TurnExecutionContext {
    handle: TurnHandle,
    turn_id: TurnId,
    correlation_id: CorrelationId,
    session_id: SessionId,
    principal: RequestPrincipal,
    authority: TurnAuthority,
    route: ProviderRoute,
    surface: SurfaceCapabilities,
    delivery: Option<DeliveryTarget>,
    tool_policy: ToolPolicy,
    sink: Arc<dyn ToolSinkPort>,
    cancellation: CancellationToken,
    deadline: Instant,
    parent: Option<WorkerHandle>,
}
```

Do not turn this into a bag of optional strings. H01/H02 types distinguish
identity, storage keys, correlations, display values, and capabilities. Values
that are optional must have explicit absence semantics. Provider credentials
stay behind a route/service capability and are not cloned into logs or durable
continuations.

`TurnContinuationScope` is split:

- durable continuation data: versioned IDs, original intent reference,
  delivery intent, response-depth/provider preference, and required authority
  references;
- admitted execution context: authenticated principal, resolved authority,
  concrete route/capabilities, sink, cancellation, deadline, and handles.

Resuming reconstructs and reauthorizes the second from the first. It never
deserializes ambient authority.

### Tool invocation

Change the typed tool boundary so invocation receives context:

```rust
async fn invoke(
    &self,
    ctx: &ToolInvocationContext,
    input: Input,
) -> Result<Output, ToolError>;
```

The registry stores stateless definitions or factories for service-bound tools.
The invocation context exposes the immutable turn context plus call ID,
per-call cancellation/deadline, and deliberately narrowed capabilities. Tools
that need a session, sink, browser host, UI surface, filesystem root, worker
parent, or delivery route take it from this argument.

Migration adapter rules:

- use Tokio task-local scope only where an upstream trait cannot accept context;
- install at the top-level invocation, not by mutating shared runtime state;
- explicitly re-scope spawned futures;
- error when absent; no global/bootstrap fallback for turn-sensitive work;
- forbid its use as an authorization proof; and
- delete it after the trait migration.

### Execution registry

`TurnExecutionRegistry` admits a fully constructed execution and returns an
RAII/lifecycle guard. Insert is single-generation, capacity-reserved, and
observable. Lookup requires typed handle plus authorization/session match where
called from an API. Removal is compare-and-remove on the exact handle; dropping
an old guard cannot remove a newer generation.

The registry stores only live/reconnect-needed owners. H03 owns durable replay.
Terminal entries leave after the documented reconnect/grace window and may not
be pinned forever by a disconnected client.

## Worker and workshop ownership

Replace `set_runtime_context`, `set_bus_session`, `active_bus_session_id`, and
unconditional `clear_bus_session` with explicit parameters and keyed owners:

```text
spawn_worker(parent_handle, intent, task)
  -> validate live parent and per-parent/global permits
  -> derive attenuated child context + child cancellation
  -> insert WorkerExecution(parent, child)
  -> run in parent's tracked task group
  -> compare/remove child on exact completion
```

The host sink, handoff slot, continuity bundle, provider route, tool policy,
surface capabilities, and delivery target live in parent/child execution state.
The worker scheduler may hold service dependencies, limits, and a keyed worker
registry, but no current host values.

For bound workshops, the session ticket owner atomically enforces one active
bound-workshop generation for that session. A second begin request receives the
documented busy/conflict behavior. Unrelated sessions do not share that permit.
Steer names session plus turn/work generation; stale steering is rejected.

## Cancellation and task lifecycle

Admission creates a cancellation tree:

```text
turn root
  provider/attempt pumps
  H03 pipeline producers/subscribers
  tool call N
    blocking job / browser request
  worker child N
    child tools/provider/pipeline
```

Every spawn goes through `TurnTaskGroup::spawn` (or equivalent `JoinSet` owner)
and inherits context plus a child token. The owner stops admission on cancel,
records the H03 accepted sequence fence, signals children, awaits a configured
grace period, aborts only remaining owned tasks, and reports leaks/incomplete
cleanup. Shutdown cancels registry roots and emits an inventory by handle.

Cancellation API results distinguish `accepted`, `already_terminal`,
`stale_generation`, `not_found`, and `unauthorized`. Provider/tool cleanup
errors are retained as diagnostics but cannot replace the one terminal outcome.

## Browser request broker

### Identity and registration

Use daemon/app-generated random request IDs and stable surface instance IDs:

```rust
struct BrowserSurfaceId {
    window_label: WindowLabel,
    webview_label: WebviewLabel,
    instance_generation: u64,
}

struct PendingBrowserRequest {
    kind: BrowserResponseKind,
    navigation_generation: u64,
    expected_origin: Origin,
    deadline: Instant,
    max_response_bytes: usize,
    sender: oneshot::Sender<Result<BrowserResponse, BrowserRequestError>>,
}
```

Registration reserves per-surface and global permits before map insertion. It
returns a guard whose drop compare-removes only `(surface_id, request_id)`.
The injected payload includes request ID, surface instance generation, and
navigation generation. Separate callback DTO variants make a snapshot
impossible to deserialize as an action/find/navigation result.

The command handler validates the invoking webview identity where the Tauri API
exposes it. Until H08 provides that origin/caller boundary, correlation is still
required but remote browser pages must not receive ambient invoke authority.

### Completion and invalidation

Completion atomically takes one matching entry and checks response kind,
surface, instance generation, navigation generation, origin, and encoded byte
limit. Mismatch leaves honest requests intact unless the surface itself became
invalid. Duplicate and unsolicited responses never allocate an entry.

Timeout, evaluation failure, caller cancellation, webview close/recreate,
navigation, and app shutdown fail/remove exact matching entries. Navigation
increments the surface generation before cancelling prior-generation work.
Late callbacks find no matching entry and are counted, not forwarded.

If `window.find` or another primitive is only safe one-at-a-time per surface,
use a per-surface semaphore with bounded wait and cancellation. Embed and
pop-out have distinct surfaces and permits.

## Resource limits

Initial ceilings are safety defaults to validate under H12; they are not
throughput claims:

| Resource | Initial policy |
| --- | --- |
| Live turn executions | configurable global count; reject before allocating heavy runtime state |
| Workers | existing product cap enforced per parent plus a global count/retained-byte cap |
| Tracked tasks | per-turn count cap; spawns reserve a permit |
| Pending browser requests | 8 per surface and 64 per app process |
| Snapshot response | 2 MiB encoded maximum before conversion; smaller caller `max_chars` still applies |
| Action/find/nav response | 64 KiB encoded maximum |
| Browser wait | existing 2 s query / 8 s snapshot-action deadlines, made cancellation-safe |
| Terminal cleanup | 2 s normal turn grace; bounded shutdown inventory after global deadline |

Measure actual retained sizes and tune them with evidence. Counts alone do not
bound response buffers, task captures, sinks, or continuation payloads.
Overload returns a typed busy/overloaded result and metrics; it does not replace
an older entry or fall back to a singleton.

## Observability and diagnostics

Record, without prompt/page/credential payloads:

- active turns/workers/tasks and high-water marks;
- context-missing invocation count (must reach zero before legacy removal);
- turn/worker handle, session-safe correlation, parent handle, lifecycle state,
  cancellation reason/fence, cleanup duration, and orphan count;
- browser pending count/bytes by surface and kind;
- response match, timeout, cancel, late, duplicate, unsolicited, wrong-kind,
  wrong-surface, stale-navigation, and oversize counters;
- per-turn sink/provider/tool latency and failures to prove isolation; and
- admission rejections by resource limit.

Debug dumps hash or otherwise redact user-derived identifiers under the
existing telemetry policy. A timeout dump shows owned task names and states,
not captured inputs.

## Migration plan

### H05.0 — Freeze the race with deterministic tests

- Add barrier-controlled two-turn tests with distinct canaries for every ISO
  dimension before changing ownership.
- Add overlap/reversed-completion tests for all four browser mailbox kinds,
  embed/pop-out, timeout/late callback, navigation, and close.
- Inventory every `turn_scope`, `active_*`, current session/profile, worker bus,
  sink, provider/model, and global reply slot. Classify true service state
  separately from request state.
- Do not merge a global serialization mutex as the final fix. If emergency
  containment is unavoidable, mark it temporary with deletion criteria and
  retain the concurrency tests.

### H05.1 — Introduce execution context and registry

- Add typed handles, immutable `TurnExecutionContext`, `TurnExecution`, bounded
  registry, exact lifecycle guard, cancellation tree, and task group.
- Construct context at HTTP/TUI/recurring/worker/external-runtime admission.
- Integrate H01 principal and H02 authority types; reject missing authority.
- Keep old scope populated only behind a measured compatibility adapter.

### H05.2 — Migrate tool invocation and sinks

- Extend the typed tool invocation contract and adapt registries.
- Migrate tools in groups: history/memory; vault/artifact/environment;
  UI/browser; workflow/delivery; worker/control; remaining adapters.
- Move the tool sink into invocation context and route all tool events through
  the owning H03 pipeline.
- Delete `ACTIVE_TOOL_SINK`, `TuiRuntime::turn_scope`, runtime-session bootstrap
  fallback for turn-sensitive calls, and all scope save/restore paths.

### H05.3 — Key worker ownership

- Make scheduler service dependencies immutable.
- Replace current runtime/bus fields with parent/child execution records.
- Require parent handle on spawn, attenuate child capabilities, and track tasks.
- Move handoff/continuity/output ownership under the parent generation.
- Replace unconditional clears with exact guard removal and add retention caps.

### H05.4 — Finish cancellation and continuation migration

- Route cancel/steer through exact typed handles and session owner admission.
- Propagate tokens/deadlines through providers, tools, workers, browser calls,
  pipeline sends, and bounded blocking services.
- Version durable continuation data and reauthorize on resume.
- Delete detached task paths and ambiguous active-turn cancellation.

### H05.5 — Replace browser mailboxes

- Add `BrowserHostState`, surface registry, navigation generation, request
  broker, typed response DTOs, limits, and cleanup guards.
- Include correlation in injected scripts and every callback.
- Wire eval failure, timeout, cancellation, navigation, close, recreate, and
  shutdown cleanup.
- Delete `SNAPSHOT_TX`, `ACT_TX`, `NAV_STATE_TX`, and `FIND_TX`.
- Hand origin/capability enforcement to H08 without weakening correlation.

### H05.6 — Remove compatibility and ship evidence

- Fail CI on new request-state singleton/current accessors in audited modules.
- Run the complete isolation/browser/cancellation matrix repeatedly and under
  supported-platform packaged Home builds.
- Remove feature flags, adapters, legacy metrics, and temporary serialization.
- Update canonical runtime/browser/API docs only as behavior ships.

Runtime and browser broker slices may proceed in parallel after H05.0. The
legacy turn scope cannot be removed until all tool construction/invocation
paths migrate. H03 should accept an explicit context/sink interface early so it
does not encode the ambient model.

## Rollout and rollback

Use short-lived internal compatibility adapters, not two independently mutable
sources of truth. During migration, derive legacy reads from the admitted
context within a scoped invocation and count every use. Never mirror context
into the old process-global lock.

Roll out by entry surface and tool group with canary isolation tests. A rollback
may re-enable the previous entry adapter only while the runtime remains
explicitly serialized for safety; it must not silently restore concurrent use
of global context. Browser rollback disables concurrent commands or the
affected bridge capability rather than restoring uncorrelated callbacks.

## Verification plan

### Deterministic unit/model tests

- execution guards remove only their exact handle/generation;
- context remains immutable while preferences/runtime configuration change;
- task spawning inherits context/token and task exit releases permits;
- parent cancellation reaches children; child cancellation does not escape;
- stale cancel/steer/worker completion cannot affect a replacement;
- missing context fails closed and cannot use bootstrap identity;
- browser broker matches exact kind/surface/generation/request;
- timeout/cancel/drop/close/nav cleanup is single-shot and leak-free; and
- per-surface/global count and byte admission rejects predictably.

### Integration and stress

- ISO-001–ISO-010 with barrier-controlled foreign canaries across outputs,
  journal/history, tools, files, delivery, provider routes, and handoffs;
- BR-001–BR-007 with reversed and adversarial callback ordering;
- BP-002–BP-007 with blocked sinks/providers/tools and cancel at each boundary;
- RET-006 far beyond caps followed by steady-state memory/registry assertions;
- repeated same-session bound-workshop admission plus exact steer/cancel; and
- daemon/Home shutdown with active turns, workers, and browser requests.

Run focused tests in parallel. A suite pass obtained only with one test thread
is a failure of the isolation goal. Use explicit barriers/failpoints instead of
timing sleeps, retain scheduler seeds/event traces on failure, and apply per-case
timeouts that dump owner state.

## Exit criteria

CONC-001 is validated only when:

- no process-global or shared-runtime turn scope/sink/current worker bus remains;
- all turn-sensitive tools consume explicit invocation context and absence
  fails closed;
- every context-retaining spawn is owned, cancellable, and deadline-bounded;
- ISO-001–ISO-010 and applicable BP cases pass repeatedly in parallel with no
  foreign canary in output, persistence, authority, provider, or delivery;
- same-session admission/cancel/steer and worker cleanup are generation-exact;
- execution/worker/task registries enforce count/byte/lifetime policy and return
  to steady state; and
- compatibility fallback counters are zero and the adapter is deleted.

CONC-002 is validated only when:

- all browser commands and callbacks carry request/surface/navigation identity;
- all four singleton reply senders are deleted;
- BR-001–BR-007 and RET-006 pass on embed and pop-out packaged webviews;
- timeout, cancellation, navigation, close, eval failure, and shutdown remove
  only exact pending entries; and
- pending counts/bytes return to zero with late/unsolicited responses harmless.

Both findings reach Shipped only after rollout, observability, rollback removal,
and canonical documentation land with the release.

## Canonical documentation at ship time

- `docs/engine/` turn, worker, cancel/steer, and continuation behavior;
- SDK/API references for typed stale/conflict/overload/cancellation outcomes;
- Home application/browser reference for concurrency and timeout behavior;
- operator diagnostics/runbooks for active execution, orphan, and pending
  browser request metrics; and
- contributor architecture guidance for tool context and keyed mutable state.

Do not document `TurnHandle` as a client authorization token or request ID as a
browser security boundary.

## Superseded code and concepts to delete

- `engine_adapters::ACTIVE_TOOL_SINK`, `set_active_tool_sink`, and
  `active_tool_sink`;
- shared `TuiRuntime::turn_scope` and all tool fields/constructors that retain it;
- `previous_scope`/install/restore logic in daemon, TUI, and worker paths;
- runtime-session bootstrap fallback for admitted turn-sensitive tools;
- `TurnWorkerScheduler::{runtime_ctx,bus_session}` plus set/clear/current APIs;
- ambiguous active-generation cancel/steer/worker cleanup paths;
- detached context-retaining task spawns;
- `SNAPSHOT_TX`, `ACT_TX`, `NAV_STATE_TX`, and `FIND_TX`; and
- uncorrelated browser callback DTOs/scripts and any temporary serialization flag.

## Code anchors

- `src/engine_adapters.rs`
- `src/agent_runtime/daemon_interactive_turn.rs`
- `src/bin/medousa_tui/agent_runtime.rs`
- `src/tools.rs`
- `src/tui/runtime_services.rs`
- `src/runtime_session.rs`
- `src/agent_runtime/turn_orchestrator.rs`
- `src/agent_runtime/turn_worker/run.rs`
- tool modules retaining `Arc<RwLock<Option<TurnContinuationScope>>>`
- `apps/medousa-home/src-tauri/src/human_browser.rs`
