# Medousa coding experience epic

**Status:** Implementation train in review; runtime recovery and workflow acceptance remain open (2026-10-01)

**Scope:** Desktop Code, workshop language services, project commands, Terminal,
Search, Problems, Tests, and the transitions between Code and other Medousa surfaces.

**Related:** [Human Code workbench](human-code-workbench-plan.md),
[Home Code workbench parity](home-code-vscode-parity-plan.md),
[Code flow-state roadmap](code-flowstate-roadmap.md),
[Code surface bridge](code-surface-bridge-plan.md), and
[Coding engine integration](../docs/engine/coding-engine.md).

## Outcome

A person can open a project, understand its current state, edit with trustworthy
language assistance, run the relevant application or check, inspect results,
and continue through interruptions without reconstructing context or decoding
runtime machinery. Code should have the same care in its interactions as the
rest of Medousa.

Medousa already knows the active workshop, project, working copy, file, language
root, editing owner, installed tools, and running processes. This epic makes
that context govern defaults, available actions, feedback, and recovery across
the coding experience.

This is a follow-up to the workbench implementation marked verified on
2026-08-21. Its shipped capabilities and implementation history remain useful.
The additional completion bar here is demonstrated workflow quality. A callable
endpoint, rendered panel, or passing unit test alone cannot close a slice.

## Evidence and unresolved questions

The 2026-10-01 investigation found these concrete behaviors:

| Finding | Evidence | Consequence |
|---------|----------|-------------|
| An unavailable coding engine can route Python, Rust, and TypeScript to Grapheme | [Connection selection](../apps/medousa-home/src/lib/code/codingEngineClient.ts), reproduced with real CodeMirror initialization and a simulated transport | Valid files receive errors from the wrong parser |
| Connection readiness is labelled with the requested language | The same client file, `createWorkspaceClientEntry` | The wrong service can appear ready |
| Grapheme emits one parse diagnostic over the entire document | Locally inspected `grapheme-lsp` 0.7.1 dependency | One issue paints the whole file red |
| Opening a Terminal can first start a human editing session | [Tracked Terminal flow](../apps/medousa-home/src/lib/utils/undertakingWorkspace.ts) | A seemingly simple navigation action encounters custody and checkout checks |
| Branch changes reject stale attached-checkout authority | [Forge checkout verification](../crates/medousa-forge/src/forge.rs) | The reported 409 reflects a branch mismatch, not a merge conflict |
| The task picker renders every discovered task into one select | [Code chrome](../apps/medousa-home/src/lib/components/code/CodeEditorChrome.svelte) | An active frontend file competes with unrelated monorepo commands |
| Search exposes replacement and advanced filters immediately; results are flat | [Workspace Search](../apps/medousa-home/src/lib/components/code/CodeWorkspaceSearch.svelte) | Basic searching starts with unnecessary decisions |
| Tests render as a flat list in a panel with fixed height limits | [Tests and output](../apps/medousa-home/src/lib/components/code/CodeTasksOutput.svelte), [feedback panel](../apps/medousa-home/src/lib/components/code/CodeFeedbackPanel.svelte) | Thousands of tests have little orientation or usable space |
| Diagnostic metadata is lost on the editor-to-Problems path | [Editor adapter](../apps/medousa-home/src/lib/components/code/CodeMirrorHost.svelte), [Problems controller](../apps/medousa-home/src/lib/code/codeProblemsController.svelte.ts) | Source attribution and precise navigation become inconsistent |

The isolated routing reproduction establishes the faulty branch. It does not
establish why the coding engine was unavailable on the workshop in the
screenshots. Capture that instance's service status and bounded startup logs
before assigning a startup fix. Do not assume a missing package is the cause.

## Product requirements

1. **No automatic LSP fallbacks.** An unavailable or incompatible service stays
   unavailable. Do not substitute another provider, root, session, transport, or
   protocol path. Grapheme is selected explicitly for Grapheme documents.
   Retry the same supported service when appropriate; report missing capability
   or an upgrade requirement clearly. Syntax highlighting and editing remain
   available independently, with language assistance accurately marked unavailable.
2. **One consistent workspace understanding.** Chrome, commands, panels, and
   errors derive from the same scoped runtime facts. A stopped or unknown
   language service cannot become ready because an error field happens to be empty.
3. **Context earns defaults.** Current package, explicit scope, prior user choice,
   and active runs determine what is prominent. A discovered item is not
   automatically a primary action. Show the chosen scope and let the person change it.
