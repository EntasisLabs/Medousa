# ADR-017: Request-scoped runtime context and exact ownership

> **Status:** Proposed
>
> **Date:** 2026-08-13
>
> **Decision owners:** daemon runtime and desktop browser maintainers
>
> **Related:** [ADR-005](adr-005-host-scheduler-bound-workshop.md), [ADR-008](adr-008-hot-swappable-agentic-runtime.md), [ADR-015](adr-015-bounded-durable-turn-pipeline.md), [H05 execution plan](../../../architecture/hardening/05-runtime-context-and-concurrency.md)

## Context

Medousa constructs one long-lived `TuiRuntime` and registers long-lived tool
instances against it. Those tools retain an
`Arc<RwLock<Option<TurnContinuationScope>>>`. Each interactive turn saves the
old value, installs its own value, runs, and restores the old value. The stream
adapter repeats the mistake with a process-global `ACTIVE_TOOL_SINK`. The
worker scheduler similarly stores one mutable runtime context and one active
bus session for the process.

Those values are not configuration. They identify a principal's session,
turn, delivery target, provider/model route, client capabilities, worker
parent, tool authority, and output sink. Overlapping operations can therefore
read another turn's value. Worse, turn A's cleanup can clear or restore over
turn B after B has installed its state. A lock makes each overwrite atomic; it
does not make the save/overwrite/restore sequence correct.

The Home human-browser bridge has the same ownership error in smaller form.
Snapshot, action, navigation-state, and find requests each install a oneshot
sender into a process-global singleton. A second call overwrites the first, and
an uncorrelated or late callback consumes whichever sender happens to be
present. Embed and pop-out browser surfaces also share these mailboxes.

These failures cannot be repaired by adding more mutexes or by serializing the
entire daemon. Runtime concurrency is intentional: different sessions, host
and worker turns, external runtimes, replay clients, and browser surfaces must
progress independently.

## Decision

### 1. Runtime services and execution context are different types

Long-lived runtime services may be shared when they are stateless or provide a
keyed, bounded, concurrency-safe service. Examples include provider client
pools, tool definitions, persistence services, metrics, clocks, and registries.

Every execution receives an immutable `Arc<TurnExecutionContext>` created at
admission. It contains typed identity and routing facts, not mutable process
defaults:

```text
TurnExecutionContext {
  handle, turn_id, correlation_id, session_id,
  principal, workshop/profile authority,
  provider_route, surface_capabilities, delivery_target,
  tool_policy, sink, cancellation, deadline,
  parent_worker_handle?
}
```

The daemon generates `TurnHandle`; clients cannot choose it or use a session ID
as a substitute. H01 supplies the authenticated principal and H02 supplies
validated identifier/root-capability types. Context creation snapshots the
authorized values for the lifetime of the execution. A mutable preference or
environment fallback cannot silently reroute an admitted turn.

Mutable state belongs to a `TurnExecution` addressed by that handle: pipeline,
task group, worker children, cancellation state, and bounded scratch. It is not
stored back into the shared runtime. Registries are keyed collections used for
explicit lookup, cancellation, reconnect, and cleanup; they never expose a
singular “current” or “active” execution.

### 2. Context is passed at the invocation boundary

Tool definitions and registries are context-free. Every invocation receives a
`ToolInvocationContext` that borrows or clones the admitted turn context. A
tool must not retain an `Arc<RwLock<Option<TurnContinuationScope>>>`, consult a
bootstrap session fallback, or call a global `active_*` accessor to discover
the caller.

An integration trait that cannot yet accept an explicit argument may use a
Tokio task-local only as a migration adapter. The scope is installed once at
the execution boundary, has no process-global fallback, fails closed when
absent, and is explicitly propagated into every spawned task. Task-local
ambient state is not the final tool API and cannot be used for authorization.

The runtime must delete `ACTIVE_TOOL_SINK`, shared `TuiRuntime::turn_scope`, and
all save/install/restore sequences after callers migrate. Cleanup drops the
exact execution handle; it never writes `None` into a shared active slot.

### 3. Worker ancestry is explicit and capability-reducing

The worker scheduler is a process service, not the owner of one active host.
Spawning requires a live parent `TurnHandle` or an explicit durable continuation
whose authority has been reconstructed and validated. The scheduler derives a
child context from the parent and may only attenuate its tool, filesystem,
delivery, provider, surface, and deadline capabilities.

Worker bus sessions are stored by parent/child handle with count, byte, and
lifetime bounds. Output and handoff slots are part of the keyed parent
execution. Sibling workers cannot replace one another's sink or host metadata.
Removal uses the exact handle/generation, so late completion from an old worker
cannot clear a replacement.

ADR-005's “one bound workshop per session” remains an admission rule for that
mode. It is enforced by the session ticket/owner, not by a process-global bus
slot. Independent sessions and explicitly parallel workers remain concurrent.

### 4. Cancellation is hierarchical and exact

