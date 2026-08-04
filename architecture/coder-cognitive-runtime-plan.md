# Coder cognitive runtime

> Status: Approved direction; slices 1–2 complete
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

Raw files, logs, diagnostics, traces, and test output remain durable evidence
objects. Large payload transport may use chunks internally, but the cognitive
interface is a focused observation plus stable pointers, not an arbitrary text
page.

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

### Slice 3 — engineering pointers and history

- Rank activity, symbol, diagnostic, process, test-run, and change-set pointers.
- Add Coder pointer-follow plus engineering-history summary/detail tools.
- Feed pointer relevance into tool-domain hints and progressive discovery.
- Refresh the visible tool subset between model rounds while preserving the
  immutable authority superset.

### Slice 4 — evidence objects and perception governor

- Persist oversized files, logs, diagnostics, and traces as governed evidence
  objects outside the worktree.
- Add ranged read/search operations with content hashes and TTL/retention.
- Compile head/tail, failure clusters, summaries, anomalies, and pointers under
  a global per-result and per-round model-context budget.

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