4. **Authority follows the workshop daemon.** Files, Git, language servers,
   commands, tests, previews, and terminals belong to that workshop. Local file
   pickers, Reveal, and `convertFileSrc` require `workshop.kind === "local"`.
5. **Execution stays predictable.** File navigation may change suggestions; it
   never changes a running process, an explicit configuration, or the identity
   used by Rerun and Stop. Viewing a panel does not silently transfer editing control.
6. **Recovery preserves work.** Drafts, selections, navigation, and valid running
   sessions survive a failure. Branch changes and ownership transfers require
   explicit actions; do not bypass Forge checks or silently reattach a project.
7. **Explain the state where it matters.** Show a concise cause and the next valid
   action near the operation. Raw HTTP, JSON, IDs, commands, and logs are available
   through Details rather than becoming the default error presentation.
8. **The ordinary path is quiet.** Explain actions before failure, reserve visual
   emphasis for decisions, and reveal advanced controls when needed. Optional
   tools are installed through Settings → Packages; the app remains the entry point.

## Shared context and interaction contract

Build on existing project, execution-authority, run, command, and layout stores.
Add a small derived context contract where necessary; avoid a second runtime or
duplicating mutable controller state in a new global store.

| Context | Authority and use |
|---------|-------------------|
| Workshop and execution runtime identity | Required on requests, pooled clients, subscriptions, cached data, and recovery actions |
| Project and governed environment identity | Daemon-authoritative working copy and branch, with revision or generation for validation |
| Active document and resolved package/language root | Suggest command/test scope within the authoritative project boundary |
| Editing ownership and allowed actions | Determine whether an operation can proceed, wait, or offer an explicit transfer |
| Tool and language-service capability | Distinguish unknown, starting, usable, missing, incompatible, and failed states |
| Dirty buffers and external changes | Drive existing save/reconcile preflight without dropping drafts |
| Explicit command scope and selection | Preserve user intent separately from contextual suggestions |
| Active runs and test/diagnostic observations | Keep stable identities, provenance, freshness, and incomplete coverage visible |

Async work must retain the identity and generation it started with. Discard
responses for a previous file, package, environment, or workshop. Revalidate
authority at action time on the daemon. Never select a context by whichever
request returned last or by a locally guessed path.

Use the existing command identities across buttons, menus, shortcuts, and
Spotlight. Represent pending, unavailable, blocked, failed, and completed states
consistently. A disabled control explains its cause and offers a valid recovery
when one exists. Unknown capability is not an empty result.

The proposed layouts below are implementation targets. Compare them with the
existing Home components and review the important normal and failure states
before fixing the final component design.

## Delivery slices

CE0–CE7 have implementation work under this epic. The train below is ready for
review and manual use; it does not close the acceptance gate. Dedicated checkout
reattachment and local/remote journey proof
remain open. Existing foundations are reused without substituting UI state for
runtime authority.

**CE0 progress (2026-10-01):** Removed automatic provider, root-discovery,
matrix-discovery, and aggregate-diagnostics compatibility fallbacks in Home.
Initialization retains provider identity and rejects an announced Grapheme
service for another language. Disconnected editor markers are cleared while
drafts remain editable. Added routing, initialization, discovery, pooled-session,
and editor lifecycle regression coverage. Final local/remote workshop journeys
and the unavailable-engine incident investigation remain required; CE0 is not
closed by automated checks alone. Workshop-switch isolation continues in CE1.

**CE1 progress (2026-10-01):** Added a derived workshop/runtime/project/environment
identity and a workshop visit generation. Applied it to language-service pooling
and discovery, task catalogs and runs, test discovery, Problems, Quick Open,
language insights, Changes, save/editing-control completion, search replacements,
and terminal creation. Late responses are discarded and pending multi-step
actions check their original scope before continuing. Changing documents within
one checkout preserves explicit task selection and active run identity. Existing
draft buffers remain owned by the document store. Language refactors now require
the Forge transaction contract without a source-batch compatibility downgrade.
Runtime cause translation and contextual package suggestions land in the
subsequent slices below. Dedicated checkout recovery and local/remote workflow
proof remain open; scope isolation alone does not close CE1.

**CE2–CE7 implementation progress (2026-10-01):**

- Terminal reveal does not start an editing session. Shell creation is explicit;
  failures stay in the dock with a readable cause and raw Details. Branch drift
  names expected/current branches and preserves drafts. Existing project flows
  can release and attach a checkout; a dedicated in-place recovery command with
  draft/process reconciliation is still required for CE2 closure.
