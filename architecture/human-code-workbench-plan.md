# Human Code workbench plan

> **Status:** Verified (2026-08-21)
>
> **Scope:** Desktop Medousa Code, Forge project custody, workshop tasks,
> Terminal, Problems, and Browser preview
>
> **Primary user:** A person editing, building, running, testing, and reviewing
> a project without requiring an AI executor
>
> **Related:** [Home Code workbench parity](home-code-vscode-parity-plan.md),
> [Code flow-state roadmap](code-flowstate-roadmap.md),
> [Code surface bridge](code-surface-bridge-plan.md), and
> [Coding session terminal](coding-session-terminal.md)

## Decision

Medousa Code needs one obvious, durable human loop:

> open project → edit → save → build/run/test → inspect output or Problems →
> open a preview → stop/rerun → review changes

The underlying capabilities mostly exist, but the product currently exposes
them as adjacent systems: editor commands, Forge tasks, Output, shell sessions,
Problems, Browser previews, and Review. This plan composes those systems into a
single project workbench without replacing Forge custody or turning Medousa
into a VS Code clone.

This plan reopens the user-experience completion claim for HCP-7B in the parity
plan. Streaming task output, readiness detection, and configured tasks have
shipped; a coherent, restorable, terminal-aware execution workflow has not.
Existing HCP commit history remains accurate as implementation history.

## Outcomes

When this plan is complete, a human can:

- open a project and immediately see its primary Run action, even before
  opening a file;
- choose among detected build, run, test, and verification configurations;
- trust that Run uses the intended saved content;
- invoke every important project/editor operation from Spotlight;
- leave and return to Code without losing a running service, selected task,
  output channel, or preview relationship;
- navigate build and test failures through the same Problems surface used by
  language diagnostics;
- attach an interactive task to Terminal when the command needs input;
- understand when Forge or another executor owns the working copy without
  learning lease vocabulary; and
- finish through the existing Changes and Forge Review workflow.

## Product principles

1. **Project actions are project-scoped.** Run, Build, Test, Terminal, Problems,
   and Changes remain available when no source file is open.
2. **Run means the visible code.** The default policy saves all dirty editable
   buffers before execution. A blocked save blocks the run with a useful path
   to the conflict.
3. **One execution model, several presentations.** Output and Terminal are
   views of workshop-owned runs, not unrelated process systems.
4. **Commands are the shared vocabulary.** Toolbar buttons, menus, shortcuts,
   and Spotlight dispatch the same command identities and context gates.
5. **State survives presentation changes.** Changing files, panes, or Home
   surfaces does not orphan a task. Reconnect restores from daemon snapshots
   and cursored streams.
6. **Forge authority stays intact and quiet.** Human runs use the governed
   working copy and append evidence. Home translates custody conflicts into
   “Take control” or “Wait for Codex,” not lease IDs and generations.
7. **Remote parity is mandatory.** Files, commands, processes, language
   services, and previews stay on the workshop daemon for remote projects.
8. **Capability must be truthful.** No detected task, missing executable, or
   unavailable language/tool contribution produces a dead control.

## Current gaps

### Entry and discovery

- The primary project task is inside Editor Options and only appears when a
  source tab is active.
- The visible Play glyph can mean resuming human editing from an agent rather
  than running the project.
- Spotlight exposes Output, Terminal, Tests, and Debug, but not discovered
  Build/Run/Test task commands.
- Task-list fetch failures silently collapse to an empty list.
- The first verification task becomes the default even when the project has a
  runnable configuration.

### Execution correctness

- Run does not save or reject dirty buffers before starting a workshop
  process.
- Rerun does not retain an exact prior invocation, especially for targeted
  tests.
- Home owns one transient `run` field and one `running` boolean, despite the
  daemon supporting independently identified runs.
- Output state, selected task, and current run are absent from persisted Code
  layout.
- stdout and stderr are rendered as separate accumulated blocks, losing their
  event ordering.

### Project intelligence

