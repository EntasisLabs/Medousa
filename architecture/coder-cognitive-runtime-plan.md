# Coder cognitive runtime

> Status: Approved direction; slices 1–4A complete
> Parent: [Agent runtime modes](agent-runtime-modes-plan.md)

## Product decision

Medousa Coder is a persistent software-engineering world, not a coding persona
or a large static tool palette. The daemon owns durable engineering state and
compiles the smallest useful present-tense observation before each inference.
Home, VS Code, Neovim, and future integrations are views and sensor sources for
the same undertaking.

The implementation follows the pattern that makes General Medousa effective:

1. durable state remains outside the model;
2. ranked pointers advertise relevant state cheaply;
3. ambient context supplies present-tense awareness;
4. focused tools resolve pointers only when detail is needed;
5. tool history preserves causal continuity across inference boundaries; and
6. capability discovery reveals already-authorized affordances progressively.

Coder prompts describe runtime physics, authority, evidence, and completion
rules. They do not depend on telling the model to role-play a senior engineer.

## Cognitive loop

Each inference operates over three context layers:

| Layer | Contents | Refresh |
|---|---|---|
| Stable world | undertaking, worktree, branch, baseline, policy, repository instructions | turn entry or authority change |
| Ambient engineering frame | focus, active agents, dirty state, latest verification, running processes, unresolved work, ranked pointers | every inference |
| Delta | actions and observations since this agent's last observation cursor | after tools and relevant external events |

The loop is:

1. perceive the compiled frame;
2. orient against objective, hypotheses, and unresolved evidence;
3. declare a short operational intent;
4. query or act through a governed tool;
5. record intended and actual effects causally;
6. compile changed state, peripheral state, and anomalies;
7. repeat, checkpoint, or finish with verification evidence.

Raw files, logs, diagnostics, traces, and test output remain at their
authoritative source whenever they are cheaply re-queryable. Only bounded,
non-replayable observations become governed evidence objects. Large payload
transport may use chunks internally, but the cognitive interface is a focused
observation plus stable pointers, not an arbitrary text page.

## Stable engineering objects

The runtime will progressively assign durable references to:

- repositories, worktrees, files, revisions, and symbols;
- diagnostics and diagnostic sets;
- commands, processes, builds, tests, and test runs;
- traces, runtime errors, deployments, and external services;
- agent intentions, claims, attempts, change sets, hypotheses, and experiments.

Paths and line numbers are observations. Detamu and code intelligence should
preserve conceptual symbol identity across moves and renames when evidence
allows it.

## Engineering activity ledger

Every meaningful editor observation, tool action, Forge transition, process
event, and repository change enters a daemon-owned ledger scoped by `work_id`.
Events include:

- stable event and causal-parent ids;
- agent/session/turn/attempt identity;
- timestamp and lifecycle status;
- short operational intent when a model initiated the action;
- inferred target objects and resources;
- bounded actual-effect/evidence receipts; and
- resolution/observation state.

Model-issued Coder tools require an `intent` field: one short,
outcome-oriented sentence explaining what the call is trying to accomplish.
It is operational metadata, not private chain-of-thought, authority, or proof
that the resulting effect matched the intention.

The runtime records `planned` before validation/execution and records
`completed` or `failed` against the same call id afterward. Other agents can
therefore understand why nearby state changed without a full conversation.

## Multi-agent undertaking

An undertaking is a shared conceptual room. Its ambient frame reports a
bounded view of active agents, their focus, current or recent intent, affected
scope, age, and overlap. Presence uses heartbeat plus expiry so crashed agents
cannot remain active forever.

Passive awareness is not a concurrency primitive. Safety is layered:

1. isolated Forge attempt/worktree per agent by default;
2. runtime-inferred read/write/verify claims on objects and shared resources;
3. digest, object-revision, and Git-baseline optimistic concurrency checks;
4. serialization for Git index, lockfiles, migrations, generated files,
   databases, ports, deployments, and other hazardous shared resources; and
5. explicit governed integration of completed change sets.

Forge currently permits one active attempt per work item. That remains the
safety boundary until the isolated-attempt slice replaces the singular active
attempt model. We will not enable multiple agents by sharing one mutable
worktree without claims and fencing.

## Attention and pointers

Engineering pointer salience combines:

- recency and time since last meaningful activity;
- same undertaking and current editor focus;
- causal adjacency to the last action;
- unresolved, failed, surprising, or still-running state;
- changed-since-observation state;
- user-request and Detamu structural relevance; and
- penalties for staleness, resolution, and unchanged repetition.