- Run uses a bounded, searchable picker grouped by command kind and the deepest
  discovered package. Explicit selections and original runs stay stable. Equal
  recommendations ask for a choice; removed pinned commands require reselection.
  Output leads with state, original package, invocation time, preview, and exact
  rerun actions.
- Search debounces queries, groups and highlights results, reveals replacement
  and filters on demand, and captures an explicitly selected package scope.
  Query/scope changes discard old results and invalidate replacement previews.
  Apply uses the frozen review inputs after editing-control waits. The runtime
  accepts one-character searches with a 50-line page cap and intersects changed
  scope with pathspec filters.
- Problems retain precise ranges, provider, code, related information, and version
  through the actual CodeMirror notification adapter. Mismatched-version or
  unsynchronized-buffer publications are ignored; editing clears old markers.
  Aggregate coverage is labelled as observed sessions, with unversioned and
  recorded-version observations distinguished. Language information uses actual
  editor connection state and places session machinery/logs behind Details.
  Package repair opens Settings → Packages and never silently installs a tool.
- Tests have file groups, name/provider filtering, file/package/project scopes,
  prior-failed-invocation filtering, bounded initial rendering, explicit file/test
  targeting, and historical output/time. A successful aggregate command is never
  expanded into invented individual passes. The 2,000-test discovery limit is
  identified. Supported targets can run in a scoped sequential queue; file targets deduplicate
  by task and path. Progress counts invocations, and failed-invocation filtering
  can queue exact supported targets again. Cancelling the pending queue retains
  the current daemon-owned process; context changes prevent any next submission.
- Frequent toolbar actions gain names at desktop widths. Search, feedback, and
  language/structure panels resize with pointer or keyboard, remember dimensions
  by workshop/project, expand, and return focus on closing. Search and replacement
  receive keyboard navigation and preview focus containment.

Regression fixtures exercise 3,000 commands and 2,000 tests with bounded rendered
rows, actual keyboard command selection, resize restoration, delayed searches,
replacement input invalidation, and the real CodeMirror diagnostic adapter.
The guide describes shipped behavior. Real workshop visual/latency review,
missing-service incident evidence, and CE8 journeys are still required; this
status is intentionally not a claim of epic completion.

**Review train and automated evidence:** `a180e5d0` separates Terminal reveal
from shell creation; `4eb1bd18` curates commands; `0f7f9a9e` integrates the
remaining search/diagnostic/test/panel work; `be7494d4` ensures current editor
observations take precedence over older aggregate diagnostic snapshots.
These follow the CE0/CE1 commits
`9b6147c6` and `054f20c0`.

- Final frontend suite: 359 files, 1,803 tests passed; Svelte check has zero errors
  and zero warnings. Runtime graph and browser capability checks pass.
- Runtime Clippy with workspace/all-targets and warnings denied passes. Both
  hermetic passes report 1,875 passed and 3 ignored. Workspace library tests
  report 2,636 passed and 4 ignored across 32 suite results.
- The first workspace test link ran out of disk space. Cleaning this package's
  generated Cargo artifacts and retrying with `CARGO_INCREMENTAL=0` completed
  the same workspace test scope; no source, test, or authority checks were bypassed.
- Strict documentation verification and diff whitespace checks pass. Production
  frontend builds pass. Existing test-theme fetch noise and dependency/toolchain
  build notices remain outside this epic's changes.

Manual review must use a rebuilt workshop daemon for the new one-character
search contract, together with the current app frontend. No release/deployment,
remote push, local/remote workshop dogfood, or screenshot-based visual acceptance
is implied by these checks. CE8 remains the explicit user-review gate.

| Slice | Priority | User outcome | Dependency |
|-------|----------|--------------|------------|
| CE0 Language-service correctness | P0 | I can trust diagnostics and service status | Immediate |
| CE1 Shared context and state | P0 | Every coding surface agrees about where I am and what is possible | CE0 can ship first |
| CE2 Terminal and checkout recovery | P0 | I can open or recover a Terminal without deciphering custody errors | CE1 |
| CE3 Contextual Run and output | P1 | I can run the relevant thing and understand its result | CE1 |
| CE4 Search and replacement | P1 | I can find code quickly and make deliberate replacements | CE1 |
| CE5 Problems and language information | P1 | I know what needs attention and whether analysis is trustworthy | CE0, CE1 |
| CE6 Test navigation and execution | P1 | I can find, run, and understand relevant tests | CE1, CE3 |
| CE7 Chrome and panel composition | P1 | Tools feel discoverable, consistent, and comfortable | CE1; integrate with CE2–CE6 |
| CE8 Complete workflow proof | Release gate | The experience holds together under real use | All slices |