- Detection is rooted only at the Forge worktree root.
- Cargo projects receive Check and Test but no build/run targets; Go, Python,
  and .NET detection is similarly verification-heavy.
- Nested packages and runnable monorepo applications do not contribute
  configurations.
- `.vscode/tasks.json` support is intentionally narrow and does not yet form a
  general task-contribution contract.

### Surface fragmentation

- Task Output, shell Terminal, language Problems, tests, and status are owned
  by separate controllers and panels.
- Task problem locations do not join the project Problems model.
- Common editor operations are split among a context menu, Editor Options,
  Explorer controls, keyboard handlers, and Spotlight.
- A background server can be ready while the durable Code status surface says
  nothing about it.

## Target user experience

### Project command bar

Every prepared project presents a compact command bar in Code chrome:

```text
[▶ Run] [Development server ▾]   Building… / Ready on :5173   [■]
```

- The main button runs the remembered primary configuration.
- The dropdown groups configurations under Run, Build, Test, Verify, and
  Custom.
- Stop replaces or accompanies Run only while the selected run is active.
- A compact status opens the relevant Output or Problems channel.
- “Open Preview” appears when the active run reports a ready private service.
- Project commands remain visible on the empty-project editor landing state.
- Agent/human custody controls remain visually distinct from execution
  controls and never reuse the same Play glyph without a label.

### Unified bottom panel

Code owns one optional, restorable bottom panel with contextual tabs:

```text
Problems (3) | Output: Build | Terminal: project | Tests
```

- Problems merges LSP and task diagnostics while retaining provenance.
- Output offers ordered, bounded, replayable channels per run.
- Terminal attaches to a project shell or an interactive task session.
- Tests may remain a thin discovered list until a structured test adapter
  exists.
- Only one bottom region consumes height at a time; switching tabs does not
  stop the underlying process.

### Command behavior

The shared command registry contributes at least:

- `workbench.action.tasks.runTask`
- `workbench.action.tasks.build`
- `workbench.action.tasks.test`
- `workbench.action.tasks.runPrimary`
- `workbench.action.tasks.rerunLast`
- `workbench.action.tasks.terminate`
- `workbench.action.files.saveAll`
- `editor.action.formatDocument`
- `editor.action.rename`
- `workbench.action.files.newFile`
- `workbench.action.files.newFolder`
- `workbench.action.files.revert`

Discovered configurations also become ephemeral Spotlight entries with stable
IDs scoped to the active project. Buttons and menus invoke these commands
rather than calling controllers directly.

## Target architecture

### 1. Project task catalog

The daemon resolves a project-scoped catalog of `ProjectTaskDescriptor` values:

```text
id                    stable within repository + resolved root
label                 human-facing configuration name
kind                  run | build | test | verify | custom
root                  repository-relative execution root
argv                  structured executable and arguments
source                detected | vscode-task | package | user
interactive           whether PTY input is supported/required
background            whether readiness precedes process exit
default_rank          provider recommendation, never hidden selection logic
ready_matcher         optional structured readiness rule
problem_matchers      zero or more output-to-diagnostic rules
requirements          executable/package capability health
```

Catalog resolution must:

- discover nested roots without escaping the Forge worktree;
- return stable IDs that include provider and relative root;
- report unsupported or unhealthy configurations with a repair reason;
- distinguish “no tasks found” from catalog-loading failure;
- deduplicate identical commands without erasing a more specific label/root;
- preserve root `.vscode/tasks.json` compatibility; and
- avoid writing configuration into the repository merely because it was
  detected.

User-selected defaults and optional custom configurations live in Forge
workspace state or another daemon-owned user setting. A later contribution
registry may replace providers without changing the Home contract.

### 2. Project run service

A daemon-owned `ProjectRunService` becomes the authority for task processes.
It is distinct from `ForgeExecutionService`, which remains admission and
blocking-work control.

The service owns:

- run identity and exact task invocation;
- state (`queued`, `starting`, `running`, `ready`, `passed`, `failed`,
  `cancelled`, `interrupted`);
- ordered output frames with one monotonic cursor across stdout/stderr;
- bounded replay, retained-byte/count limits, and terminal-run TTL;
- process supervision and cancellation;
- readiness and private-preview metadata;
- parsed task diagnostics;
- Forge evidence summary at terminal state; and
- optional PTY/session-host binding for interactive runs.

Required additive contracts:

```text
GET    /v1/forge/items/{work_id}/tasks
GET    /v1/forge/items/{work_id}/task-runs
POST   /v1/forge/items/{work_id}/tasks/{task_id}/runs
GET    /v1/forge/items/{work_id}/task-runs/{run_id}
GET    /v1/forge/items/{work_id}/task-runs/{run_id}/events?since={cursor}
DELETE /v1/forge/items/{work_id}/task-runs/{run_id}
POST   /v1/forge/items/{work_id}/task-runs/{run_id}/preview
POST   /v1/forge/items/{work_id}/task-runs/{run_id}/attach
```

The existing routes evolve compatibly. Older Home clients continue to use the
single-run snapshot. New Home clients feature-detect run listing and attach.

Run metadata and recent terminal summaries survive Home reconnect and
component remount. A daemon restart may truthfully mark non-surviving child
processes `interrupted`; the UI must never claim they are still running.

### 3. Terminal integration

Interactive tasks use the workshop session host rather than a second local
PTY implementation.

- A task may begin non-interactively and expose “Open in Terminal” only if its
  process can be attached safely.
- A task declared interactive starts with a session-host PTY from the outset.
- One run ID maps to at most one authoritative process and, when applicable,
  one shell session ID.
- Output subscribers and Terminal attaches observe the same ordered process
  stream; they do not execute the command twice.
- Detaching or hiding Terminal never terminates the run.
- Stop sends graceful interrupt first, then offers force termination after a
  bounded wait.

The implementation may stage this behind an adapter: initial non-interactive
runs can remain pipe-backed while the public run contract is made compatible
with later PTY-backed runs.

### 4. Home run store

A project-scoped `CodeRunStore` replaces component-local run ownership. It
tracks:

- task catalog and health;
- selected primary task by project/root;
- active and recent run IDs;
- ordered output channels and cursors;
- active bottom-panel tab;
- ready preview relationship; and
- loading, reconnecting, cancellation, and error states.

The store hydrates from the daemon and subscribes by cursor. Svelte components
render state and dispatch commands; they do not own process lifetime.

### 5. Problems integration

The project Problems model accepts providers:

```text
language:<session-id>
task:<run-id>
test:<run-id>
```

Every diagnostic carries source, severity, file, range, message, task/run
identity, and freshness. Starting a replacement run clears or marks stale only
the diagnostics from the replaced channel. LSP diagnostics remain independent.

### 6. Save and custody gate

The Run command performs one explicit preflight:

1. Resolve the target task and its root.
2. Detect dirty buffers relevant to the project.
3. Save all by default through existing digest- and lease-fenced APIs.
4. Stop on a save conflict and open the reconciliation path.
5. Acquire or validate human custody without exposing lease terms.
6. Check task requirements and provide an exact repair action if missing.
7. Start the run and focus Output only when useful.

If an agent owns the working copy, Run is disabled with a reason and an
explicit “Resume editing” action. The system must not silently interrupt an
agent merely because a human pressed Run.

## Delivery slices

Each slice includes production code, focused tests, compatibility behavior,
and documentation updates. Status legend: `⬜ pending`, `🔄 active`,
`✅ verified`, `⛔ blocked`.