The ambient frame always carries only vital signs: focus, dirty/change counts,
last action, elapsed time, latest build/test state, diagnostics summary,
running processes, current objective, active-agent count, and the top ranked
pointers. Pointer-follow and history tools retrieve detail on demand.

The authorized capability superset remains immutable for a governed turn. The
model-visible subset may be recomputed between inference rounds after
discovery, but discovery can only reveal authority already granted by the
runtime.

## Delivery slices

### Slice 1 — intent and shared presence

- Persist a bounded engineering activity ledger by undertaking.
- Assign stable agent identity from session + turn + Forge attempt.
- Require and strip bounded `intent` metadata on every model-visible bound
  Coder tool call.
- Record planned/completed/failed causal events and bounded target/effect
  receipts.
- Track active presence, heartbeat, current intent, and clean turn exit.
- Compile a bounded canonical STTP shared-space ambient node at Coder entry.
- Add unit tests for schema enforcement, causal history, presence expiry,
  multiple agents, and STTP validity.

Acceptance:

- A Coder call without intent is rejected before the underlying tool runs.
- The underlying domain tool never receives the Coder-only intent field.
- Successful, failed, and policy-rejected calls retain their operational
  intent and causal status.
- Two registered agents in one work id produce `active_agent_count = 2` and
  bounded awareness of the other agent.
- Dropped or expired presence is not reported as active.
- The ambient node follows canonical `sttp-1.0` structure.

### Slice 2 — per-round ambient deltas (complete)

- Add per-agent observation cursors.
- Recompile ambient/delta frames after every tool round.
- Record tool lifecycle events and operational intent in the shared ledger.
- Refresh Forge HEAD, dirty state, changed paths, and entry editor focus after
  each tool round.
- Surface elapsed time and bounded command/tool evidence without replaying full
  tool payloads.
- Inject the canonical STTP delta into the next model inference while advancing
  only the observing agent's cursor.

Live editor focus/edit events, structured Forge transition sensors, changed
symbols, and semantic build/test state remain incremental sensor work for the
pointer and evidence slices. The round-context path introduced here is their
runtime ingestion boundary.

Acceptance:

- Initial Coder context advances that agent's cursor to the represented
  revision.
- A tool round produces a bounded causal delta in the following model request.
- Re-observing without new activity produces no repeated context.
- Two agents observe the same shared events independently.
- Repository changes made during a tool round are reflected in the next
  inference's trusted Forge observation.
- Provider or Forge observation failures stop the loop instead of allowing
  Coder to continue with falsely fresh context.

### Slice 3 — engineering pointers and history (complete)

- Rank activity-derived file, symbol, diagnostic-set, process, verification,
  and change-set pointers by failure/unresolved state, recency, focus, and
  concurrent-agent relevance.
- Add bounded pointer-list, pointer-follow, and filterable/paginated
  engineering-history tools.
- Feed pointer relevance into every ambient delta and automatically reveal
  code-intelligence tools when symbol or diagnostic pointers become salient.
- Add Coder-scoped discovery for intelligence, world-model, and history
  domains that can reveal only tools in the immutable authority superset.
- Refresh the visible tool subset between model rounds while preserving the
  immutable authority superset.

The current semantic pointer kind is inferred from governed tool identity,
intent, targets, and lifecycle status. Durable diagnostic, process, test-run,
symbol, and change-set evidence objects replace that inference progressively in
slices 4 and 8.

Acceptance:

- Planned/completed/failed lifecycle events for one call resolve through one
  stable `engineering:call:*` pointer.
- Ranked pointers are bounded and present at entry and after tool rounds.
- Pointer follow returns causal lifecycle detail without replaying the chat.
- History supports bounded filters and revision pagination and is hidden until
  its domain is discovered.
- Hidden tools cannot be invoked before discovery, and non-Coder tools remain
  outside the surface.
- A successful discovery call changes the tool definitions on the next model
  inference without expanding turn authority.

### Slice 4 — reference-first perception and bounded evidence

This slice must not turn every large observation into another durable copy.
Committed source and semantic structure remain in Git + Detamu; dirty source
remains in the Forge worktree; live diagnostics remain queryable from
medousa-code. The runtime stores pointers and compact receipts by default.

Limits are orientation boundaries, not dead ends. A tool that cannot return a
complete payload must return a successful structured observation explaining
why it was bounded, what was observed, which dimensions remain unknown, and
the exact next ranged/search call shapes available to the model.

#### Slice 4A — perception governor, zero new persistence (complete)

Implemented verticals:

- Whole-file-first reads with actionable bounded line/byte orientation.
- Replayable Code and Detamu payloads remain at their authoritative source
  instead of being duplicated into artifact storage.
- The shared tool loop applies a deterministic 96 Ki-character perception
  envelope per round: 24 Ki for the refreshed mode context and 72 Ki divided
  equally across tool results, with a 48 Ki per-result ceiling.
- Oversized model-facing results preserve priority recovery metadata and
  head/tail orientation while raw invocation receipts remain unchanged.
- Repeated identical failures become an ephemeral causal cluster with a stable
  signature, occurrence count, preserved error/hint, and a change-course cue.
- Process-level and per-round counters measure bounded re-queryable,
  reference-replayable, and non-replayable observations. Only the last class
  contributes to `would_spool` object/byte totals; telemetry stores no payload.

- Keep whole-file reads as the default when the file fits the response budget.
- For oversized files, return size, available digest/line metadata, bounded
  head/tail or focused orientation, and suggested line/byte range calls rather
  than a generic failure.
- Add bounded line/byte reads that report returned and remaining coverage.
- Classify observations as replayable, re-queryable, or non-replayable.
- Query Git/Forge, Detamu, and medousa-code in place instead of copying their
  source data.
- Compile head/tail, failure clusters, summaries, anomalies, and pointers under
  deterministic per-result and per-round model-context budgets.
- Measure payloads that would require spooling before adding a disk store.

#### Slice 4B — storage accounting and execution-cache governance

- Report physical bytes by Forge worktrees, build caches, Detamu, artifacts,
  and Coder evidence.
- Give regenerable build caches separate configurable repository/global caps
  and a free-disk floor; do not confuse them with cognitive evidence.
- Evict inactive regenerable caches by pressure-aware LRU.

#### Slice 4C — ephemeral content-addressed evidence

- Store only non-replayable oversized logs, diagnostics, and traces that cannot
  be reconstructed cheaply.
- Deduplicate globally by SHA-256, compress, redact before persistence, and
  enforce per-object, per-undertaking, and global physical-byte budgets.
- Expire successful/reproducible output before failed/non-reproducible output;
  active references refresh TTL but never override the global cap.
- Use one shared blob backing layer for transient tool payloads rather than
  creating a second competing artifact cache.

#### Slice 4D — durable promotion

- Promote compact receipts into Forge at seal.
- Retain raw evidence durably only through explicit user pinning or a narrow
  review policy; never promote raw output merely because a tool returned it.

Acceptance for 4A:

- A fitting UTF-8 file can still be read completely in one call.
- An oversized whole-file request returns actionable range orientation and is
  not represented as an opaque tool failure.
- A ranged request returns bounded content plus exact coverage and continuation
  metadata.
- No source, diagnostic, log, or trace payload is newly persisted by 4A.
- Raw invocation receipts remain unchanged while the next inference receives a
  bounded observation.
- Diagnostic telemetry reports would-spool counts and byte volume without
  retaining payload bodies, paths, or content.

### Slice 5 — isolated concurrent attempts

- Replace Forge's singular active-attempt field with multiple fenced attempts.
- Give each agent an isolated attempt worktree and execution lease.
- Share ledger, pointers, notebook, and Detamu world while keeping mutation
  environments isolated.
- Preserve recovery, review, evidence, and integration invariants per attempt.

### Slice 6 — claims and collision handling

- Infer object and resource claims from tool targets.
- Add read/write/verify modes, TTL heartbeat, and structured conflict results.
- Serialize hazardous resources and retain optimistic revision checks.
- Surface overlap and conflict pointers in every affected agent's ambient
  frame.

### Slice 7 — engineering notebook and experiments

- Persist objectives, hypotheses, evidence, unresolved questions, experiments,
  acceptance criteria, and next actions by undertaking.
- Support branchable speculative states and comparison between change sets.
- Checkpoint cognitive state independently from conversation length.

### Slice 8 — semantic actions and causal runtime

- Add symbol-native refactors, affected-test selection, history, and structured
  change-set operations.
- Model traces and state transitions as stable objects.
- Support causal `why`, replay, counterfactual experiment, and regression
  comparison workflows.

## Success measures

- time to regain correct context after a pause or surface change;
- repeated reads/tool calls required to reconstruct recent work;
- collision and stale-write rate with multiple agents;
- time to notice build, test, diagnostic, and runtime anomalies;
- context tokens consumed per resolved engineering action;
- defect, rollback, and residual-risk rate; and
- ability to resume an undertaking from its cognitive state rather than its
  transcript alone.