### CE0 Language-service correctness

Audit the full language-service path, including `codingEngineClient`,
`codeLspSession`, language resolution, the workspace bridge, daemon proxy, and
orchestrator. Remove automatic compatibility and error fall-through paths from
Code language assistance. This includes project-root substitution after failed
root discovery and treating failed language-matrix reads as permission to assume
service availability. Require the supported contract and report an upgrade or
service failure when it is missing. Inventory direct Grapheme callers so an
explicit Grapheme feature remains distinct from a fallback.

Preserve the coding-engine failure reason, actual connection route, requested
language, selected provider identity when available, and authoritative language
root. Readiness requires successful initialization of the intended service;
validate negotiated identity where supplied without requiring every external
server to implement optional `serverInfo`. Fix routing at the boundary rather
than suppressing suspicious diagnostic messages.

**Acceptance:**

- Python, Rust, and TypeScript never contact a Grapheme endpoint, including when
  discovery returns unavailable, throws, times out, or reports incompatibility.
- Missing matrix/root support produces an explicit unsupported state; no alternate
  root or provider is tried. Explicit Grapheme selection uses its supported route.
- Failure preserves highlighting and the draft, clears or marks obsolete
  diagnostics, and offers only applicable Restart, Details, upgrade, or package actions.
- Startup and reconnect statuses name the same language service throughout.
  Retries are bounded and target the same service; permanent failures do not loop.
- Regression coverage exercises real connection selection and initialization,
  not just helper string formatting. Include nested roots and workshop changes.

### CE1 Shared context and state

Implement the shared contract using daemon projections and derived UI state.
Separate document context, explicit execution choice, and active execution.
Audit request scopes, client pooling, panel state, and subscriptions for workshop
identity as well as project/language identity. Define one translation from
runtime errors to user-facing causes and permitted actions.

**Acceptance:**

- Switching between frontend, Python, and Rust packages updates suggestions and
  service state together; a pinned execution choice and active run remain stable.
- Switching workshop or reattaching a checkout invalidates incompatible state;
  delayed responses cannot populate the new workspace.
- Run, Terminal, Problems, and status never contradict their underlying scoped
  state. Connected transport alone never implies a ready coding environment.
- Stale authority is caught before avoidable interaction failures where observable,
  and revalidated on execution. No UI cache grants authority.

### CE2 Terminal and checkout recovery

Distinguish revealing an existing Terminal, creating a project shell, and taking
editing control. Opening the panel can reveal a truthful pending or blocked
state without initiating an unexpected ownership transfer. When shell creation
requires authority, use the established allowed-action flow and explain the
decision in place. Keep each process tied to its original environment.

For branch drift, show expected and current branches with a readable explanation:
“This project was attached to low-user-friction. The working copy is now on
companions.” Provide a deliberate route to review and reattach to the current
checkout when allowed, or return to the expected environment. Reconcile drafts
and in-flight execution before either action; do not automatically switch Git
branches. If the runtime lacks a suitable recovery command, implement it through
Forge with its validation and evidence requirements.

**Acceptance:**

- Branch drift is described as a changed working copy, never a merge conflict;
  raw 409/JSON is confined to Details.
- The panel remains open on failure with its explanation and recovery controls.
  It does not disappear and leave a disconnected banner above the editor.
- An existing valid Terminal can be revealed without creating another process
  or claiming a new editing session. Invalid or historical sessions are identified.
- Agent ownership offers only actions the runtime permits. Any transfer is
  explicit and preserves the agent/human handoff contract.
- Recovery preserves drafts and revalidates environment identity. A session from
  the old environment is never silently relabelled as belonging to the new one.

### CE3 Contextual Run and output

Replace the exhaustive native select with a bounded, searchable command picker.
Lead with the current package and group commands by Run, Build, Test, and Check.
Keep all project commands accessible, with package paths as secondary context.
Preserve explicit choices by scope; show when a choice targets another package.
Where several targets are equally plausible, ask through the picker rather than
guessing. Package classification comes from runtime discovery.