| Slice | Deliverable | Status |
|---|---|---|
| HCW-0 | Truthful contracts, telemetry baseline, and acceptance fixtures | ✅ |
| HCW-1 | Project command bar, shared Run commands, and save-before-run | ✅ |
| HCW-2 | Nested task catalog and useful language/ecosystem providers | ✅ |
| HCW-3 | Project run service, ordered output, listing, and reconnect | ✅ |
| HCW-4 | Unified bottom panel and task-backed Problems | ✅ |
| HCW-5 | Interactive task PTY attach and durable background-service UX | ✅ |
| HCW-6 | Editor command consolidation and human coding preferences | ✅ |
| HCW-7 | Thin structured testing improvements and final dogfood | ✅ |

### Implementation checkpoints

- **2026-08-21 — HCW-1 foundation.** Added a project-scoped Run/Stop control
  with persisted command selection, save-all execution preflight, exact
  targeted-test rerun, surfaced task-catalog failures, and shared
  Run/Build/Test/Check/Rerun/Stop Spotlight commands. The existing daemon task
  contract remains compatible. HCW-1 stays active until discovered tasks
  become contextual command entries and local/remote interaction smoke tests
  cover the complete toolbar journey.
- **2026-08-21 — HCW-2 root-aware catalog foundation.** Added bounded,
  Git-visible nested-root discovery; repository-relative task roots and stable
  scoped IDs; canonical cwd containment at execution; nested diagnostic path
  normalization; lockfile-aware bun/pnpm/Yarn/npm scripts; and useful
  build/run/test/check tasks for Cargo, Go, Python, Make, and .NET. HCW-2 stays
  active for requirement-health metadata, Cargo examples/bins, Python and .NET
  application entry points, contextual ranking, and the full acceptance
  fixture matrix.
- **2026-08-21 — HCW-2 descriptor/provider pass.** Versioned the additive task
  descriptor with source, background, ranking, and executable/package
  requirement health; rejected unhealthy runs before save/lease work; and
  exposed repair guidance in Home. Added explicit Cargo bin/example
  configurations, Python module plus uv/Poetry script entry points, runnable
  .NET project commands, and healthy provider-ranked defaults without
  overriding the saved user selection. HCW-2 remains active for repository
  acceptance fixtures and local/remote catalog smoke coverage.
- **2026-08-21 — HCW-3 reconnect foundation.** Added bounded newest-first run
  listing, exact timestamped snapshots with targeted-test identity, project
  hydration of active/recent runs, ordered SSE resume from the next sequence,
  a recent-run Output selector, and persisted Output/run references with a
  legacy-daemon fallback. The listing envelope also reports retention limits
  and cumulative registry eviction. HCW-3 stays active for multi-run
  presentation and reconnect integration/soak coverage.
- **2026-08-21 — HCW-4 feedback-surface foundation.** Consolidated Problems,
  Output, Tests, and Terminal behind one persisted channel host; promoted
  matcher locations into run-provenanced Problems without replacing LSP
  diagnostics; added clear, copy, rerun, stop, command, and location actions;
  and published task state in the Code status surface. Short successful checks
  remain quiet when Output was closed, while failed matched builds open
  Problems. HCW-4 stays active for broader matcher fixtures and interaction
  smoke coverage.
- **2026-08-21 — HCW-5 shared-process foundation.** Added direct-command PTY
  hosting to `medousa-session` and attached interactive/background Forge runs
  to that single workshop process by durable session ID. Run snapshots retain
  attach and tokenized preview paths; Terminal reuses the hosted task PTY;
  graceful interrupt advances to explicit force stop; and previews can open in
  Web or beside Code through shell panes. HCW-5 stays active for broader
  local/remote attach, shell reattach, and process-tree smoke coverage.
- **2026-08-21 — HCW-6 command/preference foundation.** Added shared Save All,
  Format, Rename, New File, New Folder, Revert, Reveal, and language-repair
  identities; routed editor, Explorer, keyboard, and Spotlight entry points
  through those commands; made the bounded remappable shortcut subset visible
  and executable from Settings; and added format-on-save, autosave, safe run
  preflight, and panel-on-failure preferences. The project-specific primary
  task remains in daemon-backed Code workspace state. HCW-6 stays active for
  broader interaction smoke coverage across every mouse/keyboard entry point.
