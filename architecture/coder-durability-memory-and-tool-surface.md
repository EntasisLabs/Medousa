# Coder durability, worktree memory, and dynamic tools

> **Status:** Approved direction; Slices 1–5 and experiment notebook extension implemented
> **Parent:** [Coder cognitive runtime](coder-cognitive-runtime-plan.md)
> **Related:** [Context lanes and scratchpad](context-lanes-and-scratchpad-plan.md),
> [Turn runtime and lanes](turn-runtime-and-lanes.md), and
> [Forge plan](v0.7.0-forge-plan.md)

## Product decision

Coder is a durable engineering process, not one large inference request. Long
turns must survive context compaction, tool-round exhaustion, application
restart, agent handoff, and Forge attempt recovery without asking the model to
rediscover the undertaking from raw chat.

Four authorities cooperate without duplicating one another:

| Authority | Owns | Must not own |
|---|---|---|
| Forge + Git | Repository, governed environment, HEAD, baseline, branch, dirty files, diffs | Model conclusions or turn protocol state |
| Engineering activity + evidence | Tool lifecycle, operational intent, bounded receipts, failures, causal pointers | A synthesized engineering plan |
| Locus + STTP | Goals, discoveries, decisions, hypotheses, changes, verification, open gaps, handoffs | File truth, side-effect replay, or hidden reasoning |
| Active-turn checkpoint | Exact execution cursor, transcript slice, counters, PackHold state, Forge binding, Locus cursor | Long-lived semantic knowledge |

The model receives a bounded compilation of those authorities. It does not
become their persistence layer.

## Goals

- Preserve useful engineering cognition across turns, agents, surfaces, and
  daemon restarts.
- Resume the exact governed environment rather than silently creating a clean
  sibling worktree.
- Make the model-visible tool schema small and phase-appropriate while keeping
  the authorized capability superset immutable.
- Let Coder retrieve focused semantic context instead of repeatedly rereading
  the same files and command output.
- End or checkpoint deterministically when a budget, provider, or process
  boundary is reached.
- Keep General mode's conservative behavior independent from Coder's long-form
  engineering behavior.

## Non-goals

- Locus is not a Git mirror, transcript archive, or raw tool-output store.
- STTP nodes do not contain private chain-of-thought. They contain explicit,
  user-legible working state and evidence-grounded conclusions.
- Semantic recall cannot grant tool authority or override Forge policy.
- Recovery does not replay side-effecting tool calls automatically.
- Sibling attempt worktrees do not share transient conclusions by default.

## Governed environment memory identity

"Per-worktree memory" means memory for one stable Forge environment lineage,
not memory keyed by an absolute filesystem path. Paths can move or disappear;
attempt ids can rotate while reusing a preserved environment.

The canonical scope is derived by the runtime from existing Forge authority:

```text
profile tenant
  -> repository repo_id
    -> undertaking work_id
      -> governed environment branch + generation
        -> temporal STTP nodes
```

A suitable Locus session key is conceptually:

```text
coder:<repo_id>:<work_id>:<branch_digest>:g<environment_generation>
```

The active profile still supplies the Locus tenant. The model never supplies
or overrides this session id. Absolute paths are node attributes for operator
orientation only and never identity.

### Lifecycle

- An exact restart or a new agent attached to the same governed environment
  uses the same memory scope.
- A forked environment receives a new scope with a `derived_from` relation to
  the source scope and a bounded inherited working set.
- Sibling scopes may query undertaking-level accepted knowledge, but do not
  ingest each other's unverified transient nodes automatically.
- Accepted outcomes may promote selected decisions and verification summaries
  to undertaking or repository memory.
- Discarded or deleted worktrees leave an archived lineage for audit and
  recovery policy; they are no longer included in active ambient recall.

## STTP working-memory contract

Coder uses a small vocabulary of temporal node kinds:

| Kind | Purpose |
|---|---|
| `goal` | Current user objective and acceptance conditions |
| `discovery` | Evidence-grounded fact about code or behavior |
| `hypothesis` | Tentative explanation that still needs verification |
| `experiment` | One bounded speculative approach and the evidence it is meant to produce |
| `acceptance_criterion` | An explicit condition used to judge candidate outcomes |
| `next_action` | The next concrete engineering action at a recovery or handoff boundary |
| `decision` | Chosen approach, rationale, and rejected alternatives |
| `change` | Intent and bounded summary of an applied change set |
| `verification` | Command/check, result, and repository state observed |
| `open_gap` | Unresolved question, failure, blocker, or residual risk |
| `checkpoint` | Compact resumable working state at a safe boundary |
| `handoff` | Explicit context intended for another agent or surface |

Canonical relations include:

```text
supports        contradicts       supersedes
depends_on      applies_to        verified_by
derived_from    blocks            resolves
```

Every repository fact records enough validity information to detect staleness:

- repository id, work id, environment identity, and author identity;
- observed HEAD and, when relevant, file/content digest;
- source tool call or evidence pointer;
- timestamp, confidence, and node kind;
- parent and semantic relations;
- normalized path/symbol tags when applicable.

Recall labels a node stale when its bound Git or content observation no longer
matches. Stale nodes remain historical evidence but cannot be presented as
current repository truth without revalidation.

## Coder memory tools

Coder receives a compact typed facade rather than the generic requirement to
construct a complete four-block STTP payload on every write:

```text
cognition_coder_memory_overview
cognition_coder_memory_recall
cognition_coder_memory_commit
```

`overview` returns only the current goal, accepted decisions, touched paths,
latest verification, open gaps, and the newest checkpoint pointer.

`recall` supports bounded lookup by query, node kind, normalized path, symbol,
relation, author, and time. It traverses relations within the pinned
environment/undertaking authority and returns validity labels.

`commit` accepts structured working state: kind, summary, paths/symbols,
evidence references, validity observations, and relations. The daemon supplies
the session id, provenance, temporal envelope, semantic tags, AVEC defaults,
idempotency key, and canonical STTP serialization. The low-level generic Locus
schema/store tools remain discoverable for diagnostics but are not the normal
Coder path.

### Write boundaries

The runtime and model commit memory at meaningful boundaries, not after every
tool call:

- a material discovery or architectural decision;
- an applied patch or changed worktree state;
- completed verification or a new reproducible failure;
- a user-input boundary;
- an agent handoff;
- approaching a tool/model budget boundary;
- terminal completion, interruption, or recoverable failure.

Writes are append-only and idempotent. A deterministic key derived from the
turn, round, node kind, and evidence digest prevents duplicate nodes after
retry. Corrections create `supersedes` or `contradicts` relations rather than
mutating another agent's node.

Locus failure is non-fatal to the coding turn. The runtime queues the bounded
semantic write for retry and preserves exact recovery through the active-turn
checkpoint.

The retry queue is daemon-owned and keyed by the governed environment scope.
It persists only already-redacted, runtime-compiled commits, is capped at 64
writes and 2 MiB, and drains at most four writes per boundary. Individual
Locus operations have a two-second bound, so memory degradation cannot turn
into an unbounded coding-loop stall.

## Active-turn checkpoint

Locus provides semantic recovery; it cannot recover provider protocol state.
The daemon therefore persists an `ActiveTurnCheckpoint` keyed by session and
daemon turn id. At minimum it contains:

- agent mode and contract revision;
- authoritative user prompt and current goal;
- model/provider route and bounded model-visible transcript;
- model-round, tool-batch, prose-strike, and retry counters;
- PackHold/ambiguous-prose state;
- current scratch/working-state capsule;
- Forge work id, attempt id, environment identity, HEAD, and dirty summary;
- activity-ledger cursor and latest Locus node/cursor;
- outstanding user-input or approval boundary;
- last completed tool call/result boundary and terminal status.

Checkpoints are written only at protocol-safe boundaries. Recovery never
replays a tool call whose completion is uncertain; it verifies the activity
receipt and governed environment first.

### Recovery sequence

1. Load the newest non-terminal checkpoint for the session and daemon turn.
2. Rebind the exact Forge attempt/environment and verify root, branch, HEAD,
   and dirty state.
3. Reconcile the last tool boundary against the activity/evidence ledger.
4. Recall the latest valid environment checkpoint plus semantic nodes newer
   than the stored Locus cursor.
5. Compile a bounded working-state brief: goal, decisions, touched files,
   latest verification, open gaps, and next action.
6. Restore the model-visible tool packs and continue from a safe inference
   boundary.

When the exact active-turn checkpoint is unavailable, Coder may reconstruct
from Forge, activity evidence, and Locus. That path is explicitly marked as a
semantic recovery rather than an exact continuation.

## Dynamic model-visible tool surface

Tool authority and tool visibility are separate sets:

```text
authorized_tools = immutable Forge/runtime policy superset
visible_tools    = bounded subset supplied to the model this round
```

The model can reveal only tools already present in `authorized_tools`.
Memory, prompt text, tool output, repository instructions, and the model itself
cannot expand authority.

### Bootstrap kernel

The initial Coder surface contains only the tools needed to orient, make normal
repository progress, recover, and terminate:

- bounded code read/search and engineering pointers;
- digest-fenced patching when mutation is authorized;
- Forge-bound shell run/status when policy permits;
- Coder memory overview/recall/commit;
- user update, checkpoint, finish, and budget request controls;
- Coder tool discovery and bounded evidence read;
- peer-agent controls when the surface permits collaboration.