Output begins with the command's meaningful state and available next action:
starting, running, preview ready, completed, or needs attention. Keep ordered raw
output accessible and distinguish task output from an interactive shell. Preserve
existing save/reconcile preflight, durable runs, preview routing, and exact reruns.

**Acceptance:**

- Opening a frontend source file makes its development server and checks easy
  to find without scrolling through unrelated crates.
- Thousands of detected commands remain searchable and keyboard navigable;
  command details are inspectable before execution.
- Moving to another file never retargets Stop, Rerun, the preview, or active output.
- A failed run presents its outcome and relevant file locations. A ready server
  presents the correct workshop preview action.
- Dirty/external-change preflight preserves work; reconnect restores the same run
  without duplicate execution. No applicable command is explained as an empty state.

### CE4 Search and replacement

Use a focused query field and visible scope as the default. Reveal replacement,
include/exclude patterns, and advanced options on demand. Keep project search as
an explicit scope alongside current-package and changed-file scopes. Preserve the
last chosen scope within the project; do not silently narrow it when files change.

Group results by file with highlighted matches, useful surrounding text, counts,
and keyboard navigation. Search should respond as the person types with bounded
debouncing/cancellation. Reconcile the existing two-character restriction with
bounded one-character searches so legitimate queries do not meet an unexplained
limit. Keep replacement preview, path selection, digest preconditions, and
editing-authority checks.

**Acceptance:**

- Basic Search opens focused with useful space for results; replacement and
  advanced fields do not consume the initial layout.
- Scope, loading, no matches, cancelled, incomplete, and failed results are distinct.
- A changed query/scope cancels or discards old results; replacement previews are
  invalidated when their query, scope, replacement text, or file preconditions change.
- Enter/arrow navigation opens the correct file and location. Escape returns
  focus predictably without losing drafts.
- Replacement requires a reviewable preview and applies only the reviewed plan;
  changed files produce reconciliation rather than stale writes.

### CE5 Problems and language information

Preserve diagnostic provider/source, code, full range, document version where
available, scope, and freshness across editor markers and Problems. Use current
observations consistently while retaining task provenance and related information.
Show analysis coverage: current file, observed project sessions, pending languages,
or incomplete results. Do not label an empty active-session snapshot as proof that
the entire project has no problems. Retain accurate full-document ranges when a
provider intentionally emits them; address wrong-provider routing in CE0.

Lead language information with whether assistance works for this file, the
affected scope, and relevant recovery. Put executable names, protocol/session
metadata, package identifiers, and bounded logs behind Details.

**Acceptance:**

- Clicking a diagnostic lands on its precise range, with the same producer and
  message shown in the editor and Problems.
- Pending, unavailable, incomplete, clean, and stale analysis are distinguishable.
  A service failure never becomes “No problems found in this project.”
- Delayed diagnostics from old document versions or sessions cannot look current;
  unversioned diagnostics follow an explicit tested freshness policy.
- Restart/repair acts on the affected workshop and service. Optional package
  installation respects Settings → Packages and never silently installs software.
- Compiler/task results retain their run identity and remain distinct from
  current language analysis.

### CE6 Test navigation and execution

Provide file/package/suite grouping, filterable test names, current-file and
changed-code entry points where discovery supports them, and a failed-tests view.
Distinguish discovered-but-not-run, queued, running, passed, failed, cancelled,
unsupported targeting, and stale historical results. Never invent individual test
results from a successful aggregate command when the provider supplies no mapping.

**Acceptance:**

- A catalog of at least 2,000 tests can be filtered and navigated without a flat
  scan; render and discovery remain bounded as the catalog grows.
- Run a test, file, or package when the provider supports it. Unsupported targeting
  is explained and the supported package command remains explicitly accessible.
- Progress, result counts where known, failure locations, output, and rerun failed
  tests form one continuous workflow tied to exact invocations.
- Previous results retain their time/revision context after source changes;
  a historical pass is never presented as current verification.

### CE7 Chrome and panel composition

Preserve breadcrumbs, tabs, and navigation. Give frequent actions clear names and
group supporting actions in a consistent menu. Reuse Home controls and tooltip,
focus, typography, spacing, and error conventions. Critical explanations must be
available to keyboard users, not only through hover titles.

Give Search, Problems, Tests, Output, and Terminal appropriate space. Provide
resizable/restorable panels, expand/collapse affordances, remembered dimensions,
and predictable focus return. Remove nested height limits that leave substantial
tools in a shallow strip. Adapt the desktop composition to narrow windows without
turning this epic into a new mobile workspace implementation.