- **2026-08-21 — HCW-7 thin-testing foundation.** Made discovered tests
  provider- and nested-root-aware, added canonical stable task/path/name IDs
  with legacy `path::name` normalization, retained latest per-test run
  provenance, and exposed Run Nearest Test only for named Cargo, Python, and Go
  targets. An executable dogfood fixture proves this repository contributes
  root Cargo build/test tasks plus nested Home check/dev/test tasks without
  executing project collection code. HCW-7 stays active until the final full
  acceptance matrix and local/remote interaction audit close.
- **2026-08-21 — epic closure.** Added the checked-in polyglot acceptance
  repository under `tests/fixtures/human-code-workbench`, executable catalog
  assertions for Cargo/npm/Go/Python/.NET/Make/configured tasks, active-project
  Spotlight entries for every discovered task, configured-task label/matcher
  preservation during command deduplication, stable Python provider identity,
  and direct/proxied preview interaction tests. The focused Forge, session,
  Home, compatibility-contract, and strict-doc gates below close the residual
  foundation notes without expanding into a full Testing IDE or debugger.

### Verification baseline

| Behavior | Executable evidence |
|---|---|
| Catalog breadth and discovery latency boundary | Polyglot fixture plus Medousa-monorepo catalog tests; bounded 48 roots / 256 tasks |
| Dirty-buffer time-to-first-run gate | Home controller proves a blocked save performs no lease or process start |
| Startup and ordered output latency | Run snapshots expose `started_at`; first mixed-stream frame is sequence `0` and replay is cursor-tested |
| Cancellation | Graceful `stopping` then force `cancelled`, with exactly one terminal result/evidence path |
| Reconnect and retention | Run-list hydration, legacy-route fallback, replay-gap, TTL/count/byte, and active-run non-eviction tests |
| Local/remote preview | Co-located direct URL and retained remote proxy-path tests |
| Interactive reattach | Direct-command PTY plus retained human attach/session cursor tests |
| Daily-driver dogfood | This Cargo workspace and nested `apps/medousa-home` check/dev/test catalog assertion |

### HCW-0 — Truth and measurement

- Keep HCP-7B status and completion language aligned with the shipped
  foundation and the remaining execution work.
- Add fixtures for a Cargo binary workspace, npm monorepo, Go command,
  Python application, .NET application, Make project, and configured tasks.
- Record current task discovery, time-to-first-run, task startup, output
  latency, cancellation, and reconnect behavior.
- Add a Home controller test that proves dirty buffers currently cannot be
  executed after the slice lands.
- Define compatibility detection for old daemon task routes.

**Exit:** The repository has executable acceptance fixtures and truthful status
for every later slice.

### HCW-1 — Obvious and correct Run

- Add the persistent project command bar and distinct custody controls.
- Implement dynamic Run/Build/Test/Run Task/Rerun/Stop commands.
- Save all dirty buffers before execution and preserve conflicts.
- Remember the primary task per project using existing workspace state.
- Preserve the exact last invocation, including targeted-test arguments.
- Surface task catalog errors instead of presenting an unexplained empty UI.
- Allow Run from the project landing state with no open file.

**Compatibility:** Uses existing task APIs. If only the legacy endpoint exists,
the UI exposes one active run and labels reconnect limitations honestly.

**Exit:** A user can discover and run the correct existing task from toolbar or
Spotlight and trust that it used saved content.

### HCW-2 — Project-aware task catalog

- Introduce the versioned task descriptor and stable provider/root IDs.
- Reuse coding-engine root resolution where possible instead of inventing a
  second nested-project scanner.
- Add Cargo build/run/bin/example configurations.
- Add npm/pnpm/yarn/bun workspace scripts without assuming npm execution.
- Add Go build/run, Python application/test, .NET build/run/test, and bounded
  Make target providers.
- Return requirement health and exact missing executable/package repair data.
- Preserve compatible `.vscode/tasks.json` task and problem matcher support.
- Add project-scoped default selection ranking with an explicit user override.