Every execution owns a cancellation root, deadline, and tracked task group.
Provider pumps, tool calls, pipeline admission, workers, browser waits, and
blocking-job permits receive child tokens. Cancelling a turn targets its
`TurnHandle` and accepted generation; cancelling a parent cancels descendants,
while a child cannot cancel its parent or sibling.

Cancellation is idempotent and records one accepted sequence fence. No new
semantic work is admitted beyond that fence. Cleanup waits within a finite
deadline, aborts only tasks owned by the execution, and reports incomplete
children/resources. Detached tasks that retain turn authority are forbidden.

Same-session cancel and steer operations must name the target turn/generation
or be resolved by the session owner under one atomic admission rule. A stale
request receives a typed conflict/gone result; it never targets whatever is
currently active.

### 5. Browser callbacks are request- and surface-correlated

Home owns one managed `BrowserRequestBroker` in Tauri application state. It
contains a bounded map keyed by `(BrowserSurfaceId, BrowserRequestId)`, where a
surface includes the concrete webview instance and navigation generation. A
pending entry records expected response kind, deadline, navigation/origin
generation, payload limit, and its response sender.

Every injected request carries the generated request ID and surface/navigation
generation. Every callback returns them. The native command derives the actual
invoking webview/surface where Tauri permits, validates the expected response
kind and generation, atomically removes that exact entry, and completes it once.
Unsolicited, duplicate, wrong-surface, wrong-kind, stale-navigation, and late
responses are ignored and counted.

Evaluation failure, timeout, caller cancellation, navigation, webview close,
and shutdown remove/fail only matching entries. Where a browser operation is
intrinsically serial per webview, an explicit per-surface permit implements
that rule before registration; a singleton response slot is never used as an
accidental semaphore.

H05 owns correlation and lifecycle. ADR-018/H08 owns which origins may invoke
the bridge and what browser capabilities are exposed; request IDs are not an
authorization boundary.

### 6. Shared mutable state must declare its key and lifecycle

A process-level mutable service is acceptable only when its API makes all of
the following explicit:

- stable typed key and owner;
- admission/count/byte/time bounds;
- creation and exact removal rules;
- cancellation/shutdown behavior;
- authorization source; and
- metrics that expose high-water marks, leaks, and rejected work.

Singleton `Option<T>`, “active/current” accessors, and restore-previous guards
are forbidden for request, turn, principal, surface, provider, sink, or worker
state. A truly singleton external resource must be represented by an explicit
service/permit and return busy/queued semantics.

## Consequences

### Positive

- Concurrent turns cannot redirect tools, output, delivery, or workers through
  another turn's context.
- Cancellation and cleanup affect the exact execution generation.
- Long-lived tool registries remain reusable without retaining caller state.
- Browser responses are matched even when operations complete out of order.
- Per-session admission policies no longer suppress concurrency across
  unrelated sessions and surfaces.

### Costs and migration

- Tool invocation traits and many tool constructors/call sites must change.
- Spawn sites must propagate context and cancellation deliberately.
- Continuations need a versioned, minimal durable context rather than an
  in-memory pointer or a snapshot of ambient authority.
- Registries and brokers require limits, cleanup guards, and diagnostics.
- Tests that relied on shared ambient setup must construct explicit contexts.

### Relationship to earlier decisions

- ADR-005's host/worker role split and one bound workshop per session remain.
  ADR-017 supersedes any implementation consequence that models that rule with
  a process-global current bus, scope, or sink.
- ADR-008's daemon control plane, SDK routing, and swappable runtimes remain.
  ADR-017 requires native and external runtime operations to enter through the
  same request-scoped identity, cancellation, and correlation contract; a
  process-local wait store may be keyed but never ambient.
- ADR-015's turn pipeline is the owner of ordered output and terminal state.
  ADR-017 supplies the exact sink/context and cancellation root; it does not
  introduce a second stream owner.

## Verification

Implementation is governed by ISO-001–ISO-010, BR-001–BR-007, BP-002–BP-007,
and RET-006 in the [crash/concurrency matrix](../../../architecture/hardening/verification/crash-concurrency-matrix.md).
Tests use barriers and distinct canaries for every context dimension; sleeps or
running the suite serially do not count as isolation evidence.

## Code anchors

- `src/engine_adapters.rs` — process-global `ACTIVE_TOOL_SINK`
- `src/agent_runtime/daemon_interactive_turn.rs` — save/install/restore scope
- `src/tools.rs` and `src/tui/runtime_services.rs` — shared runtime scope and
  context-retaining tool construction
- `src/agent_runtime/turn_orchestrator.rs` — overwrites scheduler runtime/bus
- `src/agent_runtime/turn_worker/run.rs` — singular runtime and bus session
- `src/bin/medousa_tui/agent_runtime.rs` — second scope save/restore path
- `apps/medousa-home/src-tauri/src/human_browser.rs` — singleton browser reply
  senders shared by requests and surfaces