**Acceptance:**

- A new user can identify Run, Search, Terminal, and Problems without decoding icons.
- Panels resize without losing results or sessions; layout and scope restore per
  project and remain valid for the current workshop.
- Every feature has designed normal, loading, empty, blocked, error, and recovery
  states. Contrast, keyboard operation, accessible names, and focus behavior pass review.
- Opening a tool keeps the active file, selection, and draft; closing it returns
  to the initiating control or editor consistently.

### CE8 Complete workflow proof

Close the epic through complete journeys on both local and remote workshops.
Use a nested monorepo with Python, TypeScript, and Rust, multiple run targets,
at least 2,000 tests, real missing/incompatible services, and controlled branch drift.
Review at ordinary desktop and narrow-window sizes against existing Home surfaces.

| Journey | Required proof |
|---------|----------------|
| Open and understand | Active workshop, branch, package, assistance state, and primary action are understandable without opening logs |
| Edit and run | Edit a frontend file, save/reconcile, run its server, open the remote-safe preview, stop and rerun the exact target |
| Missing or incompatible LSP | Valid source keeps highlighting; no wrong parser or root is contacted; the cause and supported recovery are visible |
| Language changes | Move among Python, TypeScript, and Rust roots; diagnostics and suggestions follow the correct scope without stale results |
| Branch drift | Change the attached branch externally, open Terminal, review the mismatch, recover deliberately, and retain drafts |
| Agent handoff | Encounter agent ownership, choose an allowed action, and continue with the same file and preserved work |
| Find and replace | Search grouped results, navigate a match, preview replacement, handle an external edit, and apply only the validated plan |
| Diagnose and test | Navigate a precise problem, filter relevant tests, run supported targets, inspect failure output, and rerun |
| Large catalogs | Command and test catalogs stay usable, searchable, and bounded; incomplete discovery is identified |
| Interrupt and reconnect | Leave Code, reconnect, restore layout and current runtime state, and control the original process without duplicate execution |
| Workshop switch | Identical project/file names on another workshop cannot reuse clients, results, sessions, or recovery authority |

For each journey record the build revision, workshop/environment, actions,
screenshots or recording, result, and any unresolved defect. Include someone who
did not implement the slice in the final workflow review. Do not mark a slice
complete while its demonstrated interaction still requires interpreting raw
runtime state or navigating an uncurated catalog.

## Implementation order and verification

Ship CE0 immediately as a correctness fix with regression coverage. Define CE1
and review the shared normal/failure-state designs next. Deliver CE2 and CE3 as
complete workflows, then CE4–CE6 with CE7 integrated throughout. CE8 is a release
gate, not a substitute for reviewing each slice as it lands.

Use focused automated tests for routing, authority, stale responses, provider
and document identity, selection persistence, replacement preconditions, and
exact run/test behavior. Add component/integration coverage where it verifies
real keyboard, focus, layout, or runtime interactions. Capture query-to-result,
panel-open, and restoration latency on the large fixtures; compare before/after
on the same device/workshop and resolve perceptible regressions before closure.
Avoid continuous polling when existing events can keep state current.

Before implementation PRs, run repository CI parity:

```bash
cargo clippy --workspace --all-targets --exclude medousa-sdk-iroh -- -D warnings
./scripts/ci/test-hermetic.sh
cargo test --workspace --exclude medousa-sdk-iroh --lib
(cd apps/medousa-home && npm ci && npm run check && npm test)
bash scripts/verify-docs.sh --strict
```

Frontend checks require zero errors and zero warnings. Successful automated
checks supplement the demonstrated journeys; they do not establish visual or
interaction quality on their own.

## Documentation and completion

As each behavior ships, update `docs/guides/` and `docs/README.md` for the coding,
Terminal, package, search, and testing flows it changes. Update
`docs/engine/coding-engine.md` when removing language-service compatibility paths.
Any new recovery/context HTTP contract also updates `docs/engine/`, `docs/sdk/`,
generated contracts, and strict verification as applicable. Keep this epic's
proposed behavior separate from shipped user-facing truth.

The epic is complete when all slices meet acceptance, local and remote journeys
pass on the final build, users can understand the primary workflows without
developer guidance, relevant failure paths provide a valid next action, and the
guides describe that verified experience. Correct routing, stable authority,
accessible interactions, and usable large catalogs are completion requirements.