**Exit:** Every acceptance fixture exposes useful commands at the correct cwd,
including nested applications.

### HCW-3 — Durable runs and ordered output

- Introduce `ProjectRunService` and per-run concurrency rather than one global
  Home run slot.
- Add run listing, exact snapshots, monotonic mixed-stream events, bounded
  replay, TTL/count/byte caps, and observable eviction.
- Hydrate active and recent runs into `CodeRunStore` after Home reconnect.
- Persist selected task, active/recent run references, Output channel, and
  bottom-panel selection.
- Preserve exact rerun inputs.
- Reconcile cleanly when an older daemon lacks listing or mixed-stream cursors.

**Exit:** Start a background server, navigate away from Code, return, recover
its ordered output and controls, then stop it exactly once.

### HCW-4 — One feedback surface

- Replace independently stacked Output/Tests/Terminal regions with the unified
  bottom-panel host.
- Present a channel picker for current and recent runs.
- Feed task matcher results into Problems with run provenance and freshness.
- Add clear, copy, rerun, stop, open-location, and reveal-command actions.
- Publish execution state in the Code status surface.
- Do not automatically steal focus for a successful short verification unless
  the user opened Output or a preference requests it.

**Exit:** A failed build opens navigable Problems; ordered logs remain in its
Output channel; fixing and rerunning replaces only that run’s stale problems.

### HCW-5 — Interactive and background processes

- Add the run-to-session-host adapter and attach contract.
- Start interactive tasks in a PTY without duplicating the process.
- Support graceful stop followed by explicit force stop.
- Retain ready URL and preview authorization with the run snapshot.
- Offer Open Preview and Open Beside Code through the existing Browser model.
- Make detach, pane movement, and shell reattach safe across local and remote
  workshops.

**Exit:** Run an interactive application, type into it through Terminal, hide
and reopen Terminal without stopping it, open its private preview, then stop it
from either the command bar or Terminal chrome.

### HCW-6 — Editing coherence

- Add shared command identities for Save All, Format, Rename, New File, New
  Folder, Reload/Revert, Reveal, and language repair.
- Route editor menu, context menu, Explorer, keyboard, and Spotlight actions
  through the shared registry.
- Add bounded preferences for format on save, autosave, run save policy,
  panel-on-failure, and primary task.
- Make keybinding support truthful: either expose the shipped remappable subset
  in Settings or stop describing hidden storage overrides as user-remappable.
- Preserve the existing Forge conflict and refactor previews.

**Exit:** Every common non-AI editor action is discoverable through Spotlight
and behaves identically when invoked from mouse or keyboard.

### HCW-7 — Thin testing and dogfood

- Fix exact targeted-test rerun and retain test result provenance.
- Replace regex-only discovery where a cheap native adapter is already
  available; do not build a full Testing IDE in this plan.
- Add run-nearest-test actions only for languages with stable provider IDs.
- Dogfood the complete loop against this repository’s Rust workspace and
  nested Svelte application.
- Update user-facing Code and Packages guides and the final parity matrix.

**Exit:** A Medousa developer can edit Home, run its frontend check/dev task,
navigate a failure, preview it, run a focused test where supported, and review
the resulting change without opening an external editor.

## Dependencies and sequencing

```text
HCW-0 → HCW-1 → HCW-2
          │        │
          └────┬───┘
               ↓
             HCW-3 → HCW-4 → HCW-5
                        │
                        └────→ HCW-6 → HCW-7
```

- HCW-1 intentionally delivers immediate UX value on the existing daemon.
- HCW-2 can proceed beside HCW-1 after HCW-0 contracts are fixed.
- HCW-3 is the architectural pivot; HCW-4 and HCW-5 must not recreate
  component-owned run state around it.
- HCW-6 may add commands incrementally, but settings persistence should use
  the final run/task identities from HCW-2/3.

## Acceptance journeys

### 1. Rust workspace