### Discoverable packs

The initial pack set is selected from the request class, current STTP working
state, project markers, editor focus, and authoritative engineering pointers.
Additional packs may be revealed explicitly or by a deterministic runtime
signal:

| Pack | Examples |
|---|---|
| `intelligence` | Symbols, definitions, hover, diagnostics |
| `world_model` | Detamu repository/change/impact observations |
| `experiments` | Sealed Forge candidate and engineering-notebook comparison |
| `history` | Bounded engineering history |
| `memory` | Advanced generic Locus diagnostics |
| `research` | Web and browser tools available on the surface |
| `capabilities` | Capability, Grapheme, and MCP discovery/invocation |
| `workspace` | Vault or artifact tools explicitly relevant to the task |

Visibility is monotonic within one active turn: packs may be added but are not
removed while provider tool-call state is live. This avoids schema churn and
preserves prompt-cache stability. The next turn recomputes the smallest useful
surface. Exact visible-pack state belongs in the active-turn checkpoint, not
Locus.

Telemetry records the actual registry supplied to the model: initial and final
tool counts, serialized schema characters, packs revealed, and reveal reason.

## Budget and completion policy

General and Coder deliberately use different loop policies.

### General

- Default maximum: 30 model rounds.
- Explicit typed completion remains preferred.
- Two consecutive non-tool prose responses terminate and both are retained.
- Conservative context slicing and existing General memory policy remain.

### Coder

- Default/hard operator ceiling: 100 model rounds unless a lower task-specific
  bound is supplied.
- Track model rounds, tool batches, prose strikes, retries, and finalizer
  reserve separately.
- Interim prose does not become an accidental terminal response.
- Normal terminal outcomes are typed `finish`, `checkpoint`, or `needs_input`.
- Near the ceiling, write semantic and exact checkpoints and reserve a final
  inference for a truthful user-facing status.
- Budget exhaustion is a typed non-retryable outcome. It must not become an
  unknown runtime failure followed by a fresh loop with reset counters.
- Provider retry reuses the same remaining turn budget and Forge environment.

## Multi-agent rules

- Agents sharing an environment memory scope append independently and carry
  author/session/turn/attempt provenance.
- No shared mutable "session summary" blob exists.
- Conflicting conclusions coexist with explicit relations until evidence
  resolves them.
- Agent handoff writes a bounded `handoff` node and exact checkpoint cursor;
  the receiving agent recalls rather than receiving a transcript dump.
- An agent working in a sibling attempt sees accepted undertaking knowledge and
  explicit cross-attempt pointers, not unqualified sibling facts.

## Security and retention

- Never persist credentials, environment secrets, unredacted command output,
  or unrestricted source bodies into STTP. Durable active-turn checkpoints
  apply the same secret-marker and sensitive-JSON redaction before writing.
- Prefer evidence ids, digests, normalized paths, symbols, and bounded
  summaries.
- Repository instructions and model-authored memory are untrusted inputs;
  Forge and runtime policy remain authoritative.
- Locus node retention follows profile/workshop policy. Archived environment
  lineage may be compacted or evicted without deleting Forge/Git evidence.

## Delivery slices

### Slice 1 — true Coder visibility gate (implemented)

- Replace the partial discovery predicate with a positive visible-tool
  allowlist.
- Keep the authorized superset immutable.
- Add bounded Coder domains and monotonic reveal.
- Add regression tests proving unrelated registered tools are absent before
  discovery and hidden tools cannot be invoked.
- Measure actual initial schema size.

### Slice 2 — Forge-derived Locus scope and typed facade (implemented)

- Derive the environment memory key from profile, repo id, work id, branch,
  and generation.
- Add typed overview/recall/commit tools that construct canonical STTP in the
  daemon.
- Bind every Coder memory operation to the environment scope.
- Add stale-observation and idempotency tests.

### Slice 3 — automatic working-memory checkpoints (implemented)

- Commit at patch, verification, handoff, budget, and terminal boundaries.
- Compile a small environment overview at turn entry.
- Feed semantic state into initial tool-pack selection.
- Queue bounded, redacted writes durably when Locus is unavailable and retry
  without failing the coding tool that produced the checkpoint.

### Slice 4 — exact active-turn recovery (implemented)

- Persist `ActiveTurnCheckpoint` at safe protocol boundaries.
- Join daemon turn, session, Forge attempt, activity cursor, and Locus cursor.
- Restore the exact attempt and budget counters after restart.
- Remove inference retry for typed budget exhaustion.