Open a Cargo workspace with multiple binaries. Select one binary, edit a dirty
source file, Run, observe the file saving first, navigate a compiler error from
Problems, fix it, rerun the exact binary, and retain the successful result in
Forge evidence.

### 2. Nested web application

Open a repository whose frontend lives below the root. Select its development
server, run at the nested cwd with the detected package manager, see ordered
output and readiness, open the private preview beside Code, change files, and
stop the original process once.

### 3. Reconnect and pane movement

Start a long-running task on a remote workshop, move to Notes and Browser,
close and reopen the Code presentation, and recover the active run, output
cursor, selected configuration, preview, and stop control.

### 4. Custody conflict

While Codex owns the Forge worktree, attempt Run. Medousa explains that Codex
is working and offers Wait, Stop Codex, or Resume editing according to existing
allowed actions. It does not silently steal the lease or execute stale code.

### 5. Save conflict

Edit a dirty file while Terminal changes it on disk, then press Build. Build
does not begin. The draft remains safe and the existing compare/reconcile UI
opens. After resolution, Build uses the chosen content.

### 6. Interactive task

Start an interactive CLI task, attach Terminal, send input, detach, reattach in
another pane, interrupt gracefully, and confirm one terminal result and one
Forge evidence record.

### 7. Older workshop

Connect a new Home to a daemon with only the legacy task endpoints. Run and
stop a supported task successfully. Unsupported run history/attach controls
are absent or disabled with a clear upgrade explanation.

## Verification gates

Focused gates per slice:

- Rust unit tests for task discovery, root resolution, matcher parsing, run
  state transitions, replay gaps, retention, cancellation, and Forge evidence;
- Home unit tests for command context, save preflight, exact rerun, run-store
  hydration, ordered reduction, layout migration, and compatibility fallback;
- component tests for command bar and bottom-panel state;
- local and remote integration tests for task start/stream/stop/preview;
- session-host tests proving one interactive task maps to one process; and
- soak tests proving completed run memory and retained output return to bounds.

Before a checkpoint or final completion, run repository CI parity from the
root:

```bash
cargo clippy --workspace --all-targets --exclude medousa-sdk-iroh -- -D warnings
./scripts/ci/test-hermetic.sh
cargo test --workspace --exclude medousa-sdk-iroh --lib
cd apps/medousa-home && npm ci && npm run check && npm test
bash scripts/verify-docs.sh --strict
```

Manual acceptance must cover one co-located and one remote workshop. A UI-only
test is not sufficient evidence for execution, reconnect, PTY attach, or
preview behavior.

## Metrics

Measure from project-open to successful human feedback:

- time and interactions to first Run;
- percentage of prepared projects with at least one healthy useful task;
- task start latency and first-output latency;
- run failures caused by missing tools versus project code;
- dirty-buffer runs prevented or saved;
- active runs successfully recovered after Home reconnect;
- task diagnostics navigated from Problems;
- background services stopped cleanly; and
- preview-open success for local and remote workshops.

Do not collect command output, source paths, arguments, or repository content
in product telemetry. Aggregate capability and latency counters are enough.

## Explicit non-goals

- Embedding VS Code, its extension host, or a VSIX marketplace.
- Pixel-for-pixel VS Code chrome or permanent IDE panels across Medousa.
- Replacing Forge with unmanaged writes or local Home processes for remote
  projects.
- A complete Debug Adapter Protocol implementation.
- Coverage visualization, watch trees, or a full Testing IDE.
- Arbitrary shell text executed through the bounded task API; free-form work
  remains Terminal.
- Automatically editing repository task configuration during detection.
- Making AI controls a prerequisite for any workflow in this plan.

## Completion definition

This plan is complete only when all seven acceptance journeys pass, current and
legacy daemon behavior is explicit, task and PTY retention remain bounded, the
Code/Forge user guides describe the shipped workflow, and the final dogfood
journey can build and run Medousa’s own nested application without an external
editor.