The runtime stores normalized `genai` messages only after a model-only
continuation or a fully closed tool-call/result batch. Private reasoning and
binary/custom provider payloads are not durable checkpoint content. Transcript,
invocation, path, and tool-surface fields are independently bounded before the
512 KiB atomic checkpoint write.

Coder provider fallback and retry may continue only from a reconciled safe
boundary. If checkpointing is unavailable or Forge/activity evidence diverges,
the turn stops recoverably instead of restarting from the original prompt and
risking a repeated side effect.

Recovery distinguishes an interrupted live turn from a deliberate boundary.
An `active` or `recoverable_failure` checkpoint restores the consumed model
rounds and orchestration counters; `awaiting_user` and `budget_exhausted`
preserve transcript, scratch, Forge environment, and visible packs but start a
new turn budget. The source attempt is reopened through a new fenced Forge
lease over the exact preserved environment.

Exact continuation requires matching worktree root, branch, HEAD, environment
generation, dirty fingerprint, and a reconciled activity cursor. A tool start
or completion newer than the durable boundary, or any environment drift,
downgrades recovery to the semantic Forge/activity/Locus path. The runtime
never automatically replays that uncertain call.

Tool-round exhaustion now returns the typed
`tool_round_budget_exhausted` outcome, persists a `budget_exhausted` checkpoint,
and delivers a truthful continuation status. It is neither an inference error
nor eligible for provider/runtime retry or continuation synthesis.

Mode-aware defaults now give General 30 model rounds and Coder 100. Coder's
100-round limit is a hard ceiling; an explicit lower per-turn override is
honored without inheriting General's conservative default.

### Slice 5 — lineage inheritance and promotion (implemented)

- Link forked memory scopes to their source environment.
- Define bounded inheritance and cross-attempt recall.
- Promote accepted decisions and verification to undertaking/repository scope.
- Archive or compact terminal environment memory according to policy.

Forge now records the source branch, environment generation, and immutable
fork timestamp on each isolated environment. Older persisted work can derive
the same parent boundary from its staging environment and first attempt time,
so lineage does not require a migration rewrite.

Coder recall merges four independently pinned sources: the current environment,
the parent environment as it existed at the fork cutoff, accepted undertaking
knowledge, and accepted repository knowledge. Parent inheritance is restricted
to goals, experiments, acceptance criteria, next actions, decisions,
verification, open gaps, checkpoints, and handoffs. The runtime never queries
a live sibling environment.

Acceptance promotes decisions and verification to the stable undertaking
scope; verification also promotes to the repository scope. Promotion stores a
`derived_from` relation to the source node and Forge decision/evidence pointers.
Only nodes observed at the accepted reviewed HEAD are eligible, and queued
source writes drain before selection. If the source Locus session is temporarily
unavailable, an identifier-only promotion task is persisted and retried on a
later Coder entry.

Accepted and discarded undertakings append `lineage:archived` checkpoints to
their governed environment scopes. Archival is non-destructive: Forge and Git
remain authoritative, while terminal scopes are excluded from active ambient
recall without deleting their audit history.

### Engineering notebook experiment extension (implemented)

- Add explicit experiment, acceptance-criterion, and next-action kinds to the
  same compact memory commit schema; `kind` and `summary` remain the only
  required semantic fields.
- Carry those working-state nodes across an explicit Forge fork cutoff while
  keeping acceptance promotion limited to verified decisions and verification.
- Reveal a sealed-candidate comparison tool through the dynamic `experiments`
  pack, automatically when notebook state or multiple sealed candidates make
  it relevant.
- Compare two to four candidates through evidence manifests and exact sealed
  Git heads. Candidate memory is read from its runtime-owned environment scope
  only through the evidence seal time.
- Bound path and notebook projections and degrade unavailable Locus reads per
  candidate. Never inspect a live sibling worktree or expose raw patches,
  source bodies, or sibling filesystem paths.

## Acceptance and observability

- A large global registry produces a bounded initial Coder tool surface.
- Discovering one pack changes only the next model-visible definitions and does
  not change authority.
- Restarting during an unfinished change reopens the same dirty governed
  environment with its goal, decisions, touched files, verification, and open
  gaps intact.
- A second agent can recall the first agent's explicit working-state nodes
  without receiving its transcript or private reasoning.
- Changed HEAD/content marks incompatible recalled facts stale.
- Two or more sealed speculative attempts can be compared by exact Git tree,
  bounded evidence summary, and their temporally pinned notebook state without
  exposing live sibling state.
- Reaching the Coder ceiling produces a checkpoint and truthful status, not an
  unknown-error inference retry.
- Metrics track schema tokens, repeated reads, recovery latency, stale recalls,
  checkpoint age, queued memory writes, and successful exact resumes.
